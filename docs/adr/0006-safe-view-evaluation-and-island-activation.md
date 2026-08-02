# ADR 0006: Safe View Evaluation and Island Activation

- Status: Accepted
- Date: 2026-07-26

## Context

Phase 3 must render handler data into HTML, lower structural controls, expand
components, and activate browser islands. Treating template expressions as
JavaScript would make builds runtime-dependent, permit arbitrary code during
rendering, complicate native portability, and enlarge the injection surface.

Island hydration also needs an explicit browser boundary. Replacing
server-rendered DOM during startup would discard useful HTML and make deferred
activation visibly unstable.

## Decision

Parse expressions into a bounded Tachyon grammar evaluated only over JSON
values. The grammar provides paths, indexes, literals, boolean operators, and
comparisons; it has no calls, assignment, construction, prototype access, or
raw HTML primitive. Render text and attributes with context-appropriate
escaping.

Render every Tac component on the server. A component's optional `tac.js`
exports a default class whose constructor receives public JSON properties and
whose `hydrate(root, signal)` method adopts the existing DOM. A generated
external runtime schedules `load`, `idle`, `visible`, and `interaction`
activation; `never` ships no module or properties.

The compiler emits View IR before evaluation and source maps after rendering.
Neither artifact contains route-context values or serialized island
properties.

## Consequences

- Builds are deterministic across JavaScript and Python handler languages.
- Expression errors are source-located and cannot execute application code.
- The future native backend consumes the same View IR expression structure.
- Island DOM remains useful before and after an activation failure.
- The Phase 3 expression language is intentionally smaller than the legacy
  JavaScript-compatible renderer and requires later compatibility decisions.
- Browser companions have a small class-oriented lifecycle surface that can be
  extended additively.

## Rejected Alternatives

- **JavaScript `eval` or generated functions**: violates deterministic,
  portable, and injection-resistant compilation.
- **Execute expressions independently in each handler language**: creates
  cross-language semantic drift.
- **Client-only components**: harms first render, accessibility, and
  failure recovery.
- **Replace SSR markup during hydration**: introduces visual churn and loses
  useful content on module failure.
- **Inline executable hydration code**: weakens CSP and complicates auditing.

## Acceptance Gate

- expression parsing and evaluation have positive, negative, depth, and size
  tests;
- text, attribute, JSON-script, and URL contexts have adversarial escaping
  tests;
- every control lowers out of View IR element output;
- browser tests prove SSR adoption and failure preservation;
- source maps point generated ranges to page and component inputs;
- the threat model documents public island properties and handler execution.
