# Phase 4 macOS Native Vertical Slice

## Status and Scope

Phase 4 is a development milestone for one native target: macOS arm64. It
consumes the resolved output of the Phase 3 view pipeline and emits a SwiftUI
application whose supported subtrees are native and whose unsupported safe
subtrees become isolated WebSurfaces.

Application authors continue to write HTML. `--target macos` selects the
packaging target; there is no application-wide render-mode switch. Phase 4
does not implement iOS, Android, Windows, or Linux native hosts.

## Command Contract

```text
ty build [PROJECT] --target web
ty build [PROJECT] --target macos
```

`web` remains the default. A macOS build publishes `OUT_DIR/macos` atomically
and produces:

- a runnable `.app` bundle;
- canonical Native UI v1 for every page route;
- a native route index;
- the generated Swift host source;
- the resolved web reference bundle;
- one local document for every WebSurface fallback;
- Capability Manifest v1 and Artifact Manifest v1.

The application name, identifier, version, and entry route come from an
optional, UTF-8, bounded, non-symlinked `tachyon.json`:

```json
{
  "application": {
    "name": "Native Catalog",
    "id": "dev.example.native-catalog",
    "version": "0.1.0",
    "entry_route": "/"
  }
}
```

Unknown configuration keys, invalid identifiers, missing entry routes, unsafe
paths, and unsupported targets fail before publication. A failed build retains
the previous complete native output.

## Authored View Boundary

Native planning lowers authored Tac declarations and structural instructions;
it never consumes Yon responses or server-rendered web output. Consequently:

- Yon handlers and their response bodies never reach a native view adapter;
- `logic`, `loop`, `if`, `else`, and `for` remain framework instructions rather
  than unknown native elements;
- each native node has a deterministic route-local identifier;
- source and generated artifact budgets from Phase 3 remain in force.

The browser-owned Tac result is the behavioral comparison oracle. Native
planning never executes a Yon handler or evaluates JavaScript.

## Native Adapter Set

Phase 4 maps the following HTML semantics:

| HTML semantics | Native UI adapter |
| --- | --- |
| `html`, `body`, `main`, `section`, `article`, `div`, `header`, `footer`, `nav`, `form` | `layout.column` |
| custom element with `role="banner"` | `layout.app_bar` |
| `ul`, `ol`, `li` | `layout.list`, `layout.list_item` |
| `h1` through `h6` | `text.heading1` through `text.heading6` |
| `p`, `span`, `label`, `strong`, `em`, `small`, `code`, `pre` | `content.text` |
| `button`, or a custom element with `role="button"` | `control.button` |
| bounded text-like `input` and `textarea` | `control.text_field` |
| `output` | `content.output` |
| `details` and `summary` | `control.disclosure` |
| contained route links | `navigation.link` |
| `img` | `content.image` |
| `hr` | `content.divider` |

Document metadata does not become UI. Unsupported standard elements,
unadapted custom elements, and unsupported Tac subtrees become the smallest local
WebSurface subtree that preserves their content. An HTTPS `iframe` becomes a
remote WebSurface with no bridge. Other remote schemes fail closed.

## Declarative Controller Contract

Phase 4 adds a deliberately small, HTML-authored controller state machine:

- `data-tachyon-bind="name"` binds an input or output to a named scalar;
- `data-tachyon-state="value"` declares the initial scalar value;
- `data-tachyon-action="increment:name"` increments a numeric scalar;
- `data-tachyon-action="toggle:name"` toggles a boolean scalar.

Names match `[A-Za-z_][A-Za-z0-9_]{0,63}`. Duplicate initial declarations,
missing action state, non-numeric increments, and invalid action syntax fail
the native build. State is route-local. Input, action, route, activation,
suspension, and termination transitions are bounded and logged without user
values.

The macOS model owns the lifecycle `created -> mounted -> active <-> suspended
-> destroyed`. WebSurface coordinators own `created -> attached -> detached ->
destroyed`. No lifecycle callback is delivered after destruction.

Arbitrary browser JavaScript controller compatibility, mutable Tac class
fields, networking, persistence, and cross-route state are deferred.

## Accessibility Contract

Native planning derives semantic roles from HTML and accessible names in this
order:

1. `aria-label`;
2. `alt` for images;
3. associated semantic text for buttons, links, headings, disclosures, and
   fallback subtrees.

Interactive elements without a non-empty accessible name fail the build.
`aria-hidden="true"` removes the subtree from native accessibility. SwiftUI
receives stable accessibility identifiers, labels, headings, and control
traits. Native and web evidence compare role/name order, focusable controls,
input updates, disclosure behavior, and action results.

## WebSurface Security

Local fallback documents:

- are generated inside the application bundle;
- use a restrictive generated CSP;
- receive a non-persistent `WKWebsiteDataStore`;
- receive no script message handler or ambient native bridge;
- may load only contained bundle resources;
- cannot navigate the host window or open a remote URL.

Remote WebSurfaces:

- require HTTPS;
- have `bridge: none`;
- disable content JavaScript;
- receive a non-persistent data store;
- cannot navigate away from their declared origin;
- never inherit local capability permissions.

Capability Manifest v1 always has `default_policy: deny` and
`remote_content_bridge: false`. Phase 4 declares no native capabilities.

## Limits and Diagnostics

- configuration: 64 KiB;
- routes: existing project limit;
- native nodes: 100,000 per route;
- native depth: 64;
- WebSurfaces: 1,024 per application;
- fallback document: 10 MiB;
- scalar state entries: 1,024;
- scalar value: 4 KiB;
- generated Swift source: 4 MiB;
- lifecycle log message: 512 bytes.

Phase 4 diagnostics use `TY16xx`. Diagnostics identify the route and source
where possible, remain bounded, and never include handler values or fallback
document bodies.

## Acceptance Gate

Phase 4 is complete only when:

- compiled-binary tests prove client control plans, Yon non-execution,
  expanded components, native adapters, accessibility metadata, declarative
  state, and subtree-local fallback;
- adversarial tests prove invalid config, state, remote URLs, adapter inputs,
  fallback budgets, and failed-build rollback fail closed;
- a real arm64 macOS `.app` compiles, launches, exposes a native SwiftUI
  hierarchy, accepts a button and text-field interaction, and records valid
  lifecycle transitions;
- the macOS accessibility hierarchy and the Chromium mobile-web reference have
  matching required roles and accessible names;
- native and web screenshots pass the documented coarse visual-layout budget;
- formatting, checking, strict Clippy, tests, rustdoc, coverage, supply-chain,
  legacy compatibility, Linux-container, Windows-cross, and release gates
  remain green.

Phase 4 does not promote any artifact to preview or supported status.

The screenshot budget normalizes both 420 by 780 captures to 84 by 156
samples, derives foreground and edge distributions independently of
light/dark appearance, and requires: nonblank foreground density between 1%
and 75%, foreground-centroid distance no greater than 0.22, vertical
foreground earth-mover distance no greater than 0.16, and edge-density ratio
no greater than 4. This deliberately compares information placement and
coverage while allowing platform-native fonts, controls, title bars, and
colors.
