# ADR 0015: Tac Is Exclusively Client Rendered

- Status: Accepted
- Date: 2026-08-10
- Supersedes: ADR 0006, ADR 0009, ADR 0010, and the production-rendering assumptions of ADR 0013

## Context

Tac is the browser application framework and Yon is the server framework.
Earlier phases made Tac templates server-rendered HTML and then activated
selected islands. That divided one view across two owners: the build/server
evaluated text and structure, while the browser could refresh only selected
expressions. In particular, an `if`, `else`, `for`, or `loop` whose value
changed in a companion could not reliably change the DOM in the frontend.

That split is also an architectural ambiguity. A Tac application should not
silently acquire server-rendering behavior merely because the compiler can
evaluate one value at build time.

## Decision

Tac has no server-side rendering mode.

For every `tac.html` route, the compiler emits:

1. a bounded, versioned JSON render plan containing the parsed template and
   expression ASTs;
2. a small bootstrap document that contains no rendered Tac view subtree; and
3. the shared Tac browser renderer.

The browser renderer owns initial DOM creation and every subsequent rerender.
It evaluates interpolations, dynamic attributes, `if`/`else`, `for`/`loop`,
switches, slots, components, and event bindings. It interprets compiler-owned
AST data and never uses `eval`, `Function`, or authored expression source.

The compiler may validate and serialize literal initial state. It must not
evaluate a Tac expression, choose a Tac conditional branch, expand a Tac loop,
or emit a rendered Tac component subtree into the response document.

The existing `hydrate=` component attribute remains temporarily accepted as a
compatibility spelling for a browser **mount schedule**. It does not identify
server HTML and does not authorize hydration. New runtime APIs use `mount` and
`tachyon-component`. A legacy companion `hydrate(root, signal)` method may be
called after the browser has created the subtree, solely as a compatibility
lifecycle hook.

Yon is outside the rendering pipeline. It dispatches REST endpoints and passes
their validated HTTP responses through unchanged; see ADR 0016.

Native targets run the same client render plan in platform web views; see
[ADR 0018](0018-native-hosts-run-the-platform-javascript-engine.md). They must
not obtain Tac structure by asking the web compiler to SSR it. Target-native
page companions supply platform behavior under ADR 0019.

## Consequences

- Tac structural state changes work entirely in the frontend.
- Tac output has one owner and no hydration mismatch class of bugs.
- A no-JavaScript browser receives an explicit `noscript` message, not a
  partially interactive application.
- A Yon handler may return HTML explicitly, but that body is not interpreted as
  a Tachyon template.
- Existing SSR/island snapshots and hot-update assumptions must migrate to the
  client render-plan boundary.
- Content Security Policy remains tractable because the render plan is data,
  the runtime is external, and authored expressions are never executed as
  source.

## Rejected Alternatives

- **SSR only for static-looking Tac nodes**: makes ownership depend on compiler
  inference and reintroduces hydration semantics.
- **SSR conditionals but client loops**: exposes two incompatible structural
  models in one template.
- **An opt-in SSR flag for Tac**: violates the framework boundary and makes
  deployments behavior-dependent.
- **Send expression source and call `eval`**: weakens CSP and turns templates
  into an unbounded code-execution surface.

## Acceptance Gate

- a Tac response contains no rendered authored view subtree or island wrapper;
- the browser proves initial and reactive interpolation, conditional, loop,
  component, and event rendering;
- a Yon route publishes the handler's validated HTTP response without template
  rendering or value injection;
- native planning does not consume server-rendered Tac output;
- repository docs describe Tac as client-only and Yon as REST-only;
- canonical format, check, clippy, test, documentation, dependency-policy, and
  browser gates pass.
