# Tachyon Rust Rewrite Documentation

Start with:

For the current feature reconciliation, read
[`RECONCILIATION.md`](RECONCILIATION.md) and ADRs
[0018](adr/0018-native-hosts-run-the-platform-javascript-engine.md) and
[0019](adr/0019-companions-compile-for-their-target.md). Phase evidence below
records historical milestones; widget-planner and WebAssembly results do not
qualify the new native hosts.

1. [`../CONTEXT.md`](../CONTEXT.md) for product language and invariants.
2. [`PROJECT_PLAN.md`](PROJECT_PLAN.md) for delivery phases and exit gates.
3. [`PHASE_1_SPEC.md`](PHASE_1_SPEC.md) and
   [`PHASE_1_EVIDENCE.md`](PHASE_1_EVIDENCE.md) for the implemented web slice.
4. [`PHASE_2_SPEC.md`](PHASE_2_SPEC.md) and
   [`PHASE_2_EVIDENCE.md`](PHASE_2_EVIDENCE.md) for the supervised handler
   boundary.
5. [`PHASE_3_SPEC.md`](PHASE_3_SPEC.md) and
   [`PHASE_3_EVIDENCE.md`](PHASE_3_EVIDENCE.md) for the implemented view,
   context, component, island, and incremental-build milestone.
6. [`PHASE_4_SPEC.md`](PHASE_4_SPEC.md) and
   [`PHASE_4_EVIDENCE.md`](PHASE_4_EVIDENCE.md) for Native UI, SwiftUI,
   accessibility, lifecycle, packaging, and WebSurface fallback.
7. [`architecture/OVERVIEW.md`](architecture/OVERVIEW.md) for system boundaries.
8. [`THREAT_MODEL.md`](THREAT_MODEL.md) for trust boundaries and controls.
9. [`ENGINEERING_STANDARDS.md`](ENGINEERING_STANDARDS.md) for contribution
   expectations.
10. [`RELEASE_ENGINEERING.md`](RELEASE_ENGINEERING.md) and
   [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md) for evidence-based release claims.

Hard-to-reverse decisions live in [`adr/`](adr/). Public machine contracts live
under [`../api/`](../api/) and are executable through the
`tachyon-contracts` test suite.
