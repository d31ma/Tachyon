use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::handler::{HandlerLanguage, HandlerSource};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tachyon_contracts::{HttpMethod, RouteContext, RouteEntry, RouteKind, RouteManifest};

const TAC_ROOT: &str = "client/pages";
const YON_ROOT: &str = "server/routes";

/// The frontend that owns a discovered view source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewKind {
    /// A client `tac.html` view.
    Tac,
    /// A static server `yon.html` view.
    Yon,
}

impl ViewKind {
    const fn file_name(self) -> &'static str {
        match self {
            Self::Tac => "tac.html",
            Self::Yon => "yon.html",
        }
    }

    const fn stem(self) -> &'static str {
        match self {
            Self::Tac => "tac.",
            Self::Yon => "yon.",
        }
    }
}

/// One validated Yon handler contributor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerNode {
    source_path: String,
    absolute_source_path: PathBuf,
    language: HandlerLanguage,
}

impl HandlerNode {
    /// Returns the portable project-relative source path.
    #[must_use]
    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    /// Returns the validated absolute source path.
    #[must_use]
    pub fn absolute_source_path(&self) -> &Path {
        &self.absolute_source_path
    }

    /// Returns the selected Phase 2 adapter language.
    #[must_use]
    pub const fn language(&self) -> HandlerLanguage {
        self.language
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ViewSource {
    source_path: String,
    absolute_source_path: PathBuf,
    view_kind: ViewKind,
}

/// One validated route, optional view, and ordered handler contributors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteNode {
    route: String,
    view: Option<ViewSource>,
    handlers: Vec<HandlerNode>,
    companions: Vec<CompanionSource>,
}

/// A colocated asset compiled alongside a view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanionSource {
    /// Portable project-relative source path.
    pub source_path: String,
    /// Validated absolute source path.
    pub absolute_source_path: PathBuf,
    /// What the compiler emits for this companion.
    pub kind: CompanionKind,
}

/// The emitted form of a colocated companion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompanionKind {
    /// A stylesheet linked from the generated document.
    Style,
    /// A client module referenced by the generated document.
    ClientModule,
    /// A `TypeScript` client module transpiled before emission.
    TypeScriptModule,
}

impl CompanionKind {
    /// Returns the generated file name for this companion.
    #[must_use]
    pub const fn output_name(self) -> &'static str {
        match self {
            Self::Style => "style.css",
            Self::ClientModule | Self::TypeScriptModule => "client.js",
        }
    }
}

impl RouteNode {
    /// Returns the canonical URL route.
    #[must_use]
    pub fn route(&self) -> &str {
        &self.route
    }

    /// Returns the portable project-relative source path.
    #[must_use]
    pub fn source_path(&self) -> Option<&str> {
        self.view.as_ref().map(|view| view.source_path.as_str())
    }

    /// Returns the validated absolute source path.
    #[must_use]
    pub fn absolute_source_path(&self) -> Option<&Path> {
        self.view
            .as_ref()
            .map(|view| view.absolute_source_path.as_path())
    }

    /// Returns the owning view frontend.
    #[must_use]
    pub fn view_kind(&self) -> Option<ViewKind> {
        self.view.as_ref().map(|view| view.view_kind)
    }

    /// Returns handler contributors in canonical source-path order.
    #[must_use]
    pub fn handlers(&self) -> &[HandlerNode] {
        &self.handlers
    }

    /// Returns colocated companions in canonical source-path order.
    #[must_use]
    pub fn companions(&self) -> &[CompanionSource] {
        &self.companions
    }

    /// Returns the ordered dynamic segment names for this route.
    ///
    /// A segment authored as `_id` contributes the parameter `id`.
    #[must_use]
    pub fn parameters(&self) -> Vec<String> {
        route_parameters(&self.route)
    }

    /// Returns whether this route has at least one dynamic segment.
    #[must_use]
    pub fn is_dynamic(&self) -> bool {
        self.route
            .split('/')
            .any(|segment| segment.starts_with('_'))
    }

