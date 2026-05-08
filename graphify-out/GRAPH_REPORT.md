# Graph Report - TACHYON  (2026-05-08)

## Corpus Check
- 126 files · ~78,888 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 855 nodes · 1610 edges · 34 communities detected
- Extraction: 80% EXTRACTED · 20% INFERRED · 0% AMBIGUOUS · INFERRED: 323 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 8|Community 8]]
- [[_COMMUNITY_Community 9|Community 9]]
- [[_COMMUNITY_Community 10|Community 10]]
- [[_COMMUNITY_Community 11|Community 11]]
- [[_COMMUNITY_Community 12|Community 12]]
- [[_COMMUNITY_Community 13|Community 13]]
- [[_COMMUNITY_Community 14|Community 14]]
- [[_COMMUNITY_Community 15|Community 15]]
- [[_COMMUNITY_Community 16|Community 16]]
- [[_COMMUNITY_Community 17|Community 17]]
- [[_COMMUNITY_Community 18|Community 18]]
- [[_COMMUNITY_Community 20|Community 20]]
- [[_COMMUNITY_Community 21|Community 21]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 23|Community 23]]
- [[_COMMUNITY_Community 24|Community 24]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 27|Community 27]]
- [[_COMMUNITY_Community 30|Community 30]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 37|Community 37]]
- [[_COMMUNITY_Community 39|Community 39]]

## God Nodes (most connected - your core abstractions)
1. `Compiler` - 76 edges
2. `Yon` - 38 edges
3. `get()` - 36 edges
4. `set()` - 25 edges
5. `YonCompiledRunner` - 25 edges
6. `Router` - 24 edges
7. `handler()` - 21 edges
8. `text()` - 19 edges
9. `FyloBrowser` - 18 edges
10. `has()` - 17 edges

## Surprising Connections (you probably didn't know these)
- `getAvailablePort()` --calls--> `has()`  [INFERRED]
  tests/integration/api-routes.test.js → src/runtime/fylo-browser/app.js
- `createBackendOnlyApp()` --calls--> `write()`  [INFERRED]
  tests/integration/api-routes.test.js → src/server/process/adapters/yon-python-runner.py
- `text()` --calls--> `executeOperation()`  [INFERRED]
  tests/server/handler-adapter.test.js → src/runtime/docs/openapi-docs.js
- `text()` --calls--> `tailEvents()`  [INFERRED]
  tests/server/handler-adapter.test.js → src/server/fylo-browser/fylo-browser.js
- `trackBrowserErrors()` --calls--> `get()`  [INFERRED]
  tests/playwright/examples.e2e.js → src/runtime/fylo-browser/app.js

## Communities

### Community 0 - "Community 0"
Cohesion: 0.05
Nodes (12): getAvailablePort(), has(), classifyChange(), normalizeRelative(), pathExists(), runBuild(), runSelectiveBuild(), runWithConcurrency() (+4 more)

### Community 1 - "Community 1"
Cohesion: 0.05
Nodes (22): createAppScaffold(), ensureEmptyDirectory(), decode(), jsonOk(), basename(), HandlerAdapter, OpenAPI, basename() (+14 more)

### Community 2 - "Community 2"
Cohesion: 0.05
Nodes (20): buildRouteOutput(), writeRouteOutput(), GET, handler(), writeExecutableHandler(), LanguageRepository, LanguageService, handler() (+12 more)

### Community 3 - "Community 3"
Cohesion: 0.06
Nodes (16): set(), createPublicBrowserEnvResponse(), createPublicBrowserEnvScript(), getPublicBrowserEnv(), splitList(), withPublicBrowserEnv(), jsonResponse(), jsResponse() (+8 more)

### Community 4 - "Community 4"
Cohesion: 0.08
Nodes (7): FyloTelemetryStore, SpanFactory, Telemetry, TelemetryConfig, TelemetrySanitizer, TelemetryStore, TraceContextParser

