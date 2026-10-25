//! CLIP 语义索引：ViT-B/32 图文双塔（Qdrant 导出的 ONNX 切分版）。
//! 模型懒下载（huggingface 直连 → hf-mirror 回退），嵌入纯本地推理。
//! 用途：语义搜索 + 自然语言配额精选集。

use ndarray::Array4;
use ort::environment::Environment;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const EMB_DIM: usize = 512;
pub const EMB_MODEL: &str = "clip-vit-b32-xenova-q8";

// transformers.js 官方导出：CLIPModel 全模型单文件（双塔一次跑），int8 量化 CPU 最快
const MODEL_URLS: &[&str] = &[
    "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/onnx/model_quantized.onnx",
    "https://hf-mirror.com/Xenova/clip-vit-base-patch32/resolve/main/onnx/model_quantized.onnx",
];
const TOKENIZER_URLS: &[&str] = &[
    "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/main/tokenizer.json",
    "https://hf-mirror.com/Xenova/clip-vit-base-patch32/resolve/main/tokenizer.json",
];

/// CLIP 归一化常数（OpenAI 官方）
const MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
const STD: [f32; 3] = [0.268_629_54, 0.261_302_58, 0.275_777_11];

pub struct Clip {
    _env: Environment,
    sess: Session,
    tokenizer: tokenizers::Tokenizer,
}

/// 全局会话惰性初始化（模型加载 ~1s，进程内复用）
static CLIP: Mutex<Option<std::sync::Arc<Mutex<Clip>>>> = Mutex::new(None);

pub fn shared() -> Result<std::sync::Arc<Mutex<Clip>>, String> {
    let mut guard = CLIP.lock().map_err(|e| e.to_string())?;
    if let Some(c) = guard.as_ref() {
        return Ok(c.clone());
    }
    let models_dir = models_dir()?;
    let clip = std::sync::Arc::new(Mutex::new(Clip::load(&models_dir)?));
    *guard = Some(clip.clone());
    Ok(clip)
}

pub fn models_dir() -> Result<PathBuf, String> {
    let base = match std::env::var("WANFLOW_MODELS_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => {
            let home = std::env::var("HOME").map_err(|_| "无 HOME".to_string())?;
            PathBuf::from(home)
                .join("Library/Caches/com.wanflow.app/models")
        }
    };
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    Ok(base)
}

/// 模型是否已就绪（供 UI 判断要不要先下载）
pub fn models_ready() -> bool {
    let Ok(dir) = models_dir() else { return false };
    model_path(&dir).is_file() && tokenizer_path(&dir).is_file()
}

pub fn download_progress_text() -> String {
    "CLIP 模型（约 150 MB，仅一次，之后全部本地推理）".to_string()
}

fn model_path(dir: &Path) -> PathBuf {
    dir.join("clip-vit-b32-q8.onnx")
}
fn tokenizer_path(dir: &Path) -> PathBuf {
    dir.join("clip-vit-b32-tokenizer.json")
}

/// 下载缺失的模型文件（带镜像回退）。返回 (已下载文件数)
pub fn ensure_models(progress: &dyn Fn(&str)) -> Result<usize, String> {
    let dir = models_dir()?;
    let mut downloaded = 0;
    for (path, urls, label) in [
        (model_path(&dir), MODEL_URLS, "CLIP 模型（int8 量化）"),
        (tokenizer_path(&dir), TOKENIZER_URLS, "CLIP 分词器"),
    ] {
        if path.is_file() && std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            continue;
        }
        progress(&format!("下载 {label}…"));
        let tmp = path.with_extension("part");
        let mut ok = false;
        let mut last_err = String::new();
        for url in urls {
            progress(&format!("下载 {label}：{url}"));
            match download_to(url, &tmp, &|frac| {
                progress(&format!("下载 {label}… {:.0}%", frac * 100.0));
            }) {
                Ok(()) => {
                    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
                    ok = true;
                    downloaded += 1;
                    break;
                }
                Err(e) => {
                    last_err = format!("{url}: {e}");
                    let _ = std::fs::remove_file(&tmp);
                }
            }
        }
        if !ok {
            return Err(format!("模型下载失败。{last_err}（可在网络设置里开代理后重试）"));
        }
    }
    Ok(downloaded)
}

