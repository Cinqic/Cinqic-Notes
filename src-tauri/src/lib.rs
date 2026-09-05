#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod domain;
mod storage;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_last_library,
            commands::set_last_library,
            commands::create_library,
            commands::open_library,
            commands::list_notes,
            commands::search_notes,
            commands::get_note,
            commands::create_note,
            commands::update_note,
            commands::rename_note,
            commands::trash_note,
            commands::restore_note,
            commands::list_tasks,
            commands::toggle_task,
            commands::get_backlinks,
            commands::get_graph,
            commands::rebuild_index,
            commands::import_files,
            commands::export_note,
            commands::get_conflict,
            commands::resolve_conflict,
            commands::list_revisions,
            commands::restore_revision
        ])
        .run(tauri::generate_context!())
        .expect("error while running Cinqic Notes");
}
