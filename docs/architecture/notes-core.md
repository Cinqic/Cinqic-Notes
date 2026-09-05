# Notes Core architecture

The React UI is a client of typed Tauri commands. The Rust layer owns Library path
validation, file I/O, SQLite migrations, indexing, revisions, conflict checks, and
portable export. The canonical document is always the `.md` or `.txt` file; SQLite
stores derived search and relationship state.

The initial command surface is intentionally small and headless-friendly:

`list_notes`, `get_note`, `create_note`, `update_note`, `rename_note`, `trash_note`,
`restore_note`, `search_notes`, `list_tasks`, `toggle_task`, `get_backlinks`,
`get_graph`, `import_files`, and `export_note`.

The `cinqic-notes-cli` binary exposes the same local core for controlled
headless development use. It does not start a network listener or bypass the
storage boundary.

`update_note` accepts an expected SHA-256 content hash. A mismatch records both buffers
under the local conflict table and refuses to overwrite the file. A future Juniper
adapter should use the same command contract, record actor and operation metadata, and
require an explicit permission grant before any access.
