# Tachyon

Tachyon is a polyglot, file-system-routed full-stack framework. Application
developers author standards-based HTML; Tachyon compiles it into prerendered
web output or native-first applications for macOS, iOS, Android, Linux, and
Windows. Unsupported safe native subtrees fall back to isolated local web
surfaces while supported siblings remain native.

`Tac` is the view and interaction layer. `Yon` is the server route and handler
layer. Both use the standalone `ty` executable.

> [!IMPORTANT]
> This branch contains the greenfield Rust rewrite. Engineering phases 0–7 are
> implemented, but the rewrite is not a supported Tachyon release yet. Exact
> promotion evidence and remaining external gates are recorded in
> [docs/CUTOVER.md](docs/CUTOVER.md) and
> [docs/SUPPORT_TIERS.md](docs/SUPPORT_TIERS.md).

## Why Tachyon

- HTML is the only application-facing view language.
- Files and directories define routes, components, handlers, and middleware.
- `yon.html` receives collision-checked context from colocated handler classes.
- Templates support bounded expressions, conditionals, loops, components,
  slots, and server-rendered islands.
- JavaScript, TypeScript, Rust, Dart, Kotlin, Swift, and C# can implement Tac
  component behavior.
- JavaScript, TypeScript, Python, and registered executable adapters can
  implement Yon handlers.
- One project produces deterministic web and native artifacts.
- Generated output, child processes, input sizes, build hooks, and native
  fallback boundaries are explicitly bounded.
- Public manifests, schemas, diagnostics, and handler envelopes are versioned
  contracts under [api/](api/).

## Quick start

The rewrite currently builds from source with stable Rust 1.97.1:

```sh
cargo build --release --locked --bin ty
./target/release/ty init hello --name "Hello"
cd hello
../target/release/ty serve
```

Open `http://127.0.0.1:8080/`. The generated project contains an HTML page at
`client/pages/tac.html`; no JavaScript runtime is required to serve a static
Tac project.

Build and preview production-style static output:

```sh
ty bundle
ty preview
```

The web bundle is written to `dist/web/`. `preview` serves an existing bundle
on `127.0.0.1:3000`; add `--watch` when it should rebuild on source changes.

## Project shape

```text
client/
  pages/
    tac.html
    tac.css
    tac.js
  components/
    product-card/
      tac.html
      tac.css
      tac.js
  shared/
    assets/
    data/
    scripts/
    styles/
server/
  routes/
    products/
      yon.html
      yon.js
      yon.py
  middleware/
    yon.js
  workers/
    cleanup/
      yon.py
tachyon.json
tac.config.js
```

Only the files a project uses are required. A client-only, server-only, or
full-stack project is valid.

## Tac views

A `client/pages/tac.html` file defines a page. Nested directories define URL
paths; a directory whose name begins with `_` defines a dynamic segment.

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Catalog</title>
  </head>
  <body>
    <main>
      <h1>Catalog</h1>
      <product-grid></product-grid>
    </main>
  </body>
</html>
```

Page and component styles are colocated in `tac.css`. Files beneath
`client/shared/` publish at stable `/shared/...` URLs with file-count and byte
limits; symlinks and non-files are rejected.

### Expressions and control flow

Text and safe attribute values can interpolate bounded expressions:

```html
<h1>{title}</h1>
<p class="status-{state}">{message}</p>
```

Structural control tags are compiler syntax. They are resolved before output
and never reach a browser or native renderer as unknown elements.

```html
<if :when="products.length > 0">
  <ul>
    <for :each="product in products">
      <li>{product.name}</li>
    </for>
  </ul>
</if>
<else>
  <p>No products yet.</p>
</else>
```

`logic`, `loop`, `switch`, and `case` remain accepted compatibility spellings;
the normative grammar and expression limits are in
[docs/PHASE_3_SPEC.md](docs/PHASE_3_SPEC.md).

### Components, slots, and islands

A directory under `client/components/` defines a Tac component. Component tags
are resolved at build time and can receive static or bounded expression props.
Slots are scoped and cycles fail before publication.

Client-owned behavior lives in an island. Hydration is explicit: `load`,
`idle`, `visible`, `interaction`, or `never`.

```html
<tachyon-island component="counter" hydrate="interaction">
  <button on:click="count += 1">Count: {count}</button>
