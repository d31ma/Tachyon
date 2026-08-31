# Local feature reconciliation

## Release acceptance boundary

The reconciliation starts at released `v26.35.07` and current main `963031f`.
The feature inventory is the preserved local tree `9b1ff17`, not only the
changes selected by PR172. The old tag remains immutable. A new version may
publish only after every row below has behavioral evidence or an explicit
compatibility disposition, and the downloadable binaries pass the same gates.

Graphify provides dependency context; source snapshots and executable tests
are the authority for inclusion. Existing recovery branches and release
branches remain permanent references.

## Feature inventory

| Area | Acceptance criterion | Disposition |
| --- | --- | --- |
| Yon layers/relays/streams | Eight owned languages; six generator-capable streaming languages; no legacy interpreter fallback | Preserve released implementation and all hardening |
| Handler/source/process lifetime | Non-following owned snapshot, bounded processes, cache quota, runtime readiness, cancellation, first/second signals | Preserve released behavior; no replacement by older local implementations |
| Browser expressions | Await, assignment, `$event`, switch, arithmetic, nested lexical scope, scoped CSS, component naming | Preserve already shipped paths and extend regression coverage |
| Editing focus | Inputs without IDs, caret/selection, textarea and custom-element slot ancestry survive reactive updates | Preserve the connected editing control; verify native hosts and the actual website |
| Browser helpers | `this.tac` fetch/invalidate/precache/clearAssetCache/render and bounded retained publish/subscribe | Restore with bounded storage and lifecycle ownership |
| Persistence/decorators | `$` per-tab and `$$` cross-session persistence; @publish/@subscribe/@onMount | Restore with reload, instance isolation and teardown gates |
| Counted/declaration loops | Bounded increasing/decreasing counted loops, const/let iterable bindings, nested scope | Restore; reject malformed/nonterminating literal updates |
| HTML literals/bootstrap | Decode text references once; never reinterpret as markup/expressions; UTF-8 BOM; mobile viewport | Restore; retain tokenizer-decoded attributes without double decoding |
| Native targets | Trusted local WebView application, target-native compiled page companions, deterministic target precedence, per-route dispatch | Restore accepted local architecture with origin checks and owned process/snapshot guarantees |
| Application/browser metadata | Captured manifest and config, validated metadata/icons/window settings, deterministic native and browser output | Restore without allowing overlays to bypass validation |
| Request contracts | CHEX validates parameters/headers/body before handler execution; invalid schema/runtime fails before readiness | Restore with bounded supervised external validation and immutable schema capture |
| API documentation | Deterministic api.json and schema viewer reflect captured contracts; dynamic OPTIONS uses matched route | Restore with request-contract gate |
| CLI | preview builds/watches Tac+Yon; start runs existing output without rebuild/HMR; bundle --native stages one target | Restore while retaining compatible legacy commands and structured shutdown |
| Tests/support/release | Native per-platform execution, security, compatibility, signed provenance, installed-artifact acceptance | Update gates to actual architecture; do not downgrade tests to source-shape assertions |

## Explicit compatibility decisions

- Keep `serve`, `dev`, `native-bundle`, legacy `<if>/<else>/<for>` and bare
  iteration bindings as compatibility spellings. Removing a working spelling
  is not required to restore new capabilities. New examples use preview/start,
  bundle --native, logic, and declared loop bindings.
- `preview --static` preserves static artifact inspection without running Yon
  or rebuilding. Plain preview is the development command.
- Preserve the current framework-owned decorator-capable JavaScript runtime,
  pre-bind readiness checks, source snapshots and supervised command runner.
  Do not restore the older node default, warning-only readiness, ambient source
  reads, unbounded command capture or detached watchers.
- Preserve the released environment namespace: `TAC_DIST_PATH` selects built
  output; `YON_*` selects serving and handler runtime behavior. Do not revive
  removed `YON_DIST_PATH` or `TACHYON_*` variables from the old local tree.
- Native companions intentionally supersede browser WASM companions (ADR0019).
  Reject unsupported component-language/target combinations with migration
  guidance; never silently omit authored behavior.
