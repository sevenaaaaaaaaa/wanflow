//! 人脸管线端到端：下载模型 → 真实缩略图检测 → 同人聚类断言。
use std::path::PathBuf;

fn main() {
    let n = wanflow_lib::faces::ensure_models(&|m| println!("[download] {m}")).expect("模型下载失败");
    println!("模型就绪（新下载 {n} 个文件）");

    let home = std::env::var("HOME").unwrap();
    let thumb_dir = PathBuf::from(&home).join("Library/Caches/com.wanflow.app/thumbs");
    let mut thumbs: Vec<PathBuf> = std::fs::read_dir(&thumb_dir)
        .expect("thumbs")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            p.extension().map(|e| e == "jpg").unwrap_or(false) && !n.ends_with("_p.jpg")
        })
        .take(12)
        .collect();
    thumbs.truncate(12);
    println!("采样 {} 张真实缩略图", thumbs.len());

    let engine = wanflow_lib::faces::shared().expect("加载人脸引擎");
    let mut all_faces = Vec::new();
    let mut with_face = 0usize;
    for (i, p) in thumbs.iter().enumerate() {
        let img = image::ImageReader::open(p).unwrap().decode().unwrap();
        let faces = { engine.lock().unwrap().detect_and_embed(&img) }.expect("检测失败");
        println!("  [{}] {} 张脸", i, faces.len());
        if !faces.is_empty() {
            with_face += 1;
        }
        for f in faces {
            all_faces.push(f);
        }
    }
    println!("共 {} 张脸（{}/12 张照片有人脸）", all_faces.len(), with_face);
    assert!(with_face >= 4, "宝宝游泳照片应有较高人脸检出率");
    assert!(all_faces.len() >= 6, "至少检出 6 张脸");

    // 嵌入健康检查：单位范数
    for f in &all_faces {
        let norm: f32 = f.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "嵌入应归一化");
    }

    // 并查集聚类（阈值 0.363）
    let n = all_faces.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut Vec<usize>, mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for i in 0..n {
        for j in i + 1..n {
            let c = wanflow_lib::clip::cosine(&all_faces[i].embedding, &all_faces[j].embedding);
            if c >= wanflow_lib::faces::FACE_MATCH_COS {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }
    let mut clusters: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for i in 0..n {
        *clusters.entry(find(&mut parent, i)).or_insert(0) += 1;
    }
    let mut sizes: Vec<usize> = clusters.values().copied().collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    println!("聚类结果: {} 个簇，sizes={:?}（阈值 {}）", clusters.len(), sizes, wanflow_lib::faces::FACE_MATCH_COS);
    // 同一娃出现在多张照片 → 最大簇 ≥2
    assert!(
        sizes.first().map(|s| *s >= 2).unwrap_or(false),
        "同一个娃的多张照片应聚成一簇"
    );
    println!("✅ 人脸管线端到端验证通过");
}
