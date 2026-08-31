use crate::external_command::{ToolError, run as run_tool};
use crate::failure::diagnostic;
use crate::failure::source_span;
use crate::template::{
    ClientViewRenderer, ComponentDefinition, ComponentRegistry, SCOPE_ATTRIBUTE,
    TAC_CLIENT_RUNTIME, TemplateFrontend, client_route_context,
};
use crate::{CompanionKind, Failure, Project, ProjectDiscovery};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

const HEX: &[u8; 16] = b"0123456789abcdef";
const BUILD_STATE_VERSION: u8 = 4;
/// Cross-document view transitions, opted into by every generated page.
///
/// This is the platform's own answer to the transition half of a single-page
/// application, and it costs no JavaScript at all.
/// Largest component stylesheet the compiler will bundle.
/// Oldest TypeScript that accepts the emit flags `tac.ts` needs.
///
/// `--ignoreConfig` first appears in TypeScript 6; 5.6 and 5.9 reject it.
const MINIMUM_TYPESCRIPT_MAJOR: u32 = 6;
const MAX_COMPONENT_STYLE_BYTES: usize = 1_048_576;
const MAX_SHARED_ASSET_BYTES: u64 = 16 * 1_048_576;
const MAX_SHARED_ASSETS: usize = 4_096;
const MAX_SHARED_ASSET_TOTAL_BYTES: u64 = 64 * 1_048_576;
const MAX_BUILD_CONFIG_BYTES: u64 = 1_048_576;
const TOOL_OUTPUT_BYTES: usize = 64 * 1_024;
const TYPESCRIPT_DEADLINE: Duration = Duration::from_secs(30);
const POST_BUNDLE_RUNNER: &str = r"import { pathToFileURL } from 'node:url'
const source = process.env.TAC_CONFIG
const root = process.env.TAC_STAGE
const target = process.env.TAC_TARGET
if (!source || !root || !target) throw new Error('missing Tachyon hook context')
const loaded = await import(`${pathToFileURL(source).href}?tachyon=${Date.now()}`)
const hook = loaded.postBundle ?? loaded.default?.postBundle
if (hook !== undefined && typeof hook !== 'function') throw new Error('postBundle must be a function')
if (hook) await hook({ distRoot: root, targets: [target], targetRoots: { [target]: root } })
";
const NAVIGATION_STYLESHEET: &str = "@view-transition { navigation: auto; }\n\
tachyon-component, tachyon-island { display: block; }\n";
/// Reference to the navigation stylesheet.
const NAVIGATION_LINK: &str =
    r#"<link rel="stylesheet" href="/.tachyon/navigation.css" data-tachyon-runtime>"#;
/// Link to the bundled component stylesheet, emitted only when one exists.
const COMPONENT_STYLE_LINK: &str =
    r#"<link rel="stylesheet" href="/.tachyon/components.css" data-tachyon-runtime>"#;
/// Speculation rules that prefetch same-origin routes before they are needed.
///
/// This is the platform's answer to the instant-navigation half. The payload is
/// JSON, not executable script, and `\'inline-speculation-rules\'` is the CSP
/// keyword defined for exactly this case.
const SPECULATION_RULES: &str = r#"<script type="speculationrules" data-tachyon-runtime>
{"prefetch":[{"where":{"href_matches":"/*"},"eagerness":"moderate"}]}
</script>"#;
/// Offline cache, served from the root so its scope covers every page.
///
/// A service worker only controls pages at or below its own path, so this
/// cannot live under `.tachyon/` with the other generated assets.
///
/// The build digest is embedded in the body rather than passed as `?v=`,
/// because the browser already re-installs a worker whose script bytes differ.
/// That is the platform's own update trigger, so no cache-busting query is
/// needed and the registration URL stays stable.
const SERVICE_WORKER: &str = r"const VERSION = '__VERSION__'
const CACHE = 'tachyon-static-' + VERSION
const PREFIX = 'tachyon-static-'
// Policies the project declared by path in tac.config.js, compiled to anchored
// expressions at build time so the worker needs no glob parser.
const RULES = (__CACHE_RULES__).map((rule) => ({ ...rule, match: new RegExp(rule.pattern) }))
// Only byte-verified build outputs may bootstrap anonymously from a browser's
// ordinary document/module requests. An extension is not a privacy boundary.
const ASSETS = __PUBLIC_ASSETS__
const MAX_ENTRIES = 256
const MAX_BYTES = 32 * 1024 * 1024
const MAX_AGE = 24 * 60 * 60 * 1000
let writes = Promise.resolve()
let pendingWrites = 0

const bounded = async (work) => {
  let timer
  try { return await Promise.race([work, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('cache unavailable')), 2000) })]) }
  finally { clearTimeout(timer) }
}
const publicResponse = (response) => response && response.ok && !response.redirected
  && response.type !== 'opaque' && response.type !== 'opaqueredirect'
  && !/(?:^|,)\s*(?:private|no-store)\b/i.test(response.headers.get('cache-control') || '')
  && !response.headers.has('vary')

async function bodyBytes(response, limit) {
  const reader = response.body?.getReader()
  if (!reader) return new Uint8Array()
  const chunks = []
  let length = 0
  const deadline = Date.now() + 2000
  try {
    while (true) {
      if (Date.now() >= deadline) throw new Error('cache body deadline')
      const { value, done } = await bounded(reader.read())
      if (done) break
      length += value.byteLength
      if (length > limit) throw new Error('cache body limit')
      chunks.push(value)
    }
  } finally { void reader.cancel().catch(() => {}) }
  const bytes = new Uint8Array(length)
  let offset = 0
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength }
  return bytes
}
async function matchesAsset(bytes, asset) {
  if (bytes.byteLength !== asset.bytes) return false
  const digest = await bounded(crypto.subtle.digest('SHA-256', bytes))
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('') === asset.sha256
}
async function evict(request) {
  try { const cache = await bounded(caches.open(CACHE)); await bounded(cache.delete(request)) } catch {}
}

self.addEventListener('install', (event) => { event.waitUntil(self.skipWaiting()) })

self.addEventListener('activate', (event) => {
  event.waitUntil((async () => {
    // Assets a previous build referenced expire with their cache.
    for (const name of await caches.keys())
      if (name.startsWith(PREFIX) && name !== CACHE) await caches.delete(name)
    await self.clients.claim()
  })())
})

self.addEventListener('fetch', (event) => {
  const request = event.request
  const url = new URL(request.url)
  // Cross-origin requests and the live-reload channel are left alone.
  if (url.origin !== self.location.origin) return
  if (url.pathname.startsWith('/.tachyon/live-reload') || url.pathname === '/.tachyon/hot') return

  if (request.method !== 'GET' && request.method !== 'HEAD') {
    event.respondWith(writeThrough(request, url))
    return
  }

  if (request.method === 'HEAD') return

  const rule = RULES.find((candidate) => candidate.match.test(url.pathname))
  const asset = !url.search && Object.hasOwn(ASSETS, url.pathname) ? ASSETS[url.pathname] : null
  // This decision precedes lookup as CacheStorage does not key by credentials.
  if (request.cache === 'no-store' || request.headers.has('authorization')
      || request.headers.has('range') || rule?.policy === 'no-store'
      || (!asset && (!rule || request.credentials !== 'omit'))) {
    event.respondWith((async () => { await evict(request); return fetch(request) })())
    return
  }
  const cacheFirst = rule ? rule.policy === 'cache-first' : request.mode !== 'navigate'
  event.respondWith(cacheFirst ? fromCache(request, asset) : fromNetwork(request, asset))
})

async function writeThrough(request, url) {
  const response = await fetch(request)
  if (!response.ok) return response
  try {
    const cache = await bounded(caches.open(CACHE))
    for (const entry of await bounded(cache.keys())) {
      const path = new URL(entry.url).pathname
      if (path === url.pathname || url.pathname.startsWith(path + '/')) await bounded(cache.delete(entry))
    }
  } catch {}
  return response
}

async function cacheRead(request, asset) {
  try {
    const cache = await bounded(caches.open(CACHE))
    const response = await bounded(cache.match(request))
    if (!response) return null
    const created = Number(response.headers.get('x-tachyon-cached-at'))
    if (!publicResponse(response) || (!asset && (!created || Date.now() - created > MAX_AGE))) {
      await bounded(cache.delete(request)); return null
    }
    if (asset && !await matchesAsset(await bodyBytes(response.clone(), 4 * 1024 * 1024), asset)) {
      await bounded(cache.delete(request)); return null
    }
    return response
  } catch { return null }
}

async function cacheWrite(request, response, asset) {
  if (!publicResponse(response)) { await evict(request); return }
  if (pendingWrites >= 16) return
  pendingWrites += 1
  try {
    const bytes = await bodyBytes(response.clone(), asset ? 4 * 1024 * 1024 : 256 * 1024)
    if (asset && !await matchesAsset(bytes, asset)) { await evict(request); return }
    const headers = new Headers(response.headers)
    headers.delete('content-encoding')
    headers.set('content-length', String(bytes.byteLength))
    headers.set('x-tachyon-cache-bytes', String(bytes.byteLength))
    headers.set('x-tachyon-cached-at', String(Date.now()))
    const stored = new Response(bytes, { status: response.status, statusText: response.statusText, headers })
    writes = writes.catch(() => {}).then(async () => {
      const cache = await bounded(caches.open(CACHE))
      const entries = await bounded(cache.keys())
      let size = bytes.byteLength
      for (const entry of entries) {
        const existing = await bounded(cache.match(entry))
        const known = ASSETS[new URL(entry.url).pathname]
        // Runtime-prewarmed build assets have a known size even without metadata.
        size += Number(existing?.headers.get('x-tachyon-cache-bytes')) || known?.bytes || MAX_BYTES
      }
      if (entries.length >= MAX_ENTRIES || size > MAX_BYTES)
        for (const entry of entries) await bounded(cache.delete(entry))
      await bounded(cache.put(request, stored))
    })
    await writes
  } catch { await evict(request) }
  finally { pendingWrites -= 1 }
}

async function fromCache(request, asset) {
  return (await cacheRead(request, asset)) || fromNetwork(request, asset)
}

async function fromNetwork(request, asset) {
  let response
  try {
    response = await fetch(new Request(request, { cache: 'no-store', ...(asset ? { credentials: 'omit' } : {}) }))
  } catch (error) {
    if (request.signal.aborted) throw error
    const cached = await cacheRead(request, asset)
    if (cached) return cached
    throw error
  }
  // Optional storage must never hold a successful network response indefinitely.
  await bounded(cacheWrite(request, response, asset)).catch(() => {})
  return response
}
";

/// Registers the offline cache, except where it would fight live reload.
const SERVICE_WORKER_REGISTRATION: &str = r"// A loopback host is the development server, where a stale cached asset
// would fight live reload, so the offline cache is left unregistered there
// and any worker a previous run installed is removed.
const host = location.hostname.toLowerCase().replace(/^\[|\]$/g, '')
const loopback = host === 'localhost'
  || host.endsWith('.localhost')
  || host === '::1'
  || /^127(?:\.\d{1,3}){3}$/.test(host)

if ('serviceWorker' in navigator) {
  if (loopback || typeof globalThis.__tachyonNativeHostCall === 'function')
    navigator.serviceWorker.getRegistrations()
      .then((registrations) => registrations.forEach((registration) => registration.unregister()))
      .catch(() => {})
  else
    navigator.serviceWorker.register('/tachyon-sw.js').catch(() => {})
}
";

/// Tag registering the offline cache, injected into every page.
const SERVICE_WORKER_LINK: &str =
    r#"<script type="module" src="/.tachyon/register-sw.js" data-tachyon-runtime></script>"#;

/// Compatibility entrypoint retained for applications and release checks that
/// address the former SPA runtime directly. Navigation itself is now handled
/// by browser primitives, while this hook preserves the public rerender name.
const COMPATIBILITY_SPA_RUNTIME: &str = r"window.__tc_rerender ??= async () => {
  window.dispatchEvent(new CustomEvent('tachyon:rerender'))
}
";

/// Options for one deterministic web build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildOptions {
    /// Project-relative output directory.
    pub output_directory: PathBuf,
    /// Whether unchanged handler-free routes may reuse verified artifacts.
    pub incremental: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            output_directory: PathBuf::from("dist"),
            incremental: true,
        }
    }
}

/// Evidence returned by a successful web build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildResult {
    output_directory: PathBuf,
    route_count: usize,
    sha256: String,
    compiled_routes: usize,
    reused_routes: usize,
}

