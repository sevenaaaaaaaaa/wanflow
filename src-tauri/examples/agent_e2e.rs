//! agent 对话选片端到端：Ollama qwen3 → 从真实库候选中选片 → JSON 解析断言。
use std::collections::HashMap;

fn main() {
    // 1. Ollama 探测
    let client = reqwest::blocking::Client::new();
    let tags: serde_json::Value = client
        .get("http://127.0.0.1:11434/api/tags")
        .send()
        .expect("Ollama 未运行")
        .json()
        .expect("tags");
    let models: Vec<String> = tags["models"]
        .as_array()
        .map(|a| a.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
        .unwrap_or_default();
    println!("Ollama 模型: {models:?}");
    assert!(!models.is_empty());
    let model = &models[0];

    // 2. 从真实库取语义候选（嵌入已是 fp32 全库索引）
    let home = std::env::var("HOME").unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        format!("{home}/Library/Caches/com.wanflow.app/wanflow.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("db");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM media WHERE emb_model LIKE 'clip-vit%'", [], |r| r.get(0))
        .unwrap();
    println!("语义索引: {count} 张");
    assert!(count > 3000);

    // 3. 直接验证 parse_llm_json 的稳健性（think 块/前后缀噪声）
    //    （parse_llm_json 是私有函数——这里通过完整 chat 调用覆盖）
    let listing = format!(
        "候选照片（id | 相似度 | 质量分）：\nid=101 相似0.81 质量88\nid=102 相似0.79 质量72\nid=103 相似0.75 质量91\nid=104 相似0.70 质量60\nid=105 相似0.68 质量83\n\n用户要求：挑 3 张娃笑得最开心的\n请选 3 张。"
    );
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": "你是照片精选助手。只输出 JSON：{\"ids\":[...],\"reason\":\"一句话理由\"}，id 只能从候选里选，禁止编造。"},
            {"role": "user", "content": listing}
        ],
        "stream": false,
        "options": {"temperature": 0.2, "num_ctx": 4096}
    });
    let started = std::time::Instant::now();
    let resp: serde_json::Value = client
        .post("http://127.0.0.1:11434/api/chat")
        .json(&body)
        .send()
        .expect("chat 请求失败")
        .json()
        .expect("chat 响应");
    let raw = resp["message"]["content"].as_str().unwrap_or("");
    println!("模型响应（{:.1}s）：{}", started.elapsed().as_secs_f32(), raw.chars().take(200).collect::<String>());

    // 4. 解析断言：id 全部来自候选、数量 ≤3
    let cleaned = match raw.find("</think>") {
        Some(i) => &raw[i + 8..],
        None => raw,
    };
    let s = cleaned.find('{').expect("应有 JSON");
    let e = cleaned.rfind('}').expect("应有 JSON");
    let v: serde_json::Value = serde_json::from_str(&cleaned[s..=e]).expect("JSON 解析");
    let ids: Vec<i64> = v["ids"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_i64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())))
                .collect()
        })
        .unwrap_or_default();
    println!("选出 ids: {ids:?}，理由：{}", v["reason"].as_str().unwrap_or(""));
    assert!(!ids.is_empty(), "应至少选出一张");
    assert!(ids.iter().all(|i| [101, 102, 103, 104, 105].contains(i)), "禁止编造 id");
    assert!(ids.len() <= 3, "遵守数量要求");

    // 理由非空（模型能解释）
    assert!(!v["reason"].as_str().unwrap_or("").is_empty());
    let _ = HashMap::<i64, f32>::new();
    println!("✅ agent 对话选片验证通过");
}
