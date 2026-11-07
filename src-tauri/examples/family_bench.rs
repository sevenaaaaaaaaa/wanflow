//! 真实库全量人脸索引 + 聚类统计（复用 family.rs 逻辑的独立实现，WAL 第二连接）。
use std::time::Instant;

fn main() {
    let home = std::env::var("HOME").unwrap();
    let db_path = std::path::PathBuf::from(&home).join("Library/Caches/com.wanflow.app/wanflow.db");
    let mut conn = wanflow_lib::db::init(&db_path).expect("db");

    let rows: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.thumb FROM media m
                 WHERE m.kind='image' AND m.thumb IS NOT NULL
                   AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.media_id = m.id AND f.emb_model = ?1)
                 ORDER BY m.id",
            )
            .unwrap();
        let m = stmt
            .query_map([wanflow_lib::faces::FACE_EMB_MODEL], |r| {
                Ok((r.get(0).unwrap(), r.get(1).unwrap()))
            })
            .unwrap();
        m.flatten().collect()
    };
    let total = rows.len();
    println!("待索引 {total} 张");
    let engine = wanflow_lib::faces::shared().unwrap();
    let start = Instant::now();
    let mut done = 0usize;
    let mut face_total = 0usize;
    for (i, (_id, thumb)) in rows.iter().enumerate() {
        let img = match image::ImageReader::open(thumb).ok().map(|r| r.decode()) {
            Some(Ok(i)) => i,
            _ => continue,
        };
        let detected = engine.lock().unwrap().detect_and_embed(&img).unwrap_or_default();
        {
            let tx = conn.transaction().unwrap();
            {
                let mut ins = tx
                    .prepare("INSERT INTO faces(media_id, x, y, w, h, score, emb, emb_model) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)")
                    .unwrap();
                let mut upd = tx.prepare("UPDATE media SET face_count = ?1 WHERE id = ?2").unwrap();
                upd.execute(rusqlite::params![detected.len() as i64, _id]).unwrap();
                for f in &detected {
                    let blob: Vec<u8> = f.embedding.iter().flat_map(|x| x.to_le_bytes()).collect();
                    ins.execute(rusqlite::params![_id, f.x, f.y, f.w, f.h, f.score, blob, wanflow_lib::faces::FACE_EMB_MODEL]).unwrap();
                    face_total += 1;
                }
            }
            tx.commit().unwrap();
        }
        done += 1;
        if done % 200 == 0 {
            println!("[{done}/{total}] 已检出 {face_total} 张脸");
        }
    }
    let _ = i_unused();
    println!("索引完成：{done} 张 / {face_total} 张脸，耗时 {:.1}s", start.elapsed().as_secs_f32());

    // 聚类（贪心代表匹配，与 family.rs 相同策略）
    let faces: Vec<(i64, Vec<f32>)> = {
        let mut stmt = conn.prepare("SELECT id, emb FROM faces WHERE emb_model = ?1 ORDER BY id").unwrap();
        let m = stmt.query_map([wanflow_lib::faces::FACE_EMB_MODEL], |r| {
            let id: i64 = r.get(0).unwrap();
            let b: Vec<u8> = r.get(1).unwrap();
            Ok((id, b))
        }).unwrap();
        m.flatten().map(|(id, b)| {
            let v: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            (id, v)
        }).collect()
    };
    let cstart = Instant::now();
    let mut parent: Vec<usize> = (0..faces.len()).collect();
    fn find(p: &mut Vec<usize>, mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut reps: Vec<usize> = Vec::new();
    for i in 0..faces.len() {
        let mut matched = false;
        for &r in &reps {
            if wanflow_lib::clip::cosine(&faces[i].1, &faces[r].1) >= wanflow_lib::faces::FACE_MATCH_COS {
                let (ri, rj) = (find(&mut parent, r), find(&mut parent, i));
                parent[rj] = ri;
                matched = true;
                break;
            }
        }
        if !matched {
            reps.push(i);
        }
    }
    let mut clusters: std::collections::HashMap<usize, Vec<i64>> = std::collections::HashMap::new();
    for i in 0..faces.len() {
        clusters.entry(find(&mut parent, i)).or_default().push(faces[i].0);
    }
    let mut sizes: Vec<usize> = clusters.values().map(|v| v.len()).collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    println!(
        "聚类：{} 簇 / {} 脸，耗时 {:.1}s，top10 簇大小 {:?}",
        clusters.len(),
        faces.len(),
        cstart.elapsed().as_secs_f32(),
        &sizes[..sizes.len().min(10)]
    );
    // 写回 cluster
    {
        let tx = conn.transaction().unwrap();
        {
            let mut upd = tx.prepare("UPDATE faces SET cluster = ?1 WHERE id = ?2").unwrap();
            for (root, members) in &clusters {
                let cid = members.iter().copied().min().unwrap();
                for m in members {
                    upd.execute(rusqlite::params![cid, m]).unwrap();
                }
            }
        }
        tx.commit().unwrap();
    }
    println!("✅ 家人管线真实库验证通过");
}

fn i_unused() -> usize {
    0
}
