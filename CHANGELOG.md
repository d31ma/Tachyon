# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [26.30.03] - 2026-07-22

### Changed

- Managed remote content on macOS, Windows, and Linux is now composed into the
  primary native window's declared 25/75 split instead of opening in a detached
  top-level window. Primary `appWindow` controls therefore apply to the trusted
  controls and active remote surface as one presentation.
- Managed-content state and host manifests now report the composed presentation
  contract, while bundles without managed origins omit unsupported layout
  metadata.

### Fixed

- Closing or replacing the active managed surface now removes it from the
  composed pane and promotes another open surface without exposing the native
  bridge to remote content.
- Documentation now explains composed versus detached presentation behavior,
  platform limitations, and the global-shortcut recovery path for click-through
  windows.

## [26.30.02] - 2026-07-21

### Added

- Native-first macOS, Windows, and Linux hosts now implement desktop window
  controls, isolated managed-content surfaces, declared camera and microphone
  permissions, and truthful capability state reporting. macOS and Windows also
  implement global shortcuts and permissioned window enumeration and capture.
- Release CI now compiles generated native hosts on macOS, Linux, and Windows,
  including platform-specific managed-content and screen-capture adapters.

### Fixed

- Compiled standalone Tachyon binaries now embed the DOM-free native UI runtime,
  so a freshly initialized project can bundle native targets without resolving
  source-only runtime paths.
- Native host manifests now advertise only generated operations and preserve
  declared permissions and exact-origin managed-content policies. Remote content
  stays isolated from the application-wide native bridge.

## [26.29.03] - 2026-07-15

### Added

- Raw native filesystem capabilities now include opt-in `fs.stat`, `fs.mkdir`,
  and recursive `fs.remove` operations across Android, iOS, macOS, Windows, and
  Linux, with matching Dart, Rust, Kotlin, Swift, C#, JavaScript, and TypeScript
  companion APIs.
- Android native hosts now provide encrypted secret storage through the Android
  Keystore and local user verification through the framework `BiometricPrompt`,
  bringing `secrets.*` and `auth.verifyUser` to Android alongside Apple hosts.
- `ty bundle --csp-check` (or `TAC_CSP_CHECK=1`) audits staged JavaScript output
  for runtime code-generation constructs blocked by a strict CSP and fails
  atomically before replacing the previous bundle.

### Fixed

- Companion decorator and FYLO auto-import detection no longer mistakes an
  unrelated import followed by a later `from` expression for a real binding.
- Author-supplied element IDs no longer break Tac event or value-binding
  dispatch, and delegated handlers receive the bound element as
  `event.currentTarget`.
- A handler that clears state used later in its own conditional render pass no
  longer strands the DOM before the clean state-only render can complete.
- Elements with multiple `on:*` handlers now invoke only the handler matching
  the dispatched event type.

## [26.28.06] - 2026-07-11

### Added

- Tac components and pages can use portable Dart, Rust, Kotlin, Swift, and C#
  companions alongside JavaScript and TypeScript. Every language lowers to the
  same reactive controller ABI and implicit browser/FYLO/device prelude.
- The native capability surface now includes sandbox path discovery, secure
  storage and local user verification on Apple hosts, browser-standard file
  save, geolocation, notifications and media capture, permission-state
  inspection, and the buffered `app.ready` host event.
- Native camera, microphone, location, and notification privacy declarations
  can be configured with `tachyon.devicePermissions` in `package.json`.
- Raw native operations can be enabled individually with
  `tachyon.nativeCapabilities`: `fs.readText`, `fs.writeText`, `fs.readDir`,
  and desktop-only `shell.exec`.
- The website now has a sectioned capability atlas with dedicated overview,
  compose, react, connect, store, observe, and extend routes, plus a live
  portable-bridge panel covering the permission and capability model.

### Changed

- Tac companions are now target-neutral UI controllers instead of client
  worker endpoints. The client worker/WASM compiler and runtime have been
  removed; portable companions use normal bundled modules on web, desktop,
  iOS, and Android.
- Native filesystem and shell access is disabled by default. Package
  declarations are validated strictly, filtered by target support, embedded
  into generated metadata, and enforced independently by the JavaScript
  facade and native handler.
- Native external URL operations accept HTTP(S) only. Android bridge delivery
  is restricted to the trusted appassets origin and main frame, Apple and
  Linux bridge scripts are main-frame-only, and Android media requests grant
  only camera/audio resources explicitly recognized by the host.
- Android native hosts disable application backup and cleartext network
  traffic by default.
- The website's `/atlas` route is now a redirect shell; each indexable Atlas
  section has its own canonical route and responsive one-column mobile layout.

### Fixed

