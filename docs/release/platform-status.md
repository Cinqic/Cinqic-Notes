# Platform status

This file states exactly what has been verified, and by whom. Nothing here is
inferred from a successful compile.

## This stabilisation review (Linux, candidate branch)

Environment: Linux 7.0.0-31-generic, AMD Ryzen 7 5700G, 16 cores, 14 GB RAM,
ext4, Node 22.23.2, pnpm 11.19.0, Rust 1.98.0.

| Step                                                                        | Status                                                    |
| --------------------------------------------------------------------------- | --------------------------------------------------------- |
| `pnpm validate` (the full canonical gate)                                   | Passed                                                    |
| Rust test suite                                                             | Passed                                                    |
| Clean-clone validation at the candidate SHA                                 | Passed                                                    |
| Release CLI binary built and exercised                                      | Passed                                                    |
| 1,000 and 10,000 note fixtures indexed, searched, listed                    | Passed                                                    |
| Linux `.deb` and AppImage built (`pnpm tauri build`)                        | Passed                                                    |
| `.deb` payload inspected: ships `usr/bin/cinqic-notes`, `Exec=cinqic-notes` | Passed                                                    |
| Packaged application launched from the extracted `.deb` on `DISPLAY=:0`     | Passed — started and stayed running with no stderr output |
| Application driven through its interface (clicking, typing, screenshots)    | **Not performed**                                         |
| `.deb` installed with `dpkg -i`, uninstalled, or upgraded                   | **Not performed**                                         |
| AppImage executed as a packaged AppImage                                    | **Not performed** — payload extracted and inspected only  |
| Windows build, install, or smoke test                                       | **Not performed**                                         |

Building the packages found a release-blocking defect that no amount of source
review would have shown: the bundler selected the headless CLI as the
application, so the `.deb` and AppImage contained only `cinqic-notes-cli`, and
the desktop entry launched it with `Terminal=false`. Installing the release and
clicking the icon would have run a command-line tool that printed usage to a
terminal that does not exist and exited. It is fixed, and the release workflow
now asserts the package payload rather than only that a bundle file exists.

The application was launched and confirmed to keep running, but it was **not
driven**: this environment offered no reliable way to capture or interact with
the window, so nothing is claimed about how the interface behaves on screen.
The frontend changes on this branch are covered by unit tests over the extracted
logic — the autosave controller, the dialog focus rules, Markdown and plain-text
conversion, preferences — but their wiring inside `App.tsx` has been type-checked
and linted, not exercised in a running window. That is the largest remaining
evidence gap on this branch.

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