fn download_to(url: &str, dest: &Path, progress: &dyn Fn(f32)) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut got = 0usize;
    loop {
        let n = resp.read_to_end_safe(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        got += n;
        if total > 0 {
            progress(got as f32 / total as f32);
        }
    }
    Ok(())
}

/// reqwest::blocking::Response 的 read 扩展（避免一次性载入 600MB）
trait ReadToEndSafe {
    fn read_to_end_safe(&mut self, buf: &mut [u8]) -> Result<usize, String>;
}
impl ReadToEndSafe for reqwest::blocking::Response {
    fn read_to_end_safe(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        use std::io::Read;
        std::io::Read::read(self, buf).map_err(|e| e.to_string())
    }
}

impl Clip {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let mp = model_path(dir);
        let kp = tokenizer_path(dir);
        for p in [&mp, &kp] {
            if !p.is_file() {
                return Err(format!("模型文件缺失：{}（先执行模型下载）", p.display()));
            }
        }
        let env = ort::init()
            .with_name("wanflow-clip")
            .build()
            .map_err(|e| format!("初始化推理环境失败: {e}"))?;
        let sess = Session::builder(&env)
            .map_err(|e| e.to_string())?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?
            .with_intra_threads(4)
            .map_err(|e| e.to_string())?
            .commit_from_file(&mp)
            .map_err(|e| format!("加载 CLIP 模型失败: {e}"))?;
        let tokenizer = tokenizers::Tokenizer::from_file(&kp)
            .map_err(|e| format!("加载分词器失败: {e}"))?;
        Ok(Self { _env: env, sess, tokenizer })
    }

    const EOT: i64 = 49407;

    /// 模型输入名（调试/自适应绑定用）
    pub fn input_names(&self) -> Vec<String> {
        self.sess
.inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect()
    }

    /// 图像嵌入：thumb 路径 → [512] 归一化向量（文本塔喂占位输入）
    pub fn embed_image(&mut self, path: &Path) -> Result<Vec<f32>, String> {
        let img = image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| format!("解码失败: {e}"))?;
        let pixel_values = preprocess(&img);
        let idle_ids = ndarray::Array2::from_elem((1, 77), Self::EOT);
        let idle_mask = ndarray::Array2::<i64>::zeros((1, 77));
        let pv = Tensor::from_array(pixel_values).map_err(|e| e.to_string())?;
        let ids = Tensor::from_array(idle_ids).map_err(|e| e.to_string())?;
        let mask = Tensor::from_array(idle_mask).map_err(|e| e.to_string())?;
        let outputs = self
            .sess
            .run(ort::inputs!["pixel_values" => pv, "input_ids" => ids, "attention_mask" => mask])
            .map_err(|e| e.to_string())?;
        extract_embedding(&outputs, "image_embeds")
    }

    /// 文本嵌入：查询文本 → [512] 归一化向量（图像塔喂占位输入）
    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let enc = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| e.to_string())?;
        let mut ids = enc.get_ids().to_vec();
        ids.truncate(77);
        ids.resize(77, Self::EOT as u32); // EOT pad（HF 约定）
        let ids: Vec<i64> = ids.iter().map(|&v| v as i64).collect();
        let mask: Vec<i64> = std::iter::repeat_n(1, enc.get_ids().len().min(77))
            .chain(std::iter::repeat_n(0, 77 - enc.get_ids().len().min(77)))
            .collect();
        let idle_pv = ndarray::Array4::<f32>::zeros((1, 3, 224, 224));
        let t_ids = Tensor::from_array(ndarray::Array2::from_shape_vec((1, 77), ids).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let t_mask =
            Tensor::from_array(ndarray::Array2::from_shape_vec((1, 77), mask).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let pv = Tensor::from_array(idle_pv).map_err(|e| e.to_string())?;
        let outputs = self
            .sess
            .run(ort::inputs!["input_ids" => t_ids, "pixel_values" => pv, "attention_mask" => t_mask])
            .map_err(|e| e.to_string())?;
        extract_embedding(&outputs, "text_embeds")
    }
}