    /// Returns the deterministic output path below the build directory.
    ///
    /// A dynamic route has no single prerendered document, because its content
    /// depends on the values bound at request time.
    #[must_use]
    pub fn output_path(&self) -> Option<PathBuf> {
        self.view.as_ref()?;
        if self.is_dynamic() {
            return None;
        }
        let mut path = PathBuf::new();
        for segment in self.route.trim_start_matches('/').split('/') {
            if !segment.is_empty() {
                path.push(segment);
            }
        }
        path.push("index.html");
        Some(path)
    }

    /// Returns the portable output path for a route's authored template.
    ///
    /// Static routes use their ordinary output path. Dynamic routes retain
    /// their `_parameter` segment so preview and native runtimes can match a
    /// concrete URL without inventing one parameter value at build time.
    #[must_use]
    pub fn template_output_path(&self) -> Option<PathBuf> {
        self.view.as_ref()?;
        let mut path = PathBuf::new();
        for segment in self.route.trim_start_matches('/').split('/') {
            if !segment.is_empty() {
                path.push(segment);
            }
        }
        path.push("index.html");
        Some(path)
    }
}

/// An immutable, canonically ordered route graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteGraph {
    routes: Vec<RouteNode>,
}

impl RouteGraph {
    /// Returns routes in canonical URL order.
    #[must_use]
    pub fn routes(&self) -> &[RouteNode] {
        &self.routes
    }

    /// Converts the graph to Route Manifest v1.
    #[must_use]
    pub fn manifest(&self) -> RouteManifest {
        let routes = self
            .routes
            .iter()
            .map(|route| RouteEntry {
                route: route.route.clone(),
                parameters: route.parameters(),
                kind: if route.view.is_some() {
                    RouteKind::Page
                } else {
                    RouteKind::Api
                },
                methods: if route.handlers.is_empty() {
                    vec![HttpMethod::Get, HttpMethod::Head]
                } else {
                    vec![
                        HttpMethod::Delete,
                        HttpMethod::Get,
                        HttpMethod::Head,
                        HttpMethod::Options,
                        HttpMethod::Patch,
                        HttpMethod::Post,
                        HttpMethod::Put,
                    ]
                },
                view: route.view.as_ref().map(|view| view.source_path.clone()),
                handlers: route
                    .handlers
                    .iter()
                    .map(|handler| tachyon_contracts::RouteHandler {
                        source: handler.source_path.clone(),
                        language: String::from(handler.language.name()),
                        runtime: String::from(handler.language.adapter()),
                    })
                    .collect(),
                context: RouteContext::default(),
            })
            .collect();
        RouteManifest::v1(routes)
    }
}

/// A discovered Tachyon project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    root: PathBuf,
    route_graph: RouteGraph,
}

impl Project {
    /// Returns the canonical project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the immutable route graph.
    #[must_use]
    pub const fn route_graph(&self) -> &RouteGraph {
        &self.route_graph
    }
}

/// Discovers Tachyon projects without executing application source.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectDiscovery;

impl ProjectDiscovery {
    /// Discovers and validates a project rooted at `root`.
    ///
    /// # Errors
    ///
    /// Returns stable diagnostics for an invalid root, unsafe filesystem shape,
    /// unsupported source, route collision, or missing view/handler.
    pub fn discover(root: impl AsRef<Path>) -> Result<Project, Failure> {
        let canonical_root = canonical_project_root(root.as_ref())?;
        let mut found = Discovered::default();
        discover_views(
            &canonical_root,
            Path::new(TAC_ROOT),
            ViewKind::Tac,
            &mut found,
        );
        discover_views(
            &canonical_root,
            Path::new(YON_ROOT),
            ViewKind::Yon,
            &mut found,
        );
        let Discovered {
            views,
            handlers,
            companions,
            diagnostics,
        } = found;
        if !diagnostics.is_empty() {
            return Err(Failure::new(diagnostics));
        }
        if views.is_empty() && handlers.is_empty() {
            return Err(Failure::one(diagnostic(
                1002,
                "No Tachyon view or handler source was found.",
                Some(String::from(
                    "Add a Tac/Yon HTML view or server/routes/yon.js or yon.py.",
                )),
                None,
            )));
        }

        let route_graph = build_route_graph(views, handlers, companions)?;
        Ok(Project {
            root: canonical_root,
            route_graph,
        })
    }
}

