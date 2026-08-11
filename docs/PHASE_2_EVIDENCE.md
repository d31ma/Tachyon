# Phase 2 Yon Handler Boundary Evidence

## Status

Phase 2 is complete as a development milestone on 2026-07-26. It is not a
preview or supported Tachyon release. The normative contract and explicit
deferrals are in [`PHASE_2_SPEC.md`](PHASE_2_SPEC.md).

## Delivered Outcome

The compiled Rust `ty` executable discovers JavaScript and Python Yon handlers
and invokes either language through Handler Protocol v1. The implementation
validates the source and request, directly spawns one language-adapter process,
enforces bounded framed I/O and process lifetime, and returns one typed response
or stable diagnostic.

Handler-only routes and handlers colocated with a Tac view are represented
deterministically in Route Manifest v1. Handler values are HTTP responses and
are never merged into a build-time view context.

## Toolchain and Platforms

- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`.
- Cargo: `cargo 1.97.1 (c980f4866 2026-06-30)`.
- Local JavaScript runtime: Node.js 23.5.0.
- Local Python runtime: CPython 3.14.4.
- macOS arm64: the complete workspace, compiled CLI, and real JavaScript and
  Python process corpus execute locally on Darwin 25.5.0.
- Linux x86_64: the same locked suite executes with real Node.js and CPython
  children under the official `rust:1.97.1-bookworm` image at digest
  `sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa`.
- Windows x86_64: all workspace targets and features compile locally for
  `x86_64-pc-windows-gnu`. The repository's `windows-latest` job installs
  Node.js 24.18.0 and CPython 3.14.6 and is the authoritative native process
  execution gate; cross-compilation is recorded only as buildability.

The native GitHub Actions matrix fixes the Rust and language runtime versions,
runs the same locked workspace suite on Ubuntu, macOS, and Windows, builds
warning-free documentation, and executes the release binary.

## Behavioral Evidence

The workspace has 69 deterministic Rust tests:

- 6 real-binary Phase 1 acceptance tests;
- 5 real-binary Phase 2 handler acceptance tests;
- 6 shared real-process JavaScript/Python and adversarial tests;
- 5 Handler Protocol and Route Manifest contract tests;
- 7 repository policy tests;
- 34 core discovery, HTML, compiler, server, handler, and scaffold unit tests;
- 6 diagnostic contract tests.

The Phase 2 corpus proves:

- JavaScript and Python receive the same typed request contract and support
  synchronous and asynchronous static HTTP methods;
- request IDs, all seven HTTP methods, repeated headers, UTF-8 bodies, Unicode
  data, and projects with spaces and Unicode paths round-trip;
- handler-only and view-plus-handler routes produce canonical manifest entries;
- host environment values are denied by default and copied only by explicit
  name allowlisting;
- queue admission, startup, execution, framing, and process exit share one
  deadline and a concurrency cap;
- timeout and explicit cancellation terminate and reap sleeping or
  non-cooperative children;
- a process crash is isolated and the next invocation succeeds in a fresh
  child;
- missing runtimes, classes, methods, syntax errors, exceptions, and
  non-serializable values produce stable bounded failures;
- malformed, oversized, trailing, mismatched-ID, stdout-smuggling, and
  stderr-flooding processes fail closed.

## Quality and Security Gates

The final gate covers formatting, all-target/all-feature checking, strict
Clippy, all tests, warning-free rustdoc, dependency advisories/licenses/sources,
immutable GitHub Action references, release-binary execution, and the legacy
Bun type and test compatibility suite.

LLVM coverage is:

| Metric | Result | Required |
| --- | ---: | ---: |
| Lines | 90.67% | 90% |
| Functions | 92.62% | 90% |
| Regions | 91.95% | 90% |

The coverage floor remains uniform across all three metrics. Focused
adversarial process tests remain mandatory independent of aggregate coverage.

## Recovery, Limits, and Compatibility

The supervisor owns every child until exit or forced termination. It drains
stdout and stderr concurrently, writes a typed cancellation frame, applies a
bounded grace period, kills when required, waits for exit, and never pools a
failed process. Diagnostic content, frame size, handler source, request fields,
stderr, concurrency, and lifetime are bounded.

This is not an OS security sandbox. A handler retains the invoking developer
account's ambient filesystem and network access, and Phase 2 does not enforce
kernel CPU or memory quotas. That limitation is explicit in
[`THREAT_MODEL.md`](THREAT_MODEL.md) and [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md).

The legacy implementation remains untouched and passes its type and test
suites. Phase 2 adds no route-context or view semantics, preserving a clean
compatibility boundary for Phase 3.
