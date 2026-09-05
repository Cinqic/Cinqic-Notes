# Security

## Reporting

Please report suspected vulnerabilities privately to the Cinqic maintainers before
opening a public issue. Include the affected version, platform, reproduction steps,
and a minimal proof of impact. Do not include private note contents in a report.

## Security boundaries

- Note files are user-owned and accessed by Rust commands after path validation.
- The default Tauri capability grants core application behavior and native dialogs;
  it does not grant arbitrary shell execution or blanket filesystem access to the UI.
- Library paths reject traversal, absolute injection, and symlinked files during scans.
- Saves use a same-directory temporary file and flush before replacement. Optimistic
  hashes prevent a stale editor buffer from overwriting an external change.
- Preview and exported HTML escape content and do not execute raw Markdown HTML or
  fetch remote images.
- Imported Markdown/text is copied; the original source is not deleted.

## Support policy

The current development line is `0.1.x`. Security fixes are made to the supported
development branch when practical. Do not infer production security guarantees from
an unsigned development package.
