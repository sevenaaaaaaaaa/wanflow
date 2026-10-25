//! 文件夹扫描：遍历 → SHA256 → 增量跳过 → pHash/缩略图 → 时间恢复 → 入库 → 重复分组。
//! 全程只读原文件；一切结果写旁车库。

use crate::{db, phash, thumbs, timefix};
use rayon::prelude::*;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const IMG_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff", "heic", "heif", "avif", "jxl",
];
pub const VID_EXTS: &[&str] = &["mp4", "mov", "m4v", "avi", "mkv", "3gp", "mts", "wmv"];

const PHASH_GROUP_THRESHOLD: u32 = 6;
const PHASH_PAIRWISE_CAP: usize = 40_000;

pub struct FileRow {
    pub path: PathBuf,
    pub filename: String,
    pub ext: String,
    pub is_video: bool,
    pub size: i64,
    pub mtime: i64,
    /// 来源系统自带的权威拍摄时间（如 osxphotos 的 date），EXIF 优先级之上
    pub time_hint: Option<i64>,
}

struct MediaInsert {
    path: String,
    filename: String,
    ext: String,
    kind: &'static str,
    size: i64,
    mtime: i64,
    hash: String,
    phash: String,
    base_time: i64,
    time_source: String,
    time_confidence: i64,
    width: Option<i64>,
    height: Option<i64>,
    thumb: Option<String>,
    preview: Option<String>,
    source_id: Option<i64>,
}

enum Outcome {
    Unchanged,
    Done(Box<MediaInsert>),
    Skipped(#[allow(dead_code)] String),
}

#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    unchanged: usize,
    inserted_or_updated: usize,
    skipped: usize,
}

pub fn scan_core(
    root: &Path,
    conn: &mut Connection,
    thumb_dir: &Path,
    progress: &dyn Fn(usize, usize, &str),
    source_id: Option<i64>,
) -> crate::state::ScanSummary {
    let _ = std::fs::create_dir_all(thumb_dir);

    // 1. 遍历收集
    progress(0, 0, "正在遍历文件夹");
    let files: Vec<FileRow> = collect_files(root);
    ingest_rows(conn, files, thumb_dir, progress, source_id)
}

/// 导入明确的文件列表（照片图库引用式导入等），不走文件夹遍历。
pub fn ingest_paths(
    conn: &mut Connection,
    files: Vec<FileRow>,
    thumb_dir: &Path,
    progress: &dyn Fn(usize, usize, &str),
    source_id: Option<i64>,
) -> crate::state::ScanSummary {
    let _ = std::fs::create_dir_all(thumb_dir);
    ingest_rows(conn, files, thumb_dir, progress, source_id)
}

/// 从 FileRow 构造（供图库导入等外部来源使用）
pub fn file_row(path: PathBuf) -> Option<FileRow> {
    let meta = std::fs::metadata(&path).ok()?;
    let ext = path.extension()?.to_str()?.to_lowercase();
    let is_video = VID_EXTS.contains(&ext.as_str());
    if !is_video && !IMG_EXTS.contains(&ext.as_str()) {
        return None;
    }
    let filename = path.file_name()?.to_string_lossy().into_owned();
    Some(FileRow {
        path,
        filename,
        ext,
        is_video,
        size: meta.len() as i64,
        mtime: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        time_hint: None,
    })
}