### Community 5 - "Community 5"
Cohesion: 0.11
Nodes (28): __fyloCollection(), get(), browserContentSecurityPolicy(), deleteDocument(), executeQuery(), FyloBrowser, fyloRoot(), getDocument() (+20 more)

### Community 6 - "Community 6"
Cohesion: 0.08
Nodes (7): handler(), ItemService, JsonItemRepository, handler(), handler(), RustLanguageService, loadManifests()

### Community 7 - "Community 7"
Cohesion: 0.12
Nodes (31): applySecurity(), buildCurl(), buildExecution(), buildRequestPayload(), defaultParameterValue(), defaultRequestBody(), ensureOperationState(), escapeHtml() (+23 more)

### Community 8 - "Community 8"
Cohesion: 0.13
Nodes (28): getAttribute(), cleanBooleanAttrs(), findEventTarget(), isSameNode(), morphChildren(), parseFragment(), resolveHandler(), syncAttributes() (+20 more)

### Community 9 - "Community 9"
Cohesion: 0.15
Nodes (6): canRunPlaywright(), decode(), resolveNodeCommand(), runCommand(), text(), YonCompiledRunner

### Community 10 - "Community 10"
Cohesion: 0.15
Nodes (18): alloc(), call(), copy_bytes(), copyText(), dealloc(), init(), initTac(), output_len() (+10 more)

### Community 11 - "Community 11"
Cohesion: 0.14
Nodes (17): main(), run(), startServer(), buildPrettyLine(), createLogger(), createWriteTarget(), needsQuoting(), normalizeConsoleArgs() (+9 more)

### Community 12 - "Community 12"
Cohesion: 0.1
Nodes (3): FyloBrowser, env(), render()

### Community 13 - "Community 13"
Cohesion: 0.19
Nodes (19): appendEvents(), encryptionBanner(), errorMessage(), __fyloFetch(), __fyloPostJson(), loadCollections(), loadMeta(), pollEvents() (+11 more)

### Community 14 - "Community 14"
Cohesion: 0.2
Nodes (7): addItem(), bindRefreshListener(), clearItems(), loadHealth(), loadingState(), loadItemsCount(), refresh()

### Community 15 - "Community 15"
Cohesion: 0.16
Nodes (3): FyloTelemetryRepository, OtlpValueDecoder, TelemetryService

### Community 16 - "Community 16"
Cohesion: 0.19
Nodes (15): buildFyloBrowserPaths(), buildOperation(), buildSpec(), describeStatus(), docsContentSecurityPolicy(), docsScript(), docsStyles(), getDocsAssets() (+7 more)

### Community 17 - "Community 17"
Cohesion: 0.14
Nodes (7): decode(), runBundle(), expectInteractiveSurface(), expectNoBrowserErrors(), trackBrowserErrors(), waitForDashboardReady(), expect()

### Community 18 - "Community 18"
Cohesion: 0.17
Nodes (10): authFetch(), authHeaders(), createBackendOnlyApp(), decodeAnyValue(), describeEarlyExit(), extractPersistedSpans(), getResourceAttribute(), readTextStream() (+2 more)

### Community 20 - "Community 20"
Cohesion: 0.19
Nodes (9): JavaLanguageService, POST, localFirstFetch(), ty_createScope(), __ty_deleteCachedResponse(), __ty_isBrowserEnv(), __ty_openFetchCache(), __ty_readCachedResponse() (+1 more)

### Community 21 - "Community 21"
Cohesion: 0.27
Nodes (6): assertRun(), copyIfExists(), createTachyonApp(), run(), tachyonTarball(), uniqueName()

### Community 22 - "Community 22"
Cohesion: 0.2
Nodes (1): Fixture

### Community 23 - "Community 23"
Cohesion: 0.31
Nodes (4): call(), init(), readClicks(), writeState()

### Community 24 - "Community 24"
Cohesion: 0.32
Nodes (3): applyTheme(), refreshAll(), toggleTheme()

