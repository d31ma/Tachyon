# Graph Report - TACHYON  (2026-04-23)

## Corpus Check
- 50 files · ~47,719 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 478 nodes · 828 edges · 19 communities detected
- Extraction: 87% EXTRACTED · 13% INFERRED · 0% AMBIGUOUS · INFERRED: 110 edges (avg confidence: 0.8)
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
- [[_COMMUNITY_Community 12|Community 12]]
- [[_COMMUNITY_Community 13|Community 13]]
- [[_COMMUNITY_Community 14|Community 14]]
- [[_COMMUNITY_Community 15|Community 15]]
- [[_COMMUNITY_Community 16|Community 16]]
- [[_COMMUNITY_Community 18|Community 18]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 24|Community 24]]

## God Nodes (most connected - your core abstractions)
1. `Tac` - 61 edges
2. `Yon` - 36 edges
3. `Router` - 24 edges
4. `SpanFactory` - 15 edges
5. `Tac` - 13 edges
6. `runSelectiveBuild()` - 12 edges
7. `postPatch()` - 11 edges
8. `rerender()` - 10 edges
9. `TachyonLogger` - 10 edges
10. `configureRoutes()` - 9 edges

## Surprising Connections (you probably didn't know these)
- `isAssetRequest()` --calls--> `basename()`  [INFERRED]
  src\cli\serve.js → src\server\process-pool.js
- `expect()` --calls--> `expectNoBrowserErrors()`  [INFERRED]
  scripts\verify-package-contract.js → tests\playwright\examples.e2e.js
- `expect()` --calls--> `waitForDashboardReady()`  [INFERRED]
  scripts\verify-package-contract.js → tests\playwright\examples.e2e.js
- `expect()` --calls--> `expectInteractiveSurface()`  [INFERRED]
  scripts\verify-package-contract.js → tests\playwright\examples.e2e.js
- `findEventTarget()` --calls--> `handleDelegatedEvent()`  [INFERRED]
  src\runtime\dom-helpers.js → src\runtime\spa-renderer.js

## Communities

### Community 0 - "Community 0"
Cohesion: 0.06
Nodes (8): pathExists(), runSelectiveBuild(), jsonResponse(), jsResponse(), pathExists(), staticRouteResponse(), Tac, typedResponse()

### Community 1 - "Community 1"
Cohesion: 0.07
Nodes (9): decode(), basename(), Pool, tokenizeCommand(), pathExists(), resolvePreviewFile(), serveStaticPreviewRequest(), shouldTreatAsAsset() (+1 more)

### Community 2 - "Community 2"
Cohesion: 0.08
Nodes (7): FyloTelemetryStore, SpanFactory, Telemetry, TelemetryConfig, TelemetrySanitizer, TelemetryStore, TraceContextParser

### Community 3 - "Community 3"
Cohesion: 0.08
Nodes (16): constructor(), connectHMR(), addItem(), applyTheme(), clearItems(), constructor(), loadDiagnostics(), loadHealth() (+8 more)

### Community 4 - "Community 4"
Cohesion: 0.12
Nodes (31): applySecurity(), buildCurl(), buildExecution(), buildRequestPayload(), defaultParameterValue(), defaultRequestBody(), ensureOperationState(), escapeHtml() (+23 more)

### Community 5 - "Community 5"
Cohesion: 0.1
Nodes (2): Router, Validate

### Community 6 - "Community 6"
Cohesion: 0.14
Nodes (27): getAttribute(), cleanBooleanAttrs(), findEventTarget(), isSameNode(), morphChildren(), parseFragment(), resolveHandler(), syncAttributes() (+19 more)

### Community 7 - "Community 7"
Cohesion: 0.11
Nodes (17): main(), run(), startServer(), buildPrettyLine(), createLogger(), createWriteTarget(), needsQuoting(), normalizeConsoleArgs() (+9 more)

### Community 8 - "Community 8"
Cohesion: 0.14
Nodes (6): decode(), runBundle(), expectInteractiveSurface(), expectNoBrowserErrors(), waitForDashboardReady(), expect()