fn ingest_rows(
    conn: &mut Connection,
    files: Vec<FileRow>,
    thumb_dir: &Path,
    progress: &dyn Fn(usize, usize, &str),
    source_id: Option<i64>,
) -> crate::state::ScanSummary {
    let total = files.len();
    progress(0, total, &format!("共 {total} 个媒体文件"));

    // 2. 已知索引：路径→(size,mtime) 用于增量；hash→分析缓存 用于跨文件复用
    let mut known_paths: HashMap<String, (i64, i64)> = HashMap::new();
    let mut known_hash: HashMap<String, MediaInsert> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT path, size_bytes, mtime, content_hash, phash, base_time, time_source, time_confidence, width, height, thumb, preview, filename, ext, kind, source_id FROM media")
            .expect("查询 media 失败");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?),
                    MediaInsert {
                        path: r.get(0)?,
                        filename: r.get(12)?,
                        ext: r.get(13)?,
                        kind: if r.get::<_, String>(14)?.as_str() == "video" { "video" } else { "image" },
                        size: r.get(1)?,
                        mtime: r.get(2)?,
                        hash: r.get(3)?,
                        phash: r.get(4)?,
                        base_time: r.get(5)?,
                        time_source: r.get(6)?,
                        time_confidence: r.get(7)?,
                        width: r.get(8)?,
                        height: r.get(9)?,
                        thumb: r.get(10)?,
                        preview: r.get(11)?,
                        source_id: r.get(15)?,
                    },
                ))
            })
            .expect("读取 media 失败");
        for row in rows.flatten() {
            known_paths.insert(row.0.clone(), row.1);
            known_hash.insert(row.2.hash.clone(), row.2);
        }
    }

    // 3. 分块并行处理 + 顺序入库
    let mut counts = Counts::default();
    for chunk in files.chunks(64) {
        let results: Vec<Outcome> = chunk
            .par_iter()
            .map(|f| process_file(f, &known_paths, &known_hash, thumb_dir, source_id))
            .collect();

        let tx = conn.transaction().expect("开启事务失败");
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO media(path, filename, ext, kind, size_bytes, mtime, content_hash, phash,
                        base_time, time_source, time_confidence, width, height, thumb, preview, scanned_at, source_id)
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
                    ON CONFLICT(path) DO UPDATE SET
                        filename=excluded.filename, ext=excluded.ext, kind=excluded.kind,
                        size_bytes=excluded.size_bytes, mtime=excluded.mtime,
                        content_hash=excluded.content_hash, phash=excluded.phash,
                        base_time=excluded.base_time, time_source=excluded.time_source,
                        time_confidence=excluded.time_confidence, width=excluded.width,
                        height=excluded.height, thumb=excluded.thumb, preview=excluded.preview,
                        scanned_at=excluded.scanned_at,
                        source_id=COALESCE(excluded.source_id, media.source_id)",
                )
                .expect("准备 upsert 失败");
            for out in &results {
                match out {
                    Outcome::Unchanged => counts.unchanged += 1,
                    Outcome::Skipped(_) => counts.skipped += 1,
                    Outcome::Done(m) => {
                        counts.inserted_or_updated += 1;
                        let n = stmt
                            .execute(rusqlite::params![
                                m.path,
                                m.filename,
                                m.ext,
                                m.kind,
                                m.size,
                                m.mtime,
                                m.hash,
                                m.phash,
                                m.base_time,
                                m.time_source,
                                m.time_confidence,
                                m.width,
                                m.height,
                                m.thumb,
                                m.preview,
                                db::now_secs(),
                                m.source_id,
                            ])
                            .unwrap_or(0);
                        if n == 0 {
                            counts.inserted_or_updated -= 1;
                            counts.unchanged += 1;
                        }
                    }
                }
            }
        }
        tx.commit().expect("提交事务失败");

        if let Some(last) = chunk.last() {
            progress(counts.total_seen(), total, &last.filename);
        }
    }

    // 4. 重复分组
    progress(0, 0, "正在分组相似照片");
    let dup_groups = rebuild_dup_groups(conn);

    // 5. 库级统计
    let (images, videos) = {
        let mut stmt = conn
            .prepare("SELECT COALESCE(SUM(kind='image'),0), COALESCE(SUM(kind='video'),0) FROM media")
            .unwrap();
        stmt.query_row([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
            .unwrap_or((0, 0))
    };

    crate::state::ScanSummary {
        inserted: counts.inserted_or_updated,
        updated: 0,
        unchanged: counts.unchanged,
        skipped: counts.skipped,
        images: images as usize,
        videos: videos as usize,
        dup_groups: Some(dup_groups),
    }
}

impl Counts {
    fn total_seen(&self) -> usize {
        self.unchanged + self.inserted_or_updated + self.skipped
    }
}

fn collect_files(root: &Path) -> Vec<FileRow> {
    WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_str()?.to_lowercase();
            let is_video = VID_EXTS.contains(&ext.as_str());
            if !is_video && !IMG_EXTS.contains(&ext.as_str()) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let filename = p.file_name()?.to_string_lossy().into_owned();
            Some(FileRow {
                path: p.to_path_buf(),
                filename,
                ext,
                is_video,
                size: meta.len() as i64,
                mtime: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
                time_hint: None,
            })
        })
        .collect()
}

