# Phase 3 Tac and Yon View Semantics

## Outcome

The compiled Rust `ty build` command lowers dynamic Tac and Yon HTML into
deterministic View IR v1, renders complete web documents, composes build-time
Yon route context from every colocated JavaScript and Python handler, expands
Tac components, emits independently activated islands, publishes source maps,
and reuses verified unchanged route artifacts.

Phase 3 remains a build and prerender milestone. It does not add production
HTTP handler dispatch, mutable browser reactivity, SPA navigation, native
rendering, or a compatibility implementation of every legacy expression.

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
control, rendered HTML at most 10 MiB per route, and diagnostics at most 100
per build.

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

Every web view receives immutable built-ins `platform`, `environment`, `os`,
and `target`, each set to `"web"`. Handler context may not replace these
reserved names.

Interpolated values are HTML-escaped. Dynamic attribute values are
attribute-escaped; `null` and `false` omit the attribute, `true` emits an empty
attribute, and strings/numbers emit text. Raw HTML interpolation, function
calls, assignment, constructors, prototype access, and executable event
attributes are rejected.

## Yon Route Context

Before rendering a `yon.html` route, Tachyon invokes every same-level
`yon.js` and `yon.py` handler in canonical source-path order using Handler
Protocol v1 operation `view.context` and method `GET`.

For JavaScript:

- `Handler` may be a named or default-exported class;
- enumerable own static data fields contribute static values;
- functions, accessors, `name`, `length`, and `prototype` do not;
- static `GET(request)` may be synchronous or asynchronous.

For Python:

- the module defines a `Handler` class;
- public class data attributes contribute static values;
- methods, descriptors, and underscore-prefixed attributes do not;
- `GET` must be a `@staticmethod` and may be synchronous or asynchronous.

`GET` must return a plain JSON object. Static values and response values must
be JSON-compatible. Duplicate keys within one handler or across handlers fail
with both contributor paths; there is no last-writer-wins behavior. Context
keys use `[A-Za-z_$][A-Za-z0-9_$]*`, at most 1,024 keys may be composed, JSON
depth is at most 32, and the complete canonical context is at most 1 MiB.

Handler-only API routes are discovered and manifested but are not invoked by a
build. Tac pages have an empty route context; their component properties must
therefore derive from literals and loop/component locals in this phase.

Route Manifest v1 records the sorted static and response export names observed
during the successful build.

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
The component template sees only its properties, not ambient route context.
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

  async hydrate(root, signal) {
    // Adopt the existing server-rendered DOM.
  }
}
```

The runtime constructs the class once per component occurrence and calls
`hydrate(root, AbortSignal)`. It never replaces useful SSR DOM before
activation succeeds.

## Island Contract

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

Every island is fully server-rendered and receives a deterministic occurrence
ID. Properties are serialized as strict JSON in an escaped data attribute and
are public, non-authoritative browser input. `<`, U+2028, and U+2029 are
escaped before attribute escaping.

The runtime is an external module with no inline executable code. It validates
same-origin generated module URLs before import. On activation failure it
preserves SSR DOM, sets `data-tachyon-island-error`, emits a bounded console
error, and permits a later interaction retry. Interaction activation prevents
the triggering native action, activates once, then replays one event only
after success.

`hydrate` must be a literal supported policy. A dynamic, empty, or unsupported
policy, or any activating policy without `tac.js`, fails the build.

## View IR and Source Maps

Each rendered page emits:

- its normal `index.html`;
- `.tachyon/view-ir/<route-key>.json`, conforming to View IR v1;
- `.tachyon/source-maps/<route-key>.map.json`, conforming to View Source Map
  v1.

View IR is produced before context evaluation and contains structural
expressions rather than evaluated secrets. The source map records generated
HTML byte ranges back to portable project-relative source byte ranges. It may
name a page or component source and is canonically ordered. Generated wrapper
markup has no fabricated source mapping.

Neither View IR nor source maps include handler context values, island
properties, environment values, or application secrets.

## Incremental Build

Incremental reuse is enabled by default. The published
`.tachyon/build-state.json` records only canonical input digests and output
digests—never context values.

A Tac route or a Yon route without handlers may reuse its verified HTML, View
IR, and source map when its view, the complete component registry, compiler
version, target, and relevant options are unchanged. A Yon route with handlers
always recomposes context and rerenders because external state may have
changed.

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
resolution, and cycle checking complete before any Yon handler executes.
Independent source failures are accumulated in canonical source/span order,
bounded to 100 diagnostics, and emitted through Diagnostics v1. A failed build
does not publish partial HTML, manifests, IR, source maps, runtime assets, or
incremental state.

## Stable Failure Families

- `TY1301`-`TY1399`: template structure, controls, expressions, interpolation,
  attribute, or expansion failures;
- `TY1401`-`TY1499`: component discovery, properties, slots, cycles, companion,
  or island failures;
- `TY1501`-`TY1599`: Yon route-context invocation, shape, collision, or budget
  failures;
- `TY1601`-`TY1699`: View IR, source-map, generated asset, or incremental-state
  failures.

## Acceptance Criteria

- The compiled binary renders both canonical and compatibility control tags,
  escaped interpolation, dynamic attributes, nested loops, condition chains,
  component properties, slots, web components, and all island policies.
- JavaScript and Python static fields plus async `GET` objects compose into one
  deterministic Yon context.
- Duplicate context keys, invalid response shapes, handler failures, component
  cycles, missing components, malformed controls, unsupported expressions,
  missing values, unsafe HTML, invalid islands, expansion limits, and output
  limits fail with stable source-aware diagnostics.
- Control tags never survive in HTML or View IR as elements.
- Island IDs, HTML, View IR, source maps, manifests, assets, and build state are
  byte-identical for identical inputs.
- An unchanged eligible route is verifiably reused; a changed component
  invalidates consumers; a Yon handler route is always rerendered; corrupted
  state safely recompiles.
- Multiple independent template errors are reported together and failed builds
  preserve last known-good output.
- Real-binary browser tests prove island scheduling, SSR preservation,
  activation, failure marking, and interaction replay.
- Formatting, check, Clippy, tests, rustdoc, coverage, supply-chain, legacy
  compatibility, macOS, Linux-container, Windows-buildability, and native CI
  gates pass.

## Out of Scope

Arbitrary JavaScript template expressions, raw HTML bindings, event bindings,
reactive rerendering, two-way binding, Tac TypeScript/polyglot companions,
component CSS scoping, SPA routing, streaming SSR, production HTTP handler
dispatch, handler pooling, native rendering, WebSurface planning, and stable
release packaging belong to later compatibility or platform phases.
