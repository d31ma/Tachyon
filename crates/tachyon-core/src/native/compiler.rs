use super::android::AndroidHostGenerator;
use super::config::NativeApplication;
use super::host::GeneratedHost;
use super::ios::IosHostGenerator;
use super::linux::LinuxHostGenerator;
use super::macos::MacOsHostGenerator;
use super::planner::{NativePlanner, NativeRouteIndex, NativeRouteIndexEntry, PlannedNativeRoute};
use super::windows::WindowsHostGenerator;
use crate::compiler::{publish, resolve_output_path};
use crate::failure::diagnostic;
use crate::{BuildOptions, Failure, ProjectDiscovery, WebCompiler};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tachyon_contracts::{
    ArtifactContractVersions, ArtifactManifest, ArtifactOutput, ArtifactTarget, ArtifactToolchain,
    NativeTarget,
};

const CONTROLLER_SCRIPT_TAG: &str =
    r#"<script type="module" src="/.tachyon/native-controller.js"></script>"#;
const LEGACY_CONTROLLER_TEMPLATE: &str = r"(() => {
const __root = __ROOT__
const __state = new Map(Object.entries(__STATE__))
const __listeners = new Map()
const __call = async (capability, payload = {}) => {
  const bridge = globalThis.__tachyonNativeHostCall
  if (typeof bridge !== 'function') return {}
  const raw = await bridge(capability, JSON.stringify(payload))
  const answer = typeof raw === 'string' ? JSON.parse(raw) : raw
  if (answer && answer.ok === false) throw new Error(answer.error || capability + ' failed')
  return answer?.value ?? answer ?? {}
}
const host = {
  invoke: __call,
  on(event, handler) {
    const handlers = __listeners.get(event) || new Set()
    handlers.add(handler)
    __listeners.set(event, handlers)
    return () => handlers.delete(handler)
  },
}
const shortcuts = {
  register: (payload) => __call('shortcuts.register', payload),
  unregister: (id) => __call('shortcuts.unregister', { id }),
  unregisterAll: () => __call('shortcuts.unregisterAll', {}),
  list: () => __call('shortcuts.list', {}),
}
const contentSurface = {
  async open(payload) {
    const opened = await __call('contentSurface.open', payload)
    return opened?.pending ? __call('contentSurface.state', { id: payload.id }) : opened
  },
  state: (id) => __call('contentSurface.state', { id }),
  close: (id) => __call('contentSurface.close', { id }),
}
__PAGE_MODULE__
let __instance
let __mounted = false
const __walk = (node, visit) => {
  visit(node)
  for (const child of node?.children || []) __walk(child, visit)
}
const __display = (value) => Array.isArray(value) ? value.join('|') : String(value ?? '')
const __sync = () => {
  __walk(__root, (node) => {
    const binding = node?.properties?.binding
    if (binding) {
      const value = __instance && binding in __instance ? __instance[binding] : __state.get(binding)
      const prefix = node.properties.prefix || ''
      node.children = [{ kind: 'text', value: prefix + __display(value) }]
    }
    if (node?.kind === 'text' && __instance) {
      for (const [name, value] of Object.entries(__instance)) {
        const prefix = name.charAt(0).toUpperCase() + name.slice(1) + ': '
        if (node.value.startsWith(prefix)) node.value = prefix + __display(value)
      }
    }
  })
}
const __mount = async () => {
  if (__mounted) return
  __mounted = true
  __instance = new TachyonPage()
  for (const [name, value] of __state) if (!(name in __instance)) __instance[name] = value
  for (const method of TachyonPage.__tachyonOnMount || []) {
    if (typeof __instance[method] === 'function') await __instance[method]()
  }
  __sync()
}
const __snapshot = () => ({ schemaVersion: 1, route: '/', title: 'Tachyon', root: structuredClone(__root) })
globalThis.__tachyonNativeUI = {
  async render() {
    await __mount()
    __sync()
    return __snapshot()
  },
  async dispatch(event = {}) {
    await __mount()
    let target
    __walk(__root, (node) => { if (node?.id === event.elementId) target = node })
    const action = target?.properties?.action
    if (event.type === 'click' && typeof action === 'string') {
      const [verb, binding] = action.split(':')
      if (verb === 'increment') {
        const value = Number(__instance?.[binding] ?? __state.get(binding) ?? 0) + 1
        __state.set(binding, String(value))
        if (__instance) __instance[binding] = value
      }
    }
    __sync()
    return __snapshot()
  },
  async emit(message = {}) {
    await __mount()
    for (const handler of __listeners.get(message.event) || []) await handler(message.payload || {})
    __sync()
    return __snapshot()
  },
}
})()
";

/// Options for one Phase 5 native application build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBuildOptions {
    /// Project-relative base output directory. Artifacts publish below the target directory.
    pub output_directory: PathBuf,
    /// Native platform receiving the generated application.
    pub target: NativeTarget,
    /// Whether the generated host is compiled into an installable artifact.
    pub package: bool,
}

impl Default for NativeBuildOptions {
    fn default() -> Self {
        Self {
            output_directory: PathBuf::from("dist"),
            target: NativeTarget::Macos,
            package: true,
        }
    }
}