fn process_file(
    f: &FileRow,
    known_paths: &HashMap<String, (i64, i64)>,
    known_hash: &HashMap<String, MediaInsert>,
    thumb_dir: &Path,
    source_id: Option<i64>,
) -> Outcome {
    let path_str = f.path.to_string_lossy().into_owned();
    if known_paths.get(&path_str) == Some(&(f.size, f.mtime)) {
        return Outcome::Unchanged;
    }
    let Some(hash) = sha256_file(&f.path) else {
        return Outcome::Skipped(path_str);
    };
    let t = match f.time_hint {
        Some(secs) => timefix::TimeFound { secs, source: "图库记录时间", confidence: 3 },
        None => timefix::recover(&f.path, &f.filename, f.is_video, f.mtime),
    };

    let (ph, w, h, thumb, preview) = if f.is_video {
        (String::new(), None, None, None, None)
    } else if let Some(cached) = known_hash.get(&hash) {
        (
            cached.phash.clone(),
            cached.width,
            cached.height,
            cached.thumb.clone(),
            cached.preview.clone(),
        )
    } else {
        match thumbs::analyze(&f.path, thumb_dir, &hash) {
            Some(a) => (
                a.phash,
                Some(a.width as i64),
                Some(a.height as i64),
                a.thumb,
                a.preview,
            ),
            None => (String::new(), None, None, None, None), // HEIC 等暂不可解码
        }
    };

    Outcome::Done(Box::new(MediaInsert {
        path: path_str,
        filename: f.filename.clone(),
        ext: f.ext.clone(),
        kind: if f.is_video { "video" } else { "image" },
        size: f.size,
        mtime: f.mtime,
        hash,
        phash: ph,
        base_time: t.secs,
        time_source: t.source.to_string(),
        time_confidence: t.confidence as i64,
        width: w,
        height: h,
        thumb,
        preview,
        source_id,
    }))
}

fn sha256_file(path: &Path) -> Option<String> {
    let mut f = BufReader::new(std::fs::File::open(path).ok()?);
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher).ok()?;
    Some(format!("{:x}", hasher.finalize()))
}