impl BuildResult {
    /// Returns the canonical published output directory.
    #[must_use]
    pub fn output_directory(&self) -> &Path {
        &self.output_directory
    }

    /// Returns the number of compiled page routes.
    #[must_use]
    pub const fn route_count(&self) -> usize {
        self.route_count
    }

    /// Returns a deterministic digest over all output paths and bytes.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Returns the number of routes compiled during this invocation.
    #[must_use]
    pub const fn compiled_routes(&self) -> usize {
        self.compiled_routes
    }

    /// Returns the number of verified route artifacts reused from the prior build.
    #[must_use]
    pub const fn reused_routes(&self) -> usize {
        self.reused_routes
    }
}

/// Compiles validated Tachyon HTML sources into deterministic static output.
#[derive(Clone, Copy, Debug, Default)]
pub struct WebCompiler;

impl WebCompiler {
    /// Builds a Tachyon project and publishes its output as one completed set.
    ///
    /// # Errors
    ///
    /// Returns diagnostics when discovery, parsing, output validation, staging,
    /// or publication fails. A failed build preserves the prior output.
    pub fn build(
        project_root: impl AsRef<Path>,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure> {
        let project = ProjectDiscovery::discover(project_root)?;
        Self::build_project(&project, options)
    }

    /// Builds from an immutable project snapshot previously returned by discovery.
    ///
    /// # Errors
    ///
    /// Returns deterministic compilation or publication diagnostics.
    pub fn build_project(
        project: &Project,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                Failure::one(diagnostic(
                    1201,
                    format!("Cannot start the build runtime: {error}"),
                    None,
                    None,
                ))
            })?;
        runtime.block_on(Self::build_project_async(project, options))
    }

    /// Builds the client-rendered Tac views in a Tachyon project.
    ///
    /// # Errors
    ///
    /// Returns deterministic diagnostics and preserves the previous output on failure.
    #[allow(clippy::too_many_lines)]
    pub async fn build_async(
        project_root: impl AsRef<Path>,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure> {
        let project = ProjectDiscovery::discover(project_root)?;
        Self::build_project_async(&project, options).await
    }

    /// Asynchronously builds from one immutable discovery snapshot.
    ///
    /// # Errors
    ///
    /// Returns deterministic compilation or publication diagnostics.
    #[allow(clippy::too_many_lines)]
    pub async fn build_project_async(
        project: &Project,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure> {
        Self::build_project_with_target(project, options, None).await
    }

    pub(crate) async fn build_project_for_native(
        project: &Project,
        options: &BuildOptions,
        target: tachyon_contracts::NativeTarget,
    ) -> Result<BuildResult, Failure> {
        Self::build_project_with_target(project, options, Some(target)).await
    }

    #[allow(clippy::too_many_lines)]
    async fn build_project_with_target(
        project: &Project,
        options: &BuildOptions,
        native_target: Option<tachyon_contracts::NativeTarget>,
    ) -> Result<BuildResult, Failure> {
        let output_directory = resolve_output_path(project.root(), &options.output_directory)?;
        let snapshot_root = project.snapshot_root();
        let components = ComponentRegistry::discover(snapshot_root)?;
        let component_styles = collect_component_styles(&components)?;
        let build_config_digest = build_config_digest(snapshot_root)?;
        // Read once per build and baked into the worker, so the policy a page
        // gets is the policy that was reviewed rather than one fetched later.
        let cache_rules = crate::native::cache_rules(snapshot_root).await?;
        let cache_rules_literal = serde_json::to_string(&cache_rules).map_err(|error| {
            Failure::one(diagnostic(
                1502,
                format!("Cannot serialise the declared cache rules: {error}"),
                Some(String::from("Report this as a Tachyon bug.")),
                None,
            ))
        })?;
        let component_names = components.names();
        let mut programs = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for route in project.route_graph().routes() {
            let (Some(bytes), Some(source)) = (route.view_bytes(), route.source_path()) else {
                continue;
            };

            validate_companion_target(route, native_target)?;

            let template_source = read_template_source(bytes, source)?;
            let (template_source, inline_state) =
                match strip_page_state_scripts(&template_source, source) {
                    Ok(value) => value,
                    Err(failure) => {
                        diagnostics.extend_from_slice(failure.diagnostics());
                        (mask_script_blocks(&template_source), String::new())
                    }
                };
            let mut page_scope = parse_page_state(&inline_state);
            let page_module = route.companions().iter().find(|companion| {
                matches!(
                    companion.kind,
                    CompanionKind::ClientModule | CompanionKind::TypeScriptModule
                )
            });
            if let Some(companion) = page_module
                && let Ok(source) = std::str::from_utf8(companion.bytes())
            {
                page_scope.extend(parse_page_class_fields(source));
            }
            match TemplateFrontend::compile(&template_source, source, &component_names) {
                Ok(program) => {
                    programs.insert(
                        String::from(route.route()),
                        RouteProgram {
                            program,
                            page_scope,
                            has_page_module: page_module.is_some(),
                            inline_state,
                        },
                    );
                }
                Err(failure) => diagnostics.extend_from_slice(failure.diagnostics()),
            }
        }
        sort_diagnostics(&mut diagnostics);
        if !diagnostics.is_empty() {
            return Err(Failure::new(diagnostics));
        }
        let has_tac_routes = !programs.is_empty();

        let previous = if options.incremental {
            load_build_state(&output_directory)
        } else {
            None
        };
        let mut files = Vec::new();
        let mut next_state = BuildState::default();
        let manifest = project.route_graph().manifest();
        let page_metadata = crate::native::page_metadata(snapshot_root).await?;
        // Declared once and loaded by every document, rather than imported by
        // a companion per route whose whole body was that import.
        let browser_scripts = crate::native::browser_scripts(snapshot_root).await?;
        let browser_styles = crate::native::browser_styles(snapshot_root).await?;
        // One declaration behind the favicon, the install prompt and the
        // native application icon: they were always the same artwork.
        let manifest_head = crate::native::manifest_head(snapshot_root)?;

        let mut all_islands = BTreeSet::new();
        let mut compiled_routes = 0;
        let mut reused_routes = 0;

        for route in project.route_graph().routes() {
            let Some(route_program) = programs.get(route.route()) else {
                continue;
            };
            let program = &route_program.program;
            // A dynamic route publishes one pattern template below its authored
            // `_parameter` path. Servers match concrete URL segments to that
            // template; no concrete parameter value is invented at build time.
            let Some(output) = route.template_output_path() else {
                continue;
            };
            let output_portable = portable_path(&output);
            let key = route_key(route.route());
            // Companions are read before the reuse decision so that changing a
            // stylesheet invalidates its route, and are emitted on both paths
            // so a reused route never loses them.
            let route_directory = output.parent().map(Path::to_path_buf).unwrap_or_default();
            let mut companion_files = Vec::new();
            let mut companion_digest = Sha256::new();
            if let Some(digest) = &build_config_digest {
                companion_digest.update(digest.as_bytes());
                companion_digest.update([0]);
            }
            for companion in route.companions() {
                // Native source is compiled by its platform host, never emitted
                // into a browser-accessible bundle.
                if matches!(companion.kind, CompanionKind::Native(_)) {
                    continue;
                }
                let authored = companion.bytes().to_vec();
                let source = if matches!(
                    companion.kind,
                    CompanionKind::ClientModule | CompanionKind::TypeScriptModule
                ) {
                    prepare_component_script(
                        authored,
                        &route_program.inline_state,
                        &companion.source_path,
                    )?
                } else {
                    authored
                };
                // A TypeScript companion is emitted through the TypeScript
                // compiler itself, so its semantics are the reference
                // semantics rather than a reimplementation of them.
                let bytes = if companion.kind == CompanionKind::TypeScriptModule {
                    let snapshot_source = snapshot_root.join(&companion.source_path);
                    transpile_typescript(
                        snapshot_root,
                        &snapshot_source,
                        &companion.source_path,
                        &source,
                    )
                    .await?
                } else {
                    source.clone()
                };
                let bytes = rewrite_client_shared_imports(
                    snapshot_root,
                    &snapshot_root.join(&companion.source_path),
                    bytes,
                );
                let relative = route_directory.join(companion.kind.output_name());
                companion_digest.update(portable_path(&relative).as_bytes());
                companion_digest.update([0]);
                companion_digest.update(&source);
                companion_digest.update([0]);
                companion_files.push((relative, bytes));
            }
            let companion_sha = hex_digest(companion_digest.finalize());
            let input_sha = route_input_sha(
                route.view_bytes().unwrap_or_else(|| unreachable!()),
                components.digest(),
                &companion_sha,
            );
            let prior = previous
                .as_ref()
                .and_then(|state| state.routes.get(&key))
                .filter(|state| {
                    route.handlers().is_empty()
                        && state.input_sha == input_sha
                        && verify_artifacts(&output_directory, &state.artifacts)
                });
            files.extend(companion_files.iter().cloned());
            let route_state;
            if let Some(prior) = prior {
                for artifact in prior.artifacts.keys() {
                    files.push((
                        PathBuf::from(artifact),
                        output_io(fs::read(output_directory.join(artifact)), &output_directory)?,
                    ));
                }
                all_islands.extend(prior.islands.iter().cloned());
                route_state = prior.clone();
                reused_routes += 1;
            } else {
                let mut render_scope = client_route_context();
                render_scope.extend(route_program.page_scope.clone());
                let module_href = format!("/{}", portable_path(&route_directory.join("client.js")));
                let rendered = ClientViewRenderer::new(&components).render(
                    program,
                    &output_portable,
                    route_program
                        .has_page_module
                        .then_some(module_href.as_str()),
                    &render_scope,
                    route.route(),
                )?;
                let rendered_html = rendered.html;
                let source_map = rendered.source_map;
                let route_islands = rendered.components;
                let page_bindings = rendered.page_bindings;
                let ir_path = PathBuf::from(format!(".tachyon/view-ir/{key}.json"));
                let map_path = PathBuf::from(format!(".tachyon/source-maps/{key}.map.json"));
                // Collect the delegated event types this route binds. The
                // marker is emitted literally, so the rendered document is the
                // authoritative source.
                if page_bindings
                    && !route
                        .companions()
                        .iter()
                        .any(|companion| companion.kind != CompanionKind::Style)
                {
                    return Err(Failure::one(diagnostic(
                        1306,
                        format!(
                            "Route '{}' binds on:<event> but has no client module.",
                            route.route()
                        ),
                        Some(String::from(
                            "Add a colocated tac.js or tac.ts exporting the bound handlers.",
                        )),
                        None,
                    )));
                }
                // Every page opts into platform navigation: prefetch for
                // instant loads, view transitions for smooth ones. Both
                // degrade silently where unsupported.
                let mut html = rendered_html;
                // What a `tac.html` no longer carries: the head is written
                // here, from the route's entry in the configuration module.
                if let Some(declared) = page_metadata.get(route.route()) {
                    html = inject_before(&html, "</head>", &render_page_metadata(declared));
                    if let Some(lang) = &declared.lang {
                        html = html.replacen(
                            "<html lang=\"en\">",
                            &format!("<html lang=\"{}\">", html_attribute_escape(lang)),
                            1,
                        );
                    }
                }
                if !manifest_head.is_empty() {
                    html = inject_before(&html, "</head>", &manifest_head);
                }
                for source in &browser_styles {
                    html = inject_before(
                        &html,
                        "</head>",
                        &format!(
                            r#"<link rel="stylesheet" href="{}" data-tachyon-runtime>"#,
                            html_attribute_escape(source)
                        ),
                    );
                }
                for source in &browser_scripts {
                    html = inject_before(
                        &html,
                        "</head>",
                        &format!(
                            r#"<script type="module" src="{}" data-tachyon-runtime></script>"#,
                            html_attribute_escape(source)
                        ),
                    );
                }
                html = inject_before(&html, "</head>", NAVIGATION_LINK);
                if !component_styles.is_empty() {
                    html = inject_before(&html, "</head>", COMPONENT_STYLE_LINK);
                }
                html = inject_before(&html, "</head>", SPECULATION_RULES);
                html = inject_before(&html, "</body>", SERVICE_WORKER_LINK);
                for (companion, (relative, _)) in route
                    .companions()
                    .iter()
                    .filter(|companion| !matches!(companion.kind, CompanionKind::Native(_)))
                    .zip(&companion_files)
                {
                    let href = format!("/{}", portable_path(relative));
                    html = match companion.kind {
                        CompanionKind::Style => inject_before(
                            &html,
                            "</head>",
                            &format!(
                                r#"<link rel="stylesheet" href="{href}" data-tachyon-runtime>"#
                            ),
                        ),
                        CompanionKind::ClientModule
                        | CompanionKind::TypeScriptModule
                        | CompanionKind::Native(_) => html,
                    };
                }
                let html_bytes = html.into_bytes();
                let ir_bytes = pretty_json(&program.view_ir(), "View IR v1")?;
                let map_bytes = pretty_json(&source_map, "View Source Map v1")?;
                let artifacts = BTreeMap::from([
                    (output_portable.clone(), sha256_bytes(&html_bytes)),
                    (portable_path(&ir_path), sha256_bytes(&ir_bytes)),
                    (portable_path(&map_path), sha256_bytes(&map_bytes)),
                ]);
                files.push((output, html_bytes));
                files.push((ir_path, ir_bytes));
                files.push((map_path, map_bytes));
                all_islands.extend(route_islands.iter().cloned());
                route_state = RouteBuildState {
                    input_sha,
                    artifacts,
                    islands: route_islands,
                };
                compiled_routes += 1;
            }
            next_state.routes.insert(key, route_state);
        }

        files.extend(collect_shared_assets(snapshot_root)?);
        // Published under the media type's own name, which is what a browser
        // expects behind `rel="manifest"`.
        let manifest_source = snapshot_root.join(crate::native::MANIFEST_NAME);
        if manifest_source.is_file() {
            let bytes = fs::read(&manifest_source).map_err(|error| {
                Failure::one(diagnostic(
                    1201,
                    format!("Cannot read {}: {error}", crate::native::MANIFEST_NAME),
                    Some(String::from("Keep the manifest a readable regular file.")),
                    None,
                ))
            })?;
            files.push((PathBuf::from(crate::native::MANIFEST_OUTPUT), bytes));
        }

        files.push((
            PathBuf::from(".tachyon/navigation.css"),
            NAVIGATION_STYLESHEET.as_bytes().to_vec(),
        ));
        files.push((
            PathBuf::from("spa-renderer.js"),
            COMPATIBILITY_SPA_RUNTIME.as_bytes().to_vec(),
        ));
        if !component_styles.is_empty() {
            files.push((
                PathBuf::from(".tachyon/components.css"),
                component_styles.into_bytes(),
            ));
        }

        if has_tac_routes {
            files.push((
                PathBuf::from(".tachyon/tac-client.js"),
                TAC_CLIENT_RUNTIME.as_bytes().to_vec(),
            ));
        }

        for name in &all_islands {
            let Some(component) = components.get(name) else {
                return Err(Failure::one(diagnostic(
                    1402,
                    format!("Cached client component '{name}' is no longer registered."),
                    None,
                    None,
                )));
            };
            let Some(source) = component.script_path() else {
                return Err(Failure::one(diagnostic(
                    1405,
                    format!("Client component '{name}' has no Tac companion."),
                    None,
                    None,
                )));
            };
            // A TypeScript component companion goes through the
            // TypeScript compiler, the same route a page companion takes.
            let authored = prepare_component_script(
                output_io(fs::read(source), source)?,
                "",
                &portable_path(source),
            )?;
            let bytes = if source.extension().is_some_and(|value| value == "ts") {
                let portable = portable_path(source.strip_prefix(snapshot_root).unwrap_or(source));
                transpile_typescript(snapshot_root, source, &portable, &authored).await?
            } else {
                authored
            };
            let bytes = rewrite_client_shared_imports(snapshot_root, source, bytes);
            // Component modules always publish below `.tachyon/components`.
            // A relative public URL works both at an HTTP origin and when
            // an isolated native surface loads the bundle through `file:`.
            let bytes = String::from_utf8(bytes).map_or_else(
                std::string::FromUtf8Error::into_bytes,
                |module| {
                    module
                        .replace("'/shared/", "'../../shared/")
                        .replace("\"/shared/", "\"../../shared/")
                        .into_bytes()
                },
            );
            files.push((
                PathBuf::from(format!(".tachyon/components/{name}.js")),
                bytes,
            ));
        }
        files.push((
            PathBuf::from("route-manifest.json"),
            pretty_json(&manifest, "Route Manifest v1")?,
        ));
        files.extend(crate::handler::api_reference_files(project.route_graph())?);
        files.push((
            PathBuf::from(".tachyon/register-sw.js"),
            SERVICE_WORKER_REGISTRATION.as_bytes().to_vec(),
        ));
        // Digested over the generated output, so the worker's bytes change
        // exactly when the output does and repeated builds stay identical.
        let version = build_version(&files, build_config_digest.as_deref());
        let public_assets = service_worker_assets(&files)?;
        files.push((
            PathBuf::from("tachyon-sw.js"),
            SERVICE_WORKER
                .replace("__VERSION__", &version)
                .replace("__CACHE_RULES__", &cache_rules_literal)
                .replace("__PUBLIC_ASSETS__", &public_assets)
                .into_bytes(),
        ));
        files.push((
            PathBuf::from(".tachyon/build-state.json"),
            pretty_json(&next_state, "incremental build state")?,
        ));
        files.sort_by_key(|value| portable_path(&value.0));
        let parent = output_directory.parent().ok_or_else(|| {
            Failure::one(diagnostic(
                1201,
                "The output directory has no writable parent.",
                None,
                None,
            ))
        })?;
        output_io(fs::create_dir_all(parent), &output_directory)?;
        let stage = output_io(
            tempfile::Builder::new()
                .prefix(".tachyon-build-")
                .tempdir_in(parent),
            &output_directory,
        )?;
        output_io(write_stage(stage.path(), &files), &output_directory)?;
        run_post_bundle_hook(snapshot_root, stage.path(), "web").await?;
        let digest = output_io(digest_stage(stage.path()), &output_directory)?;
        output_io(publish(stage, &output_directory), &output_directory)?;

        Ok(BuildResult {
            output_directory,
            route_count: project.route_graph().routes().len(),
            sha256: digest,
            compiled_routes,
            reused_routes,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BuildState {
    version: u8,
    routes: BTreeMap<String, RouteBuildState>,
}

struct RouteProgram {
    program: crate::template::TemplateProgram,
    page_scope: crate::template::Scope,
    has_page_module: bool,
    inline_state: String,
}

impl Default for BuildState {
    fn default() -> Self {
        Self {
            version: BUILD_STATE_VERSION,
            routes: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RouteBuildState {
    input_sha: String,
    artifacts: BTreeMap<String, String>,
    islands: BTreeSet<String>,
}

fn validate_companion_target(
    route: &crate::RouteNode,
    target: Option<tachyon_contracts::NativeTarget>,
) -> Result<(), Failure> {
    let languages = route
        .companions()
        .iter()
        .filter_map(|companion| {
            if let CompanionKind::Native(language) = companion.kind {
                Some(language)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let browser = route.companions().iter().any(|companion| {
        matches!(
            companion.kind,
            CompanionKind::ClientModule | CompanionKind::TypeScriptModule
        )
    });
    if languages.is_empty()
        || browser
        || target.is_some_and(|target| {
            crate::project::NativeCompanion::most_specific(&languages, target).is_some()
        })
    {
        return Ok(());
    }
    Err(crate::project::unreachable_companion(
        route.route(),
        &languages,
        target.map_or("the web", crate::native_target_directory),
        target,
    ))
}

fn load_build_state(output: &Path) -> Option<BuildState> {
    let path = output.join(".tachyon/build-state.json");
    let metadata = fs::symlink_metadata(&path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    let state: BuildState = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    (state.version == BUILD_STATE_VERSION).then_some(state)
}

fn verify_artifacts(output: &Path, artifacts: &BTreeMap<String, String>) -> bool {
    !artifacts.is_empty()
        && artifacts.iter().all(|(relative, expected)| {
            let relative = Path::new(relative);
            if validate_artifact_path(relative).is_none() {
                return false;
            }
            let path = output.join(relative);
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                return false;
            };
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return false;
            }
            fs::read(path)
                .ok()
                .is_some_and(|bytes| sha256_bytes(&bytes) == *expected)
        })
}

fn validate_artifact_path(path: &Path) -> Option<()> {
    (!path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_))))
    .then_some(())
}

fn route_input_sha(source: &[u8], component_digest: &str, companion_digest: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(env!("CARGO_PKG_VERSION").as_bytes());
    hasher.update([0]);
    hasher.update(component_digest.as_bytes());
    hasher.update([0]);
    hasher.update(companion_digest.as_bytes());
    hasher.update([0]);
    hasher.update(source);
    hex_digest(hasher.finalize())
}

fn route_key(route: &str) -> String {
    if route == "/" {
        String::from("root")
    } else {
        route.trim_start_matches('/').replace('/', "__")
    }
}

/// Publishes `client/shared` at `/shared` without following links or allowing
/// one asset tree to make a build unbounded.
fn collect_shared_assets(project_root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, Failure> {
    let root = project_root.join("client/shared");
    let Ok(root_metadata) = fs::symlink_metadata(&root) else {
        return Ok(Vec::new());
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(shared_asset_failure(
            &root,
            "the shared asset root must be a regular directory",
        ));
    }

    let mut directories = vec![root.clone()];
    let mut assets = Vec::new();
    let mut total = 0_u64;
    while let Some(directory) = directories.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| shared_asset_failure(&directory, &error.to_string()))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| shared_asset_failure(&directory, &error.to_string()))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| shared_asset_failure(&path, &error.to_string()))?;
            if metadata.file_type().is_symlink() {
                return Err(shared_asset_failure(
                    &path,
                    "symbolic links are not allowed",
                ));
            }
            if metadata.is_dir() {
                directories.push(path);
                continue;
            }
            if !metadata.is_file() {
                return Err(shared_asset_failure(
                    &path,
                    "only regular files are allowed",
                ));
            }
            if metadata.len() > MAX_SHARED_ASSET_BYTES {
                return Err(shared_asset_failure(
                    &path,
                    "the file exceeds the 16 MiB per-asset limit",
                ));
            }
            total = total.saturating_add(metadata.len());
            if total > MAX_SHARED_ASSET_TOTAL_BYTES {
                return Err(shared_asset_failure(
                    &root,
                    "the asset tree exceeds the 64 MiB total limit",
                ));
            }
            if assets.len() >= MAX_SHARED_ASSETS {
                return Err(shared_asset_failure(
                    &root,
                    "the asset tree exceeds the 4096-file limit",
                ));
            }
            let relative = path
                .strip_prefix(&root)
                .map_err(|error| shared_asset_failure(&path, &error.to_string()))?;
            let bytes =
                fs::read(&path).map_err(|error| shared_asset_failure(&path, &error.to_string()))?;
            let bytes = strip_shared_css_imports(&path, bytes);
            assets.push((PathBuf::from("shared").join(relative), bytes));
        }
    }
    assets.sort_by_key(|asset| portable_path(&asset.0));
    Ok(assets)
}

/// Removes build-time CSS imports from shared browser entry modules.
///
/// The latest JavaScript binary uses Bun to bundle a bare `import './x.css'`
/// into its stylesheet. Rust publishes the same CSS directly and links it from
/// HTML, so leaving that specifier in the emitted browser module would ask the
/// browser to parse CSS as JavaScript. Only a complete bare import whose
/// specifier ends in `.css` is removed; JavaScript imports and source code are
/// preserved byte-for-byte.
fn strip_shared_css_imports(path: &Path, bytes: Vec<u8>) -> Vec<u8> {
    if !path
        .extension()
        .is_some_and(|extension| extension == "js" || extension == "mjs")
    {
        return bytes;
    }
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => return error.into_bytes(),
    };
    source
        .split_inclusive('\n')
        .filter(|line| {
            let statement = line.trim().trim_end_matches(';');
            let Some(specifier) = statement.strip_prefix("import ") else {
                return true;
            };
            let quoted = specifier.len() >= 2
                && ((specifier.starts_with('\'') && specifier.ends_with('\''))
                    || (specifier.starts_with('"') && specifier.ends_with('"')));
            let is_css = quoted
                && Path::new(&specifier[1..specifier.len() - 1])
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("css"));
            !is_css
        })
        .collect::<String>()
        .into_bytes()
}

pub(crate) fn strip_page_state_scripts(
    source: &str,
    source_path: &str,
) -> Result<(String, String), Failure> {
    let mut template = source.as_bytes().to_vec();
    let mut state = String::new();
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find("<script") {
        let start = cursor + relative;
        let Some(open_end_relative) = source[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_relative + 1;
        let opening = &source[start..open_end];
        if opening.trim() != "<script>" {
            return Err(Failure::one(diagnostic(
                1306,
                "A Tac page state script cannot declare HTML attributes.",
                Some(String::from(
                    "Use a plain <script> block for state declarations and a colocated tac.js for behavior.",
                )),
                source_span(source_path, start, open_end),
            )));
        }
        let Some(close_relative) = source[open_end..].find("</script>") else {
            return Err(Failure::one(diagnostic(
                1301,
                "Tac page state script is not closed.",
                Some(String::from("Add </script>.")),
                source_span(source_path, start, open_end),
            )));
        };
        let close = open_end + close_relative;
        let block = &source[open_end..close];
        validate_page_state(block, source_path, open_end)?;
        state.push_str(block);
        state.push('\n');
        let end = close + "</script>".len();
        for byte in &mut template[start..end] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
        cursor = end;
    }
    let template = String::from_utf8(template).map_err(|_| {
        Failure::one(diagnostic(
            1301,
            "Template source is not UTF-8.",
            None,
            source_span(source_path, 0, source.len()),
        ))
    })?;
    Ok((template, state))
}

fn mask_script_blocks(source: &str) -> String {
    let mut template = source.as_bytes().to_vec();
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find("<script") {
        let start = cursor + relative;
        let end = source[start..]
            .find("</script>")
            .map_or(source.len(), |relative| {
                start + relative + "</script>".len()
            });
        for byte in &mut template[start..end] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
        cursor = end;
    }
    String::from_utf8_lossy(&template).into_owned()
}

fn validate_page_state(source: &str, source_path: &str, offset: usize) -> Result<(), Failure> {
    for statement in source.split([';', '\n']) {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        let declaration = ["let ", "const ", "var "]
            .iter()
            .find_map(|prefix| statement.strip_prefix(prefix));
        let valid = declaration
            .and_then(|declaration| declaration.split_once('='))
            .is_some_and(|(name, value)| {
                valid_page_field(name.trim()) && parse_javascript_literal(value).is_some()
            });
        if !valid {
            let start = source.find(statement).unwrap_or(0);
            return Err(Failure::one(diagnostic(
                1306,
                "A Tac page state script may contain only literal variable declarations.",
                Some(String::from(
                    "Move behavior into the colocated tac.js module; keep only let, const, or var declarations with bounded JSON-like literal values here.",
                )),
                source_span(
                    source_path,
                    offset.saturating_add(start),
                    offset.saturating_add(start).saturating_add(statement.len()),
                ),
            )));
        }
    }
    Ok(())
}

fn read_template_source(bytes: &[u8], source_path: &str) -> Result<String, Failure> {
    if bytes.len() > 1_048_576 {
        return Err(Failure::one(diagnostic(
            1301,
            "Template source exceeds the 1 MiB limit.",
            Some(String::from("Split the view into bounded Tac components.")),
            source_span(source_path, 0, bytes.len()),
        )));
    }
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| {
            let start = error.valid_up_to();
            Failure::one(diagnostic(
                1301,
                "Template source must be valid UTF-8.",
                None,
                source_span(source_path, start, start.saturating_add(1)),
            ))
        })
}

fn parse_page_state(source: &str) -> crate::template::Scope {
    source
        .split([';', '\n'])
        .filter_map(|statement| {
            let statement = statement.trim();
            let declaration = ["let ", "const ", "var "]
                .iter()
                .find_map(|prefix| statement.strip_prefix(prefix))?;
            let (name, value) = declaration.split_once('=')?;
            let name = name.trim();
            valid_page_field(name)
                .then(|| parse_javascript_literal(value).map(|value| (String::from(name), value)))?
        })
        .collect()
}

fn parse_page_class_fields(source: &str) -> crate::template::Scope {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim().trim_end_matches(';');
            if line.starts_with('@')
                || line.starts_with("export ")
                || line.starts_with("static ")
                || line.contains('(')
            {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            let name = name.trim();
            valid_page_field(name)
                .then(|| parse_javascript_literal(value).map(|value| (String::from(name), value)))?
        })
        .collect()
}

