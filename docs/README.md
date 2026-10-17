# Tachyon Rust Rewrite Documentation

To understand the current feature reconciliation, read
[`RECONCILIATION.md`](RECONCILIATION.md) and these ADRs (architecture decision
records):

- [0018](adr/0018-native-hosts-run-the-platform-javascript-engine.md);
- [0019](adr/0019-companions-compile-for-their-target.md).

To understand how developers write applications, read these ADRs:

- [0029](adr/0029-pages-compose-components.md) (pages compose components);
- [0030](adr/0030-components-use-services.md) (components use services).

The phase evidence below records historical milestones. The widget-planner
and WebAssembly results there do not qualify the new native hosts.

Start with:

1. [`../CONTEXT.md`](../CONTEXT.md) for the product language and invariants.
2. [`PROJECT_PLAN.md`](PROJECT_PLAN.md) for the delivery phases and their exit
   gates.
3. [`PHASE_1_SPEC.md`](PHASE_1_SPEC.md) and
   [`PHASE_1_EVIDENCE.md`](PHASE_1_EVIDENCE.md) for the implemented web slice.
4. [`PHASE_2_SPEC.md`](PHASE_2_SPEC.md) and
   [`PHASE_2_EVIDENCE.md`](PHASE_2_EVIDENCE.md) for the supervised handler
   boundary.
5. [`PHASE_3_SPEC.md`](PHASE_3_SPEC.md) and
   [`PHASE_3_EVIDENCE.md`](PHASE_3_EVIDENCE.md) for the implemented milestone
   of views, context, components, islands and incremental builds.
6. [`PHASE_4_SPEC.md`](PHASE_4_SPEC.md) and
   [`PHASE_4_EVIDENCE.md`](PHASE_4_EVIDENCE.md) for Native UI, SwiftUI,
   accessibility, lifecycle, packaging and WebSurface fallback.
7. [`architecture/OVERVIEW.md`](architecture/OVERVIEW.md) for the system
   boundaries.
8. [`THREAT_MODEL.md`](THREAT_MODEL.md) for trust boundaries and controls.
9. [`ENGINEERING_STANDARDS.md`](ENGINEERING_STANDARDS.md) for what the project
   expects from contributions.
10. [`RELEASE_ENGINEERING.md`](RELEASE_ENGINEERING.md) and
    [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md) for release claims based on
    evidence.
11. [`SELF_HOSTED_RUNNERS.md`](SELF_HOSTED_RUNNERS.md) for running the Rust CI
    workflow on maintainer machines.

Decisions that are hard to undo are in [`adr/`](adr/). Public machine
contracts are in [`../api/`](../api/). The `tachyon-contracts` test suite runs
them as tests.
