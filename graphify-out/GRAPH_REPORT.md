# Graph Report - TACHYON  (2026-05-15)

## Corpus Check
- 131 files · ~81,055 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1357 nodes · 2330 edges · 108 communities (87 shown, 21 thin omitted)
- Extraction: 84% EXTRACTED · 16% INFERRED · 0% AMBIGUOUS · INFERRED: 376 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `91f9fcfa`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

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
- [[_COMMUNITY_Community 20|Community 20]]
- [[_COMMUNITY_Community 21|Community 21]]
- [[_COMMUNITY_Community 22|Community 22]]
- [[_COMMUNITY_Community 23|Community 23]]
- [[_COMMUNITY_Community 24|Community 24]]
- [[_COMMUNITY_Community 25|Community 25]]
- [[_COMMUNITY_Community 26|Community 26]]
- [[_COMMUNITY_Community 27|Community 27]]
- [[_COMMUNITY_Community 28|Community 28]]
- [[_COMMUNITY_Community 29|Community 29]]
- [[_COMMUNITY_Community 30|Community 30]]
- [[_COMMUNITY_Community 32|Community 32]]
- [[_COMMUNITY_Community 33|Community 33]]
- [[_COMMUNITY_Community 34|Community 34]]
- [[_COMMUNITY_Community 35|Community 35]]
- [[_COMMUNITY_Community 36|Community 36]]
- [[_COMMUNITY_Community 37|Community 37]]
- [[_COMMUNITY_Community 38|Community 38]]
- [[_COMMUNITY_Community 39|Community 39]]
- [[_COMMUNITY_Community 40|Community 40]]
- [[_COMMUNITY_Community 41|Community 41]]
- [[_COMMUNITY_Community 42|Community 42]]
- [[_COMMUNITY_Community 43|Community 43]]
- [[_COMMUNITY_Community 44|Community 44]]
- [[_COMMUNITY_Community 46|Community 46]]
- [[_COMMUNITY_Community 47|Community 47]]
- [[_COMMUNITY_Community 48|Community 48]]
- [[_COMMUNITY_Community 49|Community 49]]
- [[_COMMUNITY_Community 50|Community 50]]
- [[_COMMUNITY_Community 51|Community 51]]
- [[_COMMUNITY_Community 52|Community 52]]
- [[_COMMUNITY_Community 55|Community 55]]
- [[_COMMUNITY_Community 56|Community 56]]
- [[_COMMUNITY_Community 58|Community 58]]
- [[_COMMUNITY_Community 59|Community 59]]
- [[_COMMUNITY_Community 60|Community 60]]
- [[_COMMUNITY_Community 61|Community 61]]
- [[_COMMUNITY_Community 63|Community 63]]
- [[_COMMUNITY_Community 64|Community 64]]
- [[_COMMUNITY_Community 65|Community 65]]
- [[_COMMUNITY_Community 66|Community 66]]
- [[_COMMUNITY_Community 68|Community 68]]
- [[_COMMUNITY_Community 69|Community 69]]
- [[_COMMUNITY_Community 70|Community 70]]
- [[_COMMUNITY_Community 71|Community 71]]
- [[_COMMUNITY_Community 73|Community 73]]
- [[_COMMUNITY_Community 74|Community 74]]
- [[_COMMUNITY_Community 75|Community 75]]
- [[_COMMUNITY_Community 76|Community 76]]
- [[_COMMUNITY_Community 77|Community 77]]
- [[_COMMUNITY_Community 78|Community 78]]
- [[_COMMUNITY_Community 80|Community 80]]
- [[_COMMUNITY_Community 81|Community 81]]
- [[_COMMUNITY_Community 104|Community 104]]
- [[_COMMUNITY_Community 105|Community 105]]
- [[_COMMUNITY_Community 106|Community 106]]
- [[_COMMUNITY_Community 107|Community 107]]

## God Nodes (most connected - your core abstractions)
1. `Compiler` - 76 edges
2. `Compiler` - 76 edges
3. `Yon` - 38 edges
4. `Yon` - 38 edges
5. `get()` - 36 edges
6. `set()` - 25 edges
7. `YonCompiledRunner` - 25 edges
8. `Router` - 24 edges
9. `handler()` - 21 edges
10. `runSelectiveBuild()` - 20 edges

## Surprising Connections (you probably didn't know these)
- `Tac Frontend Layer (Changelog)` --semantically_similar_to--> `Tac Frontend Layer`  [INFERRED] [semantically similar]
  CHANGELOG.md → README.md
