# ADR 0013: Semantic Hot Updates Without a Production Client Renderer

- Status: Superseded by ADR 0015 for Tac rendering and hot-update ownership
- Date: 2026-08-09

## Context

The Rust development server rebuilt changed source but exposed only live
reload. It sampled every source tree every 400 milliseconds, published an
integer generation, and made each open page poll every 500 milliseconds before
calling `location.reload()`. Incremental compilation reduced compiler work but
did not preserve browser state.

Vite solves the general JavaScript case through native ESM, module graphs, and
framework-owned acceptance boundaries. Tachyon owns more specific semantic
information: styles, HTML templates, Tac islands, Yon routes, and View IR. A
Tachyon update can therefore name the framework boundary that is safe to
replace instead of presenting every source as an arbitrary JavaScript module.

ADR 0009 rejects a production client renderer. Hot updates must not quietly
reverse that decision or make application correctness depend on a development
reconciler.

## Decision

Introduce Hot Update Protocol v1 as a development-only, server-to-client
contract. The development server uses operating-system filesystem events, a
bounded 1,024-event queue, and a 75 millisecond quiet period. One editor save
produces one deterministic, canonically ordered change set. Queue overflow,
watcher uncertainty, mixed change kinds, and unknown paths widen to a reload;
they never guess at a narrower boundary.

Successful builds publish one of three actions over a same-origin
server-sent-event stream:

- `css` replaces generated same-origin stylesheets after their new bytes load;
- `island` replaces only named hydrated Tac component boundaries, imports the
  digest-addressed companion, and restores compatible declared state; or
- `reload` performs an ordinary document reload when structure, routing, Yon
  context, configuration, or another unsupported boundary changed.

A failed build publishes `diagnostics`, leaves the last-good artifacts and DOM
running, and presents Diagnostics v1 in a bounded text-only overlay. The next
successful action clears the overlay. The client does not evaluate source and
never receives authored executable text in a protocol message.

An island may implement `hotState()`, `restoreHotState(state)`, and
`hotDispose()` for an explicit state and cleanup contract. Without those
methods the runtime uses `structuredClone` on enumerable non-function fields;
a failed default clone retains an empty object. An explicit `hotState()` result
is also cloned before restore. The client claims no JSON or depth bound for
component state. The existing hydration `AbortSignal` is aborted before
replacement so listeners and asynchronous work can terminate.

The replacement client also snapshots at most the first 2,048 elements with an
`id` for browser-owned mutable state: input and textarea values, input checked
state, `<details>` disclosure, nonzero scroll offsets, and focus by element id.
It does not retain text selection or selected `<option>` state. It restores
properties only after the new companion has activated and never transfers
markup, attributes, event listeners, arbitrary DOM properties, or
contenteditable HTML.

The server sends `X-Accel-Buffering: no`; no WebSocket upgrade is required.
The existing `/.tachyon/live` integer endpoint and
`/.tachyon/live-reload.js` asset path remain compatibility surfaces, but the
client uses `/.tachyon/hot` and no longer polls.

The hot-update client is injected only while `ty dev` watches. It is never
written by `ty build`, bundled into production output, or used for navigation.
Static HTML/template changes deliberately reload until a future compiler-owned
fragment contract can prove stable identities. Native View IR updates require
a separately accepted host transport and are not claimed by this decision.

## Consequences

CSS and hydrated component behavior update without destroying unrelated page
or island state. Native form, disclosure, scroll, and focus state inside the
replaced boundary also survives a companion-only edit. Compiler errors no
longer force a broken document or hide in the terminal. Server-side and
structural changes retain an explicit, correct reload fallback.

The development runtime now owns a small island replacement path and state
lifecycle. It does not own general DOM reconciliation, import-graph emulation,
or arbitrary side-effect preservation. Applications with non-JSON state must
declare their hot-state methods if they want that state retained.

The event watcher adds the permissively licensed `notify` dependency and its
platform-specific CC0-1.0 and ISC transitive dependencies. Source
queues, protocol collections, diagnostic text, and SSE broadcast history are
bounded. A disconnected client that misses an update receives a reload on
reconnection rather than a partial replay.

## Rejected Alternatives

- Porting the legacy SPA renderer would reverse ADR 0009 and ship development
  complexity in production.
- Reproducing Vite's general ESM graph would duplicate a mature tool without
  using Tachyon's HTML and island semantics.
- Applying arbitrary HTML diffs would risk losing custom-element state,
  duplicating listeners, and desynchronising the server document.
- WebSockets add bidirectional protocol and reverse-proxy configuration that
  this one-way update stream does not need.
- Treating every successful build as hot-updatable would hide unsafe widening
  and make state retention nondeterministic.

## Acceptance Gate

- [x] Filesystem events replace source-tree polling without publication loops.
- [x] Hot Update Protocol v1 has a canonical schema and positive/negative fixtures.
- [x] CSS changes replace styles without losing island state.
- [x] Island companion changes dispose, replace, reactivate, and restore declared state.
- [x] Island replacement retains bounded native form, disclosure, scroll, and focus state.
- [x] Invalid source retains the last-good page and displays structured diagnostics.
- [x] Structural changes use the full-reload fallback.
- [x] The client and update stream are absent when source watching is disabled.
- [x] Generated production output does not contain the hot-update client.
