//! M4 AI 大脑命令层：CLIP 嵌入流水线、语义搜索、自然语言配额精选、精选集管理。
//! 原则不变：推理全本地，模型文件懒下载，一切结果只是建议。

use crate::clip;
use crate::commands::MEDIA_COLS;
use crate::db::{EFF_DECISION, EFF_STARS, EFF_TIME};
use crate::state::{AppState, ScanStatus};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn blob_to_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

// ============ 状态与模型 ============

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub models_ready: bool,
    pub emb_count: i64,
    pub image_count: i64,
    pub ollama_ready: bool,
    pub ollama_models: Vec<String>,
}

fn ollama_probe() -> (bool, Vec<String>) {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build();
    let Ok(client) = client else { return (false, vec![]) };
    let Ok(resp) = client.get("http://127.0.0.1:11434/api/tags").send() else {
        return (false, vec![]);
    };
    let Ok(body) = resp.json::<serde_json::Value>() else {
        return (true, vec![]);
    };
    let models = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    (true, models)
}

#[tauri::command]
pub fn clip_status(state: State<'_, AppState>) -> Result<AiStatus, String> {
    let conn = lock(&state.db);
    let (image_count, emb_count) = conn
        .query_row(
            "SELECT SUM(kind='image'), SUM(emb_model = ?1) FROM media",
            [clip::EMB_MODEL],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .map_err(|e| e.to_string())?;
    let (ollama_ready, ollama_models) = ollama_probe();
    Ok(AiStatus {
        models_ready: clip::models_ready(),
        emb_count,
        image_count,
        ollama_ready,
        ollama_models,
    })
}

#[tauri::command]
pub fn clip_download(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    guard_analyze(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let result = clip::ensure_models(&|msg| {
            let _ = app.emit(
                "clip://progress",
                ScanStatus {
                    running: true,
                    phase: "下载模型".into(),
                    done: 0,
                    total: 0,
                    current: msg.into(),
                },
            );
        });
        match result {
            Ok(n) => {
                let _ = app.emit("clip://done", format!("模型就绪（新下载 {n} 个文件）"));
            }
            Err(e) => {
                let _ = app.emit("clip://error", e);
            }
        }
        let state = app.state::<AppState>();
        *lock(&state.analyze) = ScanStatus::default();
    });
    Ok(())
}

/// AI 任务（模型下载/嵌入）共用 analyze 互斥锁
fn guard_analyze(state: &State<'_, AppState>) -> Result<(), String> {
    let mut a = lock(&state.analyze);
    if a.running {
        return Err("已有 AI 任务在进行中".into());
    }
    *a = ScanStatus { running: true, phase: "准备中".into(), ..Default::default() };
    Ok(())
}

// ============ 嵌入流水线 ============

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmbedSummary {
    pub embedded: usize,
    pub total: usize,
    pub skipped_model: bool,
}

#[tauri::command]
pub fn embed_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _ = &state;
    if !clip::models_ready() {
        return Err("CLIP 模型还没下载，请先在「智能精选」里下载模型".into());
    }
    guard_analyze(&state)?;
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app3 = app.clone();
        let run = move |conn: &mut Connection| -> Result<EmbedSummary, String> {
            let rows: Vec<(i64, String)> = {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, thumb FROM media
                         WHERE kind = 'image' AND thumb IS NOT NULL
                           AND (emb IS NULL OR emb_model != ?1)
                         ORDER BY id",
                    )
                    .map_err(|e| e.to_string())?;
                let mapped = stmt
                    .query_map([clip::EMB_MODEL], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(|e| e.to_string())?;
                mapped.flatten().collect()
            };
            let total = rows.len();
            if total == 0 {
                return Ok(EmbedSummary { embedded: 0, total: 0, skipped_model: false });
            }
            let clip_shared = clip::shared()?;
            let mut done = 0usize;
            let mut failed = 0usize;
            for chunk in rows.chunks(64) {
                {
                    let tx = conn.transaction().map_err(|e| e.to_string())?;
                    {
                        let mut stmt = tx
                            .prepare("UPDATE media SET emb = ?1, emb_model = ?2 WHERE id = ?3")
                            .map_err(|e| e.to_string())?;
                        for (id, thumb) in chunk {
                            let vec = match clip_shared.lock().unwrap().embed_image(std::path::Path::new(thumb)) {
                                Ok(v) => v,
                                Err(_) => {
                                    failed += 1;
                                    continue;
                                }
                            };
                            stmt.execute(rusqlite::params![vec_to_blob(&vec), clip::EMB_MODEL, id])
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    tx.commit().map_err(|e| e.to_string())?;
                }
                done += chunk.len();
                let _ = app3.emit(
                    "clip://progress",
                    ScanStatus {
                        running: true,
                        phase: "语义索引中".into(),
                        done,
                        total,
                        current: format!("失败 {failed}"),
                    },
                );
            }
            Ok(EmbedSummary { embedded: done - failed, total, skipped_model: false })
        };
        match run(&mut crate::db::init(&cache.join("wanflow.db")).expect("db")) {
            Ok(summary) => {
                let _ = app.emit("clip://done", &summary);
                let state = app.state::<AppState>();
                *lock(&state.scan) = ScanStatus::default();
            }
            Err(e) => {
                let _ = app.emit("clip://error", e);
                let state = app.state::<AppState>();
                *lock(&state.scan) = ScanStatus::default();
            }
        }
    });
    Ok(())
}

