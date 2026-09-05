# ADR 0004: Source-preserving textarea editor

The first milestone uses a native source-preserving textarea with a safe rendered
preview. It cannot silently rewrite Markdown constructs. A richer Markdown-aware editor
may be introduced only with golden round-trip coverage for the supported syntax.
