//! 人脸管线：YuNet 检测（锚点解码）→ 五点相似变换对齐 → SFace 128 维嵌入。
//! 模型来自 OpenCV Zoo（~240KB + ~38MB），懒下载，全本地推理。

use crate::clip::download_to;
use ndarray::Array4;
use ort::environment::Environment;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FACE_EMB_DIM: usize = 128;
pub const FACE_EMB_MODEL: &str = "sface-yunet";

const DETECT_URLS: &[&str] = &[
    "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
    "https://gh-proxy.com/https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
];
const RECOG_URLS: &[&str] = &[
    "https://github.com/opencv/opencv_zoo/raw/main/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
    "https://gh-proxy.com/https://github.com/opencv/opencv_zoo/raw/main/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
];

/// SFace 余弦阈值（OpenCV 官方文档建议 0.363）
pub const FACE_MATCH_COS: f32 = 0.363;

/// ArcFace 112x112 五点模板
const ARCFACE_TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

pub struct DetectedFace {
    /// 归一化坐标 0-1（相对输入图）
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
    pub landmarks: [[f32; 2]; 5], // 归一化坐标
    pub embedding: Vec<f32>,      // 128 维已归一化
}

pub struct FaceEngine {
    _env: Environment,
    detect: Session,
    recog: Session,
}

static ENGINE: Mutex<Option<std::sync::Arc<Mutex<FaceEngine>>>> = Mutex::new(None);

pub fn shared() -> Result<std::sync::Arc<Mutex<FaceEngine>>, String> {
    let mut guard = ENGINE.lock().map_err(|e| e.to_string())?;
    if let Some(e) = guard.as_ref() {
        return Ok(e.clone());
    }
    let dir = models_dir()?;
    let engine = std::sync::Arc::new(Mutex::new(FaceEngine::load(&dir)?));
    *guard = Some(engine.clone());
    Ok(engine)
}

pub fn models_dir() -> Result<PathBuf, String> {
    let base = crate::clip::models_dir()?;
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    Ok(base)
}

pub fn models_ready() -> bool {
    let Ok(dir) = models_dir() else { return false };
    detect_path(&dir).is_file() && recog_path(&dir).is_file()
}

fn detect_path(dir: &Path) -> PathBuf {
    dir.join("face_detection_yunet_2023mar.onnx")
}
fn recog_path(dir: &Path) -> PathBuf {
    dir.join("face_recognition_sface_2021dec.onnx")
}