- `Yon Backend/Runtime Layer (Changelog)` --semantically_similar_to--> `Yon Backend/Runtime Layer`  [INFERRED] [semantically similar]
  CHANGELOG.md → README.md
- `TypeScript to JavaScript+JSDoc Migration` --semantically_similar_to--> `Tac Companion Scripts`  [INFERRED] [semantically similar]
  CHANGELOG.md → README.md
- `Stage 3 Decorator Forms (@inject, @provide, @env, @onMount, @emit)` --semantically_similar_to--> `Decorator Form of Tac Helpers`  [INFERRED] [semantically similar]
  CHANGELOG.md → README.md
- `Automatic Prop-to-Field Binding` --semantically_similar_to--> `Automatic Prop-to-Field Binding`  [INFERRED] [semantically similar]
  CHANGELOG.md → README.md

## Hyperedges (group relationships)
- **Tac+Yon Framework Architecture Split (v2.0)** — changelog_tachyon_2_0, changelog_tac_frontend, changelog_yon_backend, readme_tachyon, readme_tac, readme_yon [EXTRACTED 1.00]
- **FYLO-Backed Data and Observability Surface** — readme_fylo_storage, readme_otel, fylo_index_fylo_panel, telemetry_index_telemetry_panel, users_index_users_panel [INFERRED 0.80]
- **Tac Template Conditional Rendering Pattern (<logic :if> / <loop :for>)** — fylo_index_fylo_panel, polyglot_index_polyglot_panel, inventory_index_inventory_panel, users_index_users_panel, diagnostics_index_diagnostics_panel, telemetry_index_telemetry_panel, showcase_index_showcase_panel [INFERRED 0.85]
- **Wasm Clicker Polyglot Pattern** — index_wasm_zig_clicker, index_wasm_go_clicker, index_wasm_clicker, index_wasm_assemblyscript_clicker, index_wasm_rust_clicker, index_wasm_c_clicker, wasm_clicker_pattern [INFERRED]
- **Dashboard Page Composition** — index_dashboard_page, index_stats_grid, index_panel_helpers, index_wasm_zig_clicker, index_wasm_go_clicker, index_wasm_clicker, index_wasm_assemblyscript_clicker, index_wasm_rust_clicker, index_wasm_c_clicker [INFERRED]
- **FYLO Database Ecosystem** — readme_db_structure, readme_fylo_system, readme_chex_schemas, readme_versioned_schema_layout, readme_seed_data, app_app_shell, index_stats_grid [INFERRED]

## Communities (108 total, 21 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.06
Nodes (4): getAvailablePort(), has(), Compiler, pathExists()

### Community 1 - "Community 1"
Cohesion: 0.07
Nodes (18): createBackendOnlyApp(), text(), writeExecutableHandler(), service, YonRequest, handler(), buildTestFactory(), TelemetryAlertWorker (+10 more)

### Community 2 - "Community 2"
Cohesion: 0.06
Nodes (29): appendEvents(), encryptionBanner(), errorMessage(), __fyloCollection(), __fyloFetch(), __fyloPostJson(), get(), loadCollections() (+21 more)

### Community 3 - "Community 3"
Cohesion: 0.05
Nodes (15): handler(), service, YonRequest, service, YonRequest, service, YonRequest, ItemService (+7 more)

### Community 4 - "Community 4"
Cohesion: 0.05
Nodes (38): createPublicBrowserEnvResponse(), createPublicBrowserEnvScript(), getPublicBrowserEnv(), splitList(), withPublicBrowserEnv(), bundleWatchEnabled, configureRoutes(), detectAppShape() (+30 more)

### Community 5 - "Community 5"
Cohesion: 0.04
Nodes (14): bundleEntrypoint, button, distComponentPath, layouts, modulePath, pageModulePath, prehydrateIndex, prehydrateModulePath (+6 more)

### Community 6 - "Community 6"
Cohesion: 0.05
Nodes (33): authFetch(), authHeaders(), decodeAnyValue(), describeEarlyExit(), extractPersistedSpans(), getResourceAttribute(), readTextStream(), waitForServer() (+25 more)

### Community 7 - "Community 7"
Cohesion: 0.08
Nodes (7): FyloTelemetryStore, SpanFactory, Telemetry, TelemetryConfig, TelemetrySanitizer, TelemetryStore, TraceContextParser

