# Cinqic Notes

A local-first notes and lightweight document app by Cinqic. Your notes remain
ordinary Markdown or text files in a Library you choose, while a disposable
SQLite index provides fast local search, links, backlinks, and tasks.

## Product posture

- No account, cloud service, telemetry, or AI is required.
- Markdown and plain text are canonical; the SQLite database is rebuildable.
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

Requirements: Node.js 22+, pnpm 11, Rust 1.90+, and the Tauri 2 desktop
prerequisites for the platform being built.

```powershell
pnpm install
pnpm dev
pnpm test
pnpm validate
pnpm tauri:dev
```

`pnpm validate` is the canonical frontend/native validation command. It runs
formatting, lint, type checking, frontend tests, schema checks, and the Rust
format/test/clippy checks when the Rust toolchain is available.

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
    └── revisions/
```

Deleting `.cinqic/index.sqlite3` cannot delete note contents. Use **Rebuild
index** in Settings to reconstruct search and relationship data from files.
Revision copies are local safety history, not the canonical document.

## Privacy and Juniper

The app makes no network requests and has no telemetry or remote fonts. Remote
images are not fetched by the preview. See [PRIVACY.md](PRIVACY.md) and
[SECURITY.md](SECURITY.md). Juniper is not required and no Juniper connector is
enabled in this milestone; the stable local Notes Core boundary is documented
in [docs/architecture/notes-core.md](docs/architecture/notes-core.md).

## Repository guide

- `src/` — React UI, typed API boundary, editor and rendering helpers.
- `src-tauri/src/` — Rust commands, Library storage, indexing, parsing, and
  export logic.
- `schemas/` — versioned machine-facing Notes Core contracts.
- `docs/adr/` — concise architectural decisions.
- `.github/workflows/` — validation and packaging workflows.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow.
