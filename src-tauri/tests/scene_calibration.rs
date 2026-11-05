//! 场景阈值校准工具（手工运行，不进 CI）。
//!
//! 目的：用一批带人工标注的照片，测量「同一场景是否被并到一起」「不同场景是否被误并」。
//! 真实家庭库到手后，用它把 SceneParams 从默认值校准到实测值。
//!
//! 用法：
//!   WF_CALIB_DIR=/path/to/set cargo test --test scene_calibration -- --ignored --nocapture
//!
//! 目录结构：
//!   /path/to/set/*.jpg|jpeg|png   参与校准的图片
//!   /path/to/set/labels.csv       可选；表头 file,scene[,ts]
//!
//! - scene：人工判定同一场景的标签，必须能区分「同场景多拍」与「不同场景」。
//! - ts：拍摄时间（unix 秒），可选；缺省按文件名排序每张间隔 5 分钟。
//! - 没有 labels.csv 时，把每张图当独立场景，只报告两两相似度与误并（用于摸底）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use wanflow_lib::{clip, scene};

struct Shot {
    label: String,
    ts: i64,
    emb: Vec<f32>,
}

fn parse_labels(dir: &Path) -> BTreeMap<String, (String, Option<i64>)> {
    let p = dir.join("labels.csv");
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(&p) else {
        return out;
    };
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || (i == 0 && line.starts_with("file")) {
            continue;
        }
        let cols: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        if cols.len() < 2 {
            continue;
        }
        let ts = cols.get(2).and_then(|t| t.parse::<i64>().ok());
        out.insert(cols[0].to_string(), (cols[1].to_string(), ts));
    }
    out
}

fn collect(dir: &Path) -> Vec<(PathBuf, String, i64)> {
    let labels = parse_labels(dir);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("读取校准目录失败")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()).as_deref(),
                Some("jpg") | Some("jpeg") | Some("png")
            )
        })
        .collect();
    files.sort();
    let mut out = Vec::new();
    for (i, path) in files.iter().enumerate() {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let (label, ts) = match labels.get(&name) {
            Some((l, t)) => (l.clone(), t.unwrap_or(1_800_000_000 + i as i64 * 300)),
            None => (name.clone(), 1_800_000_000 + i as i64 * 300),
        };
        out.push((path.clone(), label, ts));
    }
    out
}

fn pct(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f32 * p).round() as usize]
}

#[test]
#[ignore = "手工校准工具：需要 WF_CALIB_DIR 指向带标注的照片集"]
fn calibrate_scene_thresholds() {
    let dir = PathBuf::from(
        std::env::var("WF_CALIB_DIR").expect("请设置 WF_CALIB_DIR 指向校准照片目录"),
    );
    let raws = collect(&dir);
    assert!(!raws.is_empty(), "目录里没有图片：{}", dir.display());

    let mut clip_session = clip::Clip::load(&clip::models_dir().unwrap()).expect("加载 CLIP");
    let mut shots: Vec<Shot> = Vec::new();
    for (path, label, ts) in raws {
        let emb = clip_session.embed_image(&path).expect("嵌入失败");
        shots.push(Shot { label, ts, emb });
    }
    let labels_total = shots.iter().map(|s| s.label.as_str()).collect::<BTreeSet<_>>().len();
    println!(
        "图片 {} 张，人工场景 {} 个{}",
        shots.len(),
        labels_total,
        if labels_total < shots.len() { "（含同场景多拍）" } else { "（无同场景多拍，只能测误并）" }
    );

    let mut same = Vec::new();
    let mut cross = Vec::new();
    for i in 0..shots.len() {
        for j in (i + 1)..shots.len() {
            let s = clip::cosine(&shots[i].emb, &shots[j].emb);
            if shots[i].label == shots[j].label {
                same.push(s);
            } else {
                cross.push(s);
            }
        }
    }
    if !same.is_empty() {
        println!(
            "同场景相似度 n={} min={:.3} p10={:.3} 中位={:.3} max={:.3}",
            same.len(),
            same.iter().cloned().fold(f32::INFINITY, f32::min),
            pct(&same, 0.1),
            pct(&same, 0.5),
            same.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        );
    }
    if !cross.is_empty() {
        println!(
            "跨场景相似度 n={} p50={:.3} p90={:.3} p99={:.3} max={:.3}",
            cross.len(),
            pct(&cross, 0.5),
            pct(&cross, 0.9),
            pct(&cross, 0.99),
            cross.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        );
    }

    let items: Vec<scene::SceneItem> = shots
        .iter()
        .enumerate()
        .map(|(i, s)| scene::SceneItem { id: i as i64, ts: s.ts, emb: Some(s.emb.clone()) })
        .collect();

    println!("\nmin_sim  gap(min)  scenes  误并簇  拆分率   纯度");
    for gap_min in [30i64, 60, 90] {
        for sim in [0.72f32, 0.76, 0.78, 0.80, 0.82, 0.84, 0.86] {
            let p = scene::SceneParams {
                max_gap_secs: gap_min * 60,
                min_sim: sim,
                ..Default::default()
            };
            let cs = scene::cluster(&items, &p);
            let mut false_merge = 0usize;
            let mut hit = 0usize;
            for c in &cs {
                let labels: BTreeSet<&str> = c.members.iter().map(|m| shots[*m as usize].label.as_str()).collect();
                if labels.len() > 1 {
                    false_merge += 1;
                }
                let mut count: BTreeMap<&str, usize> = BTreeMap::new();
                for m in &c.members {
                    *count.entry(shots[*m as usize].label.as_str()).or_default() += 1;
                }
                hit += count.values().cloned().max().unwrap_or(0);
            }
            let mut split = 0usize;
            let mut multi_labels = 0usize;
            for l in shots.iter().map(|s| s.label.as_str()).collect::<BTreeSet<_>>() {
                let members: Vec<usize> = shots.iter().enumerate().filter(|(_, s)| s.label == l).map(|(i, _)| i).collect();
                if members.len() < 2 {
                    continue;
                }
                multi_labels += 1;
                let mut seen = BTreeSet::new();
                for c in &cs {
                    for m in members.iter() {
                        if c.members.contains(&(*m as i64)) {
                            seen.insert(c.key);
                        }
                    }
                }
                if seen.len() > 1 {
                    split += 1;
                }
            }
            let split_rate = if multi_labels == 0 {
                "  n/a".to_string()
            } else {
                format!("{:>5.1}%", split as f32 / multi_labels as f32 * 100.0)
            };
            let purity = hit as f32 / shots.len() as f32 * 100.0;
            println!(
                "{sim:.2}     {gap_min:>3}       {:>2}      {:>2}      {split_rate}   {purity:.1}%",
                cs.len(),
                false_merge
            );
        }
    }
    println!("\n提示：真实库校准看「拆分率」与「误并簇」两列，按产品目标（合并率≥80%、误并≤10%）选一组阈值。");
}