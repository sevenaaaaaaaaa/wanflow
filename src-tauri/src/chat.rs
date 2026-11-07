//! M5 agent 对话选片：Ollama 本地 LLM 在 CLIP 候选之上做重排与解释。
//! LLM 只能从工具给出的候选中选（id 原样透传），不产生幻觉 id；全部推理在本机。

use crate::clip;
use crate::db::{EFF_DECISION, EFF_TIME};
use crate::smart::ScoredMedia;
use crate::state::AppState;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use tauri::State;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn ollama_probe() -> (bool, Vec<String>) {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
    else {
        return (false, vec![]);
    };
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSelectResult {
    pub items: Vec<ScoredMedia>,
    pub reason: String,
    pub model: String,
    pub candidates: usize,
}

#[derive(Deserialize)]
pub struct AgentSelectQuery {
    pub prompt: String,
    pub pool: Option<String>,
    pub count: Option<u32>,
    pub model: Option<String>,
}

const SYSTEM_PROMPT: &str = r#"你是家庭照片精选助手"万照"，帮宝爸宝妈从照片库里挑照片。
规则：
1. 只能从候选列表里选，id 必须原样照抄，禁止编造 id。
2. 相似度越高与用户要求越相关；质量分是画面质量；"家人"列表示照片里有用户注册的家人（应优先）。
3. 用户没说数量就默认选 9 张；说的数量必须遵守。
4. 只输出严格 JSON，格式：{"ids":[id1,id2],"reason":"一句话说明你的选择理由"}
5. 直接输出 JSON，不要输出任何解释、思考过程或其它文字。"#;

#[tauri::command]
pub fn agent_select(
    state: State<'_, AppState>,
    query: AgentSelectQuery,
) -> Result<AgentSelectResult, String> {
    let prompt = query.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("说说你想挑什么样的照片".into());
    }
    let (ready, models) = ollama_probe();
    if !ready {
        return Err("没有检测到 Ollama。安装并运行 `ollama serve` 后重试（模型推荐 `ollama pull qwen3:0.6b`）".into());
    }
    let model = query
        .model
        .clone()
        .or_else(|| models.first().cloned())
        .ok_or("Ollama 里还没有模型：运行 `ollama pull qwen3:0.6b`")?;

    let conn = lock(&state.db);
    let count = query.count.unwrap_or(9).clamp(1, 50);

    // 工具层：CLIP 候选 + 家人/质量/时间元数据
    let candidates = gather_candidates(&conn, &prompt, query.pool.as_deref())?;
    if candidates.is_empty() {
        return Err("还没有语义索引：先在「智能精选」里完成建模".into());
    }

    let mut listing = String::from("候选照片（id | 与要求的相关度 | 质量分 | 家人 | 拍摄时间 | 状态）：\n");
    for c in &candidates {
        listing.push_str(&format!(
            "id={} 相似{:.2} 质量{:.0} 家人[{}] {} {}\n",
            c.id, c.sim, c.quality, c.family, c.time, c.decision
        ));
    }
    listing.push_str(&format!("\n用户要求：{prompt}\n请选 {count} 张。"));

    // LLM 层
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": listing}
        ],
        "stream": false,
        "options": {"temperature": 0.2, "num_ctx": 8192}
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post("http://127.0.0.1:11434/api/chat")
        .json(&body)
        .send()
        .map_err(|e| format!("连接 Ollama 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Ollama 返回 {}", resp.status()));
    }
    let data: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let raw = data
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let (ids, reason) = parse_llm_json(&raw, count)?;
    let by_id: std::collections::HashMap<i64, f32> =
        candidates.iter().map(|c| (c.id, c.sim)).collect();
    let items = crate::smart::media_by_ids_public(&conn, &ids)?;
    let mut items: Vec<ScoredMedia> = items
        .into_iter()
        .map(|item| {
            let score = by_id.get(&item.id).copied().unwrap_or(0.0);
            ScoredMedia { item, score }
        })
        .collect();
    items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let reason = if reason.is_empty() { "（模型没有给出理由）".into() } else { reason };
    Ok(AgentSelectResult { items, reason, model, candidates: candidates.len() })
}

struct Candidate {
    id: i64,
    sim: f32,
    quality: f64,
    family: String,
    time: String,
    decision: &'static str,
}

