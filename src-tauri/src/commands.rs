use crate::domain::{
    AttachmentItem, IntegrityInfo, LibraryInfo, LinkItem, NoteDocument, NoteFormat, NoteSummary,
    RecoveryDraftInfo, TagItem, TaskItem,
};
use crate::storage::{Library, StorageError};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct AppState {
    pub library: Mutex<Option<Library>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

fn failure(error: StorageError) -> String {
    error.to_string()
}

fn current_library(state: &State<'_, AppState>) -> Result<Library, String> {
    state
        .library
        .lock()
        .map_err(|_| "Library state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "Open a Notes Library first".to_owned())
}

fn preferences_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("settings.json"))
}

fn watch_library(
    state: &State<'_, AppState>,
    app: &AppHandle,
    library: &Library,
) -> Result<(), String> {
    let root = library.root.clone();
    let library = library.clone();
    let app = app.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<Event>| {
            if let Ok(event) = result {
                let changed_outside_index = event.paths.iter().any(|path| {
                    !path
                        .components()
                        .any(|component| component.as_os_str().to_string_lossy() == ".cinqic")
                });
                if changed_outside_index {
                    for path in &event.paths {
                        let _ = library.reconcile_path(path);
                    }
                    let _ = app.emit("library-changed", ());
                }
            }
        },
        Config::default(),
    )
    .map_err(|error| error.to_string())?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|error| error.to_string())?;
    state
        .watcher
        .lock()
        .map_err(|_| "Library watcher is unavailable".to_owned())?
        .replace(watcher);
    Ok(())
}

#[tauri::command]
pub fn get_last_library(app: AppHandle) -> Result<Option<String>, String> {
    let path = preferences_path(&app)?;
    let content = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    Ok(serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|value| {
            value
                .get("libraryPath")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
        }))
}

