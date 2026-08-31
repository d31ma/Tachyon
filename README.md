# Tachyon

Tachyon is a polyglot, file-system-routed full-stack framework. Application
developers author standards-based HTML; Tachyon compiles Tac into client-rendered
web applications and dispatches Yon REST endpoints, or packages those views in
native web-view hosts for macOS, iOS, Android, Linux, and Windows. Target-native
page companions provide access to the platform's own SDKs.

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
- Tac templates support browser-rendered bounded expressions, conditionals,
  loops, components, and slots. Tac has no SSR mode.
- Yon handlers return HTTP status, headers, and bodies. Tachyon does not render
  Yon templates or execute handlers during a build.
- JavaScript and TypeScript implement browser component behavior; Swift,
  Kotlin, C#, and Rust provide target-specific native page behavior.
- Yon runs JavaScript, TypeScript, Python, Java, C#, Kotlin, PHP, and Rust.
  Every server layer declares its mandatory stereotype; existing programs in
  other languages stay behind an explicit `@Delegate` + `@Relay` boundary.
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
`client/pages/tac.html`; the external Tac runtime constructs its DOM in the
browser from a compiler-produced render plan.

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
      yon.js
  services/
  repositories/
  clients/
  delegates/
  workers/
    cleanup/
      yon.py
middleware.py
.tachyonrc        # worker intervals only
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

`logic` and `loop` are the canonical conditional and iteration tags; the
`if`/`else`/`for` spellings above remain accepted. Counted loops also work:
`<loop :for="let i = 0; i < 3; i++">{i}</loop>`. Comparisons, step direction,
and iteration limits are validated. See
[the client runtime migration guide](docs/CLIENT_RUNTIME_MIGRATION.md).

### Components, slots, and browser mounting

A directory under `client/components/` defines a Tac component. Component tags
are resolved at build time and can receive static or bounded expression props.
Slots are scoped and cycles fail before publication.

Tac is entirely client-owned. Component mounting may be scheduled as `load`,
`idle`, `visible`, `interaction`, or `never`. The legacy `hydrate=` attribute
is accepted as a mount-scheduling spelling; it does not enable SSR.

```html
<counter-panel hydrate="interaction"></counter-panel>
```

Browser components use JavaScript or TypeScript modules. Page companions may
add Swift on Apple targets, Kotlin on Android, C# on Windows, or Rust on desktop
targets; those compile into the native host, not WebAssembly. Keep a JS/TS page
companion for a web build. `ty doctor` reports the required toolchains.

Every page/component instance receives `this.tac` for rendering, safe opt-in
response caching, asset precaching, and document-local publish/subscribe.
`$field` persists per tab and `$$field` across sessions; neither is secure
storage. `@onMount`, `@publish`, and `@subscribe` are compiled into lifecycle
metadata. See [the API and migration guide](docs/CLIENT_RUNTIME_MIGRATION.md)
for cache privacy rules, bounds, cleanup, and native asynchronous calls.

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

Nested loops use lexical scope. An inner loop can read every parent binding in
its iterable, text, dynamic attributes, conditions, component props, and event
arguments. Reusing a binding name shadows only the nearest scope; a following
sibling sees the restored outer value. Components receive evaluated props and
own those values rather than inheriting the caller's loop scope.

```html
<loop :for="account of accounts">
  <loop :for="role of rolesFor(account)">
    <account-role :label="format(account, role)" />
    <button on:click="select(account.id, role.id)">Select</button>
  </loop>
</loop>
```

## Yon REST endpoints

A `server/routes/**/yon.*` file defines a REST endpoint. Its selected static
HTTP method receives the request and returns either a JSON-serializable value
or an explicit response descriptor:

```javascript
@Controller
export class ProductsController {
  static async GET() {
    return { products: await loadProducts() }
  }
}
```

Ordinary values become JSON responses. To return HTML, the handler must provide
the body and explicitly select `text/html`:

```javascript
@Controller
export class ProductsController {
  static GET() {
    return {
      status: 200,
      headers: { 'content-type': 'text/html; charset=utf-8' },
      body: '<main><h1>Products</h1></main>',
    }
  }
}
```

The HTML body is transported unchanged. Yon performs no interpolation,
component expansion, control-tag evaluation, or server-side rendering.
`yon.html` is rejected with `TY1008` so a project cannot accidentally acquire
build-time handler execution.

Routes must attach `@Controller` to a class whose name ends in `Controller`;
the legacy `Handler` name fallback is removed. `@Service`, `@Repository`,
`@Client`, and `@Delegate` are likewise mandatory under their matching server
directories, with `TY2008`, `TY2009`, `TY2011`, `TY2012`, and `TY2015`
enforcing placement, dependency direction, naming, route methods, and presence.