- Preserve the accepted Yon ADR0017. Recovered native decisions receive
  ADR0018 and ADR0019; the old local streaming ADR is already covered by the
  current Yon decision and is not a competing new contract.
- Keep full-document HTML compatibility; new templates should be fragments
  and metadata should come from captured configuration. Do not bring back the
  older parser's panic on malformed leaf children.
- Persisted fetch caching is opt-in for eligible same-origin GET responses,
  with omitted credentials and no authorization/private/no-store responses.
  Enforce entry/response/age bounds; unavailable storage must not crash views.
  The unpublished local implementation's broad caching default is not safe.

## Completion evidence

Local functional qualification is complete. Publication is still pending;
this document is not a release sign-off.

The following records the initial local qualification on 2026-08-31 UTC.
The subsequent integration qualification below supersedes its outstanding
Linux/Windows checks and local binary identity.

- The complete canonical Rust gate passes: formatting, all-target/all-feature
  checking, warning-denying Clippy, 449 tests across 16 suites, warning-denying
  documentation, and dependency/license/source policy. The dependency check
  retains its allowed duplicate-version warnings. CLI phases 1-7 and all seven
  signal-lifecycle gates include production shutdown and a 60-second server soak.
- Fresh coverage passes its 80% floors: 86.53% lines, 85.84% functions and
  86.96% regions. All 442 instrumented tests pass; seven existing component
  tests are excluded only under `cfg(coverage)` and run in the canonical suite.
- Seven template regressions prove counted/declaration loops, invalid updates,
  single entity decoding, BOM handling, and mobile bootstrap metadata.
- Real CHEX integration and the compiled-CLI schema probe reject invalid body,
  headers, parameters and JSON; dynamic OPTIONS and the API viewer are served.
- Browser storage/runtime gates pass, including counted-loop DOM behavior,
  aggregate empty-loop bounds, native argument forwarding and delayed mount
  cancellation. Service-worker privacy tests cover cache hits and eviction,
  credentials, authorization, no-store/private/Vary and verified static assets.
- The input regression first reproduced one character (`C`) instead of
  `Customer`. The fixed runtime preserves the original connected input through
  typing, middle edits, backward selection, textarea input, component ancestry
  and custom-element slots. The real website gate covers home, docs and feature
  pages at 1440px and 390px (using the supported keyboard shortcut on touch),
  including asynchronous search results, spaces, rapid typing and Escape.
- A second regression reproduced late native acknowledgements overwriting
  newer typed characters. Per-field revisions now preserve optimistic input;
  deterministic browser ordering and the actual Android app both pass.
  Android types at 40 ms per character while a Kotlin setter waits 100 ms,
  then confirms the complete native field value, focus and DOM identity.
- Two-route Rust companion ABI execution proves state isolation, native OS
  access and publish callbacks. macOS accessibility/visual, Android 15 emulator
  and iOS 26.5 / iPhone SE simulator gates execute real native hosts and companions. They
  verify rendered interactions, retained input focus, native OS calls, publish
  callbacks and route isolation. Linux/Windows native GUI runner qualification
  remains outstanding; generated Windows C and Linux Rust compile locally.
- Website build-output and compiled-example suite: 36 tests, 663 assertions,
  zero failures. Website and release-probe JavaScript semantic type checks pass.
- Published-snippet verification executes 56 bundles and 53 HTTP method probes,
  including all eight layered server languages and five SSE examples, plus
  13 syntax and three JSON checks. Non-executable fragments and missing
  execution coverage are listed separately, never counted as compiled tests.
  Java/Kotlin/C#/Rust examples colocate their helper types in one Yon source;
  cross-file compilation is not claimed. Regression tests ensure those helpers
  do not become controller endpoints or change the controller stream protocol.
- `scripts/release/feature-smoke.mjs` passes against the reconciliation CLI;
  it proves client plans and production startup without recompilation or HMR.
- The exact CHEX v26.32.02 artifact is digest-pinned for all five release
  architectures and its real validation protocol was probed locally.
