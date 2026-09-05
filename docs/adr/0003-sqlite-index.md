# ADR 0003: SQLite for derived search and relationships

Use a versioned, local SQLite database with FTS5 for deterministic search and normalized
tables for tags, links, tasks, revisions, trash, and conflicts. Rebuilding it scans the
ordinary Library files and never needs a network or model.
