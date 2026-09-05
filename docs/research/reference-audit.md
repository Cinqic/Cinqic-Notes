# Reference audit

Before choosing the Notes architecture, the current working trees for
`Cinqic/Juniper-App`, `Cinqic/Cinqic-Calculator`, and `Cinqic/cinqic.com` were
reviewed during the foundation audit. Their source trees are not vendored into
this repository; this document records the implementation-relevant conclusions
without copying unrelated application code.

## Reused patterns

- Juniper: Tauri 2, React/TypeScript, a narrow native command boundary, SQLite
  migrations, typed domain contracts, capability files, pinned tool versions,
  honest platform/release documentation, and local privacy policy.
- Calculator: deliberately small local storage, atomic writes, offline-first
  behavior, reusable logic separated from UI, a restrained theme system, and
  explicit statements when Juniper is not integrated.
- cinqic.com: neutral surfaces with green used as an accent, serif display
  typography paired with system UI, visible focus states, reduced-motion rules,
  modest spacing/radii, and no claims for unfinished capabilities.

## Deliberate differences

Notes is smaller than Juniper because it has no model/provider runtime. Unlike
Calculator's private JSON state, primary document content is stored as ordinary
Markdown/text files; SQLite is only the rebuildable search/relationship layer.
The app also owns a conflict-aware file boundary because users are expected to
edit and synchronize their Library with ordinary filesystem tools.
