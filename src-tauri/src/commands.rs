//! Tauri 命令层：前端 ↔ 旁车库。所有命令同步、快速返回；
//! 扫描在后台线程跑，进度用事件推送。

use crate::db::{EFF_DECISION, EFF_STARS, EFF_TIME};
use crate::webdav;
use crate::{db, scanner, state::{AppState, ScanStatus, ScanSummary}, timefix};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ============ 扫描与导入 ============

/// 建来源行并返回 id（所有导入流程的公共入口）
fn create_source(
    conn: &Connection,
    kind: &str,
    label: &str,
    root_path: Option<&str>,
    url: Option<&str>,
    username: Option<&str>,
    remote_path: Option<&str>,
    staging_path: Option<&str>,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO sources(kind, label, root_path, url, username, remote_path, staging_path, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        rusqlite::params![kind, label, root_path, url, username, remote_path, staging_path, db::now_secs()],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

fn guard_scan(state: &State<'_, AppState>) -> Result<(), String> {
    let mut s = lock(&state.scan);
    if s.running {
        return Err("已有导入/扫描在进行中".into());
    }
    *s = ScanStatus { running: true, phase: "准备中".into(), ..Default::default() };
    Ok(())
}

/// 文件夹类导入（本地文件夹 / QQ·微信导出目录）
#[tauri::command]
pub fn import_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    kind: String,
    label: String,
) -> Result<(), String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("文件夹不存在: {path}"));
    }
    if !["folder", "qq"].contains(&kind.as_str()) {
        return Err(format!("未知来源类型: {kind}"));
    }
    guard_scan(&state)?;
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app2 = app.clone();
        let cache2 = cache.clone();
        let run = move |conn: &mut Connection| -> Result<ScanSummary, String> {
            let source_id = create_source(conn, &kind, &label, Some(&path), None, None, None, None)?;
            let progress = |done: usize, total: usize, current: &str| {
                let _ = app2.emit(
                    "scan://progress",
                    ScanStatus { running: true, phase: "扫描中".into(), done, total, current: current.into() },
                );
            };
            Ok(scanner::scan_core(&root, conn, &cache2.join("thumbs"), &progress, Some(source_id)))
        };
        with_scan_conn(app, cache, run);
    });
    Ok(())
}

/// macOS 照片图库导入：优先 osxphotos（真实文件名 + 跳过 iCloud 未下载），失败则回退直扫 originals。
#[tauri::command]
pub fn import_photos_library(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let library = dirs_home().join("Pictures/Photos Library.photoslibrary");
    let originals = library.join("originals");
    if !originals.is_dir() {
        return Err("找不到照片图库（~/Pictures/Photos Library.photoslibrary）".into());
    }
    guard_scan(&state)?;
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app2 = app.clone();
        let cache2 = cache.clone();
        let run = move |conn: &mut Connection| -> Result<ScanSummary, String> {
            let source_id = create_source(
                conn, "photos", "macOS 照片图库",
                originals.to_str(), None, None, None, None,
            )?;
            let progress = |done: usize, total: usize, current: &str| {
                let _ = app2.emit(
                    "scan://progress",
                    ScanStatus { running: true, phase: "导入图库".into(), done, total, current: current.into() },
                );
            };

            // 优先 osxphotos：真实文件名 + 精确排除 iCloud 未下载
            if let Some(bin) = find_osxphotos() {
                progress(0, 0, "正在读取照片图库（osxphotos）");
                match query_osxphotos(&bin, &progress) {
                    Ok(files) if !files.is_empty() => {
                        let summary = scanner::ingest_paths(conn, files, &cache2.join("thumbs"), &progress, Some(source_id));
                        touch_source(conn, source_id);
                        return Ok(summary);
                    }
                    _ => progress(0, 0, "osxphotos 查询失败，回退直扫模式"),
                }
            } else {
                progress(0, 0, "未安装 osxphotos，使用直扫模式（建议安装以获得更准的文件名）");
            }
            let summary = scanner::scan_core(&originals, conn, &cache2.join("thumbs"), &progress, Some(source_id));
            touch_source(conn, source_id);
            Ok(summary)
        };
        with_scan_conn(app, cache, run);
    });
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebdavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub remote_path: String,
}

