//! .wanpack：整库打包 / 恢复（metadata-only 一期）。
//!
//! 目标：把一个「库」打成一个文件，换台机器能原样打开——标签、精选集、时间修正、
//! 撤销历史都在 SQLite 里，随库一起走，不需要额外序列化。
//!
//! 不变量：
//! 1. 原图零写入。打包只读源库；默认不复制媒体字节。
//! 2. 目的路径不得落在任何来源目录内（与 export.rs 同一条规矩）。
//! 3. 容器 = 标准 ZIP（store，不压缩），根目录只有 manifest.json 与 library.sqlite。
//! 4. 恢复先校验 sha256，解到临时文件，校验通过才落到目的路径；失败不留半截库。
//! 5. 绝对路径按 sources[] 重映射；映射不上的保留原值，由调用方提示「离线」。
//!
//! 为什么自己写 ZIP 读写：仓库依赖里没有 zip，现拉 zip crate 会连带降级 21 个
//! 传递依赖（jiff / wasm-bindgen 一族）。store-only 的 ZIP 写读各一百多行，
//! 换来依赖零变动，且产物仍可被系统 unzip 打开，方便排查。

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const WANPACK_VERSION: u32 = 1;
/// 打包时的库表结构版本；恢复时统一交给 db::init 幂等升级
pub const SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_NAME: &str = "manifest.json";
pub const DB_NAME: &str = "library.sqlite";

#[derive(Debug)]
pub enum PackError {
    Io(io::Error),
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    /// 包结构不对（缺文件、版本不认、不是 zip）
    Format(String),
    /// 路径不安全（例如目的目录在来源目录里）
    Unsafe(String),
    /// 目标已存在，需要显式覆盖
    Exists(PathBuf),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::Io(e) => write!(f, "读写失败：{e}"),
            PackError::Sqlite(e) => write!(f, "数据库失败：{e}"),
            PackError::Json(e) => write!(f, "manifest 解析失败：{e}"),
            PackError::Format(s) => write!(f, "包格式不对：{s}"),
            PackError::Unsafe(s) => write!(f, "路径不安全：{s}"),
            PackError::Exists(p) => write!(f, "目标已存在：{}", p.display()),
        }
    }
}

impl std::error::Error for PackError {}

