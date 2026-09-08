# Platform status

This file states exactly what has been verified, and by whom. Nothing here is
inferred from a successful compile.

## This stabilisation review (Linux, candidate branch)

Environment: Linux 7.0.0-31-generic, AMD Ryzen 7 5700G, 16 cores, 14 GB RAM,
ext4, Node 22.23.2, pnpm 11.19.0, Rust 1.98.0.

| Step                                                        | Status            |
| ----------------------------------------------------------- | ----------------- |
| `pnpm validate` (the full canonical gate)                   | Passed            |
| Rust test suite                                             | Passed            |
| Release CLI binary built and exercised                      | Passed            |
| 1,000 and 10,000 note fixtures indexed, searched, listed    | Passed            |
| Linux `.deb` / AppImage packages built                      | **Not performed** |
| Any Linux package installed or launched                     | **Not performed** |
| Tauri desktop window launched (`tauri dev` / `tauri build`) | **Not performed** |
| Windows build, install, or smoke test                       | **Not performed** |

The GUI was not launched during this review: this environment has no desktop
session available for a WebKitGTK window, so no claim is made about the running
application's behaviour on screen. Frontend logic was covered by unit tests
instead, and the storage engine through the CLI, which uses the same code path
as the desktop commands.

## Earlier milestone claim (not re-verified here)

A previous milestone recorded that the app was built and smoke-tested on Windows
x64 through the Tauri debug MSI path. That claim predates this review, refers to
an earlier commit, and has **not** been re-verified against this candidate.

## Before calling a platform supported

Build the package, install it, launch it, and exercise Library creation, edit,
save, reopen, import, export, backup, restore, external modification and
conflict, and index rebuild. Record the distribution or Windows build.

## Android

Intentionally deferred. There is no Android package, no storage permission, and
no Storage Access Framework claim. A future mobile phase must implement
app-private storage plus explicit import and export, and verify an APK on an
emulator or device, before documenting support or adding Android to a release
matrix. The Tauri-generated Android icon assets in the repository do not
constitute Android support.