/// Returns the stable published directory name for one native target.
#[must_use]
pub const fn native_target_directory(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::Linux => "linux",
        NativeTarget::Macos => "macos",
        NativeTarget::Windows => "windows",
        NativeTarget::Android => "android",
        NativeTarget::Ios => "ios",
    }
}

/// Returns the Artifact Manifest v1 operating system, architecture, and ABI.
fn artifact_target(target: NativeTarget) -> ArtifactTarget {
    let host_architecture = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let (os, architecture, abi) = match target {
        NativeTarget::Linux => ("linux", host_architecture, "gtk4"),
        NativeTarget::Macos => ("macos", host_architecture, "swiftui"),
        NativeTarget::Windows => ("windows", "x86_64", "win32"),
        NativeTarget::Android => ("android", "aarch64", "compose"),
        NativeTarget::Ios => ("ios", host_architecture, "swiftui-simulator"),
    };
    ArtifactTarget {
        os: String::from(os),
        architecture: String::from(architecture),
        abi: String::from(abi),
    }
}

/// Evidence returned by a successful native application build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBuildResult {
    output_directory: PathBuf,
    application_bundle: PathBuf,
    route_count: usize,
    native_node_count: usize,
    web_surface_count: usize,
    sha256: String,
}

impl NativeBuildResult {
    /// Returns the published target output directory.
    #[must_use]
    pub fn output_directory(&self) -> &Path {
        &self.output_directory
    }

    /// Returns the published application bundle, executable, or package.
    #[must_use]
    pub fn application_bundle(&self) -> &Path {
        &self.application_bundle
    }

    /// Returns the number of packaged page routes.
    #[must_use]
    pub const fn route_count(&self) -> usize {
        self.route_count
    }

    /// Returns the number of planned native nodes.
    #[must_use]
    pub const fn native_node_count(&self) -> usize {
        self.native_node_count
    }

    /// Returns the number of isolated `WebSurface` boundaries.
    #[must_use]
    pub const fn web_surface_count(&self) -> usize {
        self.web_surface_count
    }

    /// Returns the digest over the complete published artifact.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Compiles resolved Tachyon routes into a native application.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeCompiler;

impl NativeCompiler {
    /// Builds a native application using a current-thread async runtime.
    ///
    /// # Errors
    ///
    /// Returns diagnostics when planning, Swift compilation, signing, or
    /// atomic publication fails.
    pub fn build(
        project_root: impl AsRef<Path>,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                Failure::one(diagnostic(
                    1605,
                    format!("Cannot start the native build runtime: {error}"),
                    None,
                    None,
                ))
            })?;
        runtime.block_on(Self::build_async(project_root, options))
    }

    /// Builds and atomically publishes a native application.
    ///
    /// # Errors
    ///
    /// Returns deterministic native diagnostics and preserves the previous
    /// native output on every failure.
    pub async fn build_async(
        project_root: impl AsRef<Path>,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure> {
        let project = ProjectDiscovery::discover(project_root)?;
        let application = NativeApplication::discover(project.root())?;
        let base_output = resolve_output_path(project.root(), &options.output_directory)?;
        if !project
            .route_graph()
            .routes()
            .iter()
            .any(|route| route.route() == application.entry_route && route.source_path().is_some())
        {
            return Err(Failure::one(diagnostic(
                1601,
                format!(
                    "Native entry route '{}' does not identify a page.",
                    application.entry_route
                ),
                Some(String::from(
                    "Point tachyon.json application.entry_route at a discovered page route.",
                )),
                None,
            )));
        }

        let (temporary_web, routes) = resolve_routes(&project, options.target).await?;
        let web_surface_count = routes
            .iter()
            .map(|route| route.web_surface_count)
            .sum::<usize>();
        if web_surface_count > 1_024 {
            return Err(Failure::one(diagnostic(
                1604,
                "Application exceeds the limit of 1,024 WebSurfaces.",
                Some(String::from(
                    "Add native adapters or reduce unsupported and remote subtrees.",
                )),
                None,
            )));
        }
        let index = native_index(&application, &routes);

        let destination = base_output.join(native_target_directory(options.target));
        let parent = destination.parent().unwrap_or(project.root());
        fs::create_dir_all(parent).map_err(|error| native_io_failure(parent, &error))?;
        let stage = tempfile::Builder::new()
            .prefix(".tachyon-native-build-")
            .tempdir_in(parent)
            .map_err(|error| native_io_failure(parent, &error))?;
        let generated = generate_host(
            options.target,
            &application,
            &routes,
            &index,
            temporary_web.path(),
            stage.path(),
            options.package,
        )
        .await?;
        write_legacy_compatibility(
            stage.path(),
            temporary_web.path(),
            &application,
            options.target,
            &routes,
            &index,
        )?;
        let outputs = collect_outputs(stage.path(), "artifact-manifest.json")?;
        let manifest = artifact_manifest(options.target, &generated, outputs);
        let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            Failure::one(diagnostic(
                1605,
                format!("Cannot serialize Artifact Manifest v1: {error}"),
                None,
                None,
            ))
        })?;
        manifest_bytes.push(b'\n');
        fs::write(stage.path().join("artifact-manifest.json"), manifest_bytes)
            .map_err(|error| native_io_failure(stage.path(), &error))?;
        let digest = digest_tree(stage.path())?;
        publish(stage, &destination).map_err(|error| native_io_failure(&destination, &error))?;

        let native_node_count = routes.iter().map(|route| route.native_node_count).sum();
        Ok(NativeBuildResult {
            output_directory: destination.clone(),
            application_bundle: destination.join(generated.application_bundle),
            route_count: routes.len(),
            native_node_count,
            web_surface_count,
            sha256: digest,
        })
    }
}

