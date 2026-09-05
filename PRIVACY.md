# Privacy

Cinqic Notes is designed to keep ordinary note-taking on the device.

## What is stored

The Library you choose contains your `.md`, `.txt`, and attachment files. Cinqic Notes
also creates `.cinqic/index.sqlite3` for local search, tags, links, tasks, and metadata;
`.cinqic/revisions/` for local revision metadata; `.cinqic/recovery/` for recovery work;
and `.cinqic/trash/` for recoverable deleted notes. The SQLite index is not the canonical
copy of document text and can be rebuilt from the Library.

The selected Library path is stored in the application's local settings directory so
the next launch can reopen it. No note contents are stored in browser localStorage.

## Network and telemetry

This milestone makes no network requests, includes no analytics, telemetry, advertising,
tracking pixels, remote fonts, or automatic synchronization. Remote images are not
fetched in the built-in preview. Export writes to a local destination selected by you.

## Juniper

Juniper integration is disabled and no Juniper service is contacted. The Notes Core
contract is documented for a future opt-in integration; any future access must have an
explicit user grant and optimistic, auditable writes.

## Dependencies and platform limits

The desktop shell uses Tauri, React, and Rust/SQLite. The repository does not claim an
Android release in this milestone. Platform package support is only considered complete
after a package is built and launched on that platform.
