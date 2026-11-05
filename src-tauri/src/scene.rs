//! 场景聚类：把连续拍摄按「时间邻近 + 内容相似」切成场景（scene）。
//!
//! 产品目标：同一段时间里，家人用不同设备、不同视角拍的同一场景要落进同一个
//! 场景；同一天里不同的场景要切开。设备 / 文件名 / 目录一律不参与判断——它们
//! 会把「家人换手机拍同一场景」拆散，那是本项目要攻破的第一件事。
//!
//! 纯函数 `cluster` 只依赖 (id, ts, emb)，便于单测；`rebuild` 负责读写旁车库。
//! 不读原图、不改原图；结果只是建议，随时可重算。

use crate::clip::{blob_to_vec, cosine};
use crate::db::EFF_TIME;
use crate::state::AppState;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use tauri::State;

/// 参与聚类的一张照片 / 一段视频
#[derive(Debug, Clone)]
pub struct SceneItem {
    pub id: i64,
    /// 生效时间（已含时间修正），unix 秒
    pub ts: i64,
    pub emb: Option<Vec<f32>>,
}

/// 阈值集中在这里。默认值只是起点，必须用真实照片校准后再定稿。
#[derive(Debug, Clone, Copy)]
pub struct SceneParams {
    /// 同一场景里相邻两张的最大间隔
    pub max_gap_secs: i64,
    /// 一个场景的最长跨度（防止「每天同一扇窗」被并成一段）
    pub max_span_secs: i64,
    /// 与场景质心的最低余弦相似度
    pub min_sim: f32,
    /// 没有 embedding 时只信时间，用更保守的间隔
    pub no_emb_gap_secs: i64,
}