/// 重复分组：精确 hash + pHash 汉明距离，并查集合并。
/// 返回组数（≥2 成员的组）。
fn rebuild_dup_groups(conn: &mut Connection) -> usize {
    let rows: Vec<(i64, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, content_hash, phash FROM media ORDER BY id")
            .unwrap();
        let r = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        r.flatten().collect()
    };

    let mut uf = UnionFind::new(rows.len());
    let mut by_hash: HashMap<&str, usize> = HashMap::new();
    for (i, (_, hash, _)) in rows.iter().enumerate() {
        if let Some(&j) = by_hash.get(hash.as_str()) {
            uf.union(i, j);
        } else {
            by_hash.insert(hash.as_str(), i);
        }
    }

    // pHash 近似对（有上限保护，超限仅精确分组）
    let hashed: Vec<(usize, u64)> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, (_, _, ph))| phash::from_hex(ph).map(|v| (i, v)))
        .collect();
    if hashed.len() <= PHASH_PAIRWISE_CAP {
        let n = hashed.len();
        let idx: Vec<usize> = hashed.iter().map(|(i, _)| *i).collect();
        let vals: Vec<u64> = hashed.iter().map(|(_, v)| *v).collect();
        let (idx_ref, vals_ref) = (&idx, &vals);
        let pairs: Vec<(usize, usize)> = (0..n)
            .into_par_iter()
            .flat_map_iter(|i| {
                (i + 1..n)
                    .filter(move |&j| phash::hamming(vals_ref[i], vals_ref[j]) <= PHASH_GROUP_THRESHOLD)
                    .map(move |j| (idx_ref[i], idx_ref[j]))
            })
            .collect();
        for (a, b) in pairs {
            uf.union(a, b);
        }
    }

    // 组代表 = 组内最小 media id
    let mut group_rep: HashMap<usize, i64> = HashMap::new();
    for i in 0..rows.len() {
        let root = uf.find(i);
        let id = rows[i].0;
        group_rep
            .entry(root)
            .and_modify(|m| *m = (*m).min(id))
            .or_insert(id);
    }
    let mut group_sizes: HashMap<usize, usize> = HashMap::new();
    for i in 0..rows.len() {
        *group_sizes.entry(uf.find(i)).or_insert(0) += 1;
    }

    conn.execute("UPDATE media SET dup_group = NULL", []).unwrap();
    {
        let tx = conn.transaction().unwrap();
        {
            let mut stmt = tx
                .prepare("UPDATE media SET dup_group = ?1 WHERE id = ?2")
                .unwrap();
            for i in 0..rows.len() {
                let root = uf.find(i);
                if group_sizes[&root] > 1 {
                    stmt.execute(rusqlite::params![group_rep[&root], rows[i].0]).unwrap();
                }
            }
        }
        tx.commit().unwrap();
    }
    group_sizes.values().filter(|&&s| s > 1).count()
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect() }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[rb.max(ra)] = rb.min(ra);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn write_gradient(path: &Path, seed: u8, size: u32) {
        let (f1, f2, f3) = (0.6 + seed as f32 * 0.11, 0.35 + seed as f32 * 0.05, 0.22);
        let mut img = RgbImage::new(size, size);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let (x, y) = (x as f32 / size as f32, y as f32 / size as f32);
            let v = (x * f1 * 6.28).sin() * 0.45
                + (y * f2 * 6.28).sin() * 0.30
                + ((x + y) * f3 * 6.28).sin() * 0.25;
            let b = ((v + 1.0) * 127.5) as u8;
            *p = Rgb([b, 255 - b, (b / 2).wrapping_add(seed.wrapping_mul(30))]);
        }
        image::DynamicImage::ImageRgb8(img).save(path).unwrap();
    }

    #[test]
    fn scan_dedup_and_time_recovery() {
        let dir = tempfile::tempdir().unwrap();
        // 缩略图与数据库放扫描目录外，避免被当成新照片扫进来
        let root = dir.path().join("media");
        std::fs::create_dir_all(&root).unwrap();
        let tdir = dir.path().join("thumbs");

        // 1: 有文件名时间的原图；2: 与 1 完全相同的内容（跨文件夹精确重复）；3: 与 1 同构图同尺寸但 PNG 无损编码（近似重复：微信压缩图 vs 原图）
        write_gradient(&root.join("IMG_20231005_080000.jpg"), 7, 300);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        write_gradient(&root.join("sub").join("mmexport1696491234567.jpg"), 7, 300);
        write_gradient(&root.join("near.png"), 7, 300);
        write_gradient(&root.join("unique.png"), 200, 260);

        let db_path = dir.path().join("t.db");
        let mut conn = db::init(&db_path).unwrap();
        let summary = scan_core(&root, &mut conn, &tdir, &|_, _, _| {}, None);

        assert_eq!(summary.inserted, 4);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.images, 4);

        // 时间恢复：IMG_20231005_080000 → 2023-10-05 08:00:00 UTC
        let t: i64 = conn
            .query_row(
                "SELECT base_time FROM media WHERE filename='IMG_20231005_080000.jpg'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t, 1696492800);
        // 微信文件名恢复
        let (t2, conf): (i64, i64) = conn
            .query_row(
                "SELECT base_time, time_confidence FROM media WHERE filename LIKE 'mmexport%'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(t2, 1696491234);
        assert_eq!(conf, 1);

        // 去重：前 3 张（1 精确重复 + 1 近似重复）同一组，unique 单独
        let grouped: i64 = conn
            .query_row("SELECT COUNT(*) FROM media WHERE dup_group IS NOT NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(grouped, 3);
        assert_eq!(summary.dup_groups, Some(1));

        // 缩略图已生成
        let thumbs = std::fs::read_dir(&tdir).unwrap().count();
        assert!(thumbs >= 3);
    }

    #[test]
    fn incremental_rescan_skips_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("media");
        std::fs::create_dir_all(&root).unwrap();
        write_gradient(&root.join("a.jpg"), 1, 120);
        let mut conn = db::init(&dir.path().join("t.db")).unwrap();
        let th = dir.path().join("th");
        let s1 = scan_core(&root, &mut conn, &th, &|_, _, _| {}, None);
        assert_eq!(s1.inserted, 1);
        let s2 = scan_core(&root, &mut conn, &th, &|_, _, _| {}, None);
        assert_eq!(s2.inserted, 0);
        assert_eq!(s2.unchanged, 1);
    }
}
