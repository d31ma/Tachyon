# 0010 — Island-local client evaluation

## Status

Superseded by [0015](0015-tac-is-exclusively-client-rendered.md).

## Context

A component template renders before any companion runs, so a field the
companion assigns is never in render scope. Building the project's own website
made the cost concrete: after every parser gap was closed, 19 diagnostics
remained and most had this single cause. A panel whose data comes from its
companion cannot render at all.

ADR 0009 declined to reproduce the legacy client renderer, and that decision
stands for pages: a page is a real document, navigated with view transitions,
rendered on the server. But an island is already the declared boundary where
client behaviour is allowed. Refusing to evaluate anything there leaves the
boundary unable to do the one job it exists for.

Two options were considered.

**Migrate the affected views to the server-rendered model.** Correct for an
application with a Yon route behind it, and wrong for the frontend-only case:
there is no handler to compute the value in, and for the website it would
delete the polyglot gallery that is the site's content.

**Let an island evaluate against its companion.** The island is already a
declared boundary with a module, a lifecycle, and a hydration policy. Adding
evaluation inside it changes no page, no route, and no document.

## Decision

Inside an island, and only inside an island, an expression the server cannot
resolve is deferred to the client instead of failing the build.

The compiler serialises the parsed expression to JSON and emits it as a
marker. After the island's companion is constructed, the runtime evaluates the
marker against the instance and fills in the result.

Three properties are preserved:

- **Nothing is evaluated as source.** The expression is already parsed into a
  bounded AST; the client interprets that AST. There is no `eval`, no `new
  Function`, and no JavaScript parser in the runtime. This is the same shape
  already used for event bindings.
- **The boundary is declared, not inferred.** A page, a route view, and a
  non-hydrated component still resolve every expression on the server, and
  still fail the build when they cannot. Deferral requires `hydrate`.
- **Server rendering stays preferred.** An expression the server *can* resolve
  is resolved there, so content is in the document before any script runs.
  Deferral is a fallback, not a mode.

A method call is permitted in a deferred expression, because the instance the
call resolves against exists on the client. `await` is permitted for the same
reason: client evaluation is async, so an awaited companion method resolves.
Both remain refused everywhere else, for the reason they always were: there is
nothing to call and nothing to await at render time.

## Consequences

- A deferred value is absent from the server-rendered HTML. It is not in the
  initial paint, not in view-source, and not visible to a client with scripting
  disabled. An island already carried that property for its behaviour; it now
  carries it for the deferred parts of its content.
- The runtime grows an expression interpreter. It is bounded by the same
  grammar the compiler accepts, so it cannot drift into a general evaluator.
- Control flow inside an island — `<loop>` and `<logic :if>` over companion
  data — is **not** covered by this decision. Deferring a subtree rather than a
  value requires shipping the template, which is a larger change and a separate
  decision.

## Addendum: assignment and refresh

A binding inside an island resolves against that island's companion, and may
assign to a field on it. After a handler or an assignment runs, the island
re-evaluates its own deferred expressions and updates their text nodes.

This is deliberately not a reactivity graph. An island's expressions are a
small fixed list known at build time, so re-running all of them is both
simpler to implement and cheaper to execute than tracking which one depends on
what: there is no dependency registration, no invalidation, and no diffing. The
cost is bounded by the island's own template, not the page.

Assignment stays refused outside an island, because there is no instance to
write to. Only a binding outside every island requires the page's client
module; an island's bindings resolve on its companion.
