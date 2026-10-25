//! 时间恢复级联：EXIF → 视频容器(mvhd) → 文件名模式 → 文件修改时间。
//! QQ/微信下载的照片普遍丢失 EXIF，文件名里的时间戳往往是唯一线索。

use chrono::{NaiveDate, Timelike};
use regex::Regex;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct TimeFound {
    pub secs: i64, // unix 秒
    pub source: &'static str,
    pub confidence: i32, // 3=EXIF 2=容器 1=文件名 0=mtime
}

const MAC_EPOCH_OFFSET: i64 = 2082844800; // 1904 → 1970

pub fn recover(path: &Path, filename: &str, is_video: bool, mtime: i64) -> TimeFound {
    if !is_video {
        if let Some(t) = exif_time(path) {
            return t;
        }
    }
    if is_video {
        if let Some(t) = mvhd_time(path) {
            return t;
        }
    }
    if let Some(t) = filename_time(filename) {
        return t;
    }
    TimeFound {
        secs: mtime,
        source: "文件修改时间",
        confidence: 0,
    }
}

/// JPEG/TIFF 的 EXIF 原始拍摄时间
fn exif_time(path: &Path) -> Option<TimeFound> {
    let file = File::open(path).ok()?;
    let mut buf = std::io::BufReader::new(&file);
    let exif = exif::Reader::new().read_from_container(&mut buf).ok()?;
    for tag in [exif::Tag::DateTimeOriginal, exif::Tag::DateTime] {
        if let Some(field) = exif.get_field(tag, exif::In::PRIMARY) {
            if let exif::Value::Ascii(bytes) = &field.value {
                if bytes.is_empty() {
                    continue;
                }
                let s = String::from_utf8_lossy(&bytes[0]);
                if let Ok(t) = chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y:%m:%d %H:%M:%S") {
                    let secs = t.and_utc().timestamp();
                    if sane(secs) {
                        return Some(TimeFound {
                            secs,
                            source: "EXIF 拍摄时间",
                            confidence: 3,
                        });
                    }
                }
            }
        }
    }
    None
}