fn canonical_project_root(root: &Path) -> Result<PathBuf, Failure> {
    let canonical = match fs::canonicalize(root) {
        Ok(canonical) => canonical,
        Err(error) => {
            return Err(Failure::one(diagnostic(
                1001,
                format!("Cannot open project root '{}': {error}", root.display()),
                Some(String::from("Pass an existing readable project directory.")),
                None,
            )));
        }
    };
    let metadata = match fs::metadata(&canonical) {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(Failure::one(diagnostic(
                1001,
                format!(
                    "Cannot inspect project root '{}': {error}",
                    canonical.display()
                ),
                None,
                None,
            )));
        }
    };
    if !metadata.is_dir() {
        return Err(Failure::one(diagnostic(
            1001,
            format!("Project root '{}' is not a directory.", canonical.display()),
            None,
            None,
        )));
    }
    Ok(canonical)
}

fn discover_views(
    project_root: &Path,
    relative_root: &Path,
    kind: ViewKind,
    found: &mut Discovered,
) {
    let source_root = project_root.join(relative_root);
    let metadata = match fs::symlink_metadata(&source_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            found.diagnostics.push(diagnostic(
                1001,
                format!("Cannot inspect '{}': {error}", portable(relative_root)),
                None,
                None,
            ));
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        found.diagnostics.push(unsafe_symlink(relative_root));
        return;
    }
    if !metadata.is_dir() {
        found.diagnostics.push(diagnostic(
            1001,
            format!(
                "Source root '{}' is not a directory.",
                portable(relative_root)
            ),
            None,
            None,
        ));
        return;
    }
    visit_directory(project_root, relative_root, relative_root, kind, found);
}

fn visit_directory(
    project_root: &Path,
    source_root: &Path,
    relative_directory: &Path,
    kind: ViewKind,
    found: &mut Discovered,
) {
    let absolute_directory = project_root.join(relative_directory);
    let entries = match fs::read_dir(&absolute_directory) {
        Ok(entries) => entries,
        Err(error) => {
            found.diagnostics.push(diagnostic(
                1001,
                format!(
                    "Cannot read source directory '{}': {error}",
                    portable(relative_directory)
                ),
                None,
                None,
            ));
            return;
        }
    };
    let mut entries = match entries.collect::<Result<Vec<_>, _>>() {
        Ok(entries) => entries,
        Err(error) => {
            found.diagnostics.push(diagnostic(
                1001,
                format!(
                    "Cannot enumerate source directory '{}': {error}",
                    portable(relative_directory)
                ),
                None,
                None,
            ));
            return;
        }
    };
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(project_root) else {
            continue;
        };
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                found.diagnostics.push(diagnostic(
                    1001,
                    format!("Cannot inspect '{}': {error}", portable(relative)),
                    None,
                    None,
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            found.diagnostics.push(unsafe_symlink(relative));
        } else if metadata.is_dir() {
            visit_directory(project_root, source_root, relative, kind, found);
        } else if metadata.is_file() {
            inspect_source_file(project_root, source_root, relative, kind, found);
        }
    }
}

