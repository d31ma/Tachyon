# Tac Stress Harness

Run Tac's DOM patch stress checks with:

```sh
bun run stress:tac
```

The harness exercises `parseFragment` and `morphChildren`, which are the runtime path used by SPA rerenders after a page or slot render returns new HTML.

It covers:

- Stable updates across 1,000 rows, including node preservation for matching IDs.
- Growth and trimming from 1,000 rows to 1,500 rows and back to 500 rows.
- Reordering 1,000 rows to document the current position-based reconciliation behavior.
- Streaming-style growth across 20 patches of 100 rows each.

This command is intentionally separate from `bun test` so slow machine-dependent timing does not block normal CI. It fails on correctness errors or obvious stalls, and otherwise prints timing data for comparison across framework changes.