// ============ 语义搜索 ============

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScoredMedia {
    #[serde(flatten)]
    pub item: crate::commands::MediaItem,
    pub score: f32,
}

fn load_embeddings(conn: &Connection) -> Result<Vec<(i64, Vec<f32>)>, String> {
    let mut stmt = conn
        .prepare("SELECT id, emb FROM media WHERE emb_model = ?1 AND emb IS NOT NULL")
        .map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map([clip::EMB_MODEL], |r| {
            let id: i64 = r.get(0)?;
            let blob: Vec<u8> = r.get(1)?;
            Ok((id, blob))
        })
        .map_err(|e| e.to_string())?;
    Ok(mapped
        .flatten()
        .map(|(id, blob)| (id, blob_to_vec(&blob)))
        .collect())
}

fn media_by_ids(conn: &Connection, ids: &[i64]) -> Result<Vec<crate::commands::MediaItem>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let id_list = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT {MEDIA_COLS} FROM media m {EFF_DECISION} {EFF_STARS} {EFF_TIME} WHERE m.id IN ({id_list})"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], crate::commands::map_media_item)
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(items)
}

#[tauri::command]
pub fn semantic_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<ScoredMedia>, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Err("请输入搜索内容".into());
    }
    let conn = lock(&state.db);
    let embeddings = load_embeddings(&conn)?;
    if embeddings.is_empty() {
        return Err("还没有语义索引：先在「智能精选」里完成建模".into());
    }
    let clip_shared = clip::shared()?;
    let qvec = clip_shared.lock().unwrap().embed_text(&q)?;
    let mut scored: Vec<(i64, f32)> = embeddings
        .iter()
        .map(|(id, v)| (*id, clip::cosine(&qvec, v)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit.unwrap_or(60) as usize);
    let items = media_by_ids(&conn, &scored.iter().map(|(i, _)| *i).collect::<Vec<_>>())?;
    let by_id: std::collections::HashMap<i64, f32> = scored.into_iter().collect();
    Ok(items
        .into_iter()
        .map(|item| {
            let score = by_id.get(&item.id).copied().unwrap_or(0.0);
            ScoredMedia { item, score }
        })
        .collect())
}

// ============ 自然语言配额精选 ============

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSelectQuery {
    pub prompt: String,
    /// keep = 只在已保留里挑；all = 全部
    pub pool: Option<String>,
    pub count: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSelectResult {
    pub count: usize,
    pub parsed_count: u32,
    pub parsed_text: String,
    pub items: Vec<ScoredMedia>,
}

/// 解析"挑 6 张娃在水里玩得开心的照片" → (6, "娃在水里玩得开心的照片")
fn parse_intent(prompt: &str) -> (u32, String) {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?:挑|选|要|来)?\s*(\d{1,3})\s*张").unwrap());
    let mut count = 9u32;
    let mut text = prompt.trim().to_string();
    if let Some(c) = re.captures(prompt) {
        if let Ok(n) = c[1].parse::<u32>() {
            if (1..=200).contains(&n) {
                count = n;
            }
        }
        text = re.replace(prompt, "").trim().to_string();
    }
    let text = text
        .trim_start_matches(|ch: char| "，。、挑选定来要最些点 ".contains(ch))
        .to_string();
    (count, if text.is_empty() { prompt.trim().to_string() } else { text })
}