- Dart toolchain provisioning and portable companion diagnostics now produce
  deterministic, actionable failures for unsupported source constructs.
- Native configuration no longer silently discards misspelled permission or
  capability names; invalid declarations fail the build before replacing an
  existing bundle.
- Android no longer grants unrelated WebView permission resources when a page
  requests camera or microphone access.

## [26.28.01] - 2026-07-06

### Fixed

- `tac.bundle` no longer clears the whole `dist/` directory: a run swaps in
  only the `dist/<target>` directories it actually built, atomically per
  target (stage, retire, rename). Building one target used to delete every
  other target's output — wiping `dist/web` out from under a running dev
  server, and hollowing out a generated macOS app running from `dist/macos`
  so its lazily-fetched page modules silently 404'd.
- The SPA runtime now requests dynamic-route page modules by their on-disk
  `_slug` directory (`pages/docs/_topic/tac.js`) instead of the route-manifest
  `:slug` form, so dynamic routes work on static hosts and native asset
  loaders, not just the dev server.
- The service worker registers with `type: "module"` — the built worker is an
  ES module and previously failed to parse on every production origin.
- The generated Android host overrides AAPT's default asset-ignore pattern,
  which silently dropped `_slug` page-module directories from the APK.
- The service worker cache version now folds in a content hash of the client
  source tree alongside the route-manifest hash, so any rebuild that changed
  inputs evicts old caches. Same-version redeploys (and native app
  reinstalls) previously served stale JS/CSS from the Cache API until site
  data was cleared.

### Changed

- The generated macOS host installs a standard menu bar (App menu with
  About/Hide/Quit, Edit menu with clipboard actions, Window menu) — without
  a main menu a WKWebView app has no working Cmd+C/V/Q. `buildMainMenu()` in
  the generated `TachyonApp.swift` is the extension point for app authors'
  custom menus.
- The generated macOS host serves the bundle through a `WKURLSchemeHandler`
  at `tachyon://localhost/` instead of `file://` with the file-access
  preference hacks. The secure-context origin enables Web Workers (Tac
  Workers running wasm) and OPFS-backed FYLO persistence, matching the iOS
  and Android hosts, with the same directory-index and deep-link handling
  and external links opening in the default browser.
- The generated iOS project actually ships its app icon: the xcodegen spec
  now includes the generated `Assets.xcassets` and sets
  `ASSETCATALOG_COMPILER_APPICON_NAME`, so the home-screen icon shows the
  Tachyon mark instead of the blank placeholder.
- External links open in the system default browser on both native hosts
  instead of navigating the WebView away from the app: the Android host
  fires an `ACTION_VIEW` intent for any origin other than its own, and the
  iOS host's `WKNavigationDelegate` hands http/https/mailto URLs outside the
  `tachyon://` scheme to `UIApplication.open`.
- The generated iOS host ships its web bundle as `WebBundle/` instead of
  `Resources/` — a top-level folder named "Resources" makes codesign/installd
  treat the flat iOS bundle as an old-style shallow bundle and installation
  fails with a misleading "Missing bundle ID" error. The `.ipa` archive also
  excludes AppleDouble sidecar files now (`ditto --norsrc`).
- The generated iOS host serves the bundle through a `WKURLSchemeHandler` at
  the `tachyon://localhost/` origin instead of `file://`. The secure-context
  origin enables Web Workers (Tac Workers running wasm) and OPFS-backed FYLO
  persistence inside WKWebView, with directory-index and extension-less
  deep-link handling matching the Android host. Service workers remain
  unavailable in WKWebView and the runtime degrades gracefully. The
  prerendered shell viewport now includes `viewport-fit=cover` so
  `env(safe-area-inset-top)` works on iOS.
- `tac.bundle --target` accepts space-separated lists in addition to
  comma-separated ones (`--target ios, android` and `--target ios android`
  both work; previously everything after the first argument was silently
  ignored).
- The generated Android host draws behind a transparent status bar: the
  status-bar inset is surfaced to pages as the `--tac-safe-top` CSS variable
  (WebView cannot read `env(safe-area-inset-top)`), and a new
  `ui.statusBarStyle` bridge capability lets pages switch the system icons
  between `dark-content` and `light-content` when their theme changes.
- The generated native icon set (`NativeIconAssets`) now renders the current
  Tachyon mark (slate tile, gradient chevrons) across SVG, Android vectors,
  and raster PNGs, so launcher icons match the PWA icons.
- The Android host ships a proper adaptive launcher icon
  (`mipmap-anydpi-v26/ic_launcher` with background, foreground, and
  Android 13+ monochrome layers) instead of a plain drawable, so the icon
  fills the launcher mask edge to edge like first-party apps.