#[tauri::command]
pub fn import_webdav(app: AppHandle, state: State<'_, AppState>, config: WebdavConfig) -> Result<(), String> {
    guard_scan(&state)?;
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app2 = app.clone();
        let cache2 = cache.clone();
    let run = move |conn: &mut Connection| -> Result<ScanSummary, String> {
            let client = webdav::DavClient::new(&config.url, &config.username, &config.password)?;
            client.test()?;
            let label = format!("网盘 {}", url_host(&config.url));
            let remote = config.remote_path.trim_matches('/').to_string();
            let remote_norm = if remote.is_empty() { String::new() } else { format!("{remote}/") };
            let source_id = create_source(
                conn, "webdav", &label, None, Some(&config.url), Some(&config.username),
                Some(&remote_norm), None,
            )?;
            let staging = cache2.join(format!("webdav_{source_id}"));
            conn.execute(
                "UPDATE sources SET staging_path = ?1 WHERE id = ?2",
                rusqlite::params![staging.to_string_lossy(), source_id],
            )
            .map_err(|e| e.to_string())?;
            // 密码只进系统钥匙串
            let kr = keyring::Entry::new("wanflow-webdav", &format!("source-{source_id}"))
                .map_err(|e| e.to_string())?;
            kr.set_password(&config.password).map_err(|e| e.to_string())?;

            let progress = |done: usize, total: usize, current: &str| {
                let _ = app2.emit(
                    "scan://progress",
                    ScanStatus { running: true, phase: "发现文件".into(), done, total, current: current.into() },
                );
            };
            let entries = client.list_recursive(&remote_norm, &progress)?;
            let files: Vec<&webdav::DavEntry> = entries
                .iter()
                .filter(|e| !e.is_dir)
                .filter(|e| {
                    let ext = e.rel.rsplit('.').next().unwrap_or("").to_lowercase();
                    scanner::IMG_EXTS.contains(&ext.as_str()) || scanner::VID_EXTS.contains(&ext.as_str())
                })
                .collect();
            let total = files.len();
            let mut downloaded = 0usize;
            for (i, e) in files.iter().enumerate() {
                let dest = staging.join(&e.rel);
                // 增量：本地已有同大小且同 mtime 的文件则跳过
                let fresh = std::fs::metadata(&dest).map(|m| {
                    let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64);
                    m.len() == e.size.unwrap_or(0) && mtime == e.modified
                }).unwrap_or(false);
                if !fresh {
                    client.download(&e.rel, &dest, e.modified).map_err(|err| format!("{err}"))?;
                    downloaded += 1;
                }
                let _ = app2.emit(
                    "scan://progress",
                    ScanStatus {
                        running: true,
                        phase: "下载中".into(),
                        done: i + 1,
                        total,
                        current: e.rel.clone(),
                    },
                );
            }
            let _ = downloaded;
            let summary = scanner::scan_core(&staging, conn, &cache2.join("thumbs"), &|done: usize, total: usize, current: &str| {
                let _ = app2.emit(
                    "scan://progress",
                    ScanStatus { running: true, phase: "扫描中".into(), done, total, current: current.into() },
                );
            }, Some(source_id));
            touch_source(conn, source_id);
            Ok(summary)
        };
        with_scan_conn(app, cache, run);
    });
    Ok(())
}

