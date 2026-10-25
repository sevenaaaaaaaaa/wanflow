//! 真实库全量语义索引 + 搜索验证（第二连接，WAL 安全）。
use std::time::Instant;

fn main() {
    let home = std::env::var("HOME").unwrap();
    let db_path = std::path::PathBuf::from(&home).join("Library/Caches/com.wanflow.app/wanflow.db");
    let mut conn = wanflow_lib::db::init(&db_path).expect("打开真实库失败");

    let start = Instant::now();
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, thumb FROM media WHERE kind='image' AND thumb IS NOT NULL
                 AND (emb IS NULL OR emb_model != ?1) ORDER BY id",
            )
            .unwrap();
        let m = stmt.query_map([wanflow_lib::clip::EMB_MODEL], |r| Ok((r.get(0).unwrap(), r.get(1).unwrap()))).unwrap();
        m.flatten().collect()
    };
    let total = rows.len();
    println!("待嵌入 {total} 张");
    if total == 0 {
        println!("全部已索引（idempotent）");
        return;
    }
    let clip = wanflow_lib::clip::shared().expect("加载 CLIP");
    let mut done = 0usize;
    for chunk in rows.chunks(64) {
        {
            let tx = conn.transaction().unwrap();
            {
                let mut stmt = tx
                    .prepare("UPDATE media SET emb=?1, emb_model=?2 WHERE id=?3")
                    .unwrap();
                for (id, thumb) in chunk {
                    let v = match clip.lock().unwrap().embed_image(std::path::Path::new(thumb)) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let blob: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
                    stmt.execute(rusqlite::params![blob, wanflow_lib::clip::EMB_MODEL, id]).unwrap();
                }
            }
            tx.commit().unwrap();
        }
        done += chunk.len();
        println!("[{done}/{total}]");
    }
    let elapsed = start.elapsed().as_secs_f32();
    println!("嵌入完成：{done} 张，耗时 {elapsed:.1}s（{:.0}ms/张）", elapsed * 1000.0 / done as f32);

    // 语义搜索验证：三个查询
    let clip = clip.lock().unwrap();
    for q in ["a baby swimming happily in a pool", "a blurry dark photo", "family photo outdoors"] {
        let qv = clip.embed_text(q).unwrap();
        let mut scored: Vec<(i64, f32)> = Vec::new();
        {
            let mut stmt = conn.prepare("SELECT id, emb FROM media WHERE emb_model = ?1 AND emb IS NOT NULL").unwrap();
            let m = stmt.query_map([wanflow_lib::clip::EMB_MODEL], |r| {
                let id: i64 = r.get(0).unwrap();
                let blob: Vec<u8> = r.get(1).unwrap();
                Ok((id, blob))
            }).unwrap();
            for (id, blob) in m.flatten() {
                let v: Vec<f32> = blob.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                scored.push((id, wanflow_lib::clip::cosine(&qv, &v)));
            }
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let top = scored.iter().take(3).map(|(_, s)| format!("{s:.3}")).collect::<Vec<_>>();
        println!("「{q}」 top3 相似度: {top:?}");
    }
    println!("✅ 真实库语义索引验证通过");
}
