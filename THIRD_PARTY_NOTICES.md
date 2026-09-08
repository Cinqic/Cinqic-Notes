# Third-party notices

Cinqic Notes is distributed under the Apache License 2.0. See [LICENSE](LICENSE)
and [NOTICE](NOTICE).

Third-party dependencies keep their own licenses. Apache-2.0 applies to Cinqic
Notes' own code and relicenses nothing else.

## Complete inventory

A full transitive inventory of both locked dependency graphs is generated
offline and committed at [docs/licenses/inventory.md](docs/licenses/inventory.md):

```bash
pnpm license:inventory
```

The generator reads `pnpm licenses list` for JavaScript and
`cargo metadata --locked` for Rust. It makes no network requests, classifies
each SPDX expression, and exits non-zero if any dependency needs review before
a public release.

## Summary at the time of writing

- **Shipped JavaScript runtime**: React and React DOM (MIT), the Tauri API and
  dialog plugin (MIT or Apache-2.0 at your option), and `scheduler` (MIT).
  Everything else in the JavaScript graph is build or development tooling and
  is not redistributed.
- **Rust**: 463 crates resolve in the graph. The overwhelming majority are
  `MIT OR Apache-2.0`. No dependency requires review; nothing is GPL or AGPL.
- **Weak copyleft**: `cssparser`, `cssparser-macros`, `dtoa-short`, `selectors`,
  and `option-ext` are MPL-2.0, reached through the Tauri stack. They are used
  unmodified. MPL-2.0 is file-level copyleft, satisfied by shipping them
  unmodified and pointing to their upstream sources.
- **SQLite** is built by `rusqlite` and is in the public domain.
- **WebKitGTK** on Linux is a system library, linked dynamically and not
  bundled.

The counts above describe the resolved graph, which spans every target and
feature combination Cargo considers; not every crate is linked into a given
platform binary.