fn inspect_source_file(
    project_root: &Path,
    source_root: &Path,
    relative: &Path,
    kind: ViewKind,
    found: &mut Discovered,
) {
    let Some(name) = relative.file_name().and_then(OsStr::to_str) else {
        found.diagnostics.push(diagnostic(
            1005,
            "Source paths must be valid Unicode.",
            None,
            None,
        ));
        return;
    };
    if name == kind.file_name() {
        match route_for(relative, source_root) {
            Ok(route) => found.views.push((
                route,
                ViewSource {
                    source_path: portable(relative),
                    absolute_source_path: project_root.join(relative),
                    view_kind: kind,
                },
            )),
            Err(failure) => found.diagnostics.extend_from_slice(failure.diagnostics()),
        }
    } else if kind == ViewKind::Yon
        && name.starts_with("yon.")
        && name != "yon.html"
        && companion_kind(kind, name).is_none()
    {
        // Any yon.<extension> is a handler candidate. Whether it can run is
        // decided by HandlerSource, which resolves built-in adapters, a
        // .tachyonrc interpreter, or an executable file, and reports why not.
        match (
            route_for(relative, source_root),
            HandlerSource::discover(project_root, relative),
        ) {
            (Ok(route), Ok(source)) => found.handlers.push((
                route,
                HandlerNode {
                    source_path: String::from(source.relative_path()),
                    absolute_source_path: source.absolute_path().to_path_buf(),
                    language: source.language(),
                },
            )),
            (Err(failure), _) | (_, Err(failure)) => {
                found.diagnostics.extend_from_slice(failure.diagnostics());
            }
        }
    } else if let Some(companion_kind) = companion_kind(kind, name) {
        match route_for(relative, source_root) {
            Ok(route) => found.companions.push((
                route,
                CompanionSource {
                    source_path: portable(relative),
                    absolute_source_path: project_root.join(relative),
                    kind: companion_kind,
                },
            )),
            Err(failure) => found.diagnostics.extend_from_slice(failure.diagnostics()),
        }
    } else if name.starts_with(kind.stem()) {
        let path = portable(relative);
        found.diagnostics.push(diagnostic(
            1008,
            format!("Companion source '{path}' has no available adapter."),
            Some(String::from(
                "Supported companions are tac.css, yon.css, tac.js, and server \
                 route yon.js and yon.py handlers.",
            )),
            source_span(&path, 0, name.len()),
        ));
    }
}

fn route_for(relative: &Path, source_root: &Path) -> Result<String, Failure> {
    let Ok(beneath_root) = relative.strip_prefix(source_root) else {
        return Err(Failure::one(diagnostic(
            1004,
            format!("Source '{}' escaped its source root.", portable(relative)),
            None,
            None,
        )));
    };
    let parent = beneath_root.parent().unwrap_or(Path::new(""));
    let mut segments = Vec::new();
    let mut parameters: Vec<String> = Vec::new();
    for component in parent.components() {
        let Component::Normal(segment) = component else {
            return Err(invalid_route(relative));
        };
        let Some(segment) = segment.to_str() else {
            return Err(invalid_route(relative));
        };
        if let Some(parameter) = segment.strip_prefix('_') {
            if !valid_parameter_name(parameter) {
                let path = portable(relative);
                return Err(Failure::one(diagnostic(
                    1006,
                    format!("Dynamic route segment '{segment}' has an invalid name."),
                    Some(String::from(
                        "Use `_name` where name is lowercase alphanumeric with underscores.",
                    )),
                    source_span(&path, 0, path.len()),
                )));
            }
            if parameters.iter().any(|existing| existing == parameter) {
                let path = portable(relative);
                return Err(Failure::one(diagnostic(
                    1006,
                    format!("Dynamic route segment '{parameter}' is declared twice."),
                    Some(String::from(
                        "Give each dynamic segment in one route a unique name.",
                    )),
                    source_span(&path, 0, path.len()),
                )));
            }
            parameters.push(String::from(parameter));
            segments.push(segment);
            continue;
        }
        if !valid_route_segment(segment) {
            return Err(invalid_route(relative));
        }
        segments.push(segment);
    }
    if segments.is_empty() {
        Ok(String::from("/"))
    } else {
        Ok(format!("/{}", segments.join("/")))
    }
}

/// Returns the ordered dynamic parameter names encoded in a route.
fn route_parameters(route: &str) -> Vec<String> {
    route
        .split('/')
        .filter_map(|segment| segment.strip_prefix('_'))
        .map(String::from)
        .collect()
}

/// Returns whether a dynamic segment name is a bounded lowercase identifier.
fn valid_parameter_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_route_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('-')
        && !segment.ends_with('-')
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !segment.as_bytes().windows(2).any(|pair| pair == b"--")
}

fn invalid_route(relative: &Path) -> Failure {
    let path = portable(relative);
    Failure::one(diagnostic(
        1005,
        format!("View source '{path}' contains an invalid static route segment."),
        Some(String::from(
            "Use lowercase ASCII letters, digits, and single internal hyphens.",
        )),
        source_span(&path, 0, path.len()),
    ))
}

