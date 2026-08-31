# Feature Parity Ledger

This ledger states, feature by feature, where the Rust implementation stands
against the released JavaScript implementation preserved as the immutable
v26.30.04 binary outside this branch.

It is maintained by hand and checked by machine. `ty migrate check` classifies
a real project against the same vocabulary, and
`scripts/compat/differential.mjs` proves the "identical" claims by rendering
both implementations in a real browser and comparing what a user or an
assistive technology observes. Set `TAC_LEGACY_BIN` to the released legacy
binary; `TAC_BIN` selects the Rust binary under test. The legacy source tree is
deliberately absent from this repository.

## How This Ledger Is Verified

Compiler/discovery classifications are checked with the real `ty` binary by
`scripts/compat/verify-ledger.sh`. Browser behavior is proved separately by the
named browser gates, and native execution by the target-specific gates. A
successful compile alone does not verify runtime behavior. Run it with:

    ./scripts/compat/verify-ledger.sh

An earlier revision claimed component `tac.css` worked because page `tac.css`
worked. The component case actually failed until scoped component styles were
implemented and added to the executable verification harness. The row below
describes the current, executed behavior.

## Vocabulary

| Status | Meaning |
| --- | --- |
| `identical` | Both implementations accept the same authored source and produce the same observable result. Proven by the differential corpus. |
| `equivalent` | Both implementations support the feature and the observable result differs only in a way recorded here as intentional. |
| `changed` | Supported, but authored source must change. `ty migrate check` reports the required action. |
| `unsupported` | No Rust equivalent. The legacy implementation remains the only way to run it. |
| `rust-only` | A convention the Rust implementation introduces that the legacy implementation has no counterpart for. |

## View Layer