fn write_legacy_compatibility(
    stage: &Path,
    web_bundle: &Path,
    application: &NativeApplication,
    target: NativeTarget,
    routes: &[PlannedNativeRoute],
    index: &NativeRouteIndex,
) -> Result<(), Failure> {
    let resources = stage.join("Resources");
    fs::create_dir_all(&resources).map_err(|error| native_io_failure(&resources, &error))?;
    let has_fallbacks = routes.iter().any(|route| route.web_surface_count > 0);
    let host = serde_json::json!({
        "schemaVersion": 2,
        "target": native_target_directory(target),
        "appName": application.name,
        "appId": application.application_id,
        "version": application.version,
        "entry": "Resources/tachyon.native-ui.json",
        "renderMode": "native",
        "nativeUIEntry": "Resources/tachyon.native-ui.json",
        "hasWebViewFallbacks": has_fallbacks,
        "platformApiVersion": 1,
        "bridgeVersion": 2,
        "hostCapabilities": [
            "app.info", "capabilities.state", "clipboard.readText", "clipboard.writeText",
            "openUrl", "window.state", "window.alwaysOnTop", "window.opacity",
            "shortcuts.register", "shortcuts.unregister", "shortcuts.unregisterAll",
            "shortcuts.list", "window.clickThrough", "window.captureProtection", "fs.paths"
        ],
        "rawHostCapabilities": [],
        "requestedDevicePermissions": [],
        "permissionOrigins": {},
        "managedContentPolicy": {
            "allowedOrigins": [], "popups": "event", "downloads": "deny",
            "uploads": "prompt", "permissions": "deny-all"
        },
        "extensions": [],
        "capabilities": []
    });
    write_pretty_json(&stage.join("tachyon.host.json"), &host)?;

    let legacy_routes = routes
        .iter()
        .map(|route| {
            serde_json::json!({
                "schemaVersion": 1,
                "route": route.route,
                "title": application.name,
                "root": legacy_native_node(&route.native_ui.root)
            })
        })
        .collect::<Vec<_>>();
    let legacy_ui = serde_json::json!({
        "schemaVersion": 1,
        "renderMode": "native",
        "entryRoute": application.entry_route,
        "controller": "tachyon.native-controller.js",
        "adapters": [],
        "hasWebViewFallbacks": has_fallbacks,
        "webViewFallbacks": [],
        "routes": legacy_routes
    });
    write_pretty_json(&resources.join("tachyon.native-ui.json"), &legacy_ui)?;

    let entry = routes
        .iter()
        .find(|route| route.route == application.entry_route)
        .or_else(|| routes.first());
    let initial_state = index
        .initial_state
        .get(&application.entry_route)
        .cloned()
        .unwrap_or_default();
    let page_module = entry
        .and_then(|route| route_client_module(web_bundle, &route.route))
        .unwrap_or_else(|| String::from("class TachyonPage {}\n"));
    let root = entry.map_or_else(
        || serde_json::json!({"kind":"element","tag":"main","children":[]}),
        |route| legacy_native_node(&route.native_ui.root),
    );
    let controller = legacy_controller(&page_module, &root, &initial_state);
    let controller_path = resources.join("tachyon.native-controller.js");
    fs::write(&controller_path, controller)
        .map_err(|error| native_io_failure(&controller_path, &error))?;
    Ok(())
}

fn write_pretty_json(path: &Path, value: &serde_json::Value) -> Result<(), Failure> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        Failure::one(diagnostic(
            1605,
            format!("Cannot serialize native compatibility artifact: {error}"),
            None,
            None,
        ))
    })?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| native_io_failure(path, &error))
}

fn route_client_module(web_bundle: &Path, route: &str) -> Option<String> {
    let mut path = web_bundle.to_path_buf();
    for segment in route
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        path.push(segment);
    }
    path.push("client.js");
    let source = fs::read_to_string(path).ok()?;
    source
        .contains("export default class")
        .then(|| source.replacen("export default class", "class TachyonPage", 1))
}

