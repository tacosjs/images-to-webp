mod commands;
mod converter;
mod watcher;

use std::sync::Mutex;
use tauri::Manager;
use watcher::WatcherHandle;

pub struct AppState {
    pub watcher: Mutex<Option<WatcherHandle>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            watcher: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::convert_batch,
            commands::start_watch,
            commands::stop_watch,
            commands::get_watch_status,
            commands::pick_folder,
        ])
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
