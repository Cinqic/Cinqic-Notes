# Performance fixtures

The repository includes a repeatable fixture generator for the workloads that
matter to a local notes app:

```powershell
node scripts/generate-benchmark-library.mjs
```

It creates disposable `benchmarks/generated/notes-1000` and
`benchmarks/generated/notes-10000` Libraries containing searchable Markdown,
tags, and tasks. Pass another output directory as the first argument when the
generated files should live elsewhere.

Use the fixtures with a development build to measure cold index rebuild time,
search latency, memory use, and editor responsiveness. These are intentionally
not committed: they are reproducible test data, not application content. The
current milestone reports no unsupported numeric performance guarantee; future
changes should record measured results here rather than describing the app as
“fast” without evidence.
