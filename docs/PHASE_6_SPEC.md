# Phase 6 Specification: Compatibility and Migration

This document is normative for Phase 6. It defines how the Rust implementation
is measured against the archived v26.30.04 release, what a compatibility claim
requires, and what `ty migrate check` must report.

Phase 6 adds no application-facing feature. It adds measurement, and the
vocabulary that makes a compatibility claim falsifiable.

## 1. The Oracle

The checksum-verified v26.30.04 executable is the behavioral oracle. It is
downloaded in CI and never committed into this branch. A compatibility claim
is a statement about observable behavior, never about internal structure.

## 2. What Is Compared

Artifacts are **not** compared. The two implementations deliberately emit
different output: the legacy build produces a single-page shell with a client
router, a service worker, and per-page chunks; the Rust build produces a
per-route bootstrap, a compiled render plan, and the Tac client runtime.
Comparing bytes would measure nothing.

What is compared is what a user or an assistive technology can observe:

1. **Route graph** — the set and canonical ordering of published routes.
2. **Semantic DOM** — for every route, the rendered document reduced to
   element names, visible text, and the attributes that carry meaning
   (`alt`, `aria-*`, `for`, `href`, `name`, `placeholder`, `role`, `src`,
   `type`, `value`). Scripts, styles, links, metadata, and every
   implementation-specific attribute are excluded.
3. **HTTP behavior** — the status served for each route.
4. **Diagnostics** — an input rejected by one implementation must be rejected
   by the other, or the difference must be recorded in the ledger.

Both implementations are served over HTTP and rendered in the same real
browser. Their output is measured after the respective client runtime owns and
renders the observable DOM.

## 3. The Corpus

`corpus/` holds the shared application corpus. Every project in it must build
under **both** implementations. A project that only one implementation can
build does not belong there; the feature it exercises belongs in the ledger as
`rust-only` or `unsupported`, and is proven by that implementation's own
suite.

Each project may declare `parity.json`:

```json
{
  "expected_divergences": [
    { "check": "...", "detail": "...", "reason": "...", "direction": "intentional" }
  ]
}
```

A declared divergence is reported in every run and does not fail the gate. An
undeclared divergence fails it. Adding an entry is a deliberate act that must
be accompanied by a ledger row.

## 4. The Ledger

[`PARITY_LEDGER.md`](PARITY_LEDGER.md) records every feature with one of
`identical`, `equivalent`, `changed`, `unsupported`, or `rust-only`.

- `identical` requires a corpus project proving it.
- `equivalent` requires either a corpus project with a declared divergence, or
  a Rust phase suite where the feature has no legacy counterpart.
- `changed` requires `ty migrate check` to report the construct with an action.
- `unsupported` requires `ty migrate check` to report the construct and name
  what the maintainer must do instead.

A feature may not be silently promoted. Promotion requires the evidence its
new status demands.

## 5. `ty migrate check`

```text
ty migrate check [PROJECT] [--json] [--allow-unsupported]
```

The analysis:

- never modifies the project and never executes project code;
- never descends into `.git`, `.tachyon`, `dist`, `dist-bin`, `node_modules`,
  `target`, `__pycache__`, or any dot-directory;
- never follows a symlink;
- reads at most 4 MiB per file and reports at most 10,000 findings;
- classifies each finding as `supported`, `changed`, or `unsupported`;
- attaches a required action to every `changed` and `unsupported` finding;
- orders findings by source then feature, so two runs over one project produce
  identical output.

Exit behavior:

| Condition | Exit |
| --- | --- |
| No `unsupported` findings | success |
| Any `unsupported` finding | `TY1702` |
| Any `unsupported` finding with `--allow-unsupported` | success |
| Unreadable or non-directory target | `TY1701` |

`--json` emits the report as Migration Report v1 instead of the human summary.

## 6. Failure Behavior

| Code | Condition |
| --- | --- |
| `TY1701` | The migration target cannot be read, is not a directory, or exceeds the analysis budget. |
| `TY1702` | The project contains constructs with no equivalent in this implementation. |

## 7. Exit Gate

- [x] Every corpus project builds under both implementations.
- [x] Every corpus route renders an identical semantic DOM, or its divergence
      is declared and carries a ledger row.
- [x] The route graph matches for every corpus project.
- [x] `ty migrate check` classifies the archived migration fixture without executing
      it, and every non-supported finding carries an action.
- [x] The ledger covers the view, server, build, and native surfaces.
- [x] The differential runs in CI.

Phase 6 does not migrate user projects. It tells a maintainer, precisely, what
migrating theirs would require.
