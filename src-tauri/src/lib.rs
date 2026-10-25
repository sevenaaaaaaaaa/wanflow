pub mod analyze;
pub mod clip;
pub mod commands;
pub mod db;
pub mod phash;
mod quality;
pub mod scanner;
mod smart;
mod state;
mod thumbs;
mod timefix;
pub mod webdav;

use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cache = app.path().app_cache_dir()?;
            std::fs::create_dir_all(&cache)?;
            let thumbs = cache.join("thumbs");
            std::fs::create_dir_all(&thumbs)?;
            let conn = db::init(&cache.join("wanflow.db")).map_err(|e| {
                anyhow_err(format!("数据库初始化失败: {e}"))
            })?;
            app.manage(state::AppState {
                db: Mutex::new(conn),
                cache_dir: cache,
                scan: Mutex::new(state::ScanStatus::default()),
                analyze: Mutex::new(state::ScanStatus::default()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::import_folder,
            commands::import_photos_library,
            commands::import_webdav,
            commands::rescan_source,
            commands::unlink_source,
            commands::list_sources,
            commands::get_scan_status,
            commands::list_media,
            commands::stats,
            commands::set_decision,
            commands::undo_last,
            commands::redo_last,
            commands::history,
            commands::batch_time_preview,
            commands::batch_time_apply,
            commands::dup_members,
            commands::burst_members,
            commands::analyze_all,
            commands::get_suggestions,
            commands::resolve_suggestions,
            smart::clip_status,
            smart::clip_download,
            smart::embed_all,
            smart::semantic_search,
            smart::smart_select,
            smart::list_albums,
            smart::album_media,
            smart::create_album,
            smart::delete_album,
        ])
        .run(tauri::generate_context!())
        .expect("WanFlow 启动失败");
}

fn anyhow_err(msg: String) -> Box<dyn std::error::Error> {
    Box::<dyn std::error::Error>::from(msg)
}
