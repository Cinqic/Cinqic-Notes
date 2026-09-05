# Cinqic Notes agent guide

- The user's Markdown and text files are the source of truth. The SQLite index and
  `.cinqic/` state are disposable and must be rebuildable from ordinary files.
- Keep the app useful without an account, network, model, or Juniper. Do not add
  network access, telemetry, remote fonts, or cloud sync without an explicit decision.
- Preserve Markdown source; do not replace the editor with lossy rich-text conversion.
- Filesystem and persistence operations belong behind the Rust/Tauri boundary. Keep
  paths relative and reject traversal, absolute injection, symlinks, unsafe archives,
  and untrusted HTML.
- Add migrations for schema changes and retain optimistic concurrency/revision safety.
- Run `pnpm validate` before handing off. Do not claim a platform or package was tested
  unless it was actually built and smoke-tested.
- Avoid dependency bloat and misleading placeholder features. Keep the default UI calm,
  accessible, keyboard-friendly, and comfortable for long writing sessions.
