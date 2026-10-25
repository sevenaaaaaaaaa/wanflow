//! M2 质量分析编排：评分 → 连拍分组 → 智能初筛建议。
//! 一切建议只是"建议"：应用才产生 verdict 事件（可撤销），忽略按 (media, rule) 持久化不再打扰。

use crate::db::{self, EFF_DECISION, EFF_TIME};
use crate::quality;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct AnalyzeSummary {
    pub scored: usize,
    pub bursts: usize,
    pub suggestions: usize,
}

/// 评分：只处理新增/重扫过的图片（scored_at < scanned_at）
fn score_pass(
    conn: &mut Connection,
    progress: &dyn Fn(usize, usize, &str),
) -> Result<usize, String> {
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, thumb FROM media
                 WHERE kind = 'image' AND thumb IS NOT NULL
                   AND (scored_at IS NULL OR scored_at < scanned_at)
                 ORDER BY id",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };
    let total = rows.len();
    let mut done = 0usize;
    for chunk in rows.chunks(50) {
        {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            {
                let mut stmt = tx
                    .prepare(
                        "UPDATE media SET sharpness=?1, exposure=?2, contrast=?3, colorfulness=?4,
                         brightness=?5, quality=?6, scored_at=?7 WHERE id=?8",
                    )
                    .map_err(|e| e.to_string())?;
                for (id, thumb) in chunk {
                    let m = match image::ImageReader::open(Path::new(thumb))
                        .ok()
                        .and_then(|r| r.decode().ok())
                        .map(|img| quality::analyze(&img))
                    {
                        Some(m) => m,
                        None => continue, // 缩略图损坏：跳过，下次再算
                    };
                    stmt.execute(rusqlite::params![
                        m.sharpness,
                        m.exposure,
                        m.contrast,
                        m.colorfulness,
                        m.brightness,
                        m.quality,
                        db::now_secs(),
                        id
                    ])
                    .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().map_err(|e| e.to_string())?;
        }
        done += chunk.len();
        progress(done, total, &chunk.last().map(|(_, p)| short_name(p)).unwrap_or_default());
    }
    Ok(total)
}

fn short_name(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_string()
}

const BURST_WINDOW_SECS: i64 = 5;
const BURST_PHASH_MAX: u32 = 12;

/// 连拍分组：时间上紧邻（级联窗口）且画面相似（pHash）的连续 shot 链。
/// 只信任 time_confidence>=1 的照片（EXIF/容器/文件名）；mtime 来源在批量导入时
/// 全是导入时刻，链起来会是假连拍。
fn burst_pass(conn: &mut Connection, progress: &dyn Fn(usize, usize, &str)) -> Result<usize, String> {
    // 一次性取齐分数，避免分组循环里逐条查库
    let mut quality_map: HashMap<i64, f64> = HashMap::new();
    let mut sharp_map: HashMap<i64, f64> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, COALESCE(quality,-1.0), COALESCE(sharpness,-1.0) FROM media")
            .map_err(|e| e.to_string())?;
        let it = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        for (id, q, s) in it.flatten() {
            quality_map.insert(id, q);
            sharp_map.insert(id, s);
        }
    }

    let rows: Vec<(i64, i64, u64)> = {
        let sql = format!(
            "SELECT m.id, COALESCE(ot.eff, m.base_time), m.phash
             FROM media m {EFF_TIME}
             WHERE m.kind = 'image' AND m.phash != '' AND m.time_confidence >= 1
             ORDER BY COALESCE(ot.eff, m.base_time) ASC, m.id ASC"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |r| {
                let ph: String = r.get(2)?;
                Ok((r.get(0)?, r.get(1)?, crate::phash::from_hex(&ph).unwrap_or(0)))
            })
            .map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };

    // 链式分组：与前一张 时间窗内 + 相似 即同链
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for (i, &(_, t, ph)) in rows.iter().enumerate() {
        if let Some(chain) = chains.last_mut() {
            let (_, pt, pph) = rows[*chain.last().unwrap()];
            if t - pt <= BURST_WINDOW_SECS && crate::phash::hamming(pph, ph) <= BURST_PHASH_MAX {
                chain.push(i);
                continue;
            }
        }
        chains.push(vec![i]);
    }

    conn.execute("UPDATE media SET burst_group = NULL, burst_best = 0", [])
        .map_err(|e| e.to_string())?;

    let burst_count = chains.iter().filter(|c| c.len() >= 2).count();
    let mut done = 0usize;
    for chain in chains.iter().filter(|c| c.len() >= 2) {
        let group_id = rows[chain[0]].0;
        // 最佳 = quality 最高（未评分视为 -1），平局取清晰度，再取 id 小
        let score = |i: usize| {
            (
                *quality_map.get(&rows[i].0).unwrap_or(&-1.0),
                *sharp_map.get(&rows[i].0).unwrap_or(&-1.0),
            )
        };
        let mut best_idx = chain[0];
        let mut best_key = score(best_idx);
        for &i in chain.iter().skip(1) {
            let k = score(i);
            if k.0 > best_key.0
                || (k.0 == best_key.0 && (k.1 > best_key.1 || (k.1 == best_key.1 && rows[i].0 < rows[best_idx].0)))
            {
                best_idx = i;
                best_key = k;
            }
        }
        let best_id = rows[best_idx].0;
        {
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            {
                let mut stmt = tx
                    .prepare("UPDATE media SET burst_group = ?1, burst_best = (id = ?2) WHERE id = ?3")
                    .map_err(|e| e.to_string())?;
                for &i in chain {
                    stmt.execute(rusqlite::params![group_id, best_id, rows[i].0])
                        .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().map_err(|e| e.to_string())?;
        }
        done += 1;
        progress(done, burst_count, &format!("连拍组 {group_id}（{} 张）", chain.len()));
    }
    Ok(burst_count)
}

/// 重新生成 pending 建议（忽略过的 media×rule 组合不再打扰）。
fn suggestion_pass(conn: &mut Connection, progress: &dyn Fn(usize, usize, &str)) -> Result<usize, String> {
    conn.execute("DELETE FROM suggestions WHERE status = 'pending'", [])
        .map_err(|e| e.to_string())?;
    let now = db::now_secs();

    // 已有人工决定的不建议
    let decided: HashSet<i64> = {
        let sql = format!("SELECT m.id FROM media m {EFF_DECISION} WHERE vd.decision IS NOT NULL");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let mapped = stmt.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };

    let rows: Vec<(i64, f64, Option<i64>, Option<i64>, Option<f64>, Option<f64>)> = {
        // id, quality, dup_group, burst_group, sharpness, brightness(区分过/欠曝文案)
        let mut stmt = conn
            .prepare(
                "SELECT id, COALESCE(quality,-1), dup_group, burst_group, sharpness, brightness
                 FROM media WHERE kind = 'image'",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })
            .map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };

    let mut burst_sizes: HashMap<i64, usize> = HashMap::new();
    for (_, _, _, bg, _, _) in &rows {
        if let Some(g) = bg {
            *burst_sizes.entry(*g).or_insert(0) += 1;
        }
    }
    let best_of: HashMap<i64, i64> = {
        let mut stmt = conn
            .prepare("SELECT burst_group, id FROM media WHERE burst_best = 1")
            .map_err(|e| e.to_string())?;
        let mapped = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };

    // 已忽略的 media×rule 组合不再打扰（部分唯一索引不约束 pending 行，需显式排除）
    let dismissed: HashSet<(i64, String)> = {
        let mut stmt = conn
            .prepare("SELECT media_id, rule FROM suggestions WHERE status = 'dismissed'")
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };

    let mut out: Vec<(i64, String, String, String)> = Vec::new(); // media, rule, reason, proposed
    for &(id, q, dup, bg, sharp, bright) in &rows {
        if decided.contains(&id) {
            continue;
        }
        let is_best = best_of.values().any(|b| *b == id);
        // 规则顺序：亮度极值（欠曝/过曝）先于模糊——暗部细节被 JPEG 压掉的照片
        // 清晰度必然低，"画面发黑"比"模糊"更可行动；连拍最佳不删（组里总要留一张）
        if !is_best {
            if let Some(b) = bright {
                if b < 8.0 || b > 248.0 {
                    if !dismissed.contains(&(id, "exposure".to_string())) {
                        let why = if b > 248.0 { "严重过曝（画面发白）" } else { "严重欠曝（画面发黑）" };
                        out.push((id, "exposure".into(), format!("{why}"), "reject".into()));
                    }
                    continue;
                }
            }
            if let Some(s) = sharp {
                if s < 12.0 && !dismissed.contains(&(id, "blur".to_string())) {
                    out.push((id, "blur".into(), format!("画面模糊（清晰度 {s:.0}/100，低于 12）"), "reject".into()));
                    continue;
                }
            }
        }
        // 规则 3：连拍非最佳（精确重复组交给重复规则处理，避免过度淘汰）
        if !is_best && dup.is_none() {
            if let Some(g) = bg {
                let size = burst_sizes.get(&g).copied().unwrap_or(0);
                if !dismissed.contains(&(id, "burst".to_string())) {
                    out.push((
                        id,
                        "burst".into(),
                        format!("{size} 张连拍，已自动选最佳（本张质量 {q:.0}）"),
                        "reject".into(),
                    ));
                }
            }
        }
    }

    let total = out.len();
    progress(total, total, "写入选片建议");
    {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR IGNORE INTO suggestions(media_id, rule, reason, proposed, status, created_at)
                     VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
                )
                .map_err(|e| e.to_string())?;
            for (id, rule, reason, proposed) in &out {
                stmt.execute(rusqlite::params![id, rule, reason, proposed, now])
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(total)
}

pub fn run(
    conn: &mut Connection,
    progress: &dyn Fn(usize, usize, &str),
) -> Result<AnalyzeSummary, String> {
    progress(0, 0, "开始质量评分");
    let scored = score_pass(conn, progress)?;
    progress(0, 0, "检测连拍");
    let bursts = burst_pass(conn, progress)?;
    progress(0, 0, "生成智能初筛建议");
    let suggestions = suggestion_pass(conn, progress)?;
    Ok(AnalyzeSummary { scored, bursts, suggestions })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn short_name_works() {
        assert_eq!(short_name("/a/b/c.jpg"), "c.jpg");
        assert_eq!(short_name("c.jpg"), "c.jpg");
    }

    #[cfg(test)]
    fn insert_media(
        conn: &Connection,
        path: &str,
        conf: i64,
        base_time: i64,
        phash: &str,
    ) -> i64 {
        conn.execute(
            "INSERT INTO media(path, filename, ext, kind, size_bytes, mtime, content_hash,
                phash, base_time, time_source, time_confidence, scanned_at)
             VALUES (?1, ?1, 'jpg', 'image', 1, 0, ?2, ?3, ?4, '测试', ?5, 0)",
            params![path, format!("hash-{path}"), phash, base_time, conf],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    /// 端到端冒烟：真实图片 → 评分 → 糊/暗建议
    #[test]
    fn pipeline_scores_and_suggests() {
        use image::{Rgb, RgbImage};

        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        let thumbs = dir.path().join("thumbs");

        let noise_img = |name: &str, lo: u8, amp: u64, blur: f32| {
            let mut s = 42_u64;
            let mut img = RgbImage::new(200, 200);
            for (_, _, p) in img.enumerate_pixels_mut() {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                // 取高位：xorshift 低位周期短，直接取模会得到低熵图案
                let v = lo + ((s >> 33) % amp) as u8;
                *p = Rgb([v, v, v]);
            }
            let img = image::DynamicImage::ImageRgb8(img);
            if blur > 0.0 {
                img.blur(blur).save(media.join(name)).unwrap();
            } else {
                img.save(media.join(name)).unwrap();
            }
        };
        noise_img("IMG_20231005_100000.jpg", 120, 40, 0.0); // 正常
        noise_img("blurred.jpg", 120, 40, 4.0); // 糊
        noise_img("dark.jpg", 0, 16, 0.0); // 暗而锐（亮度均值 ~7.5）

        let mut conn = crate::db::init(&dir.path().join("t.db")).unwrap();
        crate::scanner::scan_core(&media, &mut conn, &thumbs, &|_, _, _| {}, None);
        let summary = run(&mut conn, &|_, _, _| {}).unwrap();
        assert_eq!(summary.scored, 3);

        let by_rule: HashMap<String, i64> = {
            let mut stmt = conn
                .prepare("SELECT rule, COUNT(*) FROM suggestions WHERE status='pending' GROUP BY rule")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                .unwrap()
                .flatten()
                .collect()
        };
        assert_eq!(by_rule.get("blur"), Some(&1), "{by_rule:?}");
        assert_eq!(by_rule.get("exposure"), Some(&1), "{by_rule:?}");

        // 二次运行幂等：不重算、建议量稳定
        let summary2 = run(&mut conn, &|_, _, _| {}).unwrap();
        assert_eq!(summary2.scored, 0);
    }

    #[test]
    fn burst_pass_groups_and_picks_best() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::init(&dir.path().join("t.db")).unwrap();

        let a = insert_media(&conn, "a.jpg", 1, 1000, "aaaa");
        let b = insert_media(&conn, "b.jpg", 1, 1002, "aaab"); // 距离 1
        let c = insert_media(&conn, "c.jpg", 1, 1004, "aaac"); // 距离 2
        let _far = insert_media(&conn, "far.jpg", 1, 2000, "ffff");
        // mtime 来源（conf 0）即使时间紧邻、画面相同也不进连拍
        let _mtime = insert_media(&conn, "mm.jpg", 0, 1001, "aaaa");

        for (id, q) in [(a, 50.0), (b, 70.0), (c, 60.0)] {
            conn.execute("UPDATE media SET quality = ?1 WHERE id = ?2", params![q, id]).unwrap();
        }

        let bursts = burst_pass(&mut conn, &|_, _, _| {}).unwrap();
        assert_eq!(bursts, 1);
        let (grouped, best): (i64, Option<i64>) = conn
            .query_row(
                "SELECT COUNT(*),
                        COALESCE((SELECT id FROM media WHERE burst_best = 1), NULL)
                 FROM media WHERE burst_group IS NOT NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(grouped, 3);
        assert_eq!(best, Some(b)); // 最高 quality 者为最佳
        let mtime_grouped: i64 = conn
            .query_row("SELECT COUNT(*) FROM media WHERE path LIKE 'mm%' AND burst_group IS NOT NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mtime_grouped, 0);
    }

    #[test]
    fn suggestion_rules_and_exclusions() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = crate::db::init(&dir.path().join("t.db")).unwrap();

        let blur = insert_media(&conn, "blurry.jpg", 1, 100, "1111");
        let dark = insert_media(&conn, "dark.jpg", 1, 100, "2222");
        let best = insert_media(&conn, "best.jpg", 1, 100, "3333");
        let member = insert_media(&conn, "member.jpg", 1, 100, "3334");
        let in_dup = insert_media(&conn, "dup.jpg", 1, 100, "3335");
        let decided = insert_media(&conn, "decided.jpg", 1, 100, "3336");
        let dismissed = insert_media(&conn, "dismissed.jpg", 1, 100, "3337");

        let set = |id: i64, sql: &str| {
            conn.execute(sql, params![id]).unwrap();
        };
        set(blur, "UPDATE media SET sharpness = 5, quality = 20 WHERE id = ?1");
        set(dark, "UPDATE media SET sharpness = 40, brightness = 2, quality = 30 WHERE id = ?1");
        set(best, "UPDATE media SET sharpness = 80, brightness = 128, quality = 90, burst_group = 9, burst_best = 1 WHERE id = ?1");
        set(member, "UPDATE media SET sharpness = 50, brightness = 128, quality = 60, burst_group = 9 WHERE id = ?1");
        set(in_dup, "UPDATE media SET sharpness = 50, brightness = 128, quality = 55, dup_group = 5, burst_group = 9 WHERE id = ?1");
        set(decided, "UPDATE media SET sharpness = 3, quality = 10 WHERE id = ?1");
        set(dismissed, "UPDATE media SET sharpness = 4, quality = 10 WHERE id = ?1");

        // decided 已有保留决定
        conn.execute("INSERT INTO actions(kind, label, created_at) VALUES ('decide', '测试', 0)", []).unwrap();
        let action = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO verdicts(action_id, media_id, decision, stars) VALUES (?1, ?2, 'keep', NULL)",
            params![action, decided],
        )
        .unwrap();
        // dismissed 已忽略
        conn.execute(
            "INSERT INTO suggestions(media_id, rule, reason, proposed, status, created_at)
             VALUES (?1, 'blur', 'x', 'reject', 'dismissed', 0)",
            params![dismissed],
        )
        .unwrap();

        let n = suggestion_pass(&mut conn, &|_, _, _| {}).unwrap();
        assert_eq!(n, 3, "糊/暗/连拍非最佳（dup 优先豁免、已决定豁免、已忽略豁免）");

        let rules: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT rule FROM suggestions WHERE status='pending' ORDER BY rule")
                .unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().flatten().collect()
        };
        assert_eq!(rules, vec!["blur".to_string(), "burst".to_string(), "exposure".to_string()]);
    }
}
