# Performance

## Fixtures

```bash
pnpm benchmark:fixtures [output-directory]
```

Creates disposable `notes-1000` and `notes-10000` Libraries of searchable
Markdown with tags and tasks. They are not committed: they are reproducible test
data, not application content.

## Measured results

Candidate: `review/stabilization-0.1.x`. Environment: Linux 7.0.0-31-generic,
AMD Ryzen 7 5700G (16 cores), 14 GB RAM, ext4, release build of
`cinqic-notes-cli`, timed with `/usr/bin/time`. The CLI opens a Library through
the same `Library::open` path the desktop app uses.

10,000 notes:

| Operation                                | Before  | After   |
| ---------------------------------------- | ------- | ------- |
| Cold open, building the index from files | 13.56 s | 10.57 s |
| Warm open and list                       | 13.87 s | 0.06 s  |
| Search                                   | 13.76 s | 0.08 s  |
| Task listing                             | 10.60 s | 0.04 s  |

1,000 notes: cold open 0.22 s, warm list 0.01 s (previously 0.21 s). Peak RSS is
about 22 MB at 10,000 notes and about 7 MB at 1,000, unchanged by these changes.
The index is about 11 MB for 10,000 notes.

### What changed

Two independent problems made every operation cost a full re-index:

1. `Library::open` rebuilt the index unconditionally, so the cost was paid on
   every launch rather than when files actually changed.
2. Listing attached tags and task counts one note at a time, roughly 20,000
   queries for 10,000 notes.

Opening now compares a metadata-only fingerprint (path, size, modification time)
against the one recorded by the last rebuild, and any difference falls through
to a full rebuild. Listing and search use set-based queries.

Cold index construction is still roughly 1 ms per note. That is a one-time cost
when the files actually change, and it is the price of rebuilding from the
canonical files, which is a property worth keeping.

## Not measured

Editor responsiveness, graph construction, and backup creation were not measured
under load, and no GUI-level measurement was taken, because this environment has
no desktop session. Record those against a running desktop build before making
any claim about them.