</tachyon-island>
```

JavaScript and TypeScript companions are ordinary modules. Rust, Dart, Kotlin,
Swift, and C# component companions compile with their real language toolchains
to the versioned WebAssembly ABI in
[ADR 0011](docs/adr/0011-wasm-companion-abi.md). Run `ty doctor` to probe the
capabilities required by the current project rather than merely checking that
a compiler executable exists.

The released page-state convention remains accepted. A plain page-level
`<script>` may contain only `let`, `const`, or `var` declarations whose values
are bounded JSON-like literals; the compiler removes that block from the HTML
and adds the fields to the colocated page class. Behavior stays in `tac.js`.

```html
<script>let count = 0</script>
<button on:click="count += 1">Add</button>
<output>Count: {count}</output>
```

```javascript
export default class {
  @onMount
  async initialize() {
    // Register host-aware lifecycle behavior here.
  }
}
```

Arbitrary inline JavaScript, attributed script tags, and executable state
initializers fail with `TY1306`; they are never evaluated as authored source by
the compiler.

## Yon routes and HTML context

A `server/routes/**/yon.html` file defines the HTML view for a server route.
Every colocated `yon.*` handler at the same level can contribute context:

```javascript
export class Handler {
  static title = 'Products'

  static async GET() {
    return {
      products: await loadProducts(),
    }
  }
}
```

```html
<!doctype html>
<html lang="en">
  <head><title>{title}</title></head>
  <body>
    <h1>{title}</h1>
    <for :each="product in products">
      <article>{product.name}</article>
    </for>
  </body>
</html>
```

Public static class fields and the selected HTTP method's returned object are
merged into one route context. Multiple same-level handlers compose in
deterministic source order. Duplicate keys, unsupported values, oversized
context, and conflicting declarations fail; there is no implicit
last-writer-wins behavior. Interpolation is escaped by default.

JavaScript and Python have a `Handler` class convenience adapter. Any other
language can participate through the bounded length-prefixed JSON handler
protocol and an explicit interpreter registration. Children are spawned
directly, never through a shell, with bounded input, output, stderr, queueing,
runtime, and cancellation.

## Server runtime

`ty serve` builds the web output, dispatches Yon handlers, and watches project
sources by default. A failed rebuild leaves the last good output available.

```sh
ty serve
ty serve --port 9000
ty serve --host 127.0.0.1 --no-watch
```

The default bind address is loopback. A non-loopback address requires the
explicit `--allow-non-loopback` acknowledgement.

The runtime supports:

- exact and dynamic filesystem routes;
- standard HTTP methods, bodies, headers, and route parameters;
- before/after middleware through the same supervised protocol;
- scheduled workers declared in `.tachyonrc`;
- bounded append-only topic logs exposed as server-sent events;
- defensive response headers and traversal-resistant static serving;
- live reload for development pages.

OpenAPI generation and built-in telemetry from the legacy implementation are
deliberately out of scope. The exact compatibility decision and migration
action for each public feature is in
[docs/PARITY_LEDGER.md](docs/PARITY_LEDGER.md).

## Building targets

```sh
ty bundle --target web
ty bundle --target macos
ty bundle --target ios
ty bundle --target android
ty bundle --target linux
ty bundle --target windows
ty bundle --target all
```

Non-web targets are always native-first; there is no application-wide render
mode flag. The compiler lowers supported HTML and explicit semantic roles to
platform controls. An unsupported safe subtree becomes an isolated local
WebSurface. Native-capable parents and siblings stay native.

| Target | Host | Output |
| --- | --- | --- |
| Web | prerendered HTML, CSS, modules, service worker | `dist/web/` |
| macOS | SwiftUI/AppKit | `dist/macos/*.app` |
| iOS | SwiftUI/UIKit | `dist/ios/*.app` |
| Android | Android platform views | `dist/android/*/*.apk` |
| Linux | GTK4/WebKitGTK | `dist/linux/` |
| Windows | Win32 common controls | `dist/windows/` |

Application authors still write HTML. Standard semantics map automatically;
custom elements opt into a native meaning through explicit roles:

```html
<design-button role="button" aria-label="Save">Save</design-button>
<design-app-bar role="banner" aria-label="Primary navigation">...</design-app-bar>
```

An unknown role or unmapped element remains a WebSurface if the subtree is
safe. Tac components and control tags are resolved first, so they are never
treated as unknown platform elements. Same-origin links inside a local
WebSurface hand navigation back to the native route stack; remote surfaces are
HTTPS-only, host-pinned, bridge-free, and receive no native capabilities.

Native application identity and the entry route are configured in
`tachyon.json`:

```json
{
  "application": {
    "name": "Catalog",
    "id": "com.example.catalog",
    "version": "1.0.0",
    "entry_route": "/"
  }
}
```

Platform output is only as supported as its published evidence. A successful
cross-build is `buildable`, not `native-tested` or `supported`; see
[docs/SUPPORT_TIERS.md](docs/SUPPORT_TIERS.md).

## CLI reference

| Command | Purpose |
| --- | --- |
| `ty init [directory] --name <name>` | Create a minimal HTML project in a missing or empty directory |
| `ty serve [project]` | Build, serve, dispatch handlers, watch, and live-reload |
| `ty bundle [project] --target <target>` | Build web or native artifacts |
| `ty native-bundle [project] --target <target>` | Build a selected native host |
| `ty preview [project] --target <target>` | Serve an existing target's embedded web bundle |
| `ty cache [status\|clean]` | Inspect or remove cache left by an earlier installation |

Useful options:

```text
--diagnostic-format human|json
--out-dir <project-relative-directory>
--no-incremental
--no-bundle
--no-watch
--skip-package
--watch
--host <address>
--hostname <address>
--port <0..65535>
--allow-non-loopback
```

`--target` accepts one target, a comma-separated list, or `all`; aliases such
as `browser`, `darwin`, and `win32` remain accepted. Multi-target output always
lands exactly at `dist/web`, `dist/macos`, `dist/ios`, `dist/android`,
`dist/linux`, and `dist/windows`. `YON_DIST_PATH` selects another output root.
Existing automation may also use `TAC_BUNDLE_TARGET`, `TAC_PREVIEW_TARGET`,
`TAC_TARGET`, `YON_HOST`, `YON_HOSTNAME`, `YON_PORT`, and `YON_SKIP_BUNDLE`.
`TAC_RENDER_MODE` and `--render-mode` are rejected because native-first
subtree planning is unconditional.

The public command names from the latest standalone binary remain accepted.
Internal qualification commands such as `doctor`, `migrate`, and
`handler invoke` are intentionally omitted from normal help until cutover.

## Configuration and hooks

`tac.config.js` may export a bounded `postBundle` hook. It executes against the
staging directory before atomic publication. The hook must finish within its
deadline and its output is re-walked with file-count, per-file, total-byte, and
symlink limits.

```javascript
export default {
  async postBundle({ distRoot, targetRoots }) {
    // Generate robots.txt, sitemap.xml, or other deterministic deployment files.
  },
}
```

Node.js is used by default with Bun as a fallback. Set
`TACHYON_JAVASCRIPT_RUNTIME` to select an explicit executable.

## Determinism and failure behavior

- Discovery and route ordering are canonical.
- Builds publish through a staging directory and atomic swap.
- Incremental state is untrusted and reused only after digest verification.
- Failed builds retain the previous complete output.
- Repeated clean builds are byte-identical for identical inputs and toolchains.
- Artifact Manifest v1 records target, inputs, outputs, toolchains, contracts,
  and digests.
- Diagnostics use stable `TY####` codes and can be emitted as JSON.
- Generated native source is compiled by each real platform toolchain during
  target builds; source-shape tests do not substitute for that compilation.

