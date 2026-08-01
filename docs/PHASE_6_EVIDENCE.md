# Phase 6 Evidence

This is the validation record for [`PHASE_6_SPEC.md`](PHASE_6_SPEC.md). The
resulting feature-by-feature statement is [`PARITY_LEDGER.md`](PARITY_LEDGER.md).

Machine: macOS 26 (Darwin 25.5.0), Apple silicon, Rust 1.97.1, Bun 1.3.11,
Node 24, Playwright Chromium 1217.

## 1. Automated Gate

`cargo test --workspace --all-targets --all-features --locked`

| Suite | Coverage |
| --- | --- |
| `migrate` | clean projects, unsupported and changed classification with a required action, view-construct classification from source, exclusion of generated and vendored directories, closed failure on a missing target |
| `phase6_cli` | `TY1702` on unsupported constructs, `--allow-unsupported`, byte-identical repeated JSON reports, Migration Report v1 shape, classification of the real legacy fixture, and a build plus clean migration check for every corpus project |

## 2. Differential Over the Shared Corpus

```bash
LEGACY_TY_BIN=/path/to/26.30.04/ty \
TY_BIN=target/debug/ty \
node scripts/compat/differential.mjs
```

Result: `3/3 corpus projects match across implementations`; the 22 files
created by `ty init` are also byte-identical.

| Project | Routes | Result |
| --- | --- | --- |
| `static-pages` | `/`, `/about` | Both build; both serve 200; semantic DOM identical on both routes |
| `semantic-html` | `/`, `/about` | Both build; both serve 200; semantic DOM identical across headings, `nav`, lists, links, `img`, `hr`, `details`/`summary`, `table`, and `footer` |
| `components-slots` | `/` | Both build; both serve 200; one declared divergence (below) |

Each route is served over HTTP and rendered in the same Chromium build. The
legacy output is measured after hydration, because that is when its rendered
result exists. Artifacts are never compared: the legacy build emits a
single-page shell with a client router, a service worker, and per-page chunks,
while the Rust build emits prerendered static HTML per route.

### 2.1 Declared Divergence

`corpus/components-slots/parity.json` declares exactly one:

> `semantic dom /`: `<main>[1]: <product-card> vs <article>`

The legacy implementation keeps a Tac component as a runtime custom element.
The Rust implementation expands components at compile time, so the component
template's own root element reaches the document. Slotted content, accessible
names, and roles are identical. An undeclared divergence fails the gate.

## 3. Behavioral Differences Found by This Work

These were discovered by running both implementations, not assumed.

| Finding | Direction |
| --- | --- |
| The legacy HTML parser rejects standard void elements — `<img>`, `<hr>`, `<br>`, and `<input>` all fail with "No end tag." — and requires the self-closing form. The Rust parser accepts both. | The Rust implementation is a strict superset. Corpus fixtures use the self-closing form so the shared surface stays comparable. |
| The legacy implementation has no `yon.html` convention. It treats every file under `server/routes/**` as a handler and rejects `yon.html` outright. | Yon views and composed route context are `rust-only`. They have no legacy counterpart to compare against and are proven by the Phase 3 suite instead. |
| Tac components remain runtime custom elements in the legacy output and are expanded at compile time in the Rust output. | Intentional; declared and recorded. |

## 4. `ty migrate check` Against the Real Legacy Fixture

```bash
ty migrate check tests/fixtures/fullstack
```

`tests/fixtures/fullstack` is the legacy implementation's own full-stack
fixture: polyglot handlers, services, and workers.

Result: **14 supported, 3 changed, 33 unsupported**, exit `TY1702`.

| Classification | Examples |
| --- | --- |
| `supported` | `client/pages/tac.html`, `server/routes/**/yon.js`, `server/routes/**/yon.py`, colocated styles |
| `changed` | `tac.js` controller companions, `data-tac-on-*` event hydration |
| `unsupported` | `yon.rs`, `yon.cpp`, polyglot view companions, `server/services/**` and `server/repositories/**` imports, `server/workers/**`, `.tachyonrc` interpreters |

Every non-supported finding carries a required action. The analysis reads the
project and never executes it.

## 5. Continuous Gate

The `compatibility` job in `.github/workflows/rust-ci.yml` builds the CLI,
installs Chromium, runs the differential over the corpus, executes
`scripts/compat/standalone-rust.mjs`, and classifies the legacy fixture on every
pull request. The standalone gate verifies the released command surface,
scaffold, cache lifecycle, web build and server, source-only native bundle,
literal page-state activation and assignment in real Chromium, and native
controller state/capability behavior against the compiled Rust binary.

## 6. Released Standalone Workflow

```bash
TY_BIN=target/debug/ty node scripts/compat/standalone-rust.mjs
```

Result: `PASS: released standalone workflow matches Rust ty (macos)`.

The gate clicks an assigning page binding and waits for `Count: 1`. This
regressed once because the compiler emitted a page-island wrapper but omitted
the island runtime when no component island existed. A compiler regression
test now also requires `/.tachyon/islands.js` for that shape.

This gate exists because the old in-tree JavaScript suite imports Fylo files
that were intentionally removed during the product cutover. Its current
whole-tree run stops 67 bundle cases at missing source modules and reports five
load errors, so it cannot distinguish a Rust regression from a broken legacy
checkout. The archived 26.30.04 executable remains immutable and is used by the
neutral differential; public workflows that do not need two implementations
run directly against the Rust executable.

## 7. Open Gaps

| Gap | What closes it |
| --- | --- |
| Corpus coverage of conditionals, iteration, and islands | These need route context, which the two implementations source differently. Closing this requires a legacy-shaped context source that the Rust implementation can also consume. |
| Handler behavior compared across implementations | A handler differential invoking the same `yon.js` and `yon.py` through both supervisors and comparing responses. |
| Diagnostic parity | The legacy implementation raises untyped runtime errors, so only accept/reject agreement is compared today, not messages. |
| A recorded CI run of the `compatibility` job | The job reporting on a pull request. |

No row in the ledger is promoted without the evidence its status demands.
