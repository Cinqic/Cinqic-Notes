# Contributing

Install Node.js 22+, pnpm 11, and Rust 1.90+.

```powershell
pnpm install
pnpm validate
pnpm tauri:dev
```

Keep changes focused and explain user-facing behavior in the changelog when relevant.
Every persistence or schema change needs tests. Preserve ordinary Markdown/text files,
keep the app offline by default, and update the relevant ADR if a foundational decision
changes. Do not commit `dist/`, `src-tauri/target/`, local databases, signing material,
or personal note files.
