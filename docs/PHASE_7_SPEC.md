# Phase 7 Specification: Enterprise Qualification

This document is normative for Phase 7. It defines the qualification evidence
the implementation must produce before any target may be promoted toward
`supported`, and the budgets and drills that evidence consists of.

Phase 7 adds no application-facing feature. It adds the assurance that the
existing features hold up under hostile input, sustained load, interruption,
and audit.

## 1. Fuzzing

Every input that crosses a trust boundary has a fuzz target under `fuzz/`.

| Target | Boundary |
| --- | --- |
| `html_frontend` | Application HTML reaching the bounded parser. |
| `template_frontend` | Control tags, bindings, and component references. |
| `handler_frame` | Length-prefixed frames read from an untrusted child process. |
| `native_planner` | Resolved HTML lowered into Native UI v1 for five targets. |

Requirements:

- Every target runs under `AddressSanitizer`.
- No input may cause a panic, an abort, a timeout, or unbounded memory growth.
- `native_planner` additionally asserts a cross-platform invariant: all five
  targets must agree on whether an input is valid. A disagreement is a bug even
  when neither side crashes.
- Hand-written seeds are tracked in the repository. Generated corpus entries
  are not.
- A crashing input is published as a CI artifact and becomes a regression test.

The fuzz crate is excluded from the pinned workspace because it requires a
nightly toolchain. It never ships.

## 2. Sanitizers

The library suite runs under `AddressSanitizer`, `LeakSanitizer`, and
`ThreadSanitizer`. Our own crates set `unsafe_code = "forbid"`, so these
primarily qualify dependency code and the process, threading, and filesystem
interactions the implementation performs.

## 3. The `fuzzing` Feature

`tachyon-core` exposes crate-private parsers and decoders to fuzz targets
through the `fuzzing` feature. It is not a stable API, carries no
compatibility guarantee, and must never be enabled in a released artifact.

## 4. Recovery Drills

Each drill states an operator-visible property and the failure it simulates.

| Drill | Property |
| --- | --- |
| Interrupted build | A build killed mid-flight leaves the published output byte-identical, adds no files, and the next build succeeds. |
| Corrupted incremental cache | Damaged build state is detected rather than trusted; the rebuild succeeds and reproduces the same output. |
| Failed build | A source regression never damages the published application. |
| Concurrent builds | Racing builds never publish a torn application; every manifest route has its document. |
| Read-only output | An unwritable target produces a diagnostic, never a panic and never a partial write. |

Publication is atomic. Every drill asserts that property from the outside,
against the real executable.

## 5. Soak

Sustained rebuilds in one long-lived working tree must hold three properties:

1. output stays deterministic across every iteration;
2. open descriptors do not grow beyond a small constant;
3. latency does not drift upward as state accumulates.

The iteration count is `TACHYON_SOAK_ITERATIONS`, defaulting to 24 locally and
raised in CI.

## 6. Performance Budgets

Ceilings are deliberately wide, because shared CI hardware is noisy, and are
meant to catch an order-of-magnitude regression rather than a few percent.

| Budget | Ceiling |
| --- | --- |
| Clean build, 50 routes | 20 s |
| Incremental rebuild, 50 routes | 20 s |
| Generated output per route | 64 KiB |

A budget may be tightened when measurements justify it. Loosening one requires
a recorded reason.

## 7. Supply-Chain Evidence

- `cargo deny check advisories bans licenses sources` gates every change.
- Every GitHub Action is pinned to an immutable commit.
- A CycloneDX software bill of materials is generated for the workspace.
- The release binary is built with `cargo auditable`, embedding its dependency
  list so a deployed artifact can be audited without its source tree.

## 8. Independent Security Review

An independent review is a **human deliverable and cannot be produced by the
implementation or its automation**. Phase 7 delivers the package a reviewer
needs — trust boundaries, threat model, contracts, and the qualification
evidence above — and records the review itself as an open gap until a named
reviewer signs it off.

No target may be promoted to `supported` while that gap is open.

## 9. Exit Gate

- [x] Every trust boundary has a fuzz target that runs clean under
      `AddressSanitizer`.
- [x] The `native_planner` cross-target invariant holds under fuzzing.
- [x] The library suite passes under `AddressSanitizer`.
- [x] Every recovery drill passes against the real executable.
- [x] The soak drill holds determinism, descriptor, and latency properties.
- [x] Every performance budget is met and published.
- [x] Supply-chain policy, SBOM, and auditable-build jobs exist in CI.
- [ ] An independent security review is complete. **Open.**
- [ ] Install, upgrade, rollback, and uninstall exercises are recorded on each
      supported target. **Open.**

The last two are the remaining blockers to `supported`, and to the cutover
gate in `PROJECT_PLAN.md`.
