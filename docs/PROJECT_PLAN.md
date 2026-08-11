# Tachyon Rust Rewrite Project Plan

## Executive Decision

Rebuild Tachyon as a Rust modular monolith on a long-lived rewrite branch in
the existing repository. Preserve the current implementation as an executable
compatibility oracle until the Rust implementation satisfies the cutover gate.

The rewrite is not a line-by-line port. Stable behavior, source conventions,
contracts, and developer workflows are carried forward deliberately; internal
implementation structure is greenfield.

## Delivery Principles

- Deliver small vertical slices with implementation, negative tests,
  documentation, observability, and compatibility evidence together.
- Add a crate only when it owns production behavior and tests.
- Stabilize boundaries before optimizing implementations.
- Separate buildability from support.
- Prefer native execution evidence over cross-compilation claims.
- Do not publish an incomplete implementation under the stable Tachyon name.

## Phase 0: Foundation

### Scope

- Rust workspace with an exact supported toolchain.
- Repository lint, dependency, licensing, and source policies.
- Product context, terms, relationships, invariants, non-goals, and deferred
  decisions.
- Five accepted architecture decisions.
- Initial threat model.
- Versioned canonical schemas with accepted and rejected examples.
- Cross-platform Rust CI.
- Release, support, security, governance, and contribution policies.
- A documented boundary between the archived release oracle and greenfield code.

### Exit Gate