fn legacy_native_node(node: &tachyon_contracts::NativeNode) -> serde_json::Value {
    match node {
        tachyon_contracts::NativeNode::Text { value } => {
            serde_json::json!({"kind":"text","value":value})
        }
        tachyon_contracts::NativeNode::WebSurface {
            id,
            location,
            reason,
            accessibility,
            ..
        } => serde_json::json!({
            "kind":"webview", "tag":"web-surface", "id":id,
            "location":location, "reason":reason,
            "attributes":{}, "style":{}, "events":{}, "children":[],
            "accessibility":accessibility
        }),
        tachyon_contracts::NativeNode::NativeElement {
            id,
            adapter,
            properties,
            events,
            accessibility,
            children,
        } => {
            let tag = legacy_tag(adapter, accessibility.as_ref());
            let event_map = events
                .keys()
                .map(|event| {
                    (
                        event.clone(),
                        serde_json::Value::String(id.clone().unwrap_or_default()),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            serde_json::json!({
                "kind":"element", "tag":tag, "id":id, "key":null,
                "adapter":null, "attributes":{}, "style":{}, "events":event_map,
                "properties":properties, "accessibility":accessibility,
                "children":children.iter().map(legacy_native_node).collect::<Vec<_>>()
            })
        }
    }
}

fn legacy_tag(
    adapter: &str,
    accessibility: Option<&tachyon_contracts::NativeAccessibility>,
) -> &'static str {
    match adapter {
        "control.button" => "button",
        "control.text_field" => "input",
        "content.output" => "output",
        "content.heading1" => "h1",
        "content.heading2" => "h2",
        "content.heading3" => "h3",
        "content.text" => "p",
        "navigation.link" => "a",
        "layout.list" => "ul",
        "layout.list_item" => "li",
        _ if accessibility.and_then(|value| value.role.as_deref()) == Some("main") => "main",
        _ => "div",
    }
}

fn legacy_controller(
    page_module: &str,
    root: &serde_json::Value,
    initial_state: &BTreeMap<String, String>,
) -> String {
    LEGACY_CONTROLLER_TEMPLATE
        .replace(
            "__ROOT__",
            &serde_json::to_string(root).unwrap_or_else(|_| String::from("{}")),
        )
        .replace(
            "__STATE__",
            &serde_json::to_string(initial_state).unwrap_or_else(|_| String::from("{}")),
        )
        .replace("__PAGE_MODULE__", page_module)
}

/// Dispatches one staged build to the generator owning the selected platform.
async fn generate_host(
    target: NativeTarget,
    application: &NativeApplication,
    routes: &[PlannedNativeRoute],
    index: &NativeRouteIndex,
    web_bundle: &Path,
    stage: &Path,
    package: bool,
) -> Result<GeneratedHost, Failure> {
    match target {
        NativeTarget::Macos => {
            MacOsHostGenerator::generate(application, routes, index, web_bundle, stage, package)
                .await
        }
        NativeTarget::Ios => {
            IosHostGenerator::generate(application, routes, index, web_bundle, stage, package).await
        }
        NativeTarget::Linux => {
            LinuxHostGenerator::generate(application, routes, index, web_bundle, stage, package)
                .await
        }
        NativeTarget::Windows => {
            WindowsHostGenerator::generate(application, routes, index, web_bundle, stage, package)
                .await
        }
        NativeTarget::Android => {
            AndroidHostGenerator::generate(application, routes, index, web_bundle, stage, package)
                .await
        }
    }
}

async fn resolve_routes(
    project: &crate::Project,
    target: NativeTarget,
) -> Result<(tempfile::TempDir, Vec<PlannedNativeRoute>), Failure> {
    let components = crate::template::ComponentRegistry::discover(project.root())?;
    let temporary_web = tempfile::Builder::new()
        .prefix(".tachyon-native-web-")
        .tempdir_in(project.root())
        .map_err(|error| native_io_failure(project.root(), &error))?;
    let temporary_name = temporary_web
        .path()
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| {
            Failure::one(diagnostic(
                1605,
                "Cannot allocate the resolved web bundle.",
                None,
                None,
            ))
        })?;
    WebCompiler::build_async(
        project.root(),
        &BuildOptions {
            output_directory: temporary_name,
            incremental: false,
        },
    )
    .await?;
    install_controller_reference(temporary_web.path(), project.route_graph().routes())?;

    let mut routes = Vec::new();
    for route in project.route_graph().routes() {
        let Some(output) = route.template_output_path() else {
            continue;
        };
        let html_path = temporary_web.path().join(&output);
        let generated_html = fs::read_to_string(&html_path)
            .map_err(|error| native_io_failure(&html_path, &error))?;
        let styles = linked_styles(temporary_web.path(), &generated_html);
        // Native lowering consumes the authored Tac declaration. The generated
        // web document is only consulted for its linked styles and fallback
        // assets; it is never treated as server-rendered view structure.
        let source_path = route.source_path().unwrap_or("client/pages/tac.html");
        let source = route
            .absolute_source_path()
            .map(fs::read_to_string)
            .transpose()
            .map_err(|error| native_io_failure(project.root(), &error))?
            .unwrap_or_default();
        let (source, _) = crate::compiler::strip_page_state_scripts(&source, source_path)?;
        routes.push(NativePlanner::plan_with_components_and_state(
            target,
            route.route(),
            source_path,
            &source,
            &styles,
            &components,
            client_route_state(&generated_html),
        )?);
    }
    routes.sort_by(|left, right| left.route.cmp(&right.route));
    Ok((temporary_web, routes))
}

/// Reads the stylesheets a rendered route links, in the order it links them.
///
/// Whatever the web build decided the page needs is what a fallback subtree
/// needs, so the links are followed rather than named here. A missing or
/// oversized sheet is skipped: a native build must not fail because a style is
/// unreadable.
fn linked_styles(bundle: &Path, html: &str) -> String {
    const MAX_STYLE_BYTES: u64 = 1024 * 1024;
    const MAX_TOTAL_STYLE_BYTES: usize = 4 * 1024 * 1024;
    let mut styles = String::new();
    let Ok(bundle) = fs::canonicalize(bundle) else {
        return styles;
    };
    let mut budget = StyleBudget {
        remaining_bytes: MAX_TOTAL_STYLE_BYTES,
        remaining_imports: 128,
        active: HashSet::new(),
    };
    let mut rest = html;
    while let Some(start) = rest.find("<link") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..end];
        rest = &rest[end..];
        if !tag.contains("stylesheet") {
            continue;
        }
        let Some(href) = attribute_value(tag, "href") else {
            continue;
        };
        let Some(path) = resolve_local_style(&bundle, &bundle, &href) else {
            continue;
        };
        if fs::metadata(&path).is_ok_and(|data| data.len() > MAX_STYLE_BYTES) {
            continue;
        }
        if let Some(text) = inline_local_style(&bundle, &path, &mut budget, 0) {
            styles.push_str(&text);
            styles.push('\n');
        }
    }
    styles
}