/// 重新扫描某个来源（增量）
#[tauri::command]
pub fn rescan_source(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let (kind, root, url, username, remote, staging): (String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>) = {
        let conn = lock(&state.db);
        conn.query_row(
            "SELECT kind, root_path, url, username, remote_path, staging_path FROM sources WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .map_err(|_| "来源不存在".to_string())?
    };
    guard_scan(&state)?;
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app2 = app.clone();
        let cache2 = cache.clone();
    let run = move |conn: &mut Connection| -> Result<ScanSummary, String> {
            let progress = |done: usize, total: usize, current: &str| {
                let _ = app2.emit(
                    "scan://progress",
                    ScanStatus { running: true, phase: "同步中".into(), done, total, current: current.into() },
                );
            };
            let summary = match kind.as_str() {
                "folder" | "qq" => {
                    let path = root.ok_or("来源缺少路径")?;
                    scanner::scan_core(Path::new(&path), conn, &cache2.join("thumbs"), &progress, Some(id))
                }
                "photos" => {
                    let originals = dirs_home().join("Pictures/Photos Library.photoslibrary/originals");
                    if let Some(bin) = find_osxphotos() {
                        if let Ok(files) = query_osxphotos(&bin, &progress) {
                            scanner::ingest_paths(conn, files, &cache2.join("thumbs"), &progress, Some(id))
                        } else {
                            scanner::scan_core(&originals, conn, &cache2.join("thumbs"), &progress, Some(id))
                        }
                    } else {
                        scanner::scan_core(&originals, conn, &cache2.join("thumbs"), &progress, Some(id))
                    }
                }
                "webdav" => {
                    let url = url.ok_or("来源缺少地址")?;
                    let username = username.ok_or("来源缺少用户名")?;
                    let staging = staging.ok_or("来源缺少暂存目录")?;
                    let kr = keyring::Entry::new("wanflow-webdav", &format!("source-{id}"))
                        .map_err(|e| e.to_string())?;
                    let password = kr.get_password().map_err(|_| "钥匙串里找不到密码，请删除来源后重新导入".to_string())?;
                    let client = webdav::DavClient::new(&url, &username, &password)?;
                    let remote_norm = remote.unwrap_or_default();
                    let entries = client.list_recursive(&remote_norm, &progress)?;
                    let mut downloaded = 0usize;
                    for (i, e) in entries.iter().filter(|e| !e.is_dir).enumerate() {
                        let ext = e.rel.rsplit('.').next().unwrap_or("").to_lowercase();
                        if !(scanner::IMG_EXTS.contains(&ext.as_str()) || scanner::VID_EXTS.contains(&ext.as_str())) {
                            continue;
                        }
                        let dest = Path::new(&staging).join(&e.rel);
                        let fresh = std::fs::metadata(&dest).map(|m| {
                            let mtime = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64);
                            m.len() == e.size.unwrap_or(0) && mtime == e.modified
                        }).unwrap_or(false);
                        if !fresh {
                            client.download(&e.rel, &dest, e.modified)?;
                            downloaded += 1;
                        }
                        let _ = app2.emit(
                            "scan://progress",
                            ScanStatus {
                                running: true,
                                phase: format!("同步中（新下载 {downloaded}）"),
                                done: i + 1,
                                total: entries.len(),
                                current: e.rel.clone(),
                            },
                        );
                    }
                    scanner::scan_core(Path::new(&staging), conn, &cache2.join("thumbs"), &progress, Some(id))
                }
                other => return Err(format!("未知来源类型: {other}")),
            };
            touch_source(conn, id);
            Ok(summary)
        };
        with_scan_conn(app, cache, run);
    });
    Ok(())
}

/// 解除来源关联（文件与决策保留，仅把 media.source_id 置空）
#[tauri::command]
pub fn unlink_source(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = lock(&state.db);
    conn.execute("UPDATE media SET source_id = NULL WHERE source_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM sources WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceItem {
    pub id: i64,
    pub kind: String,
    pub label: String,
    pub root_path: Option<String>,
    pub url: Option<String>,
    pub last_scanned: Option<i64>,
    pub media_count: i64,
}

#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> Result<Vec<SourceItem>, String> {
    let conn = lock(&state.db);
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.kind, s.label, s.root_path, s.url, s.last_scanned,
                    (SELECT COUNT(*) FROM media m WHERE m.source_id = s.id)
             FROM sources s ORDER BY s.id DESC",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(SourceItem {
                id: r.get(0)?,
                kind: r.get(1)?,
                label: r.get(2)?,
                root_path: r.get(3)?,
                url: r.get(4)?,
                last_scanned: r.get(5)?,
                media_count: r.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(items)
}

// —— 导入流程辅助 ——

/// 后台线程统一入口：独立连接跑任务，结束时收尾发事件
fn with_scan_conn(
    app: AppHandle,
    cache: PathBuf,
    run: impl FnOnce(&mut Connection) -> Result<ScanSummary, String> + Send + 'static,
) {
    let mut conn = match db::init(&cache.join("wanflow.db")) {
        Ok(c) => c,
        Err(e) => {
            finish_scan(&app, ScanSummary::default(), format!("数据库打开失败: {e}"));
            return;
        }
    };
    match run(&mut conn) {
        Ok(summary) => finish_scan(&app, summary, String::new()),
        Err(e) => finish_scan(&app, ScanSummary::default(), e),
    }
}

