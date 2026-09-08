use crate::domain::{
    self, AttachmentItem, BacklinkItem, ConflictInfo, GraphData, GraphEdge, GraphNode,
    IntegrityInfo, LibraryInfo, LinkItem, NoteDocument, NoteFormat, NoteSummary, RecoveryDraftInfo,
    RevisionItem, TagItem, TaskItem,
};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;
use zip::CompressionMethod;
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

const INDEX_DIR: &str = ".cinqic";
/// Sentinel fingerprint for a Library that currently contains no note files.
const EMPTY_LIBRARY_FINGERPRINT: &str = "empty";
const MAX_ARCHIVE_MEMBER_SIZE: u64 = 256 * 1024 * 1024;
const MAX_ARCHIVE_TOTAL_SIZE: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("note not found: {0}")]
    NotFound(String),
    #[error("conflict: this note changed outside Cinqic Notes")]
    Conflict,
    #[error("unsupported note format")]
    UnsupportedFormat,
    #[error("archive error: {0}")]
    Archive(String),
    #[error("{0}")]
    Message(String),
}

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Clone)]
pub struct Library {
    pub root: PathBuf,
    index: PathBuf,
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(StorageError::NotFound(path.display().to_string()));
        }
        if !path.is_dir() {
            return Err(StorageError::InvalidPath("Library must be a folder".into()));
        }
        let root = fs::canonicalize(path)?;
        let library = Self {
            index: root.join(INDEX_DIR).join("index.sqlite3"),
            root,
        };
        library.prepare()?;
        library.ensure_index_current()?;
        Ok(library)
    }

    pub fn create(path: impl AsRef<Path>) -> StorageResult<Self> {
        fs::create_dir_all(path.as_ref())?;
        Self::open(path)
    }

    fn prepare(&self) -> StorageResult<()> {
        fs::create_dir_all(self.root.join(INDEX_DIR).join("recovery"))?;
        // Revisions live in the `revisions` table, not on disk. The directory
        // this used to create was never written to.
        fs::create_dir_all(self.root.join(INDEX_DIR).join("trash"))?;
        let connection = self.connection()?;
        migrate(&connection)
    }

    fn connection(&self) -> StorageResult<Connection> {
        if let Some(parent) = self.index.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&self.index)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        migrate(&connection)?;
        Ok(connection)
    }

    pub fn info(&self) -> StorageResult<LibraryInfo> {
        let connection = self.connection()?;
        let note_count =
            connection.query_row("SELECT COUNT(*) FROM notes WHERE trashed = 0", [], |row| {
                row.get(0)
            })?;
        let last_indexed_at = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'last_indexed_at'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(now);
        Ok(LibraryInfo {
            path: self.root.to_string_lossy().into_owned(),
            note_count,
            last_indexed_at,
        })
    }

    /// Rebuild the index only when the Library's note files have changed.
    ///
    /// Opening a Library used to re-read, re-hash, and re-insert every note
    /// unconditionally, so start-up cost grew with Library size and was paid
    /// again on every launch — measured at roughly 13.6 s for 10,000 notes.
    ///
    /// The fingerprint covers exactly the files a rebuild would read, plus each
    /// one's size and modification time. Any difference at all — including a
    /// missing or unreadable fingerprint — falls through to the full rebuild,
    /// so the index remains reconstructible from the canonical files and
    /// nothing depends on the fingerprint being correct for safety.
    pub fn ensure_index_current(&self) -> StorageResult<LibraryInfo> {
        let fingerprint = self.index_fingerprint()?;
        let connection = self.connection()?;
        let stored: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = 'index_fingerprint'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let has_notes: i64 =
            connection.query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))?;
        drop(connection);
        if stored.as_deref() == Some(fingerprint.as_str())
            && (has_notes > 0 || fingerprint == EMPTY_LIBRARY_FINGERPRINT)
        {
            return self.info();
        }
        self.rebuild_index()
    }

    /// A cheap fingerprint of every note file's path, size, and modification
    /// time. Metadata only — file contents are never read here.
    fn index_fingerprint(&self) -> StorageResult<String> {
        let mut entries = Vec::new();
        collect_note_metadata(&self.root, &self.root, &mut entries)?;
        entries.sort();
        if entries.is_empty() {
            return Ok(EMPTY_LIBRARY_FINGERPRINT.to_owned());
        }
        let mut joined = String::new();
        for (path, size, modified) in entries {
            joined.push_str(&format!("{path}\u{1f}{size}\u{1f}{modified}\n"));
        }
        Ok(domain::hash_content(&joined))
    }

    pub fn rebuild_index(&self) -> StorageResult<LibraryInfo> {
        // Taken before the files are read so a change during the rebuild leaves
        // a fingerprint that no longer matches, forcing another rebuild rather
        // than recording a state that was never indexed.
        let fingerprint = self.index_fingerprint()?;
        let files = collect_note_files(&self.root)?;
        let mut connection = self.connection()?;
        let existing: HashMap<String, (String, String, bool)> = connection
            .prepare("SELECT path, id, created_at, archived FROM notes")?
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)? != 0,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(path, id, created_at, archived)| (path, (id, created_at, archived)))
            .collect();
        let transaction = connection.transaction()?;
        transaction.execute_batch("DELETE FROM links; DELETE FROM tasks; DELETE FROM note_tags; DELETE FROM notes_fts; DELETE FROM notes;")?;
        for (path, content) in &files {
            let relative = relative_path(&self.root, path)?;
            let prior = existing
                .get(&relative)
                .map(|(id, created_at, _)| (id.clone(), created_at.clone()));
            insert_note(&transaction, &relative, content, prior)?;
            if existing.get(&relative).is_some_and(|value| value.2) {
                transaction.execute(
                    "UPDATE notes SET archived = 1 WHERE path = ?1",
                    params![relative],
                )?;
            }
        }
        resolve_links(&transaction)?;
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES('last_indexed_at', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![now()],
        )?;
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES('index_fingerprint', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![fingerprint],
        )?;
        transaction.commit()?;
        self.info()
    }

    pub fn list_notes(&self, include_trashed: bool) -> StorageResult<Vec<NoteSummary>> {
        self.list_notes_filtered(include_trashed, true)
    }

    pub fn list_notes_filtered(
        &self,
        include_trashed: bool,
        include_archived: bool,
    ) -> StorageResult<Vec<NoteSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, path, title, format, preview, hash, modified_at, created_at, archived, trashed, project
             FROM notes WHERE (?1 = 1 OR trashed = 0) AND (?2 = 1 OR archived = 0) ORDER BY updated_at DESC, title COLLATE NOCASE",
        )?;
        let rows =
            statement.query_map(params![include_trashed, include_archived], summary_from_row)?;
        let mut result = rows.collect::<Result<Vec<_>, _>>()?;
        self.attach_summary_data_bulk(&connection, &mut result)?;
        if include_trashed {
            let mut trash = connection.prepare(
                "SELECT id, original_path, title, format, hash, removed_at, project FROM trash ORDER BY removed_at DESC",
            )?;
            let rows = trash.query_map([], |row| {
                let format: String = row.get(3)?;
                Ok(NoteSummary {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(2)?,
                    format: format_from_db(&format),
                    preview: String::new(),
                    hash: row.get(4)?,
                    modified_at: row.get(5)?,
                    created_at: row.get(5)?,
                    archived: false,
                    trashed: true,
                    project: row.get::<_, i64>(6)? != 0,
                    tags: Vec::new(),
                    task_count: 0,
                    open_task_count: 0,
                })
            })?;
            for row in rows {
                result.push(row?);
            }
        }
        Ok(result)
    }

    pub fn search_notes(
        &self,
        query: &str,
        include_trashed: bool,
    ) -> StorageResult<Vec<NoteSummary>> {
        self.search_notes_filtered(query, include_trashed, true)
    }

    pub fn search_notes_filtered(
        &self,
        query: &str,
        include_trashed: bool,
        include_archived: bool,
    ) -> StorageResult<Vec<NoteSummary>> {
        if query.trim().is_empty() {
            return self.list_notes_filtered(include_trashed, include_archived);
        }
        let connection = self.connection()?;
        let fts_query = query
            .split_whitespace()
            .map(|term| format!("\"{}\"*", term.replace('"', "")))
            .filter(|term| term != "\"\"*")
            .collect::<Vec<_>>()
            .join(" AND ");
        if fts_query.is_empty() {
            return self.list_notes_filtered(include_trashed, include_archived);
        }
        let mut statement = connection.prepare(
            "SELECT n.id, n.path, n.title, n.format, n.preview, n.hash, n.modified_at, n.created_at, n.archived, n.trashed, n.project
             FROM notes n JOIN notes_fts f ON f.note_id = n.id
             WHERE notes_fts MATCH ?1 AND (?2 = 1 OR n.trashed = 0) AND (?3 = 1 OR n.archived = 0)
             ORDER BY bm25(notes_fts, 5.0, 2.0, 1.0, 1.0) ASC, n.updated_at DESC",
        )?;
        let rows = statement.query_map(
            params![fts_query, include_trashed, include_archived],
            summary_from_row,
        )?;
        let mut result = rows.collect::<Result<Vec<_>, _>>()?;
        self.attach_summary_data_bulk(&connection, &mut result)?;
        Ok(result)
    }

    pub fn get_note(&self, path: &str) -> StorageResult<NoteDocument> {
        let absolute = self.safe_path(path)?;
        if !absolute.is_file() {
            return Err(StorageError::NotFound(path.into()));
        }
        let content = fs::read_to_string(&absolute)?;
        let connection = self.connection()?;
        let relative = relative_path(&self.root, &absolute)?;
        let indexed_hash = connection
            .query_row(
                "SELECT hash FROM notes WHERE path = ?1",
                params![relative],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let disk_hash = domain::hash_content(&content);
        if indexed_hash.as_deref() != Some(disk_hash.as_str()) {
            drop(connection);
            self.sync_file(&absolute, &content)?;
        } else {
            drop(connection);
        }
        let connection = self.connection()?;
        let summary = self.summary_by_path(&connection, &relative)?;
        Ok(NoteDocument { summary, content })
    }

    pub fn create_note(
        &self,
        title: &str,
        format: NoteFormat,
        folder: &str,
    ) -> StorageResult<NoteDocument> {
        let title = clean_title(title)?;
        let folder = self.safe_relative_folder(folder)?;
        let mut path = self
            .root
            .join(&folder)
            .join(format!("{}.{}", title, format.extension()));
        let mut suffix = 2;
        while path.exists() {
            path = self.root.join(&folder).join(format!(
                "{} {}.{}",
                title,
                suffix,
                format.extension()
            ));
            suffix += 1;
        }
        let content = if format == NoteFormat::Markdown {
            format!("# {title}\n\n")
        } else {
            String::new()
        };
        atomic_write(&path, &content)?;
        self.sync_file(&path, &content)?;
        self.get_note(&relative_path(&self.root, &path)?)
    }

    pub fn create_daily_note(&self, date: &str) -> StorageResult<NoteDocument> {
        let date = date.trim();
        if !is_calendar_date(date) {
            return Err(StorageError::InvalidPath("Invalid daily note date".into()));
        }
        let path = format!("{date}.md");
        if self.root.join(&path).is_file() {
            return self.get_note(&path);
        }
        self.create_note(date, NoteFormat::Markdown, "")
    }

    pub fn move_note(&self, path: &str, folder: &str) -> StorageResult<NoteDocument> {
        let absolute = self.safe_path(path)?;
        if !absolute.is_file() {
            return Err(StorageError::NotFound(path.into()));
        }
        let folder = self.safe_relative_folder(folder)?;
        let destination_directory = self.root.join(&folder);
        fs::create_dir_all(&destination_directory)?;
        let destination = destination_directory.join(
            absolute
                .file_name()
                .ok_or_else(|| StorageError::InvalidPath(path.into()))?,
        );
        if destination == absolute {
            return self.get_note(path);
        }
        if destination.exists() {
            return Err(StorageError::Message(
                "A note with that name already exists in the destination folder.".into(),
            ));
        }
        let old_relative = relative_path(&self.root, &absolute)?;
        fs::rename(&absolute, &destination)?;
        let new_relative = relative_path(&self.root, &destination)?;
        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE notes SET path = ?1, updated_at = ?2 WHERE path = ?3",
            params![new_relative, now(), old_relative],
        )?;
        transaction.execute(
            "UPDATE notes_fts SET path = ?1 WHERE note_id = (SELECT id FROM notes WHERE path = ?1)",
            params![new_relative],
        )?;
        transaction.execute(
            "UPDATE links SET target_path = ?1 WHERE target_path = ?2",
            params![new_relative, old_relative],
        )?;
        transaction.execute(
            "UPDATE trash SET original_path = ?1 WHERE original_path = ?2",
            params![new_relative, old_relative],
        )?;
        migrate_note_metadata(&transaction, &old_relative, &new_relative)?;
        resolve_links(&transaction)?;
        transaction.commit()?;
        drop(connection);
        self.migrate_recovery_drafts(&old_relative, &new_relative)?;
        self.get_note(&new_relative)
    }

    pub fn archive_note(&self, path: &str, archived: bool) -> StorageResult<NoteDocument> {
        let relative = self.safe_relative(path)?;
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE notes SET archived = ?1, updated_at = ?2 WHERE path = ?3",
            params![archived, now(), relative],
        )?;
        if changed == 0 {
            return Err(StorageError::NotFound(path.into()));
        }
        drop(connection);
        self.get_note(&relative)
    }

    pub fn reconcile_path(&self, path: &Path) -> StorageResult<()> {
        if !path.starts_with(&self.root)
            || path
                .components()
                .any(|component| component.as_os_str() == INDEX_DIR)
        {
            return Ok(());
        }
        let supports_notes = path
            .extension()
            .and_then(|value| value.to_str())
            .and_then(NoteFormat::from_extension)
            .is_some();
        if !supports_notes {
            return Ok(());
        }
        let relative = relative_path(&self.root, path)?;
        if path.is_file() {
            if fs::symlink_metadata(path)?.file_type().is_symlink() {
                return Ok(());
            }
            let content = fs::read_to_string(path)?;
            self.sync_file(path, &content)?;
            return Ok(());
        }
        let connection = self.connection()?;
        let note_id = connection
            .query_row(
                "SELECT id FROM notes WHERE path = ?1",
                params![relative],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(note_id) = note_id {
            remove_note_rows(&connection, &note_id)?;
            resolve_links(&connection)?;
        }
        Ok(())
    }

    pub fn update_note(
        &self,
        path: &str,
        content: &str,
        expected_hash: Option<&str>,
        actor: &str,
    ) -> StorageResult<NoteDocument> {
        let absolute = self.safe_path(path)?;
        if !absolute.is_file() {
            return Err(StorageError::NotFound(path.into()));
        }
        let relative = relative_path(&self.root, &absolute)?;
        let connection = self.connection()?;
        let stored_hash: String = connection.query_row(
            "SELECT hash FROM notes WHERE path = ?1",
            params![relative],
            |row| row.get(0),
        )?;
        let disk_content = fs::read_to_string(&absolute)?;
        let disk_hash = domain::hash_content(&disk_content);
        if expected_hash.is_some_and(|value| value != stored_hash) || disk_hash != stored_hash {
            store_conflict(&connection, &relative, content, &disk_content)?;
            return Err(StorageError::Conflict);
        }
        if disk_hash != domain::hash_content(content) {
            record_revision(
                &connection,
                &relative,
                &disk_content,
                &stored_hash,
                actor,
                "before update",
            )?;
            atomic_write(&absolute, content)?;
        }
        drop(connection);
        self.sync_file(&absolute, content)?;
        let connection = self.connection()?;
        connection.execute("DELETE FROM conflicts WHERE path = ?1", params![relative])?;
        drop(connection);
        // The note is now durable, so the interim recovery drafts for it are
        // just copies of the user's text sitting in `.cinqic/recovery/`.
        self.clear_recovery_drafts(&relative)?;
        self.get_note(path)
    }

    pub fn rename_note(&self, path: &str, title: &str) -> StorageResult<NoteDocument> {
        let absolute = self.safe_path(path)?;
        if !absolute.is_file() {
            return Err(StorageError::NotFound(path.into()));
        }
        let title = clean_title(title)?;
        let extension = absolute
            .extension()
            .and_then(|value| value.to_str())
            .ok_or(StorageError::UnsupportedFormat)?;
        let target = absolute
            .parent()
            .unwrap_or(&self.root)
            .join(format!("{title}.{extension}"));
        if target != absolute && target.exists() {
            return Err(StorageError::Message(
                "A note with that name already exists here.".into(),
            ));
        }
        if target != absolute {
            fs::rename(&absolute, &target)?;
        }
        let old_relative = relative_path(&self.root, &absolute)?;
        let new_relative = relative_path(&self.root, &target)?;
        let connection = self.connection()?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "UPDATE notes SET path = ?1, updated_at = ?2 WHERE path = ?3",
            params![new_relative, now(), old_relative],
        )?;
        transaction.execute(
            "UPDATE notes_fts SET path = ?1 WHERE note_id = (SELECT id FROM notes WHERE path = ?1)",
            params![new_relative],
        )?;
        transaction.execute(
            "UPDATE trash SET original_path = ?1 WHERE original_path = ?2",
            params![new_relative, old_relative],
        )?;
        migrate_note_metadata(&transaction, &old_relative, &new_relative)?;
        resolve_links(&transaction)?;
        transaction.commit()?;
        drop(connection);
        self.migrate_recovery_drafts(&old_relative, &new_relative)?;
        self.get_note(&new_relative)
    }

    pub fn trash_note(&self, path: &str) -> StorageResult<()> {
        let absolute = self.safe_path(path)?;
        let relative = relative_path(&self.root, &absolute)?;
        let content = fs::read_to_string(&absolute)?;
        let connection = self.connection()?;
        let summary = self.summary_by_path(&connection, &relative)?;
        let trash_name = format!(
            "{}__{}",
            Uuid::new_v4(),
            absolute
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("note")
        );
        let trash_path = self.root.join(INDEX_DIR).join("trash").join(trash_name);
        fs::create_dir_all(self.root.join(INDEX_DIR).join("trash"))?;

        // Stage every database change first, then move the file, and undo the
        // move if the commit fails. Moving the file before recording where it
        // went could leave a note sitting in `.cinqic/trash/` with no row to
        // restore it from, which the user has no way to recover through the app.
        let transaction = connection.unchecked_transaction()?;
        record_revision(
            &transaction,
            &relative,
            &content,
            &summary.hash,
            "local",
            "before trash",
        )?;
        transaction.execute("INSERT INTO trash(id, original_path, trash_path, title, format, hash, project, removed_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", params![summary.id, relative, trash_path.to_string_lossy(), summary.title, format_to_db(&summary.format), summary.hash, summary.project, now()])?;
        remove_note_rows(&transaction, &summary.id)?;
        fs::rename(&absolute, &trash_path)?;
        if let Err(error) = transaction.commit() {
            let _ = fs::rename(&trash_path, &absolute);
            return Err(error.into());
        }
        Ok(())
    }

    pub fn restore_note(&self, path: &str) -> StorageResult<NoteDocument> {
        let connection = self.connection()?;
        let item: (String, String) = connection.query_row(
            "SELECT trash_path, original_path FROM trash WHERE original_path = ?1",
            params![path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let target = self.safe_path(&item.1)?;
        if target.exists() {
            return Err(StorageError::Message(
                "A file already exists at the original location.".into(),
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&item.0, &target)?;
        connection.execute("DELETE FROM trash WHERE original_path = ?1", params![path])?;
        drop(connection);
        let content = fs::read_to_string(&target)?;
        self.sync_file(&target, &content)?;
        self.get_note(path)
    }

    pub fn list_tasks(&self, include_complete: bool) -> StorageResult<Vec<TaskItem>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT t.id, n.path, n.title, t.line, t.checked, t.text, n.project FROM tasks t JOIN notes n ON n.id = t.note_id WHERE (?1 = 1 OR t.checked = 0) ORDER BY t.checked, n.updated_at DESC, t.line",
        )?;
        let rows = statement.query_map(params![include_complete], |row| {
            Ok(TaskItem {
                id: row.get(0)?,
                note_path: row.get(1)?,
                note_title: row.get(2)?,
                line: row.get(3)?,
                checked: row.get::<_, i64>(4)? != 0,
                text: row.get(5)?,
                project: row.get::<_, i64>(6)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn toggle_task(&self, task_id: &str, checked: bool) -> StorageResult<NoteDocument> {
        let connection = self.connection()?;
        let (path, line_number): (String, i64) = connection.query_row(
            "SELECT n.path, t.line FROM tasks t JOIN notes n ON n.id = t.note_id WHERE t.id = ?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        drop(connection);
        let absolute = self.safe_path(&path)?;
        let current = fs::read_to_string(&absolute)?;
        let mut lines = current.lines().map(str::to_owned).collect::<Vec<_>>();
        let index = usize::try_from(line_number - 1)
            .map_err(|_| StorageError::Message("Invalid task line".into()))?;
        let line = lines
            .get_mut(index)
            .ok_or_else(|| StorageError::Message("Task line no longer exists".into()))?;
        let marker = line
            .find('[')
            .filter(|start| line.get(*start + 2..*start + 3) == Some("] "));
        let marker =
            marker.ok_or_else(|| StorageError::Message("Task marker no longer exists".into()))?;
        line.replace_range(marker + 1..marker + 2, if checked { "x" } else { " " });
        let mut next = lines.join("\n");
        if current.ends_with('\n') {
            next.push('\n');
        }
        self.update_note(&path, &next, Some(&domain::hash_content(&current)), "local")
    }

    pub fn backlinks(&self, path: &str) -> StorageResult<Vec<BacklinkItem>> {
        let connection = self.connection()?;
        let relative = self.safe_relative(path)?;
        let title = Path::new(path)
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or(path);
        let mut statement = connection.prepare(
            "SELECT n.path, n.title, l.kind, l.label FROM links l JOIN notes n ON n.id = l.source_id WHERE l.target_note_id = (SELECT id FROM notes WHERE path = ?1) OR l.target_path IN (?1, ?2) ORDER BY n.title COLLATE NOCASE",
        )?;
        let rows = statement.query_map(params![relative, format!("{title}.md")], |row| {
            Ok(BacklinkItem {
                source_path: row.get(0)?,
                source_title: row.get(1)?,
                kind: row.get(2)?,
                label: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn graph(&self) -> StorageResult<GraphData> {
        let connection = self.connection()?;
        let mut nodes_statement = connection.prepare("SELECT path, title, project FROM notes WHERE trashed = 0 ORDER BY title COLLATE NOCASE")?;
        let nodes = nodes_statement
            .query_map([], |row| {
                Ok(GraphNode {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    project: row.get::<_, i64>(2)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut edges_statement = connection.prepare("SELECT n.path, target.path, l.kind FROM links l JOIN notes n ON n.id = l.source_id JOIN notes target ON target.id = l.target_note_id WHERE n.trashed = 0 AND target.trashed = 0")?;
        let edges = edges_statement
            .query_map([], |row| {
                Ok(GraphEdge {
                    source: row.get(0)?,
                    target: row.get(1)?,
                    kind: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GraphData { nodes, edges })
    }

    pub fn outgoing_links(&self, path: &str) -> StorageResult<Vec<LinkItem>> {
        let relative = self.safe_relative(path)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT source.path, source.title, links.target_path, target.title, links.kind, links.label,
                    CASE WHEN target.id IS NULL THEN 0 ELSE 1 END
             FROM links
             JOIN notes source ON source.id = links.source_id
             LEFT JOIN notes target ON target.id = links.target_note_id
             WHERE source.path = ?1
             ORDER BY links.label COLLATE NOCASE",
        )?;
        let rows = statement.query_map(params![relative], |row| {
            Ok(LinkItem {
                source_path: row.get(0)?,
                source_title: row.get(1)?,
                target_path: row.get(2)?,
                target_title: row.get(3)?,
                kind: row.get(4)?,
                label: row.get(5)?,
                resolved: row.get::<_, i64>(6)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn list_tags(&self) -> StorageResult<Vec<TagItem>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT t.tag, COUNT(*) FROM note_tags t
             JOIN notes n ON n.id = t.note_id
             WHERE n.trashed = 0 AND n.archived = 0
             GROUP BY t.tag ORDER BY t.tag COLLATE NOCASE",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(TagItem {
                tag: row.get(0)?,
                note_count: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn list_attachments(&self) -> StorageResult<Vec<AttachmentItem>> {
        let mut files = Vec::new();
        let directory = self.root.join("_attachments");
        if directory.is_dir() {
            collect_attachment_files(&directory, &directory, &mut files)?;
        }
        Ok(files)
    }

    pub fn import_attachment(&self, source: &str) -> StorageResult<AttachmentItem> {
        let source = PathBuf::from(source);
        let metadata = fs::symlink_metadata(&source)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(StorageError::InvalidPath(
                "Attachments must be ordinary files".into(),
            ));
        }
        let name = clean_file_name(
            source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("attachment"),
        )?;
        let directory = self.root.join("_attachments");
        fs::create_dir_all(&directory)?;
        let destination = unique_file_path(&directory, &name);
        atomic_copy(&source, &destination)?;
        self.attachment_item(&destination)
    }

    pub fn empty_trash(&self) -> StorageResult<u64> {
        let connection = self.connection()?;
        let rows = connection
            .prepare("SELECT id, trash_path FROM trash")?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let trash_root = self.root.join(INDEX_DIR).join("trash");
        let canonical_trash_root = fs::canonicalize(&trash_root).unwrap_or(trash_root.clone());
        let mut validated_paths = Vec::new();
        for (id, value) in &rows {
            let path = PathBuf::from(value);
            if path.exists() {
                let canonical = fs::canonicalize(&path)?;
                if !canonical.starts_with(&canonical_trash_root) {
                    return Err(StorageError::Message(format!(
                        "Trash entry {id} contains an unsafe path; nothing was removed."
                    )));
                }
                validated_paths.push((id.clone(), Some(canonical)));
            } else {
                validated_paths.push((id.clone(), None));
            }
        }
        let mut removed = 0;
        for (id, path) in validated_paths {
            if let Some(path) = path
                && path.exists()
            {
                fs::remove_file(path)?;
            }
            connection.execute("DELETE FROM trash WHERE id = ?1", params![id])?;
            removed += 1;
        }
        Ok(removed)
    }

    pub fn verify_integrity(&self) -> StorageResult<IntegrityInfo> {
        let connection = self.connection()?;
        let result: String =
            connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        Ok(IntegrityInfo {
            ok: result.eq_ignore_ascii_case("ok"),
            message: result,
        })
    }

    pub fn save_recovery_draft(
        &self,
        note_path: &str,
        content: &str,
    ) -> StorageResult<RecoveryDraftInfo> {
        let note_path = self.safe_relative(note_path)?;
        let directory = self.root.join(INDEX_DIR).join("recovery");
        fs::create_dir_all(&directory)?;
        let draft_id = &domain::hash_content(&format!("{note_path}\n{content}"))[..16];
        let path = directory.join(format!("draft-{draft_id}.json"));
        let record = serde_json::json!({
            "notePath": note_path,
            "content": content,
            "createdAt": now(),
        });
        atomic_write(
            &path,
            &serde_json::to_string(&record).map_err(|error| {
                StorageError::Message(format!("Could not encode recovery draft: {error}"))
            })?,
        )?;
        self.recovery_draft_info(&path)
    }

    pub fn list_recovery_drafts(&self) -> StorageResult<Vec<RecoveryDraftInfo>> {
        let directory = self.root.join(INDEX_DIR).join("recovery");
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if let Ok(info) = self.recovery_draft_info(&path) {
                result.push(info);
            }
        }
        result.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(result)
    }

    pub fn read_recovery_draft(&self, draft_path: &str) -> StorageResult<String> {
        let path = self.safe_recovery_path(draft_path)?;
        let content = fs::read_to_string(path)?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|error| StorageError::Message(format!("Invalid recovery draft: {error}")))?;
        value
            .get("content")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .ok_or_else(|| StorageError::Message("Recovery draft has no content".into()))
    }

    pub fn remove_recovery_draft(&self, draft_path: &str) -> StorageResult<()> {
        let path = self.safe_recovery_path(draft_path)?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn backup_library(&self, destination: &str, include_internal: bool) -> StorageResult<()> {
        let destination = PathBuf::from(destination);
        ensure_backup_destination(&self.root, &destination)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = destination.with_file_name(format!(
            ".{}-tmp-{}",
            destination
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("backup.zip"),
            Uuid::new_v4()
        ));
        let result = (|| {
            let file = fs::File::create(&temporary)?;
            let mut archive = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let mut files = Vec::new();
            collect_library_files(&self.root, &self.root, &mut files)?;
            for (relative, content) in files {
                archive
                    .start_file(relative, options)
                    .map_err(|error| StorageError::Archive(error.to_string()))?;
                archive.write_all(&content)?;
            }
            if include_internal {
                let manifest = self.backup_manifest()?;
                archive
                    .start_file(".cinqic/backup-manifest.json", options)
                    .map_err(|error| StorageError::Archive(error.to_string()))?;
                archive.write_all(manifest.as_bytes())?;
            }
            archive
                .finish()
                .map_err(|error| StorageError::Archive(error.to_string()))?;
            replace_file(&temporary, &destination)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn restore_backup(&self, archive_path: &str, destination: &str) -> StorageResult<()> {
        let archive_path = PathBuf::from(archive_path);
        if !archive_path.is_file() {
            return Err(StorageError::NotFound(archive_path.display().to_string()));
        }
        let destination = PathBuf::from(destination);
        ensure_restore_destination(&self.root, &destination)?;
        if destination.exists() && fs::read_dir(&destination)?.next().transpose()?.is_some() {
            return Err(StorageError::Message(
                "Restore destination must be empty so no files are overwritten.".into(),
            ));
        }
        fs::create_dir_all(&destination)?;
        let file = fs::File::open(archive_path)?;
        let mut archive =
            ZipArchive::new(file).map_err(|error| StorageError::Archive(error.to_string()))?;
        let mut total_size = 0u64;
        for index in 0..archive.len() {
            let mut member = archive
                .by_index(index)
                .map_err(|error| StorageError::Archive(error.to_string()))?;
            if member.is_dir() {
                continue;
            }
            if member.size() > MAX_ARCHIVE_MEMBER_SIZE
                || total_size.saturating_add(member.size()) > MAX_ARCHIVE_TOTAL_SIZE
            {
                return Err(StorageError::Message(
                    "Backup exceeds the safe restore size limit.".into(),
                ));
            }
            let relative = safe_archive_member(member.name())?;
            let target = destination.join(&relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            std::io::copy(&mut member, &mut output)?;
            output.sync_all()?;
            total_size = total_size.saturating_add(member.size());
        }
        restore_backup_manifest(&destination)?;
        Ok(())
    }

    fn attachment_item(&self, path: &Path) -> StorageResult<AttachmentItem> {
        let metadata = fs::metadata(path)?;
        Ok(AttachmentItem {
            path: relative_path(&self.root, path)?,
            size: metadata.len(),
            modified_at: file_modified_at(&metadata),
        })
    }

    fn recovery_draft_info(&self, path: &Path) -> StorageResult<RecoveryDraftInfo> {
        let content = fs::read_to_string(path)?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|error| StorageError::Message(format!("Invalid recovery draft: {error}")))?;
        let created_at = value
            .get("createdAt")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned();
        let note_path = value
            .get("notePath")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_owned();
        Ok(RecoveryDraftInfo {
            path: relative_path(&self.root, path)?,
            note_path,
            created_at,
            size: value
                .get("content")
                .and_then(|value| value.as_str())
                .map(str::len)
                .unwrap_or(0) as u64,
        })
    }

    /// Rewrite the note path recorded inside recovery drafts after a rename or
    /// move, so a draft stays associated with the note it belongs to.
    fn migrate_recovery_drafts(&self, old: &str, new: &str) -> StorageResult<()> {
        if old == new {
            return Ok(());
        }
        self.each_recovery_draft(old, |path, mut value| {
            value["notePath"] = serde_json::Value::String(new.to_owned());
            let encoded = serde_json::to_string(&value).map_err(|error| {
                StorageError::Message(format!("Could not encode recovery draft: {error}"))
            })?;
            atomic_write(path, &encoded)
        })
    }

    /// Drop recovery drafts for a note whose content is now safely on disk.
    ///
    /// Only ever called *after* a canonical write has succeeded, so this never
    /// removes the sole copy of unsaved work.
    fn clear_recovery_drafts(&self, note_path: &str) -> StorageResult<()> {
        self.each_recovery_draft(note_path, |path, _| {
            if path.exists() {
                fs::remove_file(path)?;
            }
            Ok(())
        })
    }

    fn each_recovery_draft(
        &self,
        note_path: &str,
        mut action: impl FnMut(&Path, serde_json::Value) -> StorageResult<()>,
    ) -> StorageResult<()> {
        let directory = self.root.join(INDEX_DIR).join("recovery");
        if !directory.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
                continue;
            };
            if value.get("notePath").and_then(|value| value.as_str()) != Some(note_path) {
                continue;
            }
            action(&path, value)?;
        }
        Ok(())
    }

    fn safe_recovery_path(&self, draft_path: &str) -> StorageResult<PathBuf> {
        let normalized = draft_path.replace('\\', "/");
        let filename = normalized
            .strip_prefix(".cinqic/recovery/")
            .or_else(|| normalized.strip_prefix("recovery/"))
            .unwrap_or(&normalized);
        let candidate = Path::new(&normalized);
        if candidate.is_absolute()
            || candidate.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || filename.contains('/')
            || filename.is_empty()
        {
            return Err(StorageError::InvalidPath(draft_path.into()));
        }
        if Path::new(filename)
            .extension()
            .and_then(|value| value.to_str())
            != Some("json")
        {
            return Err(StorageError::InvalidPath(draft_path.into()));
        }
        Ok(self.root.join(INDEX_DIR).join("recovery").join(filename))
    }

    fn backup_manifest(&self) -> StorageResult<String> {
        let connection = self.connection()?;
        let mut revisions_statement = connection.prepare(
            "SELECT id, note_path, actor, reason, created_at, hash, content FROM revisions ORDER BY created_at",
        )?;
        let revisions = revisions_statement
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "notePath": row.get::<_, String>(1)?,
                    "actor": row.get::<_, String>(2)?,
                    "reason": row.get::<_, String>(3)?,
                    "createdAt": row.get::<_, String>(4)?,
                    "hash": row.get::<_, String>(5)?,
                    "content": row.get::<_, String>(6)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut settings_statement =
            connection.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let settings = settings_statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<HashMap<_, _>, _>>()?;
        serde_json::to_string_pretty(&serde_json::json!({
            "format": "cinqic-notes-backup",
            "version": 1,
            "createdAt": now(),
            "revisions": revisions,
            "settings": settings,
        }))
        .map_err(|error| {
            StorageError::Message(format!("Could not encode backup manifest: {error}"))
        })
    }

    pub fn import_files(&self, paths: &[String]) -> StorageResult<Vec<NoteSummary>> {
        let mut imported = Vec::new();
        for source in paths {
            let source = PathBuf::from(source);
            let format = source
                .extension()
                .and_then(|v| v.to_str())
                .and_then(NoteFormat::from_extension)
                .ok_or(StorageError::UnsupportedFormat)?;
            let content = fs::read_to_string(&source)?;
            let name = clean_title(
                source
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .unwrap_or("Imported note"),
            )?;
            let destination = unique_path(&self.root, &name, &format)?;
            atomic_write(&destination, &content)?;
            self.sync_file(&destination, &content)?;
            imported.push(
                self.get_note(&relative_path(&self.root, &destination)?)?
                    .summary,
            );
        }
        Ok(imported)
    }

    pub fn export_note(&self, path: &str, destination: &str, format: &str) -> StorageResult<()> {
        let note = self.get_note(path)?;
        let content = match format {
            "md" => note.content,
            "txt" => plain_text(&note.content),
            "html" => format!(
                "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>{}</title><body><pre>{}</pre></body></html>",
                html_escape(&note.summary.title),
                html_escape(&note.content)
            ),
            _ => return Err(StorageError::UnsupportedFormat),
        };
        atomic_write(Path::new(destination), &content)
    }

    pub fn conflict(&self, path: &str) -> StorageResult<Option<ConflictInfo>> {
        let connection = self.connection()?;
        let value = connection
            .query_row(
                "SELECT path, local_content, disk_content FROM conflicts WHERE path = ?1",
                params![path],
                |row| {
                    Ok(ConflictInfo {
                        path: row.get(0)?,
                        local_content: row.get(1)?,
                        disk_content: row.get(2)?,
                        message:
                            "The file changed outside Cinqic Notes. Choose which version to keep."
                                .into(),
                    })
                },
            )
            .optional()?;
        Ok(value)
    }

    pub fn revisions(&self, path: &str) -> StorageResult<Vec<RevisionItem>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id, note_path, actor, reason, created_at, content FROM revisions WHERE note_path = ?1 ORDER BY created_at DESC LIMIT 50")?;
        let rows = statement.query_map(params![path], |row| {
            Ok(RevisionItem {
                id: row.get(0)?,
                note_path: row.get(1)?,
                actor: row.get(2)?,
                reason: row.get(3)?,
                created_at: row.get(4)?,
                content: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn restore_revision(&self, path: &str, revision_id: &str) -> StorageResult<NoteDocument> {
        let connection = self.connection()?;
        let content: String = connection.query_row(
            "SELECT content FROM revisions WHERE id = ?1 AND note_path = ?2",
            params![revision_id, path],
            |row| row.get(0),
        )?;
        let absolute = self.safe_path(path)?;
        let current = fs::read_to_string(&absolute)?;
        let current_hash = domain::hash_content(&current);
        record_revision(
            &connection,
            path,
            &current,
            &current_hash,
            "local",
            "before revision restore",
        )?;
        atomic_write(&absolute, &content)?;
        connection.execute("DELETE FROM conflicts WHERE path = ?1", params![path])?;
        drop(connection);
        self.sync_file(&absolute, &content)?;
        self.get_note(path)
    }

    pub fn resolve_conflict(&self, path: &str, resolution: &str) -> StorageResult<NoteDocument> {
        // An unrecognised value must not quietly mean "keep local" — that would
        // discard the on-disk version on a typo or a stale caller.
        let resolution = ConflictResolution::parse(resolution)?;
        let connection = self.connection()?;
        let (local, disk): (String, String) = connection.query_row(
            "SELECT local_content, disk_content FROM conflicts WHERE path = ?1",
            params![path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let selected = match resolution {
            ConflictResolution::Disk => disk,
            ConflictResolution::Local => local,
        };
        drop(connection);
        let absolute = self.safe_path(path)?;
        let current = fs::read_to_string(&absolute)?;
        let current_hash = domain::hash_content(&current);
        let connection = self.connection()?;
        record_revision(
            &connection,
            path,
            &current,
            &current_hash,
            "local",
            "before conflict resolution",
        )?;
        atomic_write(&absolute, &selected)?;
        connection.execute("DELETE FROM conflicts WHERE path = ?1", params![path])?;
        drop(connection);
        self.sync_file(&absolute, &selected)?;
        self.get_note(path)
    }

    fn sync_file(&self, absolute: &Path, content: &str) -> StorageResult<()> {
        let relative = relative_path(&self.root, absolute)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let existing: Option<(String, String)> = transaction
            .query_row(
                "SELECT id, created_at FROM notes WHERE path = ?1",
                params![relative],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        insert_note(&transaction, &relative, content, existing)?;
        resolve_links(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    fn safe_relative(&self, path: &str) -> StorageResult<String> {
        let normalized = path.replace('\\', "/");
        let candidate = Path::new(&normalized);
        if has_windows_path_prefix(&normalized)
            || candidate.is_absolute()
            || candidate.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(StorageError::InvalidPath(path.into()));
        }
        if normalized.is_empty()
            || normalized.starts_with('.')
            || normalized.contains("/.cinqic")
            || normalized == INDEX_DIR
        {
            return Err(StorageError::InvalidPath(path.into()));
        }
        Ok(normalized)
    }

    fn safe_path(&self, path: &str) -> StorageResult<PathBuf> {
        let relative = self.safe_relative(path)?;
        if Path::new(&relative)
            .extension()
            .and_then(|value| value.to_str())
            .and_then(NoteFormat::from_extension)
            .is_none()
        {
            return Err(StorageError::UnsupportedFormat);
        }
        let absolute = self.root.join(&relative);
        if absolute.exists() {
            let canonical = fs::canonicalize(&absolute)?;
            if !canonical.starts_with(&self.root) {
                return Err(StorageError::InvalidPath(path.into()));
            }
        }
        Ok(absolute)
    }

    fn safe_relative_folder(&self, folder: &str) -> StorageResult<PathBuf> {
        if folder.trim().is_empty() {
            return Ok(PathBuf::new());
        }
        let relative = self.safe_relative(folder)?;
        Ok(PathBuf::from(relative))
    }

    fn summary_by_path(&self, connection: &Connection, path: &str) -> StorageResult<NoteSummary> {
        let row = connection.query_row("SELECT id, path, title, format, preview, hash, modified_at, created_at, archived, trashed, project FROM notes WHERE path = ?1", params![path], summary_from_row)?;
        self.attach_summary_data(connection, row)
    }

    /// Attach tags and task counts to a whole result set with two queries.
    ///
    /// `attach_summary_data` costs two queries and a statement preparation per
    /// row, so listing a large Library ran tens of thousands of queries —
    /// measured at 3.49 s for 10,000 notes against 0.03 s to open the same
    /// Library. Set-based lookups keep listing flat in the number of notes.
    fn attach_summary_data_bulk(
        &self,
        connection: &Connection,
        summaries: &mut [NoteSummary],
    ) -> StorageResult<()> {
        if summaries.is_empty() {
            return Ok(());
        }
        let mut tags_by_note: HashMap<String, Vec<String>> = HashMap::new();
        let mut statement =
            connection.prepare("SELECT note_id, tag FROM note_tags ORDER BY note_id, tag")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            tags_by_note
                .entry(row.get(0)?)
                .or_default()
                .push(row.get(1)?);
        }

        let mut counts_by_note: HashMap<String, (i64, i64)> = HashMap::new();
        let mut statement = connection.prepare(
            "SELECT note_id, COUNT(*), COALESCE(SUM(CASE WHEN checked = 0 THEN 1 ELSE 0 END), 0)
             FROM tasks GROUP BY note_id",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            counts_by_note.insert(row.get(0)?, (row.get(1)?, row.get(2)?));
        }

        for summary in summaries.iter_mut() {
            summary.tags = tags_by_note.remove(&summary.id).unwrap_or_default();
            let (total, open) = counts_by_note.get(&summary.id).copied().unwrap_or((0, 0));
            summary.task_count = total;
            summary.open_task_count = open;
        }
        Ok(())
    }

    fn attach_summary_data(
        &self,
        connection: &Connection,
        mut summary: NoteSummary,
    ) -> StorageResult<NoteSummary> {
        let mut tags =
            connection.prepare("SELECT tag FROM note_tags WHERE note_id = ?1 ORDER BY tag")?;
        summary.tags = tags
            .query_map(params![summary.id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        let counts: (i64, i64) = connection.query_row("SELECT COUNT(*), COALESCE(SUM(CASE WHEN checked = 0 THEN 1 ELSE 0 END), 0) FROM tasks WHERE note_id = ?1", params![summary.id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        summary.task_count = counts.0;
        summary.open_task_count = counts.1;
        Ok(summary)
    }
}

fn migrate(connection: &Connection) -> StorageResult<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS notes(id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL, format TEXT NOT NULL, preview TEXT NOT NULL, body TEXT NOT NULL, hash TEXT NOT NULL, modified_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0, trashed INTEGER NOT NULL DEFAULT 0, project INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE IF NOT EXISTS note_tags(note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE, tag TEXT NOT NULL, PRIMARY KEY(note_id, tag));
         CREATE TABLE IF NOT EXISTS links(id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE, target_path TEXT NOT NULL, target_note_id TEXT, kind TEXT NOT NULL, label TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY, note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE, line INTEGER NOT NULL, checked INTEGER NOT NULL, text TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS revisions(id TEXT PRIMARY KEY, note_path TEXT NOT NULL, content TEXT NOT NULL, hash TEXT NOT NULL, actor TEXT NOT NULL, reason TEXT NOT NULL, created_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS trash(id TEXT PRIMARY KEY, original_path TEXT NOT NULL UNIQUE, trash_path TEXT NOT NULL, title TEXT NOT NULL, format TEXT NOT NULL, hash TEXT NOT NULL, project INTEGER NOT NULL DEFAULT 0, removed_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS conflicts(path TEXT PRIMARY KEY, local_content TEXT NOT NULL, disk_content TEXT NOT NULL, created_at TEXT NOT NULL);
         CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(note_id UNINDEXED, title, path, body, tags);
         INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES(1, datetime('now'));",
    )?;
    Ok(())
}

fn insert_note(
    connection: &Connection,
    relative: &str,
    content: &str,
    existing: Option<(String, String)>,
) -> StorageResult<()> {
    let path = Path::new(relative);
    let format = path
        .extension()
        .and_then(|value| value.to_str())
        .and_then(NoteFormat::from_extension)
        .ok_or(StorageError::UnsupportedFormat)?;
    let parsed = domain::parse_note(path, content);
    let id = existing
        .as_ref()
        .map(|value| value.0.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let created_at = existing
        .as_ref()
        .map(|value| value.1.clone())
        .unwrap_or_else(now);
    let timestamp = now();
    let hash = domain::hash_content(content);
    connection.execute(
        "INSERT INTO notes(id, path, title, format, preview, body, hash, modified_at, created_at, updated_at, archived, trashed, project) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?8, 0, 0, ?10) ON CONFLICT(path) DO UPDATE SET title=excluded.title, format=excluded.format, preview=excluded.preview, body=excluded.body, hash=excluded.hash, modified_at=excluded.modified_at, updated_at=excluded.updated_at, project=excluded.project, trashed=0",
        params![id, relative, parsed.title, format_to_db(&format), domain::preview(content), content, hash, timestamp, created_at, parsed.project],
    )?;
    let note_id: String = connection.query_row(
        "SELECT id FROM notes WHERE path = ?1",
        params![relative],
        |row| row.get(0),
    )?;
    connection.execute("DELETE FROM note_tags WHERE note_id = ?1", params![note_id])?;
    for tag in parsed.tags {
        connection.execute(
            "INSERT OR IGNORE INTO note_tags(note_id, tag) VALUES(?1, ?2)",
            params![note_id, tag],
        )?;
    }
    connection.execute("DELETE FROM links WHERE source_id = ?1", params![note_id])?;
    for link in parsed.links {
        connection.execute(
            "INSERT INTO links(id, source_id, target_path, kind, label) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                note_id,
                domain::normalized_link_target(&link.target),
                link.kind,
                link.label
            ],
        )?;
    }
    connection.execute("DELETE FROM tasks WHERE note_id = ?1", params![note_id])?;
    for task in parsed.tasks {
        connection.execute(
            "INSERT INTO tasks(id, note_id, line, checked, text) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                note_id,
                task.line,
                task.checked,
                task.text
            ],
        )?;
    }
    connection.execute("DELETE FROM notes_fts WHERE note_id = ?1", params![note_id])?;
    let tags = connection.query_row(
        "SELECT COALESCE(group_concat(tag, ' '), '') FROM note_tags WHERE note_id = ?1",
        params![note_id],
        |row| row.get::<_, String>(0),
    )?;
    connection.execute(
        "INSERT INTO notes_fts(note_id, title, path, body, tags) VALUES(?1, ?2, ?3, ?4, ?5)",
        params![note_id, parsed.title, relative, content, tags],
    )?;
    Ok(())
}

fn resolve_links(connection: &Connection) -> StorageResult<()> {
    connection.execute("UPDATE links SET target_note_id = (SELECT id FROM notes WHERE notes.path = links.target_path)", [])?;
    Ok(())
}

fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<NoteSummary> {
    let format: String = row.get(3)?;
    Ok(NoteSummary {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        format: format_from_db(&format),
        preview: row.get(4)?,
        hash: row.get(5)?,
        modified_at: row.get(6)?,
        created_at: row.get(7)?,
        archived: row.get::<_, i64>(8)? != 0,
        trashed: row.get::<_, i64>(9)? != 0,
        project: row.get::<_, i64>(10)? != 0,
        tags: Vec::new(),
        task_count: 0,
        open_task_count: 0,
    })
}

fn remove_note_rows(connection: &Connection, id: &str) -> StorageResult<()> {
    connection.execute("DELETE FROM notes_fts WHERE note_id = ?1", params![id])?;
    connection.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    Ok(())
}

fn collect_note_files(root: &Path) -> StorageResult<Vec<(PathBuf, String)>> {
    let mut files = Vec::new();
    collect_note_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_note_files_inner(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(PathBuf, String)>,
) -> StorageResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if path.file_name().is_some_and(|value| value == INDEX_DIR) {
                continue;
            }
            collect_note_files_inner(root, &path, files)?;
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|v| v.to_str())
                .and_then(NoteFormat::from_extension)
                .is_some()
        {
            let _ = root;
            files.push((path.clone(), fs::read_to_string(path)?));
        }
    }
    Ok(())
}

/// Walk the same files `collect_note_files_inner` would, recording only
/// metadata. These two must stay in step; see `index_fingerprint`.
fn collect_note_metadata(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<(String, u64, i128)>,
) -> StorageResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if path.file_name().is_some_and(|value| value == INDEX_DIR) {
                continue;
            }
            collect_note_metadata(root, &path, entries)?;
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .and_then(NoteFormat::from_extension)
                .is_some()
        {
            let metadata = entry.metadata()?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| value.as_nanos() as i128)
                .unwrap_or(-1);
            entries.push((relative_path(root, &path)?, metadata.len(), modified));
        }
    }
    Ok(())
}

fn collect_attachment_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<AttachmentItem>,
) -> StorageResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_attachment_files(root, &path, files)?;
        } else if file_type.is_file() {
            let metadata = entry.metadata()?;
            files.push(AttachmentItem {
                path: relative_path(root, &path)?,
                size: metadata.len(),
                modified_at: file_modified_at(&metadata),
            });
        }
    }
    Ok(())
}

fn collect_library_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> StorageResult<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if path.file_name().is_some_and(|value| value == INDEX_DIR) {
                continue;
            }
            collect_library_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push((relative_path(root, &path)?, fs::read(&path)?));
        }
    }
    Ok(())
}

fn relative_path(root: &Path, path: &Path) -> StorageResult<String> {
    path.strip_prefix(root)
        .map_err(|_| StorageError::InvalidPath(path.display().to_string()))
        .map(|value| value.to_string_lossy().replace('\\', "/"))
}

fn clean_title(value: &str) -> StorageResult<String> {
    let value = value.trim();
    if value.is_empty() || value == "." || value == ".." {
        return Err(StorageError::InvalidPath(
            "A note needs a valid name".into(),
        ));
    }
    let cleaned = value
        .chars()
        .map(|character| {
            if "<>:\"/\\|?*".contains(character) || character.is_control() {
                '-'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_owned();
    if cleaned.is_empty() {
        return Err(StorageError::InvalidPath(
            "A note needs a valid name".into(),
        ));
    }
    Ok(cleaned)
}

fn unique_path(root: &Path, title: &str, format: &NoteFormat) -> StorageResult<PathBuf> {
    let mut path = root.join(format!("{}.{}", title, format.extension()));
    let mut suffix = 2;
    while path.exists() {
        path = root.join(format!("{} {}.{}", title, suffix, format.extension()));
        suffix += 1;
    }
    Ok(path)
}

fn clean_file_name(value: &str) -> StorageResult<String> {
    let cleaned = value
        .trim()
        .chars()
        .map(|character| {
            if "<>:\"/\\|?*".contains(character) || character.is_control() {
                '-'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_matches([' ', '.'])
        .to_owned();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return Err(StorageError::InvalidPath(
            "Attachment needs a valid file name".into(),
        ));
    }
    Ok(cleaned)
}

fn unique_file_path(directory: &Path, name: &str) -> PathBuf {
    let original = Path::new(name);
    let stem = original
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment");
    let extension = original
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let mut path = directory.join(name);
    let mut suffix = 2;
    while path.exists() {
        path = directory.join(format!("{stem} {suffix}{extension}"));
        suffix += 1;
    }
    path
}

fn atomic_write(path: &Path, content: &str) -> StorageResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path.file_name().and_then(|v| v.to_str()).unwrap_or("note");
    let temporary = path.with_file_name(format!(".{file_name}.tmp-{}", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    drop(file);
    replace_file(&temporary, path)
}

fn atomic_copy(source: &Path, destination: &Path) -> StorageResult<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment");
    let temporary = destination.with_file_name(format!(".{file_name}.tmp-{}", Uuid::new_v4()));
    fs::copy(source, &temporary)?;
    let file = OpenOptions::new().write(true).open(&temporary)?;
    file.sync_all()?;
    drop(file);
    replace_file(&temporary, destination)
}

/// Replace `destination` with `temporary` without ever exposing a partially
/// written file.
///
/// `fs::rename` is atomic within a directory on both platforms Cinqic Notes
/// targets: POSIX `rename(2)` replaces the destination atomically, and on
/// Windows `std` uses `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`. A failure
/// is therefore transient in practice (a Windows sharing violation while an
/// indexer or antivirus holds the file), so retry briefly.
///
/// There is deliberately no copy-over-the-destination fallback. A copy would
/// truncate the user's note first and could leave it half written if the
/// process died mid-write, which is exactly the outcome atomic replacement
/// exists to prevent. If replacement genuinely cannot happen we fail and leave
/// both the original note and the temporary file intact.
fn replace_file(temporary: &Path, destination: &Path) -> StorageResult<()> {
    let mut last_error = None;
    for attempt in 0..5 {
        match fs::rename(temporary, destination) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < 4 {
                    std::thread::sleep(std::time::Duration::from_millis(20 * (attempt + 1)));
                }
            }
        }
    }
    let _ = fs::remove_file(temporary);
    Err(StorageError::Io(last_error.unwrap_or_else(|| {
        std::io::Error::other("Could not replace the note file")
    })))
}

fn ensure_backup_destination(root: &Path, destination: &Path) -> StorageResult<()> {
    let mut probe = destination
        .parent()
        .ok_or_else(|| StorageError::InvalidPath(destination.display().to_string()))?;
    while !probe.exists() {
        probe = probe
            .parent()
            .ok_or_else(|| StorageError::InvalidPath(destination.display().to_string()))?;
    }
    if fs::canonicalize(probe)?.starts_with(root) {
        return Err(StorageError::InvalidPath(
            "Backup destination must be outside the active Library".into(),
        ));
    }
    Ok(())
}

fn ensure_restore_destination(root: &Path, destination: &Path) -> StorageResult<()> {
    if destination.exists() && fs::symlink_metadata(destination)?.file_type().is_symlink() {
        return Err(StorageError::InvalidPath(
            "Restore destination must not be a symlink".into(),
        ));
    }
    let mut probe = if destination.exists() {
        destination.to_owned()
    } else {
        destination
            .parent()
            .ok_or_else(|| StorageError::InvalidPath(destination.display().to_string()))?
            .to_owned()
    };
    while !probe.exists() {
        probe = probe
            .parent()
            .ok_or_else(|| StorageError::InvalidPath(destination.display().to_string()))?
            .to_owned();
    }
    if fs::canonicalize(probe)?.starts_with(root) {
        return Err(StorageError::InvalidPath(
            "Restore destination must be outside the active Library".into(),
        ));
    }
    Ok(())
}

fn restore_backup_manifest(destination: &Path) -> StorageResult<()> {
    let manifest_path = destination.join(INDEX_DIR).join("backup-manifest.json");
    if !manifest_path.is_file() {
        return Ok(());
    }
    let content = fs::read_to_string(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| StorageError::Archive(format!("Invalid backup manifest: {error}")))?;
    if manifest.get("format").and_then(|value| value.as_str()) != Some("cinqic-notes-backup")
        || manifest.get("version").and_then(|value| value.as_i64()) != Some(1)
    {
        return Err(StorageError::Archive(
            "Unsupported Cinqic Notes backup manifest".into(),
        ));
    }
    let library = Library::open(destination)?;
    let connection = library.connection()?;
    let transaction = connection.unchecked_transaction()?;
    if let Some(revisions) = manifest.get("revisions").and_then(|value| value.as_array()) {
        for revision in revisions {
            transaction.execute(
                "INSERT OR IGNORE INTO revisions(id, note_path, content, hash, actor, reason, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    revision.get("id").and_then(|value| value.as_str()).ok_or_else(|| StorageError::Archive("Backup revision has no id".into()))?,
                    revision.get("notePath").and_then(|value| value.as_str()).ok_or_else(|| StorageError::Archive("Backup revision has no note path".into()))?,
                    revision.get("content").and_then(|value| value.as_str()).ok_or_else(|| StorageError::Archive("Backup revision has no content".into()))?,
                    revision.get("hash").and_then(|value| value.as_str()).ok_or_else(|| StorageError::Archive("Backup revision has no hash".into()))?,
                    revision.get("actor").and_then(|value| value.as_str()).ok_or_else(|| StorageError::Archive("Backup revision has no actor".into()))?,
                    revision.get("reason").and_then(|value| value.as_str()).ok_or_else(|| StorageError::Archive("Backup revision has no reason".into()))?,
                    revision.get("createdAt").and_then(|value| value.as_str()).ok_or_else(|| StorageError::Archive("Backup revision has no timestamp".into()))?,
                ],
            )?;
        }
    }
    if let Some(settings) = manifest.get("settings").and_then(|value| value.as_object()) {
        for (key, value) in settings {
            if let Some(value) = value.as_str() {
                transaction.execute(
                    "INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )?;
            }
        }
    }
    transaction.commit()?;
    Ok(())
}

fn safe_archive_member(name: &str) -> StorageResult<PathBuf> {
    let normalized = name.replace('\\', "/");
    let candidate = Path::new(&normalized);
    if normalized.is_empty()
        || has_windows_path_prefix(&normalized)
        || candidate.is_absolute()
        || candidate.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(StorageError::InvalidPath(format!(
            "Unsafe archive member: {name}"
        )));
    }
    let is_manifest = normalized == ".cinqic/backup-manifest.json";
    if !is_manifest
        && normalized
            .split('/')
            .any(|part| part == INDEX_DIR || part.is_empty())
    {
        return Err(StorageError::InvalidPath(format!(
            "Unsafe archive member: {name}"
        )));
    }
    Ok(PathBuf::from(normalized))
}

fn file_modified_at(metadata: &fs::Metadata) -> String {
    metadata
        .modified()
        .map(chrono::DateTime::<Utc>::from)
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|_| now())
}

fn has_windows_path_prefix(value: &str) -> bool {
    value.starts_with("//")
        || (value.len() >= 2
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':')
}

/// Move every path-keyed safety record with a note.
///
/// `revisions`, `conflicts`, and recovery drafts are keyed by note path rather
/// than by note id, so without this a rename or move silently stranded a
/// note's entire revision history and any pending conflict under the old path.
/// The only conflict resolutions the storage layer accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep the buffer the editor was holding.
    Local,
    /// Keep the version currently on disk.
    Disk,
}

impl ConflictResolution {
    pub fn parse(value: &str) -> StorageResult<Self> {
        match value {
            "local" => Ok(Self::Local),
            "disk" => Ok(Self::Disk),
            other => Err(StorageError::InvalidPath(format!(
                "Unknown conflict resolution: {other}"
            ))),
        }
    }
}

/// Accept only real `YYYY-MM-DD` calendar dates.
///
/// The previous shape check accepted impossible values such as `2026-99-99`,
/// which would then become a note file named after a date that does not exist.
fn is_calendar_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        value[0..4].parse::<i32>(),
        value[5..7].parse::<u32>(),
        value[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    chrono::NaiveDate::from_ymd_opt(year, month, day).is_some()
}

fn migrate_note_metadata(connection: &Connection, old: &str, new: &str) -> StorageResult<()> {
    if old == new {
        return Ok(());
    }
    connection.execute(
        "UPDATE revisions SET note_path = ?1 WHERE note_path = ?2",
        params![new, old],
    )?;
    // `conflicts.path` is the primary key, so replace any conflict already
    // recorded at the destination rather than failing the rename.
    connection.execute(
        "UPDATE OR REPLACE conflicts SET path = ?1 WHERE path = ?2",
        params![new, old],
    )?;
    Ok(())
}

fn record_revision(
    connection: &Connection,
    path: &str,
    content: &str,
    hash: &str,
    actor: &str,
    reason: &str,
) -> StorageResult<()> {
    connection.execute("INSERT INTO revisions(id, note_path, content, hash, actor, reason, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![Uuid::new_v4().to_string(), path, content, hash, actor, reason, now()])?;
    Ok(())
}

fn store_conflict(
    connection: &Connection,
    path: &str,
    local: &str,
    disk: &str,
) -> StorageResult<()> {
    connection.execute("INSERT INTO conflicts(path, local_content, disk_content, created_at) VALUES(?1, ?2, ?3, ?4) ON CONFLICT(path) DO UPDATE SET local_content=excluded.local_content, disk_content=excluded.disk_content, created_at=excluded.created_at", params![path, local, disk, now()])?;
    Ok(())
}

fn plain_text(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            line.trim_start_matches('#')
                .trim_start_matches(['-', '*', '+'])
                .trim_start()
                .replace("**", "")
                .replace("__", "")
                .replace('`', "")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#039;")
}
fn format_to_db(format: &NoteFormat) -> &'static str {
    match format {
        NoteFormat::Markdown => "markdown",
        NoteFormat::Text => "text",
    }
}
fn format_from_db(format: &str) -> NoteFormat {
    if format == "text" {
        NoteFormat::Text
    } else {
        NoteFormat::Markdown
    }
}
fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_library() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("cinqic-notes-test-{suffix}"))
    }

    #[test]
    fn library_rebuilds_searchable_files_and_relationships() -> StorageResult<()> {
        let root = temp_library();
        fs::create_dir_all(&root)?;
        fs::write(
            root.join("Home.md"),
            "# Home\n\nSee [[Project]] and #welcome",
        )?;
        fs::write(root.join("Project.md"), "# Project\n\n- [ ] Ship it")?;
        let library = Library::open(&root)?;
        assert_eq!(library.list_notes(false)?.len(), 2);
        assert_eq!(library.search_notes("welcome", false)?.len(), 1);
        assert_eq!(library.backlinks("Project.md")?.len(), 1);
        assert_eq!(library.list_tasks(true)?.len(), 1);
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn optimistic_update_preserves_a_conflict() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("Conflict", NoteFormat::Markdown, "")?;
        fs::write(root.join(&note.summary.path), "changed externally")?;
        assert!(matches!(
            library.update_note(
                &note.summary.path,
                "my draft",
                Some(&note.summary.hash),
                "local"
            ),
            Err(StorageError::Conflict)
        ));
        assert!(library.conflict(&note.summary.path)?.is_some());
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn traversal_and_absolute_paths_are_rejected() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        assert!(library.get_note("../secret.md").is_err());
        assert!(library.get_note("C:\\secret.md").is_err());
        assert!(library.safe_relative("C:/secret.md").is_err());
        assert!(library.safe_relative("//server/share.md").is_err());
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn duplicate_names_unicode_and_trash_restore_are_safe() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let first = library.create_note("Résumé", NoteFormat::Markdown, "")?;
        let second = library.create_note("Résumé", NoteFormat::Markdown, "")?;
        assert_ne!(first.summary.path, second.summary.path);
        library.trash_note(&first.summary.path)?;
        assert!(
            library
                .list_notes(false)?
                .iter()
                .all(|note| note.path != first.summary.path)
        );
        assert!(
            library
                .list_notes(true)?
                .iter()
                .any(|note| note.path == first.summary.path && note.trashed)
        );
        library.restore_note(&first.summary.path)?;
        assert!(
            library
                .list_notes(false)?
                .iter()
                .any(|note| note.path == first.summary.path)
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn revisions_can_restore_previous_content_without_rewriting_history() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("History", NoteFormat::Markdown, "")?;
        let first_content = note.content.clone();
        let updated = library.update_note(
            &note.summary.path,
            "# History\n\nVersion two",
            Some(&note.summary.hash),
            "local",
        )?;
        let revisions = library.revisions(&updated.summary.path)?;
        assert_eq!(revisions.len(), 1);
        let restored = library.restore_revision(&updated.summary.path, &revisions[0].id)?;
        assert_eq!(restored.content, first_content);
        assert!(library.revisions(&restored.summary.path)?.len() >= 2);
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn archive_move_daily_notes_and_tags_stay_searchable() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let home = library.create_note("Home", NoteFormat::Markdown, "")?;
        let project = library.create_note("Project", NoteFormat::Markdown, "")?;
        library.update_note(
            &home.summary.path,
            "# Home\n\nSee [[Project]] and #planning",
            Some(&home.summary.hash),
            "local",
        )?;
        assert_eq!(library.list_tags()?[0].tag, "planning");
        assert_eq!(library.outgoing_links(&home.summary.path)?.len(), 1);
        assert!(library.outgoing_links(&home.summary.path)?[0].resolved);

        let moved = library.move_note(&project.summary.path, "projects")?;
        assert_eq!(moved.summary.path, "projects/Project.md");
        assert_eq!(
            library.search_notes("Project", false)?[0].path,
            moved.summary.path
        );

        let archived = library.archive_note(&moved.summary.path, true)?;
        assert!(archived.summary.archived);
        assert!(
            library
                .list_notes_filtered(false, false)?
                .iter()
                .all(|note| !note.archived)
        );
        assert!(
            library
                .list_notes_filtered(false, true)?
                .iter()
                .any(|note| note.path == moved.summary.path && note.archived)
        );

        let daily = library.create_daily_note("2026-09-05")?;
        assert_eq!(daily.summary.path, "2026-09-05.md");
        assert!(library.create_daily_note("2026-9-5").is_err());
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn attachments_recovery_backups_and_integrity_are_local_and_safe() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("Portable", NoteFormat::Markdown, "")?;
        let updated = library.update_note(
            &note.summary.path,
            "# Portable\n\nUpdated",
            Some(&note.summary.hash),
            "local",
        )?;
        let source = root.parent().unwrap().join(format!(
            "cinqic-attachment-{}-source.bin",
            std::process::id()
        ));
        fs::write(&source, b"attachment bytes")?;
        let attachment = library.import_attachment(&source.to_string_lossy())?;
        assert_eq!(library.list_attachments()?.len(), 1);
        assert_eq!(fs::read(root.join(&attachment.path))?, b"attachment bytes");

        let draft = library.save_recovery_draft(&note.summary.path, "# recovered")?;
        assert_eq!(library.read_recovery_draft(&draft.path)?, "# recovered");
        assert!(library.read_recovery_draft("../draft.json").is_err());

        let backup = root.parent().unwrap().join(format!(
            "cinqic-backup-{}-{}.zip",
            std::process::id(),
            domain::hash_content(&root.to_string_lossy())
        ));
        library.backup_library(&backup.to_string_lossy(), true)?;
        let restored_root = temp_library();
        library.restore_backup(&backup.to_string_lossy(), &restored_root.to_string_lossy())?;
        assert_eq!(
            fs::read_to_string(restored_root.join("Portable.md"))?,
            updated.content
        );
        assert_eq!(
            fs::read(restored_root.join(&attachment.path))?,
            b"attachment bytes"
        );
        let restored_library = Library::open(&restored_root)?;
        assert_eq!(restored_library.revisions("Portable.md")?.len(), 1);
        assert!(library.verify_integrity()?.ok);

        library.trash_note(&note.summary.path)?;
        assert_eq!(library.empty_trash()?, 1);
        assert!(library.list_notes(true)?.iter().all(|item| !item.trashed));

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(backup);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(restored_root);
        Ok(())
    }

    #[test]
    fn archive_member_validation_rejects_traversal_and_internal_paths() {
        assert!(safe_archive_member("../outside.md").is_err());
        assert!(safe_archive_member("C:/outside.md").is_err());
        assert!(safe_archive_member(".cinqic/index.sqlite3").is_err());
        assert!(safe_archive_member("nested/note.md").is_ok());
        assert!(safe_archive_member(".cinqic/backup-manifest.json").is_ok());
    }

    #[test]
    fn renaming_a_note_keeps_its_revision_history_reachable() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("History", NoteFormat::Markdown, "")?;
        let path = note.summary.path.clone();

        let updated = library.update_note(&path, "# History\n\nfirst\n", None, "local")?;
        library.update_note(
            &path,
            "# History\n\nsecond\n",
            Some(&updated.summary.hash),
            "local",
        )?;
        let before = library.revisions(&path)?;
        assert!(!before.is_empty(), "expected revisions before the rename");

        let renamed = library.rename_note(&path, "History Renamed")?;
        assert_ne!(renamed.summary.path, path);

        let after = library.revisions(&renamed.summary.path)?;
        assert_eq!(
            after.len(),
            before.len(),
            "revision history must follow the note across a rename"
        );
        assert!(library.revisions(&path)?.is_empty());

        // The same must hold for a move into a folder.
        let moved = library.move_note(&renamed.summary.path, "Archive")?;
        assert_eq!(library.revisions(&moved.summary.path)?.len(), before.len());

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn renaming_a_note_carries_a_pending_conflict_with_it() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("Contested", NoteFormat::Markdown, "")?;
        let path = note.summary.path.clone();

        // An external editor changes the file behind the app's back.
        fs::write(root.join(&path), "# Contested\n\nexternal\n")?;
        let outcome = library.update_note(&path, "# Contested\n\nlocal\n", None, "local");
        assert!(matches!(outcome, Err(StorageError::Conflict)));
        assert!(library.conflict(&path)?.is_some());

        let renamed = library.rename_note(&path, "Contested Renamed")?;
        assert!(
            library.conflict(&renamed.summary.path)?.is_some(),
            "an unresolved conflict must not be stranded under the old path"
        );
        assert!(library.conflict(&path)?.is_none());

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn conflict_resolution_rejects_an_unknown_value() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("Choice", NoteFormat::Markdown, "")?;
        let path = note.summary.path.clone();

        fs::write(root.join(&path), "# Choice\n\ndisk\n")?;
        let _ = library.update_note(&path, "# Choice\n\nlocal\n", None, "local");
        assert!(library.conflict(&path)?.is_some());

        // A typo must not be silently treated as "keep local".
        assert!(library.resolve_conflict(&path, "Local").is_err());
        assert!(library.resolve_conflict(&path, "").is_err());
        assert!(library.resolve_conflict(&path, "theirs").is_err());
        assert!(library.conflict(&path)?.is_some());

        let resolved = library.resolve_conflict(&path, "disk")?;
        assert!(resolved.content.contains("disk"));
        assert!(library.conflict(&path)?.is_none());

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn daily_note_dates_must_be_real_calendar_dates() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;

        for invalid in [
            "2026-99-99",
            "2026-13-01",
            "2026-02-30",
            "2025-02-29",
            "0000-00-00",
            "2026-1-01",
            "not-a-date",
        ] {
            assert!(
                library.create_daily_note(invalid).is_err(),
                "{invalid} must be rejected"
            );
        }

        assert!(library.create_daily_note("2026-02-28").is_ok());
        assert!(library.create_daily_note("2024-02-29").is_ok());

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn a_successful_save_retires_recovery_drafts_for_that_note() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("Draft", NoteFormat::Markdown, "")?;
        let path = note.summary.path.clone();
        let other = library.create_note("Other", NoteFormat::Markdown, "")?;

        library.save_recovery_draft(&path, "# Draft\n\nin progress\n")?;
        library.save_recovery_draft(&other.summary.path, "# Other\n\nalso in progress\n")?;
        assert_eq!(library.list_recovery_drafts()?.len(), 2);

        library.update_note(&path, "# Draft\n\nsaved\n", None, "local")?;

        let remaining = library.list_recovery_drafts()?;
        assert_eq!(
            remaining.len(),
            1,
            "only the saved note's drafts are retired"
        );
        assert_eq!(remaining[0].note_path, other.summary.path);

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn recovery_drafts_follow_a_renamed_note() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("Movable", NoteFormat::Markdown, "")?;
        let path = note.summary.path.clone();
        library.save_recovery_draft(&path, "# Movable\n\nunsaved\n")?;

        let renamed = library.rename_note(&path, "Movable Renamed")?;
        let drafts = library.list_recovery_drafts()?;

        assert_eq!(drafts.len(), 1);
        assert_eq!(
            drafts[0].note_path, renamed.summary.path,
            "a draft must stay attached to the note it belongs to"
        );

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn trashing_records_the_note_before_the_file_moves() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        let note = library.create_note("Disposable", NoteFormat::Markdown, "")?;
        let path = note.summary.path.clone();

        library.trash_note(&path)?;

        // The file left the Library, and the trash row that makes it
        // recoverable exists, so restore is always possible.
        assert!(!root.join(&path).exists());
        let restored = library.restore_note(&path)?;
        assert_eq!(restored.summary.path, path);
        assert!(root.join(&path).is_file());

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn a_full_backup_round_trips_notes_attachments_and_revisions() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;

        let note = library.create_note("Round Trip", NoteFormat::Markdown, "Projects")?;
        let path = note.summary.path.clone();
        let first = library.update_note(&path, "# Round Trip\n\nfirst\n", None, "local")?;
        library.update_note(
            &path,
            "# Round Trip\n\nsecond\n",
            Some(&first.summary.hash),
            "local",
        )?;
        library.create_note("Ünïcode ✓", NoteFormat::Text, "Nested/Deeper")?;
        let revisions_before = library.revisions(&path)?.len();
        assert!(revisions_before > 0);

        let archive = root.with_extension("full-backup.zip");
        library.backup_library(&archive.to_string_lossy(), true)?;
        assert!(archive.is_file());

        let restored_root = temp_library();
        library.restore_backup(&archive.to_string_lossy(), &restored_root.to_string_lossy())?;
        let restored = Library::open(&restored_root)?;

        assert_eq!(
            restored.get_note(&path)?.content,
            "# Round Trip\n\nsecond\n"
        );
        assert!(restored_root.join("Nested/Deeper").is_dir());
        assert_eq!(
            restored.revisions(&path)?.len(),
            revisions_before,
            "a full backup must carry revision history"
        );

        let _ = fs::remove_file(archive);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(restored_root);
        Ok(())
    }

    #[test]
    fn restore_refuses_unsafe_destinations() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        library.create_note("Anything", NoteFormat::Markdown, "")?;
        let archive = root.with_extension("backup.zip");
        library.backup_library(&archive.to_string_lossy(), false)?;

        // Inside the active Library.
        let inside = root.join("restored");
        assert!(
            library
                .restore_backup(&archive.to_string_lossy(), &inside.to_string_lossy())
                .is_err(),
            "restoring into the active Library must be refused"
        );

        // A destination that already has content in it.
        let occupied = temp_library();
        fs::create_dir_all(&occupied)?;
        fs::write(occupied.join("existing.md"), "do not overwrite me\n")?;
        assert!(
            library
                .restore_backup(&archive.to_string_lossy(), &occupied.to_string_lossy())
                .is_err(),
            "restoring over existing files must be refused"
        );
        assert_eq!(
            fs::read_to_string(occupied.join("existing.md"))?,
            "do not overwrite me\n"
        );

        let _ = fs::remove_file(archive);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(occupied);
        Ok(())
    }

    #[test]
    fn reopening_an_unchanged_library_skips_the_rebuild_but_stays_correct() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        library.create_note("Stable", NoteFormat::Markdown, "")?;
        library.create_note("Also Stable", NoteFormat::Markdown, "Folder")?;

        let reopened = Library::open(&root)?;
        assert_eq!(reopened.list_notes(false)?.len(), 2);
        assert_eq!(reopened.search_notes("Stable", false)?.len(), 2);

        // A file added outside the app must still be picked up on the next open.
        fs::write(root.join("External.md"), "# External\n\n#outside\n")?;
        let after_add = Library::open(&root)?;
        assert_eq!(after_add.list_notes(false)?.len(), 3);
        assert_eq!(after_add.search_notes("outside", false)?.len(), 1);

        // As must a change to an existing file's contents.
        fs::write(root.join("External.md"), "# External\n\n#changed\n")?;
        let after_edit = Library::open(&root)?;
        assert_eq!(after_edit.search_notes("changed", false)?.len(), 1);
        assert_eq!(after_edit.search_notes("outside", false)?.len(), 0);

        // And a deletion.
        fs::remove_file(root.join("External.md"))?;
        let after_delete = Library::open(&root)?;
        assert_eq!(after_delete.list_notes(false)?.len(), 2);

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn a_discarded_index_is_rebuilt_from_the_canonical_files() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        library.create_note("Recoverable", NoteFormat::Markdown, "")?;
        library.create_note("Second", NoteFormat::Markdown, "Nested")?;
        drop(library);

        // Deleting the database must never cost the user their notes: the next
        // open reconstructs the index from the Markdown files themselves.
        fs::remove_file(root.join(INDEX_DIR).join("index.sqlite3"))?;
        let reopened = Library::open(&root)?;

        assert_eq!(reopened.list_notes(false)?.len(), 2);
        assert_eq!(reopened.search_notes("Recoverable", false)?.len(), 1);

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn the_fingerprint_ignores_internal_state_and_tracks_note_files() -> StorageResult<()> {
        let root = temp_library();
        let library = Library::create(&root)?;
        library.create_note("Tracked", NoteFormat::Markdown, "")?;
        let before = library.index_fingerprint()?;

        // Writing internal state must not invalidate the fingerprint.
        library.save_recovery_draft("Tracked.md", "draft text")?;
        assert_eq!(library.index_fingerprint()?, before);

        // A new note file must.
        fs::write(root.join("New.md"), "# New\n")?;
        assert_ne!(library.index_fingerprint()?, before);

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn calendar_date_validation_is_strict() {
        assert!(is_calendar_date("2026-09-08"));
        assert!(is_calendar_date("2024-02-29"));
        assert!(!is_calendar_date("2025-02-29"));
        assert!(!is_calendar_date("2026-00-10"));
        assert!(!is_calendar_date("2026-12-32"));
        assert!(!is_calendar_date("2026-09-08 "));
        assert!(!is_calendar_date("20260908"));
    }

    #[test]
    fn conflict_resolution_parses_only_known_values() {
        assert_eq!(
            ConflictResolution::parse("local").unwrap(),
            ConflictResolution::Local
        );
        assert_eq!(
            ConflictResolution::parse("disk").unwrap(),
            ConflictResolution::Disk
        );
        assert!(ConflictResolution::parse("LOCAL").is_err());
        assert!(ConflictResolution::parse("keep").is_err());
    }
}