struct StyleBudget {
    remaining_bytes: usize,
    remaining_imports: usize,
    active: HashSet<PathBuf>,
}

/// Resolves same-bundle CSS references without allowing a stylesheet to escape
/// the compiled web root through traversal or symlinks.
fn resolve_local_style(bundle: &Path, parent: &Path, reference: &str) -> Option<PathBuf> {
    let reference = reference
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    if reference.is_empty()
        || reference.starts_with("//")
        || reference
            .split('/')
            .next()
            .is_some_and(|segment| segment.contains(':'))
    {
        return None;
    }
    let candidate = if reference.starts_with('/') {
        bundle.join(reference.trim_start_matches('/'))
    } else {
        parent.join(reference)
    };
    let canonical = fs::canonicalize(candidate).ok()?;
    let metadata = fs::symlink_metadata(&canonical).ok()?;
    (canonical.starts_with(bundle) && metadata.is_file()).then_some(canonical)
}

/// Flattens local, unconditional CSS imports for a `WebSurface` document.
///
/// Conditional and remote imports remain intact so their browser semantics do
/// not silently change. Unresolvable local imports are removed: an app asset
/// URL cannot resolve them and retaining one only produces a broken request.
fn inline_local_style(
    bundle: &Path,
    path: &Path,
    budget: &mut StyleBudget,
    depth: usize,
) -> Option<String> {
    const MAX_STYLE_BYTES: u64 = 1024 * 1024;
    const MAX_IMPORT_DEPTH: usize = 16;
    if depth > MAX_IMPORT_DEPTH || budget.active.contains(path) {
        return Some(String::new());
    }
    let metadata = fs::metadata(path).ok()?;
    let byte_count = usize::try_from(metadata.len()).ok()?;
    if metadata.len() > MAX_STYLE_BYTES || byte_count > budget.remaining_bytes {
        return None;
    }
    let css = fs::read_to_string(path).ok()?;
    budget.remaining_bytes -= byte_count;
    budget.active.insert(path.to_path_buf());

    let mut output = String::with_capacity(css.len());
    let mut cursor = 0;
    while let Some((start, end)) = next_css_import(&css, cursor) {
        output.push_str(&css[cursor..start]);
        let statement = &css[start + "@import".len()..end - 1];
        let Some((reference, unconditional)) = parse_css_import(statement) else {
            output.push_str(&css[start..end]);
            cursor = end;
            continue;
        };
        if !is_local_style_reference(&reference) || !unconditional {
            output.push_str(&css[start..end]);
            cursor = end;
            continue;
        }
        let imported = budget
            .remaining_imports
            .checked_sub(1)
            .and_then(|remaining| {
                budget.remaining_imports = remaining;
                let parent = path.parent().unwrap_or(bundle);
                let imported = resolve_local_style(bundle, parent, &reference)?;
                inline_local_style(bundle, &imported, budget, depth + 1)
            })
            .unwrap_or_default();
        output.push_str(&imported);
        output.push('\n');
        cursor = end;
    }
    output.push_str(&css[cursor..]);
    budget.active.remove(path);
    Some(output)
}

fn is_local_style_reference(reference: &str) -> bool {
    !reference.is_empty()
        && !reference.starts_with("//")
        && !reference
            .split('/')
            .next()
            .is_some_and(|segment| segment.contains(':'))
}

