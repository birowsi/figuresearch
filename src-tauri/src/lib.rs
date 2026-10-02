//! Tauri backend: runs searches through `search-service` and streams each
//! store's result to the UI via the `search-store` event.

use search_service::{SearchHooks, SearchSummary, SiteCheck};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager};

fn latest_search() -> &'static Mutex<String> {
    static LATEST: OnceLock<Mutex<String>> = OnceLock::new();
    LATEST.get_or_init(|| Mutex::new(String::new()))
}

fn is_current(search_id: &str) -> bool {
    latest_search().lock().map(|id| *id == search_id).unwrap_or(true)
}

#[tauri::command]
async fn search_stores(app: AppHandle, search_id: String, term: String, stores: Vec<String>) -> Result<SearchSummary, String> {
    if let Ok(mut latest) = latest_search().lock() {
        *latest = search_id.clone();
    }
    let id = search_id.clone();
    let emitter = app.clone();
    let hooks = SearchHooks {
        capture_dir: app.path().app_local_data_dir().ok().map(|base| base.join("logs")),
        is_current: Arc::new(move || is_current(&id)),
        on_result: Arc::new(move |result| {
            let _ = emitter.emit("search-store", result);
        }),
    };
    search_service::search_stores(search_id, term, stores, hooks).await
}

#[tauri::command]
async fn check_sites() -> HashMap<String, SiteCheck> {
    search_service::check_sites().await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![search_stores, check_sites])
        .setup(|app| {
            app.handle().plugin(tauri_plugin_log::Builder::default().level(log::LevelFilter::Info).build())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
