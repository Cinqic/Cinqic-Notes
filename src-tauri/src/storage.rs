use crate::domain::{
    self, BacklinkItem, ConflictInfo, GraphData, GraphEdge, GraphNode, LibraryInfo, NoteDocument,
    NoteFormat, NoteSummary, RevisionItem, TaskItem,
};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const INDEX_DIR: &str = ".cinqic";

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
        library.rebuild_index()?;
        Ok(library)
    }

    pub fn create(path: impl AsRef<Path>) -> StorageResult<Self> {
        fs::create_dir_all(path.as_ref())?;
        Self::open(path)
    }

    fn prepare(&self) -> StorageResult<()> {
        fs::create_dir_all(self.root.join(INDEX_DIR).join("recovery"))?;
        fs::create_dir_all(self.root.join(INDEX_DIR).join("revisions"))?;
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

    pub fn rebuild_index(&self) -> StorageResult<LibraryInfo> {
        let files = collect_note_files(&self.root)?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute_batch("DELETE FROM links; DELETE FROM tasks; DELETE FROM note_tags; DELETE FROM notes_fts; DELETE FROM notes;")?;
        for (path, content) in &files {
            let relative = relative_path(&self.root, path)?;
            insert_note(&transaction, &relative, content, None)?;
        }
        resolve_links(&transaction)?;
        transaction.execute(
            "INSERT INTO settings(key, value) VALUES('last_indexed_at', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![now()],
        )?;
        transaction.commit()?;
        self.info()
    }

    pub fn list_notes(&self, include_trashed: bool) -> StorageResult<Vec<NoteSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, path, title, format, preview, hash, modified_at, created_at, archived, trashed, project
             FROM notes WHERE (?1 = 1 OR trashed = 0) ORDER BY updated_at DESC, title COLLATE NOCASE",
        )?;
        let rows = statement.query_map(params![include_trashed], summary_from_row)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(self.attach_summary_data(&connection, row?)?);
        }
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
        if query.trim().is_empty() {
            return self.list_notes(include_trashed);
        }
        let connection = self.connection()?;
        let fts_query = query
            .split_whitespace()
            .map(|term| format!("\"{}\"*", term.replace('"', "")))
            .filter(|term| term != "\"\"*")
            .collect::<Vec<_>>()
            .join(" AND ");
        if fts_query.is_empty() {
            return self.list_notes(include_trashed);
        }
        let mut statement = connection.prepare(
            "SELECT n.id, n.path, n.title, n.format, n.preview, n.hash, n.modified_at, n.created_at, n.archived, n.trashed, n.project
             FROM notes n JOIN notes_fts f ON f.note_id = n.id
             WHERE notes_fts MATCH ?1 AND (?2 = 1 OR n.trashed = 0)
             ORDER BY bm25(notes_fts, 5.0, 2.0, 1.0, 1.0) ASC, n.updated_at DESC",
        )?;
        let rows = statement.query_map(params![fts_query, include_trashed], |row| {
            summary_from_row(row)
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(self.attach_summary_data(&connection, row?)?);
        }
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
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM notes WHERE path = ?1)",
            params![relative],
            |row| row.get(0),
        )?;
        drop(connection);
        if !exists {
            self.sync_file(&absolute, &content)?;
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
        connection.execute(
            "UPDATE notes SET path = ?1, updated_at = ?2 WHERE path = ?3",
            params![new_relative, now(), old_relative],
        )?;
        connection.execute(
            "UPDATE trash SET original_path = ?1 WHERE original_path = ?2",
            params![new_relative, old_relative],
        )?;
        resolve_links(&connection)?;
        drop(connection);
        self.get_note(&new_relative)
    }

    pub fn trash_note(&self, path: &str) -> StorageResult<()> {
        let absolute = self.safe_path(path)?;
        let relative = relative_path(&self.root, &absolute)?;
        let content = fs::read_to_string(&absolute)?;
        let connection = self.connection()?;
        let summary = self.summary_by_path(&connection, &relative)?;
        record_revision(
            &connection,
            &relative,
            &content,
            &summary.hash,
            "local",
            "before trash",
        )?;
        let trash_name = format!(
            "{}__{}",
            Uuid::new_v4(),
            absolute
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("note")
        );
        let trash_path = self.root.join(INDEX_DIR).join("trash").join(trash_name);
        fs::rename(&absolute, &trash_path)?;
        connection.execute("INSERT INTO trash(id, original_path, trash_path, title, format, hash, project, removed_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)", params![summary.id, relative, trash_path.to_string_lossy(), summary.title, format_to_db(&summary.format), summary.hash, summary.project, now()])?;
        remove_note_rows(&connection, &summary.id)?;
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
        let connection = self.connection()?;
        let (local, disk): (String, String) = connection.query_row(
            "SELECT local_content, disk_content FROM conflicts WHERE path = ?1",
            params![path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let selected = if resolution == "disk" { disk } else { local };
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
        let candidate = Path::new(path);
        if candidate.is_absolute()
            || candidate.components().any(|part| {
                matches!(
                    part,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(StorageError::InvalidPath(path.into()));
        }
        let normalized = candidate.to_string_lossy().replace('\\', "/");
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
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            fs::copy(&temporary, path)?;
            fs::remove_file(&temporary)?;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(StorageError::Io(error))
        }
    }
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
}
