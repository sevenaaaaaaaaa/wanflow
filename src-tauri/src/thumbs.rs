//! 图像解码 + pHash + 缩略图/预览图生成。
//! HEIC/AVIF 等格式当前 image 后端解不了时优雅降级（无缩略图，仍可挑拣/导出）。

use crate::phash;
use std::path::{Path, PathBuf};

pub struct ImageAnalysis {
    pub phash: String,
    pub width: u32,
    pub height: u32,
    pub thumb: Option<String>,
    pub preview: Option<String>,
}

const THUMB_EDGE: u32 = 480;
const PREVIEW_EDGE: u32 = 1600;

pub fn analyze(path: &Path, thumb_dir: &Path, hash: &str) -> Option<ImageAnalysis> {
    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let img = reader.decode().ok()?;
    let (w, h) = (img.width(), img.height());
    let ph = phash::to_hex(phash::phash_u64(&img));

    let short_hash: &str = &hash[..16.min(hash.len())];
    let thumb_path = thumb_dir.join(format!("{short_hash}.jpg"));
    if !thumb_path.exists() {
        let t = img.thumbnail(THUMB_EDGE, THUMB_EDGE);
        t.save_with_format(&thumb_path, image::ImageFormat::Jpeg)
            .ok()?;
    }
    let mut preview_path = None;
    if w.max(h) > PREVIEW_EDGE {
        let p = thumb_dir.join(format!("{short_hash}_p.jpg"));
        if !p.exists() {
            let pv = img.thumbnail(PREVIEW_EDGE, PREVIEW_EDGE);
            if pv
                .save_with_format(&p, image::ImageFormat::Jpeg)
                .is_ok()
            {
                preview_path = Some(p);
            }
        } else {
            preview_path = Some(p);
        }
    }

    Some(ImageAnalysis {
        phash: ph,
        width: w,
        height: h,
        thumb: Some(thumb_path.to_string_lossy().into_owned()),
        preview: preview_path.map(|p: PathBuf| p.to_string_lossy().into_owned()),
    })
}