impl From<io::Error> for PackError {
    fn from(e: io::Error) -> Self {
        PackError::Io(e)
    }
}
impl From<rusqlite::Error> for PackError {
    fn from(e: rusqlite::Error) -> Self {
        PackError::Sqlite(e)
    }
}
impl From<serde_json::Error> for PackError {
    fn from(e: serde_json::Error) -> Self {
        PackError::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, PackError>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PackSource {
    pub id: i64,
    pub kind: String,
    pub label: String,
    /// 本地源才有；网盘源为空
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PackFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PackStats {
    pub media: i64,
    pub actions: i64,
    pub tags: i64,
    pub albums: i64,
    pub faces: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub wanpack_version: u32,
    pub app_version: String,
    pub schema_version: u32,
    pub created_at: i64,
    pub stats: PackStats,
    pub sources: Vec<PackSource>,
    pub files: Vec<PackFile>,
}

#[derive(Clone, Debug)]
pub struct PackOptions {
    pub overwrite: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self { overwrite: false }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPlan {
    pub dest: PathBuf,
    pub db_bytes: u64,
    pub stats: PackStats,
    pub sources: Vec<PackSource>,
    /// 非空表示这次打包不能做，逐条是原因
    pub blocked: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackOutcome {
    pub dest: PathBuf,
    pub pack_bytes: u64,
    pub db_bytes: u64,
    pub stats: PackStats,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub dest: PathBuf,
    pub stats: PackStats,
    pub media_rewritten: usize,
    pub sources_rewritten: usize,
    /// 没有命中任何映射的媒体条数（源可能已不在本机）
    pub media_unmapped: usize,
}

/// 一条路径映射：恢复时把 from 前缀换成 to（按目录边界匹配）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Remap {
    pub from: String,
    pub to: String,
}

// ── 读库 ────────────────────────────────────────────────────

fn read_sources(conn: &Connection) -> Result<Vec<PackSource>> {
    let mut stmt = conn.prepare("SELECT id, kind, label, root_path FROM sources ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok(PackSource {
            id: r.get(0)?,
            kind: r.get(1)?,
            label: r.get(2)?,
            root: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn count(conn: &Connection, table: &str) -> Result<i64> {
    // 表名来自本文件内的常量，不接受外部输入
    let sql = format!("SELECT COUNT(*) FROM {table}");
    match conn.query_row(&sql, [], |r| r.get(0)) {
        Ok(n) => Ok(n),
        // 老库还没有这张表（tags / faces 是后加的），按 0 计；恢复时由 db::init 补建
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("no such table") => Ok(0),
        Err(e) => Err(e.into()),
    }
}

fn read_stats(conn: &Connection) -> Result<PackStats> {
    Ok(PackStats {
        media: count(conn, "media")?,
        actions: count(conn, "actions")?,
        tags: count(conn, "tags")?,
        albums: count(conn, "albums")?,
        faces: count(conn, "faces")?,
    })
}

// ── 路径安全 ────────────────────────────────────────────────

fn canonical(p: &Path) -> PathBuf {
    fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// 目标文件还不存在，不能直接 canonicalize：先把父目录规范化再拼文件名。
/// macOS 上 /var 与 /private/var 是同一处，不这样做前缀比较会漏判。
fn canonical_target(dest: &Path) -> PathBuf {
    if let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) {
        if let Ok(p) = fs::canonicalize(parent) {
            return p.join(name);
        }
    }
    dest.to_path_buf()
}

/// 目的路径是否落在任何本地来源目录内（是则返回命中的来源说明）
pub fn dest_inside_sources(dest: &Path, sources: &[PackSource]) -> Option<String> {
    let dest_c = canonical_target(dest);
    for s in sources {
        let Some(root) = &s.root else { continue };
        if root.trim().is_empty() {
            continue;
        }
        let root_c = canonical(Path::new(root));
        if dest_c.starts_with(&root_c) {
            return Some(format!("{}（来源「{}」）", root_c.display(), s.label));
        }
    }
    None
}

// ── 打包 ────────────────────────────────────────────────────

/// 只读预览：这不写任何文件
pub fn plan(db_path: &Path, dest: &Path) -> Result<PackPlan> {
    let conn = open_readonly(db_path)?;
    let stats = read_stats(&conn)?;
    let sources = read_sources(&conn)?;
    let db_bytes = fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    let mut blocked = Vec::new();
    if let Some(hit) = dest_inside_sources(dest, &sources) {
        blocked.push(format!("目的路径在来源目录内：{hit}"));
    }
    if dest.exists() {
        blocked.push(format!("目标已存在：{}", dest.display()));
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            blocked.push(format!("目标目录不存在：{}", parent.display()));
        }
    }
    if dest.extension().and_then(|e| e.to_str()) != Some("wanpack") {
        blocked.push("扩展名建议用 .wanpack".to_string());
    }

    Ok(PackPlan {
        dest: dest.to_path_buf(),
        db_bytes,
        stats,
        sources,
        blocked,
    })
}

fn open_readonly(db_path: &Path) -> Result<Connection> {
    use rusqlite::OpenFlags;
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    Ok(Connection::open_with_flags(db_path, flags)?)
}

/// 打包。dry_run 只返回计划，不落盘。
pub fn pack(
    db_path: &Path,
    dest: &Path,
    app_version: &str,
    dry_run: bool,
    opts: &PackOptions,
) -> Result<PackOutcome> {
    let plan = plan(db_path, dest)?;
    // 「目标已存在」在 overwrite 下不算阻塞
    let hard: Vec<&String> = plan
        .blocked
        .iter()
        .filter(|m| !(opts.overwrite && m.starts_with("目标已存在")))
        .filter(|m| !m.starts_with("扩展名建议"))
        .collect();
    if !hard.is_empty() {
        return Err(PackError::Unsafe(
            hard.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("；"),
        ));
    }
    if dry_run {
        return Ok(PackOutcome {
            dest: dest.to_path_buf(),
            pack_bytes: 0,
            db_bytes: plan.db_bytes,
            stats: plan.stats,
            sha256: String::new(),
        });
    }
    if dest.exists() {
        if !opts.overwrite {
            return Err(PackError::Exists(dest.to_path_buf()));
        }
        fs::remove_file(dest)?;
    }

    let temp_db = dest.with_extension("wanpack.snapshot");
    let _ = fs::remove_file(&temp_db);
    // VACUUM INTO 产出一致性快照（含 WAL 里已提交的内容）
    {
        let src = open_readonly(db_path)?;
        let snapshot = temp_db
            .to_str()
            .ok_or_else(|| PackError::Unsafe("快照路径不是 UTF-8".into()))?;
        // VACUUM INTO 的表达式不能直接绑参，路径里的单引号要自己转义
        let escaped = snapshot.replace('\'', "''");
        src.execute_batch(&format!("VACUUM INTO '{escaped}'"))?;
    }

    let db_bytes = fs::read(&temp_db)?;
    let sha = sha256_hex(&db_bytes);
    let manifest = Manifest {
        wanpack_version: WANPACK_VERSION,
        app_version: app_version.to_string(),
        schema_version: SCHEMA_VERSION,
        created_at: chrono::Utc::now().timestamp(),
        stats: plan.stats,
        sources: plan.sources.clone(),
        files: vec![PackFile {
            path: DB_NAME.to_string(),
            bytes: db_bytes.len() as u64,
            sha256: sha.clone(),
        }],
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;

    let partial = dest.with_extension("wanpack.partial");
    let entries = vec![
        (MANIFEST_NAME.to_string(), manifest_json),
        (DB_NAME.to_string(), db_bytes),
    ];
    zip_write(&partial, &entries)?;
    fs::rename(&partial, dest)?;
    let _ = fs::remove_file(&temp_db);

    Ok(PackOutcome {
        dest: dest.to_path_buf(),
        pack_bytes: fs::metadata(dest).map(|m| m.len()).unwrap_or(0),
        db_bytes: manifest.files[0].bytes,
        stats: manifest.stats,
        sha256: sha,
    })
}

// ── 检查 / 恢复 ─────────────────────────────────────────────

/// 只读打开包，返回 manifest（恢复前的预览）
pub fn inspect(pack_path: &Path) -> Result<Manifest> {
    let entries = zip_read(pack_path)?;
    let raw = entries
        .get(MANIFEST_NAME)
        .ok_or_else(|| PackError::Format(format!("缺少 {MANIFEST_NAME}")))?;
    let manifest: Manifest = serde_json::from_slice(raw)?;
    if manifest.wanpack_version > WANPACK_VERSION {
        return Err(PackError::Format(format!(
            "包版本 {} 比本机支持的 {} 新",
            manifest.wanpack_version, WANPACK_VERSION
        )));
    }
    for f in &manifest.files {
        let bytes = entries
            .get(&f.path)
            .ok_or_else(|| PackError::Format(format!("缺少文件 {}", f.path)))?;
        if bytes.len() as u64 != f.bytes {
            return Err(PackError::Format(format!("{} 字节数不符", f.path)));
        }
    }
    Ok(manifest)
}

/// 恢复：校验 → 解出库 → 重映射路径 → 落到目的路径
pub fn restore(
    pack_path: &Path,
    dest_db: &Path,
    remap: &[Remap],
    dry_run: bool,
    overwrite: bool,
) -> Result<RestoreOutcome> {
    let entries = zip_read(pack_path)?;
    let manifest = inspect(pack_path)?;

    // 校验 sha256（inspect 只核对了字节数）
    let mut db_bytes = Vec::new();
    for f in &manifest.files {
        let bytes = entries
            .get(&f.path)
            .ok_or_else(|| PackError::Format(format!("缺少文件 {}", f.path)))?;
        let got = sha256_hex(bytes);
        if got != f.sha256 {
            return Err(PackError::Format(format!(
                "{} 校验失败（期望 {}，实际 {}）",
                f.path, f.sha256, got
            )));
        }
        if f.path == DB_NAME {
            db_bytes = bytes.clone();
        }
    }
    if db_bytes.is_empty() {
        return Err(PackError::Format(format!("包里没有 {DB_NAME}")));
    }
    if dest_db.exists() && !overwrite {
        return Err(PackError::Exists(dest_db.to_path_buf()));
    }
    if dry_run {
        return Ok(RestoreOutcome {
            dest: dest_db.to_path_buf(),
            stats: manifest.stats,
            media_rewritten: 0,
            sources_rewritten: 0,
            media_unmapped: 0,
        });
    }

    let tmp = dest_db.with_extension("restore.partial");
    if let Some(parent) = dest_db.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&tmp, &db_bytes)?;

    let report = {
        let conn = Connection::open(&tmp)?;
        remap_paths(&conn, remap)?
    };

    if dest_db.exists() {
        fs::remove_file(dest_db)?;
    }
    fs::rename(&tmp, dest_db)?;
    // 统一交给 db::init 幂等升级（老包缺的列会补上）
    let _ = crate::db::init(dest_db)?;

    Ok(RestoreOutcome {
        dest: dest_db.to_path_buf(),
        stats: manifest.stats,
        media_rewritten: report.media_rewritten,
        sources_rewritten: report.sources_rewritten,
        media_unmapped: report.media_unmapped,
    })
}

#[derive(Default, Debug)]
struct RemapReport {
    media_rewritten: usize,
    sources_rewritten: usize,
    media_unmapped: usize,
}

fn remap_one(path: &str, map: &[Remap]) -> Option<String> {
    for m in map {
        if m.from.is_empty() {
            continue;
        }
        let Some(rest) = path.strip_prefix(m.from.as_str()) else {
            continue;
        };
        // 只在目录边界上替换：/a/b 不能吃掉 /a/bc
        if !rest.is_empty() && !rest.starts_with('/') && !rest.starts_with('\\') {
            continue;
        }
        let rest = rest.trim_start_matches(['/', '\\']);
        if rest.is_empty() {
            return Some(m.to.clone());
        }
        return Some(format!("{}/{}", m.to.trim_end_matches('/'), rest));
    }
    None
}

fn remap_paths(conn: &Connection, map: &[Remap]) -> Result<RemapReport> {
    let mut report = RemapReport::default();
    if map.is_empty() {
        report.media_unmapped = count(conn, "media")? as usize;
        return Ok(report);
    }

    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare("SELECT id, root_path FROM sources WHERE root_path IS NOT NULL")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (id, old) in rows {
            if let Some(new) = remap_one(&old, map) {
                tx.execute(
                    "UPDATE sources SET root_path = ?1 WHERE id = ?2",
                    params![new, id],
                )?;
                report.sources_rewritten += 1;
            }
        }
    }
    {
        let mut stmt = tx.prepare("SELECT id, path FROM media")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (id, old) in rows {
            match remap_one(&old, map) {
                Some(new) => {
                    tx.execute("UPDATE media SET path = ?1 WHERE id = ?2", params![new, id])?;
                    report.media_rewritten += 1;
                }
                None => report.media_unmapped += 1,
            }
        }
    }
    tx.commit()?;
    Ok(report)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// ── store-only ZIP（写 / 读）──────────────────────────────────
// 只支持 method=0；够用且产物可被系统 unzip 打开。

fn crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    table
}

fn crc32(data: &[u8]) -> u32 {
    let table = crc32_table();
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn zip_write(dest: &Path, entries: &[(String, Vec<u8>)]) -> Result<()> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    // 固定时间戳 2026-01-01 00:00，避免打包结果不稳定
    let dos_time: u16 = 0;
    let dos_date: u16 = ((2026 - 1980) << 9) | (1 << 5) | 1;

    for (name, data) in entries {
        let name_bytes = name.as_bytes();
        let crc = crc32(data);
        let offset = out.len() as u32;
        let size = data.len() as u32;

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 名
        out.extend_from_slice(&0u16.to_le_bytes()); // method: store
        out.extend_from_slice(&dos_time.to_le_bytes());
        out.extend_from_slice(&dos_date.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // method
        central.extend_from_slice(&dos_time.to_le_bytes());
        central.extend_from_slice(&dos_date.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);
    }

    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());

    fs::write(dest, out)?;
    Ok(())
}

fn rd_u16(buf: &[u8], at: usize) -> Result<u16> {
    let end = at + 2;
    let s = buf.get(at..end).ok_or_else(|| PackError::Format("zip 越界".into()))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn rd_u32(buf: &[u8], at: usize) -> Result<u32> {
    let end = at + 4;
    let s = buf.get(at..end).ok_or_else(|| PackError::Format("zip 越界".into()))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn zip_read(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let buf = fs::read(path)?;
    if buf.len() < 22 {
        return Err(PackError::Format("不是 zip：文件太短".into()));
    }
    // EOCD 在结尾，可能有注释，从后往前找签名
    let mut eocd = None;
    let start = buf.len().saturating_sub(22 + 0xFFFF);
    for i in (start..=buf.len() - 22).rev() {
        if rd_u32(&buf, i)? == 0x0605_4b50 {
            eocd = Some(i);
            break;
        }
    }
    let eocd = eocd.ok_or_else(|| PackError::Format("不是 zip：找不到目录尾".into()))?;
    let total = rd_u16(&buf, eocd + 10)? as usize;
    let cd_offset = rd_u32(&buf, eocd + 16)? as usize;

    let mut out = BTreeMap::new();
    let mut p = cd_offset;
    for _ in 0..total {
        if rd_u32(&buf, p)? != 0x0201_4b50 {
            return Err(PackError::Format("中央目录损坏".into()));
        }
        let method = rd_u16(&buf, p + 10)?;
        let crc = rd_u32(&buf, p + 16)?;
        let csize = rd_u32(&buf, p + 20)? as usize;
        let name_len = rd_u16(&buf, p + 28)? as usize;
        let extra_len = rd_u16(&buf, p + 30)? as usize;
        let comment_len = rd_u16(&buf, p + 32)? as usize;
        let local_offset = rd_u32(&buf, p + 42)? as usize;
        let name_start = p + 46;
        let name = String::from_utf8_lossy(
            buf.get(name_start..name_start + name_len)
                .ok_or_else(|| PackError::Format("zip 名字越界".into()))?,
        )
        .to_string();

        if method != 0 {
            return Err(PackError::Format(format!(
                "{name} 是压缩条目（method={method}），本实现只读 store"
            )));
        }
        // 本地头长度可变（extra 通常为 0，但不假设）
        if rd_u32(&buf, local_offset)? != 0x0403_4b50 {
            return Err(PackError::Format(format!("{name} 本地头损坏")));
        }
        let l_name_len = rd_u16(&buf, local_offset + 26)? as usize;
        let l_extra_len = rd_u16(&buf, local_offset + 28)? as usize;
        let data_start = local_offset + 30 + l_name_len + l_extra_len;
        let data = buf
            .get(data_start..data_start + csize)
            .ok_or_else(|| PackError::Format(format!("{name} 数据越界")))?
            .to_vec();
        if crc32(&data) != crc {
            return Err(PackError::Format(format!("{name} CRC 校验失败")));
        }
        out.insert(name, data);
        p = name_start + name_len + extra_len + comment_len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn seed_db(dir: &Path, root: &str) -> PathBuf {
        let db = dir.join("wanflow.db");
        let conn = crate::db::init(&db).unwrap();
        conn.execute(
            "INSERT INTO sources(kind, label, root_path, created_at) VALUES ('local', '家庭照片', ?1, 0)",
            params![root],
        )
        .unwrap();
        for i in 1..=3 {
            conn.execute(
                "INSERT INTO media(path, filename, ext, kind, size_bytes, mtime, content_hash,
                                   base_time, time_source, scanned_at)
                 VALUES (?1, ?2, 'jpg', 'image', 100, 0, ?3, 0, 'exif', 0)",
                params![
                    format!("{root}/2026/photo{i}.jpg"),
                    format!("photo{i}.jpg"),
                    format!("hash{i}")
                ],
            )
            .unwrap();
        }
        conn.execute("INSERT INTO actions(kind, label, created_at) VALUES ('tag', '打标签', 0)", [])
            .unwrap();
        conn.execute("INSERT INTO tags(name, created_at) VALUES ('外婆家', 0)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO tag_ops(action_id, tag_id, media_id, added) VALUES (1, 1, 1, 1)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO albums(name, created_at) VALUES ('2026 春节', 0)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO album_members(album_id, media_id) VALUES (1, 1)",
            [],
        )
        .unwrap();
        db
    }

    #[test]
    fn pack_then_inspect_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let db = seed_db(dir.path(), root.to_str().unwrap());
        let pack_path = dir.path().join("family.wanpack");

        let out = pack(&db, &pack_path, "9.9.9", false, &PackOptions::default()).unwrap();
        assert_eq!(out.stats.media, 3);
        assert_eq!(out.stats.tags, 1);
        assert_eq!(out.stats.albums, 1);
        assert!(out.pack_bytes > 0);

        let m = inspect(&pack_path).unwrap();
        assert_eq!(m.app_version, "9.9.9");
        assert_eq!(m.stats, out.stats);
        assert_eq!(m.sources.len(), 1);
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.files[0].path, DB_NAME);
    }

    #[test]
    fn restore_remaps_roots_and_keeps_tags() {
        let dir = tempfile::tempdir().unwrap();
        let old_root = dir.path().join("photos");
        fs::create_dir_all(&old_root).unwrap();
        let db = seed_db(dir.path(), old_root.to_str().unwrap());
        let pack_path = dir.path().join("family.wanpack");
        pack(&db, &pack_path, "9.9.9", false, &PackOptions::default()).unwrap();

        let new_root = dir.path().join("nas").join("家庭照片");
        fs::create_dir_all(&new_root).unwrap();
        let dest = dir.path().join("restored").join("wanflow.db");
        let remap = vec![Remap {
            from: old_root.to_str().unwrap().to_string(),
            to: new_root.to_str().unwrap().to_string(),
        }];
        let out = restore(&pack_path, &dest, &remap, false, false).unwrap();
        assert_eq!(out.media_rewritten, 3);
        assert_eq!(out.sources_rewritten, 1);
        assert_eq!(out.media_unmapped, 0);

        let conn = crate::db::init(&dest).unwrap();
        let path: String = conn
            .query_row("SELECT path FROM media WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(path.starts_with(new_root.to_str().unwrap()), "{path}");
        let root: String = conn
            .query_row("SELECT root_path FROM sources WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(root, new_root.to_str().unwrap());
        // 标签与精选集随库过来
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tag_members", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM album_members", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    /// 前端 api.ts 用的是 camelCase，边界结构体必须跟着改
    #[test]
    fn boundary_json_is_camel_case() {
        let plan = PackPlan {
            dest: PathBuf::from("/tmp/a.wanpack"),
            db_bytes: 12,
            stats: PackStats { media: 1, actions: 0, tags: 0, albums: 0, faces: 0 },
            sources: vec![],
            blocked: vec![],
        };
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("dbBytes"), "{json}");
        assert!(!json.contains("db_bytes"), "{json}");

        let manifest = Manifest {
            wanpack_version: 1,
            app_version: "1.0".into(),
            schema_version: 1,
            created_at: 0,
            stats: PackStats::default(),
            sources: vec![],
            files: vec![],
        };
        let mj = serde_json::to_string(&manifest).unwrap();
        assert!(mj.contains("wanpackVersion"), "{mj}");
        assert!(mj.contains("schemaVersion"), "{mj}");

        // 入参也按 camelCase 收（前端传 { from, to }）
        let r: Remap = serde_json::from_str(r#"{"from":"/a","to":"/b"}"#).unwrap();
        assert_eq!(r.from, "/a");
    }

    #[test]
    fn remap_respects_directory_boundary() {
        let map = vec![Remap { from: "/a/b".into(), to: "/new".into() }];
        assert_eq!(remap_one("/a/b/x.jpg", &map).as_deref(), Some("/new/x.jpg"));
        assert_eq!(remap_one("/a/bc/x.jpg", &map), None);
        assert_eq!(remap_one("/a/b", &map).as_deref(), Some("/new"));
    }

    #[test]
    fn pack_refuses_dest_inside_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let db = seed_db(dir.path(), root.to_str().unwrap());
        let dest = root.join("backup.wanpack");
        let err = pack(&db, &dest, "9.9.9", false, &PackOptions::default()).unwrap_err();
        assert!(matches!(err, PackError::Unsafe(_)), "{err:?}");
        assert!(!dest.exists());
    }

    #[test]
    fn restore_refuses_existing_dest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let db = seed_db(dir.path(), root.to_str().unwrap());
        let pack_path = dir.path().join("family.wanpack");
        pack(&db, &pack_path, "9.9.9", false, &PackOptions::default()).unwrap();

        let dest = dir.path().join("taken.db");
        fs::write(&dest, b"x").unwrap();
        let err = restore(&pack_path, &dest, &[], false, false).unwrap_err();
        assert!(matches!(err, PackError::Exists(_)));
        assert_eq!(fs::read(&dest).unwrap(), b"x");
    }

    /// 手工用：生成一个真包，方便用系统 unzip 验互通
    /// cargo test dump_sample_pack -- --ignored --nocapture
    #[test]
    #[ignore]
    fn dump_sample_pack_for_manual_check() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let db = seed_db(dir.path(), root.to_str().unwrap());
        let dest = PathBuf::from("/tmp/wf-sample.wanpack");
        let _ = fs::remove_file(&dest);
        let out = pack(&db, &dest, "9.9.9", false, &PackOptions::default()).unwrap();
        println!("pack={} bytes={}", dest.display(), out.pack_bytes);
    }

    /// 手工用：拿真实库跑一遍打包 + 恢复
    /// WF_LIVE_DB=~/Library/Caches/com.wanflow.app/wanflow.db cargo test live_library_smoke -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_library_smoke() {
        let Some(db) = std::env::var_os("WF_LIVE_DB") else {
            println!("WF_LIVE_DB 未设置，跳过");
            return;
        };
        let db = PathBuf::from(db);
        let dest = PathBuf::from("/tmp/wf-live.wanpack");
        let out = pack(&db, &dest, "live", false, &PackOptions { overwrite: true }).unwrap();
        println!("pack stats={:?} bytes={}", out.stats, out.pack_bytes);

        let m = inspect(&dest).unwrap();
        assert_eq!(m.stats, out.stats);
        let remap: Vec<Remap> = m
            .sources
            .iter()
            .filter_map(|s| s.root.as_ref())
            .map(|r| Remap { from: r.clone(), to: "/tmp/wf-live-root".to_string() })
            .collect();
        let restore_dest = PathBuf::from("/tmp/wf-live-restore/wanflow.db");
        let r = restore(&dest, &restore_dest, &remap, false, true).unwrap();
        println!(
            "restore stats={:?} media_rewritten={} sources_rewritten={} unmapped={}",
            r.stats, r.media_rewritten, r.sources_rewritten, r.media_unmapped
        );
        let conn = crate::db::init(&restore_dest).unwrap();
        let media: i64 = conn.query_row("SELECT COUNT(*) FROM media", [], |x| x.get(0)).unwrap();
        let tags: i64 = conn.query_row("SELECT COUNT(*) FROM tag_members", [], |x| x.get(0)).unwrap();
        println!("restored media={media} tag_members={tags}");
        assert_eq!(media, r.stats.media);
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("photos");
        fs::create_dir_all(&root).unwrap();
        let db = seed_db(dir.path(), root.to_str().unwrap());
        let dest = dir.path().join("preview.wanpack");
        let out = pack(&db, &dest, "9.9.9", true, &PackOptions::default()).unwrap();
        assert_eq!(out.pack_bytes, 0);
        assert!(!dest.exists());
    }
}