/// MP4/MOV 的 mvhd 创建时间（QuickTime 1904 纪元）
fn mvhd_time(path: &Path) -> Option<TimeFound> {
    let f = File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(4 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    let pos = find_sub(&buf, b"mvhd")? + 4; // 指向 version 字节
    if buf.len() < pos + 8 {
        return None;
    }
    let version = buf[pos];
    let creation: u64 = if version == 1 {
        if buf.len() < pos + 4 + 8 {
            return None;
        }
        let b: [u8; 8] = buf[pos + 4..pos + 12].try_into().ok()?;
        u64::from_be_bytes(b)
    } else {
        let b: [u8; 4] = buf[pos + 4..pos + 8].try_into().ok()?;
        u32::from_be_bytes(b) as u64
    };
    let secs = creation as i64 - MAC_EPOCH_OFFSET;
    if sane(secs) {
        Some(TimeFound {
            secs,
            source: "视频拍摄时间",
            confidence: 2,
        })
    } else {
        None
    }
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn sane(secs: i64) -> bool {
    // 2001-01-01 ~ 2035-12-31，挡掉相机没电复位、二进制误匹配
    (978307200..=2082758400).contains(&secs)
}

/// 文件名模式解析，按优先级：
/// 1) IMG_20231005_123456 / VID_… / wxcamera_20231005123456（紧凑日期+时间）
/// 2) 安卓截图 Screenshot_2023-10-05-12-34-56-…
/// 3) 微信 mmexport1696491234567 / wx_camera_1696491234567
/// 4) 独立 13 位毫秒时间戳
/// 5) 仅日期 IMG_20231005 → 当天 12:00
pub fn filename_time(filename: &str) -> Option<TimeFound> {
    let name = filename.to_lowercase();

    static RE_DT: OnceLock<Regex> = OnceLock::new();
    static RE_DASH: OnceLock<Regex> = OnceLock::new();
    static RE_WX: OnceLock<Regex> = OnceLock::new();
    static RE_MS: OnceLock<Regex> = OnceLock::new();
    static RE_DATE: OnceLock<Regex> = OnceLock::new();

    let re_dt = RE_DT.get_or_init(|| Regex::new(r"(20\d{6})[-_]?(\d{6})").unwrap());
    if let Some(c) = re_dt.captures(&name) {
        let t = ymd_hms(&c[1], &c[2]);
        if let Some(secs) = t.filter(|s| sane(*s)) {
            return Some(TimeFound { secs, source: "文件名时间", confidence: 1 });
        }
    }

    let re_dash = RE_DASH.get_or_init(|| {
        Regex::new(r"(20\d{2})-(\d{2})-(\d{2})-(\d{2})-(\d{2})-(\d{2})").unwrap()
    });
    if let Some(c) = re_dash.captures(&name) {
        let d = format!("{}{}{}", &c[1], &c[2], &c[3]);
        let t = ymd_hms(&d, &format!("{}{}{}", &c[4], &c[5], &c[6]));
        if let Some(secs) = t.filter(|s| sane(*s)) {
            return Some(TimeFound { secs, source: "文件名时间", confidence: 1 });
        }
    }

    let re_wx = RE_WX.get_or_init(|| Regex::new(r"(?:mmexport|wx_camera)[-_]?(\d{13})").unwrap());
    if let Some(c) = re_wx.captures(&name) {
        if let Some(secs) = ms_epoch(&c[1]).filter(|s| sane(*s)) {
            return Some(TimeFound { secs, source: "微信文件名时间", confidence: 1 });
        }
    }

    let re_ms = RE_MS.get_or_init(|| Regex::new(r"(?:^|[^0-9])(\d{13})(?:[^0-9]|$)").unwrap());
    if let Some(c) = re_ms.captures(&name) {
        if let Some(secs) = ms_epoch(&c[1]).filter(|s| sane(*s)) {
            return Some(TimeFound { secs, source: "文件名时间戳", confidence: 1 });
        }
    }

    let re_date = RE_DATE.get_or_init(|| Regex::new(r"(20\d{6})").unwrap());
    if let Some(c) = re_date.captures(&name) {
        if let Some(secs) = ymd_noon(&c[1]).filter(|s| sane(*s)) {
            return Some(TimeFound { secs, source: "文件名日期", confidence: 1 });
        }
    }
    None
}

fn ymd_hms(d: &str, t: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(d, "%Y%m%d").ok()?;
    let time = chrono::NaiveTime::parse_from_str(t, "%H%M%S").ok()?;
    Some(
        date.and_hms_opt(time.hour(), time.minute(), time.second())?
            .and_utc()
            .timestamp(),
    )
}

fn ymd_noon(d: &str) -> Option<i64> {
    let date = NaiveDate::parse_from_str(d, "%Y%m%d").ok()?;
    Some(date.and_hms_opt(12, 0, 0)?.and_utc().timestamp())
}

fn ms_epoch(s: &str) -> Option<i64> {
    let ms: i64 = s.parse().ok()?;
    Some(ms / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2023-10-05 12:34:56 UTC 的 unix 秒
    const T: i64 = 1696509296;

    #[test]
    fn android_pattern() {
        let t = filename_time("IMG_20231005_123456.jpg").unwrap();
        assert_eq!(t.secs, T);
        assert_eq!(t.confidence, 1);
    }

    #[test]
    fn huawei_extra_suffix() {
        let t = filename_time("IMG_20231005_123456_001.jpg").unwrap();
        assert_eq!(t.secs, T);
    }

    #[test]
    fn wxcamera_compact() {
        let t = filename_time("wxcamera_20231005123456.jpg").unwrap();
        assert_eq!(t.secs, T);
    }

    #[test]
    fn wechat_export() {
        let t = filename_time("mmexport1696491234567.jpeg").unwrap();
        assert_eq!(t.secs, 1696491234);
        assert_eq!(t.source, "微信文件名时间");
    }

    #[test]
    fn wx_camera_epoch() {
        let t = filename_time("wx_camera_1696491234567.jpg").unwrap();
        assert_eq!(t.secs, 1696491234);
    }

    #[test]
    fn android_screenshot_dashed() {
        let t = filename_time("Screenshot_2023-10-05-12-34-56-123_com.tencent.mm.jpg").unwrap();
        assert_eq!(t.secs, T);
    }

    #[test]
    fn video_name() {
        let t = filename_time("VID_20231005_123456.mp4").unwrap();
        assert_eq!(t.secs, T);
    }

    #[test]
    fn date_only_noon() {
        let t = filename_time("IMG_20231005.jpg").unwrap();
        assert_eq!(t.secs, 1696507200); // 12:00 UTC
    }

    #[test]
    fn garbage_returns_none() {
        assert!(filename_time("DSC00123.jpg").is_none());
        assert!(filename_time("photo.jpg").is_none());
    }

    #[test]
    fn mvhd_version0() {
        // 最小假 mp4: size + 'mvhd' + v0 + creation + modification + timescale + duration
        let mut buf = vec![0u8; 4];
        buf.extend_from_slice(b"mvhd");
        buf.push(0u8); // version
        buf.extend_from_slice(&[0, 0, 0]); // flags
        let creation: u32 = (T + MAC_EPOCH_OFFSET) as u32;
        buf.extend_from_slice(&creation.to_be_bytes());
        buf.extend_from_slice(&creation.to_be_bytes());
        buf.extend_from_slice(&1000u32.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("v.mp4");
        std::fs::write(&p, &buf).unwrap();
        let t = mvhd_time(&p).unwrap();
        assert_eq!(t.secs, T);
        assert_eq!(t.source, "视频拍摄时间");
    }

    #[test]
    fn recover_prefers_exif_then_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mmexport1696491234567.jpg");
        std::fs::write(&p, b"not an image").unwrap();
        // 无 EXIF（非图片解不动）→ 文件名恢复
        let t = recover(&p, "mmexport1696491234567.jpg", false, 1000);
        assert_eq!(t.secs, 1696491234);
        assert_eq!(t.confidence, 1);
        // 无任何线索 → mtime
        let t2 = recover(&p, "x.jpg", false, 123456);
        assert_eq!(t2.secs, 123456);
        assert_eq!(t2.confidence, 0);
    }
}