- The generated Android host runs without the native action bar
  (`Theme.Material.Light.NoActionBar`) and disables WebView scrollbars for a
  native app feel.

### Added

- Yon supports any and all languages — there is no "supported languages"
  list, only the developer's machine toolchain. A `yon.<ext>` handler runs
  by its extension: the extension names the language and an `interpreters`
  map turns it into a run command (`.go` → `go run`, `.rb` → `ruby`, …),
  seeded with common defaults and extensible/overridable per project under
  `interpreters` in `.tachyonrc`. No shebang and no `chmod` needed — write
  `server/routes/ping/yon.go`, declare its HTTP methods in an adjacent
  `OPTIONS.schema.json`, and it runs as a process (the handler reads the
  JSON request on stdin and writes the response on stdout). A prebuilt
  executable or a script with its own shebang is also accepted. The built-in
  languages are ergonomic conveniences (write `class Handler`, skip the
  stdin/stdout glue) provided by a pluggable provider registry
  (`src/server/process/language-providers.js`); apps add or override
  providers from `server/yon.providers.js` — e.g. point python at a
  virtualenv interpreter. `HandlerAdapter.supportedLanguages` /
  `isSupportedLanguage` are gone; `HandlerAdapter.knownLanguages` lists the
  convenience adapters for tooling, not as a gate.

### Removed

- Yon no longer has a compile-to-binary deployment step: `yon.build` (the
  `yon.build` bin, `src/cli/build.js`) and the `native-binary` execution
  backend are removed. The developer ships the `server/` source as-is and
  every route runs as a process; compiled-language routes compile on first
  request and cache the result, so production needs the same toolchain the
  developer used rather than pre-built binaries.
- Yon's in-house WASM execution backend is removed
  (`src/server/process/backends/wasm-compiled.js`) along with the now-empty
  backend registry: every Yon route runs as a subprocess, with no in-process
  WASM path. (The Tac frontend worker compiler is separate and unaffected.)
- Tac workers now support only Rust, JavaScript, and TypeScript. The C#, C++,
  Swift, and Kotlin worker compilers/providers are removed; `yon.<ext>`
  backend handlers in those languages are unaffected (that is Yon, not Tac).
- The generated Android host serves the bundle through
  `androidx.webkit` `WebViewAssetLoader` at the trusted
  `https://appassets.androidapp.com/` origin (explicit domain, directory
  indexes, service-worker interception) instead of `file://`. Web Workers
  (Tac Workers running wasm), OPFS-backed FYLO persistence, and service
  workers now all function inside the native app, and the deprecated
  file-URL access grants are gone. The iOS host README documents the
  equivalent `WKURLSchemeHandler` upgrade it still needs.
- Production bundles (or `tac.bundle --package`) export distributable native
  artifacts when the system toolchains exist, and `dist/<target>/` then
  contains only the artifact: `dist/android/<App>-<v>.apk` via Gradle
  (debug-keystore fallback signing, `TAC_ANDROID_KEYSTORE*` for store
  signing), `dist/ios/<App>-<v>.ipa` via xcodegen + xcodebuild (unsigned by
  default, `TAC_IOS_TEAM_ID` to sign), an ad-hoc-signed `dist/macos/<App>.app`
  via swiftc, `dist/linux/<App>/` (executable + Resources) via CMake, and
  `dist/windows/<App>/` (`.exe` + Resources) via CMake/MSVC.
  Missing toolchains downgrade to a logged skip that keeps the buildable host
  project; `--skip-package` opts out.
- Prerendered shells automatically link a web app manifest when
  `client/shared/assets/manifest.webmanifest` (or `manifest.json`) exists,
  surfacing its `theme_color` as a `<meta name="theme-color">` tag. Together
  with the built-in service worker, shipping a manifest beside the favicon is
  all an app needs to become an installable PWA.
- Desktop native Tac workers (starting with Rust on macOS, Linux, and Windows).
  `browser/workers/<route>/tac.rs` files are compiled to native executables during
  `tac.bundle --target <desktop>` and shipped inside the host's `Resources/workers/`
  directory. The generated desktop native hosts expose a `tachyon.worker` bridge
  capability that spawns the executable per request, passing a JSON envelope over
  stdin and returning the response envelope to the frontend.

- Yon backend routes for Rust/C++/C# whose handler fits Tachyon's supported
  subset now compile to WebAssembly **in-house** (no rustc/clang/.NET) and
  execute **in-process** through a pluggable execution backend; handlers beyond
  the subset transparently fall back to the existing subprocess runner. Subset
  handlers can read request fields (`request.query/path/header("k")`) and return
  structured JSON object literals (`{ "k": value, ... }`) — both resolved/built
  in-house with no JSON library inside the wasm module — so wasm routes return
  validated object responses. The consolidated `/language` example route
  exercises this: its Rust (`yon.rs`) and C++ (`yon.cpp`) handlers fit the subset
  and compile to WebAssembly in production.

