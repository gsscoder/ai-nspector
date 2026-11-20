#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod analysis;
mod error;
mod files;
mod llm;
mod modules;
mod overview;
mod parse;
mod scan;
mod settings;
mod store;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                // GNOME cannot resize a frameless window, so Linux keeps the native frame.
                #[cfg(target_os = "linux")]
                window.set_decorations(true)?;
                window.show()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan::open_repo,
            scan::list_recent_repos,
            files::read_file,
            modules::load_modules,
            modules::module_relations,
            modules::generate_modules,
            modules::cancel_generation,
            overview::load_overview,
            overview::generate_overview,
            analysis::list_analyses,
            analysis::start_project_analysis,
            analysis::create_project_agent_prompt,
            settings::get_settings,
            settings::save_settings,
            llm::test_connection,
        ])
        .run(tauri::generate_context!())
}