### Community 25 - "Community 25"
Cohesion: 0.33
Nodes (2): __fyloFetch(), __fyloPostJson()

### Community 27 - "Community 27"
Cohesion: 0.7
Nodes (4): buildFingerprint(), pathFingerprint(), runFreshBundleBuild(), startPreviewBundleWatcher()

### Community 30 - "Community 30"
Cohesion: 0.67
Nodes (1): Fixture

### Community 31 - "Community 31"
Cohesion: 0.67
Nodes (2): dart_language_service.dart, DartLanguageService

### Community 32 - "Community 32"
Cohesion: 0.67
Nodes (1): GoLanguageService

### Community 33 - "Community 33"
Cohesion: 0.67
Nodes (1): PhpLanguageService

### Community 34 - "Community 34"
Cohesion: 0.67
Nodes (1): CSharpLanguageService

### Community 35 - "Community 35"
Cohesion: 0.67
Nodes (1): RubyLanguageService

### Community 37 - "Community 37"
Cohesion: 0.67
Nodes (1): Tac

### Community 39 - "Community 39"
Cohesion: 1.0
Nodes (1): DartLanguageService

## Knowledge Gaps
- **5 isolated node(s):** `Fixture`, `DartLanguageService`, `dart_language_service.dart`, `DartLanguageService`, `YonPythonRunner`
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 22`** (10 nodes): `createHelpers()`, `Fixture`, `.boom()`, `.boot()`, `.save()`, `.saveFail()`, `.saveOk()`, `.tick()`, `.work()`, `tac-decorators.test.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 25`** (7 nodes): `createFyloClient()`, `__fyloBasicAuth()`, `__fyloCollection()`, `__fyloFetch()`, `__fyloPostJson()`, `resolveBrowserPath()`, `fylo-global.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 30`** (3 nodes): `createHelpers()`, `Fixture`, `tac.test.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 31`** (3 nodes): `dart_language_service.dart`, `DELETE.dart`, `DartLanguageService`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 32`** (3 nodes): `go_language_service.go`, `GoLanguageService`, `.Describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 33`** (3 nodes): `php_language_service.php`, `PhpLanguageService`, `.describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 34`** (3 nodes): `CSharpLanguageService`, `.Describe()`, `CSharpLanguageService.cs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 35`** (3 nodes): `ruby_language_service.rb`, `RubyLanguageService`, `.describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 37`** (3 nodes): `tac.js`, `Tac`, `.constructor()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 39`** (2 nodes): `dart_language_service.dart`, `DartLanguageService`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `get()` connect `Community 5` to `Community 0`, `Community 1`, `Community 3`, `Community 4`, `Community 6`, `Community 7`, `Community 8`, `Community 12`, `Community 13`, `Community 17`, `Community 20`?**
  _High betweenness centrality (0.255) - this node is a cross-community bridge._
- **Why does `Compiler` connect `Community 0` to `Community 1`, `Community 2`, `Community 3`?**
  _High betweenness centrality (0.109) - this node is a cross-community bridge._
- **Why does `set()` connect `Community 3` to `Community 0`, `Community 1`, `Community 2`, `Community 4`, `Community 5`, `Community 6`, `Community 7`, `Community 8`, `Community 13`, `Community 16`, `Community 17`, `Community 25`?**
  _High betweenness centrality (0.096) - this node is a cross-community bridge._
- **Are the 33 inferred relationships involving `get()` (e.g. with `trackBrowserErrors()` and `.request_id()`) actually correct?**
  _`get()` has 33 INFERRED edges - model-reasoned connections that need verification._
- **Are the 23 inferred relationships involving `set()` (e.g. with `trackBrowserErrors()` and `.fetchTelemetry()`) actually correct?**
  _`set()` has 23 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Fixture`, `DartLanguageService`, `dart_language_service.dart` to the rest of the system?**
  _5 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.05 - nodes in this community are weakly interconnected._