| Feature | Status | Notes |
| --- | --- | --- |
| `client/pages/**/tac.html` discovery and routing | `identical` | Corpus: `static-pages`, `semantic-html`. |
| **Dynamic route segments** (`_id`, `_version`) | `identical` | **Verified: implemented.** Discovery accepts `_name`, the route manifest carries ordered `parameters`, and nested segments compose (`/items/_id/reviews/_review`). The compiler publishes one client render-plan template for the route pattern; the browser owns its DOM. Invalid or duplicated names fail with `TY1006`. |
| Semantic HTML: headings, `nav`, lists, links, images, rules, `details`/`summary`, tables, `footer` | `identical` | Corpus: `semantic-html`, two routes, byte-identical semantic DOM. |
| Standard HTML void elements (`<img>`, `<hr>`, `<br>`, `<input>`) | `rust-only` | The legacy parser rejects them with "No end tag." and requires the self-closing form. The Rust parser accepts both, so it is a strict superset. Corpus fixtures use the self-closing form so both can be compared. |
| Text interpolation `{name}` | `equivalent` | The compiler serializes the expression AST and the Tac browser renderer escapes the resolved value contextually. |
| Dynamic attributes `:name="expr"` | `equivalent` | Same syntax, same escaping. |
| Conditionals `<logic :if>`, `<logic :else-if>`, `<logic else>`, `<if :when>`, `<else>` | `equivalent` | Serialized into the bounded Tac render plan and resolved reactively in the browser. No branch is selected during compilation. |
| Iteration `<loop :for>`, `<for :each>` | `equivalent` | Same syntax and bounds; expansion occurs in the Tac browser renderer. |
| Tac components and `<slot>` | `equivalent` | Component templates and slots are resolved into the client render plan. The browser creates the resulting subtree and preserves accessible names and roles. |
| Conditional and arithmetic template expressions | `equivalent` | **Verified: implemented.** `condition ? a : b` and `+ - * /` are supported, so a template can pick a class inline (`slug === active ? 'active' : ''`) and build a path (`'/docs/' + topic.slug`), which is what the project's own website does throughout. `+` concatenates when either side is a string and adds otherwise; `*` and `/` bind tighter than `+` and `-`; division by zero is a missing value rather than an infinity, and a `null` alternative drops the attribute instead of printing `null`. The expression language stays bounded: no calls, no assignment, no side effects. |
| Companion fields inside a component | `equivalent` | **Verified in a real browser.** A component may read its companion's fields and call its methods: `{loadingState()}`, `{label}`, `{count + 1}`, `{report.rows.length}`, and `{await note()}` all resolve against the instance. Nothing is evaluated as source: the compiler emits the bounded AST it already parsed and the runtime interprets that fixed shape. See ADR 0015. |
| Released page class and literal `<script>` state | `equivalent` | **Verified in real Chromium against both web and staged native documents by `scripts/compat/standalone-rust.mjs`.** A plain page script may declare bounded literal state (`let count = 0`); a colocated default class supplies literal fields, methods, and `@onMount` lifecycle metadata. The compiler removes the script from source output and serializes literal initial state into the client render plan. The browser observes mount state, clicks `count += 1` and observes `Count: 1`. This is not a native WebView transport claim; separate platform UI gates cover that boundary. Arbitrary inline expressions, attributes on the script tag, and executable initializers fail with `TY1306`. |
| Component fields with `mount="never"` | `unsupported` | A never-mounted component has no companion instance. Move instance-dependent expressions into a mounted component or pass literal props. |
| Method calls without a companion instance (`{loadingState()}`) | `unsupported` | A call requires a page or component companion owner in the browser. |
| `<switch>` / `<case :when>` / `<case default>` | `equivalent` | **Verified: implemented.** A switch is sugar for a conditional chain — `<case :when="a">` means `value == a`, first match wins, `<case default>` is the else — so it is desugared during parsing. The renderer, the native planner, and the view IR never learn a third control shape. The comparison node is built directly rather than by concatenating source, so a switch value like `a || b` cannot change meaning through operator precedence. A case after `default`, a non-`<case>` child, and an empty switch each fail with a diagnostic naming the rule. |
| Component `tac.ts` | `equivalent` | **Verified: implemented.** A TypeScript component companion is emitted through the TypeScript compiler itself, the same route a page companion takes, so its semantics are the reference semantics rather than a reimplementation. TypeScript 6 or newer is required, and a lower version fails with `TY1009` naming the version found. |
| Component `tac.css` | `equivalent` | **Verified: implemented, after failing silently for months.** A page-level `tac.css` worked while a component `tac.css` was rejected with `TY1401`, because `verify-ledger.sh` only ever probed the page case. Both are probed now. Styles are scoped with CSS `@scope`, the platform's own answer to component scoping: the compiler puts one `data-tac-scope` attribute on the component's root elements and emits `@scope ([data-tac-scope="<name>"]) { ... }` into `.tachyon/components.css`. Nothing parses or rewrites a selector, and no attribute is added per element the way `data-v-*` scoping requires. A browser without `@scope` ignores the block, leaving the component unstyled rather than leaking its rules. The stylesheet and its link are emitted only when a component has one. |
| Component path convention | `identical` | A component's directory names its tag at any depth: `clicker/` names `<clicker>`, `date-picker/` names `<date-picker>`, and `product/card/` names `<product-card>`. A tag without a hyphen is not a custom element name, but a component is compiled away rather than registered, so the collision that matters is with an element HTML has today — and a directory named for one is refused rather than silently changing what a template means. |
| Component mount schedules | `equivalent` | The browser creates component DOM and activates companions on load, idle, visibility, interaction, or never. The legacy `hydrate` attribute is accepted only as a schedule alias; no server hydration occurs. |
| `on:<event>` event bindings | `equivalent` | **Verified: implemented.** `on:click="increment()"` compiles into the bounded Tac client render plan, whose browser renderer attaches the listener and calls the owning companion or named route export. Proven in real Chromium by `scripts/events-browser-test.mjs`: handlers with no arguments, literal arguments, and the dispatched event all fire. Binding values are parsed data and are **never** evaluated as JavaScript source. A page binding named handlers without a client module fails with `TY1306`. |
| `tac.js` client module | `equivalent` | **Verified: implemented.** Emitted beside the route as `client.js` and referenced with `<script type="module">`. |
| `tac.ts` companion | `equivalent` | **Verified: implemented.** Emitted by the TypeScript compiler itself, so semantics are the reference semantics. Requires TypeScript 6 or newer at build time, which is the version that first accepts the emit flags — verified against 5.6, 5.9, 6.0.3 and 7.0.2. An older or absent compiler fails closed with `TY1009`. See ADR 0007. |
| Target-native page companions | `changed` | Swift targets Apple hosts, Kotlin Android, C# Windows, and Rust desktop hosts. They compile into the host and dispatch by canonical route. A native-only page fails a web build with `TY1010`; add JS/TS for web behavior. Python/Dart are not Tac companion targets. See `CLIENT_RUNTIME_MIGRATION.md`. |
| General inline `<script>` in a view | `unsupported` | Executable inline code, imports, and attributed scripts fail with `TY1306`. The one compatibility exception is the literal Tac page-state declaration described above. Behavior belongs in a client module. Yon has no view source. |
| Native `onclick=` attributes | `unsupported` | Verified: fails with `TY1306`, directing authors to `on:<event>`. This matches the legacy compiler, which also refuses to leave raw `on*` handlers in the DOM. |
| `$event` in an `on:<event>` binding | `equivalent` | **Verified in a real browser.** A binding may pass `$event` or a bounded dotted path off it, as in `on:input="setField('email', $event.target.value)"`. `$event` is the name Vue, Angular, and Alpine all converged on, and the one the project's own website already uses. The binding is parsed into a structure and emitted as JSON, never evaluated as JavaScript, so the no-script property holds: paths are plain identifiers, at most four segments deep. Proven by `scripts/events-browser-test.mjs` driving a real input. |
| Render-scope arguments in an `on:<event>` binding | `equivalent` | Event arguments resolve from the binding's lexical render scope when the event fires, including nested-loop locals and bounded `$event` paths. No expression source is evaluated and ambient browser globals are not imported into expression scope. |
| Assignment in an `on:<event>` binding | `equivalent` | Page and component bindings update their owning instance and rerender the client-owned document. Native field assignments cross `companion.invoke` before rerender; failed host writes are reported, not silently treated as successful. Browser regression: `scripts/runtime-browser-test.mjs`. |
| `tac.css` / `yon.css` colocated styles | `identical` | **Verified: implemented.** Emitted beside the owning route as `style.css` and linked from the document. Reused routes keep them, and changing one invalidates its route. |