fn valid_page_field(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
}

fn parse_javascript_literal(source: &str) -> Option<serde_json::Value> {
    let source = source.trim();
    if source.starts_with('[') && source.ends_with(']') {
        let inner = &source[1..source.len() - 1];
        if inner.trim().is_empty() {
            return Some(serde_json::Value::Array(Vec::new()));
        }
        return inner
            .split(',')
            .map(parse_javascript_literal)
            .collect::<Option<Vec<_>>>()
            .map(serde_json::Value::Array);
    }
    if source.len() >= 2
        && ((source.starts_with('\'') && source.ends_with('\''))
            || (source.starts_with('"') && source.ends_with('"')))
    {
        return Some(serde_json::Value::String(String::from(
            &source[1..source.len() - 1],
        )));
    }
    serde_json::from_str(source).ok()
}

/// One decorator the compiler applies at build time.
struct Decorated {
    /// The member it decorates.
    member: String,
    /// The signal it publishes to or subscribes from.
    signal: String,
    /// `method` or `field`, which decides how the runtime applies it.
    kind: &'static str,
}

/// Reads the argument of a decorator line, if it was called with one.
///
/// `@publish` and `@publish('cart.total')` are both valid; without a name the
/// member's own name is the signal.
fn decorator_argument(line: &str) -> Option<String> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    let inner = line.get(open + 1..close)?.trim();
    let quote = inner.chars().next()?;
    if !matches!(quote, '\'' | '"') || !inner.ends_with(quote) || inner.len() < 3 {
        return None;
    }
    let name = inner.get(1..inner.len() - 1)?;
    (name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$'))
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '$'))
        && name.len() <= 128)
        .then(|| String::from(name))
}

