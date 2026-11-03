//! 精选集导出：把「整理结果」落成可交付副本（原图只读）。
//! 一律先 dry-run 预览 → 用户确认 → 执行；只写目标目录，不碰源文件。
//! 元数据：默认原样复制；可剥离 EXIF（含 GPS）；可用 WanFlow 修正后的时间改写 JPEG 的
//! DateTimeOriginal / DateTimeDigitized / DateTime（就地覆盖，不重建 TIFF）。

use crate::db::{EFF_DECISION, EFF_STARS, EFF_TIME};
use crate::state::{AppState, ScanStatus};
use chrono::{Datelike, Local, TimeZone, Timelike};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ============ 参数与结果 ============

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// 目标目录（必须已存在或可创建；不得位于任一来源目录内部由前端保证）
    pub dest_dir: String,
    /// album（精选集）| keeps（所有保留）| selection（当前选中）
    pub scope: String,
    pub album_id: Option<i64>,
    pub ids: Option<Vec<i64>>,
    /// flat | year | month | album
    pub layout: String,
    /// {date} {time} {yyyy} {mm} {dd} {album} {seq} {orig} {stars}
    pub name_template: String,
    /// 剥离 EXIF（含 GPS）；仅 JPEG 可处理，其余格式原样复制并标注
    pub strip_metadata: bool,
    /// 用 WanFlow 修正后的时间改写 EXIF 时间（仅 JPEG，与 strip_metadata 互斥）
    pub rewrite_time: bool,
    /// 是否包含视频
    pub include_videos: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportRow {
    pub id: i64,
    pub src: String,
    pub dest: String,
    pub filename: String,
    pub kind: String,
    pub size_bytes: i64,
    /// copy | skip
    pub action: String,
    pub reason: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExportPlan {
    pub rows: Vec<ExportRow>,
    pub copy_count: usize,
    pub skip_count: usize,
    pub copy_bytes: i64,
    pub dest_dir: String,
    pub notes: Vec<String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub copied: usize,
    pub skipped: usize,
    pub failed: usize,
    pub bytes: i64,
    pub dest_dir: String,
    pub duration_ms: u64,
}

/// 导出清单（写在目标目录根，用于重复运行时不重复导出）
#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    #[serde(default = "manifest_version")]
    version: u32,
    #[serde(default)]
    entries: Vec<ManifestEntry>,
}

fn manifest_version() -> u32 { 1 }

#[derive(Serialize, Deserialize, Clone)]
struct ManifestEntry {
    src: String,
    dest: String,
    size_bytes: i64,
    exported_at: i64,
}

fn manifest_path(dest_root: &Path) -> PathBuf {
    dest_root.join(".wanflow-export.json")
}