## Server Layer

| Feature | Status | Notes |
| --- | --- | --- |
| Request correlation identifiers | `identical` | **Verified:** dispatched requests carry a TTID, natively computed rather than spawned per request. Rust-generated ids match the immutable v26.30.04 wire behavior and decode to their true creation time. |
| Route parameters delivered to handlers | `identical` | **Verified:** Handler Protocol v1 carries an optional `parameters` map, absent for a static route. |
| `server/routes/**/yon.js` handlers | `equivalent` | Rust supervises one bounded process per request under Handler Protocol v1. `GET()` may take zero parameters. Operators may instead select a Firecracker control driver exclusively through parent environment variables; application source and requests cannot weaken that deployment policy. The driver transport currently accepts JavaScript and Python only; TypeScript and prepared direct-language paths fail closed before driver spawn. No hardware-isolation support claim exists without separate driver and host evidence. |
| `server/routes/**/yon.py` handler signature | `changed` | **Verified:** Python's `GET` is invoked as `GET(request)` and must declare the parameter; the JavaScript adapter tolerates zero. |
| `server/routes/**/yon.py` handlers | `equivalent` | Same protocol and bounds. |
| Yon language set and mandatory layers | `changed` | **Verified:** routes and layer sources use JavaScript, TypeScript, Python, Java, C#, Kotlin, PHP, or Rust and must attach the stereotype matching `server/routes`, `services`, `repositories`, `clients`, or `delegates`. The legacy `Handler` fallback is removed; `TY2015` reports a missing declaration and the existing `TY2008`–`TY2012` diagnostics enforce placement, direction, naming, and controller methods. |
| Programs in any other language | `changed` | They are no longer routes, middleware, or workers. Keep the program behind an explicit `@Delegate` method carrying `@Relay`. Relay commands bypass a shell, drain bounded stdout/stderr concurrently, inherit the request deadline, redact process details, and are reaped with the owning handler process group. |
| `.tachyonrc` interpreter registration | `unsupported` | The `interpreters` object fails closed with `TY1502` and `@Relay` migration guidance. Shebang and executable-handler fallbacks are also removed. The `workers` object remains supported. |
| Controllers importing lower Yon layers | `changed` | **Verified:** a controller may import a declared `@Service`, and the build validates the owned module graph without executing it. References must follow the ADR 0017 direction rules: controllers may reach services and the three boundary layers, services may reach repositories, clients, and delegates, and a lower layer may never reach upward or sideways. Every imported server-layer source must carry the stereotype matching its directory. |
| `server/routes/**/yon.html` views | `unsupported` | Yon is REST-only. Discovery rejects `yon.html` with `TY1008` and directs the developer to return an explicit `text/html` response from a handler. |
| Route context from static handler fields and `GET()` | `unsupported` | Removed by ADR 0016. A handler method executes only for a request or explicit invocation, and its return value is the HTTP response. |
| Root middleware | `changed` | **Verified for before/after phases.** Middleware uses one of the eight Yon languages. Returning 204 continues; another status answers early, and the after phase may merge or replace a response. Arbitrary `.tachyonrc` middleware interpreters are removed. |
| `server/workers/**` background workers | `changed` | **Verified:** `.tachyonrc.workers` still declares bounded 1–86,400 second intervals and scheduling reuses handler supervision. Worker sources now use one of the eight Yon languages; registration no longer admits arbitrary interpreters. |
| `@Stream` handler methods | `rust-only` | JavaScript, TypeScript, Python, PHP, Kotlin, and C# may mark a yielding HTTP method with `@Stream`; each yield becomes a bounded SSE event. Java and Rust are single-response. Missing/mismatched annotations fail with `TY2013`/`TY2014`, and timeout or subscriber disconnect terminates the entire process group. |
| HTTP handler dispatch | `equivalent` | **Verified: implemented.** The server matches a request path against the route graph, binds dynamic parameters, and invokes the supervised handler per request. Proven over a real socket in `crates/tachyon-core/tests/dispatch.rs`: parameter binding, percent-decoding, request bodies, multiple methods, explicit byte-preserving `text/html` responses, 405 for an unimplemented method, 404 for unknown paths, and traversal resistance. Production concerns (pooling and keep-alive tuning) remain open. |
| Realtime and topic logs | `equivalent` | **Verified: implemented, following the released protocol rather than replacing it.** The v26.30.04 design is an append-only NDJSON log per topic read by an integer-position cursor over server-sent events, so that contract is kept: a topic is `.tachyon/topics/<topic>.jsonl` and `GET /.tachyon/topics/<topic>` streams it as `text/event-stream`, each frame carrying its position as the event id. Publishing is appending a line, so a handler in any language publishes without a client library. A new cursor-less `EventSource` starts at the oldest retained replay record; browser reconnection sends `Last-Event-ID` and resumes at the following record. An explicit `?position=` is accepted for non-browser clients, and an evicted explicit cursor closes with a named JSON `topic-error` so the client can recreate the source without that cursor. Admission is 128 globally, 32 per topic and 32 active topics; replay is 256 records, each record is at most 64 KiB, and the log is at most 16 MiB. Capacity exhaustion is HTTP 503 and requires close plus bounded retry. No WebSocket upgrade is implemented, because the released realtime surface does not use one. |
| Telemetry and OpenTelemetry spans | `out-of-scope` | **Excluded by an explicit product decision.** The archived JavaScript release emitted telemetry spans; the Rust implementation does not. A project depending on emitted spans must instrument outside Tachyon or remain on the archived release. Structured request logging is a separate concern and is not covered by this row. `ty migrate check` flags the archived built-in telemetry project surface so migrations do not silently lose it. |
| OpenAPI generation and `/api-docs` | `out-of-scope` | **Excluded by an explicit product decision, not because it is unused.** The archived release served OpenAPI 3.1 at `/openapi.json` and a client at `/api-docs`. A project relying on those endpoints must generate a specification another way or remain on the archived release. Revisit if that trade is unwanted. |
| Every legacy command name (`init`, `serve`, `bundle`, `native-bundle`, `preview`, `cache`) | `changed` | **Verified by the released standalone workflow.** Existing command names, target aliases, target/environment selection, host and port environment variables, `--no-bundle`, `--skip-package`, and one-or-many bundle targets are accepted. The packaging override is now `TAC_DIST_PATH`; archived v26.30.04 qualification still uses its legacy `YON_DIST_PATH` spelling. Removed whole-application render-mode inputs fail with an actionable native-web-view migration diagnostic. |
| `ty init` scaffold | `changed` | **Verified against 26.30.04 with declared migrations.** The 18 parity-covered files remain byte-identical after the differential removes the legacy database scaffold and JavaScript-server-only environment defaults, normalizes the standalone-only installer sentence, converts the archived route's exact `Handler` declaration to ADR 0017's `@Controller RootController`, and excludes the replacement standalone ambient type contract from byte parity. Rust scaffold tests cover that generated ambient contract. New projects contain only the four environment settings read by the Rust CLI and no legacy database layout. |
| `ty cache status` / `ty cache clean` | `intentional` | **Verified by the standalone workflow.** The Rust binary preserves the cache directory, `runtime/native-v1` entry, status wording, and clean lifecycle while standardizing the override as `TAC_CACHE_DIR`; its native runtime does not need JavaScript extraction. |
| Offline static cache | `changed` | **Verified in a real browser.** `/tachyon-sw.js` caches only fingerprint-verified packaged assets or explicitly configured anonymous public API reads. Packaged documents are network-first; other packaged files are cache-first. Credentialed/Authorization/no-store requests bypass and evict matching entries, and private/no-store/Vary responses are excluded. Static bodies are bounded to 4 MiB, API bodies to 256 KiB, the cache to 256 entries/32 MiB. Loopback and native hosts skip registration. `scripts/service-worker-browser-test.mjs` proves privacy exclusions, fingerprint checks on writes and hits, native exclusion, and offline document/style rendering. |
| Legacy component re-render registry | `changed` | **Superseded by the bounded Tac client renderer.** Components render from compiler-owned plans in the browser, and semantic hot updates replace named component boundaries through that renderer with bounded state transfer. There is no general application-owned render-closure registry. |
| `ty migrate check` accuracy | `equivalent` | **Verified: it runs the compiler instead of guessing.** The old file-name classifier once reported 142 supported, 1 changed, and 7 unsupported while the website build failed with 97 diagnostics. Views now use the real parser and its diagnostics become findings verbatim. After the website migration, the command reports 128 supported, 1 changed, and 1 unsupported, and the same sources build successfully. File- and project-level rules still cover what no view reveals. |