/// Records a decorator against the member on the line that follows it.
fn decorated_member(line: &str, argument: Option<String>) -> Option<Decorated> {
    let mut signature = line.trim();
    while let Some(rest) = ["public ", "readonly ", "async "]
        .iter()
        .find_map(|prefix| signature.strip_prefix(prefix))
    {
        signature = rest.trim_start();
    }
    if ["static ", "private ", "protected "]
        .iter()
        .any(|prefix| signature.starts_with(prefix))
    {
        return None;
    }
    let head = signature
        .split(['(', '=', ';', ' ', ':', '?', '!'])
        .next()
        .unwrap_or_default()
        .trim();
    if !valid_page_field(head) {
        return None;
    }
    if matches!(head, "constructor" | "tac" | "__proto__" | "prototype") {
        return None;
    }
    // A method is followed by its parameter list; anything else is a field.
    let kind = if signature[head.len()..].trim_start().starts_with('(') {
        "method"
    } else {
        "field"
    };
    Some(Decorated {
        member: String::from(head),
        signal: argument.unwrap_or_else(|| String::from(head)),
        kind,
    })
}

/// Serialises one decorator list for the class the runtime reads it from.
fn decorator_metadata(name: &str, entries: &[Decorated]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let items = entries
        .iter()
        .map(|entry| {
            format!(
                "[{}, {}, {}]",
                serde_json::to_string(&entry.member).unwrap_or_default(),
                serde_json::to_string(&entry.signal).unwrap_or_default(),
                serde_json::to_string(entry.kind).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("\n  static {name} = [{items}];")
}

fn transform_page_module(bytes: Vec<u8>, inline_state: &str) -> Vec<u8> {
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => return error.into_bytes(),
    };
    let mut output = String::with_capacity(source.len() + inline_state.len() + 128);
    let mut mount_methods = Vec::new();
    let mut published = Vec::new();
    let mut subscribed = Vec::new();
    // What the previous line asked for, waiting for the member it decorates.
    let mut pending = Vec::new();
    let mask = crate::lexical::code_mask("js", &source);
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        let executable = mask
            .code
            .get(offset + line.len() - line.trim_start().len())
            .copied()
            .unwrap_or(false);
        offset += line.len();
        // No browser implements the decorator proposal, so one that reached a
        // page would be a syntax error rather than a feature. They are read
        // here and compiled into metadata the runtime applies.
        let decorator = executable
            .then(|| {
                ["onMount", "publish", "subscribe"]
                    .into_iter()
                    .find(|name| {
                        trimmed
                            .strip_prefix('@')
                            .and_then(|rest| rest.strip_prefix(*name))
                            .is_some_and(|rest| rest.is_empty() || rest.starts_with('('))
                    })
            })
            .flatten();
        if let Some(decorator) = decorator {
            pending.push((decorator, decorator_argument(trimmed)));
            continue;
        }
        if executable && !trimmed.is_empty() {
            for (decorator, argument) in pending.drain(..) {
                if let Some(entry) = decorated_member(line, argument) {
                    match decorator {
                        "onMount" => mount_methods.push(entry.member),
                        "publish" => published.push(entry),
                        _ => subscribed.push(entry),
                    }
                }
            }
        }
        output.push_str(line.trim_end_matches('\n'));
        output.push('\n');
    }
    let output_mask = crate::lexical::code_mask("js", &output);
    let Some(open) = default_class_open(&output, &output_mask.code) else {
        return output.into_bytes();
    };
    let insertion = open + 1;
    let fields = inline_state
        .split([';', '\n'])
        .filter_map(|statement| {
            let declaration = ["let ", "const ", "var "]
                .iter()
                .find_map(|prefix| statement.trim().strip_prefix(prefix))?;
            let (name, value) = declaration.split_once('=')?;
            let value = parse_javascript_literal(value)?;
            valid_page_field(name.trim()).then(|| {
                let value = serde_json::to_string(&value).unwrap_or_else(|_| String::from("null"));
                format!("\n  {} = {value};", name.trim())
            })
        })
        .collect::<String>();
    let methods = serde_json::to_string(&mount_methods).unwrap_or_else(|_| String::from("[]"));
    output.insert_str(
        insertion,
        &format!(
            "\n  static __tachyonOnMount = {methods};{}{}{fields}",
            decorator_metadata("__tachyonPublish", &published),
            decorator_metadata("__tachyonSubscribe", &subscribed),
        ),
    );
    output.into_bytes()
}

fn prepare_component_script(
    bytes: Vec<u8>,
    inline_state: &str,
    source_path: &str,
) -> Result<Vec<u8>, Failure> {
    let source = std::str::from_utf8(&bytes).map_err(|_| decorator_failure(source_path))?;
    let mask = crate::lexical::code_mask("js", source);
    let mut pending = false;
    let mut mount_pending = false;
    let mut has_decorators = source.match_indices('@').any(|(at, _)| {
        mask.code.get(at) == Some(&true)
            && ["@onMount", "@publish", "@subscribe"]
                .iter()
                .any(|name| source[at..].starts_with(name))
    });
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        let executable = mask
            .code
            .get(offset + line.len() - line.trim_start().len())
            .copied()
            .unwrap_or(false);
        offset += line.len();
        if !executable || trimmed.is_empty() {
            continue;
        }
        if let Some(name) = ["onMount", "publish", "subscribe"]
            .iter()
            .find(|name| trimmed.starts_with(&format!("@{name}")))
        {
            let suffix = trimmed
                .strip_prefix(&format!("@{name}"))
                .unwrap_or_default();
            if !suffix.is_empty()
                && (*name == "onMount"
                    || !suffix.ends_with(')')
                    || decorator_argument(trimmed).is_none())
            {
                return Err(decorator_failure(source_path));
            }
            pending = true;
            mount_pending |= *name == "onMount";
            has_decorators = true;
        } else if pending {
            if decorated_member(trimmed, None)
                .is_none_or(|entry| mount_pending && entry.kind != "method")
            {
                return Err(decorator_failure(source_path));
            }
            pending = false;
            mount_pending = false;
        }
    }
    if pending {
        return Err(decorator_failure(source_path));
    }
    if !has_decorators && inline_state.trim().is_empty() {
        return Ok(bytes);
    }
    if has_decorators {
        validate_decorator_scope(source, &mask.code, source_path)?;
    }
    Ok(transform_page_module(bytes, inline_state))
}

fn default_class_open(source: &str, mask: &[bool]) -> Option<usize> {
    let start = source
        .match_indices("export default class")
        .find_map(|(at, _)| (mask.get(at) == Some(&true)).then_some(at))?;
    source
        .as_bytes()
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(at, byte)| (*byte == b'{' && mask.get(at) == Some(&true)).then_some(at))
}

