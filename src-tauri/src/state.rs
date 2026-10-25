use rusqlite::Connection;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;

/// 扫描进度（前端轮询 + 事件推送）
#[derive(Clone, Serialize, Default)]
pub struct ScanStatus {
    pub running: bool,
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Serialize, Clone, Default)]
pub struct ScanSummary {
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub skipped: usize,
    pub images: usize,
    pub videos: usize,
    pub dup_groups: Option<usize>,
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub cache_dir: PathBuf,
    pub scan: Mutex<ScanStatus>,
    pub analyze: Mutex<ScanStatus>,
}
