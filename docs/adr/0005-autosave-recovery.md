# ADR 0005: Debounced autosave and revisions

Editor changes are written after a short idle debounce. Writes flush a same-directory
temporary file before replacement, and meaningful updates create a prior-content
revision. Hashes provide optimistic concurrency and conflict preservation.
