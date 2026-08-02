# Architecture Decision Records

ADRs record accepted, rejected, or superseded hard-to-reverse decisions.

Statuses:

- **Proposed**: under review and not an implementation authority.
- **Accepted**: the current architectural decision.
- **Superseded**: replaced by a later ADR that links back to it.
- **Rejected**: evaluated and deliberately not selected.

Public behavior that still needs design belongs in an RFC. Routine
implementation choices belong in code review, not an ADR.

- [1. Rust Core and Runtime Boundaries](0001-rust-core-and-runtime-boundaries.md)
- [2. HTML Frontends and Versioned View IR](0002-html-frontends-and-versioned-view-ir.md)
- [3. Supervised Handler Process Protocol](0003-supervised-handler-process-protocol.md)
- [4. Native Rendering and WebSurface Fallback](0004-native-rendering-and-websurface-fallback.md)
- [5. Versioning, Compatibility, and Support Evidence](0005-versioning-compatibility-and-support-evidence.md)
- [6. Safe View Evaluation and Island Activation](0006-safe-view-evaluation-and-island-activation.md)
- [7. TypeScript companions are emitted by the TypeScript compiler](0007-typescript-companion-emission.md)
- [8. Event Binding Contract](0008-event-binding-contract.md)
- [9. Platform Navigation Over a Client Renderer](0009-platform-navigation-over-a-client-renderer.md)
- [10. Island-local Client Evaluation](0010-island-local-client-evaluation.md)
- [11. One wasm ABI for Companions in Any Language](0011-wasm-companion-abi.md)
- [12. Whole-workspace Coverage Ratchet](0012-whole-workspace-coverage-ratchet.md)
