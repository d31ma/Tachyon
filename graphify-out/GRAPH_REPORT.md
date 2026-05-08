# Graph Report - TACHYON  (2026-05-08)

## Corpus Check
- 123 files · ~77,643 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 841 nodes · 1590 edges · 35 communities detected
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
- [[_COMMUNITY_Community 19|Community 19]]
- [[_COMMUNITY_Community 21|Community 21]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 23|Community 23]]
- [[_COMMUNITY_Community 24|Community 24]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 28|Community 28]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 36|Community 36]]
- [[_COMMUNITY_Community 38|Community 38]]
- [[_COMMUNITY_Community 40|Community 40]]

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
Cohesion: 0.03
Nodes (26): buildRouteOutput(), writeRouteOutput(), handler(), GET, handler(), writeExecutableHandler(), ItemService, JsonItemRepository (+18 more)

### Community 1 - "Community 1"
Cohesion: 0.05
Nodes (9): pathExists(), runSelectiveBuild(), text(), Compiler, jsonResponse(), jsResponse(), pathExists(), staticRouteResponse() (+1 more)

### Community 2 - "Community 2"
Cohesion: 0.05
Nodes (15): has(), set(), decode(), basename(), basename(), Pool, tokenizeCommand(), Validate (+7 more)

### Community 3 - "Community 3"
Cohesion: 0.06
Nodes (16): __fyloCollection(), get(), jsonOk(), PythonLanguageRepository, PythonLanguageService, Router, configureRoutes(), detectAppShape() (+8 more)

### Community 4 - "Community 4"
Cohesion: 0.08
Nodes (7): FyloTelemetryStore, SpanFactory, Telemetry, TelemetryConfig, TelemetrySanitizer, TelemetryStore, TraceContextParser

### Community 5 - "Community 5"
Cohesion: 0.12
Nodes (31): applySecurity(), buildCurl(), buildExecution(), buildRequestPayload(), defaultParameterValue(), defaultRequestBody(), ensureOperationState(), escapeHtml() (+23 more)

### Community 6 - "Community 6"
Cohesion: 0.1
Nodes (23): main(), run(), startServer(), classifyChange(), normalizeRelative(), runBuild(), runWithConcurrency(), startBundleWatcher() (+15 more)

### Community 7 - "Community 7"
Cohesion: 0.14
Nodes (27): getAttribute(), cleanBooleanAttrs(), findEventTarget(), isSameNode(), morphChildren(), parseFragment(), resolveHandler(), syncAttributes() (+19 more)

### Community 8 - "Community 8"
Cohesion: 0.17
Nodes (24): browserContentSecurityPolicy(), deleteDocument(), executeQuery(), FyloBrowser, fyloRoot(), getDocument(), getEncryptedFields(), isReadOnlyQuery() (+16 more)

### Community 9 - "Community 9"
Cohesion: 0.15
Nodes (18): alloc(), call(), copy_bytes(), copyText(), dealloc(), init(), initTac(), output_len() (+10 more)

### Community 10 - "Community 10"
Cohesion: 0.1
Nodes (3): FyloBrowser, env(), render()

### Community 11 - "Community 11"
Cohesion: 0.21
Nodes (1): YonCompiledRunner

### Community 12 - "Community 12"
Cohesion: 0.21
Nodes (19): appendEvents(), encryptionBanner(), errorMessage(), __fyloFetch(), __fyloPostJson(), loadCollections(), loadMeta(), pollEvents() (+11 more)

### Community 13 - "Community 13"
Cohesion: 0.15
Nodes (16): buildFyloBrowserPaths(), buildOperation(), buildSpec(), describeStatus(), docsContentSecurityPolicy(), docsScript(), docsStyles(), getDocsAssets() (+8 more)

### Community 14 - "Community 14"
Cohesion: 0.2
Nodes (7): addItem(), bindRefreshListener(), clearItems(), loadHealth(), loadingState(), loadItemsCount(), refresh()

### Community 15 - "Community 15"
Cohesion: 0.16
Nodes (3): FyloTelemetryRepository, OtlpValueDecoder, TelemetryService

### Community 16 - "Community 16"
Cohesion: 0.15
Nodes (11): authFetch(), authHeaders(), createBackendOnlyApp(), decodeAnyValue(), describeEarlyExit(), extractPersistedSpans(), getAvailablePort(), getResourceAttribute() (+3 more)

### Community 17 - "Community 17"
Cohesion: 0.14
Nodes (7): decode(), runBundle(), expectInteractiveSurface(), expectNoBrowserErrors(), trackBrowserErrors(), waitForDashboardReady(), expect()

### Community 18 - "Community 18"
Cohesion: 0.18
Nodes (7): createAppScaffold(), ensureEmptyDirectory(), canRunPlaywright(), decode(), resolveNodeCommand(), runCommand(), HandlerAdapter

### Community 19 - "Community 19"
Cohesion: 0.19
Nodes (9): JavaLanguageService, POST, localFirstFetch(), ty_createScope(), __ty_deleteCachedResponse(), __ty_isBrowserEnv(), __ty_openFetchCache(), __ty_readCachedResponse() (+1 more)

### Community 21 - "Community 21"
Cohesion: 0.2
Nodes (1): Fixture

### Community 22 - "Community 22"
Cohesion: 0.31
Nodes (4): call(), init(), readClicks(), writeState()