- Eight browser gates and all 39 compatibility-ledger claims pass. Workflow
  syntax passes `actionlint -shellcheck=''`; plain actionlint still reports
  five ShellCheck findings reproduced unchanged on the base commit.
- GitHub had no open issues when checked on 2026-08-31 UTC. Existing unrelated
  dependency PRs are unchanged.

All final executable/browser/native probes selected the same frozen local
development CLI, with SHA-256
`0a08ee6bedc01e6e0bbe63938a2f0745400d61b06db9a0055a3331e574dd17fb`,
verified unchanged before and after native qualification. Its embedded version
is still `26.35.07`; that identifies the unbumped development build, not a
replacement for the immutable published release. The canonical and coverage
gates independently built the same source tree. Local detailed reports are
retained under ignored `target/`; published artifacts need fresh evidence.

## Integration qualification (2026-08-31 UTC)

PR [179](https://github.com/d31ma/Tachyon/pull/179) merged into main at
`8a3016f5079bd45c6e56c85d3f40c235a9b1b0f9`. Its final head
`8a6e86bd11bc7e43065ecd2f0633d57f6180dbe5` passed all 18 jobs in
[the complete CI run](https://github.com/d31ma/Tachyon/actions/runs/33359303631).
This includes actual Linux GTK4 and Windows WebView2 navigation, native calls,
stylesheets and cleanup; macOS/iOS and Android UI; website typing; Rust gates
on all three desktop systems; compatibility, coverage, fuzzing and sanitizers.
No review conversations or open issues remained at the merge check.

The final local canonical gate passes 450 tests across 16 suites with no
failures or ignored tests, plus formatting, check, Clippy, rustdoc and dependency
policy. Production-source coverage passes the unchanged 80% floors: 86.51%
lines, 85.85% functions and 86.95% regions. The final frozen development CLI has
SHA-256 `d04a19eed574449cc1671adc8f391254864413f9d7ff63f176fcda34fd9414d9`.
That CLI includes Linux content-type and Windows local-response routing fixes;
the later CI corrections affect test harnesses only.

The Linux headless gate retains WebKit compositing with Mesa software rendering
and shared-memory buffers. Windows cleanup waits for owned WebView processes
under bounded deadlines. The scheduled-worker snapshot test waits for a
completed marker write instead of file creation; a deliberately delayed write
reproduces the original race. Existing behavioral assertions and deadlines are
retained. These are not platform exclusions or waived CI gates.

Release `26.36.01` is prepared from this merged main and requires a new
immutable tag.
Publication, downloaded-artifact verification, matched website deployment and
installed-CLI verification remain required; integration CI alone does not
establish those outcomes. The separate cybersecurity-review waiver below
remains explicit and does not change automated release qualification.

## Review disposition and operational limits

- Codebase Steward and focused native/website UX reviews passed. The final
  full run found one standalone Swift test omitted the shared JSON helper;
  its compile inputs were corrected without changing the production binary.
  The targeted test and complete suite then passed.
- The user waived the separate cybersecurity review for this run. Completed
  fixes and existing automated regression/CI gates remain intact. This waiver
  is not a cybersecurity sign-off or a permanent change to release CI policy.
- SRE review found no blocking operational regression, conditional on remote
  native CI and downloaded-artifact qualification. Native-call timeouts bound
  the caller's wait, not application-owned execution or rollback. Companions
  must stay responsive; hung native work can require an application relaunch.
- Earlier local CHEX startup and Apple helper timeout failures passed unchanged
  on subsequent runs, including the final full run; their causes are unconfirmed.
  One Android emulator boot-readiness installation failure passed on unchanged
  retry. These are retained as observed transients, not claimed root-cause fixes.
- Existing coarse-pointer search-button discoverability is deferred to a
  separate UX change; the website design is preserved. Simulator/emulator
  evidence does not establish physical-device support.

The release workflow now runs schema, native ABI, and template/CLI gates on
the downloaded staged executables, plus storage/runtime browser gates on the
downloaded Linux executable. Current-head release PR checks (including Linux/Windows
native GUI), review feedback, a new immutable version and publication evidence
are still required. Record the release SHA, artifact digests, matching website
deployment and installed CLI before claiming completion.
