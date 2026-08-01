# 0012 — Whole-workspace coverage ratchet

## Status

Accepted.

## Context

Phase 0 through Phase 4 measured a target-neutral subset of the Rust core and
held it above 90% for lines, functions, and regions. Phases 5 through 7 added
CLI orchestration, five native hosts, external toolchain probes, WebAssembly
compiler drivers, server lifecycle code, and a fuzz-only interface. Those
paths are exercised primarily by real-process, real-browser, and native-host
gates. They are nevertheless present in LLVM's whole-workspace denominator.

The first clean Linux pull-request measurement of that expanded denominator
was 81.26% lines, 80.49% functions, and 80.97% regions. Keeping a 90% check
while describing a narrower measurement as workspace coverage would be false;
excluding every real-world boundary solely to preserve the percentage would
make the metric less useful.

## Decision

The release gate enforces a uniform 80% floor across lines, functions, and
regions for the whole workspace with all features enabled. Only
`native/compiler.rs` and `native/macos.rs` remain excluded because the LLVM
instrumentation does not represent their external Swift compiler, signing,
and AppKit process execution meaningfully. Their behavior is mandatory in the
native application gates.

Coverage is one ratchet, not the definition of test quality. Parser, path,
template, compiler, handler-protocol, server, and native-planner boundaries
also require their focused negative, adversarial, fuzz, process, browser, or
native-host tests. A change may not trade those tests for aggregate coverage.

Raising the uniform floor requires observed headroom on the full CI
measurement. Lowering it requires another accepted ADR with a measured reason.

## Consequences

- The number in CI describes the code actually included in the release
  workspace rather than a hand-selected target-neutral subset.
- A one-point regression in the lowest metric fails the pull request.
- Historical Phase 0 through Phase 4 evidence remains valid for the narrower
  source set and continues to state that scope.
- Native, browser, fuzz, sanitizer, compatibility, and lifecycle jobs remain
  required because aggregate source coverage cannot replace execution in the
  environment whose behavior is being claimed.