fn load_manifest(dest_root: &Path) -> Manifest {
    std::fs::read_to_string(manifest_path(dest_root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_manifest(dest_root: &Path, entries: Vec<ManifestEntry>) {
    let manifest = Manifest { version: 1, entries };
    if let Ok(text) = serde_json::to_string_pretty(&manifest) {
        let _ = std::fs::write(manifest_path(dest_root), text);
    }
}

struct Target {
    id: i64,
    path: String,
    filename: String,
    kind: String,
    size_bytes: i64,
    eff: i64,
    stars: Option<i64>,
}

// ============ 查询目标 ============

fn load_targets(conn: &Connection, opts: &ExportOptions) -> Result<Vec<Target>, String> {
    let mut where_sql: String;
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    match opts.scope.as_str() {
        "album" => {
            let id = opts.album_id.ok_or("缺少精选集 id")?;
            where_sql = "am.album_id = ?1".into();
            params.push(Box::new(id));
        }
        "keeps" => {
            where_sql = "vd.decision = 'keep'".into();
        }
        "selection" => {
            let ids = opts.ids.clone().unwrap_or_default();
            if ids.is_empty() {
                return Err("没有选中任何照片".into());
            }
            let holes = vec!["?"; ids.len()].join(",");
            where_sql = format!("m.id IN ({holes})");
            for id in ids {
                params.push(Box::new(id));
            }
        }
        other => return Err(format!("未知导出范围: {other}")),
    }
    if !opts.include_videos {
        where_sql.push_str(" AND m.kind = 'image'");
    }

    let album_join = if opts.scope == "album" {
        "JOIN album_members am ON am.media_id = m.id"
    } else {
        ""
    };
    let sql = format!(
        "SELECT m.id, m.path, m.filename, m.kind, m.size_bytes, m.quality,
                COALESCE(ot.eff, m.base_time) AS eff, vd.decision, vs.stars
         FROM media m
         {album_join}
         {EFF_DECISION} {EFF_STARS} {EFF_TIME}
         WHERE {where_sql}
         ORDER BY eff ASC, m.id ASC"
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mapped = stmt
        .query_map(rusqlite::params_from_iter(params.iter().map(|b| b.as_ref())), |r| {
            Ok(Target {
                id: r.get(0)?,
                path: r.get(1)?,
                filename: r.get(2)?,
                kind: r.get(3)?,
                size_bytes: r.get(4)?,
                eff: r.get(6)?,
                stars: r.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(mapped.flatten().collect())
}

fn album_name(conn: &Connection, album_id: Option<i64>) -> String {
    let Some(id) = album_id else { return "精选".into() };
    conn.query_row("SELECT name FROM albums WHERE id = ?1", [id], |r| r.get::<_, String>(0))
        .unwrap_or_else(|_| "精选".into())
}

// ============ 纯函数：命名与目录 ============

/// 时间戳按本机时区格式化（DB 里存的是 unix 秒）
fn local_dt(secs: i64) -> chrono::DateTime<Local> {
    Local.timestamp_opt(secs, 0).single().unwrap_or_else(Local::now)
}

/// 去掉路径分隔符与文件系统非法字符，避免模板把文件写到目标目录之外
pub fn sanitize_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    // 先压掉 ".." 连续点（避免生成 .. 形态的名字），再去首尾点与空白
    let dedotted = out.trim().replace("..", "_");
    let trimmed = dedotted.trim().trim_matches('.').trim();
    let limited: String = trimmed.chars().take(80).collect();
    if limited.is_empty() { "未命名".into() } else { limited }
}

fn stem_of(filename: &str) -> String {
    Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| filename.to_string())
}

fn ext_of(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn render_name(template: &str, filename: &str, eff: i64, album: &str, seq: usize, stars: Option<i64>) -> String {
    let dt = local_dt(eff);
    let mut out = expand_seq(template, seq);
    let pairs: [(&str, String); 8] = [
        ("{date}", format!("{:04}{:02}{:02}", dt.year(), dt.month(), dt.day())),
        ("{time}", format!("{:02}{:02}{:02}", dt.hour(), dt.minute(), dt.second())),
        ("{yyyy}", format!("{:04}", dt.year())),
        ("{mm}", format!("{:02}", dt.month())),
        ("{dd}", format!("{:02}", dt.day())),
        ("{album}", album.to_string()),
        ("{orig}", stem_of(filename)),
        ("{stars}", stars.map(|s| s.to_string()).unwrap_or_default()),
    ];
    for (k, v) in pairs {
        out = out.replace(k, &v);
    }
    let base = sanitize_component(&out);
    let ext = ext_of(filename);
    if ext.is_empty() { base } else { format!("{base}.{ext}") }
}

/// 展开 {seq} / {seq:04} / {seq:3} 这类序号占位符（宽度可自定义，默认 4 位）
fn expand_seq(template: &str, seq: usize) -> String {
    let mut out = String::new();
    let mut rest = template;
    loop {
        let Some(i) = rest.find("{seq") else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(close) = tail.find('}') else {
            out.push_str(tail);
            return out;
        };
        let inner = &tail[..close];
        match inner.strip_prefix("{seq:").and_then(|w| w.parse::<usize>().ok()) {
            Some(width) => out.push_str(&format!("{seq:0width$}")),
            None if inner == "{seq" => out.push_str(&format!("{seq:04}")),
            None => out.push_str(inner),
        }
        rest = &tail[close + 1..];
    }
}

/// 目录结构：flat / year（YYYY）/ month（YYYY/MM）/ album（精选集名/）
pub fn layout_dir(dest_root: &Path, layout: &str, eff: i64, album: &str) -> PathBuf {
    let dt = local_dt(eff);
    match layout {
        "year" => dest_root.join(format!("{:04}", dt.year())),
        "month" => dest_root.join(format!("{:04}", dt.year())).join(format!("{:02}", dt.month())),
        "album" => dest_root.join(sanitize_component(album)),
        _ => dest_root.to_path_buf(),
    }
}

/// 目标已存在：同大小视为已导出（可重复运行）；不同大小则加序号
pub fn resolve_dest(wanted: &Path, size: i64) -> (PathBuf, Option<String>) {
    if !wanted.exists() {
        return (wanted.to_path_buf(), None);
    }
    if std::fs::metadata(wanted).map(|m| m.len() as i64 == size).unwrap_or(false) {
        return (wanted.to_path_buf(), Some("目标已存在且大小相同，跳过".into()));
    }
    let parent = wanted.parent().unwrap_or(Path::new("."));
    let stem = wanted.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = wanted.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    for i in 2..1000 {
        let name = if ext.is_empty() { format!("{stem}_{i}") } else { format!("{stem}_{i}.{ext}") };
        let cand = parent.join(name);
        if !cand.exists() {
            return (cand, Some("同名文件不同内容，已加序号".into()));
        }
    }
    (wanted.to_path_buf(), Some("同名文件过多，跳过".into()))
}

// ============ JPEG 元数据处理 ============

fn is_jpeg(path: &Path) -> bool {
    matches!(ext_of(&path.to_string_lossy()).as_str(), "jpg" | "jpeg")
}

/// 去掉 JPEG 的 APP1/Exif 段（含 GPS、相机型号等全部 EXIF）。
/// 返回 None 表示不是 JPEG 或没有 EXIF 段，调用方应原样复制。
pub fn strip_jpeg_exif(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&data[0..2]);
    let mut i = 2usize;
    let mut removed = false;
    while i + 2 <= data.len() {
        if data[i] != 0xFF {
            out.extend_from_slice(&data[i..]);
            break;
        }
        let marker = data[i + 1];
        if marker == 0xD9 || marker == 0xDA || i + 4 > data.len() {
            // EOI / SOS（其后是压缩数据）/ 尾巴不足一段，原样搬完
            out.extend_from_slice(&data[i..]);
            break;
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 || i + 2 + seg_len > data.len() {
            out.extend_from_slice(&data[i..]);
            break;
        }
        let seg_end = i + 2 + seg_len;
        let is_exif = marker == 0xE1
            && data.len() >= i + 10
            && &data[i + 4..i + 10] == b"Exif\x00\x00";
        if is_exif {
            removed = true;
        } else {
            out.extend_from_slice(&data[i..seg_end]);
        }
        i = seg_end;
    }
    if removed { Some(out) } else { None }
}

/// 就地改写 JPEG EXIF 里的三个时间标签（值长度必须正好 20 字节的 ASCII）。
/// 返回是否改动过；解析失败一律返回 false，绝不破坏文件。
pub fn patch_jpeg_exif_time(data: &mut [u8], text: &str) -> bool {
    if text.len() != 19 {
        return false;
    }
    let Some(exif_start) = find_exif_payload(data) else { return false };
    // exif_start 指向 "Exif\0\0" 之后的 TIFF 头
    if exif_start + 8 > data.len() {
        return false;
    }
    let le = match &data[exif_start..exif_start + 2] {
        b"II" => true,
        b"MM" => false,
        _ => return false,
    };
    let read_u16 = |b: &[u8]| if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
    let read_u32 = |b: &[u8]| {
        if le {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        }
    };
    let ifd0 = exif_start + read_u32(&data[exif_start + 4..exif_start + 8]) as usize;
    let mut changed = false;
    let mut targets: Vec<usize> = vec![ifd0];
    // ExifIFD 指针在 IFD0 里
    if ifd0 + 2 <= data.len() {
        let n = read_u16(&data[ifd0..ifd0 + 2]) as usize;
        for k in 0..n {
            let e = ifd0 + 2 + k * 12;
            if e + 12 > data.len() {
                break;
            }
            let tag = read_u16(&data[e..e + 2]);
            if tag == 0x8769 {
                let off = exif_start + read_u32(&data[e + 8..e + 12]) as usize;
                targets.push(off);
            }
        }
    }
    for ifd in targets {
        if ifd + 2 > data.len() {
            continue;
        }
        let n = read_u16(&data[ifd..ifd + 2]) as usize;
        for k in 0..n {
            let e = ifd + 2 + k * 12;
            if e + 12 > data.len() {
                break;
            }
            let tag = read_u16(&data[e..e + 2]);
            if !matches!(tag, 0x0132 | 0x9003 | 0x9004) {
                continue;
            }
            let typ = read_u16(&data[e + 2..e + 4]);
            let count = read_u32(&data[e + 4..e + 8]) as usize;
            if typ != 2 || count != 20 {
                continue;
            }
            let voff = exif_start + read_u32(&data[e + 8..e + 12]) as usize;
            if voff + 20 > data.len() {
                continue;
            }
            if data[voff + 19] != 0 {
                continue;
            }
            data[voff..voff + 19].copy_from_slice(text.as_bytes());
            changed = true;
        }
    }
    changed
}

fn find_exif_payload(data: &[u8]) -> Option<usize> {
    let mut i = 2usize;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        if seg_len < 2 || i + 2 + seg_len > data.len() {
            return None;
        }
        if marker == 0xE1 && data.len() >= i + 10 && &data[i + 4..i + 10] == b"Exif\x00\x00" {
            return Some(i + 10);
        }
        i = i + 2 + seg_len;
    }
    None
}

/// "YYYY:MM:DD HH:MM:SS"（19 字节，EXIF 规范格式）
fn exif_time_text(secs: i64) -> String {
    let dt = local_dt(secs);
    format!("{:04}:{:02}:{:02} {:02}:{:02}:{:02}",
        dt.year(), dt.month(), dt.day(), dt.hour(), dt.minute(), dt.second())
}

/// 目标目录若落在某个已导入来源的根目录里，会把副本混进照片库 —— 直接拒绝。
fn dest_inside_sources(conn: &Connection, dest: &Path) -> Option<String> {
    let dest_abs = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
    let mut stmt = conn
        .prepare("SELECT label, root_path FROM sources WHERE root_path IS NOT NULL AND root_path <> ''")
        .ok()?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .ok()?;
    for (label, root) in rows.flatten() {
        let root_path = PathBuf::from(&root);
        let root_abs = root_path.canonicalize().unwrap_or(root_path);
        if dest_abs.starts_with(&root_abs) {
            return Some(label);
        }
    }
    None
}

/// 在本次计划内找一个没被占用的变体名（_2、_3 …）
fn next_free_variant(wanted: &Path, planned: &std::collections::HashSet<PathBuf>) -> PathBuf {
    let parent = wanted.parent().unwrap_or(Path::new("."));
    let stem = wanted.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = wanted.extension().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    for i in 2..1000 {
        let name = if ext.is_empty() { format!("{stem}_{i}") } else { format!("{stem}_{i}.{ext}") };
        let cand = parent.join(name);
        if !cand.exists() && !planned.contains(&cand) {
            return cand;
        }
    }
    wanted.to_path_buf()
}

// ============ 计划（dry-run） ============

fn build_plan(conn: &Connection, opts: &ExportOptions) -> Result<ExportPlan, String> {
    let dest_root = PathBuf::from(&opts.dest_dir);
    if opts.dest_dir.trim().is_empty() {
        return Err("请选择导出目录".into());
    }
    let album = album_name(conn, opts.album_id);
    if opts.name_template.trim().is_empty() {
        return Err("命名模板不能为空".into());
    }
    if let Some(label) = dest_inside_sources(conn, &dest_root) {
        return Err(format!(
            "导出目录不能放进来源目录「{label}」里——那会把副本混进照片库，换一个目录"
        ));
    }
    let targets = load_targets(conn, opts)?;
    if targets.is_empty() {
        return Err("没有可导出的照片（检查范围/筛选条件）".into());
    }
    let mut rows = Vec::with_capacity(targets.len());
    let mut notes: Vec<String> = Vec::new();
    let mut copy_count = 0usize;
    let mut skip_count = 0usize;
    let mut copy_bytes = 0i64;
    let mut planned: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    // 上次导出过的（同一张原图）直接跳过，保证重复运行不会造出一堆 _2/_3
    let exported: std::collections::HashMap<String, String> = load_manifest(&dest_root)
        .entries
        .into_iter()
        .map(|e| (e.src, e.dest))
        .collect();
    for (idx, t) in targets.iter().enumerate() {
        if let Some(prev) = exported.get(&t.path) {
            if Path::new(prev).exists() {
                rows.push(ExportRow {
                    id: t.id,
                    src: t.path.clone(),
                    dest: prev.clone(),
                    filename: t.filename.clone(),
                    kind: t.kind.clone(),
                    size_bytes: t.size_bytes,
                    action: "skip".into(),
                    reason: Some("上次已导出过，跳过".into()),
                });
                skip_count += 1;
                continue;
            }
        }
        let dir = layout_dir(&dest_root, &opts.layout, t.eff, &album);
        let name = render_name(&opts.name_template, &t.filename, t.eff, &album, idx + 1, t.stars);
        let wanted = dir.join(&name);
        let (mut dest, conflict) = resolve_dest(&wanted, t.size_bytes);
        let exists_ok = conflict.as_deref().map(|r| r.contains("跳过")).unwrap_or(false);
        let mut reason = conflict;
        let mut action = "copy";
        if exists_ok {
            action = "skip";
            skip_count += 1;
        } else {
            // 同一次导出里两个源文件可能撞到同一个目标名（例如模板只用 {album}）
            if planned.contains(&dest) {
                dest = next_free_variant(&dest, &planned);
                reason = Some("与本次导出中其他文件重名，已加序号".into());
            }
            planned.insert(dest.clone());
            if opts.strip_metadata && !is_jpeg(Path::new(&t.path)) {
                let note = "非 JPEG 无法剥离 EXIF，将原样复制".to_string();
                reason = Some(match reason { Some(r) => format!("{r}；{note}"), None => note });
                if !notes.contains(&"部分非 JPEG 文件无法剥离元数据，已原样复制".to_string()) {
                    notes.push("部分非 JPEG 文件无法剥离元数据，已原样复制".into());
                }
            }
            copy_count += 1;
            copy_bytes += t.size_bytes;
        }
        rows.push(ExportRow {
            id: t.id,
            src: t.path.clone(),
            dest: dest.to_string_lossy().to_string(),
            filename: t.filename.clone(),
            kind: t.kind.clone(),
            size_bytes: t.size_bytes,
            action: action.into(),
            reason,
        });
    }
    if opts.strip_metadata && opts.rewrite_time {
        notes.push("「剥离元数据」与「改写时间」互斥，本次未改写时间".into());
    }
    Ok(ExportPlan {
        rows,
        copy_count,
        skip_count,
        copy_bytes,
        dest_dir: opts.dest_dir.clone(),
        notes,
    })
}

// ============ Tauri 命令 ============

#[tauri::command]
pub fn export_preview(state: State<'_, AppState>, opts: ExportOptions) -> Result<ExportPlan, String> {
    let conn = lock(&state.db);
    build_plan(&conn, &opts)
}

#[tauri::command]
pub fn export_status(state: State<'_, AppState>) -> ScanStatus {
    lock(&state.export).clone()
}

#[tauri::command]
pub fn export_apply(app: AppHandle, state: State<'_, AppState>, opts: ExportOptions) -> Result<(), String> {
    {
        let mut e = lock(&state.export);
        if e.running {
            return Err("已有导出在进行中".into());
        }
        *e = ScanStatus { running: true, phase: "准备中".into(), ..Default::default() };
    }
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let conn = match crate::db::init(&cache.join("wanflow.db")) {
            Ok(c) => c,
            Err(e) => {
                finish_export(&app, ExportSummary::default(), format!("数据库打开失败: {e}"));
                return;
            }
        };
        let plan = match build_plan(&conn, &opts) {
            Ok(p) => p,
            Err(e) => {
                finish_export(&app, ExportSummary::default(), e);
                return;
            }
        };
        let mut summary = execute_plan(&conn, &plan, &opts, |done, total, current| {
            let _ = app.emit(
                "export://progress",
                ScanStatus {
                    running: true,
                    phase: "导出中".into(),
                    done,
                    total,
                    current: current.to_string(),
                },
            );
        }, |msg| {
            let _ = app.emit("export://error", msg);
        });
        summary.duration_ms = started.elapsed().as_millis() as u64;
        finish_export(&app, summary, String::new());
    });
    Ok(())
}

/// 真正执行计划：只写目标目录，原图只读。进度/错误通过回调上报。
pub fn execute_plan<P, E>(
    conn: &Connection,
    plan: &ExportPlan,
    opts: &ExportOptions,
    mut on_progress: P,
    mut on_error: E,
) -> ExportSummary
where
    P: FnMut(usize, usize, &str),
    E: FnMut(String),
{
    let total = plan.copy_count;
    let mut summary = ExportSummary { dest_dir: opts.dest_dir.clone(), ..Default::default() };
    let mut done = 0usize;
    let mut fresh: Vec<ManifestEntry> = Vec::new();
    for row in &plan.rows {
        if row.action == "skip" {
            summary.skipped += 1;
            continue;
        }
        let src = PathBuf::from(&row.src);
        let dest = PathBuf::from(&row.dest);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                summary.failed += 1;
                on_error(format!("建目录失败 {}: {e}", parent.display()));
                continue;
            }
        }
        let eff = conn
            .query_row(
                &format!("SELECT COALESCE(ot.eff, m.base_time) FROM media m {EFF_TIME} WHERE m.id = ?1"),
                [row.id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or_else(|_| crate::db::now_secs());
        match copy_one(&src, &dest, eff, opts) {
            Ok(bytes) => {
                summary.copied += 1;
                summary.bytes += bytes;
                fresh.push(ManifestEntry {
                    src: row.src.clone(),
                    dest: row.dest.clone(),
                    size_bytes: bytes,
                    exported_at: crate::db::now_secs(),
                });
            }
            Err(e) => {
                summary.failed += 1;
                on_error(format!("复制失败 {}: {e}", row.filename));
            }
        }
        done += 1;
        if done % 5 == 0 || done == total {
            on_progress(done, total, &row.filename);
        }
    }
    if !fresh.is_empty() {
        let root = PathBuf::from(&opts.dest_dir);
        let mut manifest = load_manifest(&root);
        for entry in fresh {
            manifest.entries.retain(|e| e.src != entry.src);
            manifest.entries.push(entry);
        }
        save_manifest(&root, manifest.entries);
    }
    summary
}

fn copy_one(src: &Path, dest: &Path, eff: i64, opts: &ExportOptions) -> Result<i64, String> {
    let mut data = std::fs::read(src).map_err(|e| e.to_string())?;
    let mut handled = false;
    if is_jpeg(src) {
        if opts.strip_metadata {
            if let Some(stripped) = strip_jpeg_exif(&data) {
                data = stripped;
                handled = true;
            }
        } else if opts.rewrite_time {
            let text = exif_time_text(eff);
            if patch_jpeg_exif_time(&mut data, &text) {
                handled = true;
            }
        }
    }
    let src_len = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    if handled || data.len() as u64 != src_len {
        std::fs::write(dest, &data).map_err(|e| e.to_string())?;
        // 写出的副本按拍摄时间排序更好用
        let mtime = filetime::FileTime::from_unix_time(eff, 0);
        let _ = filetime::set_file_mtime(dest, mtime);
        return Ok(data.len() as i64);
    }
    std::fs::copy(src, dest).map_err(|e| e.to_string())?;
    let mtime = filetime::FileTime::from_unix_time(eff, 0);
    let _ = filetime::set_file_mtime(dest, mtime);
    Ok(std::fs::metadata(dest).map(|m| m.len() as i64).unwrap_or(0))
}

fn finish_export(app: &AppHandle, summary: ExportSummary, error: String) {
    let state = app.state::<AppState>();
    *lock(&state.export) = ScanStatus::default();
    if !error.is_empty() {
        let _ = app.emit("export://error", error);
    }
    let _ = app.emit("export://done", &summary);
}

// ============ 测试 ============

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_renders_placeholders() {
        let name = render_name("{date}_{seq}_{orig}", "IMG_0001.HEIC", 1_696_435_200, "周岁", 7, Some(3));
        assert!(name.starts_with("20231005_0007_IMG_0001"));
        assert!(name.ends_with(".heic"));
    }

    #[test]
    fn template_sanitizes_slashes() {
        let name = render_name("../../etc/{album}", "a.jpg", 0, "宝宝/满月", 1, None);
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
        assert!(!name.starts_with('.'));
        assert!(!name.contains(".."));
    }

    #[test]
    fn layout_paths_group_by_month() {
        let root = Path::new("/tmp/out");
        let p = layout_dir(root, "month", 1_696_435_200, "精选");
        let s = p.to_string_lossy();
        assert!(s.ends_with("/2023/10"), "got {s}");
    }

    #[test]
    fn sanitize_component_falls_back() {
        assert_eq!(sanitize_component("///"), "___");
        assert_eq!(sanitize_component("   "), "未命名");
    }

    #[test]
    fn strip_removes_exif_app1() {
        let mut jpg = vec![0xFF, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xE1]);
        let payload = b"Exif\x00\x00MM\x00*\x00\x00\x00\x08";
        jpg.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        jpg.extend_from_slice(payload);
        jpg.extend_from_slice(&[0xFF, 0xD9]);
        let out = strip_jpeg_exif(&jpg).expect("应该去掉 EXIF");
        assert_eq!(out, vec![0xFF, 0xD8, 0xFF, 0xD9]);
    }

    #[test]
    fn strip_keeps_plain_jpeg() {
        let jpg = vec![0xFF, 0xD8, 0xFF, 0xD9];
        assert!(strip_jpeg_exif(&jpg).is_none());
    }

    /// 构造最小 EXIF：IFD0 带 DateTimeOriginal(0x9003) + ExifIFD 指针，验证就地改写
    #[test]
    fn patch_datetime_in_place() {
        // TIFF 头 (II) + IFD0(2 个 tag) + ExifIFD(1 个 tag) + 值区
        let mut tiff: Vec<u8> = Vec::new();
        tiff.extend_from_slice(b"II\x2a\x00");
        tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 偏移
        // IFD0: 2 个 tag（IFD0 偏移 = 8）
        tiff.extend_from_slice(&2u16.to_le_bytes());
        // tag 0x0132 DateTime, ASCII, count 20, 值在值区偏移 80
        tiff.extend_from_slice(&0x0132u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&20u32.to_le_bytes());
        tiff.extend_from_slice(&80u32.to_le_bytes());
        // tag 0x8769 ExifIFD 指针 -> 偏移 50
        tiff.extend_from_slice(&0x8769u16.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&50u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        // 补齐到 50
        while tiff.len() < 50 {
            tiff.push(0);
        }
        // ExifIFD @50: 1 个 tag (0x9003 -> 偏移 100)
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x9003u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&20u32.to_le_bytes());
        tiff.extend_from_slice(&100u32.to_le_bytes());
        tiff.extend_from_slice(&0u32.to_le_bytes());
        while tiff.len() < 80 {
            tiff.push(0);
        }
        tiff.extend_from_slice(b"2000:01:01 00:00:00\x00");
        while tiff.len() < 100 {
            tiff.push(0);
        }
        tiff.extend_from_slice(b"2000:01:01 00:00:00\x00");
        while tiff.len() < 130 {
            tiff.push(0);
        }
        // 包进 JPEG APP1
        let mut jpg: Vec<u8> = vec![0xFF, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xE1]);
        let payload_len = 2 + 6 + tiff.len();
        jpg.extend_from_slice(&(payload_len as u16).to_be_bytes());
        jpg.extend_from_slice(b"Exif\x00\x00");
        jpg.extend_from_slice(&tiff);
        jpg.extend_from_slice(&[0xFF, 0xD9]);
        let ok = patch_jpeg_exif_time(&mut jpg, "2023:10:05 12:34:56");
        assert!(ok, "应改写成功");
        let text = String::from_utf8_lossy(&jpg);
        assert!(text.contains("2023:10:05 12:34:56"));
        assert!(!text.contains("2000:01:01 00:00:00"));
    }

    #[test]
    fn seq_placeholder_supports_width() {
        assert_eq!(expand_seq("{seq:04}", 7), "0007");
        assert_eq!(expand_seq("{seq:2}", 7), "07");
        assert_eq!(expand_seq("{seq}", 7), "0007");
        let name = render_name("{date}_{seq:03}_{orig}", "a.jpg", 1_696_435_200, "精选", 5, None);
        assert!(name.starts_with("20231005_005_a.jpg"), "got {name}");
    }

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("wanflow_export_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn fake_jpeg() -> Vec<u8> {
        let mut jpg = vec![0xFF, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xE1]);
        let payload = b"Exif\x00\x00MM\x00*\x00\x00\x00\x08";
        jpg.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        jpg.extend_from_slice(payload);
        jpg.extend_from_slice(&[0xFF, 0xD9]);
        jpg
    }

    fn seed_db(root: &std::path::Path, jpg_path: &std::path::Path, size: i64) -> Connection {
        let conn = crate::db::init(&root.join("wanflow.db")).unwrap();
        conn.execute(
            "INSERT INTO sources(kind,label,root_path,created_at) VALUES('folder','源目录',?1,0)",
            [root.join("src").to_string_lossy().to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO media(path,filename,ext,kind,size_bytes,mtime,content_hash,phash,
                base_time,time_source,time_confidence,scanned_at)
             VALUES(?1,'IMG_0001.JPG','JPG','image',?2,0,'hash1','',1_696_435_200,'exif',90,0)",
            rusqlite::params![jpg_path.to_string_lossy().to_string(), size],
        )
        .unwrap();
        conn
    }

    fn selection_opts(dest: &std::path::Path) -> ExportOptions {
        ExportOptions {
            dest_dir: dest.to_string_lossy().to_string(),
            scope: "selection".into(),
            album_id: None,
            ids: Some(vec![1]),
            layout: "month".into(),
            name_template: "{date}_{seq:04}_{orig}".into(),
            strip_metadata: true,
            rewrite_time: false,
            include_videos: false,
        }
    }

    #[test]
    fn plan_and_execute_copies_without_touching_source() {
        let root = tmp_root("run");
        let src_dir = root.join("src");
        let out_dir = root.join("out");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&out_dir).unwrap();
        let jpg = fake_jpeg();
        let jpg_path = src_dir.join("IMG_0001.JPG");
        std::fs::write(&jpg_path, &jpg).unwrap();
        let conn = seed_db(&root, &jpg_path, jpg.len() as i64);

        let opts = selection_opts(&out_dir);
        let plan = build_plan(&conn, &opts).unwrap();
        assert_eq!(plan.copy_count, 1, "应计划复制 1 个: {:?}", plan.notes);
        assert_eq!(plan.skip_count, 0);

        let mut errors: Vec<String> = Vec::new();
        let summary = execute_plan(&conn, &plan, &opts, |_, _, _| {}, |m| errors.push(m));
        assert_eq!(summary.copied, 1, "errors: {errors:?}");
        assert_eq!(summary.failed, 0);

        let dest = std::path::PathBuf::from(&plan.rows[0].dest);
        assert!(dest.exists(), "目标文件应存在: {}", dest.display());
        let copied = std::fs::read(&dest).unwrap();
        assert_eq!(copied, vec![0xFF, 0xD8, 0xFF, 0xD9], "EXIF 应被剥离");
        // 原图一个字节都不许动
        assert_eq!(std::fs::read(&jpg_path).unwrap(), jpg);

        // 再跑一次：同大小目标应判定为跳过，不产生 _2
        let plan2 = build_plan(&conn, &opts).unwrap();
        assert_eq!(plan2.skip_count, 1);
        assert_eq!(plan2.copy_count, 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_dest_inside_source_dir() {
        let root = tmp_root("guard");
        let src_dir = root.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let jpg = fake_jpeg();
        let jpg_path = src_dir.join("IMG_0001.JPG");
        std::fs::write(&jpg_path, &jpg).unwrap();
        let conn = seed_db(&root, &jpg_path, jpg.len() as i64);
        let opts = selection_opts(&src_dir);
        let err = build_plan(&conn, &opts).unwrap_err();
        assert!(err.contains("来源目录"), "got {err}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
