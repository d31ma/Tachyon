# Graph Report - TACHYON  (2026-04-30)

## Corpus Check
- 118 files · ~72,598 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 793 nodes · 1493 edges · 30 communities detected
- Extraction: 78% EXTRACTED · 22% INFERRED · 0% AMBIGUOUS · INFERRED: 322 edges (avg confidence: 0.8)
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
- [[_COMMUNITY_Community 18|Community 18]]
- [[_COMMUNITY_Community 19|Community 19]]
- [[_COMMUNITY_Community 20|Community 20]]
- [[_COMMUNITY_Community 21|Community 21]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 27|Community 27]]
- [[_COMMUNITY_Community 28|Community 28]]
- [[_COMMUNITY_Community 29|Community 29]]
- [[_COMMUNITY_Community 31|Community 31]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]

## God Nodes (most connected - your core abstractions)
1. `Compiler` - 63 edges
2. `Yon` - 38 edges
3. `get()` - 36 edges
4. `YonCompiledRunner` - 25 edges
5. `Router` - 24 edges
6. `set()` - 23 edges
7. `handler()` - 21 edges
8. `text()` - 20 edges
9. `FyloBrowser` - 18 edges
10. `has()` - 17 edges

## Surprising Connections (you probably didn't know these)
- `clearItems()` --calls--> `handler()`  [INFERRED]
  examples\browser\components\panel\inventory\index.ts → examples\server\routes\languages\typescript\items\_id\DELETE.ts
- `loadEnvFile()` --calls--> `text()`  [INFERRED]
  scripts\create-localstack-buckets.js → tests\server\handler-adapter.test.js
- `has()` --calls--> `getAvailablePort()`  [INFERRED]
  src\runtime\fylo-browser\app.js → tests\integration\api-routes.test.js
- `handler()` --calls--> `buildRouteOutput()`  [INFERRED]
  examples\server\routes\languages\typescript\items\_id\GET.ts → src\cli\bundle.js
- `handler()` --calls--> `buildRouteOutput()`  [INFERRED]
  examples\server\routes\languages\typescript\items\_id\PUT.ts → src\cli\bundle.js

## Communities

### Community 0 - "Community 0"
Cohesion: 0.05
Nodes (18): has(), buildRouteOutput(), classifyChange(), normalizeRelative(), pathExists(), runBuild(), runSelectiveBuild(), runWithConcurrency() (+10 more)

### Community 1 - "Community 1"
Cohesion: 0.05
Nodes (10): decode(), jsonOk(), basename(), Pool, tokenizeCommand(), Router, Validate, configureRoutes() (+2 more)

### Community 2 - "Community 2"
Cohesion: 0.04
Nodes (20): handler(), GET, handler(), ItemService, JsonItemRepository, LanguageRepository, LanguageService, handler() (+12 more)

### Community 3 - "Community 3"
Cohesion: 0.06
Nodes (30): getAttribute(), FyloBrowser, env(), render(), cleanBooleanAttrs(), findEventTarget(), isSameNode(), morphChildren() (+22 more)

### Community 4 - "Community 4"
Cohesion: 0.09
Nodes (6): writeExecutableHandler(), buildTestFactory(), TelemetryAlertWorker, YonCompiledRunner, write(), YonRubyRunner

### Community 5 - "Community 5"
Cohesion: 0.08
Nodes (7): FyloTelemetryStore, SpanFactory, Telemetry, TelemetryConfig, TelemetrySanitizer, TelemetryStore, TraceContextParser

### Community 6 - "Community 6"
Cohesion: 0.07
Nodes (21): createAppScaffold(), ensureEmptyDirectory(), canRunPlaywright(), decode(), resolveNodeCommand(), runCommand(), basename(), HandlerAdapter (+13 more)

### Community 7 - "Community 7"
Cohesion: 0.11
Nodes (32): set(), applySecurity(), buildCurl(), buildExecution(), buildRequestPayload(), defaultParameterValue(), defaultRequestBody(), ensureOperationState() (+24 more)

### Community 8 - "Community 8"
Cohesion: 0.11
Nodes (22): appendEvents(), encryptionBanner(), errorMessage(), __fyloCollection(), get(), loadCollections(), loadMeta(), pollEvents() (+14 more)

### Community 9 - "Community 9"
Cohesion: 0.1
Nodes (21): main(), run(), startServer(), endpointArgs(), ensureBucket(), loadEnvFile(), run(), buildPrettyLine() (+13 more)

### Community 10 - "Community 10"
Cohesion: 0.17
Nodes (24): browserContentSecurityPolicy(), deleteDocument(), executeQuery(), FyloBrowser, fyloRoot(), getDocument(), getEncryptedFields(), isReadOnlyQuery() (+16 more)

### Community 11 - "Community 11"
Cohesion: 0.15
Nodes (16): buildFyloBrowserPaths(), buildOperation(), buildSpec(), describeStatus(), docsContentSecurityPolicy(), docsScript(), docsStyles(), getDocsAssets() (+8 more)

