# Cinqic Notes — development pause record

Pause date: 2026-09-17

## Status

`DEVELOPMENT PAUSED / NOT RETIRED / NOT DISCONTINUED`

Active development of Cinqic Notes is paused until further notice. Cinqic Notes
remains a Cinqic project, and Cinqic may resume it later. No date for resuming
has been set, and resuming is not promised.

The GitHub repository is archived so that it is intentionally read-only while
development is paused. Archival is used here as a preservation mechanism. It
does not mean the project is retired, discontinued, abandoned, completed, or
superseded, and it does not mean Cinqic Notes has been replaced by another
Cinqic product. The repository can be unarchived if development resumes.

This document and [README.md](README.md) are the current authority for project
status. Other documents in this repository record the state of the project at
the time they were written.

## Project state at pause

Canonical `main` before the pause changes: `2c46a800e6c1e8dc7f43e2932c9ed36755d55bff`.
The tag `paused-2026-09-17` identifies canonical `main` after this record was
merged.

Cinqic Notes is a local-first notes and lightweight document app built with
Tauri 2, React, and Rust. Notes are ordinary Markdown and text files in a
Library directory the user chooses; a rebuildable SQLite index provides search
and relationships.

### Version and release

- The package version is `0.1.0`, described in [CHANGELOG.md](CHANGELOG.md) as
  a development milestone. The later 0.1.x stabilisation work is merged into
  `main` and remains listed as unreleased.
- **No public release exists.** There is no GitHub Release, no published
  package or installer, and no version tag. The pause did not create one.
- The tag `paused-2026-09-17` marks the repository state at the pause. It is
  not a version or a release.

### Implemented in source on `main`

- Local Libraries of `.md` and `.txt` files, with application state under
  `.cinqic/`.
- SQLite indexing with full-text search, tags, wiki links, backlinks, graph
  data, and Markdown task extraction, rebuildable from the note files.
- Debounced autosave with atomic writes, local revisions, trash and restore,
  recovery drafts, and optimistic conflict detection that keeps both versions.
- Import, export to Markdown, plain text, and escaped HTML, Library ZIP and full
  backup ZIP, and restore of a backup into a new, empty folder.
- A React/Tauri desktop interface with light, dark, and system themes and a
  command palette.
- A headless `cinqic-notes-cli` over the same local core.
- A documented, versioned Juniper Notes Core tool contract in `schemas/`.
  **No Juniper connector is implemented or enabled.**

### Tested

- `pnpm validate`, the canonical gate, passed in GitHub Actions on `2c46a80`
  (run `34273072149`). It covers formatting, lint, type checking, 48 frontend
  unit tests, the production frontend build, Rust formatting, Clippy with
  warnings denied, Rust tests, schema validation, and version and license
  consistency. It passed again locally on the same commit on the pause date.
- Frontend tests exercise extracted logic such as the autosave controller and
  rendering helpers. There is no automated test that drives real React
  components, and no GUI-level test was performed. See
  [docs/testing/strategy.md](docs/testing/strategy.md).
- CLI performance was measured at 1,000 and 10,000 notes. See
  [docs/testing/performance.md](docs/testing/performance.md), including its
  documented limit: the first open after any change rebuilds the whole index.

### Packaged

- During the 0.1.x stabilisation review, Linux `.deb` and AppImage packages were
  built locally, the `.deb` payload was inspected, and the application was
  launched from the extracted package and stayed running.
- The application was **not** driven through its interface, the `.deb` was not
  installed with `dpkg`, the AppImage was not run as an AppImage, and Windows was
  not built or tested in that review. An earlier Windows debug MSI smoke test
  was not re-verified.
- The release workflow and the manual package smoke-build workflow have never
  run in GitHub Actions.
- Android was never packaged. See
  [docs/release/platform-status.md](docs/release/platform-status.md).

### Published

Nothing. No platform is supported, because no package was ever published.

### Planned but not implemented

- Juniper integration ([ADR 0008](docs/adr/0008-juniper-boundary.md)).
- Android storage and packaging ([ADR 0010](docs/adr/0010-android-storage.md)).
- Per-file incremental re-indexing, noted in the performance document.
- Automated interaction tests for React components, noted in the testing
  strategy.

## Why development is paused

Cinqic is choosing not to actively allocate development effort to Cinqic Notes
at this time while other work receives priority. This is a prioritization
decision, not a conclusion that the project has failed or has no future value.

## Preservation policy

- Git history is preserved and was not rewritten.
- Source code, tests, schemas, scripts, and workflows are preserved.
- Architectural decision records are preserved unchanged.
- Known limitations are preserved as written.
- Incomplete evidence remains incomplete. Steps recorded as **Not performed**
  were not turned into passes, and no new platform or package claim was added.
- Existing release infrastructure remains in the repository as historical
  material. It was not used during the pause and was not removed.
- Historical documents were not rewritten to suggest the pause was planned when
  they were written. Where a document could be mistaken for a current support
  statement, a short status notice was added above its original content.
- The project was not rewritten into a finished state.

## Potential return direction

The direction below was under consideration when development paused. It is
recorded so it is not lost. **None of it is implemented, and none of it is a
commitment.** No architecture for it has been designed or decided.

- Cinqic Notes could become a local-first shared knowledge system used by both
  the person who owns the notes and Juniper.
- Juniper could receive explicitly permissioned access to read or change notes
  through a controlled interface, consistent with the boundary in
  [ADR 0008](docs/adr/0008-juniper-boundary.md).
- External systems such as Git or GitHub could serve as optional backup or
  portability targets.
- Local, user-owned note files would remain central to the product.
- External backup is not the same thing as synchronization between devices, and
  should not be presented as one.

## Resumption procedure

If Cinqic decides to resume development:

1. Unarchive `Cinqic/Cinqic-Notes`.
2. Update this document and [README.md](README.md) to record that development
   has resumed, before normal development work starts.
3. Review dependency and security drift accumulated during the pause, including
   `pnpm audit`, `cargo audit`, and toolchain versions. Do not assume the
   advisory results in [SECURITY.md](SECURITY.md) still hold.
4. Re-establish platform evidence on current systems. Do not assume the results
   in [docs/release/platform-status.md](docs/release/platform-status.md) still
   apply.
5. Run the canonical validation suite: `pnpm install --frozen-lockfile` and
   `pnpm validate`.
6. Confirm the validation, package, and release workflows still work before
   relying on them.
7. Start new work on branches from current canonical `main`.
8. Update Cinqic.com to describe active development only after the canonical
   repository supports that claim.
