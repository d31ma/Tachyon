# Phase 3 Tac View Semantics

> Superseded by ADR 0015 and ADR 0016. Tac is exclusively client-rendered;
> Yon is REST-only and never enters the view pipeline.

## Outcome

The compiled Rust `ty build` command lowers dynamic Tac HTML into deterministic
View IR v1, emits bounded client render plans, publishes source maps, and reuses
verified unchanged route artifacts. It never invokes Yon handlers.

Phase 3 is a build and client-render-plan milestone. It does not add production
HTTP handler dispatch, SPA navigation, native rendering, or a compatibility
implementation of every legacy expression.

## Template Language

Application developers continue to author HTML. A template may use:

- escaped text interpolation: `{title}` or `{product.name}`;
- dynamic attributes and component properties: `:aria-label="title"`;
- legacy-compatible conditionals:
  `<logic :if="featured">`, `<logic :else-if="archived">`, and
  `<logic else>`;
- direct conditional aliases:
  `<if :when="featured">` followed by `<else>`;
- legacy-compatible iteration:
  `<loop :for="product of products">`;
- direct iteration aliases:
  `<for :each="product in products">`;
- registered Tac components using custom-element-shaped tags;
- component `hydrate` policies: `load`, `idle`, `visible`, `interaction`, and
  `never`;
- `<slot>` inside a component template.

Whitespace-only text and comments may occur between conditional branches.
An `else-if` or `else` without an immediately preceding conditional chain is a
diagnostic. Control elements are compiler syntax and never appear in emitted
HTML or View IR as ordinary elements.

The maximum source size remains 1 MiB. Expression length is at most 1,024
bytes, expression nesting at most 32, template/component nesting at most 64,
expanded nodes at most 100,000 per route, iterations at most 10,000 per
control, generated route artifacts at most 10 MiB per route, and diagnostics at
most 100 per build.

## Expression Contract

Expressions are parsed by Tachyon and never passed to `eval`, a shell, or a
language runtime. The Phase 3 expression grammar supports:

- JSON strings, finite numbers, booleans, and `null`;
- identifiers and dotted object paths;
- array indexes using a non-negative integer literal;
- unary `!`;
- `===`, `!==`, `==`, `!=`, `<`, `<=`, `>`, and `>=`;
- `&&`, `||`, and parentheses.

Truthiness is defined over JSON values: `false`, `null`, numeric zero, and an
empty string are false; arrays and objects are true. Equality compares JSON
values without coercion; `==` and `===` are aliases, as are `!=` and `!==`.
Ordering accepts two numbers or two strings. A missing key, invalid index,
unsupported operator/call, non-finite number, or type mismatch is a
source-located diagnostic.

Every web view receives immutable browser built-ins `platform`, `environment`,
`os`, and `target`, each set to `"web"`. Yon does not contribute view values.

Interpolated values are HTML-escaped. Dynamic attribute values are
attribute-escaped; `null` and `false` omit the attribute, `true` emits an empty
attribute, and strings/numbers emit text. Raw HTML interpolation, function
calls, assignment, constructors, prototype access, and executable event
attributes are rejected.

## Yon Boundary

Yon is REST-only under ADR 0016. `ty build` discovers handler routes for
Route Manifest v1 but does not invoke them. `yon.html`, route-context
composition, and the private `view.context` operation are unsupported.
A handler that explicitly returns `Content-Type: text/html` supplies opaque
response bytes; Tachyon does not evaluate that body as a template.

## Tac Components

Components live below `client/components/**/tac.html`. Each directory segment
uses lowercase ASCII letters and digits, and the path segments joined by `-`
form the tag. Phase 3 requires at least two segments so the resulting name is a
standards-shaped custom element, for example:

```text
client/components/product/card/tac.html -> <product-card>
```

An unregistered hyphenated element remains a web component. An unknown
non-HTML, non-control, non-component element is a diagnostic.

Static component attributes become string properties and `:property`
attributes become evaluated JSON values. Reserved `hydrate` is not a property.
The component template sees its properties and browser-owned render scope; Yon
data is obtained through REST requests, never ambient route context.
Invocation children replace `<slot>`; a component without a slot rejects
non-whitespace children. Component expansion is recursive, cycle-checked, and
bounded.

An optional colocated `tac.js` is a browser activation module. Other `tac.*`
companions remain deferred. The module exports a default class:

```javascript
export default class ProductCard {
  constructor(props) {
    this.props = props
  }

  async mount(root, signal) {
    // Attach behavior after the browser renderer creates this subtree.
  }
}
```

The runtime constructs the class once per component occurrence and calls
`mount(root, AbortSignal)`. A legacy `hydrate` method is accepted only as a
lifecycle alias; there is no server DOM to hydrate.

## Component Mount Contract