/// Returns the companion kind for a colocated file name, if one applies.
fn companion_kind(kind: ViewKind, name: &str) -> Option<CompanionKind> {
    match (kind, name) {
        (_, "tac.css" | "yon.css") => Some(CompanionKind::Style),
        (ViewKind::Tac, "tac.js") => Some(CompanionKind::ClientModule),
        (ViewKind::Tac, "tac.ts") => Some(CompanionKind::TypeScriptModule),
        _ => None,
    }
}

/// Everything one discovery pass accumulates.
#[derive(Debug, Default)]
struct Discovered {
    views: Vec<(String, ViewSource)>,
    handlers: Vec<(String, HandlerNode)>,
    companions: Vec<(String, CompanionSource)>,
    diagnostics: Vec<tachyon_diagnostics::Diagnostic>,
}

fn build_route_graph(
    views: Vec<(String, ViewSource)>,
    handlers: Vec<(String, HandlerNode)>,
    companions: Vec<(String, CompanionSource)>,
) -> Result<RouteGraph, Failure> {
    let mut routes: BTreeMap<String, RouteNode> = BTreeMap::new();
    for (route, view) in views {
        let node = routes.entry(route.clone()).or_insert_with(|| RouteNode {
            route: route.clone(),
            view: None,
            handlers: Vec::new(),
            companions: Vec::new(),
        });
        if let Some(existing) = &node.view {
            return Err(Failure::one(diagnostic(
                1003,
                format!(
                    "Route '{}' is claimed by both '{}' and '{}'.",
                    route, existing.source_path, view.source_path
                ),
                Some(String::from(
                    "Keep exactly one Tac or Yon view for each route.",
                )),
                source_span(&view.source_path, 0, view.source_path.len()),
            )));
        }
        node.view = Some(view);
    }
    for (route, handler) in handlers {
        routes
            .entry(route.clone())
            .or_insert_with(|| RouteNode {
                route,
                view: None,
                handlers: Vec::new(),
                companions: Vec::new(),
            })
            .handlers
            .push(handler);
    }
    for (route, companion) in companions {
        routes
            .entry(route.clone())
            .or_insert_with(|| RouteNode {
                route,
                view: None,
                handlers: Vec::new(),
                companions: Vec::new(),
            })
            .companions
            .push(companion);
    }
    for route in routes.values_mut() {
        route.companions.sort_by(|left, right| {
            left.source_path
                .as_bytes()
                .cmp(right.source_path.as_bytes())
        });
        route.handlers.sort_by(|left, right| {
            left.source_path
                .as_bytes()
                .cmp(right.source_path.as_bytes())
        });
    }
    Ok(RouteGraph {
        routes: routes.into_values().collect(),
    })
}

fn unsafe_symlink(relative: &Path) -> tachyon_diagnostics::Diagnostic {
    let path = portable(relative);
    diagnostic(
        1004,
        format!("Symlinked project source '{path}' is not allowed."),
        Some(String::from(
            "Replace the symlink with a regular file or directory.",
        )),
        source_span(&path, 0, path.len()),
    )
}

