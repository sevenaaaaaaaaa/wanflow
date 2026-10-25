//! 对真实库跑一次完整质量分析（第二连接，WAL 安全），用于性能验证。
use std::time::Instant;

fn main() {
    let db_path = dirs_home().join("Library/Caches/com.wanflow.app/wanflow.db");
    let mut conn = wanflow_lib::db::init(&db_path).expect("打开真实库失败");
    let start = Instant::now();
    let summary = wanflow_lib::analyze::run(&mut conn, &|done, total, current| {
        if done % 200 == 0 || done == total {
            println!("[{done}/{total}] {current}");
        }
    })
    .expect("分析失败");
    println!(
        "完成：评分 {} 张，连拍组 {}，建议 {} 条，耗时 {:.1}s",
        summary.scored,
        summary.bursts,
        summary.suggestions,
        start.elapsed().as_secs_f32()
    );
}

fn dirs_home() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
}