### Community 9 - "Community 9"
Cohesion: 0.12
Nodes (31): applySecurity(), buildCurl(), buildExecution(), buildRequestPayload(), defaultParameterValue(), defaultRequestBody(), ensureOperationState(), escapeHtml() (+23 more)

### Community 10 - "Community 10"
Cohesion: 0.16
Nodes (27): browserContentSecurityPolicy(), deleteDocument(), executeQuery(), FyloBrowser, fyloRoot(), getDocument(), getEncryptedFields(), isReadOnlyQuery() (+19 more)

### Community 11 - "Community 11"
Cohesion: 0.1
Nodes (20): localFirstFetch(), __ty_compiled_factory__, __ty_deleteCachedResponse(), TY_INTERNAL_FIELDS, __ty_isBrowserEnv(), __ty_module_imports__, __ty_openFetchCache(), __ty_readCachedResponse() (+12 more)

### Community 12 - "Community 12"
Cohesion: 0.08
Nodes (31): CLI Binary Rename (tach.* to tac.*/yon.*), OOP Companion Classes (export default class extends Tac), @d31ma/tachyon/decorators Package Export, Local-First fetch() with IndexedDB, FYLO-backed OpenTelemetry Storage, Polyglot Backend Handlers, Rationale: Framework Split into Tac+Yon in v2.0, Stage 3 Decorator Forms (@inject, @provide, @env, @onMount, @emit) (+23 more)

### Community 13 - "Community 13"
Cohesion: 0.15
Nodes (18): alloc(), call(), copy_bytes(), copyText(), dealloc(), init(), initTac(), output_len() (+10 more)

### Community 14 - "Community 14"
Cohesion: 0.1
Nodes (21): findEventTarget(), canHandleClientNavigation(), context, delegatedEvents, dispatchAction(), ensureDelegatedEvent(), handleDelegatedEvent(), layouts (+13 more)

### Community 15 - "Community 15"
Cohesion: 0.14
Nodes (17): main(), run(), startServer(), buildPrettyLine(), createLogger(), createWriteTarget(), needsQuoting(), normalizeConsoleArgs() (+9 more)

### Community 16 - "Community 16"
Cohesion: 0.11
Nodes (10): buildRouteOutput(), classifyChange(), normalizeRelative(), pathExists(), runBuild(), runSelectiveBuild(), runWithConcurrency(), startBundleWatcher() (+2 more)

### Community 17 - "Community 17"
Cohesion: 0.12
Nodes (3): jsonOk(), Router, configureRoutes()

### Community 18 - "Community 18"
Cohesion: 0.12
Nodes (26): App Shell Template (app.html), Imports Module Script, SPA Renderer Script, Logic If Directive, Refresh All Handler, Sidebar Navigation, Toggle Sidebar Handler, Toggle Theme Handler (+18 more)

### Community 19 - "Community 19"
Cohesion: 0.09
Nodes (8): service, YonRequest, service, service, service, LanguageRepository, LanguageService, handler()

### Community 21 - "Community 21"
Cohesion: 0.13
Nodes (4): FyloTelemetryRepository, service, OtlpValueDecoder, TelemetryService

### Community 22 - "Community 22"
Cohesion: 0.13
Nodes (3): FyloBrowser, env(), loadLazyComponent()

### Community 23 - "Community 23"
Cohesion: 0.15
Nodes (16): buildFyloBrowserPaths(), buildOperation(), buildSpec(), describeStatus(), docsContentSecurityPolicy(), docsScript(), docsStyles(), getDocsAssets() (+8 more)

### Community 25 - "Community 25"
Cohesion: 0.15
Nodes (5): decode(), basename(), basename(), isAssetRequest(), YonPhpRunner

### Community 26 - "Community 26"
Cohesion: 0.2
Nodes (7): addItem(), bindRefreshListener(), clearItems(), loadHealth(), loadingState(), loadItemsCount(), refresh()

### Community 27 - "Community 27"
Cohesion: 0.14
Nodes (7): decode(), runBundle(), expectInteractiveSurface(), expectNoBrowserErrors(), trackBrowserErrors(), waitForDashboardReady(), expect()

### Community 28 - "Community 28"
Cohesion: 0.2
Nodes (9): assertRun(), copyIfExists(), createTachyonApp(), run(), tachyonTarball(), uniqueName(), apps, proc (+1 more)

### Community 29 - "Community 29"
Cohesion: 0.19
Nodes (12): getAttribute(), cleanBooleanAttrs(), isSameNode(), resolveHandler(), syncAttributes(), syncFormControlState(), getAttribute(), canHandleClientNavigation() (+4 more)

