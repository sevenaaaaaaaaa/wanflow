//! M5 家人管线命令：全库人脸索引 → 聚类 → 注册命名 → 智能选片加权。
//! 人脸数据全部本地推理，嵌入只存旁车库。

use crate::db;
use crate::faces::{self, FACE_EMB_MODEL, FACE_MATCH_COS};
use crate::state::{AppState, ScanStatus};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn guard(state: &State<'_, AppState>) -> Result<(), String> {
    let mut s = lock(&state.scan);
    if s.running {
        return Err("已有任务在进行中".into());
    }
    *s = ScanStatus { running: true, phase: "准备中".into(), ..Default::default() };
    Ok(())
}

fn finish(app: &AppHandle, error: String) {
    let state = app.state::<AppState>();
    *lock(&state.scan) = ScanStatus { running: false, phase: error.clone(), ..Default::default() };
    if !error.is_empty() {
        let _ = app.emit("scan://error", error);
    }
    let _ = app.emit("scan://done", ());
}

// ============ 全库人脸索引 ============

#[tauri::command]
pub fn family_index_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if !faces::models_ready() {
        return Err("人脸模型还没下载，请在「家人」面板里下载".into());
    }
    guard(&state)?;
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app2 = app.clone();
        let run = move |conn: &mut Connection| -> Result<FamilyIndexSummary, String> {
            let rows: Vec<(i64, String)> = {
                let mut stmt = conn
                    .prepare(
                        "SELECT m.id, m.thumb FROM media m
                         WHERE m.kind='image' AND m.thumb IS NOT NULL
                           AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.media_id = m.id AND f.emb_model = ?1)
                         ORDER BY m.id",
                    )
                    .map_err(|e| e.to_string())?;
                let m = stmt
                    .query_map([FACE_EMB_MODEL], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(|e| e.to_string())?;
                m.flatten().collect()
            };
            let total = rows.len();
            let engine = faces::shared()?;
            let mut done = 0usize;
            let mut face_total = 0usize;
            for chunk in rows.chunks(50) {
                {
                    let tx = conn.transaction().map_err(|e| e.to_string())?;
                    {
                        let mut stmt = tx
                            .prepare(
                                "INSERT INTO faces(media_id, x, y, w, h, score, emb, emb_model)
                                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                            )
                            .map_err(|e| e.to_string())?;
                        let mut upd = tx
                            .prepare("UPDATE media SET face_count = ?1 WHERE id = ?2")
                            .map_err(|e| e.to_string())?;
                        for (id, thumb) in chunk {
                            let Ok(img) = image::ImageReader::open(std::path::Path::new(thumb))
                                .map_err(|e| e.to_string())
                                .and_then(|r| r.decode().map_err(|e| e.to_string()))
                            else {
                                continue;
                            };
                            let detected = match engine.lock().unwrap().detect_and_embed(&img) {
                                Ok(v) => v,
                                Err(_) => continue,
                            };
                            upd.execute(rusqlite::params![detected.len() as i64, id])
                                .map_err(|e| e.to_string())?;
                            for f in &detected {
                                let blob: Vec<u8> =
                                    f.embedding.iter().flat_map(|x| x.to_le_bytes()).collect();
                                stmt.execute(rusqlite::params![
                                    id, f.x, f.y, f.w, f.h, f.score, blob, FACE_EMB_MODEL
                                ])
                                .map_err(|e| e.to_string())?;
                                face_total += 1;
                            }
                        }
                    }
                    tx.commit().map_err(|e| e.to_string())?;
                }
                done += chunk.len();
                let _ = app2.emit(
                    "scan://progress",
                    ScanStatus {
                        running: true,
                        phase: "人脸索引".into(),
                        done,
                        total,
                        current: format!("已检出 {face_total} 张脸"),
                    },
                );
            }

            // 聚类（并查集，SFace 余弦阈值）
            let clusters = cluster_faces(conn)?;
            // 新脸自动认领：与已注册家人的脸匹配
            let auto = auto_assign_persons(conn)?;

            Ok(FamilyIndexSummary { photos: done, faces: face_total, clusters, auto_assigned: auto })
        };
        match db::init(&cache.join("wanflow.db")) {
            Ok(mut c) => match run(&mut c) {
                Ok(summary) => {
                    let _ = app.emit("family://done", &summary);
                    finish(&app, String::new());
                }
                Err(e) => finish(&app, e),
            },
            Err(e) => finish(&app, e.to_string()),
        }
    });
    Ok(())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FamilyIndexSummary {
    pub photos: usize,
    pub faces: usize,
    pub clusters: usize,
    pub auto_assigned: usize,
}

