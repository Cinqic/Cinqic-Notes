# Cinqic Notes

A local-first notes and lightweight document app by Cinqic. Your notes remain
ordinary Markdown or text files in a Library you choose, while a disposable
SQLite index provides fast local search, links, backlinks, and tasks.

## Product posture

- No account, cloud service, telemetry, or AI is required.
- Markdown and plain text are canonical. Search, tags, links, and tasks are
  rebuildable from those files; revisions, trash, conflicts, and settings live
  only in `.cinqic/`. See [PRIVACY.md](PRIVACY.md) for the precise split.
- Autosave is local and debounced, with revisions and optimistic conflict checks.
- `[[Wiki Links]]`, Markdown links, tags, projects, checklists, and local export
  are supported without a network connection.
- The note list can narrow results by tag, document type, or task state; local
  revisions can be viewed/copied/restored; sharing includes copy, export, print,
  and the device share sheet when the platform provides one.
- Juniper integration is a documented future contract, disabled by default.

This is an early development milestone (`0.1.0`). It is not a collaboration or
cloud-sync product, and it does not claim platform support that has not been
verified in this repository.

## Development

Requirements: Node.js 22+, pnpm 11.19.0 (pinned via `packageManager`; run
`corepack enable`), Rust 1.90+, and the Tauri 2 desktop prerequisites for the
platform being built.

```powershell
pnpm install
pnpm dev
pnpm test
pnpm validate
pnpm tauri:dev
```

`pnpm validate` is the canonical validation command and the gate a release must
pass. It runs formatting, lint, type checking, frontend tests, the production
build, Rust format, Clippy with warnings denied, Rust tests, schema validation,
and version consistency. The release workflow runs it against the exact tagged
commit before anything is packaged.

Before a public release, regenerate the dependency licence inventory:

```powershell
pnpm license:inventory
```

The same Notes Core can be used headlessly through the local CLI during
development:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin cinqic-notes-cli -- <Library> list
cargo run --manifest-path src-tauri/Cargo.toml --bin cinqic-notes-cli -- <Library> search "meeting notes"
```

The CLI has no server listener and uses the same path validation, revision, and
export behaviour as the desktop app. For the same optimistic write the editor
performs, pass the hash from a previous `get`:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin cinqic-notes-cli -- <Library> update Note.md "new content" --expect <hash>
```

Without `--expect`, an update is still refused when the file changed on disk
behind the index, but a caller holding a stale copy can overwrite a newer
indexed change.

## Library format and storage

A Library is an ordinary directory. Cinqic Notes writes `.md` and `.txt` files
there and keeps application state under `.cinqic/`:

```text
My Library/
├── Notes/
│   └── Welcome.md
├── _attachments/
└── .cinqic/
    ├── index.sqlite3
    ├── recovery/
    └── trash/
```

Deleting `.cinqic/index.sqlite3` cannot delete note contents: the next open
rebuilds search and relationship data from the files themselves, and **Rebuild
index** in Settings forces it. Revision history, trash, and unresolved conflicts
are stored inside that database and are local safety history rather than
canonical documents — deleting `.cinqic/` discards them. Use **Full backup ZIP**
in Settings to include them in a backup, and **Restore from ZIP…** to restore a
backup into a new, empty folder.

Opening a Library re-indexes only when its note files have changed, compared by
path, size, and modification time. The check covers the whole Library at once,
so the first open after any change — including one the app itself made — still
rebuilds fully. See [performance](docs/testing/performance.md).

## Privacy and Juniper

The app makes no network requests and has no telemetry or remote fonts. Remote
images are not fetched by the preview, and external links are rendered without a
live `href` so a click cannot navigate the app to a remote page. See [PRIVACY.md](PRIVACY.md) and
[SECURITY.md](SECURITY.md). Juniper is not required and no Juniper connector is
enabled in this milestone; the stable local Notes Core boundary is documented
in [docs/architecture/notes-core.md](docs/architecture/notes-core.md).

## Repository guide

- `src/` — React UI, typed API boundary, editor and rendering helpers.
- `src-tauri/src/` — Rust commands, Library storage, indexing, parsing, and
  export logic.
- `schemas/` — versioned machine-facing Notes Core contracts.
- `docs/adr/` — concise architectural decisions.
- `.github/workflows/` — the reusable validation gate, plus packaging and
  release workflows that both call it.
- `docs/licenses/inventory.md` — generated transitive dependency licences.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow.

## License

Cinqic Notes is licensed under the Apache License 2.0. See [LICENSE](LICENSE)
for the full text and [NOTICE](NOTICE) for the attribution notice.
Third-party dependencies keep their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and
[docs/licenses/inventory.md](docs/licenses/inventory.md).