### Community 30 - "Community 30"
Cohesion: 0.15
Nodes (7): ctx, method, r, request, seen, stored, TEMPLATE_PATH

### Community 32 - "Community 32"
Cohesion: 0.15
Nodes (13): `$` and `$$` Field Persistence, API Docs, code:bash (YON_OTEL_ENABLED=true), code:bash (bun test tests/integration/api-routes.test.js), code:bash (cd examples), code:bash (curl -i \), code:bash (cd examples), code:js (export default class extends Tac {) (+5 more)

### Community 33 - "Community 33"
Cohesion: 0.15
Nodes (12): code:bash (yon.init my-app), code:text (dist/), code:env (UPSTASH_REDIS_REST_URL=), Commands, Distributed Rate Limiting, Features, License, Operations (+4 more)

### Community 34 - "Community 34"
Cohesion: 0.21
Nodes (5): set(), pathExists(), resolvePreviewFile(), serveStaticPreviewRequest(), shouldTreatAsAsset()

### Community 35 - "Community 35"
Cohesion: 0.24
Nodes (5): canRunPlaywright(), decode(), resolveNodeCommand(), runCommand(), HandlerAdapter

### Community 36 - "Community 36"
Cohesion: 0.17
Nodes (11): [1.11.1] and earlier, [2.0.0], Added, Added, Breaking Changes, Changed, Changed, Changelog (+3 more)

### Community 37 - "Community 37"
Cohesion: 0.31
Nodes (11): parseFragment(), findLazyAncestor(), isInsideSlot(), loadLazyComponent(), observeLazyComponents(), patchBody(), patchSlot(), postPatch() (+3 more)

### Community 38 - "Community 38"
Cohesion: 0.31
Nodes (11): morphChildren(), dispatchAction(), findLazyAncestor(), isInsideSlot(), observeLazyComponents(), patchBody(), patchSlot(), postPatch() (+3 more)

### Community 40 - "Community 40"
Cohesion: 0.2
Nodes (11): Automatic Prop-to-Field Binding, Scoped Component CSS (@scope wrapper), SessionStorage Persistence for $-prefixed Fields, Clicker Component (persisted $clicks), Clicker UI Component (session-scoped clicks), Automatic Prop-to-Field Binding, Rationale: Prop Value Wins Over Field Default, Reactive Companion Fields (+3 more)

### Community 42 - "Community 42"
Cohesion: 0.2
Nodes (9): code:text (db/), code:text (db/schemas/<collection>/), code:text (db/seed/<collection>/<document-id>.json), code:bash (fylo.admin rebuild <collection> --root db/collections), collections/, db/, Layout, schemas/ (+1 more)

### Community 44 - "Community 44"
Cohesion: 0.31
Nodes (4): call(), init(), readClicks(), writeState()

### Community 46 - "Community 46"
Cohesion: 0.32
Nodes (3): applyTheme(), refreshAll(), toggleTheme()

### Community 47 - "Community 47"
Cohesion: 0.25
Nodes (8): Browser Environment Variables, code:js (export default class extends Tac {), code:js (export default class extends Tac {), code:js (export default class extends Tac {), code:bash (TAC_PUBLIC_ENV=PUBLIC_API_BASE_URL,PUBLIC_SENTRY_DSN), Prop Auto-Binding, Reactive Fields, Tac Companion Scripts

### Community 49 - "Community 49"
Cohesion: 0.25
Nodes (8): Browser Env Allowlisting (TAC_PUBLIC_ENV), Inventory CRUD Lab Panel, Yon Backend Routing (server/routes), Tachyon Configuration System (YON_*, TAC_*, FYLO_* env vars), Browser Env Allowlisting (TAC_PUBLIC_ENV), MVC Backend Architecture (routes->services->repositories), Rationale: MVC Backend Dependency Direction, Rationale: No Secure Browser Secrets Boundary

### Community 50 - "Community 50"
Cohesion: 0.29
Nodes (7): Backend Routing, code:ts (import ItemService from '../../../../services/item-service.t), code:js (export default class GET {), code:json ({), code:rust (use crate::yon_json::JsonValue;), code:json ({), code:text (server/)

### Community 55 - "Community 55"
Cohesion: 0.33
Nodes (6): code:text (browser/), code:env (YON_PORT=8000), code:bash (bun -e "console.log(await Bun.password.hash('user:pass'))"), code:env (FYLO_ROOT=db/collections), code:bash (cd examples), Scaffold Layout

### Community 56 - "Community 56"
Cohesion: 0.47
Nodes (5): createAppScaffold(), ensureEmptyDirectory(), createAppScaffold(), ensureEmptyDirectory(), files

### Community 59 - "Community 59"
Cohesion: 0.7
Nodes (4): buildFingerprint(), pathFingerprint(), runFreshBundleBuild(), startPreviewBundleWatcher()

### Community 60 - "Community 60"
Cohesion: 0.4
Nodes (5): code:ts (/// <reference types="@d31ma/tachyon/globals" />), code:js (import tachyonGlobals from '@d31ma/tachyon/eslint-globals'), code:js (export default class extends Tac {), code:js (import { inject, provide, env, onMount, emit } from '@d31ma/), Decorator Form

### Community 61 - "Community 61"
Cohesion: 0.4
Nodes (5): code:html (<!-- browser/components/clicker/index.html -->), code:text (browser/components/clicker/), code:text (browser/components/clicker/), code:json ({), Wasm Companions

### Community 63 - "Community 63"
Cohesion: 0.5
Nodes (3): dart_language_service.dart, DartLanguageService, DartLanguageService

### Community 64 - "Community 64"
Cohesion: 0.5
Nodes (4): code:html (<!-- browser/pages/index.html -->), code:js (// browser/pages/index.js), code:js (export default class extends Tac {}), Tac Templates

### Community 65 - "Community 65"
Cohesion: 0.5
Nodes (4): Distributed Rate Limiting (Upstash Redis), Production Security Hardening, Distributed Rate Limiting (Upstash Redis), Security Features (Hashed Basic Auth, CSP, HSTS, JWT)

### Community 66 - "Community 66"
Cohesion: 0.5
Nodes (4): Rationale: Language Compilers Are Optional, Rationale: Wasm Adapter Encapsulates DOM Access, tac-wasm-json@1 ABI, Wasm Companions

### Community 74 - "Community 74"
Cohesion: 0.67
Nodes (3): code:bash (bun add @d31ma/tachyon), code:ini (# ~/.npmrc), Install

### Community 75 - "Community 75"
Cohesion: 0.67
Nodes (3): code:text (browser/pages/), code:text (browser/components/), Frontend Routing

### Community 76 - "Community 76"
Cohesion: 0.67
Nodes (3): CHANGELOG.md Document, Keep a Changelog Format, Semantic Versioning 2.0.0

### Community 77 - "Community 77"
Cohesion: 0.67
Nodes (3): Build Manifests Moved to src/shared/manifests/, Rationale: Separate Generated Artifacts from Runtime Source, Source File Reorganization (src/server/ subdirectories)

### Community 78 - "Community 78"
Cohesion: 0.67
Nodes (3): Generated OpenAPI 3.1 Docs and Self-Hosted API Docs UI, Platform Surfaces Panel, OpenAPI 3.1 Docs and /api-docs UI

## Knowledge Gaps
- **189 isolated node(s):** `allocatedTestPorts`, `routeTestCases`, `languageRoutes`, `handler`, `requestSpan` (+184 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **21 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `get()` connect `Community 2` to `Community 0`, `Community 34`, `Community 3`, `Community 4`, `Community 38`, `Community 7`, `Community 9`, `Community 10`, `Community 11`, `Community 48`, `Community 17`, `Community 22`, `Community 24`, `Community 25`, `Community 27`, `Community 29`?**
  _High betweenness centrality (0.199) - this node is a cross-community bridge._
- **Why does `set()` connect `Community 34` to `Community 0`, `Community 1`, `Community 2`, `Community 3`, `Community 4`, `Community 7`, `Community 9`, `Community 10`, `Community 43`, `Community 48`, `Community 17`, `Community 51`, `Community 22`, `Community 23`, `Community 24`, `Community 27`?**
  _High betweenness centrality (0.077) - this node is a cross-community bridge._
- **Why does `Compiler` connect `Community 0` to `Community 16`, `Community 17`, `Community 4`, `Community 1`?**
  _High betweenness centrality (0.066) - this node is a cross-community bridge._
- **Are the 33 inferred relationships involving `get()` (e.g. with `trackBrowserErrors()` and `.request_id()`) actually correct?**
  _`get()` has 33 INFERRED edges - model-reasoned connections that need verification._
- **What connects `allocatedTestPorts`, `routeTestCases`, `languageRoutes` to the rest of the system?**
  _189 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.06 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.07 - nodes in this community are weakly interconnected._