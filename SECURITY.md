# Security

## Reporting a vulnerability

**Private vulnerability reporting is not currently enabled on this repository,
and there is no published security contact address.** Until one exists, there
is no confidential channel for reporting a vulnerability in Cinqic Notes.

If you have found something sensitive, please open an issue that says only that
you have a security report and asks how to send it privately — do not include
the details, a proof of concept, or note contents in a public issue.

Enabling GitHub Private Vulnerability Reporting on this repository would give
reporters a confidential channel; that is a repository setting the maintainers
need to turn on. This section will be replaced with concrete instructions once
a private channel is available.

When you can report privately, include the affected version, the platform,
reproduction steps, and a minimal proof of impact. Never include private note
contents.

## Security boundaries

- Note files are user-owned and are only reached through Rust commands that
  validate paths first.
- The default Tauri capability grants `core:default` and `dialog:default` only.
  The UI has no blanket filesystem access, no shell execution, and no network
  permission.
- Library paths reject traversal, absolute paths, Windows drive and UNC
  prefixes, and symlinked files during scans. Archive members are validated the
  same way, with per-member and total size limits on restore.
- Saves write a temporary file in the same directory, flush it, and replace the
  destination with `fs::rename`, which is atomic within a directory on both
  supported platforms. If the replacement cannot be performed the save fails and
  leaves the original note untouched — there is no fallback that copies over the
  destination, because that would truncate the note before rewriting it.
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

`cargo audit` reports no vulnerabilities. It reports seven warnings, all reached
transitively through the Tauri and GTK stacks rather than chosen directly:
`proc-macro-error` and the `unic-*` crates are unmaintained, and `glib` has a
documented unsoundness in an iterator this project does not call. They can only
be resolved upstream. `pnpm audit` reports no vulnerabilities.

## Support policy

The current development line is `0.1.x`. Security fixes are made to the
supported development branch when practical. Packages are unsigned; do not infer
production security guarantees from a development build.