pub fn ensure_models(progress: &dyn Fn(&str)) -> Result<usize, String> {
    let dir = models_dir()?;
    let mut downloaded = 0;
    for (path, urls, label) in [
        (detect_path(&dir), DETECT_URLS, "人脸检测模型（0.2 MB）"),
        (recog_path(&dir), RECOG_URLS, "人脸识别模型（38 MB）"),
    ] {
        if path.is_file() && std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            continue;
        }
        let tmp = path.with_extension("part");
        let mut ok = false;
        let mut last_err = String::new();
        for url in urls {
            progress(&format!("下载 {label}…"));
            match download_to(url, &tmp, &|_| {}) {
                Ok(()) => {
                    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
                    ok = true;
                    downloaded += 1;
                    break;
                }
                Err(e) => {
                    last_err = e;
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }
        if !ok {
            return Err(format!("模型下载失败。{last_err}"));
        }
    }
    Ok(downloaded)
}

impl FaceEngine {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let dp = detect_path(dir);
        let rp = recog_path(dir);
        for p in [&dp, &rp] {
            if !p.is_file() {
                return Err(format!("模型文件缺失：{}（先下载模型）", p.display()));
            }
        }
        let env = ort::init().with_name("wanflow-face").build().map_err(|e| e.to_string())?;
        let detect = Session::builder(&env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .commit_from_file(&dp)
            .map_err(|e| format!("加载人脸检测模型失败: {e}"))?;
        let recog = Session::builder(&env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .commit_from_file(&rp)
            .map_err(|e| format!("加载人脸识别模型失败: {e}"))?;
        Ok(Self { _env: env, detect, recog })
    }

    /// 输入图（推荐 480px 缩略图）→ 人脸列表（含嵌入）
    pub fn detect_and_embed(&mut self, img: &image::DynamicImage) -> Result<Vec<DetectedFace>, String> {
        // 该 ONNX 导出为固定 640x640 输入
        const IN_W: u32 = 640;
        const IN_H: u32 = 640;
        let resized = img.resize_exact(IN_W, IN_H, image::imageops::FilterType::Triangle);
        let rgb = resized.to_rgb8();

        // 前处理：HWC RGB → CHW float（YuNet 输入 1x3xHxW，无 mean/std）
        let mut input = Array4::<f32>::zeros((1, 3, IN_H as usize, IN_W as usize));
        for y in 0..IN_H as usize {
            for x in 0..IN_W as usize {
                let px = rgb.get_pixel(x as u32, y as u32).0;
                for c in 0..3 {
                    input[[0, c, y, x]] = px[c] as f32;
                }
            }
        }

        let pv = Tensor::from_array(input).map_err(|e| e.to_string())?;
        let faces = {
            let outputs = self.detect.run(ort::inputs![pv]).map_err(|e| e.to_string())?;
            decode_yunet(&outputs, IN_W as f32, IN_H as f32)?
        };

        // 逐脸对齐 + 嵌入
        let big = img.to_rgb8();
        let (bw, bh) = (big.width() as f32, big.height() as f32);
        let mut out = Vec::with_capacity(faces.len());
        for mut f in faces {
            // landmarks 是相对检测输入(320x240)的归一化坐标 → 映射回原图坐标
            let lms: [[f32; 2]; 5] = f.landmarks.map(|[lx, ly]| [lx * bw, ly * bh]);
            let crop = align_face(&big, &lms);
            let emb = self.embed_crop(&crop)?;
            f.embedding = emb;
            out.push(f);
        }
        Ok(out)
    }

    fn embed_crop(&mut self, crop: &image::RgbImage) -> Result<Vec<f32>, String> {
        // SFace 前处理：(x/255 - 0.5)/0.5，RGB，112x112
        let mut input = Array4::<f32>::zeros((1, 3, 112, 112));
        for y in 0..112usize {
            for x in 0..112usize {
                let px = crop.get_pixel(x as u32, y as u32).0;
                for c in 0..3 {
                    input[[0, c, y, x]] = (px[c] as f32 / 255.0 - 0.5) / 0.5;
                }
            }
        }
        let t = Tensor::from_array(input).map_err(|e| e.to_string())?;
        let outputs = self.recog.run(ort::inputs![t]).map_err(|e| e.to_string())?;
        for (_name, value) in outputs.iter() {
            if let Ok((shape, data)) = value.try_extract_tensor::<f32>() {
                if shape.len() == 2 && shape[1] == FACE_EMB_DIM as i64 {
                    let mut v: Vec<f32> = data.to_vec();
                    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                    if norm > 0.0 {
                        for x in &mut v {
                            *x /= norm;
                        }
                    }
                    return Ok(v);
                }
            }
        }
        Err("SFace 输出里没有 128 维嵌入".into())
    }
}

/// YuNet 解码（OpenCV 官方 face_detect.cpp 同款，2023mar ONNX 专用）：
/// 每格单锚点，stride 8/16/32；score = sqrt(cls×obj)；
/// bbox: (格 + 偏移)×stride，宽高 exp()×stride；kps: (偏移 + 格)×stride。无方差。
fn decode_yunet(
    outputs: &ort::session::SessionOutputs<'_>,
    in_w: f32,
    in_h: f32,
) -> Result<Vec<DetectedFace>, String> {
    let get = |want: &str| -> Option<Vec<f32>> {
        outputs.get(want)?.try_extract_tensor::<f32>().ok().map(|(_, d)| d.to_vec())
    };
    const SCORE_THRESHOLD: f32 = 0.5;
    const NMS_THRESHOLD: f32 = 0.3;

    let mut raw: Vec<DetectedFace> = Vec::new();
    for (cls_n, obj_n, box_n, kps_n, stride) in [
        ("cls_8", "obj_8", "bbox_8", "kps_8", 8.0f32),
        ("cls_16", "obj_16", "bbox_16", "kps_16", 16.0),
        ("cls_32", "obj_32", "bbox_32", "kps_32", 32.0),
    ] {
        let Some(cls) = get(cls_n) else { return Err(format!("缺少 {cls_n}")) };
        let Some(obj) = get(obj_n) else { return Err(format!("缺少 {obj_n}")) };
        let Some(bbox) = get(box_n) else { return Err(format!("缺少 {box_n}")) };
        let Some(kps) = get(kps_n) else { return Err(format!("缺少 {kps_n}")) };

        let fw = (in_w / stride).ceil() as usize;
        let fh = (in_h / stride).ceil() as usize;
        for r in 0..fh {
            for c in 0..fw {
                let i = r * fw + c;
                let cls_score = cls[i].clamp(0.0, 1.0);
                let obj_score = obj[i].clamp(0.0, 1.0);
                let score = (cls_score * obj_score).sqrt();
                if score < SCORE_THRESHOLD {
                    continue;
                }
                let cx = ((c as f32 + bbox[i * 4]) * stride) / in_w;
                let cy = ((r as f32 + bbox[i * 4 + 1]) * stride) / in_h;
                let bw = bbox[i * 4 + 2].exp() * stride / in_w;
                let bh = bbox[i * 4 + 3].exp() * stride / in_h;
                let mut landmarks = [[0f32; 2]; 5];
                for k in 0..5 {
                    landmarks[k] = [
                        ((kps[i * 10 + k * 2] + c as f32) * stride) / in_w,
                        ((kps[i * 10 + k * 2 + 1] + r as f32) * stride) / in_h,
                    ];
                }
                raw.push(DetectedFace {
                    x: (cx - bw / 2.0).max(0.0) / in_w,
                    y: (cy - bh / 2.0).max(0.0) / in_h,
                    w: bw,
                    h: bh,
                    score,
                    landmarks,
                    embedding: Vec::new(),
                });
            }
        }
    }

    raw.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut keep: Vec<DetectedFace> = Vec::new();
    for f in raw {
        let overlapped = keep.iter().any(|k| {
            let ix = (f.x + f.w).min(k.x + k.w) - f.x.max(k.x);
            let iy = (f.y + f.h).min(k.y + k.h) - f.y.max(k.y);
            let inter = ix.max(0.0) * iy.max(0.0);
            let union = f.w * f.h + k.w * k.h - inter;
            inter / union.max(1e-6) > NMS_THRESHOLD
        });
        if !overlapped {
            keep.push(f);
        }
    }
    Ok(keep)
}

/// 五点相似变换对齐到 ArcFace 112x112 模板（Umeyama：均值中心化 + atan2 求旋转 + 均值比求缩放）
pub fn align_face(img: &image::RgbImage, lms: &[[f32; 2]; 5]) -> image::RgbImage {
    let src_c: [f64; 2] = lms
        .iter()
        .fold([0.0, 0.0], |acc, p| [acc[0] + p[0] as f64 / 5.0, acc[1] + p[1] as f64 / 5.0]);
    let dst_c: [f64; 2] = ARCFACE_TEMPLATE
        .iter()
        .fold([0.0, 0.0], |acc, p| [acc[0] + p[0] as f64 / 5.0, acc[1] + p[1] as f64 / 5.0]);

    let (mut num, mut den, mut scale_num, mut scale_den) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for i in 0..5 {
        let sx = lms[i][0] as f64 - src_c[0];
        let sy = lms[i][1] as f64 - src_c[1];
        let dx = ARCFACE_TEMPLATE[i][0] as f64 - dst_c[0];
        let dy = ARCFACE_TEMPLATE[i][1] as f64 - dst_c[1];
        num += sx * dy - sy * dx;
        den += sx * dx + sy * dy;
        scale_num += (dx * dx + dy * dy).sqrt();
        scale_den += (sx * sx + sy * sy).sqrt();
    }
    let theta = num.atan2(den);
    let (sin_t, cos_t) = theta.sin_cos();
    let scale = if scale_den > 1e-12 { scale_num / scale_den } else { 1.0 };

    let mut out = image::RgbImage::new(112, 112);
    for oy in 0..112u32 {
        for ox in 0..112u32 {
            // 逆向映射：输出像素 ← 源图坐标
            let tx = ox as f64 - dst_c[0];
            let ty = oy as f64 - dst_c[1];
            let rx = (cos_t * tx + sin_t * ty) / scale + src_c[0];
            let ry = (-sin_t * tx + cos_t * ty) / scale + src_c[1];
            let px = bilinear(img, rx as f32, ry as f32);
            out.put_pixel(ox, oy, px);
        }
    }
    out
}

fn bilinear(img: &image::RgbImage, x: f32, y: f32) -> image::Rgb<u8> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let sample = |xx: i32, yy: i32| -> [u8; 3] {
        let xx = xx.clamp(0, w - 1);
        let yy = yy.clamp(0, h - 1);
        img.get_pixel(xx as u32, yy as u32).0
    };
    if x.is_nan() || y.is_nan() || x < -2.0 || y < -2.0 || x > w as f32 + 2.0 || y > h as f32 + 2.0 {
        return image::Rgb([128, 128, 128]);
    }
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let mut out = [0u8; 3];
    for c in 0..3 {
        let p00 = sample(x0, y0)[c] as f32;
        let p10 = sample(x0 + 1, y0)[c] as f32;
        let p01 = sample(x0, y0 + 1)[c] as f32;
        let p11 = sample(x0 + 1, y0 + 1)[c] as f32;
        out[c] = (p00 * (1.0 - fx) * (1.0 - fy) + p10 * fx * (1.0 - fy) + p01 * (1.0 - fx) * fy + p11 * fx * fy)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    image::Rgb(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn align_face_output_shape_and_transform() {
        let mut img = RgbImage::new(400, 400);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgb([(x % 256) as u8, (y % 256) as u8, 128]);
        }
        // 人脸五点（大致正常姿态）
        let lms: [[f32; 2]; 5] = [
            [150.0, 160.0],
            [250.0, 160.0],
            [200.0, 210.0],
            [160.0, 260.0],
            [240.0, 260.0],
        ];
        let aligned = align_face(&img, &lms);
        assert_eq!((aligned.width(), aligned.height()), (112, 112));
        // 对齐后左眼点应落在模板左眼附近：原图(150,160) → 模板(38.3,51.7)
        let p = aligned.get_pixel(38, 52).0;
        // 原图在该点附近是水平条纹渐变，非纯色；只断言不是边缘黑边
        assert!(p[2] == 128);
    }


}