`@Relay` belongs on a delegate-facing method and runs an explicit command
without a shell under bounded stdout, stderr, deadline, and process-tree
cleanup. `.tachyonrc.interpreters`, shebang handlers, and executable-handler
fallbacks are removed. `.tachyonrc.workers` remains, but middleware and workers
must use one of the eight Yon languages.

`@Stream` marks a yielding route method. JavaScript, TypeScript, Python, PHP,
Kotlin, and C# stream multiple bounded frames as server-sent events; Java and
Rust remain single-response languages. Dropped subscribers and deadlines reap
the complete handler process group. Runtime overrides use
`YON_JAVASCRIPT_RUNTIME` and `YON_PYTHON_RUNTIME`.

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

Before preparing output, binding a socket, advertising readiness, registering
routes or middleware, or starting workers, `ty serve` runs bounded capability
probes for every runtime required by the discovered Yon entry points. This also
applies to `--no-bundle`; a static-only project requires no Yon runtime.
JavaScript and TypeScript share one JavaScript-runtime probe, while Java and
Kotlin share one Java-runtime probe. A missing executable is `TY2112`; an
installed but unusable capability remains a bounded `TY2101` readiness failure.
The C# probe resolves the installed SDK and builds a framework-owned minimal
project in isolated temporary CLI/NuGet state, so a runtime-only installation
cannot advertise readiness.
`ty doctor` uses the same probe matrix and reports configured overrides by
environment-variable name, never by executable path.

The runtime supports:

- exact and dynamic filesystem routes;
- standard HTTP methods, bodies, headers, and route parameters;
- before/after middleware through the same supervised protocol;
- scheduled workers declared in `.tachyonrc`;
- bounded append-only topic logs exposed as server-sent events;
- defensive response headers and traversal-resistant static serving;
- semantic CSS and island hot updates with a safe reload fallback for development pages.

Topic subscriptions admit at most 128 clients globally, 32 clients per topic,
and 32 active topics. Each active topic retains 256 replay records; records are
at most 64 KiB and a log is at most 16 MiB. A cursor-less `EventSource` starts
at the oldest retained record. Browser reconnection sends `Last-Event-ID` and
resumes at the following record; an explicitly stale cursor terminates with a
named `topic-error` event. Its JSON payload includes `code`, `message`,
`category`, `guidance`, and `terminal: true`. Clients should parse that event,
call `close()`, and, for `TY_TOPIC_CURSOR_STALE`, recreate the `EventSource`
without `?position=` to recover at the replay floor. Capacity exhaustion is an
HTTP 503: close the attempted subscription and retry with bounded backoff.

Handler and middleware invocation failures return a bounded public message and
an `x-tachyon-request-id` header. The server writes a redacted structured
failure event containing the same time-sortable request ID, so operators can
correlate a client-visible failure without exposing child stderr or internal
diagnostics over HTTP. A `TY2112` event adds only the logical `runtime_family`
and `failure_kind: "not_found"`; it does not include the executable path,
operating-system error, environment values, source, request body, or child
output.

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

Non-web targets host the same compiled Tac HTML/CSS/JavaScript in a platform
web view. There is no render-mode flag or native-control approximation of the
document. Target-native companions provide OS access through a route-scoped
`companion.invoke` bridge, available only to the packaged local application.

| Target | Host | Output |
| --- | --- | --- |
| Web | bootstrap HTML, client render plans, CSS, modules, service worker | `dist/web/` |
| macOS | AppKit/WKWebView | `dist/macos/*.app` |
| iOS | UIKit/WKWebView | `dist/ios/*.app` |
| Android | Android WebView | `dist/android/*/*.apk` |
| Linux | GTK4/WebKitGTK | `dist/linux/` |
| Windows | Win32/WebView2 | `dist/windows/` |

Application authors still write semantic HTML and accessible custom elements:

```html
<design-button role="button" aria-label="Save">Save</design-button>
<design-app-bar role="banner" aria-label="Primary navigation">...</design-app-bar>
```

HTML and custom elements render through the platform browser engine. Roles
remain accessibility semantics, not adapter selectors. The packaged bundle
owns navigation; remote pages must never receive the application bridge.

Native application identity and the entry route can be configured in
`tac.config.js` (legacy `tachyon.json` remains accepted):

```javascript
export const application = {
  name: 'Catalog',
  id: 'com.example.catalog',
  version: '1.0.0',
  entryRoute: '/',
}
```

Platform output is only as supported as its published evidence. A successful
cross-build is `buildable`, not `native-tested` or `supported`; see
[docs/SUPPORT_TIERS.md](docs/SUPPORT_TIERS.md).

## CLI reference