fn validate_decorator_scope(source: &str, mask: &[bool], source_path: &str) -> Result<(), Failure> {
    let open = default_class_open(source, mask).ok_or_else(|| decorator_failure(source_path))?;
    let mut depth = 0_usize;
    let mut finished = false;
    for (at, byte) in source.bytes().enumerate() {
        if mask.get(at) != Some(&true) {
            continue;
        }
        if at >= open && !finished {
            if byte == b'{' {
                depth = depth.saturating_add(1);
            } else if byte == b'}' {
                depth = depth.saturating_sub(1);
            }
            finished = depth == 0;
        }
        if byte != b'@'
            || !["@onMount", "@publish", "@subscribe"]
                .iter()
                .any(|name| source[at..].starts_with(name))
        {
            continue;
        }
        let line = source[..at]
            .rsplit_once('\n')
            .map_or(&source[..at], |(_, line)| line);
        if at < open || finished || depth != 1 || !line.trim().is_empty() {
            return Err(decorator_failure(source_path));
        }
    }
    Ok(())
}

fn decorator_failure(source_path: &str) -> Failure {
    Failure::one(diagnostic(
        1306,
        "Invalid Tac lifecycle or signal decorator.",
        Some(String::from(
            "Use @onMount on an instance method, or @publish/@subscribe with an optional quoted signal name on an instance field or method; place each decorator on its own line.",
        )),
        source_span(source_path, 0, 0),
    ))
}

/// Renders one route's declared metadata into head elements.
///
/// Only what was declared: a project that names a title and nothing else gets
/// a title and nothing else, rather than a page of empty social tags.
fn render_page_metadata(declared: &crate::native::PageMetadata) -> String {
    use std::fmt::Write as _;

    let mut head = String::new();
    if let Some(title) = &declared.title {
        let title = html_attribute_escape(title);
        let _ = write!(
            head,
            "<title>{title}</title>\
             <meta property=\"og:title\" content=\"{title}\">\
             <meta name=\"twitter:title\" content=\"{title}\">"
        );
    }
    if let Some(description) = &declared.description {
        let description = html_attribute_escape(description);
        let _ = write!(
            head,
            "<meta name=\"description\" content=\"{description}\">\
             <meta property=\"og:description\" content=\"{description}\">\
             <meta name=\"twitter:description\" content=\"{description}\">"
        );
    }
    if let Some(canonical) = &declared.canonical {
        let canonical = html_attribute_escape(canonical);
        let _ = write!(
            head,
            "<link rel=\"canonical\" href=\"{canonical}\">\
             <meta property=\"og:url\" content=\"{canonical}\">"
        );
    }
    if let Some(image) = &declared.image {
        let image = html_attribute_escape(image);
        let _ = write!(
            head,
            "<meta property=\"og:image\" content=\"{image}\">\
             <meta name=\"twitter:card\" content=\"summary_large_image\">"
        );
    }
    if let Some(site) = &declared.site_name {
        let site = html_attribute_escape(site);
        let _ = write!(
            head,
            "<meta property=\"og:site_name\" content=\"{site}\">\
             <meta property=\"og:type\" content=\"website\">"
        );
    }
    head
}

/// Escapes a configured value for an HTML attribute or text node.
pub(crate) fn html_attribute_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn shared_asset_failure(path: &Path, detail: &str) -> Failure {
    Failure::one(diagnostic(
        1201,
        format!(
            "Cannot publish shared asset '{}': {detail}.",
            path.display()
        ),
        Some(String::from(
            "Keep client/shared inside the project as bounded regular files and directories.",
        )),
        None,
    ))
}

fn pretty_json(value: &impl Serialize, name: &str) -> Result<Vec<u8>, Failure> {
    serde_json::to_vec_pretty(value)
        .map(|mut bytes| {
            bytes.push(b'\n');
            bytes
        })
        .map_err(|error| {
            Failure::one(diagnostic(
                1201,
                format!("Cannot serialize {name}: {error}"),
                None,
                None,
            ))
        })
}

/// Digests the generated output into a short, stable build version.
///
/// Paths are folded in as well as contents, so a file that only moves still
/// produces a new version and invalidates the previous cache.
/// Reads every component stylesheet and scopes it to that component.
///
/// CSS `@scope` is the platform's own answer to component style scoping, so
/// nothing here parses or rewrites a selector: the rule names the attribute
/// the compiler put on the component's roots, and the browser does the rest.
/// A browser without `@scope` ignores the block, which leaves the component
/// unstyled rather than leaking its rules across the page.
fn collect_component_styles(components: &ComponentRegistry) -> Result<String, Failure> {
    let mut bundle = String::new();
    for name in components.names() {
        let Some(style) = components
            .get(&name)
            .and_then(ComponentDefinition::style_path)
        else {
            continue;
        };
        let source = fs::read_to_string(style).map_err(|error| {
            Failure::one(diagnostic(
                1403,
                format!("Cannot read component stylesheet for '{name}': {error}"),
                Some(String::from(
                    "Check the stylesheet's permissions and encoding.",
                )),
                source_span(&portable_path(style), 0, 0),
            ))
        })?;
        if source.len() > MAX_COMPONENT_STYLE_BYTES {
            return Err(Failure::one(diagnostic(
                1403,
                format!("Component stylesheet for '{name}' exceeds the 1 MiB limit."),
                Some(String::from("Split the component or trim its stylesheet.")),
                source_span(&portable_path(style), 0, 0),
            )));
        }
        let _ = writeln!(
            bundle,
            "@scope ([{SCOPE_ATTRIBUTE}=\"{name}\"]) {{\n{}\n}}",
            source.trim_end()
        );
    }
    Ok(bundle)
}

fn build_version(files: &[(PathBuf, Vec<u8>)], build_config_digest: Option<&str>) -> String {
    let mut ordered: Vec<(String, &Vec<u8>)> = files
        .iter()
        .map(|(path, bytes)| (portable_path(path), bytes))
        .collect();
    ordered.sort_by(|left, right| left.0.cmp(&right.0));

    let mut hasher = Sha256::new();
    // A security-only worker upgrade must also retire the previous cache.
    hasher.update(SERVICE_WORKER.as_bytes());
    hasher.update(SERVICE_WORKER_REGISTRATION.as_bytes());
    if let Some(digest) = build_config_digest {
        hasher.update(b"tac.config.js\0");
        hasher.update(digest.as_bytes());
        hasher.update([0]);
    }
    for (path, bytes) in ordered {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(bytes);
    }
    hex_digest(hasher.finalize())
        .get(..16)
        .unwrap_or("0")
        .to_owned()
}

fn service_worker_assets(files: &[(PathBuf, Vec<u8>)]) -> Result<String, Failure> {
    let mut assets = BTreeMap::new();
    for (path, bytes) in files {
        if bytes.len() > 4 * 1_048_576 {
            continue;
        }
        let path = format!("/{}", portable_path(path));
        let asset = serde_json::json!({ "sha256": sha256_bytes(bytes), "bytes": bytes.len() });
        assets.insert(path.clone(), asset.clone());
        if let Some(directory) = path.strip_suffix("index.html") {
            assets.insert(directory.to_owned(), asset.clone());
            if directory != "/" {
                assets.insert(directory.trim_end_matches('/').to_owned(), asset);
            }
        }
    }
    serde_json::to_string(&assets).map_err(|_| {
        Failure::one(diagnostic(
            1201,
            "Cannot encode public asset fingerprints.",
            None,
            None,
        ))
    })
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(hasher.finalize())
}

pub(crate) fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut encoded = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub(crate) fn portable_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Rewrites authored relative imports into the stable public shared-asset URL.
///
/// The legacy compiler bundles a companion from its authored location, so a
/// relative `../../../shared/...` specifier is the compatible source form. The
/// Rust compiler publishes that module at a different output location and
/// therefore makes the same specifier root-relative after transpilation. Only
/// the exact path from this source directory to `client/shared` is rewritten;
/// unrelated relative imports and source text are left untouched.
fn rewrite_client_shared_imports(project_root: &Path, source: &Path, bytes: Vec<u8>) -> Vec<u8> {
    let Some(parent) = source.parent() else {
        return bytes;
    };
    let client = project_root.join("client");
    let Ok(relative_parent) = parent.strip_prefix(client) else {
        return bytes;
    };
    let depth = relative_parent.components().count();
    if depth == 0 {
        return bytes;
    }
    let mut module = match String::from_utf8(bytes) {
        Ok(module) => module,
        Err(error) => return error.into_bytes(),
    };
    let authored = format!("{}shared/", "../".repeat(depth));
    for quote in ['\'', '"'] {
        module = module.replace(&format!("{quote}{authored}"), &format!("{quote}/shared/"));
    }
    module.into_bytes()
}

fn sort_diagnostics(diagnostics: &mut [tachyon_diagnostics::Diagnostic]) {
    diagnostics.sort_by(|left, right| {
        let left_span = left.spans.first();
        let right_span = right.spans.first();
        left_span
            .map(|span| (&span.file, span.start, span.end))
            .cmp(&right_span.map(|span| (&span.file, span.start, span.end)))
            .then_with(|| left.code.cmp(&right.code))
    });
}

pub(crate) fn validate_output_path(path: &Path) -> Result<&Path, Failure> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(invalid_output(path));
    }
    let mut components = path.components();
    let Some(Component::Normal(first)) = components.next() else {
        return Err(invalid_output(path));
    };
    if ["client", "server", ".git"]
        .iter()
        .any(|reserved| first == *reserved)
        || components.any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid_output(path));
    }
    Ok(path)
}

pub(crate) fn resolve_output_path(root: &Path, path: &Path) -> Result<PathBuf, Failure> {
    if !path.is_absolute() {
        let relative = validate_output_path(path)?;
        assert_output_is_safe(root, relative)?;
        return Ok(root.join(relative));
    }

    let normalized_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let normalized = normalize_absolute_output(path)?;
    if normalized == normalized_root
        || normalized == Path::new("/")
        || normalized.starts_with(normalized_root.join("client"))
        || normalized.starts_with(normalized_root.join("server"))
        || normalized.starts_with(normalized_root.join(".git"))
    {
        return Err(invalid_output(path));
    }

    let mut current = PathBuf::new();
    for component in normalized.components() {
        match component {
            // A Windows drive/UNC prefix is not itself a filesystem path. It
            // becomes inspectable only after the following root component is
            // appended (for example `\\?\C:\`, not `\\?\C:`).
            Component::Prefix(prefix) => {
                current.push(prefix.as_os_str());
                continue;
            }
            Component::RootDir => current.push(component.as_os_str()),
            Component::Normal(segment) => current.push(segment),
            Component::CurDir | Component::ParentDir => return Err(invalid_output(path)),
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Failure::one(diagnostic(
                    1202,
                    format!("Output path '{}' contains a symlink.", path.display()),
                    Some(String::from("Choose a regular output directory.")),
                    None,
                )));
            }
            Ok(metadata) if current == normalized && !metadata.is_dir() => {
                return Err(Failure::one(diagnostic(
                    1202,
                    format!("Output path '{}' is not a directory.", path.display()),
                    None,
                    None,
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(output_error(&current, &error)),
        }
    }
    Ok(normalized)
}

fn normalize_absolute_output(path: &Path) -> Result<PathBuf, Failure> {
    let mut current = path;
    let mut missing = Vec::new();
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) => {
                if current == path && metadata.file_type().is_symlink() {
                    return Err(Failure::one(diagnostic(
                        1202,
                        format!("Output path '{}' is a symlink.", path.display()),
                        Some(String::from("Choose a regular output directory.")),
                        None,
                    )));
                }
                let mut normalized =
                    fs::canonicalize(current).map_err(|error| output_error(current, &error))?;
                for segment in missing.iter().rev() {
                    normalized.push(segment);
                }
                return Ok(normalized);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(segment) = current.file_name() else {
                    return Err(invalid_output(path));
                };
                missing.push(segment.to_os_string());
                let Some(parent) = current.parent() else {
                    return Err(invalid_output(path));
                };
                current = parent;
            }
            Err(error) => return Err(output_error(current, &error)),
        }
    }
}

fn invalid_output(path: &Path) -> Failure {
    Failure::one(diagnostic(
        1202,
        format!("Output directory '{}' is not a safe path.", path.display()),
        Some(String::from(
            "Use a regular directory outside client/, server/, and .git/.",
        )),
        None,
    ))
}

