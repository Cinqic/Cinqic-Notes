# Testing strategy

`pnpm validate` is the canonical gate, locally and in CI. It runs Prettier,
ESLint, TypeScript, frontend tests, the production frontend build, Rust
formatting, Clippy with warnings denied, Rust tests, schema validation, and
version consistency. The release workflow runs the same gate against the exact
tagged commit before anything is packaged.

## What is covered

**Rust — storage (`src-tauri/src/storage.rs`)**: rebuildable indexing, FTS
search, wiki-link backlinks, task extraction, traversal and absolute-path
rejection, Unicode and duplicate names, trash and restore, optimistic conflicts,
revision restore, rename and move carrying revision history and unresolved
conflicts, conflict-resolution input validation, calendar-date validation,
recovery-draft lifecycle, full backup round trip including revisions, restore
refusals for unsafe destinations, archive member validation, and index
fingerprinting including rebuild-after-database-loss.

**Rust — parsing (`src-tauri/src/domain.rs`)**: frontmatter tag scoping and
inline forms, body `#tags`, headings, deduplication, titles, tasks, links,
Unicode, and an unterminated frontmatter block.

**Rust — CLI**: `--expect` hash parsing and its rejections.

**Frontend — autosave (`src/app/autosave.ts`)**: the save state machine, driven
deterministically through a controllable backend rather than through the DOM.
Covers a late completion arriving after a newer edit, hash ordering across
consecutive writes, single-flight serialisation, flushing inside the debounce
window, failure and conflict both leaving the buffer dirty, and a completion
arriving after the user navigated away.

**Frontend — rendering and helpers**: escaped preview, raw HTML safety, remote
image blocking, external links never producing a live `href`, attribute-breakout
attempts, plain-text conversion preserving prose that merely contains Markdown
punctuation, local calendar dates, and appearance-preference fallbacks.

**Contracts**: `pnpm schema:validate` compiles both JSON Schemas with Ajv,
validates the shipped tool contract against its schema, and runs positive and
negative cases so that passing demonstrates the schemas discriminate.

## Principles

- A test for a fix should fail against the code before the fix. When a
  regression test is added for a defect, check that it actually discriminates.
- Prefer testing a state machine directly over driving it through the DOM. The
  autosave controller is deliberately framework-agnostic for this reason.
- Do not weaken an assertion to make a run green.

## Gaps

There is no test that drives real React components, so interaction details —
focus movement, modal focus trapping, and the command palette's rendered
keyboard behaviour — are verified by reading and by the logic they delegate to,
not by an automated interaction test. Adding a lightweight React interaction
testing dependency is the natural next step; it was not added here rather than
introduce a dependency this change set does not yet exercise properly.

No GUI-level or packaged-application testing was performed in this environment.
See [platform status](../release/platform-status.md) for exactly what has and
has not been verified.
