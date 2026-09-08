# Contributing

Install Node.js 22+, pnpm 11.19.0 (`corepack enable` uses the pinned version),
and Rust 1.90+ with `rustfmt` and `clippy`.

```bash
pnpm install --frozen-lockfile
pnpm validate
pnpm tauri:dev
```

`pnpm validate` is the gate. A release cannot be published unless it passes on
the exact tagged commit, so a change that cannot pass it is not ready.

Keep changes focused and explain user-facing behaviour in the changelog when
relevant. Every persistence or schema change needs tests, and a test written for
a bug fix should fail against the unfixed code — check that it does.

Preserve ordinary Markdown and text files as the canonical copy, keep the app
offline by default, and update the relevant ADR only if a foundational decision
actually changed.

Do not claim a platform or package works unless you built it and launched it;
record what you did in [docs/release/platform-status.md](docs/release/platform-status.md).

Do not commit `dist/`, `src-tauri/target/`, local databases, signing material,
benchmark fixtures, or personal note files.