## Security boundary

Tachyon defaults to loopback networking, escaped interpolation, deny-by-default
native capabilities, no shell execution, no WebSurface bridge, strict local
asset schemes, and bounded external input. Threats and residual risks are
documented in [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

Handler children still run with the developer account's ambient filesystem and
network access. The supervisor constrains the protocol and lifecycle, not the
operating system sandbox. Deploy handlers inside the isolation boundary your
environment requires.

## Migration from the released implementation

Run the compatibility analyzer before switching a project:

```sh
ty migrate check path/to/project
ty migrate check path/to/project --json
```

The report classifies findings as `supported`, `changed`, or `unsupported` and
names a required action for every non-supported result. The neutral
compatibility corpus compares route graphs, HTTP status, and semantic browser
output against the latest standalone binary; generated artifacts are not
compared because their internal shape intentionally differs.

## Verification

The canonical local gate is:

```sh
cargo fmt --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
cargo deny check advisories bans licenses sources
```

Browser, compatibility, native, recovery, soak, performance, and release
lifecycle commands are recorded with their results in the corresponding
`docs/PHASE_*_EVIDENCE.md` files. Claims that require unavailable hardware or
credentials remain open rather than being inferred from source inspection.

## Architecture and contribution

Start with:

- [CONTEXT.md](CONTEXT.md) for product language and invariants;
- [docs/architecture/OVERVIEW.md](docs/architecture/OVERVIEW.md) for the data
  path and dependency direction;
- [docs/PARITY_LEDGER.md](docs/PARITY_LEDGER.md) for compatibility decisions;
- [docs/ENGINEERING_STANDARDS.md](docs/ENGINEERING_STANDARDS.md) for the test,
  security, and review bar;
- [CONTRIBUTING.md](CONTRIBUTING.md) for the contributor workflow;
- [docs/RELEASE_ENGINEERING.md](docs/RELEASE_ENGINEERING.md) for release and
  rollback policy.

The Rust implementation is greenfield. The remaining JavaScript tree is a
behavioral oracle and migration corpus; new Rust code does not import or copy
its private internals.

## License

Tachyon is licensed under the terms in [LICENSE](LICENSE).
