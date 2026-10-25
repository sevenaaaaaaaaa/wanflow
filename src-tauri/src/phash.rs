use image::DynamicImage;

/// 经典 pHash：32x32 灰度 → 二维 DCT-II → 左上 8x8 系数与中位数比较 → 64 位指纹。
/// 缩放/轻微压缩/色调变化不改变指纹，适合"连拍/近似图"分组。
pub fn phash_u64(img: &DynamicImage) -> u64 {
    let small = img
        .grayscale()
        .resize_exact(32, 32, image::imageops::FilterType::CatmullRom)
        .to_luma8();
    let mut m = [[0f32; 32]; 32];
    for y in 0..32 {
        for x in 0..32 {
            m[y][x] = small.get_pixel(x as u32, y as u32).0[0] as f32;
        }
    }
    // 行 DCT
    let rows: Vec<[f32; 32]> = m.iter().map(|r| dct1(r)).collect();
    // 列 DCT
    let mut coeffs = [[0f32; 32]; 32];
    for x in 0..32 {
        let col: [f32; 32] = std::array::from_fn(|y| rows[y][x]);
        let c = dct1(&col);
        for y in 0..32 {
            coeffs[y][x] = c[y];
        }
    }
    let mut vals = [0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            vals[y * 8 + x] = coeffs[y][x];
        }
    }
    let mut sorted = vals;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = (sorted[31] + sorted[32]) / 2.0;
    let mut h: u64 = 0;
    for (i, v) in vals.iter().enumerate() {
        if *v > median {
            h |= 1 << i;
        }
    }
    h
}

fn dct1(v: &[f32; 32]) -> [f32; 32] {
    let mut out = [0f32; 32];
    for (k, o) in out.iter_mut().enumerate() {
        let mut s = 0.0;
        for (n, x) in v.iter().enumerate() {
            s += x * ((std::f32::consts::PI / 32.0) * (n as f32 + 0.5) * k as f32).cos();
        }
        *o = s;
    }
    out
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn to_hex(h: u64) -> String {
    format!("{h:016x}")
}

pub fn from_hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    // 低频正弦图案：8x8 DCT 低频系数有真实结构，且对 JPEG 有损压缩稳定（真实照片以低频为主）
    fn pattern(seed: u8, size: u32) -> DynamicImage {
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
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn same_image_same_hash() {
        let img = pattern(10, 128);
        assert_eq!(hamming(phash_u64(&img), phash_u64(&img)), 0);
    }

    #[test]
    fn resized_copy_stays_close() {
        let big = pattern(7, 512);
        let small = pattern(7, 96); // 同构图，仅分辨率不同
        assert!(hamming(phash_u64(&big), phash_u64(&small)) <= 10, "hamming = {}", hamming(phash_u64(&big), phash_u64(&small)));
    }

    #[test]
    fn different_images_far_apart() {
        let a = pattern(3, 128);
        let b = pattern(140, 128); // 不同频率组合 → 不同画面
        let d = hamming(phash_u64(&a), phash_u64(&b));
        assert!(d > 20, "hamming = {d}");
    }
}
