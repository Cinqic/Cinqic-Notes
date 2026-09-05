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
            commands::list_notes_filtered,
            commands::search_notes,
            commands::search_notes_filtered,
            commands::get_note,
            commands::create_note,
            commands::create_daily_note,
            commands::update_note,
            commands::rename_note,
            commands::move_note,
            commands::archive_note,
            commands::trash_note,
            commands::restore_note,
            commands::list_tasks,
            commands::toggle_task,
            commands::get_backlinks,
            commands::get_graph,
            commands::get_outgoing_links,
            commands::list_tags,
            commands::list_attachments,
            commands::import_attachment,
            commands::rebuild_index,
            commands::import_files,
            commands::export_note,
            commands::backup_library,
            commands::restore_backup,
            commands::empty_trash,
            commands::verify_integrity,
            commands::save_recovery_draft,
            commands::list_recovery_drafts,
            commands::read_recovery_draft,
            commands::remove_recovery_draft,
            commands::get_conflict,
            commands::resolve_conflict,
            commands::list_revisions,
            commands::restore_revision,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Cinqic Notes");
}