| Command | Purpose |
| --- | --- |
| `ty init [directory] --name <name>` | Create a minimal HTML project in a missing or empty directory |
| `ty serve [project]` | Build, serve, dispatch handlers, watch, and hot-update |
| `ty bundle [project] --target <target>` | Build web or native artifacts |
| `ty native-bundle [project] --target <target>` | Build a selected native host |
| `ty preview [project] --target <target>` | Serve an existing target's embedded web bundle |
| `ty cache [status\|clean]` | Inspect or remove cache left by an earlier installation |

`ty cache` manages the installation/runtime cache selected by `TAC_CACHE_DIR`.
It does **not** inspect or clear a project's compiled handler cache at
`.tachyon/handlers`; use the safe handler-cache recovery procedure in
[the engineering standards](docs/ENGINEERING_STANDARDS.md#handler-cache-operations).

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
`dist/linux`, and `dist/windows`. `TAC_DIST_PATH` selects another output root.
Existing automation may also use `TAC_BUNDLE_TARGET`, `TAC_PREVIEW_TARGET`,
`TAC_TARGET`, `YON_HOST`, `YON_HOSTNAME`, `YON_PORT`, and `YON_SKIP_BUNDLE`.
`TAC_RENDER_MODE` and `--render-mode` are rejected: native hosts run the same
Tac document in their platform web view and do not select a rendering mode.

The public command names from the latest standalone binary remain accepted.
Internal qualification commands such as `doctor`, `migrate`, and
`handler invoke` are intentionally omitted from normal help until cutover.

### Long-running command readiness and shutdown

For `serve`, `preview`, and `bundle --watch`, standard error is a stream of
compact JSON lifecycle events. Parse it one line at a time. The
`runtime.signal_handlers_ready` event proves only which operating-system signal
handlers were installed; despite its name, it is **not** application-readiness
evidence. Wait for the command's readiness line on standard output and, for a
deployed service, a successful health probe before sending traffic.

The first supported signal emits `runtime.shutdown_requested` and requests
graceful shutdown. The development server bounds its owned internal tasks, but
preview may await cooperative connection drain and `bundle --watch` may finish
its synchronous fingerprint pass or current bounded build. Supervisors should
enforce their own grace period. A second signal during graceful shutdown emits
`runtime.shutdown_forced` and exits immediately: `130` for Unix `SIGINT` or
Windows CTRL-C, `143` for Unix `SIGTERM`, and `131` for Windows CTRL-BREAK.
Unix `SIGKILL`, Windows `TerminateProcess`, out-of-memory termination, and host
shutdown cannot be observed through this interface. The complete structured
event and privacy contract is [CLI Signal Lifecycle](docs/SIGNAL_LIFECYCLE.md).

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
`TAC_JAVASCRIPT_RUNTIME` to select an explicit executable.

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

Tachyon defaults to loopback networking, escaped interpolation, no shell
dispatch for framework-owned tool invocations, strict local asset schemes,
and bounded external input. The native bridge is reserved for the packaged
local document, never remote content or subframes. Native companion code runs
with host-process OS privileges; it is not a sandbox or a fixed allowlist of
device APIs. Threats and residual risks are
documented in [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md).

In the default process mode, handler children still run with the developer
account's ambient filesystem and network access. The process supervisor
constrains the protocol and lifecycle, not the operating-system sandbox.
Deploy handlers inside the isolation boundary your environment requires.

### Environment-selected Yon isolation

Production operators can route JavaScript and Python Yon execution through a
qualified Firecracker control program without giving the application a
project-file override:

```sh
export YON_ISOLATION=firecracker
export YON_FIRECRACKER_DRIVER=/usr/local/libexec/ty-firecracker
export YON_FIRECRACKER_POOL=production
export YON_FIRECRACKER_VCPUS=1
export YON_FIRECRACKER_MEMORY_MIB=256
export YON_FIRECRACKER_EGRESS=deny
ty serve
```

`process` is the default. Firecracker mode uses the same framed Handler
Protocol, deadlines, cancellation, bounded diagnostics, and response
validation. It currently rejects TypeScript and the prepared Java, PHP,
Kotlin, C#, and Rust paths with `TY2010` before starting the driver because the
driver contract has no artifact-transfer boundary for them. The configured
control program and host must be qualified separately; setting the environment
variable alone does not prove hardware isolation. See
[ADR 0014](docs/adr/0014-environment-selected-yon-isolation.md).

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

A rollback across the mandatory-layer migration is one coordinated restore:
restore the matching `ty` binary, Yon handler sources, and `.tachyonrc`
together. Restoring only one of them can revive removed interpreter,
shebang, or `Handler` fallback assumptions and is not a supported rollback.

The Rust implementation is greenfield. The former JavaScript framework and its
test tree have been removed. Compatibility gates compare Rust with the
immutable v26.30.04 release binary over neutral fixtures in `corpus/`; no Rust
crate imports or copies private legacy internals.

## License

Tachyon is licensed under the terms in [LICENSE](LICENSE).
