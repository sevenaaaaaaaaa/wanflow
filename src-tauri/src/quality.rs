//! 纯 CV 质量指标：从 480px 缩略图计算，毫秒级/张，无模型依赖、全平台一致。
//! MUSIQ / insightface 等 ONNX 模型后续接入时写入同一组列，精度升级对上层透明。

use image::DynamicImage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityMetrics {
    /// 清晰度（Laplacian 方差对数映射 0-100）
    pub sharpness: f64,
    /// 曝光（黑场/白场裁剪 + 均值偏离惩罚 0-100）
    pub exposure: f64,
    /// 对比度（亮度标准差 0-100）
    pub contrast: f64,
    /// 色彩丰富度（Hasler–Süsstrunk 0-100）
    pub colorfulness: f64,
    /// 亮度均值 0-255（区分过曝/欠曝文案用）
    pub brightness: f64,
    /// 综合分（家用权重：清晰 0.4 / 曝光 0.35 / 对比 0.15 / 色彩 0.10）
    pub quality: f64,
}

pub fn analyze(img: &DynamicImage) -> QualityMetrics {
    let luma = img.to_luma8();
    let (w, h) = (luma.width(), luma.height());
    let n = (w * h) as f64;

    let (mut sum, mut sq, mut low_clip, mut high_clip) = (0f64, 0f64, 0f64, 0f64);
    for px in luma.pixels() {
        let v = px.0[0] as f64;
        sum += v;
        sq += v * v;
        if v <= 6.0 {
            low_clip += 1.0;
        } else if v >= 249.0 {
            high_clip += 1.0;
        }
    }
    let mean = sum / n;
    let std = (sq / n - mean * mean).max(0.0).sqrt();

    // Laplacian 方差（内部像素）
    let (mut lsum, mut lsum2, mut ln) = (0f64, 0f64, 0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = luma.get_pixel(x, y).0[0] as f64;
            let lap = 4.0 * c
                - luma.get_pixel(x, y - 1).0[0] as f64
                - luma.get_pixel(x, y + 1).0[0] as f64
                - luma.get_pixel(x - 1, y).0[0] as f64
                - luma.get_pixel(x + 1, y).0[0] as f64;
            lsum += lap;
            lsum2 += lap * lap;
            ln += 1.0;
        }
    }
    let lap_var = if ln > 0.0 { lsum2 / ln - (lsum / ln).powi(2) } else { 0.0 };

    let sharpness = map_log(lap_var, 10.0, 1000.0);
    let low_frac = low_clip / n;
    let high_frac = high_clip / n;
    let mut exposure = 100.0 - low_frac * 300.0 - high_frac * 300.0;
    exposure -= if mean < 90.0 {
        (90.0 - mean) * 0.7
    } else if mean > 170.0 {
        (mean - 170.0) * 0.7
    } else {
        0.0
    };
    let exposure = exposure.clamp(0.0, 100.0);
    let contrast = (std / 60.0 * 100.0).clamp(0.0, 100.0);

    let colorfulness = {
        let rgb = img.to_rgb8();
        let (w2, h2) = rgb.dimensions();
        let n2 = (w2 * h2) as f64;
        let mut rg_sum = 0f64;
        let mut rg_sq = 0f64;
        let mut yb_sum = 0f64;
        let mut yb_sq = 0f64;
        for px in rgb.pixels() {
            let (r, g, b) = (px.0[0] as f64, px.0[1] as f64, px.0[2] as f64);
            let rg = r - g;
            let yb = 0.5 * (r + g) - b;
            rg_sum += rg;
            rg_sq += rg * rg;
            yb_sum += yb;
            yb_sq += yb * yb;
        }
        let (rg_m, yb_m) = (rg_sum / n2, yb_sum / n2);
        let (rg_s, yb_s) = ((rg_sq / n2 - rg_m * rg_m).max(0.0).sqrt(), (yb_sq / n2 - yb_m * yb_m).max(0.0).sqrt());
        let c = (rg_s * rg_s + yb_s * yb_s).sqrt() + 0.3 * (rg_m * rg_m + yb_m * yb_m).sqrt();
        (c / 60.0 * 100.0).clamp(0.0, 100.0)
    };

    let quality = 0.40 * sharpness + 0.35 * exposure + 0.15 * contrast + 0.10 * colorfulness;
    QualityMetrics { sharpness, exposure, contrast, colorfulness, brightness: mean, quality }
}

/// 对数映射：raw = lo → 0 分，raw = hi → 100 分（清晰度跨数量级，线性映射会失真）
fn map_log(raw: f64, lo: f64, hi: f64) -> f64 {
    let l = raw.max(lo).min(hi);
    100.0 * (l.log10() - lo.log10()) / (hi.log10() - lo.log10())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn xorshift_img(size: u32, seed: u64) -> DynamicImage {
        // 高频伪随机噪声：清晰度极高
        let mut s = seed | 1;
        let mut img = RgbImage::new(size, size);
        for (_, _, p) in img.enumerate_pixels_mut() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *p = Rgb([(s & 0xff) as u8, ((s >> 8) & 0xff) as u8, ((s >> 16) & 0xff) as u8]);
        }
        DynamicImage::ImageRgb8(img)
    }

    fn flat_img(size: u32, v: u8) -> DynamicImage {
        DynamicImage::ImageRgb8(RgbImage::from_pixel(size, size, Rgb([v, v, v])))
    }

    #[test]
    fn sharp_beats_blurred() {
        let sharp = xorshift_img(200, 42);
        let blurred = sharp.blur(2.5);
        let a = analyze(&sharp);
        let b = analyze(&blurred);
        assert!(
            a.sharpness > b.sharpness + 10.0,
            "sharp={} blurred={}",
            a.sharpness,
            b.sharpness
        );
    }

    #[test]
    fn black_and_white_are_bad_exposure() {
        let black = analyze(&flat_img(64, 0));
        let white = analyze(&flat_img(64, 255));
        assert!(black.exposure < 20.0, "black exposure={}", black.exposure);
        assert!(white.exposure < 20.0, "white exposure={}", white.exposure);
        assert!(black.contrast < 5.0 && white.contrast < 5.0);
    }

    #[test]
    fn midtone_image_scores_well() {
        // 中间调 + 轻微纹理：曝光应当高分
        let mut img = RgbImage::new(120, 120);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let v = 110 + ((x * 7 + y * 13) % 60) as u8;
            *p = Rgb([v, v, v]);
        }
        let m = analyze(&DynamicImage::ImageRgb8(img));
        assert!(m.exposure > 70.0, "exposure={}", m.exposure);
        assert!(m.sharpness > m.quality * 0.0);
    }

    #[test]
    fn scores_in_range() {
        for img in [xorshift_img(100, 7), flat_img(100, 128), flat_img(100, 255)] {
            let m = analyze(&img);
            for v in [m.sharpness, m.exposure, m.contrast, m.colorfulness, m.quality] {
                assert!((0.0..=100.0).contains(&v), "out of range: {v}");
            }
            // 综合分是四维加权
            let expect = 0.40 * m.sharpness + 0.35 * m.exposure + 0.15 * m.contrast + 0.10 * m.colorfulness;
            assert!((m.quality - expect).abs() < 1e-9);
        }
    }
}
