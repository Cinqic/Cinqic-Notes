# Changelog

## Unreleased — stabilisation

Data durability

- Fixed autosave discarding a newer edit when a save completed late. The guard
  compared two values captured from the same render, so it never fired; typing
  during an in-flight save reverted the editor and marked it saved.
- Editing and immediately switching notes no longer drops the pending edit.
  Navigation, Library switching, and window close now flush instead of
  cancelling the debounce.
- Renaming or moving a note now carries its revision history and any unresolved
  conflict with it, instead of stranding them under the old path.
- Trashing a note records where it went before the file moves, so a failure
  cannot leave a note in `.cinqic/trash/` with no way to restore it.
- Replacing a note file no longer falls back to copying over the destination,
  which could truncate a note mid-write.
- Recovery drafts are retired once a note saves and follow a renamed note.

Correctness

- Search now runs when you type. Nothing previously re-queried on input.
- Conflict resolution rejects unrecognised values instead of silently keeping
  the local copy.
- Daily notes use the local calendar date, and impossible dates are rejected.
- Frontmatter tags are read only from the `tags:` key; other YAML lists such as
  `authors:` are no longer indexed as tags.
- "Copy plain text" no longer strips punctuation character by character, which
  corrupted ordinary prose.
- Clipboard failures are reported instead of being announced as success.

Interface

- The command palette's arrow-key navigation now exists, matching its own
  footer, with listbox semantics and a selection marker that is not colour-only.
- The search box shows the shortcut that actually focuses it, without a Command
  glyph on Windows and Linux.
- Appearance persists across launches.
- Settings can restore a backup into a new, empty folder.

Privacy

- External links no longer render as live anchors, which could navigate the
  desktop WebView to a remote page.

Performance

- Opening a Library re-indexes only when its files changed, and listing uses
  set-based queries. At 10,000 notes, listing went from 13.9 s to 0.06 s.

Project

- Completed the Apache-2.0 migration in package metadata and notices, and added
  a generated transitive dependency licence inventory. `pnpm license:check`
  now fails validation if a first-party declaration drifts again.
- A release can no longer publish without `pnpm validate` passing on the exact
  tagged commit.
- `pnpm schema:validate` now performs real JSON Schema validation.

## 0.1.0 — Development milestone

- Added local Notes Libraries backed by ordinary Markdown and text files.
- Added SQLite indexing, full-text search, tags, wiki links, backlinks, graph data,
  and Markdown task extraction.
- Added debounced autosave, atomic writes, local revisions, trash, import/export,
  and optimistic conflict protection.
- Added a calm responsive React/Tauri shell with light, dark, and system themes.
- Added a documented, disabled-by-default Juniper Notes Core boundary.