impl Default for SceneParams {
    fn default() -> Self {
        Self {
            max_gap_secs: 90 * 60,
            max_span_secs: 6 * 3600,
            min_sim: 0.82,
            no_emb_gap_secs: 30 * 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// 场景键 = 场景第一张的 media id（重算时只要首张不变就稳定）
    pub key: i64,
    pub t0: i64,
    pub t1: i64,
    pub members: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildStats {
    pub scenes: i64,
    pub images: i64,
    pub with_emb: i64,
    pub missing_emb: i64,
    pub changed: i64,
}

struct Builder {
    key: i64,
    t0: i64,
    last: i64,
    members: Vec<i64>,
    sum: Vec<f32>,
    count: usize,
}

impl Builder {
    fn new(item: &SceneItem) -> Self {
        let mut b = Builder {
            key: item.id,
            t0: item.ts,
            last: item.ts,
            members: vec![item.id],
            sum: Vec::new(),
            count: 0,
        };
        b.push_emb(item);
        b
    }

    fn push_emb(&mut self, item: &SceneItem) {
        let Some(e) = item.emb.as_ref().filter(|v| !v.is_empty()) else {
            return;
        };
        if self.sum.is_empty() {
            self.sum = e.clone();
            self.count = 1;
        } else if self.sum.len() == e.len() {
            for (s, v) in self.sum.iter_mut().zip(e) {
                *s += v;
            }
            self.count += 1;
        }
    }

    /// 单位质心；没有任何有效 embedding 时为 None
    fn centroid(&self) -> Option<Vec<f32>> {
        if self.count == 0 {
            return None;
        }
        let mut c: Vec<f32> = self.sum.iter().map(|v| v / self.count as f32).collect();
        let norm = c.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 1e-6 {
            for v in c.iter_mut() {
                *v /= norm;
            }
        }
        Some(c)
    }

    fn fits(&self, item: &SceneItem, p: &SceneParams) -> bool {
        let gap = item.ts - self.last;
        if gap > p.max_gap_secs || item.ts - self.t0 > p.max_span_secs {
            return false;
        }
        match (self.centroid(), item.emb.as_deref().filter(|v| !v.is_empty())) {
            (Some(c), Some(e)) if c.len() == e.len() => cosine(&c, e) >= p.min_sim,
            // 任一侧没有内容证据时退回纯时间判断，且用更严的间隔
            _ => gap <= p.no_emb_gap_secs,
        }
    }

    fn push(&mut self, item: &SceneItem) {
        self.last = item.ts;
        self.members.push(item.id);
        self.push_emb(item);
    }

    fn finish(self) -> Scene {
        Scene {
            key: self.key,
            t0: self.t0,
            t1: self.last,
            members: self.members,
        }
    }
}

/// 纯函数：输入顺序任意，输出按时间升序、场景内成员按时间升序。
pub fn cluster(items: &[SceneItem], p: &SceneParams) -> Vec<Scene> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| (items[i].ts, items[i].id));

    let mut scenes: Vec<Scene> = Vec::new();
    let mut cur: Option<Builder> = None;
    for idx in order {
        let item = &items[idx];
        match cur.take() {
            None => cur = Some(Builder::new(item)),
            Some(mut b) => {
                if b.fits(item, p) {
                    b.push(item);
                    cur = Some(b);
                } else {
                    scenes.push(b.finish());
                    cur = Some(Builder::new(item));
                }
            }
        }
    }
    if let Some(b) = cur {
        scenes.push(b.finish());
    }
    scenes
}

/// 重算全库场景，把场景键写回 media.scene_group。返回统计，便于 UI 判断是否
/// 「一张都没嵌」而提示先建语义索引。
pub fn rebuild(conn: &mut Connection, p: &SceneParams) -> Result<RebuildStats, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut items: Vec<SceneItem> = Vec::new();
    {
        let sql = format!(
            "SELECT m.id, COALESCE(ot.eff, m.base_time) AS eff, m.emb
             FROM media m {EFF_TIME}
             WHERE m.kind IN ('image', 'video')
             ORDER BY eff ASC, m.id ASC"
        );
        let mut stmt = tx.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                let id: i64 = r.get(0)?;
                let ts: i64 = r.get(1)?;
                let blob: Option<Vec<u8>> = r.get(2)?;
                Ok(SceneItem {
                    id,
                    ts,
                    emb: blob.filter(|b| b.len() >= 4).map(|b| blob_to_vec(&b)),
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            items.push(row.map_err(|e| e.to_string())?);
        }
    }

    let with_emb = items.iter().filter(|i| i.emb.is_some()).count() as i64;
    let scenes = cluster(&items, p);
    let mut next: HashMap<i64, i64> = HashMap::new();
    for sc in &scenes {
        for id in &sc.members {
            next.insert(*id, sc.key);
        }
    }

    let mut changed = 0i64;
    {
        let mut upd = tx
            .prepare(
                "UPDATE media SET scene_group = ?1
                 WHERE id = ?2 AND (scene_group IS NULL OR scene_group != ?1)",
            )
            .map_err(|e| e.to_string())?;
        for (id, key) in &next {
            changed += upd.execute(params![key, id]).map_err(|e| e.to_string())? as i64;
        }
        // 不再属于任何场景的旧值清掉
        changed += tx
            .execute(
                "UPDATE media SET scene_group = NULL
                 WHERE scene_group IS NOT NULL AND id NOT IN (SELECT value FROM json_each(?1))",
                params![serde_json::to_string(&next.keys().collect::<Vec<_>>())
                    .map_err(|e| e.to_string())?],
            )
            .map_err(|e| e.to_string())? as i64;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(RebuildStats {
        scenes: scenes.len() as i64,
        images: items.len() as i64,
        with_emb,
        missing_emb: items.len() as i64 - with_emb,
        changed,
    })
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[tauri::command]
pub fn rebuild_scenes(state: State<'_, AppState>) -> Result<RebuildStats, String> {
    let mut conn = lock(&state.db);
    rebuild(&mut conn, &SceneParams::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::vec_to_blob;

    fn v2(x: f32, y: f32) -> Option<Vec<f32>> {
        Some(vec![x, y])
    }

    fn item(id: i64, ts: i64, emb: Option<Vec<f32>>) -> SceneItem {
        SceneItem { id, ts, emb }
    }

    #[test]
    fn empty_and_single() {
        let p = SceneParams::default();
        assert!(cluster(&[], &p).is_empty());
        let one = vec![item(7, 100, v2(1.0, 0.0))];
        let out = cluster(&one, &p);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, 7);
        assert_eq!(out[0].members, vec![7]);
        assert_eq!((out[0].t0, out[0].t1), (100, 100));
    }

    #[test]
    fn merges_same_scene_across_devices_and_time() {
        // 家人换手机、不同视角：文件名/设备无关，只看向量与时间
        let p = SceneParams::default();
        let items = vec![
            item(3, 1_000, v2(1.0, 0.0)),
            item(9, 1_600, v2(0.99, 0.05)),
            item(12, 2_400, v2(0.98, 0.08)),
        ];
        let out = cluster(&items, &p);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].members, vec![3, 9, 12]);
        assert_eq!(out[0].key, 3);
    }

    #[test]
    fn splits_different_scenes_at_same_time() {
        let p = SceneParams::default();
        let items = vec![
            item(1, 1_000, v2(1.0, 0.0)),
            item(2, 1_100, v2(0.0, 1.0)),
        ];
        let out = cluster(&items, &p);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].members, vec![1]);
        assert_eq!(out[1].members, vec![2]);
    }

    #[test]
    fn splits_on_time_gap() {
        let p = SceneParams::default();
        let items = vec![
            item(1, 0, v2(1.0, 0.0)),
            item(2, p.max_gap_secs + 1, v2(1.0, 0.0)),
        ];
        assert_eq!(cluster(&items, &p).len(), 2);
    }

    #[test]
    fn splits_on_scene_span() {
        let mut p = SceneParams::default();
        p.max_gap_secs = 2 * 3600;
        p.max_span_secs = 3 * 3600;
        let items = vec![
            item(1, 0, v2(1.0, 0.0)),
            item(2, 3600, v2(1.0, 0.0)),
            item(3, 7200, v2(1.0, 0.0)),
            // 间隔仍在 max_gap 内，但场景总跨度会超过 max_span → 必须切开
            item(4, 14400, v2(1.0, 0.0)),
        ];
        let out = cluster(&items, &p);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].members, vec![1, 2, 3]);
        assert_eq!(out[1].members, vec![4]);
    }

    #[test]
    fn cross_midnight_continuous_shoot_stays_one_scene() {
        let p = SceneParams::default();
        let day1 = 23 * 3600;
        let items = vec![
            item(1, day1, v2(1.0, 0.0)),
            item(2, day1 + 20 * 60, v2(0.99, 0.02)),
            item(3, day1 + 40 * 60, v2(0.98, 0.03)),
        ];
        let out = cluster(&items, &p);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].members.len(), 3);
    }

    #[test]
    fn same_scene_next_day_splits() {
        let p = SceneParams::default();
        let items = vec![
            item(1, 0, v2(1.0, 0.0)),
            item(2, 24 * 3600, v2(1.0, 0.0)),
        ];
        assert_eq!(cluster(&items, &p).len(), 2);
    }

    #[test]
    fn no_embedding_falls_back_to_strict_time() {
        let p = SceneParams::default();
        let items = vec![
            item(1, 0, None),
            item(2, 600, None),
            item(3, 600 + p.no_emb_gap_secs + 1, None),
        ];
        let out = cluster(&items, &p);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].members, vec![1, 2]);
        assert_eq!(out[1].members, vec![3]);
    }

    #[test]
    fn mixed_embedding_uses_time_fallback() {
        let p = SceneParams::default();
        let items = vec![
            item(1, 0, v2(1.0, 0.0)),
            item(2, 600, None),
            item(3, 1200, v2(1.0, 0.0)),
        ];
        let out = cluster(&items, &p);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].members, vec![1, 2, 3]);
    }

    #[test]
    fn mismatched_dimensions_do_not_panic_or_merge_by_content() {
        let p = SceneParams::default();
        let items = vec![
            item(1, 0, Some(vec![1.0, 0.0, 0.0])),
            item(2, 600, Some(vec![1.0, 0.0])),
        ];
        let out = cluster(&items, &p);
        assert_eq!(out.len(), 1); // 时间很近 → 时间兜底
    }

    #[test]
    fn deterministic_for_shuffled_input() {
        let p = SceneParams::default();
        let items = vec![
            item(5, 2_000, v2(1.0, 0.0)),
            item(2, 1_000, v2(1.0, 0.0)),
            item(8, 1_500, v2(0.0, 1.0)),
        ];
        let a = cluster(&items, &p);
        let mut shuffled = items.clone();
        shuffled.reverse();
        let b = cluster(&shuffled, &p);
        assert_eq!(a, b);
        // 1000/1500 内容不同 → 两段；2000 与 1500 内容不同 → 第三段
        assert_eq!(a.len(), 3);
        assert_eq!(a[0].key, 2);
        assert_eq!(a[1].key, 8);
        assert_eq!(a[2].key, 5);
    }

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open memory db");
        conn.execute_batch(
            "CREATE TABLE media(
                id INTEGER PRIMARY KEY, kind TEXT NOT NULL, base_time INTEGER NOT NULL,
                emb BLOB, scene_group INTEGER);
             CREATE TABLE actions(id INTEGER PRIMARY KEY, undone INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE time_overrides(
                id INTEGER PRIMARY KEY, action_id INTEGER NOT NULL,
                media_id INTEGER NOT NULL, new_time INTEGER NOT NULL);",
        )
        .expect("schema");
        conn
    }

    #[test]
    fn rebuild_writes_groups_and_is_idempotent() {
        let mut conn = mem_db();
        let p = SceneParams::default();
        let rows: [(i64, i64, Vec<f32>); 4] = [
            (1, 0, vec![1.0, 0.0]),
            (2, 600, vec![0.99, 0.05]),
            (3, 24 * 3600, vec![0.0, 1.0]),
            (4, 24 * 3600 + 600, vec![0.0, 1.0]),
        ];
        for (id, ts, emb) in rows {
            conn.execute(
                "INSERT INTO media(id, kind, base_time, emb) VALUES (?1, 'image', ?2, ?3)",
                params![id, ts, vec_to_blob(&emb)],
            )
            .expect("insert");
        }

        let first = rebuild(&mut conn, &p).expect("rebuild");
        assert_eq!(first.scenes, 2);
        assert_eq!(first.images, 4);
        assert_eq!(first.with_emb, 4);
        assert_eq!(first.missing_emb, 0);
        assert_eq!(first.changed, 4);

        let groups: Vec<(i64, Option<i64>)> = conn
            .prepare("SELECT id, scene_group FROM media ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(groups, vec![(1, Some(1)), (2, Some(1)), (3, Some(3)), (4, Some(3))]);

        let second = rebuild(&mut conn, &p).expect("rebuild again");
        assert_eq!(second.scenes, 2);
        assert_eq!(second.changed, 0);
    }

    #[test]
    fn rebuild_clears_stale_groups_and_counts_missing_embeddings() {
        let mut conn = mem_db();
        let p = SceneParams::default();
        conn.execute(
            "INSERT INTO media(id, kind, base_time, emb, scene_group) VALUES (1, 'image', 0, NULL, 99)",
            [],
        )
        .unwrap();
        let stats = rebuild(&mut conn, &p).expect("rebuild");
        assert_eq!(stats.missing_emb, 1);
        assert_eq!(stats.with_emb, 0);
        let group: Option<i64> = conn
            .query_row("SELECT scene_group FROM media WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(group, Some(1));
    }
}
