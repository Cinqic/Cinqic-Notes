# Security

## Status during the development pause

Cinqic Notes development is paused (see [PAUSED.md](PAUSED.md)), and the project
is **not actively maintained** while the pause lasts. Security fixes are not
promised during the pause. No public release or package exists; anyone using
Cinqic Notes has built it from source and takes on responsibility for it.

## Reporting a vulnerability

**There is no confidential channel for reporting a vulnerability in Cinqic
Notes.** GitHub Private Vulnerability Reporting is not enabled, there is no
published security contact address, and the archived repository does not accept
new issues.

Do not publish vulnerability details, a proof of concept, or note contents in
any public place in an attempt to report them. If development resumes, this
section will be updated before reports are invited again.

## Security boundaries

These boundaries describe the design and implementation on `main` when
development paused. They are a description of the code, not a maintenance
guarantee.

- Note files are user-owned and are only reached through Rust commands that
  validate paths first.
- The default Tauri capability grants `core:default`, `dialog:default`, and
  `core:window:allow-destroy`. The last exists so that closing the window can
  wait for an in-flight save to finish before the window goes away; it is the
  only window mutation the UI can perform. There is no blanket filesystem
  access, no shell execution, and no network permission.
- Library paths reject traversal, absolute paths, Windows drive and UNC
  prefixes, and symlinked files during scans. Archive members are validated the
  same way, with per-member and total size limits on restore.
- Saves write a temporary file in the same directory, flush it, and replace the
  destination with `fs::rename`, which is atomic within a directory on both
  targeted desktop platforms. If the replacement cannot be performed the save
  fails and leaves the original note untouched — there is no fallback that
  copies over the destination, because that would truncate the note before
  rewriting it.
- Optimistic content hashes stop a stale editor buffer from overwriting a change
  made outside the app; the conflicting versions are kept so the user chooses.
  The CLI's `update` takes `--expect <hash>` for the same protection.
- The preview and exported HTML escape all content. Raw Markdown HTML is never
  executed, remote images are never fetched, and external links are rendered
  without a live `href`, so viewing or clicking inside a note cannot navigate
  the WebView to a remote page.
- Imported Markdown and text files are copied; the original is never deleted.
- Restoring a backup requires an empty destination outside the active Library,
  so it cannot overwrite existing files or replace the Library in place.

## Known dependency advisories

The following was recorded during the 0.1.x stabilisation review, merged on
September 8, 2026. It is a historical result, was not re-run for the pause, and
is not a statement about the current state of the dependencies. New advisories
may have been published since.

`cargo audit` reported no vulnerabilities. It reported seven warnings, all
reached transitively through the Tauri and GTK stacks rather than chosen
directly: `proc-macro-error` and the `unic-*` crates are unmaintained, and
`glib` has a documented unsoundness in an iterator this project does not call.
They could only be resolved upstream. `pnpm audit` reported no vulnerabilities.

## Support policy

No version of Cinqic Notes is supported. Development paused on the `0.1.x`
development line, no release was published, and no security maintenance is
taking place during the pause. Any locally built package is unsigned; do not
infer production security guarantees from a development build.
