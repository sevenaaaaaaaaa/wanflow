//! v0.3 批量属性：标签。
//!
//! 标签是跨相册 / 跨时间轴的横向分组；打标签和取消标签都写成事件
//! （`actions` + `tag_ops`），撤销 = 把动作组标 `undone = 1`，
//! 当前成员由 `tag_members` 视图重算，不存冗余状态。

use crate::db;
use crate::state::AppState;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};
use tauri::State;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TagItem {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

pub(crate) fn list_tags_conn(conn: &Connection) -> rusqlite::Result<Vec<TagItem>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(tm.media_id)
         FROM tags t
         LEFT JOIN tag_members tm ON tm.tag_id = t.id
         GROUP BY t.id, t.name
         ORDER BY COUNT(tm.media_id) DESC, t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TagItem {
            id: r.get(0)?,
            name: r.get(1)?,
            count: r.get(2)?,
        })
    })?;
    rows.collect()
}

fn normalize(raw: &str) -> Result<String, String> {
    let name = raw.trim().trim_start_matches('#').trim().to_string();
    if name.is_empty() {
        return Err("标签名不能为空".into());
    }
    if name.chars().count() > 24 {
        return Err("标签名最多 24 个字".into());
    }
    Ok(name)
}

/// 打 / 取消标签。只对真正变化的照片落事件，幂等。
/// 返回给用户看的一句话（同时也是撤销条上的标签）。
pub(crate) fn assign_tags_conn(
    conn: &mut Connection,
    ids: &[i64],
    raw_name: &str,
    remove: bool,
) -> Result<String, String> {
    let name = normalize(raw_name)?;
    let mut seen = HashSet::new();
    let ids: Vec<i64> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    if ids.is_empty() {
        return Err("没有选中照片".into());
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let existing: Option<i64> = tx
        .query_row("SELECT id FROM tags WHERE name = ?1", [&name], |r| r.get(0))
        .ok();
    if remove && existing.is_none() {
        return Err(format!("没有「{name}」这个标签"));
    }

    let tag_id = match existing {
        Some(id) => id,
        None => {
            tx.execute(
                "INSERT INTO tags(name, created_at) VALUES (?1, ?2)",
                params![name, db::now_secs()],
            )
            .map_err(|e| e.to_string())?;
            tx.last_insert_rowid()
        }
    };

    let current: HashSet<i64> = {
        let mut stmt = tx
            .prepare("SELECT media_id FROM tag_members WHERE tag_id = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([tag_id], |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        rows.flatten().collect()
    };
    // 打标签只动「还没有」的；取消只动「已经有」的
    let changed: Vec<i64> = ids
        .into_iter()
        .filter(|id| current.contains(id) == remove)
        .collect();
    if changed.is_empty() {
        return Ok(if remove {
            format!("选中的照片都没有标签「{name}」")
        } else {
            format!("选中的照片都已带标签「{name}」")
        });
    }

    let label = format!(
        "{}「{}」{} 张",
        if remove { "取消标签" } else { "打标签" },
        name,
        changed.len()
    );
    tx.execute(
        "INSERT INTO actions(kind, label, created_at) VALUES ('tag', ?1, ?2)",
        params![label, db::now_secs()],
    )
    .map_err(|e| e.to_string())?;
    let action_id = tx.last_insert_rowid();
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO tag_ops(action_id, tag_id, media_id, added) VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(|e| e.to_string())?;
        for id in &changed {
            stmt.execute(params![action_id, tag_id, id, if remove { 0 } else { 1 }])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(label)
}

#[tauri::command]
pub fn tag_list(state: State<'_, AppState>) -> Result<Vec<TagItem>, String> {
    let conn = lock(&state.db);
    list_tags_conn(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn tag_assign(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    name: String,
    remove: Option<bool>,
) -> Result<String, String> {
    let mut conn = lock(&state.db);
    assign_tags_conn(&mut conn, &ids, &name, remove.unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(dir: &tempfile::TempDir) -> Connection {
        let conn = crate::db::init(&dir.path().join("t.db")).unwrap();
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO media(path, filename, ext, kind, size_bytes, mtime, content_hash,
                                   base_time, time_source, scanned_at)
                 VALUES (?1, ?2, 'jpg', 'image', 0, 0, ?3, 0, 'exif', 0)",
                params![
                    format!("/tmp/m{i}.jpg"),
                    format!("m{i}.jpg"),
                    format!("h{i}")
                ],
            )
            .unwrap();
        }
        conn
    }

    fn ids(conn: &Connection) -> Vec<i64> {
        let mut stmt = conn.prepare("SELECT id FROM media ORDER BY id").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0)).unwrap();
        rows.flatten().collect()
    }

    fn member_count(conn: &Connection, tag: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM tag_members tm JOIN tags t ON t.id = tm.tag_id WHERE t.name = ?1",
            [tag],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn assign_creates_tag_and_membership() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = seed(&dir);
        let all = ids(&conn);
        let label = assign_tags_conn(&mut conn, &all, "春天", false).unwrap();
        assert!(label.contains("3 张"), "label = {label}");
        assert_eq!(member_count(&conn, "春天"), 3);
        let tags = list_tags_conn(&conn).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "春天");
        assert_eq!(tags[0].count, 3);
    }

    #[test]
    fn assign_is_idempotent_and_writes_no_event() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = seed(&dir);
        let all = ids(&conn);
        assign_tags_conn(&mut conn, &all, "春天", false).unwrap();
        let actions_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM actions", [], |r| r.get(0))
            .unwrap();
        let label = assign_tags_conn(&mut conn, &all, "春天", false).unwrap();
        assert!(label.contains("都已带标签"), "label = {label}");
        let actions_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM actions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(actions_before, actions_after, "重复打标签不该新写事件");
        assert_eq!(member_count(&conn, "春天"), 3);
    }

    #[test]
    fn undo_removes_membership_without_deleting_rows() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = seed(&dir);
        let first_two = ids(&conn)[..2].to_vec();
        assign_tags_conn(&mut conn, &first_two, "周岁", false).unwrap();
        assert_eq!(member_count(&conn, "周岁"), 2);
        // 撤销 = 标记 undone，视图随之重算
        conn.execute("UPDATE actions SET undone = 1", []).unwrap();
        assert_eq!(member_count(&conn, "周岁"), 0);
        let raw: i64 = conn
            .query_row("SELECT COUNT(*) FROM tag_ops", [], |r| r.get(0))
            .unwrap();
        assert_eq!(raw, 2, "事件行保留，撤销只是标记");
        // 重做
        conn.execute("UPDATE actions SET undone = 0", []).unwrap();
        assert_eq!(member_count(&conn, "周岁"), 2);
    }

    #[test]
    fn remove_then_undo_restores() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = seed(&dir);
        let all = ids(&conn);
        assign_tags_conn(&mut conn, &all, "海", false).unwrap();
        let label = assign_tags_conn(&mut conn, &all[..1], "海", true).unwrap();
        assert!(label.contains("取消标签"), "label = {label}");
        assert_eq!(member_count(&conn, "海"), 2);
        // 最后一次动作（取消）撤销
        conn.execute(
            "UPDATE actions SET undone = 1 WHERE id = (SELECT MAX(id) FROM actions WHERE kind = 'tag')",
            [],
        )
        .unwrap();
        assert_eq!(member_count(&conn, "海"), 3);
    }

    #[test]
    fn rejects_empty_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = seed(&dir);
        let all = ids(&conn);
        assert!(assign_tags_conn(&mut conn, &[], "x", false).is_err());
        assert!(assign_tags_conn(&mut conn, &all, "   ", false).is_err());
        assert!(assign_tags_conn(&mut conn, &all, "不存在", true).is_err());
        let long = "字".repeat(25);
        assert!(assign_tags_conn(&mut conn, &all, &long, false).is_err());
    }
}
