# Testing strategy

`pnpm validate` is the canonical local/CI gate. It runs Prettier, ESLint,
TypeScript, frontend tests, the production frontend build, Rust formatting,
Clippy with warnings denied, Rust tests, schema checks, and version consistency.

The Rust storage tests exercise rebuildable indexing, FTS search, wiki-link
backlinks, task extraction, traversal/absolute path rejection, Unicode and
duplicate names, trash/restore, optimistic conflicts, and revision restore.
Frontend tests cover escaped Markdown preview, raw HTML safety, remote image
blocking, plain-text rendering, and malformed date handling. Additional UI
coverage should be added alongside any new interaction rather than relying on
browser-only snapshots. The production UI also keeps filters and device-share
capabilities progressive: unsupported platform APIs fall back to local copy or
export rather than blocking ordinary note work.

The Windows development environment verifies the Tauri executable launch and
debug MSI bundle. Linux package and Android checks are CI responsibilities until
those platforms are available for local verification; documentation must keep
that distinction explicit.
