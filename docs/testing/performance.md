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

### When the fast path applies, and when it does not

The fingerprint is all-or-nothing for the whole Library, and it is only written
by a full rebuild. So the first open after _any_ note changed — including a note
the app itself saved — rebuilds the entire index. Measured at 10,000 notes:

| Sequence                        | Wall time |
| ------------------------------- | --------- |
| Open, nothing changed           | 0.20 s    |
| Open, after one note was edited | 11.32 s   |
| Open again, still unchanged     | 0.07 s    |

So a session that edits notes pays a full rebuild on the next launch. This is
strictly better than before, when every launch rebuilt unconditionally, but the
0.06 s figure describes an unchanged Library, not the common edit-then-relaunch
case.

The obvious shortcut — refreshing the fingerprint after each save — is **not
safe** and was deliberately not done. Saving one note makes the index current
for that note only; recording a whole-Library fingerprint at that moment would
assert that every other file is indexed too. A note changed outside the app
while it was closed would then be skipped on the next open and never indexed.

Removing the remaining cost needs per-file state (path, size, modification time
per note) so that an open can re-index only what actually changed. That is a
real change to the index schema and was left out of this stabilisation branch
rather than added under time pressure.

Cold index construction is roughly 1 ms per note. It is the price of being able
to rebuild everything from the canonical files, which is a property worth
keeping.

## Not measured

Editor responsiveness, graph construction, and backup creation were not measured
under load, and no GUI-level measurement was taken, because this environment has
no desktop session. Record those against a running desktop build before making
any claim about them.
