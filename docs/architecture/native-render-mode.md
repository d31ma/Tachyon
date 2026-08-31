# Native application rendering

Status: platform web-view hosts, Native Host contract v3 (ADRs 0018 and 0019).

Every target uses the same client-rendered Tac application. The compiler does
not lower a separate widget tree, server-render native pages, or choose
per-subtree fallback. JavaScript/TypeScript components run in the platform web
engine. Page companions compile to their selected native target.

## Pipeline

1. Capture one immutable project snapshot and validate routes and configuration.
2. Compile bounded client render plans, modules, styles, and shared assets.
3. Select one native companion per route using the canonical target table.
4. Emit bounded per-route dispatch tables and target-native companion code.
5. Stage the platform web-view host and local bundle resources.
6. Build with the target SDK when packaging is requested, then atomically
   publish the complete artifact and accurate host manifest.

WKWebView serves macOS/iOS, WebKitGTK serves Linux, WebView2 serves Windows,
and Android WebView serves Android. Root-relative assets and canonical static
and dynamic routes resolve within the staged bundle.

Automatic companion member discovery currently exposes supported fields and
zero-argument public methods. A method taking arguments requires an explicit
native member table; the transport forwards only authored JSON arguments,
never an implicit DOM event. Unsupported automatic bindings must not be
mistaken for generated methods. Native methods return browser Promises.

## Security boundary

Only the main application frame at the exact local bundle origin may invoke
declared capabilities. Navigation checks and per-message origin/frame checks
are separate requirements. A canonical route in a request must belong to the
compiler-generated registry and match the active application page.

Native companions are trusted application code with the host's operating-system
privileges. They are not sandboxed plugins. Remote pages, subframes, arbitrary
file URLs, unknown routes, unknown operations, and undeclared members do not
inherit the bridge. User-originated strings never become executable script
without JSON encoding.

## Bundle and migration

Each target publishes beneath `dist/<target>/`, including when multiple
targets are built together. `tachyon.host.json` declares schema/bridge version
3, bundle rendering, the entry route/document, selected route companions,
window controls, and supported host capabilities. Artifact metadata no longer
claims that an absent Native UI v1 document was produced.

The former widget planner and WebAssembly companion contracts are superseded.
Use [ADR 0019](../adr/0019-companions-compile-for-their-target.md) for companion
migration and [RECONCILIATION.md](../RECONCILIATION.md) for release qualification.
`--render-mode` remains rejected: the renderer is an architectural contract,
not an undocumented compatibility switch.
