# Phase 1 Web Vertical Slice Evidence

## Status

Phase 1 is complete as a development milestone on 2026-07-26. It is not a
preview or supported Tachyon release. The normative behavior and explicit
deferrals are in [`PHASE_1_SPEC.md`](PHASE_1_SPEC.md).

## Delivered Outcome

The compiled Rust `ty` executable initializes a project, discovers static Tac
views and Yon REST-handler routes, emits deterministic web output and Route
Manifest v1, and serves generated files from a loopback-safe development
server. No Bun, Node.js, handler runtime, or application dependency is used by
this path.

The implementation deliberately rejects Phase 2 and Phase 3 behavior:
companions, handler execution, context injection, control tags, components,
bindings, and islands.

## Toolchain and Platforms

- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`.
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`.
- macOS arm64: the complete workspace and real-binary acceptance suite execute
  locally on Darwin 25.5.0.
- Linux x86_64: the same locked suite executes in the official
  `rust:1.97.1-bookworm` image with digest
  `sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa`.
- Windows x86_64: all workspace targets and features compile locally for
  `x86_64-pc-windows-gnu`. Native execution is enforced by the repository's
  `windows-latest` CI job; cross-compilation is recorded only as buildability.

The GitHub Actions native matrix applies the exact toolchain, checks, tests,
documentation build, and release-binary smoke test on Ubuntu, macOS, and
Windows.

## Behavioral Evidence

The workspace has 47 deterministic Rust tests:

- 6 real-binary Phase 1 acceptance tests;
- 4 contract/schema tests;
- 7 repository policy tests;
- 24 core discovery, HTML, compiler, scaffold, and server tests;
- 6 diagnostic contract tests.

The acceptance suite proves:

- `ty init`, `build`, `bundle`, `dev`, and `serve` use the compiled binary;
- two builds of the generated project are byte-identical;
- static Tac views and Yon handler routes have canonical manifest order;
- failures have stable human and JSON diagnostics;
- collisions and unsupported companions retain last known-good output;
- unsafe initialization and non-loopback serving fail closed;
- GET and HEAD succeed, missing and traversal paths do not expose source, and
  defensive headers are emitted.

## Quality and Security Gates

The final local gate covers formatting, all-target/all-feature checking, strict
Clippy, all tests, warning-free rustdoc, dependency advisories/licenses/sources,
release-binary execution, and legacy Bun type and test compatibility.

LLVM coverage is:

| Metric | Result | Required |
| --- | ---: | ---: |
| Lines | 91.04% | 90% |
| Functions | 94.00% | 90% |
| Regions | 91.53% | 90% |

Adversarial coverage includes malformed/oversized/non-UTF-8 HTML, NUL input,
tokenizer errors, scripts and inline events, unknown/control tags, invalid and
dynamic route segments, collisions, companions, symlinked sources/targets,
unsafe output paths, occupied binds, output rollback, missing HTTP paths, and
raw traversal requests.

## Recovery and Compatibility

Build output is staged beside its destination and published only after every
route and the canonical manifest succeed. A failed compilation restores or
retains the previous output. Initialization writes through a temporary sibling
and never overwrites a non-empty target.

The legacy implementation remains untouched and continues to pass its type and
test suites. Phase 1 adds no handler or template semantics that could conflict
with the Phase 2 and Phase 3 compatibility work.