#[tauri::command]
pub fn set_last_library(app: AppHandle, path: String) -> Result<(), String> {
    let destination = preferences_path(&app)?;
    let temp = destination.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(
        &temp,
        serde_json::json!({ "libraryPath": path }).to_string(),
    )
    .map_err(|error| error.to_string())?;
    fs::rename(&temp, &destination)
        .or_else(|_| {
            fs::copy(&temp, &destination)
                .map(|_| ())
                .and_then(|_| fs::remove_file(&temp))
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_library(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<LibraryInfo, String> {
    let library = Library::create(path).map_err(failure)?;
    let info = library.info().map_err(failure)?;
    watch_library(&state, &app, &library)?;
    state
        .library
        .lock()
        .map_err(|_| "Library state is unavailable".to_owned())?
        .replace(library);
    Ok(info)
}

#[tauri::command]
pub fn open_library(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<LibraryInfo, String> {
    let library = Library::open(path).map_err(failure)?;
    let info = library.info().map_err(failure)?;
    watch_library(&state, &app, &library)?;
    state
        .library
        .lock()
        .map_err(|_| "Library state is unavailable".to_owned())?
        .replace(library);
    Ok(info)
}

#[tauri::command]
pub fn list_notes(
    state: State<'_, AppState>,
    include_trashed: bool,
) -> Result<Vec<NoteSummary>, String> {
    current_library(&state)?
        .list_notes(include_trashed)
        .map_err(failure)
}

#[tauri::command]
pub fn list_notes_filtered(
    state: State<'_, AppState>,
    include_trashed: bool,
    include_archived: bool,
) -> Result<Vec<NoteSummary>, String> {
    current_library(&state)?
        .list_notes_filtered(include_trashed, include_archived)
        .map_err(failure)
}

#[tauri::command]
pub fn search_notes(
    state: State<'_, AppState>,
    query: String,
    include_trashed: bool,
) -> Result<Vec<NoteSummary>, String> {
    current_library(&state)?
        .search_notes(&query, include_trashed)
        .map_err(failure)
}

#[tauri::command]
pub fn search_notes_filtered(
    state: State<'_, AppState>,
    query: String,
    include_trashed: bool,
    include_archived: bool,
) -> Result<Vec<NoteSummary>, String> {
    current_library(&state)?
        .search_notes_filtered(&query, include_trashed, include_archived)
        .map_err(failure)
}

#[tauri::command]
pub fn get_note(state: State<'_, AppState>, path: String) -> Result<NoteDocument, String> {
    current_library(&state)?.get_note(&path).map_err(failure)
}

#[tauri::command]
pub fn create_note(
    state: State<'_, AppState>,
    title: String,
    format: NoteFormat,
    folder: String,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .create_note(&title, format, &folder)
        .map_err(failure)
}

#[tauri::command]
pub fn create_daily_note(state: State<'_, AppState>, date: String) -> Result<NoteDocument, String> {
    current_library(&state)?
        .create_daily_note(&date)
        .map_err(failure)
}

#[tauri::command]
pub fn update_note(
    state: State<'_, AppState>,
    path: String,
    content: String,
    expected_hash: Option<String>,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .update_note(&path, &content, expected_hash.as_deref(), "local")
        .map_err(failure)
}

#[tauri::command]
pub fn rename_note(
    state: State<'_, AppState>,
    path: String,
    title: String,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .rename_note(&path, &title)
        .map_err(failure)
}

#[tauri::command]
pub fn move_note(
    state: State<'_, AppState>,
    path: String,
    folder: String,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .move_note(&path, &folder)
        .map_err(failure)
}

#[tauri::command]
pub fn archive_note(
    state: State<'_, AppState>,
    path: String,
    archived: bool,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .archive_note(&path, archived)
        .map_err(failure)
}

#[tauri::command]
pub fn trash_note(state: State<'_, AppState>, path: String) -> Result<(), String> {
    current_library(&state)?.trash_note(&path).map_err(failure)
}

#[tauri::command]
pub fn restore_note(state: State<'_, AppState>, path: String) -> Result<NoteDocument, String> {
    current_library(&state)?
        .restore_note(&path)
        .map_err(failure)
}

#[tauri::command]
pub fn list_tasks(
    state: State<'_, AppState>,
    include_complete: bool,
) -> Result<Vec<TaskItem>, String> {
    current_library(&state)?
        .list_tasks(include_complete)
        .map_err(failure)
}

#[tauri::command]
pub fn toggle_task(
    state: State<'_, AppState>,
    task_id: String,
    checked: bool,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .toggle_task(&task_id, checked)
        .map_err(failure)
}

#[tauri::command]
pub fn get_backlinks(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<crate::domain::BacklinkItem>, String> {
    current_library(&state)?.backlinks(&path).map_err(failure)
}

#[tauri::command]
pub fn get_graph(state: State<'_, AppState>) -> Result<crate::domain::GraphData, String> {
    current_library(&state)?.graph().map_err(failure)
}

#[tauri::command]
pub fn get_outgoing_links(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<LinkItem>, String> {
    current_library(&state)?
        .outgoing_links(&path)
        .map_err(failure)
}

#[tauri::command]
pub fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagItem>, String> {
    current_library(&state)?.list_tags().map_err(failure)
}

#[tauri::command]
pub fn list_attachments(state: State<'_, AppState>) -> Result<Vec<AttachmentItem>, String> {
    current_library(&state)?.list_attachments().map_err(failure)
}

#[tauri::command]
pub fn import_attachment(
    state: State<'_, AppState>,
    source: String,
) -> Result<AttachmentItem, String> {
    current_library(&state)?
        .import_attachment(&source)
        .map_err(failure)
}

#[tauri::command]
pub fn rebuild_index(state: State<'_, AppState>) -> Result<LibraryInfo, String> {
    current_library(&state)?.rebuild_index().map_err(failure)
}

#[tauri::command]
pub fn import_files(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<Vec<NoteSummary>, String> {
    current_library(&state)?
        .import_files(&paths)
        .map_err(failure)
}

#[tauri::command]
pub fn export_note(
    state: State<'_, AppState>,
    path: String,
    destination: String,
    format: String,
) -> Result<(), String> {
    current_library(&state)?
        .export_note(&path, &destination, &format)
        .map_err(failure)
}

#[tauri::command]
pub fn backup_library(
    state: State<'_, AppState>,
    destination: String,
    include_internal: bool,
) -> Result<(), String> {
    current_library(&state)?
        .backup_library(&destination, include_internal)
        .map_err(failure)
}

#[tauri::command]
pub fn restore_backup(
    state: State<'_, AppState>,
    archive_path: String,
    destination: String,
) -> Result<(), String> {
    current_library(&state)?
        .restore_backup(&archive_path, &destination)
        .map_err(failure)
}

#[tauri::command]
pub fn empty_trash(state: State<'_, AppState>) -> Result<u64, String> {
    current_library(&state)?.empty_trash().map_err(failure)
}

#[tauri::command]
pub fn verify_integrity(state: State<'_, AppState>) -> Result<IntegrityInfo, String> {
    current_library(&state)?.verify_integrity().map_err(failure)
}

#[tauri::command]
pub fn save_recovery_draft(
    state: State<'_, AppState>,
    note_path: String,
    content: String,
) -> Result<RecoveryDraftInfo, String> {
    current_library(&state)?
        .save_recovery_draft(&note_path, &content)
        .map_err(failure)
}

#[tauri::command]
pub fn list_recovery_drafts(state: State<'_, AppState>) -> Result<Vec<RecoveryDraftInfo>, String> {
    current_library(&state)?
        .list_recovery_drafts()
        .map_err(failure)
}

#[tauri::command]
pub fn read_recovery_draft(state: State<'_, AppState>, path: String) -> Result<String, String> {
    current_library(&state)?
        .read_recovery_draft(&path)
        .map_err(failure)
}

#[tauri::command]
pub fn remove_recovery_draft(state: State<'_, AppState>, path: String) -> Result<(), String> {
    current_library(&state)?
        .remove_recovery_draft(&path)
        .map_err(failure)
}

#[tauri::command]
pub fn get_conflict(
    state: State<'_, AppState>,
    path: String,
) -> Result<Option<crate::domain::ConflictInfo>, String> {
    current_library(&state)?.conflict(&path).map_err(failure)
}

#[tauri::command]
pub fn list_revisions(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<crate::domain::RevisionItem>, String> {
    current_library(&state)?.revisions(&path).map_err(failure)
}

#[tauri::command]
pub fn restore_revision(
    state: State<'_, AppState>,
    path: String,
    revision_id: String,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .restore_revision(&path, &revision_id)
        .map_err(failure)
}

#[tauri::command]
pub fn resolve_conflict(
    state: State<'_, AppState>,
    path: String,
    resolution: String,
) -> Result<NoteDocument, String> {
    current_library(&state)?
        .resolve_conflict(&path, &resolution)
        .map_err(failure)
}
