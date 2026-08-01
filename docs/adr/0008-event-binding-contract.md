# ADR 0008: Event Binding Contract

- Status: Accepted
- Date: 2026-07-27

## Context

Phase 3 refused every executable event attribute with `TY1306`, directing
authors to an island companion. That kept arbitrary code out of a view, which
is a real invariant, but it left two defects.

An author writing the legacy `on:click="increment()"` could not compile at all.
An author writing the compiled marker `data-tac-on-click` compiled fine and got
a document containing a dead attribute: the button rendered, and clicking it
did nothing. Nothing reported the problem. A silent no-op is worse than a
diagnostic, because the author has no signal to act on.

The legacy compiler treats `on:<event>` as the authored form and
`data-tac-on-<event>` as its own output, encoded that way so Light-DOM web
components do not re-interpret a raw `on*` attribute and double-bind.

## Decision

Accept `on:<event>` as the authored binding and keep `TY1306` for anything
executable.

A binding value is parsed, never evaluated. The grammar is a handler name,
optionally followed by literal arguments that may only be strings, numbers,
booleans, or null. Anything else is a diagnostic, including member calls,
arrow functions, string concatenation, and multiple statements.

The compiler emits `data-tac-on-<event>` carrying the parsed handler name and
literals, encoding a namespaced event's colon as `__` exactly as the legacy
compiler does. A delegated runtime resolves the handler against the named
export of the route's client module, which the `tac.js` and `tac.ts`
companions already emit.

A route that binds an event without a client module fails with `TY1306`. The
dead-marker case is therefore impossible by construction.

Native `onclick=` attributes remain refused, directing authors to `on:<event>`.

## Consequences

An author can write ordinary event handlers again, and the invariant that a
view never carries executable code is preserved: the document contains a
handler *name*, and only a module the author wrote can supply behavior.
Handlers live in a real module, so they are ordinary functions that can be
tested and type-checked.

Against that, the runtime is delegated from the document root, so a handler
attached to a node that is later removed no longer fires. That matches
delegation semantics and the legacy implementation. Binding to a handler name
that the client module does not export is a runtime error reported to the
console, not a compile-time one, because the compiler does not parse the
module's exports.

## Rejected Alternatives

Evaluating the binding as a JavaScript expression, as a plain framework would,
was rejected. It reintroduces arbitrary execution into a view, contradicts the
bounded-expression design used everywhere else in the template language, and
widens the injection surface for no benefit the named-export form does not
already provide.

Keeping islands as the only path was rejected. An island is the right tool for
a stateful subtree, but requiring one for a single button is disproportionate,
and it does not explain why the compiled marker silently did nothing.

Parsing the client module to verify exports at compile time was rejected for
now. It requires a JavaScript parser in the compiler, which is the dependency
question ADR 0007 already answered by deferring to an external toolchain.

## Acceptance Gate

- [x] `on:click="increment()"` compiles to `data-tac-on-click="increment"`.
- [x] Literal arguments survive to the handler, and the event object is passed.
- [x] Executable values are refused with `TY1306`.
- [x] A route binding events without a client module fails closed.
- [x] Handlers fire in a real browser, proven by a CI gate.