/// CLIP 预处理：缩放短边到 224 → 中心裁剪 → 归一化 → CHW
fn preprocess(img: &image::DynamicImage) -> Array4<f32> {
    const S: u32 = 224;
    let (w, h) = (img.width(), img.height());
    let scaled = if w < h {
        img.resize_exact(S, (S * h + w - 1) / w, image::imageops::FilterType::Triangle)
    } else {
        img.resize_exact((S * w + h - 1) / h, S, image::imageops::FilterType::Triangle)
    };
    let (w2, h2) = (scaled.width(), scaled.height());
    let x0 = (w2.saturating_sub(S)) / 2;
    let y0 = (h2.saturating_sub(S)) / 2;
    let crop = image::imageops::crop_imm(&scaled, x0, y0, S.min(w2), S.min(h2)).to_image();
    let rgb = image::DynamicImage::ImageRgba8(crop).to_rgb8();

    let mut out = Array4::<f32>::zeros((1, 3, S as usize, S as usize));
    for y in 0..S as usize {
        for x in 0..S as usize {
            let px = rgb.get_pixel(x as u32, y as u32).0;
            for c in 0..3 {
                out[[0, c, y, x]] = (px[c] as f32 / 255.0 - MEAN[c]) / STD[c];
            }
        }
    }
    out
}

/// 从会话输出提取 [1,512] 嵌入并做 L2 归一化（优先取指定输出名）
fn extract_embedding(outputs: &ort::session::SessionOutputs<'_>, want: &str) -> Result<Vec<f32>, String> {
    let from = |value: &ort::value::DynValue| -> Option<Vec<f32>> {
        let (shape, data) = value.try_extract_tensor::<f32>().ok()?;
        if shape.len() == 2 && shape[1] == EMB_DIM as i64 {
            let mut v: Vec<f32> = data.to_vec();
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in &mut v {
                    *x /= norm;
                }
            }
            Some(v)
        } else {
            None
        }
    };
    if let Some(v) = outputs.get(want).and_then(|v| from(v)) {
        return Ok(v);
    }
    for (_name, value) in outputs.iter() {
        if let Some(v) = from(&value) {
            return Ok(v);
        }
    }
    Err("模型输出里没有 [1,512] 嵌入（模型版本不兼容）".into())
}

/// 余弦相似度（向量已归一化 → 点积）
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_shape_and_range() {
        let img = image::DynamicImage::new_rgb8(640, 480);
        let t = preprocess(&img);
        assert_eq!(t.shape(), &[1, 3, 224, 224]);
        // 黑图：每通道常数 (0-mean)/std，值域 [-2.1, 1.8] 内
        let v = t[[0, 0, 0, 0]];
        let expect = -MEAN[0] / STD[0];
        assert!((v - expect).abs() < 1e-6, "v={v} expect={expect}");
        // 竖图也输出 224 方形
        let tall = image::DynamicImage::new_rgb8(100, 900);
        assert_eq!(preprocess(&tall).shape(), &[1, 3, 224, 224]);
    }

    #[test]
    fn cosine_normalized() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let c = vec![1.0, 0.0];
        assert!((cosine(&a, &b)).abs() < 1e-6);
        assert!((cosine(&a, &c) - 1.0).abs() < 1e-6);
    }

    // 需要模型文件与推理：见 examples/clip_e2e.rs
}