### Community 23 - "Community 23"
Cohesion: 0.32
Nodes (3): applyTheme(), refreshAll(), toggleTheme()

### Community 24 - "Community 24"
Cohesion: 0.43
Nodes (1): TelemetryAlertWorker

### Community 25 - "Community 25"
Cohesion: 0.4
Nodes (2): __fyloFetch(), __fyloPostJson()

### Community 26 - "Community 26"
Cohesion: 0.6
Nodes (5): createPublicBrowserEnvResponse(), createPublicBrowserEnvScript(), getPublicBrowserEnv(), splitList(), withPublicBrowserEnv()

### Community 28 - "Community 28"
Cohesion: 0.7
Nodes (4): buildFingerprint(), pathFingerprint(), runFreshBundleBuild(), startPreviewBundleWatcher()

### Community 31 - "Community 31"
Cohesion: 0.67
Nodes (1): Fixture

### Community 32 - "Community 32"
Cohesion: 0.67
Nodes (2): dart_language_service.dart, DartLanguageService

### Community 33 - "Community 33"
Cohesion: 0.67
Nodes (1): GoLanguageService

### Community 34 - "Community 34"
Cohesion: 0.67
Nodes (1): PhpLanguageService

### Community 35 - "Community 35"
Cohesion: 0.67
Nodes (1): CSharpLanguageService

### Community 36 - "Community 36"
Cohesion: 0.67
Nodes (1): RubyLanguageService

### Community 38 - "Community 38"
Cohesion: 0.67
Nodes (1): Tac

### Community 40 - "Community 40"
Cohesion: 1.0
Nodes (1): DartLanguageService

## Knowledge Gaps
- **5 isolated node(s):** `Fixture`, `DartLanguageService`, `dart_language_service.dart`, `DartLanguageService`, `YonPythonRunner`
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 11`** (26 nodes): `yon-compiled-runner.js`, `YonCompiledRunner`, `.compileRust()`, `.copyServiceSources()`, `.dartExecutable()`, `.javaJsonSupportSource()`, `.javaMainSource()`, `.routeClassName()`, `.run()`, `.runCommand()`, `.runCSharp()`, `.runDart()`, `.runGo()`, `.runJava()`, `.runKotlin()`, `.runRust()`, `.rustJsonSupportSource()`, `.rustMainSource()`, `.rustTargetArgs()`, `.safeId()`, `.serviceExtension()`, `.servicesPath()`, `.windowsVcArch()`, `.windowsVcVarsPath()`, `.workspace()`, `.writeSourceWithoutShebang()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 21`** (10 nodes): `createHelpers()`, `Fixture`, `.boom()`, `.boot()`, `.save()`, `.saveFail()`, `.saveOk()`, `.tick()`, `.work()`, `tac-decorators.test.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 24`** (7 nodes): `telemetry-alert-worker.js`, `TelemetryAlertWorker`, `.buildAlerts()`, `.constructor()`, `.fetchTelemetry()`, `.numberEnv()`, `.run()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 25`** (6 nodes): `createFyloClient()`, `__fyloCollection()`, `__fyloFetch()`, `__fyloPostJson()`, `resolveBrowserPath()`, `fylo-global.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 31`** (3 nodes): `createHelpers()`, `Fixture`, `tac.test.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 32`** (3 nodes): `dart_language_service.dart`, `DELETE.dart`, `DartLanguageService`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 33`** (3 nodes): `go_language_service.go`, `GoLanguageService`, `.Describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 34`** (3 nodes): `php_language_service.php`, `PhpLanguageService`, `.describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 35`** (3 nodes): `CSharpLanguageService`, `.Describe()`, `CSharpLanguageService.cs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 36`** (3 nodes): `ruby_language_service.rb`, `RubyLanguageService`, `.describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 38`** (3 nodes): `tac.js`, `Tac`, `.constructor()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 40`** (2 nodes): `dart_language_service.dart`, `DartLanguageService`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `get()` connect `Community 3` to `Community 0`, `Community 1`, `Community 2`, `Community 4`, `Community 5`, `Community 7`, `Community 8`, `Community 10`, `Community 12`, `Community 17`, `Community 19`?**
  _High betweenness centrality (0.262) - this node is a cross-community bridge._
- **Why does `Compiler` connect `Community 1` to `Community 0`, `Community 2`, `Community 3`?**
  _High betweenness centrality (0.113) - this node is a cross-community bridge._
- **Why does `set()` connect `Community 2` to `Community 1`, `Community 3`, `Community 4`, `Community 5`, `Community 7`, `Community 8`, `Community 12`, `Community 13`, `Community 17`, `Community 24`, `Community 25`?**
  _High betweenness centrality (0.097) - this node is a cross-community bridge._
- **Are the 33 inferred relationships involving `get()` (e.g. with `trackBrowserErrors()` and `.request_id()`) actually correct?**
  _`get()` has 33 INFERRED edges - model-reasoned connections that need verification._
- **Are the 23 inferred relationships involving `set()` (e.g. with `trackBrowserErrors()` and `.fetchTelemetry()`) actually correct?**
  _`set()` has 23 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Fixture`, `DartLanguageService`, `dart_language_service.dart` to the rest of the system?**
  _5 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.03 - nodes in this community are weakly interconnected._