## Developer Loop

| Feature | Status | Notes |
| --- | --- | --- |
| Source watching and rebuild on change | `rust-only` | **Verified: implemented.** `ty dev` uses operating-system events, a bounded queue, and a 75 ms quiet period instead of repeatedly walking the source tree. Failed rebuilds publish Diagnostics v1 and keep the prior output and DOM running. `--no-watch` disables the watcher and injected client. |
| Semantic hot updates | `rust-only` | **Verified in a real browser.** Hot Update Protocol v1 streams over SSE. CSS changes preserve state; Tac companion changes abort and dispose the old browser-owned instance, load digest-addressed code, and restore declared component state plus bounded form, disclosure, scroll, and focus state; invalid source displays diagnostics over the last-good page; structural and mixed changes reload safely. `scripts/hot-update-browser-test.mjs` is the repeatable browser gate. See ADR 0015. |

| Single-page navigation | `changed` | **Deliberate divergence.** Tac renders each route in the browser, but navigation remains browser-native and cross-document. View transitions and same-origin speculation prefetching provide smooth navigation without a client router. In-memory state does not survive a navigation; use storage, a shared worker, or the server. See ADR 0015. |

## Build and Output

| Feature | Status | Notes |
| --- | --- | --- |
| Route graph and canonical ordering | `identical` | Compared per corpus project. |
| Deterministic, byte-identical repeated builds | `rust-only` | Guaranteed and tested in Rust. |
| Generated output shape | `equivalent` | **Intentional divergence:** the legacy implementation emits a single-page shell with a client router, service worker, and per-page chunks. The Rust implementation emits per-route bootstrap HTML plus a bounded Tac client render plan. The browser-rendered result is compared, not the artifacts. |
| Route Manifest v1 | `rust-only` | Versioned public contract. |
| View IR v1 and View Source Map v1 | `rust-only` | Versioned public contracts. |
| Incremental digest-verified reuse | `rust-only` | |
| Reused route event retention | `rust-only` | **Verified: implemented.** Route build state persists event descriptors and restores them when a route is reused, so an incremental build cannot remove `.tachyon/events.js` merely because no route was recompiled. A compiler test builds the event fixture twice, and the migrated website retains a byte-identical runtime with `compiled=0 reused=10`. |
| `client/shared/**` publication | `equivalent` | **Verified: implemented.** Files publish deterministically beneath `/shared/`, matching the website's authored URLs. Publication is bounded to 4,096 files, 16 MiB per file, and 64 MiB total, and refuses symlinks and non-files rather than following them outside the project. |
| Defensive browser execution policy | `equivalent` | Authored expressions remain bounded AST data, never `eval` or `Function`. The runtime/decorator/storage browser gate runs under strict CSP without `unsafe-eval` or `wasm-unsafe-eval`. Target-native companions do not instantiate WebAssembly in the browser. |
| Failed builds preserve the last good output | `rust-only` | Guaranteed and tested in Rust. |
| Diagnostics v1 with stable codes | `rust-only` | The legacy implementation raises untyped runtime errors. |