### Community 9 - "Community 9"
Cohesion: 0.2
Nodes (14): buildOperation(), buildSpec(), describeStatus(), docsContentSecurityPolicy(), docsScript(), docsStyles(), getDocsAssets(), getPackageMetadata() (+6 more)

### Community 10 - "Community 10"
Cohesion: 0.17
Nodes (11): OpenAPI, configureRoutes(), detectAppShape(), directoryHasFiles(), isAssetRequest(), isAuthorizedHmrRequest(), isLoopbackHost(), loadMiddleware() (+3 more)

### Community 12 - "Community 12"
Cohesion: 0.22
Nodes (8): buildRouteOutput(), classifyChange(), normalizeRelative(), runBuild(), runWithConcurrency(), startBundleWatcher(), watchPaths(), writeRouteOutput()

### Community 13 - "Community 13"
Cohesion: 0.27
Nodes (7): authFetch(), authHeaders(), decodeAnyValue(), extractPersistedSpans(), getResourceAttribute(), waitForServer(), waitForTelemetrySpans()

### Community 14 - "Community 14"
Cohesion: 0.43
Nodes (1): TelemetryAlertWorker

### Community 15 - "Community 15"
Cohesion: 0.53
Nodes (4): canRunPlaywright(), decode(), resolveNodeCommand(), runCommand()

### Community 16 - "Community 16"
Cohesion: 0.7
Nodes (4): buildFingerprint(), pathFingerprint(), runFreshBundleBuild(), startPreviewBundleWatcher()

### Community 18 - "Community 18"
Cohesion: 0.83
Nodes (3): getPublicBrowserEnv(), splitList(), withPublicBrowserEnv()

### Community 22 - "Community 22"
Cohesion: 1.0
Nodes (2): createAppScaffold(), ensureEmptyDirectory()

### Community 24 - "Community 24"
Cohesion: 0.67
Nodes (1): Fixture

## Knowledge Gaps
- **1 isolated node(s):** `Fixture`
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `Community 5`** (33 nodes): `Router`, `.allowedOrigins()`, `.filesystemPathToRoute()`, `.getHeaders()`, `.hasPageRoute()`, `.headers()`, `.isOriginAllowed()`, `.parseKVParams()`, `.parseParams()`, `.processRequest()`, `.readBodyBytes()`, `.resolveAllowedOrigin()`, `.resolvePageRoute()`, `.resolveRoutePattern()`, `.resolveWorkspacePath()`, `.routeToFilesystemPath()`, `.splitConfigList()`, `.validatePageRoute()`, `.validatePageRoutes()`, `.validateRoute()`, `.validateRoutes()`, `.validateSegmentPath()`, `Validate`, `.matchStatusCode()`, `.sanitizePropertyName()`, `.validateData()`, `.validateObject()`, `route-handler.js`, `schema-validator.js`, `.getParams()`, `.routePathFromPageSource()`, `.healthResponse()`, `.rejectDisallowedOrigin()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 14`** (7 nodes): `telemetry-alert-worker.js`, `TelemetryAlertWorker`, `.buildAlerts()`, `.constructor()`, `.fetchTelemetry()`, `.numberEnv()`, `.run()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 22`** (3 nodes): `createAppScaffold()`, `ensureEmptyDirectory()`, `app-scaffold.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Community 24`** (3 nodes): `createHelpers()`, `Fixture`, `tac.test.js`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `loadManifests()` connect `Community 3` to `Community 6`?**
  _High betweenness centrality (0.230) - this node is a cross-community bridge._
- **Why does `Tac` connect `Community 0` to `Community 12`, `Community 5`, `Community 7`?**
  _High betweenness centrality (0.205) - this node is a cross-community bridge._
- **Why does `resolvePageHandler()` connect `Community 6` to `Community 5`?**
  _High betweenness centrality (0.186) - this node is a cross-community bridge._
- **What connects `Fixture` to the rest of the system?**
  _1 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.06 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.07 - nodes in this community are weakly interconnected._
- **Should `Community 2` be split into smaller, more focused modules?**
  _Cohesion score 0.08 - nodes in this community are weakly interconnected._