### Community 12 - "Community 12"
Cohesion: 0.2
Nodes (7): addItem(), bindRefreshListener(), clearItems(), loadHealth(), loadingState(), loadItemsCount(), refresh()

### Community 13 - "Community 13"
Cohesion: 0.16
Nodes (3): FyloTelemetryRepository, OtlpValueDecoder, TelemetryService

### Community 14 - "Community 14"
Cohesion: 0.14
Nodes (7): decode(), runBundle(), expectInteractiveSurface(), expectNoBrowserErrors(), trackBrowserErrors(), waitForDashboardReady(), expect()

### Community 15 - "Community 15"
Cohesion: 0.19
Nodes (9): JavaLanguageService, POST, localFirstFetch(), ty_createScope(), __ty_deleteCachedResponse(), __ty_isBrowserEnv(), __ty_openFetchCache(), __ty_readCachedResponse() (+1 more)

### Community 16 - "Community 16"
Cohesion: 0.17
Nodes (11): authFetch(), authHeaders(), createBackendOnlyApp(), decodeAnyValue(), describeEarlyExit(), extractPersistedSpans(), getAvailablePort(), getResourceAttribute() (+3 more)

### Community 18 - "Community 18"
Cohesion: 0.2
Nodes (1): Fixture

### Community 19 - "Community 19"
Cohesion: 0.32
Nodes (3): applyTheme(), refreshAll(), toggleTheme()

### Community 20 - "Community 20"
Cohesion: 0.4
Nodes (2): __fyloCollection(), get()

### Community 21 - "Community 21"
Cohesion: 0.6
Nodes (5): createPublicBrowserEnvResponse(), createPublicBrowserEnvScript(), getPublicBrowserEnv(), splitList(), withPublicBrowserEnv()

### Community 22 - "Community 22"
Cohesion: 0.7
Nodes (4): buildFingerprint(), pathFingerprint(), runFreshBundleBuild(), startPreviewBundleWatcher()

### Community 25 - "Community 25"
Cohesion: 0.67
Nodes (2): DartLanguageService, dart_language_service.dart

### Community 26 - "Community 26"
Cohesion: 0.67
Nodes (1): CSharpLanguageService

### Community 27 - "Community 27"
Cohesion: 0.67
Nodes (1): GoLanguageService

### Community 28 - "Community 28"
Cohesion: 0.67
Nodes (1): PhpLanguageService

### Community 29 - "Community 29"
Cohesion: 0.67
Nodes (1): RubyLanguageService

### Community 31 - "Community 31"
Cohesion: 0.67
Nodes (1): Tac

### Community 33 - "Community 33"
Cohesion: 0.67
Nodes (1): Fixture

### Community 34 - "Community 34"
Cohesion: 1.0
Nodes (1): DartLanguageService

## Knowledge Gaps
- **5 isolated node(s):** `DartLanguageService`, `dart_language_service.dart`, `DartLanguageService`, `YonPythonRunner`, `Fixture`
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 18`** (10 nodes): `createHelpers()`, `Fixture`, `.boom()`, `.boot()`, `.save()`, `.saveFail()`, `.saveOk()`, `.tick()`, `.work()`, `tac-decorators.test.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 20`** (6 nodes): `imports.js`, `__fyloCollection()`, `__fyloPostJson()`, `get()`, `has()`, `set()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 25`** (3 nodes): `DartLanguageService`, `dart_language_service.dart`, `DELETE.dart`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 26`** (3 nodes): `CSharpLanguageService`, `.Describe()`, `CSharpLanguageService.cs`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 27`** (3 nodes): `go_language_service.go`, `GoLanguageService`, `.Describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 28`** (3 nodes): `php_language_service.php`, `PhpLanguageService`, `.describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 29`** (3 nodes): `ruby_language_service.rb`, `RubyLanguageService`, `.describe()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 31`** (3 nodes): `tac.js`, `Tac`, `.constructor()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 33`** (3 nodes): `createHelpers()`, `Fixture`, `tac.test.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 34`** (2 nodes): `DartLanguageService`, `dart_language_service.dart`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `get()` connect `Community 8` to `Community 0`, `Community 1`, `Community 2`, `Community 3`, `Community 5`, `Community 6`, `Community 7`, `Community 10`, `Community 14`, `Community 15`?**
  _High betweenness centrality (0.287) - this node is a cross-community bridge._
- **Why does `Compiler` connect `Community 0` to `Community 1`, `Community 4`?**
  _High betweenness centrality (0.100) - this node is a cross-community bridge._
- **Why does `text()` connect `Community 0` to `Community 1`, `Community 4`, `Community 6`, `Community 7`, `Community 9`, `Community 10`, `Community 14`, `Community 16`?**
  _High betweenness centrality (0.099) - this node is a cross-community bridge._
- **Are the 33 inferred relationships involving `get()` (e.g. with `.request_id()` and `.version()`) actually correct?**
  _`get()` has 33 INFERRED edges - model-reasoned connections that need verification._
- **What connects `DartLanguageService`, `dart_language_service.dart`, `DartLanguageService` to the rest of the system?**
  _5 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.05 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.05 - nodes in this community are weakly interconnected._