## Native

| Feature | Status | Notes |
| --- | --- | --- |
| macOS, iOS, Linux, Windows, Android applications | `changed` | Each hosts the compiled Tac document in WKWebView, Android WebView, WebKitGTK, or WebView2. The previous Native UI control adapter/per-subtree fallback architecture is retired; old phase evidence is historical, not qualification of this host revision. See `SUPPORT_TIERS.md`. |
| Target-native companions | `changed` | Page-local `tac.swift` (Apple), `tac.kt` (Android), `tac.cs` (Windows), and `tac.rs` (desktop) compile with target toolchains. The most-specific available language is selected; JS/TS remains the web implementation. |
| Per-route companion state and invocation | `rust-only` | `scripts/native/companion-test.mjs` builds two routes with the actual selected binary, executes its generated native ABI, and verifies isolated state, reads/writes, methods, native OS access, unknown-route rejection, and host publish callbacks. Platform UI gates separately verify the web-view transport. |
| Native asynchronous browser bridge | `changed` | The compiled plan identifies the canonical route. Initialization reads declared fields but never executes methods. Calls occur only on explicit invocation; setters settle before rerender and method calls refresh field state. Requests/responses are bounded and fail with sanitized errors. |
| Former fixed JS host verbs | `changed` | `host.on`, `shortcuts.register`, and `contentSurface.open/state` belong to the retired widget host. Use platform SDKs from a target-native companion, native publication, and `this.tac.subscribe`; these verbs are not silently emulated. `tachyonWindow` is limited to declared window controls. |
| Native-to-Tac signals | `rust-only` | The local host queues early publications; the client drains at most 128 records into the document-local retained bus. This is not a network/SSE topic subscription. |
| Former browser Wasm companions, including Dart | `changed` | The old Wasm ABI and loaders are retired. Move browser behavior to JS/TS and OS-specific behavior to target-native page companions. Dart/Python are not supported Tac companion languages; they can remain explicit Yon relay programs where appropriate. Historical ADR 0011 is not the current API. |
| `ty doctor` | `rust-only` | Reports the selected native and TypeScript toolchain requirements. It does not register arbitrary interpreters or require the retired browser-Wasm toolchains. |
| Capability Manifest v1 and Artifact Manifest v1 | `rust-only` | Versioned contracts record target/bridge revision, input provenance, generated outputs, and digests. Source discovery and publication retain the captured-input and atomic-output boundaries. |
| Remote content | `changed` | The privileged bridge belongs only to the packaged local application, never a remote document or subframe. Host navigation and frame checks enforce that boundary. |