pub(crate) fn assert_output_is_safe(root: &Path, relative: &Path) -> Result<(), Failure> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(invalid_output(relative));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Failure::one(diagnostic(
                    1202,
                    format!("Output path '{}' contains a symlink.", relative.display()),
                    Some(String::from("Choose a regular project-relative directory.")),
                    None,
                )));
            }
            Ok(metadata) if current == root.join(relative) && !metadata.is_dir() => {
                return Err(Failure::one(diagnostic(
                    1202,
                    format!("Output path '{}' is not a directory.", relative.display()),
                    None,
                    None,
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(output_error(&current, &error)),
        }
    }
    Ok(())
}

/// Inserts a reference immediately before a closing tag, appending when the
/// document has no such tag.
fn inject_before(html: &str, close: &str, value: &str) -> String {
    html.rfind(close).map_or_else(
        || format!("{html}{value}"),
        |position| format!("{}{value}{}", &html[..position], &html[position..]),
    )
}

/// Emits JavaScript for one TypeScript companion using the TypeScript compiler.
///
/// The compiler is located project-first so a pinned `node_modules` copy wins
/// over anything on `PATH`. Type checking is skipped: emission must not depend
/// on a project type-checking cleanly, which `ty` never claims to enforce.
#[allow(clippy::too_many_lines)]
async fn transpile_typescript(
    project_root: &Path,
    source: &Path,
    portable_source: &str,
    source_bytes: &[u8],
) -> Result<Vec<u8>, Failure> {
    let local = project_root.join("node_modules/.bin/tsc");
    let program = if local.is_file() {
        local
    } else {
        PathBuf::from("tsc")
    };
    // `--ignoreConfig` first exists in TypeScript 6, verified against 5.6, 5.9,
    // 6.0.3, and 7.0.2. An older compiler must
    // be reported as a version requirement, not as an unknown-option error.
    let mut version_command = tokio::process::Command::new(&program);
    version_command
        .arg("--version")
        .stdin(std::process::Stdio::null());
    let version = run_tool(&mut version_command, TYPESCRIPT_DEADLINE, TOOL_OUTPUT_BYTES)
        .await
        .map_err(|error| {
            typescript_error(
                portable_source,
                &format!(
                    "cannot start the TypeScript compiler '{}': {error}. Install \
                     typescript 6 or newer in the project or on PATH.",
                    program.display()
                ),
            )
        })?;
    let banner = String::from_utf8_lossy(&version.stdout);
    let major = banner
        .split_whitespace()
        .last()
        .and_then(|value| value.split('.').next())
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if major < MINIMUM_TYPESCRIPT_MAJOR {
        return Err(typescript_error(
            portable_source,
            &format!(
                "found TypeScript '{}', but emitting tac.ts requires TypeScript 6 \
                 or newer.",
                banner.trim()
            ),
        ));
    }

    let staged = tempfile::Builder::new()
        .prefix(".tachyon-typescript-")
        .tempdir()
        .map_err(|error| typescript_error(portable_source, &error.to_string()))?;
    let out_dir = staged.path().to_string_lossy().into_owned();
    let source_name = source
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("tac.ts"));
    let staged_source = staged.path().join(source_name);
    fs::write(&staged_source, source_bytes)
        .map_err(|error| typescript_error(portable_source, &error.to_string()))?;
    let source_argument = staged_source.to_string_lossy().into_owned();

    let mut emit_command = tokio::process::Command::new(&program);
    emit_command
        .args([
            source_argument.as_str(),
            "--outDir",
            out_dir.as_str(),
            "--target",
            "es2022",
            "--module",
            "esnext",
            "--moduleResolution",
            "bundler",
            "--noCheck",
            // The emit must not depend on a project's tsconfig.json: the
            // target and module format are fixed by Tachyon so output stays
            // deterministic across projects.
            "--ignoreConfig",
        ])
        .current_dir(project_root)
        .stdin(std::process::Stdio::null());
    let output = run_tool(&mut emit_command, TYPESCRIPT_DEADLINE, TOOL_OUTPUT_BYTES)
        .await
        .map_err(|error| {
            typescript_error(
                portable_source,
                &format!(
                    "cannot start the TypeScript compiler '{}': {error}. Install \
                     typescript 6 or newer in the project or on PATH.",
                    program.display()
                ),
            )
        })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stdout);
        let detail = if detail.trim().is_empty() {
            String::from_utf8_lossy(&output.stderr).into_owned()
        } else {
            detail.into_owned()
        };
        return Err(typescript_error(
            portable_source,
            detail.trim().get(..512).unwrap_or(detail.trim()),
        ));
    }
    // tsc mirrors the input file name into the output directory.
    let stem = source.file_stem().map_or_else(
        || String::from("tac"),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let emitted = staged.path().join(format!("{stem}.js"));
    fs::read(&emitted).map_err(|error| {
        typescript_error(
            portable_source,
            &format!("the TypeScript compiler emitted no JavaScript: {error}"),
        )
    })
}

fn typescript_error(source: &str, detail: &str) -> Failure {
    Failure::one(diagnostic(
        1009,
        format!("Cannot compile TypeScript companion '{source}': {detail}"),
        Some(String::from(
            "Tachyon emits tac.ts with the TypeScript compiler. Install \
             typescript 6 or newer as a project dependency or on PATH.",
        )),
        None,
    ))
}

fn build_config_digest(project_root: &Path) -> Result<Option<String>, Failure> {
    let config = crate::native::config_module_path(project_root)?;
    let manifest = project_root.join(crate::native::MANIFEST_NAME);
    let mut inputs = Vec::new();
    if let Some(path) = config {
        inputs.push(path);
    }
    if output_io(manifest.try_exists(), &manifest)? {
        inputs.push(manifest);
    }
    if inputs.is_empty() {
        return Ok(None);
    }
    let mut digest_input = Vec::new();
    for path in inputs {
        let metadata = output_io(fs::symlink_metadata(&path), &path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_BUILD_CONFIG_BYTES
        {
            return Err(Failure::one(diagnostic(
                1201,
                "Build configuration and manifest must be regular non-symlinked files no larger than 1 MiB.",
                None,
                None,
            )));
        }
        let bytes = output_io(fs::read(&path), &path)?;
        digest_input.extend_from_slice(path.file_name().unwrap_or_default().as_encoded_bytes());
        digest_input.push(0);
        digest_input.extend_from_slice(&bytes);
        digest_input.push(0);
    }
    Ok(Some(sha256_bytes(&digest_input)))
}

async fn run_post_bundle_hook(
    project_root: &Path,
    stage: &Path,
    target: &str,
) -> Result<(), Failure> {
    let Some(config) = crate::native::config_module_path(project_root)? else {
        return Ok(());
    };
    let configured = std::env::var_os("TAC_JAVASCRIPT_RUNTIME").map(PathBuf::from);
    let programs = configured
        .into_iter()
        .chain([PathBuf::from("node"), PathBuf::from("bun")]);
    for program in programs {
        let mut command = tokio::process::Command::new(&program);
        if config
            .extension()
            .is_some_and(|extension| extension == "ts")
            && program.file_stem().is_some_and(|name| name == "node")
        {
            command.arg("--experimental-strip-types");
        }
        command
            .args(["--input-type=module", "--eval", POST_BUNDLE_RUNNER])
            .current_dir(project_root)
            .env("TAC_CONFIG", &config)
            .env("TAC_STAGE", stage)
            .env("TAC_TARGET", target)
            .stdin(std::process::Stdio::null());
        let output = match run_tool(&mut command, Duration::from_secs(30), TOOL_OUTPUT_BYTES).await
        {
            Ok(output) => output,
            Err(ToolError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(Failure::one(diagnostic(
                    1201,
                    format!("Cannot run the tac.config.js runtime: {error}"),
                    None,
                    None,
                )));
            }
        };
        if output.status.success() {
            return Ok(());
        }
        return Err(Failure::one(diagnostic(
            1201,
            format!(
                "tac.config.js postBundle exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
            Some(String::from(
                "Fix the hook error reported above or remove postBundle.",
            )),
            None,
        )));
    }
    Err(Failure::one(diagnostic(
        1201,
        "tac.config.js needs a JavaScript runtime, but neither node nor bun is available.",
        Some(String::from(
            "Install Node.js or Bun, or set TAC_JAVASCRIPT_RUNTIME.",
        )),
        None,
    )))
}

fn digest_stage(stage: &Path) -> io::Result<String> {
    let mut pending = vec![stage.to_path_buf()];
    let mut files = Vec::new();
    while let Some(current) = pending.pop() {
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "postBundle output may not contain symbolic links",
            ));
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&current)? {
                pending.push(entry?.path());
            }
        } else if metadata.is_file() {
            if metadata.len() > MAX_SHARED_ASSET_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "postBundle output file exceeds 16 MiB",
                ));
            }
            files.push(current);
        }
    }
    if files.len() > MAX_SHARED_ASSETS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "postBundle output exceeds 4,096 files",
        ));
    }
    files.sort();
    let mut total = 0_u64;
    let mut hasher = Sha256::new();
    for path in files {
        let bytes = fs::read(&path)?;
        total = total.saturating_add(bytes.len() as u64);
        if total > MAX_SHARED_ASSET_TOTAL_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "postBundle output exceeds 64 MiB",
            ));
        }
        let relative = path.strip_prefix(stage).map_err(io::Error::other)?;
        hasher.update(portable_path(relative).as_bytes());
        hasher.update([0]);
        hasher.update(&bytes);
        hasher.update([0]);
    }
    Ok(hex_digest(hasher.finalize()))
}

fn write_stage(stage: &Path, files: &[(PathBuf, Vec<u8>)]) -> io::Result<String> {
    let mut hasher = Sha256::new();
    for (relative, contents) in files {
        let output = stage.join(relative);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&output, contents)?;
        let portable = relative.to_string_lossy().replace('\\', "/");
        hasher.update(portable.as_bytes());
        hasher.update([0]);
        hasher.update(contents);
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(encoded)
}

pub(crate) fn publish(stage: tempfile::TempDir, destination: &Path) -> io::Result<()> {
    let persisted = stage.keep();
    if !destination.exists() {
        return fs::rename(persisted, destination);
    }

    let backup = persisted.with_extension("backup");
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(&persisted, destination) {
        let _rollback = fs::rename(&backup, destination);
        let _cleanup = fs::remove_dir_all(&persisted);
        return Err(error);
    }
    if let Err(error) = fs::remove_dir_all(&backup) {
        let move_new = fs::rename(destination, &persisted);
        let restore_old = fs::rename(&backup, destination);
        if move_new.is_ok() && restore_old.is_ok() {
            let _cleanup = fs::remove_dir_all(&persisted);
        }
        return Err(error);
    }
    Ok(())
}

fn output_error(path: &Path, error: &io::Error) -> Failure {
    Failure::one(diagnostic(
        1201,
        format!("Cannot publish build output '{}': {error}", path.display()),
        Some(String::from(
            "Check directory permissions and that no process has locked the output.",
        )),
        None,
    ))
}