- [x] The exact Rust toolchain installs from `rust-toolchain.toml`.
- [x] `cargo fmt --check` passes.
- [x] `cargo check --workspace --all-targets --all-features --locked` passes.
- [x] `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
      passes.
- [x] `cargo test --workspace --all-targets --all-features --locked` passes.
- [x] `cargo doc --workspace --all-features --no-deps --locked` passes with
      warnings denied.
- [x] Every canonical schema passes Draft 2020-12 meta-validation.
- [x] Every canonical schema accepts its positive example and rejects its
      negative example.
- [x] Dependency advisories, licenses, sources, and banned dependencies pass
      policy.
- [x] Linux, macOS, and Windows CI use the same exact toolchain and commands.
- [x] All GitHub Actions are pinned to immutable commits.
- [x] A new contributor can explain the system and its trust boundaries without
      reading implementation source.
- [x] Phase 0 evidence is recorded in `docs/PHASE_0_EVIDENCE.md`.

Phase 0 does not implement a compiler, server, handler runtime, or renderer.

## Phase 1: Web Vertical Slice

Status: complete on 2026-07-26. The normative behavior is recorded in
[`PHASE_1_SPEC.md`](PHASE_1_SPEC.md), and the validation record is in
[`PHASE_1_EVIDENCE.md`](PHASE_1_EVIDENCE.md).

### Scope

- Project discovery and deterministic Tac view/Yon REST route graphs.
- Bounded, source-aware HTML tokenization and Phase 1 feature validation.
- Stable human and Diagnostics v1 failures.
- Deterministic staged web output and Route Manifest v1.
- `ty init`, `ty build`, and a loopback-safe `ty dev`.
- Real-binary, cross-platform acceptance tests.

### Exit Gate

- [x] The real `ty` binary initializes, builds, and serves one generated
      project.
- [x] Repeated builds are byte-identical and emit Route Manifest v1.
- [x] Static `tac.html` routes and `yon.*` REST handlers have canonical ordering.
- [x] Unsupported Phase 2/3 syntax fails with stable diagnostics.
- [x] Failed builds preserve the last known-good output.
- [x] HTTP GET, HEAD, 404, traversal resistance, and defensive headers are
      exercised against the real server.
- [x] macOS executes the native acceptance suite.
- [x] Linux executes the same suite in a pinned official container.
- [x] Windows compiles locally; the native `windows-latest` CI job is the
      authoritative execution gate.
- [x] Formatting, check, Clippy, tests, rustdoc, coverage, supply-chain policy,
      and the released-binary compatibility suite pass.

## Phase 2: Yon Handler Boundary

Status: complete on 2026-07-26. The normative behavior is recorded in
[`PHASE_2_SPEC.md`](PHASE_2_SPEC.md), and the validation record is in
[`PHASE_2_EVIDENCE.md`](PHASE_2_EVIDENCE.md).

### Scope

- Typed Handler Protocol v1 request, response, error, and cancellation
  envelopes.
- Deterministic `yon.js` and `yon.py` discovery and manifest metadata.
- Direct-spawn, bounded, one-process-per-request supervision.
- JavaScript and Python sync/async adapters.
- Deadlines, cancellation, concurrency admission, forced termination, reaping,
  and crash isolation.
- Deny-by-default environment inheritance with explicit allowlisting.
- Real-runtime and compiled-binary contract tests.

### Exit Gate

- [x] JavaScript and Python pass the same request/response corpus against real
      child processes.
- [x] The compiled `ty handler invoke` path exercises both runtimes and every
      Handler Protocol v1 HTTP method.
- [x] Handler-only and composed routes emit deterministic Route Manifest v1
      metadata.
- [x] Framing, stdout/stderr separation, frame size, request IDs, and response
      validation fail closed.
- [x] Deadline time includes concurrency admission and process lifetime.
- [x] Cancellation, timeout, crash, malformed output, and forced termination
      reap the child; a later invocation starts cleanly.
- [x] Child environments expose only a minimum runtime baseline plus explicit
      allowlist entries.
- [x] Paths with spaces and Unicode, exceptions, missing classes/methods,
      syntax errors, non-serializable results, and missing runtimes are tested.
- [x] Formatting, check, Clippy, tests, rustdoc, coverage, dependency policy,
      action pinning, cross-platform buildability, Linux-container execution,
      and released-binary compatibility gates pass.

## Phase 3: Tac View Semantics

Status: complete on 2026-07-26. The normative behavior is recorded in
[`PHASE_3_SPEC.md`](PHASE_3_SPEC.md), and the validation record is in
[`PHASE_3_EVIDENCE.md`](PHASE_3_EVIDENCE.md).

### Scope

- Safe bounded binding expressions and contextual escaping.
- Canonical and legacy-compatible conditionals and iterations.
- Recursive Tac components, properties, slots, and cycle detection.
- Yon handler discovery without build-time execution or view context.
- Browser-owned Tac rendering with load, idle, visible, interaction, and never
  component mount schedules (superseded design recorded by ADR 0015).
- View IR v1, View Source Map v1, verified incremental builds, and
  multi-source diagnostic recovery.

### Exit Gate

- [x] Compiled-binary golden tests cover controls, bindings, components, slots,
      Tac render plans, source maps, Yon non-execution, and incremental reuse.
- [x] Malformed expressions, orphan controls, component
      cycles, invalid islands, cache corruption, and failed-build rollback fail
      closed.
- [x] Real Chromium proves client initial render, automatic component mounting,
      rerendering, bounded failure marking, and interaction replay.
- [x] Phase 1 and Phase 2 compatibility suites remain green.
- [x] Formatting, check, Clippy, tests, rustdoc, coverage, dependency policy,
      action pinning, Linux-container execution, and Windows buildability pass.

## Phase 4: Native Vertical Slice

Status: complete on 2026-07-26. The normative behavior is recorded in
[`PHASE_4_SPEC.md`](PHASE_4_SPEC.md), the implementation boundary in
[`PHASE_4_IMPLEMENTATION_PLAN.md`](PHASE_4_IMPLEMENTATION_PLAN.md), and the
validation record in [`PHASE_4_EVIDENCE.md`](PHASE_4_EVIDENCE.md).

### Scope

- Native UI v1 planning from fully resolved Phase 3 output.
- Fixed semantic HTML-to-SwiftUI adapters with deterministic node identities.
- HTML-authored scalar state, bindings, increment/toggle actions, and lifecycle
  logging.
- Smallest-subtree local fallback plus bridge-free HTTPS WebSurfaces.
- Capability Manifest v1, Artifact Manifest v1, generated Swift source,
  ad-hoc-signed `.app` bundle, and atomic publication.
- Real macOS Accessibility and mobile-Chromium semantic/visual comparison.

### Exit Gate

- [x] Compiled-binary and schema tests cover adapters, evaluated views,
      accessibility, declarative state, manifests, fallback, and rollback.
- [x] Invalid configuration, state, accessibility, URL, nesting, surface count,
      symlink, and output shapes fail closed.
- [x] A real SwiftUI `.app` builds, signs, launches, handles native button,
      keyboard input, and disclosure interactions, and tears down its lifecycle.
- [x] Required roles and names match the mobile-web reference; native and web
      screenshots pass the documented coarse visual-layout budget.
- [x] Formatting, check, Clippy, tests, rustdoc, coverage, dependency policy,
      action pinning, Linux-container execution, Windows buildability,
      released-binary compatibility, and release-smoke gates pass.

## Phase 5: Platform Expansion

Status: complete on 2026-07-26. The normative behavior is recorded in
[`PHASE_5_SPEC.md`](PHASE_5_SPEC.md), and the validation record is in
[`PHASE_5_EVIDENCE.md`](PHASE_5_EVIDENCE.md).

Add Android, iOS, Windows, and Linux one at a time. A platform advances only
after native compilation, execution, packaging, interaction, accessibility,
upgrade, and rollback evidence exists.

### Scope

- A `NativeTarget` selection threaded through the CLI, planner, staging, and
  Artifact Manifest v1, with per-target output isolation.
- Platform-neutral staging shared by every host generator.
- iOS `UIKit`-backed `SwiftUI`, Linux GTK4 with `WebKitGTK`, Windows Win32
  common controls, and Android platform views.
- Reproducible per-platform gates and the CI jobs that run them.

### Exit Gate

- [x] One project lowers to byte-identical Native UI v1 for every target
      apart from the target tag and the platform-named fallback reason.
- [x] Each target publishes only under its own directory, and a failed build
      publishes nothing there.
- [x] iOS builds a signed simulator `.app` that launches and answers native
      button, keyboard, and disclosure interaction.
- [x] Android assembles an APK that launches on an emulator and exposes
      native `Button` and `EditText` widgets with declared content
      descriptions.
- [x] Linux compiles with warnings denied, launches headlessly, and returns
      declared names, native roles, and an assistive-technology activation
      over AT-SPI.
- [x] Windows cross-compiles to a `PE32+` GUI binary with warnings denied.
- [x] Every declared platform reduction has a named viability gate recorded
      in `PHASE_5_SPEC.md` and reflected in `SUPPORT_TIERS.md`.
- [x] Formatting, check, Clippy, tests, and rustdoc pass.

Windows execution evidence is owned by the `windows-native` CI job. A native
run was recorded on 2026-08-01; Windows remains below `native-tested` until
semantic UI Automation roles and accessibility activation are evidenced.

## Phase 6: Compatibility and Migration

Status: complete on 2026-07-26. The normative behavior is recorded in
[`PHASE_6_SPEC.md`](PHASE_6_SPEC.md), the validation record is in
[`PHASE_6_EVIDENCE.md`](PHASE_6_EVIDENCE.md), and the resulting statement is
[`PARITY_LEDGER.md`](PARITY_LEDGER.md).

Run the archived v26.30.04 release and Rust implementation over the shared application corpus.
Compare route graphs, rendered HTML, HTTP behavior, events, diagnostics, and
artifacts. Deliver `ty migrate check` and a feature-parity ledger.

### Scope

- A shared corpus every project of which builds under both implementations.
- A browser-based differential comparing route graphs, semantic DOM, and HTTP
  status, with declared divergences kept visible and undeclared ones fatal.
- `ty migrate check`, which classifies a project without executing it.
- A ledger covering the view, server, build, and native surfaces.

### Exit Gate

- [x] Every corpus project builds under both implementations.
- [x] Every corpus route renders an identical semantic DOM, or its divergence
      is declared and carries a ledger row.
- [x] `ty migrate check` classifies the archived migration fixture, attaches an
      action to every non-supported finding, and is byte-deterministic.
- [x] The ledger distinguishes `identical`, `equivalent`, `changed`,
      `unsupported`, and `rust-only`, and states what it does not claim.
- [x] The differential runs in CI.

Artifacts are deliberately not compared; the two implementations emit
different output by design. Observable behavior is the contract.

## Phase 7: Enterprise Qualification

Status: implementation complete on 2026-07-26. Release-candidate packaging,
fail-closed installers, signing, attestation, native post-publication
verification, and draft-to-public promotion were completed on 2026-08-01.
Automated security qualification is the pre-tag security gate; independent
human review is optional. The normative behavior is in
[`PHASE_7_SPEC.md`](PHASE_7_SPEC.md), and the validation record is in
[`PHASE_7_EVIDENCE.md`](PHASE_7_EVIDENCE.md).

Complete fuzzing, sanitizers, supply-chain evidence, performance budgets,
soak tests, recovery drills, automated security qualification, and supported-target
promotion.

### Exit Gate

- [x] Every trust boundary has a fuzz target; 7,197,692 executions under
      `AddressSanitizer` produced zero crashes, timeouts, or OOMs.
- [x] The native planner's cross-target validity invariant holds under
      fuzzing.
- [x] The library suite passes under `AddressSanitizer`.
- [x] Interruption, corruption, contention, read-only, and failed-build
      recovery drills pass against the real executable.
- [x] Soak holds determinism, descriptor, and latency properties.
- [x] Performance budgets are published and met.
- [x] Supply-chain policy passes; SBOM and auditable-build jobs exist.
- [x] **Automated security qualification is complete.** No unowned critical or
      high finding remains; see
      [`SECURITY_REVIEW_PACKAGE.md`](SECURITY_REVIEW_PACKAGE.md).
- [x] **Install, upgrade, rollback, and uninstall exercises are recorded for
      the `ty` artifact.** The release lifecycle drill installs, upgrades,
      rolls back, verifies, and uninstalls a deterministic archive.

Independent human review remains welcome as defense in depth but does not
block CI, packaging, support-tier claims, tagging, or publication.

## Cutover Gate

Current status, condition by condition, is recorded in
[`CUTOVER.md`](CUTOVER.md). **The gate is not met.**

The Rust implementation becomes Tachyon's default only when:

- every stable behavior has passing compatibility evidence or a documented
  migration;
- supported platforms pass their native evidence profiles;
- install, upgrade, rollback, and uninstall have been exercised;
- release artifacts are signed, attested, and independently verifiable;
- no unowned critical threat-model finding remains;
- stable documentation describes the Rust implementation rather than plans.

## Post-qualification Capability: Semantic Hot Updates

Status: complete on 2026-08-09. The decision and safety boundary are recorded
in [`adr/0013-semantic-hot-updates.md`](adr/0013-semantic-hot-updates.md) and
superseded for current renderer ownership by ADR 0015.

Development rebuilds originate from bounded operating-system file events and
publish Hot Update Protocol v1 over a same-origin event stream. CSS updates
preserve the document, renderer-owned Tac component boundaries use
lifecycle-aware state transfer, compiler failures retain the last-good page
with structured diagnostics, and every ambiguous or structural change reloads
safely. `scripts/hot-update-browser-test.mjs` is the repeatable browser gate.

## Post-qualification Capability: Yon Isolation Backends

Status: transport boundary complete on 2026-08-09. The decision and security
limits are recorded in
[`adr/0014-environment-selected-yon-isolation.md`](adr/0014-environment-selected-yon-isolation.md).

Operators select the default process backend or a Firecracker control driver
exclusively through the parent environment. HTTP handlers, middleware,
workers, and explicit invocation use that policy. Builds never execute Yon.
Both paths retain Handler Protocol v1 framing, process-group cleanup,
deadlines, cancellation, concurrency, bounded output, and diagnostics while
the Firecracker path passes bounded pool, CPU, memory, and deny-egress policy
to the driver.

This is not yet a first-party Firecracker runtime or hardware-isolation support
claim. A production-qualified control program, jailer/host profile, guest
image, warm-pool lifecycle, snapshot lineage, and native Linux evidence remain
future vertical slices.