/// CLIP 语义候选（top 40）+ 家人/质量/时间标注；已淘汰的排除
fn gather_candidates(
    conn: &Connection,
    prompt: &str,
    pool: Option<&str>,
) -> Result<Vec<Candidate>, String> {
    let embeddings: Vec<(i64, Vec<f32>)> = {
        let mut stmt = conn
            .prepare("SELECT id, emb FROM media WHERE emb_model = ?1 AND emb IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let m = stmt
            .query_map([clip::EMB_MODEL], |r| {
                Ok((r.get::<_, i64>(0)?, blob_vec(&r.get::<_, Vec<u8>>(1)?)))
            })
            .map_err(|e| e.to_string())?;
        m.flatten().collect()
    };
    if embeddings.is_empty() {
        return Ok(vec![]);
    }
    let clip_shared = clip::shared()?;
    let qvec = clip_shared.lock().unwrap().embed_text(prompt)?;

    let pool_keep = pool == Some("keep");
    let mut keep_ids = std::collections::HashSet::new();
    if pool_keep {
        let sql = format!("SELECT m.id FROM media m {EFF_DECISION} WHERE vd.decision = 'keep'");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let m = stmt.query_map([], |r| r.get::<_, i64>(0)).map_err(|e| e.to_string())?;
        keep_ids = m.flatten().collect();
    }

    let sql = format!(
        "SELECT m.id, COALESCE(m.quality, 50.0), COALESCE(ot.eff, m.base_time),
                COALESCE(vd.decision, ''),
                (SELECT GROUP_CONCAT(DISTINCT p.name) FROM faces f JOIN persons p ON p.id = f.person_id
                 WHERE f.media_id = m.id AND p.is_family = 1)
         FROM media m {EFF_DECISION} {EFF_TIME}
         WHERE m.kind = 'image' AND NOT COALESCE(vd.decision, '') = 'reject'"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let m = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut scored: Vec<(i64, f32)> = Vec::new();
    let emb_map: HashMap<i64, &Vec<f32>> = embeddings.iter().map(|(i, v)| (*i, v)).collect();
    for (id, _q, _t, _d, _fam) in m.flatten() {
        if pool_keep && !keep_ids.contains(&id) {
            continue;
        }
        if let Some(v) = emb_map.get(&id) {
            scored.push((id, clip::cosine(&qvec, v)));
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(40);

    // 元数据连查（数量小，直接循环查）
    let mut out = Vec::with_capacity(scored.len());
    for (id, sim) in scored {
        let (quality, time, decision, family): (f64, i64, String, Option<String>) = {
            let sql2 = format!(
                "SELECT COALESCE(m.quality, 50.0), COALESCE(ot.eff, m.base_time),
                        COALESCE(vd.decision, '未处理'),
                        (SELECT GROUP_CONCAT(DISTINCT p.name) FROM faces f JOIN persons p ON p.id = f.person_id
                         WHERE f.media_id = m.id AND p.is_family = 1)
                 FROM media m {EFF_DECISION} {EFF_TIME} WHERE m.id = ?1"
            );
            let mut s2 = conn.prepare(&sql2).map_err(|e| e.to_string())?;
            s2.query_row([id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| e.to_string())?
        };
        let decision: &'static str = match decision.as_str() {
            "keep" => "已保留",
            "pending" => "待定",
            "未处理" => "未处理",
            _ => "已保留",
        };
        out.push(Candidate {
            id,
            sim,
            quality,
            family: family.unwrap_or_else(|| "无".into()),
            time: chrono::DateTime::from_timestamp(time, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default(),
            decision,
        });
    }
    Ok(out)
}

fn blob_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// 从 LLM 输出提取 JSON（剥掉 qwen3 的 <think> 块，截取首尾大括号）
fn parse_llm_json(raw: &str, count: u32) -> Result<(Vec<i64>, String), String> {
    let mut text = raw;
    if let Some(end) = text.find("</think>") {
        text = &text[end + 8..];
    }
    let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) else {
        return Err(format!("模型没有输出 JSON：{}", &raw.chars().take(120).collect::<String>()));
    };
    let json_text = &text[start..=end];
    let v: serde_json::Value = serde_json::from_str(json_text)
        .map_err(|e| format!("解析模型 JSON 失败: {e}；原文：{json_text:.200}"))?;
    let ids: Vec<i64> = v
        .get("ids")
        .and_then(|i| i.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok()))
                })
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err("模型没有选出任何照片".into());
    }
    let mut ids = ids;
    ids.truncate(count as usize);
    let reason = v
        .get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .to_string();
    Ok((ids, reason))
}
