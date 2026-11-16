#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod error;
mod files;
mod llm;
mod modules;
mod overview;
mod parse;
mod scan;
mod settings;
mod store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scan::open_repo,
            scan::list_recent_repos,
            files::read_file,
            modules::load_modules,
            modules::generate_modules,
            modules::cancel_generation,
            overview::load_overview,
            overview::generate_overview,
            settings::get_settings,
            settings::save_settings,
            llm::test_connection,
        ])
        .run(tauri::generate_context!())
}