#[tauri::command]
pub fn smart_select(
    state: State<'_, AppState>,
    query: SmartSelectQuery,
) -> Result<SmartSelectResult, String> {
    let (parsed_count, text) = parse_intent(&query.prompt);
    let count = query.count.unwrap_or(parsed_count).max(1);
    let conn = lock(&state.db);
    let embeddings = load_embeddings(&conn)?;
    if embeddings.is_empty() {
        return Err("还没有语义索引：先完成「建立语义索引」".into());
    }
    let clip_shared = clip::shared()?;
    let qvec = clip_shared.lock().unwrap().embed_text(&text)?;

    // 候选池：全部图片 / 仅已保留
    let pool_keep = query.pool.as_deref() == Some("keep");
    let mut keep_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    if pool_keep {
        let sql = format!(
            "SELECT m.id FROM media m {EFF_DECISION} WHERE vd.decision = 'keep'"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mapped = stmt.query_map([], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())?;
        keep_ids = mapped.flatten().collect();
    }

    // 语义分 + 质量小加成（同等相关时更清晰优先），拒绝项强排除
    let mut scored: Vec<(i64, f32)> = Vec::new();
    {
        let sql = format!(
            "SELECT m.id, COALESCE(m.quality, 50.0) FROM media m {EFF_DECISION}
             WHERE m.kind = 'image' AND NOT COALESCE(vd.decision, '') = 'reject'"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))
            .map_err(|e| e.to_string())?;
        let emb_map: std::collections::HashMap<i64, &Vec<f32>> =
            embeddings.iter().map(|(i, v)| (*i, v)).collect();
        for (id, quality) in mapped.flatten() {
            if pool_keep && !keep_ids.contains(&id) {
                continue;
            }
            if let Some(v) = emb_map.get(&id) {
                let sem = clip::cosine(&qvec, v);
                let bonus = 0.10 * (quality as f32 / 100.0);
                scored.push((id, sem + bonus));
            }
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(count as usize);

    let items = media_by_ids(&conn, &scored.iter().map(|(i, _)| *i).collect::<Vec<_>>())?;
    let by_id: std::collections::HashMap<i64, f32> = scored.into_iter().collect();
    let mut items: Vec<ScoredMedia> = items
        .into_iter()
        .map(|item| {
            let score = by_id.get(&item.id).copied().unwrap_or(0.0);
            ScoredMedia { item, score }
        })
        .collect();
    items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(SmartSelectResult {
        count: items.len(),
        parsed_count,
        parsed_text: text,
        items,
    })
}

// ============ 精选集 ============

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumItem {
    pub id: i64,
    pub name: String,
    pub count: i64,
    pub created_at: i64,
}

#[tauri::command]
pub fn list_albums(state: State<'_, AppState>) -> Result<Vec<AlbumItem>, String> {
    let conn = lock(&state.db);
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.name, a.created_at,
                    (SELECT COUNT(*) FROM album_members m WHERE m.album_id = a.id)
             FROM albums a ORDER BY a.id DESC",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(AlbumItem {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                count: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(items)
}

#[tauri::command]
pub fn create_album(
    state: State<'_, AppState>,
    name: String,
    ids: Vec<i64>,
    scores: Option<Vec<f64>>,
) -> Result<i64, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("给精选集起个名字".into());
    }
    if ids.is_empty() {
        return Err("精选集是空的".into());
    }
    let mut conn = lock(&state.db);
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO albums(name, created_at) VALUES (?1, ?2)",
        rusqlite::params![name, crate::db::now_secs()],
    )
    .map_err(|e| e.to_string())?;
    let album_id = tx.last_insert_rowid();
    {
        let mut stmt = tx
            .prepare("INSERT OR REPLACE INTO album_members(album_id, media_id, score) VALUES (?1, ?2, ?3)")
            .map_err(|e| e.to_string())?;
        for (i, id) in ids.iter().enumerate() {
            let score = scores.as_ref().and_then(|s| s.get(i).copied()).unwrap_or(0.0);
            stmt.execute(rusqlite::params![album_id, id, score])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(album_id)
}

#[tauri::command]
pub fn delete_album(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = lock(&state.db);
    conn.execute("DELETE FROM album_members WHERE album_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM albums WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn album_media(state: State<'_, AppState>, id: i64) -> Result<Vec<ScoredMedia>, String> {
    let conn = lock(&state.db);
    let sql = format!(
        "SELECT {MEDIA_COLS}, am.score
         FROM album_members am
         JOIN media m ON m.id = am.media_id
         {EFF_DECISION} {EFF_STARS} {EFF_TIME}
         WHERE am.album_id = ?1
         ORDER BY am.score DESC"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([id], |r| {
            Ok(ScoredMedia {
                item: crate::commands::map_media_item(r)?,
                score: r.get::<_, Option<f64>>(20)?.unwrap_or(0.0) as f32,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(items)
}
