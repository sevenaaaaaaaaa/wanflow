use rusqlite::Connection;
use std::path::Path;

/// 旁车数据库：一切决策/修正只写这里，原图永不改动。
/// 决策与时间修正都是"事件"，undo = 把事件组标记为 undone，状态查询自动忽略。
pub fn init(db_path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    // 扫描线程与 UI 线程各自持有连接，写写并发时等锁而不是报错
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media(
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            filename TEXT NOT NULL,
            ext TEXT NOT NULL,
            kind TEXT NOT NULL,
            size_bytes INTEGER NOT NULL,
            mtime INTEGER NOT NULL,
            content_hash TEXT NOT NULL,
            phash TEXT NOT NULL DEFAULT '',
            base_time INTEGER NOT NULL,
            time_source TEXT NOT NULL,
            time_confidence INTEGER NOT NULL DEFAULT 0,
            width INTEGER,
            height INTEGER,
            thumb TEXT,
            preview TEXT,
            dup_group INTEGER,
            scanned_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_media_hash ON media(content_hash);
        CREATE INDEX IF NOT EXISTS idx_media_dup ON media(dup_group);

        CREATE TABLE IF NOT EXISTS actions(
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            label TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            undone INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS verdicts(
            id INTEGER PRIMARY KEY,
            action_id INTEGER NOT NULL REFERENCES actions(id),
            media_id INTEGER NOT NULL REFERENCES media(id),
            decision TEXT,
            stars INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_verdicts_media ON verdicts(media_id);

        CREATE TABLE IF NOT EXISTS time_overrides(
            id INTEGER PRIMARY KEY,
            action_id INTEGER NOT NULL REFERENCES actions(id),
            media_id INTEGER NOT NULL REFERENCES media(id),
            new_time INTEGER NOT NULL,
            note TEXT
        );

        CREATE TABLE IF NOT EXISTS suggestions(
            id INTEGER PRIMARY KEY,
            media_id INTEGER NOT NULL,
            rule TEXT NOT NULL,
            reason TEXT NOT NULL,
            proposed TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_sugg_status ON suggestions(status);

        CREATE TABLE IF NOT EXISTS sources(
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            label TEXT NOT NULL,
            root_path TEXT,
            url TEXT,
            username TEXT,
            remote_path TEXT,
            staging_path TEXT,
            last_scanned INTEGER,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS albums(
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            note TEXT,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS album_members(
            album_id INTEGER NOT NULL REFERENCES albums(id),
            media_id INTEGER NOT NULL REFERENCES media(id),
            score REAL,
            PRIMARY KEY (album_id, media_id)
        );
    ",
    )?;
    ensure_columns(&conn)?;
    Ok(conn)
}

/// 轻量迁移：给老库补新列（PRAGMA table_info 检查后 ALTER）
fn ensure_columns(conn: &Connection) -> Result<(), rusqlite::Error> {
    let cols: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(media)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.flatten().collect()
    };
    let alters: &[(&str, &str)] = &[
        ("sharpness", "ALTER TABLE media ADD COLUMN sharpness REAL"),
        ("exposure", "ALTER TABLE media ADD COLUMN exposure REAL"),
        ("contrast", "ALTER TABLE media ADD COLUMN contrast REAL"),
        ("colorfulness", "ALTER TABLE media ADD COLUMN colorfulness REAL"),
        ("brightness", "ALTER TABLE media ADD COLUMN brightness REAL"),
        ("quality", "ALTER TABLE media ADD COLUMN quality REAL"),
        ("scored_at", "ALTER TABLE media ADD COLUMN scored_at INTEGER"),
        ("burst_group", "ALTER TABLE media ADD COLUMN burst_group INTEGER"),
        ("burst_best", "ALTER TABLE media ADD COLUMN burst_best INTEGER"),
        ("source_id", "ALTER TABLE media ADD COLUMN source_id INTEGER"),
        ("emb", "ALTER TABLE media ADD COLUMN emb BLOB"),
        ("emb_model", "ALTER TABLE media ADD COLUMN emb_model TEXT"),
    ];
    for (name, sql) in alters {
        if !cols.contains(*name) {
            conn.execute(sql, [])?;
        }
    }
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_media_source ON media(source_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_media_emb ON media(emb_model)",
        [],
    )?;
    Ok(())
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// —— 有效状态查询片段（排除已撤销 action 的最新事件）——
pub const EFF_DECISION: &str = "LEFT JOIN (
    SELECT v.media_id AS mid, v.decision AS decision
    FROM verdicts v
    JOIN (SELECT v2.media_id AS mm, MAX(v2.id) AS mx
          FROM verdicts v2 JOIN actions a2 ON a2.id = v2.action_id AND a2.undone = 0
          WHERE v2.decision IS NOT NULL GROUP BY v2.media_id) lm ON lm.mx = v.id
) vd ON vd.mid = m.id";

pub const EFF_STARS: &str = "LEFT JOIN (
    SELECT v.media_id AS mid3, v.stars AS stars
    FROM verdicts v
    JOIN (SELECT v2.media_id AS mm3, MAX(v2.id) AS mx3
          FROM verdicts v2 JOIN actions a2 ON a2.id = v2.action_id AND a2.undone = 0
          WHERE v2.stars IS NOT NULL GROUP BY v2.media_id) lm3 ON lm3.mx3 = v.id
) vs ON vs.mid3 = m.id";

pub const EFF_TIME: &str = "LEFT JOIN (
    SELECT t.media_id AS mid4, t.new_time AS eff
    FROM time_overrides t
    JOIN (SELECT t2.media_id AS mm4, MAX(t2.id) AS mx4
          FROM time_overrides t2 JOIN actions a3 ON a3.id = t2.action_id AND a3.undone = 0
          GROUP BY t2.media_id) lt ON lt.mx4 = t.id
) ot ON ot.mid4 = m.id";
