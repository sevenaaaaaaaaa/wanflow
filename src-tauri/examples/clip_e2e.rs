//! CLIP 端到端验证：下载模型 → 加载 → 语义排序断言（真实照片 vs 合成文档图）。
use std::path::PathBuf;

fn main() {
    // 1. 下载模型（直连失败自动切 hf-mirror）
    let n = wanflow_lib::clip::ensure_models(&|msg| println!("[download] {msg}")).expect("模型下载失败");
    println!("模型就绪（新下载 {n} 个文件）");

    // 2. 从真实库取 3 张缩略图
    let home = std::env::var("HOME").unwrap();
    let thumb_dir = PathBuf::from(&home).join("Library/Caches/com.wanflow.app/thumbs");
    let mut photos: Vec<PathBuf> = std::fs::read_dir(&thumb_dir)
        .expect("缩略图目录")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|e| e == "jpg").unwrap_or(false)
                && !p.file_name().unwrap().to_string_lossy().ends_with("_p.jpg")
        })
        .take(3)
        .collect();
    assert!(photos.len() == 3, "需要 3 张真实缩略图");
    println!("测试照片: {photos:?}");

    // 3. 合成一张"文档"图（白底黑线表格）——与婴儿照片语义相距最远
    let doc = PathBuf::from(&home).join("Library/Caches/com.wanflow.app/_doc_test.jpg");
    let mut img = image::RgbImage::new(400, 400);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let line = (y % 40 < 3) || (x % 80 < 3 && y % 40 >= 3);
        let v = if line { 20 } else { 245 };
        *p = image::Rgb([v, v, v]);
    }
    image::DynamicImage::ImageRgb8(img).save(&doc).unwrap();
    let mut all = photos.clone();
    all.push(doc.clone());

    // 4. 嵌入 + 断言语义排序
    let clip = wanflow_lib::clip::shared().expect("加载 CLIP");
    {
        let c = clip.lock().unwrap();
        let names: Vec<String> = c.input_names();
        println!("模型输入: {names:?}");
    }
    let query = "a baby swimming in a pool";
    let q = clip.lock().unwrap().embed_text(query).expect("文本嵌入");
    println!("查询: {query}");

    let mut scores = Vec::new();
    for p in &all {
        let v = clip.lock().unwrap().embed_image(p).expect("图像嵌入");
        let s = wanflow_lib::clip::cosine(&q, &v);
        scores.push((p.clone(), s));
        println!("  {:.4}  {}", s, p.file_name().unwrap().to_string_lossy());
    }

    let photo_avg: f32 = scores[..3].iter().map(|(_, s)| s).sum::<f32>() / 3.0;
    let doc_score = scores[3].1;
    println!("真实照片平均相似度 {photo_avg:.4} vs 文档图 {doc_score:.4}");
    assert!(
        photo_avg > doc_score + 0.03,
        "真实照片对婴儿游泳查询的相似度应显著高于文档图"
    );

    // 5. 反向查询也应翻转排序
    let q2 = clip.lock().unwrap().embed_text("a spreadsheet document").unwrap();
    let v0 = clip.lock().unwrap().embed_image(&photos[0]).unwrap();
    let vd = clip.lock().unwrap().embed_image(&doc).unwrap();
    let (s0, sd) = (wanflow_lib::clip::cosine(&q2, &v0), wanflow_lib::clip::cosine(&q2, &vd));
    println!("反向: 照片 {s0:.4} vs 文档 {sd:.4}");
    assert!(sd > s0, "文档查询下文档图应胜出");

    let _ = std::fs::remove_file(&doc);
    println!("✅ CLIP 端到端验证通过");
}