/// Finds an `@import ...;` statement while ignoring comments and strings.
fn next_css_import(css: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = css.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"/*") {
            let offset = css[index + 2..].find("*/")?;
            index += offset + 4;
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"') {
            index = skip_css_string(bytes, index)?;
            continue;
        }
        if bytes[index] == b'@'
            && bytes
                .get(index..index + 7)
                .is_some_and(|word| word.eq_ignore_ascii_case(b"@import"))
            && bytes.get(index + 7).is_some_and(u8::is_ascii_whitespace)
        {
            let start = index;
            index += 7;
            let mut parentheses = 0usize;
            while index < bytes.len() {
                if bytes[index..].starts_with(b"/*") {
                    let offset = css[index + 2..].find("*/")?;
                    index += offset + 4;
                    continue;
                }
                if matches!(bytes[index], b'\'' | b'"') {
                    index = skip_css_string(bytes, index)?;
                    continue;
                }
                match bytes[index] {
                    b'(' => parentheses += 1,
                    b')' => parentheses = parentheses.saturating_sub(1),
                    b';' if parentheses == 0 => return Some((start, index + 1)),
                    _ => {}
                }
                index += 1;
            }
            return None;
        }
        index += 1;
    }
    None
}

fn skip_css_string(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 2;
        } else if bytes[index] == quote {
            return Some(index + 1);
        } else {
            index += 1;
        }
    }
    None
}

/// Returns the import reference and whether it has no conditional suffix.
fn parse_css_import(statement: &str) -> Option<(String, bool)> {
    let statement = statement.trim();
    let (reference, suffix) = if statement
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("url("))
    {
        let close = statement.find(')')?;
        let reference = statement[4..close].trim().trim_matches(['\'', '"']).trim();
        (reference, statement[close + 1..].trim())
    } else if matches!(statement.as_bytes().first(), Some(b'\'' | b'"')) {
        let quote = statement.as_bytes()[0];
        let end = statement.as_bytes()[1..]
            .iter()
            .position(|byte| *byte == quote)?
            + 1;
        (&statement[1..end], statement[end + 1..].trim())
    } else {
        return None;
    };
    Some((String::from(reference), suffix.is_empty()))
}

/// Reads one double-quoted attribute out of an opening tag.
fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(String::from(&rest[..end]))
}

fn native_index(
    application: &NativeApplication,
    routes: &[PlannedNativeRoute],
) -> NativeRouteIndex {
    NativeRouteIndex {
        contract_version: 1,
        entry_route: application.entry_route.clone(),
        routes: routes
            .iter()
            .map(|route| NativeRouteIndexEntry {
                route: route.route.clone(),
                document: format!("{}.json", route.document_key),
            })
            .collect(),
        initial_state: routes
            .iter()
            .map(|route| (route.route.clone(), route.initial_state.clone()))
            .collect(),
    }
}

fn install_controller_reference(
    web_root: &Path,
    routes: &[crate::RouteNode],
) -> Result<(), Failure> {
    let mut required = false;
    for route in routes {
        let Some(output) = route.template_output_path() else {
            continue;
        };
        let path = web_root.join(output);
        let mut html =
            fs::read_to_string(&path).map_err(|error| native_io_failure(&path, &error))?;
        if route_requires_controller(&html) {
            required = true;
            html = inject_before_body(&html, CONTROLLER_SCRIPT_TAG);
            fs::write(&path, html).map_err(|error| native_io_failure(&path, &error))?;
        }
    }
    if required {
        let runtime = web_root.join(".tachyon/native-controller.js");
        if let Some(parent) = runtime.parent() {
            fs::create_dir_all(parent).map_err(|error| native_io_failure(parent, &error))?;
        }
        fs::write(&runtime, CONTROLLER_RUNTIME)
            .map_err(|error| native_io_failure(&runtime, &error))?;
    }
    Ok(())
}

