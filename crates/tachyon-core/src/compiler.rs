mod api_baseline;
mod build_options;
mod build_profile;
mod build_result;
mod build_stage;
mod build_state;
mod captured_browser_behavior;
mod component_script;
mod component_style_boundary;
#[cfg(test)]
mod component_style_boundary_tests;
mod component_styles;
mod content_digest;
mod content_security_policy;
#[cfg(test)]
mod csharp_task_model_tests;
mod css_escape;
mod css_imports;
mod css_problem;
mod css_rule;
mod css_rules;
mod css_token;
mod css_tokens;
mod css_url;
mod element_declaration;
mod element_events;
mod exported_elements;
mod foundation_colors;
mod foundation_declarations;
mod foundation_priority;
mod foundation_properties;
mod foundation_rules;
mod foundation_selectors;
mod foundation_values;
mod module_code;
mod output_path;
mod page_composition;
mod page_head;
#[cfg(test)]
mod page_head_tests;
mod page_state;
mod portable_path;
mod precompression;
mod route_budget;
mod route_build_state;
mod script_assets;
#[cfg(test)]
mod script_assets_tests;
pub(crate) mod script_import;
pub(crate) mod script_imports;
mod service_checks;
mod service_script;
mod service_worker;
mod setup_class;
mod setup_registrations;
mod setup_sharing;
#[cfg(test)]
mod setup_sharing_tests;
mod shared_assets;
pub(crate) mod shared_scripts;
#[cfg(test)]
mod shared_scripts_tests;
mod site_build;
mod speculation_rules;
mod style_file;
mod style_path;
mod style_sources;
#[cfg(test)]
mod style_sources_tests;
#[cfg(test)]
mod tac_component_scope_tests;
#[cfg(test)]
mod tac_source_convention_tests;
#[cfg(test)]
mod tac_utility_import_tests;
mod tool_build;
mod typescript_cache;
mod typescript_compiler;
mod typescript_field_type;
pub(crate) mod utility_module;
#[cfg(test)]
mod utility_module_tests;
mod view_transition_guard;

pub use build_options::BuildOptions;
pub use build_profile::BuildProfile;
pub use build_result::BuildResult;
pub use precompression::CompressionTotals;
pub(crate) use route_budget::BudgetCheck;
pub(crate) use tool_build::ToolBuild;

use crate::failure::diagnostic;
use crate::{Failure, Project, ProjectDiscovery};
pub use api_baseline::ApiBaselineCheck;
pub(crate) use build_stage::BuildStage;
use build_state::BuildState;
pub(crate) use content_digest::ContentDigest;
pub(crate) use content_security_policy::ContentSecurityPolicy;
pub(crate) use output_path::OutputPath;
pub(crate) use page_head::PageHead;
pub(crate) use portable_path::PortablePath;
use service_worker::ServiceWorker;
use site_build::SiteBuild;
pub(crate) use speculation_rules::SpeculationRules;
use std::io;
use std::path::{Path, PathBuf};
pub(crate) use style_path::StylePath;
use style_sources::StyleSources;
pub(crate) use typescript_compiler::TypeScriptCompiler;
pub(crate) use view_transition_guard::ViewTransitionGuard;

#[derive(Clone, Copy, Debug, Default)]
pub struct WebCompiler;

impl WebCompiler {
    const MINIMUM_TYPESCRIPT_MAJOR: u32 = 6;
    const NAVIGATION_STYLESHEET: &str = "@view-transition { navigation: auto; }\n\
tachyon-component, tachyon-island { display: block; }\n";
    const NAVIGATION_LINK: &str =
        r#"<link rel="stylesheet" href="/.tachyon/navigation.css" data-tachyon-runtime>"#;
    const COMPONENT_STYLE_LINK: &str =
        r#"<link rel="stylesheet" href="/.tachyon/components.css" data-tachyon-runtime>"#;
    const SERVICE_WORKER: &str = r"const VERSION = '__VERSION__'
const CACHE = 'tachyon-static-' + VERSION
const PREFIX = 'tachyon-static-'
// Policies the project declared by path in client/manifest.json, compiled to anchored
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
  const revalidate = !asset && request.cache === 'no-cache'
  const cacheFirst = !revalidate && (rule ? rule.policy === 'cache-first' : request.mode !== 'navigate')
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
    if (asset && !await matchesAsset(await bodyBytes(response.clone(), asset.bodyLimit ?? 4 * 1024 * 1024), asset)) {
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
    const bytes = await bodyBytes(response.clone(), asset ? (asset.bodyLimit ?? 4 * 1024 * 1024) : 256 * 1024)
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

    const SERVICE_WORKER_LINK: &str =
        r#"<script type="module" src="/.tachyon/register-sw.js" data-tachyon-runtime></script>"#;

    const COMPATIBILITY_SPA_RUNTIME: &str = r"window.__tc_rerender ??= async () => {
  window.dispatchEvent(new CustomEvent('tachyon:rerender'))
}
";
}

impl WebCompiler {
    pub(crate) fn native_worker_registration() -> &'static str {
        Self::SERVICE_WORKER_REGISTRATION
    }

    #[must_use]
    pub fn has_previous_build(output_directory: &Path) -> bool {
        BuildState::load(output_directory).is_some()
    }

    pub fn build(
        project_root: impl AsRef<Path>,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure> {
        let project = ProjectDiscovery::discover(project_root)?;
        Self::build_project(&project, options)
    }

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

    pub async fn build_async(
        project_root: impl AsRef<Path>,
        options: &BuildOptions,
    ) -> Result<BuildResult, Failure> {
        let project = ProjectDiscovery::discover(project_root)?;
        Self::build_project_async(&project, options).await
    }

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

    async fn build_project_with_target(
        project: &Project,
        options: &BuildOptions,
        native_target: Option<tachyon_contracts::NativeTarget>,
    ) -> Result<BuildResult, Failure> {
        let types_root = Self::composed_types_root(project, native_target);
        crate::ComposedTypes::write(types_root)?;
        let mut build = SiteBuild::discover(project, options, native_target)?;
        let site = crate::native::SiteSettings::load(build.snapshot_root)?;
        StyleSources::validate(build.snapshot_root, &site)?;
        let cache_rules = ServiceWorker::cache_rules_literal(&site.cache_rules)?;
        let programs = build.compile_route_programs()?;
        let previous = options
            .incremental
            .then(|| BuildState::load(&build.output_directory))
            .flatten();
        build.finish_phase("discovery");
        let manifest = project.route_graph().manifest();
        build.add_route_files(&programs, previous.as_ref(), &site)?;
        build.finish_phase("routes");
        let needs_static_files =
            Self::needs_static_files(!programs.is_empty(), !site.elements.is_empty());
        build.add_static_files(needs_static_files)?;
        build.add_element_files(&site.elements)?;
        build.finish_phase("static files");
        build.add_service_files().await?;
        build.finish_phase(SiteBuild::SERVICES_PHASE);
        build.add_manifest_and_worker_files(&manifest, &cache_rules)?;
        build.add_site_map_files(&site)?;
        build.finish_phase("manifest and service worker");
        build.write_output()
    }

    fn needs_static_files(has_routes: bool, has_elements: bool) -> bool {
        has_routes || has_elements
    }

    fn composed_types_root(
        project: &Project,
        native_target: Option<tachyon_contracts::NativeTarget>,
    ) -> &Path {
        if native_target.is_some() {
            return project.snapshot_root();
        }
        project.root()
    }
}

pub(crate) fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    ContentDigest::hex(bytes)
}

pub(crate) fn portable_path(path: &Path) -> String {
    PortablePath::of(path)
}

pub(crate) fn html_attribute_escape(value: &str) -> String {
    PageHead::escape(value)
}

pub(crate) fn resolve_output_path(root: &Path, path: &Path) -> Result<PathBuf, Failure> {
    OutputPath::resolve(root, path)
}

pub(crate) fn publish(stage: tempfile::TempDir, destination: &Path) -> io::Result<()> {
    BuildStage::publish(stage, destination)
}