fn touch_source(conn: &Connection, id: i64) {
    let _ = conn.execute(
        "UPDATE sources SET last_scanned = ?1 WHERE id = ?2",
        rusqlite::params![db::now_secs(), id],
    );
}

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

pub fn find_osxphotos() -> Option<PathBuf> {
    let venv = dirs_home().join(".wanflow/osxphotos-venv/bin/osxphotos");
    if venv.is_file() {
        return Some(venv);
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let p = dir.join("osxphotos");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// 跑 osxphotos query --json，返回存在的文件列表（iCloud 未下载的自动跳过）
pub fn query_osxphotos(
    bin: &Path,
    progress: &dyn Fn(usize, usize, &str),
) -> Result<Vec<scanner::FileRow>, String> {
    let output = std::process::Command::new(bin)
        .args(["query", "--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("启动 osxphotos 失败: {e}"))?
        .wait_with_output()
        .map_err(|e| format!("读取 osxphotos 输出失败: {e}"))?;
    if !output.status.success() {
        return Err("osxphotos 查询失败".into());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("解析 osxphotos 输出失败: {e}"))?;
    let items = json.as_array().ok_or("osxphotos 输出格式异常")?;
    let mut files = Vec::new();
    let mut in_cloud = 0usize;
    for item in items {
        let Some(path) = item.get("path").and_then(|p| p.as_str()) else {
            in_cloud += 1;
            continue;
        };
        let time_hint = item
            .get("date")
            .and_then(|d| d.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.timestamp());
        if let Some(mut row) = scanner::file_row(PathBuf::from(path)) {
            row.time_hint = time_hint;
            files.push(row);
        } else {
            in_cloud += 1;
        }
    }
    if in_cloud > 0 {
        progress(0, 0, &format!("有 {in_cloud} 张在 iCloud 云端未下载，已跳过"));
    }
    Ok(files)
}

fn url_host(u: &str) -> String {
    u.splitn(2, "://")
        .nth(1)
        .unwrap_or(u)
        .split('/')
        .next()
        .unwrap_or(u)
        .to_string()
}

// ============ 查询 ============

fn finish_scan(app: &AppHandle, summary: ScanSummary, error: String) {
    let state = app.state::<AppState>();
    {
        let mut s = lock(&state.scan);
        *s = ScanStatus { running: false, phase: if error.is_empty() { "完成".into() } else { error.clone() }, ..Default::default() };
    }
    let _ = app.emit("scan://done", &summary);
}

#[tauri::command]
pub fn get_scan_status(state: State<'_, AppState>) -> ScanStatus {
    lock(&state.scan).clone()
}

// ============ 查询 ============

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub decision: Option<String>,
    pub sort: Option<String>,
    pub dir: Option<String>,
    pub dup_only: Option<bool>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub kind: String,
    pub size_bytes: i64,
    pub thumb: Option<String>,
    pub preview: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub dup_group: Option<i64>,
    pub decision: Option<String>,
    pub stars: Option<i64>,
    pub eff_time: i64,
    pub time_source: String,
    pub time_confidence: i64,
    pub quality: Option<f64>,
    pub sharpness: Option<f64>,
    pub exposure: Option<f64>,
    pub burst_group: Option<i64>,
    pub burst_best: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    pub items: Vec<MediaItem>,
    pub total: i64,
}

#[tauri::command]
pub fn list_media(state: State<'_, AppState>, query: ListQuery) -> Result<ListResult, String> {
    let conn = lock(&state.db);
    list_media_conn(&conn, &query).map_err(|e| e.to_string())
}

fn list_media_conn(conn: &Connection, query: &ListQuery) -> rusqlite::Result<ListResult> {
    let mut where_clauses = vec!["1=1".to_string()];
    match query.decision.as_deref() {
        Some("unreviewed") => where_clauses.push("vd.decision IS NULL".into()),
        Some(d @ ("keep" | "reject" | "pending")) => {
            where_clauses.push(format!("vd.decision = '{d}'"));
        }
        _ => {}
    }
    if query.dup_only.unwrap_or(false) {
        where_clauses.push("m.dup_group IS NOT NULL".into());
    }
    let where_sql = where_clauses.join(" AND ");

    let order = match query.sort.as_deref().unwrap_or("added") {
        "time" => "eff",
        "name" => "LOWER(m.filename)",
        "size" => "m.size_bytes",
        "stars" => "(vs.stars IS NULL), vs.stars",
        "quality" => "COALESCE(m.quality, -1)",
        _ => "m.id",
    };
    let dir_sql = if query.dir.as_deref() == Some("asc") { "ASC" } else { "DESC" };

    let base_sql = format!(
        "FROM media m
         {EFF_DECISION}
         {EFF_STARS}
         {EFF_TIME}
         WHERE {where_sql}"
    );

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) {base_sql}"),
        [],
        |r| r.get(0),
    )?;

    let sql = format!(
        "SELECT {MEDIA_COLS}
         {base_sql}
         ORDER BY {order} {dir_sql}, m.id {dir_sql}
         LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(
            rusqlite::params![query.limit.unwrap_or(200), query.offset.unwrap_or(0)],
            map_media_item,
        )?
        .flatten()
        .collect();

    Ok(ListResult { items, total })
}

#[tauri::command]
pub fn dup_members(state: State<'_, AppState>, group: i64) -> Result<Vec<MediaItem>, String> {
    group_members(&state, group, "dup_group")
}

#[tauri::command]
pub fn burst_members(state: State<'_, AppState>, group: i64) -> Result<Vec<MediaItem>, String> {
    group_members(&state, group, "burst_group")
}

fn group_members(state: &State<'_, AppState>, group: i64, col: &str) -> Result<Vec<MediaItem>, String> {
    let conn = lock(&state.db);
    let sql = format!(
        "SELECT {MEDIA_COLS}
         FROM media m
         {EFF_DECISION} {EFF_STARS} {EFF_TIME}
         WHERE m.{col} = ?1
         ORDER BY eff ASC"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([group], map_media_item)
        .map_err(|e| e.to_string())?
        .flatten()
        .collect::<Vec<_>>();
    Ok(items)
}

pub(crate) fn map_media_item(r: &rusqlite::Row<'_>) -> rusqlite::Result<MediaItem> {
    Ok(MediaItem {
        id: r.get(0)?,
        path: r.get(1)?,
        filename: r.get(2)?,
        kind: r.get(3)?,
        size_bytes: r.get(4)?,
        thumb: r.get(5)?,
        preview: r.get(6)?,
        width: r.get(7)?,
        height: r.get(8)?,
        dup_group: r.get(9)?,
        time_source: r.get(10)?,
        time_confidence: r.get(11)?,
        eff_time: r.get(12)?,
        decision: r.get(13)?,
        stars: r.get(14)?,
        quality: r.get(15)?,
        sharpness: r.get(16)?,
        exposure: r.get(17)?,
        burst_group: r.get(18)?,
        burst_best: r.get(19)?,
    })
}

/// 媒体查询的公共 SELECT 列（含 M2 质量/连拍字段）
pub(crate) const MEDIA_COLS: &str =
    "m.id, m.path, m.filename, m.kind, m.size_bytes, m.thumb, m.preview, m.width, m.height,
     m.dup_group, m.time_source, m.time_confidence,
     COALESCE(ot.eff, m.base_time) AS eff, vd.decision, vs.stars,
     m.quality, m.sharpness, m.exposure, m.burst_group, m.burst_best";

// ============ 决策（可撤销事件） ============

#[tauri::command]
pub fn set_decision(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    decision: Option<String>,
    stars: Option<i64>,
) -> Result<String, String> {
    if ids.is_empty() {
        return Err("没有选中任何照片".into());
    }
    if decision.is_none() && stars.is_none() {
        return Err("没有任何操作".into());
    }
    if let Some(d) = &decision {
        if !["keep", "reject", "pending"].contains(&d.as_str()) {
            return Err(format!("未知决定: {d}"));
        }
    }
    if let Some(s) = stars {
        if !(0..=5).contains(&s) {
            return Err("星级范围 0-5".into());
        }
    }
    let mut parts: Vec<String> = Vec::new();
    match decision.as_deref() {
        Some("keep") => parts.push("保留".into()),
        Some("reject") => parts.push("淘汰".into()),
        Some("pending") => parts.push("待定".into()),
        _ => {}
    }
    if let Some(s) = stars {
        parts.push(format!("{s} 星"));
    }
    let label = format!("{} 张：{}", ids.len(), parts.join(" + "));

    let mut conn = lock(&state.db);
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO actions(kind, label, created_at) VALUES ('decide', ?1, ?2)",
        rusqlite::params![label, db::now_secs()],
    )
    .map_err(|e| e.to_string())?;
    let action_id = tx.last_insert_rowid();
    {
        let mut stmt = tx
            .prepare("INSERT INTO verdicts(action_id, media_id, decision, stars) VALUES (?1, ?2, ?3, ?4)")
            .map_err(|e| e.to_string())?;
        for id in &ids {
            stmt.execute(rusqlite::params![action_id, id, decision, stars])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(label)
}

#[tauri::command]
pub fn undo_last(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let conn = lock(&state.db);
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, label FROM actions WHERE undone = 0 ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((id, label)) = row {
        conn.execute("UPDATE actions SET undone = 1 WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(Some(label))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn redo_last(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let conn = lock(&state.db);
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, label FROM actions
             WHERE undone = 1 AND id > COALESCE((SELECT MAX(id) FROM actions WHERE undone = 0), 0)
             ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if let Some((id, label)) = row {
        conn.execute("UPDATE actions SET undone = 0 WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(Some(label))
    } else {
        Ok(None)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub label: String,
    pub kind: String,
    pub undone: bool,
    pub created_at: i64,
}

#[tauri::command]
pub fn history(state: State<'_, AppState>, limit: Option<i64>) -> Result<Vec<HistoryEntry>, String> {
    let conn = lock(&state.db);
    let mut stmt = conn
        .prepare(
            "SELECT id, label, kind, undone, created_at FROM actions
             ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit.unwrap_or(50)], |r| {
            Ok(HistoryEntry {
                id: r.get(0)?,
                label: r.get(1)?,
                kind: r.get(2)?,
                undone: r.get(3)?,
                created_at: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(rows)
}

// ============ 批量时间修正（dry-run → 执行） ============

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchTimeSpec {
    pub ids: Vec<i64>,
    /// offset | reparse | anchor
    pub mode: String,
    pub hours: Option<i64>,
    pub minutes: Option<i64>,
    pub anchor_media: Option<i64>,
    pub anchor_new_time: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimeDiffRow {
    pub id: i64,
    pub filename: String,
    pub old_time: i64,
    pub old_source: String,
    pub new_time: Option<i64>,
    pub changed: bool,
    pub note: Option<String>,
}

fn compute_time_diff(conn: &Connection, spec: &BatchTimeSpec) -> Result<Vec<TimeDiffRow>, String> {
    // 当前有效时间 + 来源
    let mut rows: Vec<(i64, String, String, i64)> = Vec::new(); // id, filename, source, eff
    {
        let sql = format!(
            "SELECT m.id, m.filename, m.time_source, COALESCE(ot.eff, m.base_time) AS eff
             FROM media m {EFF_TIME} WHERE m.id IN ({}) ORDER BY eff ASC",
            spec.ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let it = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .map_err(|e| e.to_string())?;
        for r in it.flatten() {
            rows.push(r);
        }
    }

    let anchor_delta: Option<i64> = if spec.mode == "anchor" {
        let am = spec.anchor_media.ok_or("锚点照片缺失")?;
        let an = spec.anchor_new_time.ok_or("锚点目标时间缺失")?;
        let cur: i64 = {
            let sql = format!(
                "SELECT COALESCE(ot.eff, m.base_time) FROM media m {EFF_TIME} WHERE m.id = ?1"
            );
            conn.query_row(&sql, [am], |r| r.get(0))
                .map_err(|_| "找不到锚点照片".to_string())?
        };
        Some(an - cur)
    } else {
        None
    };

    let mut out = Vec::with_capacity(rows.len());
    for (id, filename, source, old) in rows {
        let (new, note): (Option<i64>, Option<String>) = match spec.mode.as_str() {
            "offset" => {
                let delta = spec.hours.unwrap_or(0) * 3600 + spec.minutes.unwrap_or(0) * 60;
                (Some(old + delta), None)
            }
            "reparse" => match timefix::filename_time(&filename) {
                Some(t) => (Some(t.secs), Some(format!("从文件名解析（{}）", t.source))),
                None => (None, Some("文件名中没有可用时间".into())),
            },
            "anchor" => {
                let d = anchor_delta.unwrap_or(0);
                (Some(old + d), None)
            }
            other => return Err(format!("未知模式: {other}")),
        };
        let changed = new.map(|n| n != old).unwrap_or(false);
        out.push(TimeDiffRow {
            id,
            filename,
            old_time: old,
            old_source: source,
            new_time: new,
            changed,
            note,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn batch_time_preview(
    state: State<'_, AppState>,
    spec: BatchTimeSpec,
) -> Result<Vec<TimeDiffRow>, String> {
    if spec.ids.is_empty() {
        return Err("没有选中任何照片".into());
    }
    let conn = lock(&state.db);
    compute_time_diff(&conn, &spec)
}

#[tauri::command]
pub fn batch_time_apply(
    state: State<'_, AppState>,
    spec: BatchTimeSpec,
) -> Result<String, String> {
    let mut conn = lock(&state.db);
    let diffs = compute_time_diff(&conn, &spec)?;
    let changed: Vec<&TimeDiffRow> = diffs.iter().filter(|d| d.changed).collect();
    if changed.is_empty() {
        return Err("没有任何时间会发生变化".into());
    }
    let label = match spec.mode.as_str() {
        "offset" => format!("批量偏移 {} 张拍摄时间", changed.len()),
        "reparse" => format!("从文件名重解析 {} 张拍摄时间", changed.len()),
        "anchor" => format!("以锚点对齐 {} 张拍摄时间", changed.len()),
        _ => unreachable!(),
    };
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO actions(kind, label, created_at) VALUES ('batch_time', ?1, ?2)",
        rusqlite::params![label, db::now_secs()],
    )
    .map_err(|e| e.to_string())?;
    let action_id = tx.last_insert_rowid();
    {
        let mut stmt = tx
            .prepare("INSERT INTO time_overrides(action_id, media_id, new_time, note) VALUES (?1, ?2, ?3, ?4)")
            .map_err(|e| e.to_string())?;
        for d in &changed {
            stmt.execute(rusqlite::params![
                action_id,
                d.id,
                d.new_time.unwrap_or(d.old_time),
                d.note
            ])
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(label)
}

// ============ 统计 ============

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total: i64,
    pub images: i64,
    pub videos: i64,
    pub bytes: i64,
    pub keeps: i64,
    pub rejects: i64,
    pub pendings: i64,
    pub unreviewed: i64,
    pub dup_groups: i64,
    pub last_scan: i64,
}

#[tauri::command]
pub fn stats(state: State<'_, AppState>) -> Result<Stats, String> {
    let conn = lock(&state.db);
    let sql = format!(
        "SELECT COUNT(*),
            COALESCE(SUM(m.kind='image'),0), COALESCE(SUM(m.kind='video'),0),
            COALESCE(SUM(m.size_bytes),0),
            COALESCE(SUM(vd.decision='keep'),0), COALESCE(SUM(vd.decision='reject'),0),
            COALESCE(SUM(vd.decision='pending'),0), COALESCE(SUM(vd.decision IS NULL),0),
            (SELECT COUNT(DISTINCT dup_group) FROM media WHERE dup_group IS NOT NULL),
            (SELECT COALESCE(MAX(scanned_at),0) FROM media)
         FROM media m {EFF_DECISION}"
    );
    conn.query_row(&sql, [], |r| {
        Ok(Stats {
            total: r.get(0)?,
            images: r.get(1)?,
            videos: r.get(2)?,
            bytes: r.get(3)?,
            keeps: r.get(4)?,
            rejects: r.get(5)?,
            pendings: r.get(6)?,
            unreviewed: r.get(7)?,
            dup_groups: r.get(8)?,
            last_scan: r.get(9)?,
        })
    })
    .map_err(|e| e.to_string())
}

// ============ M2：质量分析 + 智能初筛建议 ============

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeSummary {
    pub scored: usize,
    pub bursts: usize,
    pub suggestions: usize,
}

#[tauri::command]
pub fn analyze_all(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut a = lock(&state.analyze);
        if a.running {
            return Err("已有分析在进行中".into());
        }
        *a = ScanStatus { running: true, phase: "准备中".into(), ..Default::default() };
    }
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = match db::init(&cache.join("wanflow.db")) {
            Ok(c) => c,
            Err(e) => {
                finish_analyze(&app, AnalyzeSummary::default(), format!("数据库打开失败: {e}"));
                return;
            }
        };
        match crate::analyze::run(&mut conn, &|done, total, current| {
            let _ = app.emit(
                "analyze://progress",
                ScanStatus {
                    running: true,
                    phase: "分析中".into(),
                    done,
                    total,
                    current: current.into(),
                },
            );
        }) {
            Ok(s) => finish_analyze(
                &app,
                AnalyzeSummary { scored: s.scored, bursts: s.bursts, suggestions: s.suggestions },
                String::new(),
            ),
            Err(e) => finish_analyze(&app, AnalyzeSummary::default(), e),
        }
    });
    Ok(())
}

fn finish_analyze(app: &AppHandle, summary: AnalyzeSummary, error: String) {
    let state = app.state::<AppState>();
    {
        let mut a = lock(&state.analyze);
        *a = ScanStatus {
            running: false,
            phase: if error.is_empty() { "完成".into() } else { error.clone() },
            ..Default::default()
        };
    }
    if !error.is_empty() {
        let _ = app.emit("analyze://error", error);
    }
    let _ = app.emit("analyze://done", &summary);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionItem {
    pub id: i64,
    pub media_id: i64,
    pub rule: String,
    pub reason: String,
    pub proposed: String,
    pub filename: String,
    pub thumb: Option<String>,
    pub quality: Option<f64>,
}

#[tauri::command]
pub fn get_suggestions(state: State<'_, AppState>) -> Result<Vec<SuggestionItem>, String> {
    let conn = lock(&state.db);
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.media_id, s.rule, s.reason, s.proposed, m.filename, m.thumb, m.quality
             FROM suggestions s JOIN media m ON m.id = s.media_id
             WHERE s.status = 'pending'
             ORDER BY s.rule, COALESCE(m.quality, -1) ASC",
        )
        .map_err(|e| e.to_string())?;
    let items = stmt
        .query_map([], |r| {
            Ok(SuggestionItem {
                id: r.get(0)?,
                media_id: r.get(1)?,
                rule: r.get(2)?,
                reason: r.get(3)?,
                proposed: r.get(4)?,
                filename: r.get(5)?,
                thumb: r.get(6)?,
                quality: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    Ok(items)
}

/// 应用 = 写入 reject 决策（一个 action，可整体撤销）；忽略 = 永久静默该 media×rule
#[tauri::command]
pub fn resolve_suggestions(state: State<'_, AppState>, ids: Vec<i64>, apply: bool) -> Result<String, String> {
    if ids.is_empty() {
        return Err("没有选中任何建议".into());
    }
    let mut conn = lock(&state.db);
    let ids_json = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
    let rows: Vec<(i64, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, media_id FROM suggestions
                 WHERE status = 'pending' AND id IN (SELECT value FROM json_each(?1))",
            )
            .map_err(|e| e.to_string())?;
        let mapped = stmt
            .query_map([&ids_json], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        mapped.flatten().collect()
    };
    if rows.is_empty() {
        return Err("建议已失效".into());
    }

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    if apply {
        let label = format!("智能初筛：淘汰 {} 张", rows.len());
        tx.execute(
            "INSERT INTO actions(kind, label, created_at) VALUES ('suggestion_apply', ?1, ?2)",
            rusqlite::params![label, db::now_secs()],
        )
        .map_err(|e| e.to_string())?;
        let action_id = tx.last_insert_rowid();
        {
            let mut vstmt = tx
                .prepare("INSERT INTO verdicts(action_id, media_id, decision, stars) VALUES (?1, ?2, 'reject', NULL)")
                .map_err(|e| e.to_string())?;
            for (_, media_id) in &rows {
                vstmt.execute(rusqlite::params![action_id, media_id]).map_err(|e| e.to_string())?;
            }
            let mut ustmt = tx
                .prepare("UPDATE suggestions SET status = 'applied' WHERE id = ?1")
                .map_err(|e| e.to_string())?;
            for (id, _) in &rows {
                ustmt.execute([id]).map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(label)
    } else {
        tx.execute(
            "UPDATE suggestions SET status = 'dismissed' WHERE id IN (SELECT value FROM json_each(?1))",
            rusqlite::params![serde_json::to_string(&ids).unwrap_or_default()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(format!("已忽略 {} 条建议", rows.len()))
    }
}