### Changed

- Tac workers now use five language families with explicit target scopes:
  JavaScript/TypeScript for web, Swift for macOS/iOS, Kotlin for Android, C#
  for Windows, and Rust across every target. C, C++, Go, Python, and Zig are no
  longer accepted under `client/workers`.
- The in-repo website (`website/`) has been revamped from a dense capability atlas
  into a minimalist marketing landing page. It now features a dark hero with an
  install command, a file-system scaffold visualization, a feature grid, tabbed
  polyglot code examples, and a curated interactive demo using the polyglot,
  desktop native, and canvas panels. The old atlas components have been removed.

- Bundled npm dependency modules now live under `/shared/modules/` instead of `/modules/` (e.g., `/shared/modules/dayjs.js`). This is a clean break: the old `/modules/*` URLs are no longer served.

### Added

- The app shell now includes an inline `<script type="importmap">` that maps bare npm specifiers (e.g., `dayjs`) to their bundled `/shared/modules/` URLs. User-defined mappings can be supplied in `browser/shared/importmap.json` (with `shared/importmap.json` as a fallback); user entries override auto-generated entries.

### Changed

- Tac worker `OPTIONS.schema.json` now matches the Yon route schema shape but
  collapses response schemas into three status buckets: `ok` (2xx),
  `clientError` (4xx), and `serverError` (5xx). Per HTTP method the keys are
  `payload` (request) plus whichever of the three response buckets are relevant.
  2xx responses always use the `ok` schema. The old `response`/`error` keys,
  numeric status-code keys, and the `native` capability declaration block have
  been removed from the worker schema. Capabilities are now authorized through
  the `TAC_NATIVE_CAPABILITIES` environment variable (a comma-separated
  allowlist), resolved at compile time and baked into the worker runtime. Raw OS
  capabilities (`fs.*`, `shell.*`, `process.*`) still require an additional entry
  in `TAC_DANGEROUS_CAPABILITIES`.
- Tac frontend messaging now uses a single signals system. `@publish` and
  `@subscribe` replace the previous `@emit`, `@provide`, and `@inject`
  decorators, with retained field signals for context-style values and
  ephemeral method signals for event-style messages.
- Tac method subscribers can now pass `{ onMount: true }` to run once after
  mount and then react to future signal publications.
- Bare `@subscribe` and `@publish` now use the decorated field or method name
  as the default signal name.
- Removed the `@render` decorator from the strict v2 Tac API. Companion state
  now rerenders through field assignment, including `$` and `$$` persistent
  fields.
- Upgraded `@d31ma/fylo` to `^26.23.7`. The new version adds query caching
  (`FYLO_CACHE_BACKEND`, `FYLO_CACHE_METHOD`, `FYLO_CACHE_TTL`), WORM mode
  (`FYLO_WORM=strict`), RLS toggle (`FYLO_RLS`), local queue toggle
  (`FYLO_QUEUE`), and sync-mode selection (`FYLO_SYNC_MODE`) as environment-
  driven options in `fyloOptions()`. The Fylo browser now exposes soft-delete
  inspection and restoration through `/_fylo/api/deleted` (GET) and
  `/_fylo/api/restore` (POST).
- Upgraded Tachyon website's `@d31ma/fylo` dependency from `^26.19.7` to
  `^26.23.7`.
