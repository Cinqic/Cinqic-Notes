# Privacy

Cinqic Notes is designed to keep ordinary note-taking on the device.

## What is stored

The Library you choose contains your `.md`, `.txt`, and attachment files. Those
files are the canonical copy of your writing and stay readable with any ordinary
tool.

Cinqic Notes also keeps application state under `.cinqic/` in the Library:

- `index.sqlite3` — search, tags, links, tasks, and metadata, plus revision
  history, trash records, unresolved conflicts, archive flags, and settings;
- `recovery/` — autosave drafts of unsaved work, retired once the note saves;
- `trash/` — notes you deleted, kept so they can be restored.

### What is and is not rebuildable

This distinction matters, so it is stated precisely:

- **Rebuildable from your files**: search, tags, links, backlinks, tasks, note
  titles and previews. Deleting `index.sqlite3` cannot lose note text; the next
  open reconstructs this from the Markdown and text files themselves. There is a
  regression test for exactly that.
- **Not rebuildable**: revision history, trash contents' metadata, unresolved
  conflicts, archive flags, and settings exist only in `.cinqic/`. They are
  safety history, not canonical documents, but deleting `.cinqic/` does discard
  them permanently. Notes still in the trash live under `.cinqic/trash/` and
  would go with it.

Use **Full backup ZIP** in Settings when you want revisions and settings
included; the plain **Library ZIP** contains your ordinary files only.

The selected Library path is stored in the application's local settings
directory so the next launch can reopen it. Your appearance preference (system,
light, or dark) is stored in local browser storage. No note contents are ever
stored in browser storage.

## Network and telemetry

This milestone makes no network requests. There is no analytics, telemetry,
advertising, tracking, remote font, or automatic synchronisation of any kind.

Verified for this candidate: the built frontend bundle contains no HTTP request
of any kind — the only `http` strings in it are XML namespace identifiers and a
React error-documentation URL used in a message. No HTTP client crate appears in
the Rust dependency graph. The Tauri capability grants no network permission,
and the content security policy restricts `connect-src` to `'self'`.

Remote images are not fetched by the preview; an image reference is shown as a
placeholder. External `http://` and `https://` links are rendered without a live
`href`, so neither viewing a note nor clicking a link can navigate the
application to a remote page. Activating an external link copies its address
and tells you Cinqic Notes does not open remote pages — opening it is your
choice, in your own browser.

Export and backup write only to a local destination you select.

## Juniper

Juniper integration is disabled and no Juniper service is contacted. The Notes
Core contract is documented for a future opt-in integration; any future access
must have an explicit user grant and optimistic, auditable writes.

## Dependencies and platform limits

The desktop shell uses Tauri, React, and Rust with SQLite. A complete transitive
dependency licence inventory is generated offline and committed at
[docs/licenses/inventory.md](docs/licenses/inventory.md).

The repository does not claim an Android release in this milestone. Platform
package support is only considered complete after a package has been built and
launched on that platform; see
[docs/release/platform-status.md](docs/release/platform-status.md) for exactly
what has and has not been verified.
