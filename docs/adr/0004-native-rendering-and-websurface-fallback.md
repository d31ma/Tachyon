# ADR 0004: Native Rendering and WebSurface Fallback

- Status: Accepted
- Date: 2026-07-26

## Context

Tachyon targets web, macOS, iOS, Android, Windows, and Linux while requiring
application developers to author HTML. Not every HTML element, Tac component,
or web component will have a native adapter on every platform.

Falling back the entire application to a WebView because one subtree is
unsupported defeats the native architecture. Pretending an unsupported tag is
a native primitive produces broken behavior.

## Decision

Plan native rendering per View IR subtree.

- Standard elements and components with registered adapters render natively.
- Control tags have already been lowered and never require adapters.
- An unsupported safe subtree becomes a WebSurface containing a generated
  local web bundle.
- Supported parents, siblings, and unrelated descendants remain native.
- The boundary serializes explicit properties and events through the portable
  controller contract.
- Remote web content always uses `bridge: none`.
- Local WebSurface capabilities must appear in a deny-by-default capability
  manifest and are revalidated per call.
- A fallback decision emits an inspectable build diagnostic and appears in the
  native artifact manifest.

Fallback must select the smallest subtree that preserves semantic and lifecycle
correctness. It may widen only when layout, state, or event boundaries cannot
be safely separated.

## Consequences

- Applications remain predominantly native as adapter coverage grows.
- Unknown custom content has predictable behavior.
- Native and web lifecycle synchronization becomes a first-class contract.
- Visual similarity alone is insufficient; accessibility and interaction
  parity are required.
- Local WebSurface bundle isolation and navigation policy become security
  boundaries.

## Rejected Alternatives

- **Application-wide render mode flag**: makes one unsupported component
  downgrade the entire application.
- **Drop unsupported nodes**: silently loses behavior and accessibility.
- **Render unknown tags as generic labels**: changes semantics.
- **Give remote WebViews the native bridge**: unacceptable capability
  escalation.

## Acceptance Gate

- Native UI v1 represents native elements, text, and WebSurface boundaries;
- its invalid fixture proves remote content cannot receive a bridge;
- capability manifest v1 proves default deny;
- future platform tests compare semantics, events, accessibility, and visuals
  against the web reference.