fn route_requires_controller(html: &str) -> bool {
    html.contains("data-tachyon-action=")
        || html.contains("data-tachyon-bind=")
        || html.contains(r#""name":"data-tachyon-action""#)
        || html.contains(r#""name":"data-tachyon-bind""#)
}

fn client_route_state(html: &str) -> BTreeMap<String, String> {
    let Some(script_start) = html.find(r#"<script id="tachyon-view""#) else {
        return BTreeMap::new();
    };
    let Some(open_end_relative) = html[script_start..].find('>') else {
        return BTreeMap::new();
    };
    let json_start = script_start + open_end_relative + 1;
    let Some(json_end_relative) = html[json_start..].find("</script>") else {
        return BTreeMap::new();
    };
    let json_end = json_start + json_end_relative;
    let Ok(plan) = serde_json::from_str::<Value>(&html[json_start..json_end]) else {
        return BTreeMap::new();
    };
    let Some(state) = plan.get("state").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    state
        .iter()
        .filter_map(|(name, value)| {
            let value = match value {
                Value::Null => String::new(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                Value::String(value) => value.clone(),
                Value::Array(_) | Value::Object(_) => return None,
            };
            Some((name.clone(), value))
        })
        .collect()
}

fn inject_before_body(html: &str, value: &str) -> String {
    if let Some(position) = html.rfind("</body>") {
        let mut output = String::with_capacity(html.len() + value.len());
        output.push_str(&html[..position]);
        output.push_str(value);
        output.push_str(&html[position..]);
        output
    } else {
        format!("{html}{value}")
    }
}

fn artifact_manifest(
    target: NativeTarget,
    generated: &GeneratedHost,
    outputs: Vec<ArtifactOutput>,
) -> ArtifactManifest {
    ArtifactManifest {
        contract_version: 1,
        release_version: String::from(tachyon_contracts::PRODUCT_VERSION),
        commit: source_revision(),
        source_date_epoch: std::env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        target: artifact_target(target),
        toolchains: vec![
            ArtifactToolchain {
                name: String::from("rust"),
                version: String::from("1.97.1"),
            },
            ArtifactToolchain {
                name: generated.toolchain_name.clone(),
                version: generated.toolchain_version.clone(),
            },
        ],
        contracts: ArtifactContractVersions::default(),
        outputs,
    }
}

fn source_revision() -> String {
    std::env::var("GITHUB_SHA")
        .ok()
        .filter(|value| {
            matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .unwrap_or_else(|| "0".repeat(40))
}

fn collect_outputs(root: &Path, excluded: &str) -> Result<Vec<ArtifactOutput>, Failure> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
            .map_err(|error| native_io_failure(&directory, &error))?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| native_io_failure(&path, &error))?;
            if metadata.file_type().is_symlink() {
                return Err(Failure::one(diagnostic(
                    1605,
                    "Native output contains a symlink.",
                    None,
                    None,
                )));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                let relative = portable_path(path.strip_prefix(root).unwrap_or(&path));
                if relative != excluded {
                    let bytes =
                        fs::read(&path).map_err(|error| native_io_failure(&path, &error))?;
                    files.push(ArtifactOutput {
                        path: relative,
                        sha256: sha256_bytes(&bytes),
                        size: metadata.len(),
                    });
                }
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn digest_tree(root: &Path) -> Result<String, Failure> {
    let outputs = collect_outputs(root, "\0")?;
    let mut hasher = Sha256::new();
    for output in outputs {
        hasher.update(output.path.as_bytes());
        hasher.update([0]);
        hasher.update(output.sha256.as_bytes());
        hasher.update([0]);
    }
    Ok(hex_digest(hasher.finalize()))
}

fn sha256_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    hex_digest(hasher.finalize())
}

fn hex_digest(value: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value.as_ref() {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn portable_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn native_io_failure(path: &Path, error: &io::Error) -> Failure {
    Failure::one(diagnostic(
        1605,
        format!("Native build cannot access '{}': {error}", path.display()),
        Some(String::from(
            "Check permissions and keep generated output paths regular and project-contained.",
        )),
        None,
    ))
}

const CONTROLLER_RUNTIME: &str = r"const state = new Map()
const valid = /^[A-Za-z_][A-Za-z0-9_]{0,63}$/
for (const node of document.querySelectorAll('[data-tachyon-bind][data-tachyon-state]')) {
  const key = node.dataset.tachyonBind
  if (valid.test(key) && !state.has(key)) state.set(key, node.dataset.tachyonState)
}
const render = (key) => {
  const value = state.get(key) ?? ''
  for (const node of document.querySelectorAll('[data-tachyon-bind]')) {
    if (node.dataset.tachyonBind !== key) continue
    if ('value' in node && node.matches('input,textarea')) node.value = value
    else node.textContent = value
  }
}
for (const key of state.keys()) render(key)
document.addEventListener('input', (event) => {
  const key = event.target?.dataset?.tachyonBind
  if (!valid.test(key || '') || !state.has(key)) return
  state.set(key, String(event.target.value).slice(0, 4096))
  render(key)
})
document.addEventListener('click', (event) => {
  const target = event.target.closest('[data-tachyon-action]')
  if (!target) return
  const [verb, key, extra] = target.dataset.tachyonAction.split(':')
  if (extra !== undefined || !valid.test(key || '') || !state.has(key)) return
  if (verb === 'increment') {
    const value = Number.parseInt(state.get(key), 10)
    if (Number.isSafeInteger(value)) state.set(key, String(value + 1))
  } else if (verb === 'toggle') {
    state.set(key, state.get(key) === 'true' ? 'false' : 'true')
  } else return
  render(key)
})
document.documentElement.dataset.tachyonController = 'mounted'
globalThis.addEventListener('pageshow', () => {
  document.documentElement.dataset.tachyonController = 'active'
})
globalThis.addEventListener('pagehide', () => {
  document.documentElement.dataset.tachyonController = 'suspended'
})
";

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        NativeBuildOptions, NativeCompiler, client_route_state, inject_before_body, linked_styles,
        route_requires_controller, source_revision,
    };
    use std::fs;

    #[test]
    fn controller_script_injection_preserves_documents_and_fragments() {
        assert_eq!(
            inject_before_body("<body>x</body>", "<script></script>"),
            "<body>x<script></script></body>"
        );
        assert_eq!(
            inject_before_body("<main>x</main>", "<script></script>"),
            "<main>x</main><script></script>"
        );
    }

    #[test]
    fn controller_detection_covers_authored_html_and_client_view_plans() {
        for document in [
            r#"<button data-tachyon-action="increment:count">Add</button>"#,
            r#"<output data-tachyon-bind="count">0</output>"#,
            r#"<script id="tachyon-view">{"attributes":[{"name":"data-tachyon-action","value":"increment:count"}]}</script>"#,
            r#"<script id="tachyon-view">{"attributes":[{"name":"data-tachyon-bind","value":"count"}]}</script>"#,
        ] {
            assert!(route_requires_controller(document), "{document}");
        }
        assert!(!route_requires_controller(
            r#"<script id="tachyon-view">{"attributes":[{"name":"class","value":"count"}]}</script>"#
        ));
    }

    #[test]
    fn client_route_state_reads_bounded_scalar_plan_values() {
        let state = client_route_state(
            r#"<!doctype html><script id="tachyon-view" type="application/json">{"state":{"count":0,"ready":true,"label":"ok","none":null,"items":[1,2]}}</script>"#,
        );
        assert_eq!(state.get("count").map(String::as_str), Some("0"));
        assert_eq!(state.get("ready").map(String::as_str), Some("true"));
        assert_eq!(state.get("label").map(String::as_str), Some("ok"));
        assert_eq!(state.get("none").map(String::as_str), Some(""));
        assert!(!state.contains_key("items"));
    }

    #[test]
    fn fallback_revision_always_satisfies_the_manifest_shape() {
        assert!(matches!(source_revision().len(), 40 | 64));
        assert!(
            source_revision()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }

    #[test]
    fn web_surface_styles_flatten_nested_same_bundle_imports() {
        let bundle = tempfile::tempdir().expect("bundle");
        fs::create_dir_all(bundle.path().join("pages")).expect("pages");
        fs::create_dir_all(bundle.path().join("shared/assets")).expect("assets");
        fs::write(
            bundle.path().join("pages/home.css"),
            "@import url('/shared/site.css');\nmain{display:block}",
        )
        .expect("route style");
        fs::write(
            bundle.path().join("shared/site.css"),
            "@import '../shared/assets/design.css';\nbody{color:var(--ink)}",
        )
        .expect("site style");
        fs::write(
            bundle.path().join("shared/assets/design.css"),
            ":root{--ink:#123456}",
        )
        .expect("design style");

        let styles = linked_styles(
            bundle.path(),
            r#"<link rel="stylesheet" href="/pages/home.css">"#,
        );
        assert!(styles.contains(":root{--ink:#123456}"), "{styles}");
        assert!(styles.contains("body{color:var(--ink)}"), "{styles}");
        assert!(styles.contains("main{display:block}"), "{styles}");
        assert!(!styles.contains("@import"), "{styles}");
    }

    #[test]
    fn web_surface_styles_bound_cycles_and_preserve_remote_imports() {
        let bundle = tempfile::tempdir().expect("bundle");
        fs::write(
            bundle.path().join("one.css"),
            "@import 'two.css';@import 'https://example.test/font.css';.one{color:red}",
        )
        .expect("one");
        fs::write(
            bundle.path().join("two.css"),
            "@import 'one.css';.two{color:blue}",
        )
        .expect("two");

        let styles = linked_styles(bundle.path(), r#"<link rel="stylesheet" href="/one.css">"#);
        assert!(styles.contains(".one{color:red}"), "{styles}");
        assert!(styles.contains(".two{color:blue}"), "{styles}");
        assert!(styles.contains("https://example.test/font.css"), "{styles}");
        assert!(styles.len() < 512, "{styles}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn in_process_native_build_covers_packaging_and_public_evidence() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            root.path().join("tachyon.json"),
            r#"{"application":{"name":"Coverage","id":"dev.tachyon.coverage","version":"1.0.0","entry_route":"/"}}"#,
        )
        .expect("configuration");
        fs::write(
            pages.join("tac.html"),
            r#"<main aria-label="Coverage"><h1>Coverage</h1><button aria-label="Add" data-tachyon-action="increment:count">Add</button><output aria-label="Count" data-tachyon-bind="count" data-tachyon-state="0">0</output><input aria-label="Name" data-tachyon-bind="name" data-tachyon-state=""><details aria-label="Details"><summary>Details</summary><p>Open</p></details><x-chart aria-label="Chart">Web</x-chart><iframe aria-label="Report" src="https://reports.example.test/path"></iframe></main>"#,
        )
        .expect("view");

        let result =
            NativeCompiler::build(root.path(), &NativeBuildOptions::default()).expect("build");
        assert!(result.output_directory().ends_with("dist/macos"));
        assert!(result.application_bundle().is_dir());
        assert_eq!(result.route_count(), 1);
        assert!(result.native_node_count() >= 6);
        assert_eq!(result.web_surface_count(), 2);
        assert_eq!(result.sha256().len(), 64);
    }

    #[test]
    fn invalid_entry_route_fails_before_native_tool_execution() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            root.path().join("tachyon.json"),
            r#"{"application":{"name":"Entry","id":"dev.tachyon.entry","version":"1","entry_route":"/missing"}}"#,
        )
        .expect("configuration");
        fs::write(pages.join("tac.html"), "<main>Home</main>").expect("view");
        let error = NativeCompiler::build(root.path(), &NativeBuildOptions::default())
            .expect_err("missing entry");
        assert!(error.to_string().contains("TY1601"));
    }
}
