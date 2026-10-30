mod commands;
mod db;
mod error;
mod feed;
mod fulltext;
mod image_cache;
mod models;
mod scheduler;
mod state;

use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let pool = db::init_pool(&data_dir).map_err(|e| e.to_string())?;
            let app_state = AppState::new(pool, data_dir).map_err(|e| e.to_string())?;
            app.manage(app_state);

            // Kick off the background scheduler (startup fetch + periodic).
            scheduler::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_feeds,
            commands::create_folder,
            commands::create_feed,
            commands::update_feed,
            commands::delete_feed,
            commands::list_articles,
            commands::get_article,
            commands::mark_read,
            commands::mark_unread,
            commands::set_starred,
            commands::mark_all_read,
            commands::refresh_feed,
            commands::refresh_all,
            commands::export_opml,
            commands::import_opml,
            commands::get_settings,
            commands::update_settings,
            commands::prefetch_fulltext,
            commands::cache_images,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