## Restored Client Runtime

| Feature | Status | Notes |
| --- | --- | --- |
| `this.tac` runtime API | `rust-only` | Non-enumerable binding provides `fetch`, `invalidate`, `precache`, `clearAssetCache`, `publish`, `subscribe`, `retained`, and `render`. Hot state remains cloneable. |
| `$` / `$$` field persistence | `equivalent` | Explicit prefixed fields use per-tab session storage or origin-local persistent storage; denied/quota/malformed storage degrades to in-memory defaults. `scripts/storage-browser-test.mjs` proves reload/tab semantics and failure handling. |
| Response caching and invalidation | `changed` | Persistent caching requires same-origin GET/HEAD, `credentials: 'omit'`, no Authorization, and a response without private/no-store/Vary directives. Default policy is network-first, not default permission to persist credentials. Cache records are bounded and expire after 24 hours; the privacy migration discards unpartitioned version-1 records. |
| `@publish` / `@subscribe` / `@onMount` | `rust-only` | Build-time lowering runs before TS emission for page and component default classes; quoted signal names and instance-member scope are validated. Field persistence composes with publication. Methods publish returned/resolved values, never rejected results. |
| Retained signals and subscription ownership | `rust-only` | Document-local, bounded topics/listeners/retained values. Abort, component removal, and HMR dispose subscriptions. Synchronous/async subscriber errors are isolated without logging payloads. Native publish uses the same bus. |
| Counted loops | `rust-only` | `<loop :for="let i = 0; i < 3; i++">` supports ascending/descending bounds and positive step magnitudes. A loop is bounded to 10,000 iterations; aggregate iteration/node budgets also bound nested empty loops. Non-progressing steps fail safely. Legacy iterable/control spellings remain accepted. `scripts/runtime-browser-test.mjs` exercises dynamic bounds, invalid steps, scope, caps, and recovery. |
| Ordinary JS/TS module helpers | `equivalent` | Top-level constants, helper functions, and imports remain legal. The unpublished blanket class-only restriction was not adopted. Native reflection has its own language-specific boundary. |

## What the Ledger Does Not Claim

- No row claims that a Rust feature is production-ready. Readiness is governed
  by `SUPPORT_TIERS.md`.
- `unsupported` never means "will not be supported". It means there is no Rust
  equivalent today and the legacy implementation is still required.
- The differential corpus covers the intersection both implementations can
  build. Features marked `rust-only` have no legacy counterpart and are proven
  by the Rust phase suites instead.