fn portable(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{ProjectDiscovery, ViewKind};
    use std::fs;

    #[test]
    fn routes_are_sorted_and_manifested() {
        let root = tempfile::tempdir().expect("project");
        fs::create_dir_all(root.path().join("client/pages/zeta")).expect("directory");
        fs::create_dir_all(root.path().join("client/pages/alpha")).expect("directory");
        fs::write(root.path().join("client/pages/zeta/tac.html"), "<p>Z</p>").expect("source");
        fs::write(root.path().join("client/pages/alpha/tac.html"), "<p>A</p>").expect("source");

        let project = ProjectDiscovery::discover(root.path()).expect("discovery");
        let routes = project.route_graph().routes();
        assert_eq!(routes[0].route(), "/alpha");
        assert_eq!(routes[1].route(), "/zeta");
        assert_eq!(
            routes[0].output_path(),
            Some(std::path::PathBuf::from("alpha/index.html"))
        );
        assert_eq!(project.route_graph().manifest().contract_version, 1);
        assert_eq!(project.root(), fs::canonicalize(root.path()).expect("root"));
        assert_eq!(routes[0].source_path(), Some("client/pages/alpha/tac.html"));
        assert_eq!(routes[0].view_kind(), Some(ViewKind::Tac));
        assert!(
            routes[0]
                .absolute_source_path()
                .is_some_and(std::path::Path::is_absolute)
        );
    }

    #[test]
    fn invalid_static_segments_are_rejected() {
        // `_slug` is no longer here: dynamic segments are supported and are
        // covered by `dynamic_route_segments_are_discovered_with_their_parameters`.
        for (segment, code) in [
            ("Not-Lower", "TY1005"),
            ("-start", "TY1005"),
            ("end-", "TY1005"),
            ("two--hyphens", "TY1005"),
            ("café", "TY1005"),
        ] {
            let root = tempfile::tempdir().expect("project");
            let directory = root.path().join("client/pages").join(segment);
            fs::create_dir_all(&directory).expect("directory");
            fs::write(directory.join("tac.html"), "<p>Page</p>").expect("source");
            let error = ProjectDiscovery::discover(root.path()).expect_err("invalid route");
            assert!(error.to_string().contains(code));
        }
    }

    #[test]
    fn missing_invalid_and_empty_roots_have_stable_diagnostics() {
        let root = tempfile::tempdir().expect("workspace");
        let missing = root.path().join("missing");
        assert!(
            ProjectDiscovery::discover(&missing)
                .expect_err("missing")
                .to_string()
                .contains("TY1001")
        );

        let file = root.path().join("file");
        fs::write(&file, "not a directory").expect("file");
        assert!(
            ProjectDiscovery::discover(&file)
                .expect_err("file root")
                .to_string()
                .contains("TY1001")
        );
        assert!(
            ProjectDiscovery::discover(root.path())
                .expect_err("empty")
                .to_string()
                .contains("TY1002")
        );

        fs::write(root.path().join("client"), "not a directory").expect("source root file");
        assert!(
            ProjectDiscovery::discover(root.path())
                .expect_err("source root")
                .to_string()
                .contains("TY1001")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_sources_are_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("workspace");
        let outside = root.path().join("outside.html");
        fs::write(&outside, "<main>Outside</main>").expect("outside source");
        fs::create_dir_all(root.path().join("client/pages")).expect("source root");
        symlink(&outside, root.path().join("client/pages/tac.html")).expect("symlink");
        let error = ProjectDiscovery::discover(root.path()).expect_err("symlink");
        assert!(error.to_string().contains("TY1004"));
    }

    #[test]
    fn route_collisions_and_companions_are_rejected() {
        let collision = tempfile::tempdir().expect("collision project");
        fs::create_dir_all(collision.path().join("client/pages")).expect("Tac root");
        fs::create_dir_all(collision.path().join("server/routes")).expect("Yon root");
        fs::write(
            collision.path().join("client/pages/tac.html"),
            "<main>Tac</main>",
        )
        .expect("Tac source");
        fs::write(
            collision.path().join("server/routes/yon.html"),
            "<main>Yon</main>",
        )
        .expect("Yon source");
        assert!(
            ProjectDiscovery::discover(collision.path())
                .expect_err("collision")
                .to_string()
                .contains("TY1003")
        );

        // A yon.<extension> with no registered interpreter and no executable
        // bit is now a handler candidate that cannot run, so it reports the
        // handler diagnostic naming both remedies rather than TY1008.
        let companion = tempfile::tempdir().expect("companion project");
        fs::create_dir_all(companion.path().join("server/routes")).expect("Yon root");
        fs::write(
            companion.path().join("server/routes/yon.rb"),
            "class Handler; end",
        )
        .expect("companion");
        assert!(
            ProjectDiscovery::discover(companion.path())
                .expect_err("companion")
                .to_string()
                .contains("TY2003")
        );
    }

    #[test]
    fn handler_only_and_composed_routes_are_manifested_canonically() {
        let root = tempfile::tempdir().expect("project");
        fs::create_dir_all(root.path().join("server/routes/api")).expect("api");
        fs::create_dir_all(root.path().join("client/pages/products")).expect("page");
        fs::create_dir_all(root.path().join("server/routes/products")).expect("handlers");
        fs::write(
            root.path().join("server/routes/api/yon.py"),
            "class Handler: pass",
        )
        .expect("Python");
        fs::write(
            root.path().join("client/pages/products/tac.html"),
            "<main>Products</main>",
        )
        .expect("view");
        fs::write(
            root.path().join("server/routes/products/yon.py"),
            "class Handler: pass",
        )
        .expect("Python");
        fs::write(
            root.path().join("server/routes/products/yon.js"),
            "export class Handler {}",
        )
        .expect("JavaScript");

        let project = ProjectDiscovery::discover(root.path()).expect("discovery");
        let routes = project.route_graph().routes();
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].route(), "/api");
        assert!(routes[0].source_path().is_none());
        assert_eq!(routes[0].handlers().len(), 1);
        assert_eq!(routes[1].route(), "/products");
        assert_eq!(routes[1].handlers().len(), 2);
        assert_eq!(
            routes[1].handlers()[0].source_path(),
            "server/routes/products/yon.js"
        );
        assert_eq!(
            routes[1].handlers()[1].language(),
            super::HandlerLanguage::Python
        );

        let manifest = project.route_graph().manifest();
        assert_eq!(manifest.routes[0].kind, tachyon_contracts::RouteKind::Api);
        assert_eq!(manifest.routes[0].methods.len(), 7);
        assert_eq!(manifest.routes[1].kind, tachyon_contracts::RouteKind::Page);
        assert_eq!(manifest.routes[1].handlers[0].runtime, "javascript.v1");
    }

    #[test]
    fn dynamic_route_segments_are_discovered_with_their_parameters() {
        let root = tempfile::tempdir().expect("project");
        for (path, body) in [
            ("client/pages/tac.html", "<main>home</main>"),
            ("client/pages/items/_id/tac.html", "<main>item</main>"),
            (
                "client/pages/items/_id/reviews/_review/tac.html",
                "<main>r</main>",
            ),
        ] {
            let file = root.path().join(path);
            fs::create_dir_all(file.parent().expect("parent")).expect("dir");
            fs::write(&file, body).expect("source");
        }
        let project = ProjectDiscovery::discover(root.path()).expect("discover");
        let routes = project.route_graph().routes();

        let item = routes
            .iter()
            .find(|route| route.route() == "/items/_id")
            .expect("dynamic route");
        assert!(item.is_dynamic());
        assert_eq!(item.parameters(), vec![String::from("id")]);
        // A dynamic route has no single prerendered document.
        assert!(item.output_path().is_none());

        let nested = routes
            .iter()
            .find(|route| route.route() == "/items/_id/reviews/_review")
            .expect("nested dynamic route");
        assert_eq!(
            nested.parameters(),
            vec![String::from("id"), String::from("review")]
        );

        let home = routes
            .iter()
            .find(|route| route.route() == "/")
            .expect("static route");
        assert!(!home.is_dynamic());
        assert!(home.parameters().is_empty());
        assert!(home.output_path().is_some());
    }

    #[test]
    fn invalid_dynamic_segments_fail_closed() {
        for path in [
            "client/pages/_/tac.html",       // no name
            "client/pages/_1id/tac.html",    // leading digit
            "client/pages/_Id/tac.html",     // uppercase
            "client/pages/_id/_id/tac.html", // duplicate in one route
        ] {
            let root = tempfile::tempdir().expect("project");
            let file = root.path().join(path);
            fs::create_dir_all(file.parent().expect("parent")).expect("dir");
            fs::write(&file, "<main>x</main>").expect("source");
            let error = ProjectDiscovery::discover(root.path())
                .err()
                .unwrap_or_else(|| panic!("{path} was accepted"));
            assert!(
                error.to_string().contains("TY1006") || error.to_string().contains("TY1005"),
                "{path}: {error}"
            );
        }
    }
}