A component with a `tac.js` companion and no explicit policy activates with
`load`. A component without a companion remains static. Explicit policies
behave as follows:

| Policy | Activation |
| --- | --- |
| `load` | when the external island runtime scans the document |
| `idle` | `requestIdleCallback`, with a bounded timer fallback |
| `visible` | intersection within a 100px margin, with immediate fallback |
| `interaction` | first pointer, keyboard, submit, input, or focus interaction |
| `never` | never loads a module or serializes properties |

Every scheduled component receives a deterministic occurrence ID. Properties
remain public, non-authoritative browser input. The browser renderer creates
the component DOM and then runs its scheduled companion lifecycle.

The runtime is an external module with no inline executable code. It validates
same-origin generated module URLs before import. On activation failure it
preserves the browser-rendered DOM, reports a bounded error, and permits a
later interaction retry.

`hydrate` must be a literal supported policy. A dynamic, empty, or unsupported
policy, or any activating policy without `tac.js`, fails the build.

## View IR and Source Maps

Each compiled page emits:

- its normal `index.html`;
- `.tachyon/view-ir/<route-key>.json`, conforming to View IR v1;
- `.tachyon/source-maps/<route-key>.map.json`, conforming to View Source Map
  v1.

View IR contains structural expressions rather than evaluated values. The
source map records generated artifact byte ranges back to portable
project-relative source byte ranges. It may
name a page or component source and is canonically ordered. Generated wrapper
markup has no fabricated source mapping.

Neither View IR nor source maps include Yon response values, runtime component
state, environment values, or application secrets.

## Incremental Build

Incremental reuse is enabled by default. The published
`.tachyon/build-state.json` records only canonical input digests and output
digests—never Yon response values.

A Tac route may reuse its verified bootstrap document, client render plan,
View IR, and source map when its view, the complete component registry,
compiler version, target, and relevant options are unchanged. Yon handler-only
routes produce no view artifact and never execute during a build.

Reuse reads only regular files under the prior output, verifies every recorded
SHA-256 digest and path, and otherwise recompiles. Missing, malformed,
unsupported-version, symlinked, or digest-mismatched state is a safe cache miss,
not a build failure. Publication remains atomic and a failed build preserves
the complete prior output.

`ty build --no-incremental` disables reuse. CLI output reports compiled and
reused route counts. Identical inputs still produce byte-identical published
artifacts.

## Diagnostic Recovery

Discovery, parsing, structural validation, expression validation, component
resolution, and cycle checking never execute a Yon handler.
Independent source failures are accumulated in canonical source/span order,
bounded to 100 diagnostics, and emitted through Diagnostics v1. A failed build
does not publish partial HTML, manifests, IR, source maps, runtime assets, or
incremental state.

## Stable Failure Families

- `TY1301`-`TY1399`: template structure, controls, expressions, interpolation,
  attribute, or expansion failures;
- `TY1401`-`TY1499`: component discovery, properties, slots, cycles, companion,
  or island failures;
- `TY1501`-`TY1599`: reserved historical diagnostics; Yon has no view-context
  operation;
- `TY1601`-`TY1699`: View IR, source-map, generated asset, or incremental-state
  failures.

## Acceptance Criteria

- The compiled binary emits client plans for canonical and compatibility control tags,
  escaped interpolation, dynamic attributes, nested loops, condition chains,
  component properties, slots, web components, and all island policies.
- JavaScript and Python Yon handlers are discovered without execution, and
  `yon.html` is rejected.
- Invalid handler response shapes, handler failures, component
  cycles, missing components, malformed controls, unsupported expressions,
  missing values, unsafe HTML, invalid islands, expansion limits, and output
  limits fail with stable source-aware diagnostics.
- Control tags never survive as browser custom elements; they lower to client
  render-plan instructions.
- Island IDs, HTML, View IR, source maps, manifests, assets, and build state are
  byte-identical for identical inputs.
- An unchanged eligible Tac route is verifiably reused; a changed component
  invalidates consumers; Yon handler routes are not rendered; corrupted state
  safely recompiles.
- Multiple independent template errors are reported together and failed builds
  preserve last known-good output.
- Real-binary browser tests prove client initial rendering, component mount
  scheduling, reactive structural updates, and interaction handling.
- Formatting, check, Clippy, tests, rustdoc, coverage, supply-chain, legacy
  compatibility, macOS, Linux-container, Windows-buildability, and native CI
  gates pass.

## Out of Scope

Arbitrary JavaScript template expressions, raw HTML bindings, event bindings,
general-purpose reactivity, two-way binding, Tac TypeScript/polyglot companions,
component CSS scoping, SPA routing, production HTTP handler
dispatch, handler pooling, native rendering, WebSurface planning, and stable
release packaging belong to later compatibility or platform phases.
