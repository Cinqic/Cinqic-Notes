# ADR 0001: Tauri 2 for the desktop shell

## Decision

Use Tauri 2 with React/TypeScript for the UI and Rust for native persistence.

## Why

This matches the proven Cinqic application boundary while keeping the bundle and
permissions smaller than a browser-runtime-heavy desktop framework. Android remains a
future target until its storage and package behavior can be tested honestly.