fn output_io<T>(result: io::Result<T>, path: &Path) -> Result<T, Failure> {
    result.map_err(|error| output_error(path, &error))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        BuildOptions, WebCompiler, output_error, output_io, rewrite_client_shared_imports,
        strip_page_state_scripts, transform_page_module, write_stage,
    };
    #[cfg(unix)]
    use crate::ProjectDiscovery;
    use std::fs;
    use std::io;
    use std::path::PathBuf;

    #[test]
    fn signal_decorators_lower_without_inline_state_and_preserve_lexical_decoys() {
        let source = br"const note = `
@publish('not-real')
export default class Fake { }
`;
export default class Counter {
  @publish('counter')
  // The field also persists.
  $value = 2;
  @subscribe('counter')
  seen = 0;
  @publish
  @onMount
  async ready() { return 3; }
}
";
        let output =
            super::prepare_component_script(source.to_vec(), "", "tac.js").expect("decorators");
        let text = String::from_utf8(output).expect("UTF-8");
        assert!(
            text.contains("@publish('not-real')"),
            "string decoy preserved"
        );
        assert!(text.contains("[\"$value\", \"counter\", \"field\"]"));
        assert!(text.contains("[\"ready\", \"ready\", \"method\"]"));
        assert!(text.contains("static __tachyonOnMount = [\"ready\"]"));
        assert!(text.contains("[\"seen\", \"counter\", \"field\"]"));
        assert!(!text.contains("@subscribe('counter')"));
    }

    #[test]
    fn invalid_signal_decorators_fail_before_emission() {
        for decorated in [
            "@publish(untrusted)",
            "@publish('bad space')",
            "@publish('.invalid-leading-dot')",
            "@subscribe('-invalid-leading-hyphen')",
            "@subscribe('x'",
            "@onMount('x')",
            "@publish('')",
        ] {
            let source = format!("export default class {{\n{decorated}\nvalue = 0;\n}}");
            assert!(
                super::prepare_component_script(source.into_bytes(), "", "tac.js").is_err(),
                "{decorated}"
            );
        }
        for member in ["static value = 0", "constructor() {}", "tac = 0"] {
            let source = format!("export default class {{\n@publish\n{member}\n}}");
            assert!(
                super::prepare_component_script(source.into_bytes(), "", "tac.js").is_err(),
                "{member}"
            );
        }
        assert!(
            super::prepare_component_script(
                b"export default class {\n@onMount\nfield = 0\n}".to_vec(),
                "",
                "tac.js"
            )
            .is_err()
        );
    }

    #[test]
    fn signal_decorators_are_emitted_for_component_and_page_companions() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        let component = root.path().join("client/components/signal/panel");
        fs::create_dir_all(&pages).expect("pages");
        fs::create_dir_all(&component).expect("component");
        fs::write(pages.join("tac.html"), "<signal-panel hydrate=\"load\" />").expect("view");
        fs::write(component.join("tac.html"), "<p>{seen}</p>").expect("view");
        let source = "export default class {\n@subscribe('counter')\nseen = 0;\n}";
        fs::write(pages.join("tac.js"), source).expect("page module");
        fs::write(component.join("tac.js"), source).expect("component module");
        WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        for name in ["client.js", ".tachyon/components/signal-panel.js"] {
            let output =
                fs::read_to_string(root.path().join("dist").join(name)).expect("emitted module");
            assert!(output.contains("static __tachyonSubscribe"), "{name}");
            assert!(!output.contains("@subscribe"), "{name}");
        }
    }

    #[test]
    fn failed_build_preserves_the_previous_output() {
        let root = tempfile::tempdir().expect("project");
        let source = root.path().join("client/pages/tac.html");
        fs::create_dir_all(source.parent().expect("parent")).expect("directory");
        fs::write(&source, "<main>Good</main>").expect("source");
        let first = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let published = fs::read(root.path().join("dist/index.html")).expect("output");

        fs::write(&source, "<if>Bad</if>").expect("invalid source");
        assert!(WebCompiler::build(root.path(), &BuildOptions::default()).is_err());
        assert_eq!(
            fs::read(root.path().join("dist/index.html")).expect("old output"),
            published
        );
        assert_eq!(first.sha256().len(), 64);
    }

    #[test]
    fn authored_shared_imports_work_from_legacy_locations_and_rust_output() {
        let root = tempfile::tempdir().expect("project");
        let source = root.path().join("client/components/site/header/tac.js");
        let emitted = rewrite_client_shared_imports(
            root.path(),
            &source,
            b"import '../../../shared/scripts/imports.js'\n".to_vec(),
        );
        assert_eq!(
            String::from_utf8(emitted).expect("UTF-8"),
            "import '/shared/scripts/imports.js'\n"
        );

        let unrelated = b"import '../../../services/client.js'\n".to_vec();
        assert_eq!(
            rewrite_client_shared_imports(root.path(), &source, unrelated.clone()),
            unrelated
        );
    }

    #[test]
    fn inline_page_state_is_literal_only_and_transformed_without_source_evaluation() {
        let (template, state) = strip_page_state_scripts(
            "<script>let count = 0; const labels = ['one', 'two']</script><p>{count}</p>",
            "client/pages/tac.html",
        )
        .expect("literal state");
        assert!(!template.contains("<script>"));
        let module = transform_page_module(b"export default class {}".to_vec(), &state);
        let module = String::from_utf8(module).expect("transformed module");
        assert!(module.contains("count = 0;"));
        assert!(module.contains(r#"labels = ["one","two"];"#));

        let failure = strip_page_state_scripts(
            "<script>let count = globalThis.alert('unsafe')</script><p>{count}</p>",
            "client/pages/tac.html",
        )
        .expect_err("executable inline state");
        assert!(failure.to_string().contains("TY1306"));
        assert!(
            failure
                .to_string()
                .contains("literal variable declarations")
        );
    }

    #[test]
    fn inline_page_state_emits_only_the_tac_client_runtime() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            pages.join("tac.html"),
            "<script>let count = 0</script><button on:click=\"count += 1\">Add</button><p>{count}</p>",
        )
        .expect("view");
        fs::write(pages.join("tac.js"), "export default class {}\n").expect("module");

        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let document =
            fs::read_to_string(result.output_directory().join("index.html")).expect("document");
        let runtime = fs::read_to_string(result.output_directory().join(".tachyon/tac-client.js"))
            .expect("Tac client runtime");

        assert!(document.contains(r#"src="/.tachyon/tac-client.js""#));
        assert!(!document.contains("tachyon-island"), "{document}");
        assert!(!document.contains("<p>0</p>"), "{document}");
        assert!(
            !result
                .output_directory()
                .join(".tachyon/islands.js")
                .exists()
        );
        assert!(runtime.contains("const renderNodes = async"));
        assert!(!runtime.contains("WebAssembly.instantiate"));
        assert!(runtime.contains("globalThis.__tc_rerender = render"));
    }

    #[test]
    fn tac_never_server_renders_and_yon_html_is_rejected() {
        let root = tempfile::tempdir().expect("project");
        let tac = root.path().join("client/pages/tac.html");
        let yon = root.path().join("server/routes/server/yon.html");
        fs::create_dir_all(tac.parent().expect("Tac parent")).expect("Tac directory");
        fs::write(
            &tac,
            "<script>let show = false</script><if :when=\"show\"><p>Tac true</p></if><else><p>Tac false</p></else>",
        )
        .expect("Tac view");
        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let tac = fs::read_to_string(result.output_directory().join("index.html")).expect("Tac");
        let tac_body = tac.split_once("<body>").expect("Tac body").1;

        assert!(tac.contains(r#""branch":"if""#), "{tac}");
        assert!(tac.contains(r#""branch":"else""#), "{tac}");
        assert!(!tac_body.contains("Tac true"), "{tac_body}");
        assert!(!tac_body.contains("Tac false"), "{tac_body}");
        fs::create_dir_all(yon.parent().expect("Yon parent")).expect("Yon directory");
        fs::write(&yon, "<main>Yon rendered</main>").expect("Yon view");
        let failure = WebCompiler::build(root.path(), &BuildOptions::default())
            .expect_err("yon.html must not compile");
        assert!(failure.to_string().contains("TY1008"));
        assert!(failure.to_string().contains("Content-Type: text/html"));
    }

    #[test]
    fn unsafe_output_paths_are_rejected() {
        let root = tempfile::tempdir().expect("project");
        let source = root.path().join("client/pages/tac.html");
        fs::create_dir_all(source.parent().expect("parent")).expect("directory");
        fs::write(source, "<main>Good</main>").expect("source");

        for output in ["../out", "client/out", ".git/out", "dist/../out", ""] {
            let options = BuildOptions {
                output_directory: PathBuf::from(output),
                ..BuildOptions::default()
            };
            let error = WebCompiler::build(root.path(), &options).expect_err("unsafe output");
            assert!(error.to_string().contains("TY1202"));
        }

        let absolute = root.path().join("published");
        let result = WebCompiler::build(
            root.path(),
            &BuildOptions {
                output_directory: absolute.clone(),
                ..BuildOptions::default()
            },
        )
        .expect("explicit absolute output");
        assert_eq!(
            result.output_directory(),
            fs::canonicalize(root.path())
                .expect("temporary project root should canonicalize")
                .join("published")
        );
    }

    #[test]
    fn nested_output_is_supported_and_output_files_are_rejected() {
        let root = tempfile::tempdir().expect("project");
        let source = root.path().join("client/pages/tac.html");
        fs::create_dir_all(source.parent().expect("parent")).expect("directory");
        fs::write(source, "<main>Good</main>").expect("source");
        let nested = BuildOptions {
            output_directory: PathBuf::from("build/site"),
            ..BuildOptions::default()
        };
        assert!(WebCompiler::build(root.path(), &nested).is_ok());

        fs::write(root.path().join("blocked"), "file").expect("blocked output");
        let blocked = BuildOptions {
            output_directory: PathBuf::from("blocked"),
            ..BuildOptions::default()
        };
        assert!(
            WebCompiler::build(root.path(), &blocked)
                .expect_err("file output")
                .to_string()
                .contains("TY1202")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_output_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("project");
        let source = root.path().join("client/pages/tac.html");
        fs::create_dir_all(source.parent().expect("parent")).expect("directory");
        fs::write(source, "<main>Good</main>").expect("source");
        let outside = tempfile::tempdir().expect("outside");
        symlink(outside.path(), root.path().join("dist")).expect("symlink");
        let error =
            WebCompiler::build(root.path(), &BuildOptions::default()).expect_err("symlink output");
        assert!(error.to_string().contains("TY1202"));
    }

    #[test]
    fn internal_io_failures_are_actionable() {
        let root = tempfile::tempdir().expect("workspace");
        let file = root.path().join("file");
        fs::write(&file, "occupied").expect("file");
        let error = write_stage(&file, &[(PathBuf::from("nested/index.html"), vec![1])])
            .expect_err("stage under file");
        assert!(output_error(&file, &error).to_string().contains("TY1201"));
        assert!(matches!(
            error.kind(),
            io::ErrorKind::NotADirectory | io::ErrorKind::AlreadyExists
        ));
        assert!(output_io::<()>(Err(error), &file).is_err());
    }

    #[test]
    fn emitted_route_manifest_satisfies_the_canonical_schema() {
        let root = tempfile::tempdir().expect("project");
        let source = root.path().join("client/pages/tac.html");
        fs::create_dir_all(source.parent().expect("parent")).expect("directory");
        fs::write(source, "<main>Good</main>").expect("source");
        WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(root.path().join("dist/route-manifest.json")).expect("manifest"),
        )
        .expect("JSON");
        let contract = tachyon_contracts::find("route-manifest").expect("contract");
        let schema = tachyon_contracts::parse_schema(contract).expect("schema");
        let validator = jsonschema::validator_for(&schema).expect("validator");
        assert!(validator.is_valid(&manifest));
    }

    #[test]
    fn companions_are_emitted_linked_reused_and_invalidated() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(pages.join("about")).expect("pages");
        fs::write(pages.join("tac.html"), "<main>Home</main>").expect("view");
        fs::write(pages.join("tac.css"), "h1{color:red}").expect("style");
        fs::write(pages.join("tac.js"), "globalThis.ready = 1\n").expect("module");
        fs::write(pages.join("about/tac.html"), "<main>About</main>").expect("view");
        fs::write(pages.join("about/tac.css"), "h2{color:teal}").expect("style");

        let options = BuildOptions::default();
        let first = WebCompiler::build(root.path(), &options).expect("first build");
        let dist = first.output_directory().to_path_buf();

        // Each companion is emitted beside the route that owns it.
        assert_eq!(
            fs::read_to_string(dist.join("style.css")).expect("style"),
            "h1{color:red}"
        );
        assert_eq!(
            fs::read_to_string(dist.join("about/style.css")).expect("nested style"),
            "h2{color:teal}"
        );
        assert!(dist.join("client.js").is_file());

        // The document references them.
        let index = fs::read_to_string(dist.join("index.html")).expect("index");
        assert!(index.contains(r#"href="/style.css""#), "{index}");
        assert!(index.contains(r#""module":"/client.js""#), "{index}");
        assert!(!index.contains(r#"src="/client.js""#), "{index}");

        // A reused route must not lose its companions.
        let reused = WebCompiler::build(root.path(), &options).expect("second build");
        assert_eq!(reused.reused_routes(), 2);
        assert!(
            dist.join("style.css").is_file(),
            "reuse dropped the stylesheet"
        );
        assert!(dist.join("client.js").is_file(), "reuse dropped the module");

        // Changing only a companion must invalidate its route.
        fs::write(pages.join("tac.css"), "h1{color:blue}").expect("style");
        let changed = WebCompiler::build(root.path(), &options).expect("third build");
        assert_eq!(
            changed.compiled_routes(),
            1,
            "a companion change was ignored"
        );
        assert_eq!(
            fs::read_to_string(dist.join("style.css")).expect("style"),
            "h1{color:blue}"
        );
    }

    #[test]
    fn dynamic_templates_and_post_bundle_hooks_publish_atomically() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let root = tempfile::tempdir().expect("project");
        let page = root.path().join("client/pages/items/_id/tac.html");
        fs::create_dir_all(page.parent().expect("parent")).expect("pages");
        fs::write(&page, "<main>Dynamic template</main>").expect("view");
        fs::write(root.path().join("package.json"), r#"{"type":"module"}"#).expect("package");
        fs::write(
            root.path().join("tac.config.js"),
            "import { writeFile } from 'node:fs/promises'\n\
             export async function postBundle({ targetRoots }) {\n\
               await writeFile(`${targetRoots.web}/hook.txt`, 'complete')\n\
             }\n",
        )
        .expect("config");

        let built = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        assert!(
            built
                .output_directory()
                .join("items/_id/index.html")
                .is_file()
        );
        assert_eq!(
            fs::read_to_string(built.output_directory().join("hook.txt")).expect("hook output"),
            "complete"
        );
        assert_eq!(built.sha256().len(), 64);
    }

    /// Returns whether a TypeScript compiler new enough to emit is reachable.
    fn typescript_available() -> bool {
        std::process::Command::new("tsc")
            .arg("--version")
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .last()
                    .and_then(|value| value.split('.').next())
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_some_and(|major| major >= super::MINIMUM_TYPESCRIPT_MAJOR)
            })
    }

    #[test]
    fn a_typescript_companion_is_emitted_through_the_typescript_compiler() {
        if !typescript_available() {
            // The compiler is an external toolchain, like node or swiftc. Its
            // absence is covered by `a_missing_typescript_compiler_fails_closed`.
            return;
        }
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(pages.join("tac.html"), "<main>Home</main>").expect("view");
        fs::write(
            pages.join("tac.ts"),
            "interface P { a: number }\nenum E { On = 1 }\n\
             export const f = (p: P): number => p.a + E.On\n",
        )
        .expect("module");

        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let emitted = fs::read_to_string(result.output_directory().join("client.js"))
            .expect("emitted module");
        // Types are erased and an enum is desugared to its runtime form.
        assert!(!emitted.contains("interface"), "{emitted}");
        assert!(!emitted.contains(": number"), "{emitted}");
        assert!(emitted.contains("E[E[\"On\"] = 1]"), "{emitted}");

        let index =
            fs::read_to_string(result.output_directory().join("index.html")).expect("index");
        assert!(index.contains(r#""module":"/client.js""#));
        assert!(!index.contains(r#"src="/client.js""#));
    }

    #[test]
    fn an_absent_or_outdated_typescript_compiler_fails_closed() {
        // A missing compiler, or one older than 7, must report TY1009 and name
        // the remedy, never emit TypeScript as if it were JavaScript.
        if typescript_available() {
            return;
        }
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(pages.join("tac.html"), "<main>Home</main>").expect("view");
        fs::write(pages.join("tac.ts"), "export const x: number = 1").expect("module");
        let error = WebCompiler::build(root.path(), &BuildOptions::default())
            .expect_err("missing compiler");
        assert!(error.to_string().contains("TY1009"), "{error}");
    }

    #[test]
    fn component_stylesheets_are_scoped_to_their_own_component() {
        // A page-level tac.css worked while a component tac.css failed, because
        // only the page case was ever probed. Both are covered now.
        let root = tempfile::tempdir().expect("project");
        let write = |relative: &str, contents: &str| {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, contents).expect("source");
        };
        write(
            "client/pages/tac.html",
            r#"<main aria-label="T"><product-card><p>s</p></product-card></main>"#,
        );
        write(
            "client/components/product/card/tac.html",
            r#"<article aria-label="C"><slot></slot></article>"#,
        );
        write(
            "client/components/product/card/tac.css",
            "article { color: rgb(0, 128, 0) }",
        );

        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let dist = result.output_directory();

        // CSS @scope is the platform's own scoping mechanism, so the rule names
        // the attribute rather than a rewritten selector.
        let stylesheet =
            fs::read_to_string(dist.join(".tachyon/components.css")).expect("stylesheet");
        assert!(
            stylesheet.contains(r#"@scope ([data-tac-scope="product-card"])"#),
            "{stylesheet}"
        );
        assert!(stylesheet.contains("color: rgb(0, 128, 0)"), "{stylesheet}");

        let document = fs::read_to_string(dist.join("index.html")).expect("document");
        // Scoping is part of the browser plan, not server-rendered markup.
        assert!(document.contains(r#""scope":true"#), "{document}");
        assert!(!document.contains("<article"), "{document}");
        assert!(document.contains(super::COMPONENT_STYLE_LINK), "{document}");
    }

    #[test]
    fn mounted_component_plan_carries_the_style_scope() {
        let root = tempfile::tempdir().expect("project");
        let write = |relative: &str, contents: &str| {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, contents).expect("source");
        };
        write(
            "client/pages/tac.html",
            r#"<main><product-card hydrate="load" /></main>"#,
        );
        write(
            "client/components/product/card/tac.html",
            r#"<article class="card">Product</article>"#,
        );
        write(
            "client/components/product/card/tac.css",
            ".card { display: grid }",
        );
        write(
            "client/components/product/card/tac.js",
            "export default class ProductCard {}",
        );

        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let document =
            fs::read_to_string(result.output_directory().join("index.html")).expect("document");
        assert!(document.contains(r#""mount":"load""#), "{document}");
        assert!(document.contains(r#""scope":true"#), "{document}");
        assert!(!document.contains("<article"), "{document}");
        assert!(!document.contains("data-tachyon-hydrate"), "{document}");
    }

    #[test]
    fn a_project_without_component_styles_emits_neither_file_nor_link() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            pages.join("tac.html"),
            r#"<main aria-label="T"><h1>T</h1></main>"#,
        )
        .expect("view");

        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let dist = result.output_directory();
        assert!(!dist.join(".tachyon/components.css").exists());
        let document = fs::read_to_string(dist.join("index.html")).expect("document");
        assert!(!document.contains(super::COMPONENT_STYLE_LINK));
    }

    #[test]
    fn event_bindings_emit_markers_and_the_delegated_runtime() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            pages.join("tac.html"),
            r#"<main><button on:click="increment()">Add</button>
               <input on:input="rename()"></main>"#,
        )
        .expect("view");
        fs::write(pages.join("tac.js"), "export const increment = () => {}\n").expect("module");

        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let dist = result.output_directory();
        let index = fs::read_to_string(dist.join("index.html")).expect("index");

        // The authored `on:` syntax becomes data in the client render plan,
        // never an executable HTML attribute or server-rendered marker.
        assert!(!index.contains("on:click"), "{index}");
        assert!(index.contains(r#""eventType":"click""#), "{index}");
        assert!(index.contains(r#""eventType":"input""#), "{index}");
        assert!(!dist.join(".tachyon/events.js").exists());
        let runtime = fs::read_to_string(dist.join(".tachyon/tac-client.js")).expect("runtime");
        assert!(runtime.contains("const bindEvent ="), "{runtime}");

        let reused = WebCompiler::build(root.path(), &BuildOptions::default()).expect("reuse");
        assert_eq!(reused.reused_routes(), 1);
        assert!(dist.join(".tachyon/tac-client.js").is_file());
    }

    #[test]
    fn a_route_binding_events_without_a_client_module_fails_closed() {
        // A handler name that no module can export is an authoring error, and
        // silently emitting a dead marker is what this replaces.
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            pages.join("tac.html"),
            r#"<main><button on:click="increment()">Add</button></main>"#,
        )
        .expect("view");
        let error = WebCompiler::build(root.path(), &BuildOptions::default())
            .expect_err("missing client module");
        assert!(error.to_string().contains("TY1306"), "{error}");
    }

    #[test]
    fn a_route_without_events_emits_no_event_runtime() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(pages.join("tac.html"), "<main>Plain</main>").expect("view");
        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        assert!(
            !result
                .output_directory()
                .join(".tachyon/events.js")
                .exists()
        );
    }

    #[test]
    fn generated_pages_opt_into_platform_navigation() {
        // Client rendering and browser-native navigation work together.
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(pages.join("about")).expect("pages");
        fs::write(
            pages.join("tac.html"),
            r#"<main><a href="/about">A</a></main>"#,
        )
        .expect("view");
        fs::write(pages.join("about/tac.html"), "<main>About</main>").expect("view");

        let result = WebCompiler::build(root.path(), &BuildOptions::default()).expect("build");
        let dist = result.output_directory();
        let stylesheet =
            fs::read_to_string(dist.join(".tachyon/navigation.css")).expect("stylesheet");
        assert!(stylesheet.contains("@view-transition"), "{stylesheet}");
        assert!(
            stylesheet.contains("tachyon-component, tachyon-island { display: block; }"),
            "{stylesheet}"
        );

        for route in ["index.html", "about/index.html"] {
            let document = fs::read_to_string(dist.join(route)).expect(route);
            assert!(
                document.contains(super::NAVIGATION_LINK),
                "{route}: {document}"
            );
            assert!(
                document.contains(r#"<script type="speculationrules" data-tachyon-runtime>"#),
                "{route}: {document}"
            );
            // The rules payload is JSON, never executable script.
            assert!(document.contains(r#""prefetch""#), "{route}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn public_build_from_project_uses_only_the_owned_component_asset_and_config_snapshot() {
        use std::os::unix::fs::symlink;

        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let workspace = tempfile::tempdir().expect("workspace");
        let authored = workspace.path().join("project");
        let write = |root: &std::path::Path, relative: &str, contents: &str| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, contents).expect("source");
        };
        write(
            &authored,
            "client/pages/tac.html",
            r#"<main><owned-card hydrate="load" /></main>"#,
        );
        write(
            &authored,
            "client/components/owned/card/tac.html",
            "<section>owned component snapshot</section>",
        );
        write(
            &authored,
            "client/components/owned/card/tac.js",
            "export default class OwnedCard { static origin = 'owned-script' }\n",
        );
        write(&authored, "client/shared/origin.txt", "owned-shared");
        write(&authored, "package.json", r#"{"type":"module"}"#);
        write(
            &authored,
            "tac.config.js",
            "import { writeFile } from 'node:fs/promises'\nexport async function postBundle({ targetRoots }) { await writeFile(`${targetRoots.web}/hook.txt`, 'owned-config') }\n",
        );
        let project = ProjectDiscovery::discover(&authored).expect("owned snapshot");

        let retained = workspace.path().join("retained");
        fs::rename(&authored, &retained).expect("move project");
        let planted = tempfile::tempdir().expect("planted");
        write(
            planted.path(),
            "client/pages/tac.html",
            "<main>planted page canary</main>",
        );
        write(
            planted.path(),
            "client/components/owned/card/tac.html",
            "<section>planted component canary</section>",
        );
        write(
            planted.path(),
            "client/components/owned/card/tac.js",
            "export default class Planted { static origin = 'planted-script' }\n",
        );
        write(planted.path(), "client/shared/origin.txt", "planted-shared");
        write(planted.path(), "package.json", r#"{"type":"module"}"#);
        write(
            planted.path(),
            "tac.config.js",
            "import { writeFile } from 'node:fs/promises'\nexport async function postBundle({ targetRoots }) { await writeFile(`${targetRoots.web}/hook.txt`, 'planted-config') }\n",
        );
        symlink(planted.path(), &authored).expect("ambient replacement");

        let output = workspace.path().join("web-output");
        let built = WebCompiler::build_project(
            &project,
            &BuildOptions {
                output_directory: output,
                incremental: false,
            },
        )
        .expect("snapshot build");
        let root = built.output_directory();
        let index = fs::read_to_string(root.join("index.html")).expect("index");
        let component = fs::read_to_string(root.join(".tachyon/components/owned-card.js"))
            .expect("component script");
        assert!(index.contains("owned component snapshot"), "{index}");
        assert!(!index.contains("planted"), "{index}");
        assert!(component.contains("owned-script"), "{component}");
        assert!(!component.contains("planted"), "{component}");
        assert_eq!(
            fs::read_to_string(root.join("shared/origin.txt")).expect("shared asset"),
            "owned-shared"
        );
        assert_eq!(
            fs::read_to_string(root.join("hook.txt")).expect("config hook"),
            "owned-config"
        );
    }
}