- Native bundle targets now ship their webview host directly at `dist/<target>/`
  instead of a separate `dist/<target>-native/` directory. The standalone web
  bundle is no longer emitted for native targets (its assets are embedded under
  the host's `Resources/`); `--skip-native-host` still emits the plain web
  bundle at `dist/<target>/`.

### Removed

- Dropped Swift (`yon.swift`) and Kotlin (`yon.kt`) backend route handler
  support. These are mobile-first languages; Yon backend routes now target
  server languages only. Removed the compiled-runner Swift/Kotlin adapters, the
  `/languages/swift` and `/languages/kotlin` example routes, and their
  services/repositories.

## [26.23.01] — 2026-06-01

### Added

- Yon realtime SSE mailboxes behind `YON_REALTIME_ENABLED=true`.
  `/_yon/realtime/clients` issues durable TTID client IDs,
  `/_yon/realtime/stream` replays queued messages over Server-Sent Events, and
  `/_yon/realtime/messages` stores messages in FYLO's local queue while waking
  any currently connected client stream.
- The example app now includes a Yon realtime messaging slice: MVC routes under
  `/realtime`, FYLO/CHEX schemas for demo clients and messages, and a Tac
  companion script ready for the browser UI markup/styling pass.
- Tac's compiler-injected `fylo` browser client now caches successful
  collection reads in IndexedDB, supports explicit cache policies, falls back
  to cached reads when offline, and invalidates collection cache entries after
  FYLO mutations.
- The example users panel now demonstrates every FYLO browser cache policy and
  includes a temporary create/delete flow that shows mutation invalidation.
- `bun serve` now treats `browser/` + `db/` apps with
  `YON_DATA_BROWSER_ENABLED=true` as a built-in backend, so app authors can use
  the Tac FYLO browser wrapper without adding a `server/` folder.

### Changed

- Upgraded `@d31ma/fylo` to `26.22.7` and migrated Tachyon's FYLO integrations
  to the root-first constructor API.
- Yon compiled route adapters now cache Java, C#, Dart, C++, Swift, Kotlin, and Rust
  build artifacts when `NODE_ENV=production`, keyed by the route file,
  same-language service files, and adapter cache version.
- Yon now supports C++ backend route handlers through `yon.cpp`, generated
  dependency-free `YonJson` helpers, and the compiled route adapter.
- Yon now supports Swift (`yon.swift`) and Kotlin (`yon.kt`) backend route
  handlers through the compiled route adapter. Swift handlers namespace static
  verbs under an `enum`/`struct`/`class Handler` and exchange JSON through
  `Foundation`; Kotlin handlers expose verbs from a `class Handler` companion
  object (or top-level `object Handler`) and receive a generated dependency-free
  `YonJson` helper. Example routes ship under `/languages/swift` and
  `/languages/kotlin`.
- Yon now supports Rust (`yon.rs`) backend route handlers through `rustc`.
  Rust handlers use `struct Handler; impl Handler { pub fn GET(...) { ... } }`,
  receive a generated dependency-free `YonJson` helper, and ship with a
  `/languages/rust` example route/service.
- The example capability atlas now uses a responsive mobile drawer for its
  navigation rail while preserving the persistent desktop rail.

### Fixed

- The example FYLO users panel now redacts API-key-shaped previews before
  rendering them in the browser.

## [26.22.03] — 2026-05-27

### Added

- PostgREST-style query filtering on Fylo browser collection endpoints.
  `GET /_fylo/<collection>/?field=operator.value` supports `eq`, `neq`,
  `gt`, `gte`, `lt`, `lte`, `like`, `ilike`, `in`, `is`, and `not` prefix.
  Reserved params: `select` (vertical filtering), `order` (sorting),
  `limit`, `offset` (pagination).
- The browser example dashboard now includes a Tac input gallery covering
  native input types, `select`, `datalist`, and `textarea`, with live event
  feedback and reactive persisted value bindings.
- The example browser app is now a guided capability atlas linking Tac
  composition, reactive browser state, Yon routes, FYLO storage, telemetry,
  and WebAssembly companions into one navigable application tour.
- A Browser studio panel demonstrates Tac-driven canvas drawing together with
  native semantic `progress`, `meter`, `output`, `time`, and `details`
  surfaces under scoped responsive CSS.
- Tac templates support `:checked="field"` so checkbox and radio state remains
  reactive across component rerenders.
- The checked-in example app now participates in standalone strict type
  checking with its Tac globals and browser dependencies resolved.

### Changed

- **BREAKING**: Yon route handlers use a class-per-route model.
  `export class Handler` with static methods named after HTTP verbs
  (`static async GET(request)`) replaces the prior method-per-directory
  convention (`routes/<path>/GET/yon.<ext>`). All polyglot adapters (JS,
  TS, Python, Ruby, PHP, Java, C#, Dart, C++) support the new pattern.
- Upgraded `@d31ma/fylo` to `^26.22.3`. Removed `getHistory()` calls;
  added `findDeletedDocs()` and `restoreDoc()` to the machine-interface
  demo.
- The Fylo browser query panel now accepts PostgREST-style filter input
  instead of SQL/findDocs modes. The `<fylo-browser>` Tac component and
  the vanilla JS shell both use the new interface.

### Removed

- Removed the `POST /_fylo/api/query` endpoint (SQL and find queries).
  Index-backed filtering now uses PostgREST-style query params on the
  collection URL. The `fylo.sql()` client method is removed.
- Removed Go and Rust language support from example routes and handler
  adapters.
- Removed version history rendering from the Fylo browser (the upstream
  `getHistory()` API was removed in Fylo 26.22.3).

### Fixed

- `HandlerAdapter.hasMethod` regex now matches async generator handlers
  (`static async *GET()`) inside class-per-route definitions.
- `Router.validateSegmentPath` no longer produces false-positive duplicate
  route errors for nested slug routes (e.g. `/items/_id/detail` vs
  `/items/detail`). Slug segments are mapped to a `:` placeholder instead
  of being stripped, preserving path depth in comparisons.
- Static prerendering now preserves literal replacement-token text such as
  Tac's `$` and `$$` persistence sigils instead of expanding it while
  inserting rendered pages into the HTML shell.
- Browser acceptance tests accept `TACHYON_EXAMPLES_E2E_PORT`, avoiding false
  failures when the default example port is already in use.
- The example inventory form now binds CRUD draft state directly to its
  visible native input, so a successful Yon save reliably clears the field.
- Tac client-side navigation preserves query parameters and URL fragments on
  first hydration, intercepted link clicks, and browser history traversal.
- Component stylesheet regression coverage now ensures template-literal
  characters in author CSS comments remain intact in emitted Tac modules.
- Tac `:value` bindings now render loop-local expressions without evaluating
  their identifier outside the loop scope, preventing `ReferenceError` during
  builds such as `<loop :for="option of options"><input :value="option" />`.
- Synthetic value-binding updates preserve `$event.target`,
  `$event.currentTarget`, and `$event.type`, so DOM-style `@input` and
  `@change` handlers continue to work when combined with `:value`.
- `@onMount` registration is deferred until after renderer companion binding,
  preserving mounted callbacks even when an app constructor calls
  `super(props)` without forwarding the injected Tac helper argument.

## [26.21.7] — 2026-05-24

### Added

- New `POST /languages/javascript/fylo` example route that drives the full
  FYLO machine-interface suite (~25 ops). Moved off the lightweight
  `/languages/javascript` dashboard endpoint so diagnostics stays fast.

### Changed

- **BREAKING**: Tac page and component templates now use `tac.html` (with
  optional sibling `tac.js`/`tac.ts`/`tac.css`/`tac.wasm`/`tac.tac.json` and
  source-backed companions). The previous `index.html` convention for Tac
  source is removed; bundled `dist/**/index.html` output is unchanged so
  static hosting and previews behave as before, while dist page/component
  modules emit as `tac.js`.
- **BREAKING**: Yon route handlers move to `server/routes/<path>/<METHOD>/yon.<ext>`.
  The HTTP method is now the parent directory of the handler file, and every
  handler file is named `yon.<ext>` regardless of language. The companion
  `OPTIONS.schema.json` continues to sit as a sibling of the method
  directories. Polyglot class-based handlers (Java, C#, Python, PHP, Ruby)
  derive their class name from the `yon` basename, producing a Pascal-cased
  `Yon` class.
- Repo restructure: `package-contract/` moved to `tests/package-contract/`,
  `stress/` moved to `tests/stress/`, and integration test runners/workers
  moved to `tests/integration/helpers/`. `bun test`, `test:blackbox`, and
  `stress:tac` scripts updated accordingly. Pure layout change with no
  runtime behavior change.
- Upgraded `@d31ma/fylo` to `26.21.6`. The Tachyon-facing surface
  (`new Fylo(fyloOptions(root))`, `Fylo.uniqueTTID`, `executeSQL`,
  `findDocs`, `inspectCollection`) is unchanged; the bump tracks upstream
  bug fixes and the `LocalQueue` / `publish` / `consume` queue exports
  that example apps may consume directly.

### Internal

- Replaced the in-place `tsc --noEmit` typecheck with
  `scripts/typecheck.js`, a wrapper that stages the selected project into
  the OS temp directory on cloud-synced working copies before running the
  same compiler command. `tsconfig.src.json` remains the default release
  gate; pass `tsconfig.tests.json` or `tsconfig.examples.json` to run those
  scoped projects. Controls: `TACHYON_TYPECHECK_TIMEOUT_MS` (default 2
  minutes), `TACHYON_TYPECHECK_STAGE=1` or `0`, and
  `TACHYON_TYPECHECK_KEEP_STAGE=1`.
- Hardened the FYLO browser against traversal-shaped collection names when
  resolving REST-style collection URLs and event-tail files. Invalid
  event-tail collection names return a 200 JSON error in the existing FYLO
  browser API style.
- Removed remaining FYLO browser DOM `innerHTML` rendering for dynamic
  collection, document, history, REST result, encrypted-field, and event
  values; the UI now writes dynamic values with text nodes.
- Restored `tsconfig.json` to inherit from `tsconfig.base.json` so Bun's
  runtime module resolver picks up the `@/*` → `./examples/server/*`
  path alias used by the example polyglot route handlers.
- `Pool.prewarmAllHandlers` fallback handler path updated to the new
  `<METHOD>/<routeFileName>` layout. The previous `<METHOD>` fallback
  silently produced non-existent paths after the Yon rename and dropped
  most polyglot handlers from the warmed-process pool.
- Restored `static chexSchemas = new Map()` on `Validate`; the schema
  validator's response-matching path requires it, and the working tree
  had it removed mid-refactor.
- `FyloMachineRepository.ensureDemoSchema()` now writes to a
  `fylo-demo-items` collection instead of `items`, so the demo run no
  longer overwrites the real items schema that other example routes
  validate against.

## [26.21.05] — 2026-05-22

### Added

- Stage 3 decorator forms of the `Tac` runtime helpers, available as bare
  identifiers in companion scripts: `@inject(key, fallback?)`,
  `@provide(key)`, `@env(key, fallback?)`, `@onMount`, and `@emit(name)`. The
  Tachyon compiler auto-imports these into companion scripts when it sees the
  `@<name>` syntax, so user code references them without an `import` line.
  Outside of companion scripts (tests, library code), import them from the
  new `@d31ma/tachyon/decorators` package export. The existing instance-method
  API (`this.inject`, `this.provide`, `this.onMount`, …) is unchanged.
- Automatic prop-to-field binding: `Tac` copies values from `this.props` onto
  any same-named instance field the subclass declared, removing the need for
  boilerplate `this.foo = this.props.foo ?? default` assignments in the
  constructor. A leading `$` on the field name is stripped when matching, so
  `$`-prefixed persistent fields auto-bind to their unprefixed prop
  counterparts (`$clicks` ↔ `props.clicks`); a direct match against the
  prefixed key still takes precedence. Only fields the subclass explicitly
  declared participate, and `props` and `tac` are skipped so a malicious
  prop object cannot overwrite framework state.
- New `./decorators` package export resolving to `src/runtime/decorators.js`.

### Changed

- The build-time compiler class previously exported as `Tac` from the
  `./compiler` entry is now exported as `Compiler`, and its source moved from
  `src/compiler/template-compiler.js` to `src/compiler/index.js`. The
  `./compiler` package export still resolves; consumers who imported the
  default export under the name `Tac` should rename the binding to `Compiler`
  to avoid colliding with the runtime `Tac` class.
- Internal reorganization of `src/server/`: flat files moved into
  responsibility-scoped subdirectories. `route-handler.js`, `browser-env.js`,
  and `schema-validator.js` are now under `src/server/http/`; `openapi.js`
  under `src/server/openapi/`; `logger.js`, `console-logger.js`, and
  `telemetry.js` under `src/server/observability/`; `process-pool.js` under
  `src/server/process/`. `yon.js` stays at the `src/server/` root.
  `package.json#main`, `#types`, and `exports."."` were updated to the new
  `src/server/http/route-handler.js` path; `exports."./server"` is unchanged.
- Build-time manifests (`route-manifest.json`, `shell-manifest.json`) moved
  out of `src/runtime/` into `src/shared/manifests/` to separate generated
  artifacts from runtime source. Compiler path constants and `.gitignore`
  updated.

### Removed

- Unused `Compiler` import from `src/cli/serve.js`. The compiler module has
  no top-level side effects, so the import was strictly dead.
- Orphan `layout-manifest.json`. The compiler writes `Compiler.layoutMapping`
  to `shell-manifest.json`; nothing in the codebase ever read or wrote
  `layout-manifest.json`. Removed from `src/shared/manifests/` and
  `.gitignore`.

### Fixed

- Upgraded `@d31ma/fylo` to `26.21.2` (bringing `@d31ma/chex` `26.21.02`),
  which introduced a `SchemaValidator` API requiring `.schema.json` extension
  in schema references and `path:`-prefixed cache keys. Aligned
  `validateWithChex` to seed the CHEX cache with the expected key format.
- Updated `Router.optionsFileName` from `OPTIONS.json` to `OPTIONS.schema.json`
  so the route handler discovers the renamed schema files.
- Renamed route `OPTIONS.json` files to `OPTIONS.schema.json` and schema
  history files (`v1.json`) to `v1.schema.json` for consistency with the
  framework's naming convention. Test env file moved from root to
  `tests/.env.test`.

## [2.0.0]

Tachyon 2.0 is a full rewrite. The framework splits into `Tac` (frontend) and
`Yon` (backend/runtime), the source moves from TypeScript to JavaScript with
strict JSDoc, and the CLI surface is renamed. Consumers upgrading from 1.x
should expect to touch imports, bin names, and any code that assumed the old
`dist/layouts/` output.

### Breaking Changes

- Source rewritten from TypeScript to JavaScript + JSDoc. Package `main`,
  `types`, and the `.`, `./server`, `./compiler` exports now resolve to `.js`
  files.
- `./server` export now points at `src/server/yon.js` (previously
  `process-executor.ts`).
- CLI binaries renamed:
  - `tach.init` → `yon.init`
  - `tach.serve` → `yon.serve`
  - `tach.bundle` → `tac.bundle`
  - `tach.preview` → `tac.preview`
- Removed `dist/layouts/` output. Page shells are now represented through
  `shells.json`, and the runtime uses a single app shell template with the HMR
  client injected only in development.

### Added

- Polyglot backend handlers: any executable file with a shebang can serve a
  route, receiving JSON on stdin.
- OOP-style companion classes with `export default class extends Tac`, plus
  support for anonymous subclasses.
- Automatic `sessionStorage` persistence for `$`-prefixed instance fields,
  keyed by module path + page/component identity + field name.
- Local-first browser `fetch()` inside Tac page/component scripts, backed by
  IndexedDB with read caching for `GET`/`HEAD` and mutation-aware invalidation.
- Explicit browser env allowlisting through `TAC_PUBLIC_ENV` and
  `this.env(key, fallback)`.
- Generated OpenAPI 3.1 document at `/openapi.json` and a self-hosted Tachyon
  docs UI at `/api-docs` with request auth, filtering, deep links, cURL
  generation, and live "try it out" execution.
- FYLO-backed OpenTelemetry storage with request and handler span correlation,
  `traceparent` continuation, and response `Traceparent` / `X-Trace-Id` headers.
  Configured through `OTEL_ENABLED`, `OTEL_FYLO_ROOT`, `OTEL_SERVICE_NAME`, and
  `OTEL_CAPTURE_IP`.
- Scoped component CSS via `@scope ([data-tac-scope="..."])` wrappers.
- Shared frontend surface: `browser/shared/assets/*`, `browser/shared/data/*`,
  `browser/shared/scripts/main.js` entry, and `browser/shared/styles/*`.
- Built-in health/readiness endpoints: `/health`, `/healthz`, `/ready`,
  `/readyz`.
- Shape-aware `yon.serve`: `browser/`-only bundles and serves the frontend,
  `server/`-only serves backend routes, and apps with both run full-stack on
  one port.
- Tac template runtime with `{expr}`, `{!expr}`, `@event`, `:prop`,
  `:value`, `<loop :for>`, `<logic :if>`, and `<my-component lazy />`.
- `Tac` helpers: `env`, `fetch`, `emit`, `inject`, `provide`, `onMount`,
  `rerender`, `isBrowser`, `isServer`, `props`.
- Distributed rate limiting via an exported `rateLimiter` from `middleware.js`
  (Upstash Redis supported out of the box); in-memory limiter retained for
  single-instance deployments.
- Production hardening: Bun password verification for hashed Basic Auth,
  request body and parameter limits, handler timeout enforcement, JWT expiry
  rejection when decodable, route request/response validation through
  `OPTIONS`, configurable CSP, optional HSTS.
- Proxy-aware request context, origin-aware CORS rejection before handler
  execution, and document-request detection using `Sec-Fetch-*` headers with
  `Accept: text/html` as fallback.
- Cache headers for runtime assets, chunks, shared assets, and shared data.
- Inferred content type on handler responses (`application/json` for
  JSON-shaped output, `text/plain` otherwise).
- `tac.bundle --watch` and `tac.preview --watch` for iterative static builds.

### Changed

- Publishing moved to GitHub Packages under `@d31ma/tachyon`.
- Scaffolds are now JavaScript-first with strict JSDoc; TypeScript companion
  scripts remain supported at runtime.

### Fixed

- `findDocs('otel-spans')` call in the telemetry integration test now passes
  the required `StoreQuery` argument so `tsc --noEmit` stays clean against
  `@d31ma/fylo` 2.3.

## [1.11.1] and earlier

See the Git history and GitHub release notes for pre-2.0 changes.

[Unreleased]: https://github.com/d31ma/Tachyon/compare/v26.22.03...HEAD
[26.22.03]: https://github.com/d31ma/Tachyon/compare/v26.21.7...v26.22.03
[26.21.7]: https://github.com/d31ma/Tachyon/compare/v26.21.05...v26.21.7
[26.21.05]: https://github.com/d31ma/Tachyon/compare/v2.0.0...v26.21.05
[2.0.0]: https://github.com/d31ma/Tachyon/compare/v1.11.1...v2.0.0
[1.11.1]: https://github.com/d31ma/Tachyon/releases/tag/v1.11.1