fn blob_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// 对全部人脸做并查集聚类，写回 faces.cluster（簇代表 = 最小 face id）
fn cluster_faces(conn: &mut Connection) -> Result<usize, String> {
    let rows: Vec<(i64, Vec<f32>)> = {
        let mut stmt = conn
            .prepare("SELECT id, emb FROM faces WHERE emb_model = ?1 ORDER BY id")
            .map_err(|e| e.to_string())?;
        let m = stmt
            .query_map([FACE_EMB_MODEL], |r| {
                let id: i64 = r.get(0)?;
                let blob: Vec<u8> = r.get(1)?;
                Ok((id, blob))
            })
            .map_err(|e| e.to_string())?;
        m.flatten()
            .map(|(id, b)| (id, blob_vec(&b)))
            .collect()
    };
    let n = rows.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    // 贪心：按 face id 顺序与各簇代表比较（O(n·簇数)，家用量级足够）
    let mut rep_emb: Vec<usize> = Vec::new(); // 簇根 -> rows 下标
    for i in 0..n {
        let mut matched = false;
        for &r in &rep_emb {
            let c = crate::clip::cosine(&rows[i].1, &rows[r].1);
            if c >= FACE_MATCH_COS {
                let (ri, rj) = (find(&mut parent, r), find(&mut parent, i));
                parent[rj] = ri;
                matched = true;
                break;
            }
        }
        if !matched {
            rep_emb.push(i);
        }
    }
    // cluster id = 簇内最小 face id
    conn.execute("UPDATE faces SET cluster = NULL", []).map_err(|e| e.to_string())?;
    let mut cluster_min: HashMap<usize, i64> = HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        let id = rows[i].0;
        cluster_min
            .entry(root)
            .and_modify(|m| *m = (*m).min(id))
            .or_insert(id);
    }
    {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare("UPDATE faces SET cluster = ?1 WHERE id = ?2")
                .map_err(|e| e.to_string())?;
            for i in 0..n {
                let root = find(&mut parent, i);
                stmt.execute(rusqlite::params![cluster_min[&root], rows[i].0])
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(cluster_min.len())
}

/// 自动认领：未挂 person 的脸与已注册家人（is_family=1）的脸比对，≥阈值即挂到该 person
fn auto_assign_persons(conn: &mut Connection) -> Result<usize, String> {
    let persons: Vec<(i64, Vec<Vec<f32>>)> = {
        let mut stmt = conn
            .prepare(
                "SELECT p.id, f.emb FROM persons p JOIN faces f ON f.person_id = p.id
                 WHERE p.is_family = 1 AND f.emb_model = ?1",
            )
            .map_err(|e| e.to_string())?;
        let m = stmt
            .query_map([FACE_EMB_MODEL], |r| {
                let pid: i64 = r.get(0)?;
                let blob: Vec<u8> = r.get(1)?;
                Ok((pid, blob_vec(&blob)))
            })
            .map_err(|e| e.to_string())?;
        let mut map: HashMap<i64, Vec<Vec<f32>>> = HashMap::new();
        for (pid, v) in m.flatten() {
            map.entry(pid).or_default().push(v);
        }
        map.into_iter().collect()
    };
    if persons.is_empty() {
        return Ok(0);
    }
    let unknown: Vec<(i64, Vec<f32>)> = {
        let mut stmt = conn
            .prepare("SELECT id, emb FROM faces WHERE person_id IS NULL AND emb_model = ?1")
            .map_err(|e| e.to_string())?;
        let m = stmt
            .query_map([FACE_EMB_MODEL], |r| {
                Ok((r.get::<_, i64>(0)?, blob_vec(&r.get::<_, Vec<u8>>(1)?)))
            })
            .map_err(|e| e.to_string())?;
        m.flatten().collect()
    };
    let mut assigned = 0;
    {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare("UPDATE faces SET person_id = ?1 WHERE id = ?2")
                .map_err(|e| e.to_string())?;
            for (fid, emb) in unknown {
                for (pid, refs) in &persons {
                    if refs
                        .iter()
                        .any(|r| crate::clip::cosine(r, &emb) >= FACE_MATCH_COS)
                    {
                        stmt.execute(rusqlite::params![pid, fid])
                            .map_err(|e| e.to_string())?;
                        assigned += 1;
                        break;
                    }
                }
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(assigned)
}

// ============ 候选人列表（聚类 → 家人） ============

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterItem {
    pub cluster: i64,
    pub face_count: i64,
    pub photo_count: i64,
    pub rep_thumb: Option<String>,
    pub person_id: Option<i64>,
    pub person_name: Option<String>,
    pub is_family: bool,
}

#[tauri::command]
pub fn family_clusters(state: State<'_, AppState>) -> Result<Vec<ClusterItem>, String> {
    let conn = lock(&state.db);
    let mut stmt = conn
        .prepare(
            "SELECT f.cluster, COUNT(*), COUNT(DISTINCT f.media_id),
                    (SELECT m.thumb FROM faces f2 JOIN media m ON m.id = f2.media_id
                     WHERE f2.cluster = f.cluster AND m.thumb IS NOT NULL
                     ORDER BY f2.score DESC LIMIT 1),
                    (SELECT f3.person_id FROM faces f3 WHERE f3.cluster = f.cluster AND f3.person_id IS NOT NULL LIMIT 1)
             FROM faces f
             WHERE f.cluster IS NOT NULL
             GROUP BY f.cluster
             ORDER BY COUNT(*) DESC",
        )
        .map_err(|e| e.to_string())?;
    let mut items = stmt
        .query_map([], |r| {
            Ok(ClusterItem {
                cluster: r.get(0)?,
                face_count: r.get(1)?,
                photo_count: r.get(2)?,
                rep_thumb: r.get(3)?,
                person_id: r.get(4)?,
                person_name: None,
                is_family: false,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect::<Vec<_>>();
    for it in &mut items {
        if let Some(pid) = it.person_id {
            let (name, fam): (String, i64) = conn
                .query_row(
                    "SELECT name, is_family FROM persons WHERE id = ?1",
                    [pid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap_or_default();
            it.person_name = Some(name);
            it.is_family = fam == 1;
        }
    }
    Ok(items)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyMember {
    pub id: i64,
    pub name: String,
    pub face_count: i64,
    pub photo_count: i64,
}

#[tauri::command]
pub fn family_members(state: State<'_, AppState>) -> Result<Vec<FamilyMember>, String> {
    let conn = lock(&state.db);
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.name,
                    (SELECT COUNT(*) FROM faces f WHERE f.person_id = p.id),
                    (SELECT COUNT(DISTINCT media_id) FROM faces f WHERE f.person_id = p.id)
             FROM persons p WHERE p.is_family = 1 ORDER BY p.id",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(FamilyMember {
                id: r.get(0)?,
                name: r.get(1)?,
                face_count: r.get(2)?,
                photo_count: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(items)
}

/// 命名一个簇并注册为家人
#[tauri::command]
pub fn family_register(
    state: State<'_, AppState>,
    cluster: i64,
    name: String,
) -> Result<i64, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("给家人起个名字".into());
    }
    let mut conn = lock(&state.db);
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO persons(name, is_family, created_at) VALUES (?1, 1, ?2)",
        rusqlite::params![name, db::now_secs()],
    )
    .map_err(|e| e.to_string())?;
    let pid = tx.last_insert_rowid();
    tx.execute(
        "UPDATE faces SET person_id = ?1 WHERE cluster = ?2",
        rusqlite::params![pid, cluster],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(pid)
}

/// 把一个簇并入已有家人（该簇尚未命名时使用）
#[tauri::command]
pub fn family_assign_cluster(
    state: State<'_, AppState>,
    cluster: i64,
    person_id: i64,
) -> Result<(), String> {
    let conn = lock(&state.db);
    conn.execute(
        "UPDATE faces SET person_id = ?1 WHERE cluster = ?2",
        rusqlite::params![person_id, cluster],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 取消注册（脸保留，person 删除）
#[tauri::command]
pub fn family_remove(state: State<'_, AppState>, person_id: i64) -> Result<(), String> {
    let conn = lock(&state.db);
    conn.execute("UPDATE faces SET person_id = NULL WHERE person_id = ?1", [person_id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM persons WHERE id = ?1", [person_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