#[cfg(test)]
mod web_api_tests;

#[cfg(test)]
mod model_script_tests;

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::build_stage::BuildStage;
    use super::component_script::ComponentScript;
    use super::page_state::PageState;
    use super::script_assets::ScriptAssets;
    use super::service_checks::ServiceChecks;
    use super::typescript_cache::TypeScriptCache;
    use super::typescript_compiler::TypeScriptCompiler;
    use super::{BuildOptions, BuildResult, WebCompiler};
    use crate::Failure;
    #[cfg(unix)]
    use crate::ProjectDiscovery;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, contents).expect("source");
    }

    #[test]
    fn browser_service_checks_read_a_rust_schema_model() {
        let root = tempfile::tempdir().expect("project");
        let service = "#[Service]\nstruct OrderService { status: String }\nimpl OrderService {\n    fn new() -> Self { Self { status: String::from(\"ready\") } }\n    fn accept(&self, #[Check(OrderInput)] order: BehaviorValue) -> BehaviorResult<BehaviorValue> { Ok(order) }\n}\n";
        write_file(root.path(), "client/services/OrderService.rs", service);
        write_file(
            root.path(),
            "client/models/OrderInput.rs",
            "#[Checked]\npub struct OrderInput {\n    #[NotEmpty]\n    #[Length({\"min\":1,\"max\":80})]\n    pub name: String,\n    #[Required]\n    #[Min(1)]\n    #[Max(9)]\n    pub level: f64,\n}\n",
        );
        let path = Path::new("client/services/OrderService.rs");
        let roles = crate::behavior::ServiceRoles::read(path, service).expect("owner role");
        ServiceChecks::check_source(root.path(), path, &roles.source, true)
            .unwrap_or_else(|failure| panic!("{failure}"));
    }

    #[test]
    fn a_go_helper_failure_names_the_helper_file() {
        let root = tempfile::tempdir().expect("project");
        let service = "// @Service\ntype TallyService struct{}\n";
        write_file(root.path(), "client/services/TallyService.go", service);
        write_file(
            root.path(),
            "client/helpers/StepHelper.go",
            "\n// @Helper\ntype StepHelper struct{}\n",
        );
        let path = Path::new("client/services/TallyService.go");
        let roles = crate::behavior::ServiceRoles::read(path, service).expect("owner role");
        let failure = ServiceChecks::check_native_source(root.path(), path, &roles.source)
            .expect_err("Helper is unavailable in Go");
        let diagnostic = &failure.diagnostics()[0];
        assert!(diagnostic.message.contains("not available"), "{failure}");
        assert_eq!(diagnostic.spans[0].file, "client/helpers/StepHelper.go");
        assert_eq!(diagnostic.spans[0].start, 4);
    }

    fn helper_failure(service: &str, helper: &str, extension: &str) -> Failure {
        let root = tempfile::tempdir().expect("project");
        let service_path = format!("client/services/TallyService.{extension}");
        write_file(root.path(), &service_path, service);
        let helper_path = format!("client/helpers/StepHelper.{extension}");
        write_file(root.path(), &helper_path, helper);
        let path = Path::new(&service_path);
        let roles = crate::behavior::ServiceRoles::read(path, service).expect("owner role");
        ServiceChecks::check_native_source(root.path(), path, &roles.source)
            .expect_err("Helper is unavailable in this compiled language")
    }

    #[test]
    fn a_rust_helper_is_supported_at_its_own_file() {
        let root = tempfile::tempdir().expect("project");
        let service = "#[Service]\nstruct TallyService {}\n";
        write_file(root.path(), "client/services/TallyService.rs", service);
        write_file(
            root.path(),
            "client/helpers/StepHelper.rs",
            "#[Helper]\nstruct StepHelper {}\n",
        );
        let path = Path::new("client/services/TallyService.rs");
        let roles = crate::behavior::ServiceRoles::read(path, service).expect("owner role");
        ServiceChecks::check_native_source(root.path(), path, &roles.source)
            .expect("Rust Helper source is supported");
    }

    #[test]
    fn a_csharp_helper_failure_names_the_helper_file() {
        let failure = helper_failure(
            "using System;\n[Service]\nclass TallyService {}\n",
            "using System;\n[Helper]\nclass StepHelper {}\n",
            "cs",
        );
        let diagnostic = &failure.diagnostics()[0];
        assert_eq!(diagnostic.code.to_string(), "TY1308");
        assert!(
            diagnostic
                .message
                .contains("explicit caller-owned construction"),
            "{failure}"
        );
        assert_eq!(diagnostic.spans[0].file, "client/helpers/StepHelper.cs");
        assert_eq!(diagnostic.spans[0].start, 14);
    }

    #[test]
    fn a_kotlin_helper_failure_names_the_helper_file() {
        let service = "@Service\nclass TallyService\n";
        let helper = "\n@Helper\nclass StepHelper {}\n";
        let failure = helper_failure(service, helper, "kt");
        let diagnostic = &failure.diagnostics()[0];
        assert!(diagnostic.message.contains("not available"), "{failure}");
        assert_eq!(
            diagnostic.spans[0].file, "client/helpers/StepHelper.kt",
            "{failure}"
        );
        assert_eq!(diagnostic.spans[0].start, 1, "{failure}");
    }

    fn compose(root: &Path, page: &str, name: &str, markup: &str) {
        let pages = root.join("client/pages").join(page);
        fs::create_dir_all(&pages).expect("page directory");
        fs::write(pages.join("tac.html"), format!("<page-{name} />")).expect("page");
        let component = root.join("client/components/page").join(name);
        fs::create_dir_all(&component).expect("component directory");
        fs::write(component.join("tac.html"), markup).expect("component view");
    }

    fn service(root: &Path, class: &str, body: &str) {
        let services = root.join("client/services");
        fs::create_dir_all(&services).expect("services directory");
        fs::write(
            services.join(format!("{class}.js")),
            format!("@Service()\nexport default class {class} {body}\n"),
        )
        .expect("service");
    }

    fn hydrate_page_view(root: &Path) {
        fs::write(
            root.join("client/pages/tac.html"),
            "<page-view hydrate=\"load\" />",
        )
        .expect("page");
    }

    pub(super) fn build_default(root: &Path) -> Result<BuildResult, Failure> {
        WebCompiler::build(root, &BuildOptions::default())
    }

    fn typescript_available() -> bool {
        let Ok(output) = std::process::Command::new("tsc").arg("--version").output() else {
            return false;
        };
        let banner = String::from_utf8_lossy(&output.stdout);
        TypeScriptCompiler::major_version(&banner) >= WebCompiler::MINIMUM_TYPESCRIPT_MAJOR
    }

    #[test]
    fn tac_preserves_unannotated_plain_methods() {
        let source = br"const note = `@publish('not-code')`;
export default class Counter {
  value = 2;
  increment() { this.value += 1; }
}
";
        let output = ComponentScript::prepare(Path::new("/nonexistent"), source, "tac.js")
            .expect("plain companion");
        assert_eq!(output, source);
    }

    #[test]
    fn retired_tac_decorators_fail_with_migration_guidance() {
        for decorator in ["@onMount", "@publish", "@subscribe('counter')"] {
            let source = format!("export default class {{\n{decorator}\nvalue = 0;\n}}");
            let failure =
                ComponentScript::prepare(Path::new("/nonexistent"), source.as_bytes(), "tac.js")
                    .expect_err("retired decorator");
            let rendered = failure.to_string();
            assert!(rendered.contains("TY1306"), "{rendered}");
            assert!(rendered.contains("OnStart"), "{rendered}");
        }
    }

    #[test]
    fn a_view_that_writes_the_document_fails_the_build() {
        let root = tempfile::tempdir().expect("project");
        compose(
            root.path(),
            "",
            "view",
            "<!doctype html><html><body><p>Hi</p></body></html>",
        );
        let rendered = build_default(root.path())
            .expect_err("document view")
            .to_string();
        assert!(rendered.contains("TY1307"), "{rendered}");
        assert!(
            rendered.contains("client/components/page/view/tac.html"),
            "{rendered}"
        );

        compose(root.path(), "", "view", "<p>Hi</p>");
        fs::write(
            root.path().join("client/pages/tac.html"),
            "<head><title>Home</title></head><page-view />",
        )
        .expect("page");
        let rendered = build_default(root.path())
            .expect_err("document page")
            .to_string();
        assert!(rendered.contains("TY1307"), "{rendered}");
        assert!(rendered.contains("client/pages/tac.html"), "{rendered}");
    }

    #[test]
    fn failed_build_preserves_the_previous_output() {
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "view", "<main>Good</main>");
        let source = root.path().join("client/components/page/view/tac.html");
        let first = build_default(root.path()).expect("build");
        let published = fs::read(root.path().join("dist/index.html")).expect("output");

        fs::write(&source, "<if>Bad</if>").expect("invalid source");
        assert!(build_default(root.path()).is_err());
        assert_eq!(
            fs::read(root.path().join("dist/index.html")).expect("old output"),
            published
        );
        assert_eq!(first.sha256().len(), 64);
    }

    #[test]
    fn authored_shared_imports_rebase_to_the_emitted_script_location() {
        let root = tempfile::tempdir().expect("project");
        write_file(
            root.path(),
            "client/shared/scripts/imports.js",
            "export class Imports {}",
        );
        let source = root.path().join("client/components/site/header/tac.js");
        let output = Path::new(".tachyon/services/HeaderService.js");
        let mut assets = ScriptAssets::new(root.path(), root.path());
        let emitted = assets
            .rewrite(
                &source,
                output,
                b"import '../../../shared/scripts/imports.js'\n".to_vec(),
            )
            .expect("shared dependency");
        assert_eq!(
            String::from_utf8(emitted).expect("UTF-8"),
            "import '../../shared/scripts/imports.js'\n"
        );

        let unrelated = b"import '../../../services/client.js'\n".to_vec();
        assert_eq!(
            assets
                .rewrite(&source, output, unrelated.clone())
                .expect("unrelated import"),
            unrelated
        );
    }

    #[test]
    fn inline_page_state_is_literal_only() {
        let (template, _) = PageState::strip_scripts(
            "<script>let count = 0; const labels = ['one', 'two']</script><p>{count}</p>",
            "client/pages/tac.html",
        )
        .expect("literal state");
        assert!(!template.contains("<script>"));

        let failure = PageState::strip_scripts(
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
    fn component_state_emits_only_the_tac_client_runtime() {
        let root = tempfile::tempdir().expect("project");
        compose(
            root.path(),
            "",
            "view",
            "<button on:click=\"view.count += 1\">Add</button><p>{view.count}</p>",
        );
        hydrate_page_view(root.path());
        service(root.path(), "ViewService", "{ count = 0 }");

        let result = build_default(root.path()).expect("build");
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

    fn basket_service_source() -> &'static str {
        r#"
/**
 * @typedef {Object} BasketAdded
 * @property {string} id
 * @property {number} total
 */
/**
 * @typedef {Object} BasketSynced
 * @property {number} total
 */
@Service()
export default class BasketService {
  applicationEventPublisher;
  total = 0;
  /** @param {BasketAdded} item */
  add(item) { this.applicationEventPublisher.publishEvent(item, true); }
  /**
   * @param {BasketAdded} value
   * @returns {BasketSynced}
   */
  @Order(-5)
  @OnEvent({"replay":true,"id":"basket-sync","condition":"value.total >= 10"})
  sync(value) { return {total: value.total}; }
  /** @param {BasketSynced} value */
  @InBackground()
  @OnEvent
  async report(value) { this.total = value.total; }
}
"#
    }

    fn event_plan(output: &str) -> serde_json::Value {
        let plan = output
            .split_once("export const __tachyon_event_plan__ = ")
            .expect("event plan")
            .1
            .split_once(";\n")
            .expect("event plan end")
            .0;
        serde_json::from_str(plan).expect("event plan JSON")
    }

    #[test]
    fn the_event_plan_carries_listener_options_and_the_injected_publisher() {
        let source = basket_service_source();
        let output = ComponentScript::prepare(
            Path::new("/nonexistent"),
            source.as_bytes(),
            "client/services/BasketService.js",
        )
        .expect("event lowering");
        let output = String::from_utf8(output).expect("utf8");
        let plan = event_plan(&output);
        let service_type = &plan["types"][0];
        assert_eq!(service_type["name"], "BasketService");
        assert!(service_type.get("publishers").is_none(), "{plan}");
        assert_eq!(
            service_type["resources"],
            serde_json::json!([
                {"field":"applicationEventPublisher","name":"applicationEventPublisher"}
            ]),
            "{plan}"
        );
        assert_eq!(
            service_type["listeners"],
            serde_json::json!([
                {
                    "method": "sync",
                    "events": ["BasketAdded"],
                    "replay": true,
                    "condition": {
                        "k": "cmp",
                        "op": "ge",
                        "l": {"k": "value", "path": ["total"]},
                        "r": {"k": "lit", "v": 10},
                    },
                    "id": "basket-sync",
                    "order": -5,
                    "asynchronous": false,
                    "models": {"BasketAdded": {"open": false, "fields": {"id": "!string", "total": "!number"}}},
                    "returns": "BasketSynced",
                },
                {
                    "method": "report",
                    "events": ["BasketSynced"],
                    "replay": false,
                    "condition": null,
                    "id": null,
                    "order": 0,
                    "asynchronous": true,
                    "models": {"BasketSynced": {"open": false, "fields": {"total": "!number"}}},
                },
            ]),
            "{plan}"
        );
        assert!(
            !output.contains("@OnEvent") && !output.contains("@Order"),
            "{output}"
        );
        assert!(
            output.contains("publishEvent(\"BasketAdded\", item, true)"),
            "{output}"
        );
    }

    #[test]
    fn the_removed_event_publisher_decorator_is_rejected() {
        let removed = basket_service_source().replace(
            "  add(item) { this.applicationEventPublisher.publishEvent(item, true); }",
            "  @EventPublisher({\"event\":\"basket.added\"})\n  add(item) { return item; }",
        );
        let failure = ComponentScript::prepare(
            Path::new("/nonexistent"),
            removed.as_bytes(),
            "client/services/BasketService.js",
        )
        .expect_err("EventPublisher is removed");
        assert!(
            failure
                .to_string()
                .contains("EventPublisher is removed (ADR 0031)"),
            "{failure}"
        );
    }

    #[test]
    fn event_metadata_preserves_injected_resources_outside_client_constructors() {
        let source = r#"
@Client
class ProductsClient {
  @WebCall({"client":"products","method":"GET","path":"/api/products"})
  async list() { return {}; }
}
@NeedsAll
export default class Products {
  productsClient;
  @Needs
  lifecycle;
  @Needs()
  taskScheduler;
  @OnStart
  async initialize() { this.products = await this.productsClient.list(); }
}
"#;
        let output = ComponentScript::prepare(
            Path::new("/nonexistent"),
            source.as_bytes(),
            "client/pages/tac.js",
        )
        .expect("event and client lowering");
        let output = String::from_utf8(output).expect("utf8");
        assert!(output.contains("export const __tachyon_event_plan__"));
        assert!(output.contains(r#""post_construct":"initialize""#));
        assert!(output.contains(r#""field":"lifecycle","name":"lifecycle""#));
        assert!(output.contains(r#""field":"taskScheduler","name":"taskScheduler""#));
        assert!(output.contains("export function __tachyon_event_owners__"));
        assert!(!output.contains("@OnStart"));
        assert!(!output.contains("@Needs"));
        assert!(!output.contains("resolve('lifecycle')"));
        assert!(!output.contains("resolve('taskScheduler')"));
    }

    #[test]
    fn a_service_that_needs_a_store_gets_the_store_with_the_context_of_its_setup() {
        let root = tempfile::tempdir().expect("project");
        write_file(
            root.path(),
            "client/setups/CatalogSetup.js",
            "@Setup\nexport default class CatalogSetup {\n  static configure() {\n    return { entities: {} };\n  }\n}\n",
        );
        write_file(
            root.path(),
            "client/entities/ProductEntity.js",
            "@Entity({\"adapter\":\"indexeddb\"})\n@Table('products')\nexport default class ProductEntity {\n  @Id\n  id;\n  status;\n}\n",
        );
        write_file(
            root.path(),
            "client/stores/ProductStore.js",
            "@Store(ProductEntity, {\"fields\":[\"id\",\"status\"],\"defaultSize\":20,\"maxSize\":100,\"setup\":\"CatalogSetup\"})\nexport default class ProductStore {\n  async countByStatus(status) {}\n}\n",
        );
        let source = "@Service\nexport default class ProductService {\n  /** @type {ProductStore} */\n  @Needs\n  productStore;\n  summary = '';\n\n  async load() {\n    this.summary = String(await this.productStore.countByStatus('active'));\n  }\n}\n";
        let path = "client/services/ProductService.js";
        write_file(root.path(), path, source);
        let output =
            ComponentScript::prepare_captured(root.path(), source.as_bytes().to_vec(), path)
                .expect("the store is injected");
        let output = String::from_utf8(output).expect("utf8");
        for part in [
            "for (const [name, store] of [['ProductStore',ProductStore]]) stores.set(name, await store.tachyonContext(signal));",
            "resolve('ProductStore')",
            "value:stores.get(name) || context",
            "static tachyonContext(signal) {",
        ] {
            assert!(output.contains(part), "missing {part:?}:\n{output}");
        }
    }

    #[test]
    fn a_service_that_needs_a_converter_gets_the_converter_without_a_context() {
        let root = tempfile::tempdir().expect("project");
        write_file(
            root.path(),
            "client/converters/ProductConverter.js",
            "@Converter\nexport default class ProductConverter {\n  @Convert({\"fields\":[{\"source\":\"name\",\"target\":\"title\"}]})\n  async toSummary(product) {}\n}\n",
        );
        let source = "@Service\nexport default class ProductService {\n  /** @type {ProductConverter} */\n  @Needs\n  productConverter;\n  title = '';\n\n  async load() {\n    const summary = await this.productConverter.toSummary({ name: 'Anvil' });\n    this.title = summary.title;\n  }\n}\n";
        let path = "client/services/ProductService.js";
        write_file(root.path(), path, source);
        let output =
            ComponentScript::prepare_captured(root.path(), source.as_bytes().to_vec(), path)
                .expect("the converter is injected");
        let output = String::from_utf8(output).expect("utf8");
        for part in [
            "case 'ProductConverter': instance = new ProductConverter();",
            "resolve('ProductConverter')",
            "for (const [name, store] of []) stores.set(",
        ] {
            assert!(output.contains(part), "missing {part:?}:\n{output}");
        }
    }

    #[test]
    fn a_topic_listener_opens_the_topic_stream_without_a_client() {
        let root = tempfile::tempdir().expect("project");
        let source = r"
/**
 * @Topic
 * @typedef {Object} OrderPlaced
 * @property {string} id
 */
@Service
export default class Orders {
  placed = [];
  /** @param {OrderPlaced} order */
  @OnEvent
  record(order) { this.placed = [...this.placed, order.id]; }
}
";
        let output =
            ComponentScript::prepare(root.path(), source.as_bytes(), "client/services/Orders.js")
                .expect("topic listener lowering");
        let output = String::from_utf8(output).expect("utf8");
        assert!(
            output.contains(r#""path":"/.tachyon/topics/order-placed","topic":true"#),
            "{output}"
        );
        let untopical = source.replace(" * @Topic\n", "");
        let output = ComponentScript::prepare(
            root.path(),
            untopical.as_bytes(),
            "client/services/Orders.js",
        )
        .expect("local listener lowering");
        let output = String::from_utf8(output).expect("utf8");
        assert!(!output.contains("/.tachyon/topics/"), "{output}");
    }

    #[test]
    fn stream_requests_become_owned_streams_in_the_event_plan() {
        let root = tempfile::tempdir().expect("project");
        let source = r#"
/**
 * @typedef {Object} Tick
 * @property {string} sku
 * @property {number} level
 */
@HTTPClient("/api")
class TicksClient {
  /** @param {Tick} tick */
  @StreamRequest("/ticks")
  streamTicks(tick) {}
}
@NeedsAll
export default class Ticks {
  ticksClient;
}
"#;
        let output =
            ComponentScript::prepare(root.path(), source.as_bytes(), "client/services/Ticks.js")
                .expect("stream request lowering");
        let output = String::from_utf8(output).expect("utf8");
        assert!(
            output
                .contains(r#""streams":[{"event":"Tick","method":"streamTicks","models":{"Tick":"#),
            "{output}"
        );
        assert!(output.contains(r#""path":"/api/ticks"}]"#), "{output}");
        assert!(!output.contains("@StreamRequest"), "{output}");
        assert!(output.contains("export function __tachyon_event_owners__"));
        let untyped = source.replace("/** @param {Tick} tick */", "");
        let failure =
            ComponentScript::prepare(root.path(), untyped.as_bytes(), "client/services/Ticks.js")
                .expect_err("a stream needs a message model");
        assert!(
            failure
                .to_string()
                .contains("Each @StreamRequest message is checked against the model type"),
            "{failure}"
        );
        let unknown = source.replace("{Tick} tick", "{Tock} tick");
        let failure =
            ComponentScript::prepare(root.path(), unknown.as_bytes(), "client/services/Ticks.js")
                .expect_err("a stream model must exist");
        assert!(
            failure
                .to_string()
                .contains("The StreamRequest method 'streamTicks'"),
            "{failure}"
        );
        let empty = "@HTTPClient(\"/api\")\nclass TicksClient {\n}\n@NeedsAll\nexport default class Ticks {\n  ticksClient;\n}\n";
        let failure =
            ComponentScript::prepare(root.path(), empty.as_bytes(), "client/services/Ticks.js")
                .expect_err("an empty client is refused");
        assert!(
            failure
                .to_string()
                .contains("@StreamRequest or @SocketRequest"),
            "{failure}"
        );
    }

    #[test]
    fn socket_requests_become_owned_sockets_in_the_event_plan() {
        let root = tempfile::tempdir().expect("project");
        let source = r#"
/**
 * @typedef {Object} ChatMessage
 * @property {string} text
 */
@HTTPClient("/api")
class ChatClient {
  /** @param {ChatMessage} message */
  @SocketRequest("/chat")
  receive(message) {}
}
@NeedsAll
export default class Chat {
  chatClient;
}
"#;
        let output =
            ComponentScript::prepare(root.path(), source.as_bytes(), "client/services/Chat.js")
                .expect("socket request lowering");
        let output = String::from_utf8(output).expect("utf8");
        assert!(
            output.contains(
                r#""streams":[{"event":"ChatMessage","method":"receive","models":{"ChatMessage":"#
            ),
            "{output}"
        );
        assert!(
            output.contains(r#""path":"/api/chat","socket":true}]"#),
            "{output}"
        );
        assert!(!output.contains("@SocketRequest"), "{output}");
        let twice = source.replace(
            "  receive(message) {}
",
            "  receive(message) {}
  /** @param {ChatMessage} message */
  @SocketRequest(\"/other\")
  other(message) {}
",
        );
        let failure =
            ComponentScript::prepare(root.path(), twice.as_bytes(), "client/services/Chat.js")
                .expect_err("one socket for each client");
        assert!(
            failure.to_string().contains("at most one @SocketRequest"),
            "{failure}"
        );
        let untyped = source.replace("/** @param {ChatMessage} message */", "");
        let failure =
            ComponentScript::prepare(root.path(), untyped.as_bytes(), "client/services/Chat.js")
                .expect_err("a socket needs a message model");
        assert!(
            failure
                .to_string()
                .contains("Each @SocketRequest message is checked against"),
            "{failure}"
        );
    }

    fn required_prop_build(page: &str, layout: &str) -> Result<String, Failure> {
        let root = tempfile::tempdir().expect("project");
        write_file(root.path(), "client/pages/tac.html", page);
        write_file(
            root.path(),
            "client/components/docs/layout/tac.html",
            layout,
        );
        write_file(
            root.path(),
            "client/components/docs/sidebar/tac.html",
            "<nav>{drawer.open} {drawer.label}</nav>",
        );
        write_file(
            root.path(),
            "client/services/DrawerService.js",
            "@Service()\n@NeedsAll()\nexport default class DrawerService {\n  open\n  label = 'Menu'\n}\n",
        );
        let result = build_default(root.path())?;
        Ok(fs::read_to_string(result.output_directory().join("index.html")).expect("Tac"))
    }

    #[test]
    fn a_needs_all_service_demands_every_field_without_an_initial_value() {
        let forwarded = "<main><docs-sidebar :open=\"open\" /></main>";
        let tac = required_prop_build("<docs-layout :open=\"true\" />", forwarded).expect("build");
        assert!(tac.contains(r#""required":["open"]"#), "{tac}");
        let failure = required_prop_build("<docs-layout />", "<main><docs-sidebar /></main>")
            .expect_err("a missing prop is refused");
        assert!(
            failure.to_string().contains(
                "DrawerService needs `open`; pass :open=\"...\" to <docs-sidebar> (from <docs-layout>)."
            ),
            "{failure}"
        );
        let failure = required_prop_build("<docs-layout />", forwarded)
            .expect_err("a forwarded prop the layout never receives is refused");
        assert!(
            failure.to_string().contains(
                "DrawerService needs `open`; <docs-sidebar> gets it from `open`, but <docs-layout> is not given `open`"
            ),
            "{failure}"
        );
        let looped =
            "<main><loop :for=\"open of items\"><docs-sidebar :open=\"open\" /></loop></main>";
        required_prop_build("<docs-layout :items=\"true\" />", looped)
            .expect("a loop binding supplies the prop");
    }

    #[test]
    fn typescript_owned_resources_refuse_static_initialized_and_method_targets() {
        let root = tempfile::tempdir().expect("resource project");
        for declaration in [
            "private static documentTitle!: DocumentTitle;",
            "static readonly documentTitle: DocumentTitle;",
            "private documentTitle: DocumentTitle = other;",
            "private documentTitle(title: string): void {}",
        ] {
            let source = format!("@Service\nclass PageService {{\n@Needs\n{declaration}\n}}");
            let failure = ComponentScript::prepare(
                root.path(),
                source.as_bytes(),
                "client/services/PageService.ts",
            )
            .expect_err(&format!(
                "resource target must fail full lowering: {declaration}"
            ));
            assert!(
                failure.to_string().contains("TY1308"),
                "{declaration}: {failure}"
            );
            assert!(
                failure
                    .to_string()
                    .contains("client/services/PageService.ts"),
                "{declaration}: {failure}"
            );
        }
    }

    #[test]
    fn required_prop_forwarding_accepts_nested_literals_without_outer_properties() {
        for literal in ["true", "false", "null", "(false)", "'false'"] {
            let layout = format!("<main><docs-sidebar :open=\"{literal}\" /></main>");
            let tac = required_prop_build("<docs-layout />", &layout)
                .unwrap_or_else(|failure| panic!("literal {literal} must build: {failure}"));
            assert!(tac.contains(r#""required":["open"]"#), "{tac}");
        }
    }

    #[test]
    fn required_prop_forwarding_still_refuses_unknown_bare_identifiers() {
        for identifier in ["missing", "$missing", "(missing)"] {
            let layout = format!("<main><docs-sidebar :open=\"{identifier}\" /></main>");
            let failure = required_prop_build("<docs-layout />", &layout)
                .expect_err("unknown forwarded identifier must be refused");
            assert!(failure.to_string().contains("TY1407"), "{failure}");
            let name = identifier.trim_matches(['(', ')']);
            assert!(
                failure
                    .to_string()
                    .contains(&format!("gets it from `{name}`")),
                "{failure}"
            );
        }
    }

    fn named_prop_build(page: &str, field: &str) -> Result<String, Failure> {
        let root = tempfile::tempdir().expect("project");
        write_file(root.path(), "client/pages/tac.html", page);
        write_file(
            root.path(),
            "client/components/docs/sidebar/tac.html",
            "<nav>{sidebar.path}</nav>",
        );
        write_file(
            root.path(),
            "client/services/SidebarService.js",
            &format!(
                "@Service()\n@NeedsAll()\nexport default class SidebarService {{\n  path\n  {field}\n}}\n"
            ),
        );
        write_file(
            root.path(),
            "client/services/DocumentationService.js",
            "@Service()\nexport default class DocumentationService {\n  _title = 'Docs'\n}\n",
        );
        let result = build_default(root.path())?;
        Ok(fs::read_to_string(result.output_directory().join("index.html")).expect("Tac"))
    }

    #[test]
    fn a_prop_the_view_passes_never_lands_on_a_field_named_like_a_service() {
        let page = "<docs-sidebar :path=\"'/docs'\" :documentation=\"data\" />";
        let failure = named_prop_build(page, "documentation")
            .expect_err("a passed prop that a service field gets injected is refused");
        assert!(
            failure.to_string().contains(
                "`documentation` is both a property that <docs-sidebar> passes and the name of \
                 DocumentationService's instance, so SidebarService gets the service and the \
                 property never arrives. Rename the field so that it receives the property, or \
                 mark it @Needs and stop passing :documentation if you meant the service."
            ),
            "{failure}"
        );
        let renamed = "<docs-sidebar :path=\"'/docs'\" :documents=\"data\" />";
        let tac = named_prop_build(renamed, "documents").expect("a renamed field is a prop");
        assert!(tac.contains(r#""required":["path","documents"]"#), "{tac}");
        let tac = named_prop_build("<docs-sidebar :path=\"'/docs'\" />", "documentation")
            .expect("a field named like a service instance is still that service");
        assert!(tac.contains(r#""required":["path"]"#), "{tac}");
    }

    #[test]
    fn tac_never_server_renders_and_a_server_routes_folder_is_refused() {
        let root = tempfile::tempdir().expect("project");
        compose(
            root.path(),
            "",
            "view",
            "<logic :if=\"view.show\"><p>Tac true</p></logic><logic else><p>Tac false</p></logic>",
        );
        hydrate_page_view(root.path());
        service(root.path(), "ViewService", "{ show = false }");
        let result = build_default(root.path()).expect("build");
        let tac = fs::read_to_string(result.output_directory().join("index.html")).expect("Tac");
        let tac_body = tac.split_once("<body>").expect("Tac body").1;

        assert!(tac.contains(r#""branch":"if""#), "{tac}");
        assert!(tac.contains(r#""branch":"else""#), "{tac}");
        assert!(!tac_body.contains("Tac true"), "{tac_body}");
        assert!(!tac_body.contains("Tac false"), "{tac_body}");
        write_file(
            root.path(),
            "server/routes/server/yon.html",
            "<main>Yon rendered</main>",
        );
        let failure = build_default(root.path()).expect_err("server/routes must not compile");
        assert!(failure.to_string().contains("TY1310"), "{failure}");
        assert!(
            failure
                .to_string()
                .contains("which Tachyon no longer reads"),
            "{failure}"
        );
    }

    fn output_options(output_directory: impl Into<PathBuf>) -> BuildOptions {
        BuildOptions {
            output_directory: output_directory.into(),
            ..BuildOptions::default()
        }
    }

    #[test]
    fn unsafe_output_paths_are_rejected() {
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "view", "<main>Good</main>");

        for output in ["../out", "client/out", ".git/out", "dist/../out", ""] {
            let error = WebCompiler::build(root.path(), &output_options(output))
                .expect_err("unsafe output");
            assert!(error.to_string().contains("TY1202"));
        }

        let absolute = root.path().join("published");
        let result = WebCompiler::build(root.path(), &output_options(absolute))
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
        compose(root.path(), "", "view", "<main>Good</main>");
        assert!(WebCompiler::build(root.path(), &output_options("build/site")).is_ok());

        fs::write(root.path().join("blocked"), "file").expect("blocked output");
        let error =
            WebCompiler::build(root.path(), &output_options("blocked")).expect_err("file output");
        assert!(error.to_string().contains("TY1202"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_output_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("project");
        write_file(root.path(), "client/pages/tac.html", "<main>Good</main>");
        let outside = tempfile::tempdir().expect("outside");
        symlink(outside.path(), root.path().join("dist")).expect("symlink");
        let error = build_default(root.path()).expect_err("symlink output");
        assert!(error.to_string().contains("TY1202"));
    }

    #[test]
    fn internal_io_failures_are_actionable() {
        let root = tempfile::tempdir().expect("workspace");
        let file = root.path().join("file");
        fs::write(&file, "occupied").expect("file");
        let error = BuildStage::write(&file, &[(PathBuf::from("nested/index.html"), vec![1])])
            .expect_err("stage under file");
        assert!(
            BuildStage::output_error(&file, &error)
                .to_string()
                .contains("TY1201")
        );
        assert!(matches!(
            error.kind(),
            io::ErrorKind::NotADirectory | io::ErrorKind::AlreadyExists
        ));
        assert!(BuildStage::output_io::<()>(Err(error), &file).is_err());
    }

    #[test]
    fn emitted_route_manifest_satisfies_the_canonical_schema() {
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "view", "<main>Good</main>");
        build_default(root.path()).expect("build");
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(root.path().join("dist/route-manifest.json")).expect("manifest"),
        )
        .expect("JSON");
        let contract = tachyon_contracts::find("route-manifest").expect("contract");
        let schema = tachyon_contracts::parse_schema(contract).expect("schema");
        let validator = jsonschema::validator_for(&schema).expect("validator");
        assert!(validator.is_valid(&manifest));
    }

    pub(super) fn write_styled_home_and_about(root: &Path) {
        compose(
            root,
            "",
            "home",
            "<main><h1>Home {pageHome.ready}</h1></main>",
        );
        compose(root, "about", "about", "<main><h2>About</h2></main>");
        write_file(root, "client/components/page/home/tac.css", "h1{color:red}");
        service(root, "PageHomeService", "{ ready = 1 }");
        write_file(
            root,
            "client/components/page/about/tac.css",
            "h2{color:teal}",
        );
    }

    fn assert_first_build_links_styles_and_services(dist: &Path) {
        let stylesheet = fs::read_to_string(dist.join(".tachyon/components.css")).expect("styles");
        assert!(stylesheet.contains("h1{color:red}"), "{stylesheet}");
        assert!(stylesheet.contains("h2{color:teal}"), "{stylesheet}");
        assert!(dist.join(".tachyon/services/PageHomeService.js").is_file());
        assert!(!dist.join(".tachyon/components").exists());

        for route in ["index.html", "about/index.html"] {
            let document = fs::read_to_string(dist.join(route)).expect(route);
            assert!(
                document.contains(WebCompiler::COMPONENT_STYLE_LINK),
                "{route}: {document}"
            );
        }
        let index = fs::read_to_string(dist.join("index.html")).expect("index");
        assert!(
            index.contains("/.tachyon/services/PageHomeService.js"),
            "{index}"
        );
        assert!(
            index.contains(
                r#""pageHome":{"module":"/.tachyon/services/PageHomeService.js","native":false,"scope":"singleton"}"#
            ),
            "{index}"
        );
        assert!(
            index.contains(r#""services":[{"module":"/.tachyon/services/PageHomeService.js","name":"pageHome","native":false,"scope":"singleton"}]"#),
            "{index}"
        );
    }

    #[test]
    fn a_rebuilt_ty_gets_a_new_build_id_for_route_reuse() {
        let root = tempfile::tempdir().expect("tools");
        let executable = root.path().join("ty");
        fs::write(&executable, b"first build").expect("first build");
        let first = super::ToolBuild::id(Some(&executable));
        assert_eq!(first, super::ToolBuild::id(Some(&executable)));
        assert!(first.starts_with(env!("CARGO_PKG_VERSION")), "{first}");
        fs::write(&executable, b"second, longer build").expect("second build");
        assert_ne!(first, super::ToolBuild::id(Some(&executable)));
        assert_eq!(
            super::ToolBuild::id(None),
            env!("CARGO_PKG_VERSION"),
            "without a known executable only the version is used"
        );
    }

    #[test]
    fn typescript_output_is_cached_per_compiler_install_and_source() {
        let root = tempfile::tempdir().expect("tools");
        let program = root.path().join("tsc");
        fs::write(&program, "tsc 6.0").expect("compiler");
        let service = Path::new("client/services/AService.ts");
        let entry = |program: &Path, source: &Path, bytes: &[u8]| {
            TypeScriptCache::entry(program, source, bytes)
        };
        let first = entry(&program, service, b"let a = 1").expect("cache entry");
        assert_eq!(Some(first.clone()), entry(&program, service, b"let a = 1"));
        assert_ne!(Some(first.clone()), entry(&program, service, b"let a = 2"));
        let other = Path::new("client/services/BService.ts");
        assert_ne!(Some(first.clone()), entry(&program, other, b"let a = 1"));
        fs::write(&program, "tsc 6.1, a newer install").expect("updated compiler");
        assert_ne!(Some(first), entry(&program, service, b"let a = 1"));
        let missing = root.path().join("not-installed");
        assert_eq!(entry(&missing, service, b"let a = 1"), None);
    }

    #[test]
    fn a_build_reports_its_phase_timings() {
        let root = tempfile::tempdir().expect("project");
        write_styled_home_and_about(root.path());
        let built = build_default(root.path()).expect("build");
        let phases: Vec<&str> = built
            .timings()
            .iter()
            .map(|(phase, _)| phase.as_str())
            .collect();
        assert_eq!(
            phases,
            [
                "discovery",
                "routes",
                "static files",
                "services",
                "manifest and service worker",
                "compression",
                "output"
            ]
        );
        assert!(built.timings_line().starts_with("Build took "));
    }

    #[test]
    fn styles_and_services_are_emitted_linked_reused_and_invalidated() {
        let root = tempfile::tempdir().expect("project");
        write_styled_home_and_about(root.path());

        let first = build_default(root.path()).expect("first build");
        let dist = first.output_directory().to_path_buf();
        assert_first_build_links_styles_and_services(&dist);

        let reused = build_default(root.path()).expect("second build");
        assert_eq!(reused.reused_routes(), 2);
        assert!(
            dist.join(".tachyon/components.css").is_file(),
            "reuse dropped the stylesheet"
        );
        assert!(
            dist.join(".tachyon/services/PageHomeService.js").is_file(),
            "reuse dropped the module"
        );

        write_file(
            root.path(),
            "client/components/page/home/tac.css",
            "h1{color:blue}",
        );
        let changed = build_default(root.path()).expect("third build");
        assert!(
            changed.compiled_routes() >= 1,
            "a component change was ignored"
        );
        let stylesheet = fs::read_to_string(dist.join(".tachyon/components.css")).expect("styles");
        assert!(stylesheet.contains("h1{color:blue}"), "{stylesheet}");

        service(root.path(), "PageHomeService", "{ ready = 2 }");
        let changed = build_default(root.path()).expect("fourth build");
        assert!(
            changed.compiled_routes() >= 1,
            "a service change was ignored"
        );
        let module =
            fs::read_to_string(dist.join(".tachyon/services/PageHomeService.js")).expect("module");
        assert!(module.contains("ready = 2"), "{module}");
    }

    fn write_sitemap_project(root: &Path) {
        compose(root, "", "home", "<main>Home</main>");
        compose(root, "items/_id", "item", "<main>Dynamic template</main>");
        compose(root, "drafts", "draft", "<main>Draft</main>");
        write_file(
            root,
            "client/shared/data/items.json",
            r#"{"items":[{"id":"beta"},{"id":"alpha"}]}"#,
        );
        write_file(
            root,
            "client/manifest.json",
            r#"{"tac":{"origin":"https://shop.example","metadata":{"routes":{"/drafts":{"noindex":true}}},"sitemap":{"/items/_id":{"data":"/shared/data/items.json","values":"/items","field":"id"}}}}"#,
        );
    }

    #[test]
    fn dynamic_templates_and_the_generated_sitemap_publish_atomically() {
        let root = tempfile::tempdir().expect("project");
        write_sitemap_project(root.path());

        let built = build_default(root.path()).expect("build");
        let output = built.output_directory();
        assert!(output.join("items/_id/index.html").is_file());
        assert_eq!(
            fs::read_to_string(output.join("sitemap.xml")).expect("sitemap"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n  \
             <url><loc>https://shop.example/</loc></url>\n  \
             <url><loc>https://shop.example/items/alpha</loc></url>\n  \
             <url><loc>https://shop.example/items/beta</loc></url>\n\
             </urlset>\n"
        );
        assert_eq!(
            fs::read_to_string(output.join("robots.txt")).expect("robots"),
            "User-agent: *\nAllow: /\n\nSitemap: https://shop.example/sitemap.xml\n"
        );
        assert_eq!(built.sha256().len(), 64);
    }

    #[test]
    fn a_site_without_an_origin_gets_no_sitemap() {
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "home", "<main>Home</main>");
        let built = build_default(root.path()).expect("build");
        assert!(!built.output_directory().join("sitemap.xml").exists());
        assert!(!built.output_directory().join("robots.txt").exists());
    }

    #[test]
    fn sitemap_values_that_cannot_be_read_stop_the_build() {
        for (sitemap, expected) in [
            (
                r#"{"/items/_id":{"data":"/shared/data/missing.json","values":"/items"}}"#,
                "is not a file in client/shared",
            ),
            (
                r#"{"/items/_id":{"data":"/shared/data/items.json","values":"/nothing"}}"#,
                "is not a list",
            ),
            (
                r#"{"/items/_id":{"data":"/shared/data/items.json","values":"/items"}}"#,
                "must be a string",
            ),
            (
                r#"{"/others/_id":{"data":"/shared/data/items.json","values":"/items","field":"id"}}"#,
                "is not a page route",
            ),
            (
                r#"{"/items/_id":{"data":"/shared/data/items.json","values":"/bad","field":"id"}}"#,
                "is not a plain path segment",
            ),
        ] {
            let root = tempfile::tempdir().expect("project");
            write_sitemap_project(root.path());
            write_file(
                root.path(),
                "client/shared/data/items.json",
                r#"{"items":[{"id":"alpha"}],"bad":[{"id":"a/b"}]}"#,
            );
            write_file(
                root.path(),
                "client/manifest.json",
                &format!(r#"{{"tac":{{"origin":"https://shop.example","sitemap":{sitemap}}}}}"#),
            );
            let error = build_default(root.path()).expect_err(sitemap).to_string();
            assert!(error.contains("TY1601"), "{error}");
            assert!(error.contains(expected), "{sitemap}: {error}");
        }
    }

    #[test]
    fn manifest_metadata_scripts_styles_and_cache_reach_every_page() {
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "home", "<main>Home</main>");
        compose(root.path(), "about", "about", "<main>About</main>");
        write_file(
            root.path(),
            "client/manifest.json",
            r#"{"tac":{
                "origin":"https://shop.example",
                "scripts":["/shared/site.js"],
                "styles":["/shared/site.css"],
                "cache":[{"path":"/api/*","policy":"network-first"}],
                "metadata":{
                    "defaults":{"image":"/shared/logo.svg","siteName":"Shop"},
                    "routes":{"/":{"title":"Home & Shop","canonical":"/"}}
                }
            }}"#,
        );
        write_file(
            root.path(),
            "client/shared/site.css",
            "body { color: #123456; margin: 0; }",
        );
        let built = build_default(root.path()).expect("build");
        let output = built.output_directory();
        let home = fs::read_to_string(output.join("index.html")).expect("home");
        let about = fs::read_to_string(output.join("about/index.html")).expect("about");
        assert!(home.contains("<title>Home &amp; Shop</title>"), "{home}");
        assert!(
            home.contains(r#"<link rel="canonical" href="https://shop.example/">"#),
            "{home}"
        );
        assert!(!about.contains("<title>"), "{about}");
        for page in [&home, &about] {
            assert!(
                page.contains(
                    r#"<meta property="og:image" content="https://shop.example/shared/logo.svg">"#
                ),
                "{page}"
            );
            assert!(
                page.contains(r#"<meta property="og:site_name" content="Shop">"#),
                "{page}"
            );
            assert!(
                page.contains(
                    r#"<link rel="stylesheet" href="/shared/site.css" data-tachyon-runtime>"#
                ),
                "{page}"
            );
            assert!(
                page.contains(
                    r#"<script type="module" src="/shared/site.js" data-tachyon-runtime></script>"#
                ),
                "{page}"
            );
        }
        let worker = fs::read_to_string(output.join("tachyon-sw.js")).expect("worker");
        assert!(
            worker.contains(r#"{"pattern":"^/api/[^?]*$","policy":"network-first"}"#),
            "{worker}"
        );
    }

    #[test]
    fn a_typescript_companion_is_emitted_through_the_typescript_compiler() {
        if !typescript_available() {
            return;
        }
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "view", "<main>Home {typed.a}</main>");
        write_file(
            root.path(),
            "client/services/TypedService.ts",
            "@Service()\nexport default class TypedService {\n  a: number = 1\n\
             \x20 private twice(value: number): number { return (value as number) * 2 }\n}\n",
        );

        let result = build_default(root.path()).expect("build");
        let emitted = fs::read_to_string(
            result
                .output_directory()
                .join(".tachyon/services/TypedService.js"),
        )
        .expect("emitted module");
        assert!(!emitted.contains(": number"), "{emitted}");
        assert!(!emitted.contains(" as number"), "{emitted}");
        assert!(emitted.contains("twice(value)"), "{emitted}");

        let index =
            fs::read_to_string(result.output_directory().join("index.html")).expect("index");
        assert!(index.contains(r#""module":"/.tachyon/services/TypedService.js""#));
        assert!(!index.contains(r#"src="/.tachyon/services/TypedService.js""#));
    }

    #[test]
    fn an_absent_or_outdated_typescript_compiler_fails_closed() {
        if typescript_available() {
            return;
        }
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "view", "<main>Home {view.x}</main>");
        write_file(
            root.path(),
            "client/services/ViewService.ts",
            "@Service()\nexport default class ViewService { x: number = 1 }",
        );
        let error = build_default(root.path()).expect_err("missing compiler");
        assert!(error.to_string().contains("TY1009"), "{error}");
    }

    #[test]
    fn pages_compose_components_and_nothing_else() {
        let root = tempfile::tempdir().expect("project");
        let root = root.path();
        write_file(
            root,
            "client/components/site/shell/tac.html",
            "<main><slot></slot></main>",
        );
        write_file(
            root,
            "client/components/product/card/tac.html",
            "<article>{productCard.name}</article>",
        );
        write_file(
            root,
            "client/services/ProductCardService.js",
            "@Service()\nexport default class ProductCardService { name = 'Anvil' }",
        );
        write_file(
            root,
            "client/pages/tac.html",
            "<!-- the catalog -->\n<site-shell>\n  <product-card hydrate=\"load\" />\n</site-shell>\n",
        );
        build_default(root).expect("a composed page builds");
        for (page, what) in [
            ("<main><product-card /></main>", "a plain element"),
            ("<product-card />\nloose text", "text or an expression"),
            ("<site-shell>{name}</site-shell>", "text or an expression"),
            (
                "<logic :if=\"true\"><product-card /></logic>",
                "control flow",
            ),
            (
                "<script>let count = 0</script>\n<product-card />",
                "a page state script",
            ),
        ] {
            write_file(root, "client/pages/tac.html", page);
            let failure = build_default(root).expect_err("a page owns no markup of its own");
            let rendered = format!("{failure:?}");
            assert!(rendered.contains("[49, 51, 48, 57]"), "{page}: {rendered}");
            assert!(rendered.contains(what), "{page}: {rendered}");
        }
    }

    #[test]
    fn component_stylesheets_are_scoped_to_their_own_component() {
        let root = tempfile::tempdir().expect("project");
        let root = root.path();
        write_file(root, "client/pages/tac.html", "<product-card />");
        write_file(
            root,
            "client/components/product/card/tac.html",
            r#"<article aria-label="C">C</article>"#,
        );
        write_file(
            root,
            "client/components/product/card/tac.css",
            "article { color: rgb(0, 128, 0) }",
        );

        let result = build_default(root).expect("build");
        let dist = result.output_directory();

        let stylesheet =
            fs::read_to_string(dist.join(".tachyon/components.css")).expect("stylesheet");
        assert!(
            stylesheet.contains(r#"@scope (tachyon-component[data-tac-scope="product-card"])"#),
            "{stylesheet}"
        );
        assert!(stylesheet.contains("color: rgb(0, 128, 0)"), "{stylesheet}");

        let document = fs::read_to_string(dist.join("index.html")).expect("document");
        assert!(document.contains(r#""scope":true"#), "{document}");
        assert!(!document.contains("<article"), "{document}");
        assert!(
            document.contains(WebCompiler::COMPONENT_STYLE_LINK),
            "{document}"
        );
    }

    #[test]
    fn mounted_component_plan_carries_the_style_scope() {
        let root = tempfile::tempdir().expect("project");
        let root = root.path();
        write_file(
            root,
            "client/pages/tac.html",
            r#"<product-card hydrate="load" />"#,
        );
        write_file(
            root,
            "client/components/product/card/tac.html",
            r#"<article class="card">{productCard.title}</article>"#,
        );
        write_file(
            root,
            "client/components/product/card/tac.css",
            ".card { display: grid }",
        );
        write_file(
            root,
            "client/services/ProductCardService.js",
            "@Service()\nexport default class ProductCardService { title = 'Product' }",
        );

        let result = build_default(root).expect("build");
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
        compose(
            root.path(),
            "",
            "view",
            r#"<main aria-label="T"><h1>T</h1></main>"#,
        );

        let result = build_default(root.path()).expect("build");
        let dist = result.output_directory();
        assert!(!dist.join(".tachyon/components.css").exists());
        let document = fs::read_to_string(dist.join("index.html")).expect("document");
        assert!(!document.contains(WebCompiler::COMPONENT_STYLE_LINK));
    }

    #[test]
    fn event_bindings_emit_markers_and_the_delegated_runtime() {
        let root = tempfile::tempdir().expect("project");
        compose(
            root.path(),
            "",
            "view",
            r#"<main><button on:click="view.increment()">Add</button>
               <input aria-label="Name" on:input="view.rename()"></main>"#,
        );
        hydrate_page_view(root.path());
        service(root.path(), "ViewService", "{ increment() {} rename() {} }");

        let result = build_default(root.path()).expect("build");
        let dist = result.output_directory();
        let index = fs::read_to_string(dist.join("index.html")).expect("index");

        assert!(!index.contains("on:click"), "{index}");
        assert!(index.contains(r#""eventType":"click""#), "{index}");
        assert!(index.contains(r#""eventType":"input""#), "{index}");
        assert!(!dist.join(".tachyon/events.js").exists());
        let runtime = fs::read_to_string(dist.join(".tachyon/tac-client.js")).expect("runtime");
        assert!(runtime.contains("const bindEvent ="), "{runtime}");

        let reused = build_default(root.path()).expect("reuse");
        assert_eq!(reused.reused_routes(), 1);
        assert!(dist.join(".tachyon/tac-client.js").is_file());
    }

    #[test]
    fn a_route_binding_events_without_a_client_module_fails_closed() {
        let root = tempfile::tempdir().expect("project");
        compose(
            root.path(),
            "",
            "view",
            r#"<main><button on:click="increment()">Add</button></main>"#,
        );
        let error = build_default(root.path()).expect_err("missing client module");
        assert!(error.to_string().contains("TY1306"), "{error}");
    }

    #[test]
    fn a_route_without_events_emits_no_event_runtime() {
        let root = tempfile::tempdir().expect("project");
        compose(root.path(), "", "view", "<main>Plain</main>");
        let result = build_default(root.path()).expect("build");
        assert!(
            !result
                .output_directory()
                .join(".tachyon/events.js")
                .exists()
        );
    }

    #[test]
    fn generated_pages_opt_into_platform_navigation() {
        let root = tempfile::tempdir().expect("project");
        compose(
            root.path(),
            "",
            "home",
            r#"<main><a href="/about">A</a></main>"#,
        );
        compose(root.path(), "about", "about", "<main>About</main>");

        let result = build_default(root.path()).expect("build");
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
                document.contains(WebCompiler::NAVIGATION_LINK),
                "{route}: {document}"
            );
            assert!(
                document.contains(r#"<script type="speculationrules" data-tachyon-runtime>"#),
                "{route}: {document}"
            );
            assert!(document.contains(r#""prefetch""#), "{route}");
            assert!(
                document.contains(&super::ViewTransitionGuard::tag()),
                "{route}: {document}"
            );
        }
    }

    #[cfg(unix)]
    fn write_card_project(root: &Path, page: &str, section: &str, origin: &str) {
        write_file(root, "client/pages/tac.html", page);
        write_file(
            root,
            "client/components/owned/card/tac.html",
            &format!("<section>{section} {{ownedCard.marker}}</section>"),
        );
        write_file(
            root,
            "client/services/OwnedCardService.js",
            &format!(
                "@Service()\nexport default class OwnedCardService {{ static origin = '{origin}-script'; marker = '' }}\n"
            ),
        );
        write_file(
            root,
            "client/shared/origin.txt",
            &format!("{origin}-shared"),
        );
        write_file(
            root,
            "client/manifest.json",
            &format!(r#"{{"tac":{{"origin":"https://{origin}.example"}}}}"#),
        );
    }

    #[cfg(unix)]
    #[test]
    fn public_build_from_project_uses_only_the_owned_component_asset_and_config_snapshot() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().expect("workspace");
        let authored = workspace.path().join("project");
        write_card_project(
            &authored,
            r#"<owned-card hydrate="load" />"#,
            "owned component snapshot",
            "owned",
        );
        let project = ProjectDiscovery::discover(&authored).expect("owned snapshot");

        let retained = workspace.path().join("retained");
        fs::rename(&authored, &retained).expect("move project");
        let planted = tempfile::tempdir().expect("planted");
        write_card_project(
            planted.path(),
            "<owned-card />",
            "planted component canary",
            "planted",
        );
        symlink(planted.path(), &authored).expect("ambient replacement");

        let options = BuildOptions {
            output_directory: workspace.path().join("web-output"),
            incremental: false,
            ..BuildOptions::default()
        };
        let built = WebCompiler::build_project(&project, &options).expect("snapshot build");
        let root = built.output_directory();
        let index = fs::read_to_string(root.join("index.html")).expect("index");
        let component = fs::read_to_string(root.join(".tachyon/services/OwnedCardService.js"))
            .expect("service script");
        assert!(index.contains("owned component snapshot"), "{index}");
        assert!(!index.contains("planted"), "{index}");
        assert!(component.contains("owned-script"), "{component}");
        assert!(!component.contains("planted"), "{component}");
        assert_eq!(
            fs::read_to_string(root.join("shared/origin.txt")).expect("shared asset"),
            "owned-shared"
        );
        let robots = fs::read_to_string(root.join("robots.txt")).expect("robots");
        assert!(robots.contains("https://owned.example"), "{robots}");
        assert!(!robots.contains("planted"), "{robots}");
    }
}
