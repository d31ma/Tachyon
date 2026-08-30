use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::handler::{HandlerLanguage, HandlerSource, OwnedSourceRoot};
use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tachyon_contracts::{HttpMethod, RouteContext, RouteEntry, RouteKind, RouteManifest};

const TAC_ROOT: &str = "client/pages";
const YON_ROOT: &str = "server/routes";
const MAX_CAPTURE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_CAPTURE_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CAPTURE_FILES: usize = 4_096;

/// The frontend that owns a discovered view source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewKind {
    /// A client `tac.html` view.
    Tac,
}

impl ViewKind {
    const fn file_name(self) -> &'static str {
        match self {
            Self::Tac => "tac.html",
        }
    }

    const fn stem(self) -> &'static str {
        match self {
            Self::Tac => "tac.",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceKind {
    Tac,
    Yon,
}

/// One validated Yon handler contributor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerNode {
    source_path: String,
    absolute_source_path: PathBuf,
    language: HandlerLanguage,
    source: HandlerSource,
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

    pub(crate) fn source(&self) -> &HandlerSource {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ViewSource {
    source_path: String,
    absolute_source_path: PathBuf,
    view_kind: ViewKind,
    bytes: Vec<u8>,
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
    bytes: Vec<u8>,
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

impl CompanionSource {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
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

    pub(crate) fn view_bytes(&self) -> Option<&[u8]> {
        self.view.as_ref().map(|view| view.bytes.as_slice())
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
#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
    route_graph: RouteGraph,
    snapshot: Arc<OwnedSourceRoot>,
    capability: Arc<Dir>,
    middleware: Option<HandlerSource>,
    workers: Vec<ScheduledWorker>,
}

/// One validated scheduled worker bound to the project's immutable snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScheduledWorker {
    relative: String,
    every_seconds: u64,
    source: HandlerSource,
}

impl ScheduledWorker {
    pub(crate) fn relative(&self) -> &str {
        &self.relative
    }

    pub(crate) const fn every_seconds(&self) -> u64 {
        self.every_seconds
    }

    pub(crate) const fn source(&self) -> &HandlerSource {
        &self.source
    }
}

impl PartialEq for Project {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && self.route_graph == other.route_graph
    }
}

impl Eq for Project {}

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

    pub(crate) fn snapshot_root(&self) -> &Path {
        self.snapshot.path()
    }

    pub(crate) fn capability(&self) -> Arc<Dir> {
        Arc::clone(&self.capability)
    }

    pub(crate) fn middleware(&self) -> Option<&HandlerSource> {
        self.middleware.as_ref()
    }

    pub(crate) fn workers(&self) -> &[ScheduledWorker] {
        &self.workers
    }

    pub(crate) fn invocation_sources(&self) -> impl Iterator<Item = &HandlerSource> + '_ {
        self.route_graph
            .routes
            .iter()
            .flat_map(|route| route.handlers.iter().map(HandlerNode::source))
            .chain(self.middleware.iter())
            .chain(self.workers.iter().map(ScheduledWorker::source))
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
        let project =
            Dir::open_ambient_dir(&canonical_root, ambient_authority()).map_err(|error| {
                Failure::one(diagnostic(
                    1001,
                    format!(
                        "Cannot open project root '{}': {error}",
                        canonical_root.display()
                    ),
                    None,
                    None,
                ))
            })?;
        Self::discover_opened(canonical_root, &project)
    }

    #[allow(clippy::too_many_lines)]
    fn discover_opened(canonical_root: PathBuf, project: &Dir) -> Result<Project, Failure> {
        let mut found = Discovered::default();
        let tac = capture_source_root(project, Path::new(TAC_ROOT), &mut found.diagnostics);
        let components = capture_source_root(
            project,
            Path::new("client/components"),
            &mut found.diagnostics,
        );
        let shared =
            capture_source_root(project, Path::new("client/shared"), &mut found.diagnostics);
        let configs = [
            "package.json",
            "tac.config.js",
            "tachyon.json",
            ".tachyonrc",
        ]
        .into_iter()
        .filter_map(|relative| {
            capture_project_file(project, Path::new(relative), &mut found.diagnostics)
        })
        .collect::<Vec<_>>();
        let middleware_file = capture_root_middleware(project, &mut found.diagnostics);
        let server = open_source_root(project, Path::new("server"), &mut found.diagnostics);
        let server_files = server.as_ref().map_or_else(Vec::new, |server| {
            let mut files = Vec::new();
            visit_capability_directory(
                server,
                Path::new("server"),
                &mut found.diagnostics,
                &mut files,
            );
            files
        });
        let routes_valid = server.as_ref().is_some_and(|server| {
            validate_server_source_root(server, Path::new(YON_ROOT), &mut found.diagnostics)
        });
        let routes = if routes_valid {
            files_beneath(&server_files, Path::new(YON_ROOT))
        } else {
            Vec::new()
        };
        let mut layers = vec![(PathBuf::from(YON_ROOT), routes.clone())];
        for layer in crate::stereotype::Stereotype::ALL
            .into_iter()
            .filter(|layer| layer.root() != YON_ROOT)
        {
            let root = PathBuf::from(layer.root());
            let valid = server.as_ref().is_some_and(|server| {
                validate_server_source_root(server, &root, &mut found.diagnostics)
            });
            let files = if valid {
                files_beneath(&server_files, &root)
            } else {
                Vec::new()
            };
            layers.push((root, files));
        }
        check_captured_layers(&layers, &mut found.diagnostics);

        let source_root = OwnedSourceRoot::new_project(YON_ROOT)?;
        for file in tac
            .iter()
            .chain(components.iter())
            .chain(shared.iter())
            .chain(configs.iter())
            .chain(middleware_file.iter())
            .chain(server_files.iter())
        {
            if let Err(failure) = source_root.stage(&file.relative, &file.bytes, &file.portable()) {
                found
                    .diagnostics
                    .extend(failure.diagnostics().iter().cloned());
            }
        }
        for file in tac {
            inspect_source_file(
                &canonical_root,
                project,
                Path::new(TAC_ROOT),
                &file,
                SourceKind::Tac,
                &source_root,
                &mut found,
            );
        }
        for file in routes {
            inspect_source_file(
                &canonical_root,
                project,
                Path::new(YON_ROOT),
                &file,
                SourceKind::Yon,
                &source_root,
                &mut found,
            );
        }
        let middleware = middleware_file.and_then(|file| {
            match HandlerSource::discover_snapshot(
                canonical_root.clone(),
                project,
                &file.relative,
                file.bytes,
                Arc::clone(&source_root),
            ) {
                Ok(source) => Some(source),
                Err(failure) => {
                    found.diagnostics.extend_from_slice(failure.diagnostics());
                    None
                }
            }
        });
        let worker_config = configs
            .iter()
            .find(|file| file.relative == Path::new(".tachyonrc"))
            .map(|file| file.bytes.as_slice());
        let schedules = match crate::Workers::from_captured(worker_config) {
            Ok(workers) => workers,
            Err(failure) => {
                found.diagnostics.extend_from_slice(failure.diagnostics());
                crate::Workers::default()
            }
        };
        let mut workers = Vec::new();
        for (relative, every_seconds) in schedules.iter() {
            let Some(file) = server_files
                .iter()
                .find(|file| file.relative == Path::new(relative))
            else {
                found.diagnostics.push(diagnostic(
                    2001,
                    format!("Cannot inspect handler source '{relative}': source does not exist"),
                    None,
                    source_span(relative, 0, relative.len()),
                ));
                continue;
            };
            match HandlerSource::discover_snapshot(
                canonical_root.clone(),
                project,
                &file.relative,
                file.bytes.clone(),
                Arc::clone(&source_root),
            ) {
                Ok(source) => workers.push(ScheduledWorker {
                    relative: relative.clone(),
                    every_seconds,
                    source,
                }),
                Err(failure) => found.diagnostics.extend_from_slice(failure.diagnostics()),
            }
        }
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
                    "Add a client/pages/tac.html view or a server/routes/yon.* handler.",
                )),
                None,
            )));
        }

        let route_graph = build_route_graph(views, handlers, companions)?;
        Ok(Project {
            root: canonical_root,
            route_graph,
            snapshot: source_root,
            capability: Arc::new(project.try_clone().map_err(|error| {
                Failure::one(diagnostic(
                    1001,
                    format!("Cannot retain the project capability: {error}"),
                    None,
                    None,
                ))
            })?),
            middleware,
            workers,
        })
    }
}

#[derive(Clone, Debug)]
struct CapturedFile {
    relative: PathBuf,
    bytes: Vec<u8>,
}

impl CapturedFile {
    fn portable(&self) -> String {
        portable(&self.relative)
    }
}

fn capture_project_file(
    project: &Dir,
    relative: &Path,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) -> Option<CapturedFile> {
    let metadata = match project.symlink_metadata(relative) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            diagnostics.push(diagnostic(
                1001,
                format!("Cannot inspect '{}': {error}", portable(relative)),
                None,
                None,
            ));
            return None;
        }
    };
    if metadata.is_symlink() {
        diagnostics.push(unsafe_symlink(relative));
        return None;
    }
    if !metadata.is_file() {
        diagnostics.push(diagnostic(
            1001,
            format!(
                "Project input '{}' is not a regular file.",
                portable(relative)
            ),
            None,
            None,
        ));
        return None;
    }
    if metadata.len() > MAX_CAPTURE_FILE_BYTES {
        diagnostics.push(diagnostic(
            1001,
            format!(
                "Project input '{}' exceeds the 16 MiB snapshot limit.",
                portable(relative)
            ),
            None,
            None,
        ));
        return None;
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let bytes = project
        .open_with(relative, &options)
        .and_then(|mut source| {
            let mut bytes = Vec::new();
            source.read_to_end(&mut bytes).map(|_| bytes)
        });
    match bytes {
        Ok(bytes) => Some(CapturedFile {
            relative: relative.to_path_buf(),
            bytes,
        }),
        Err(error) => {
            diagnostics.push(diagnostic(
                1001,
                format!(
                    "Cannot read project input '{}': {error}",
                    portable(relative)
                ),
                None,
                None,
            ));
            None
        }
    }
}

fn capture_root_middleware(
    project: &Dir,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) -> Option<CapturedFile> {
    let entries = match project.entries() {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(diagnostic(
                1001,
                format!("Cannot enumerate project root: {error}"),
                None,
                None,
            ));
            return None;
        }
    };
    let mut entries = match entries.collect::<Result<Vec<_>, _>>() {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(diagnostic(
                1001,
                format!("Cannot enumerate project root: {error}"),
                None,
                None,
            ));
            return None;
        }
    };
    entries.sort_by_key(cap_std::fs::DirEntry::file_name);
    let selected = entries.into_iter().find_map(|entry| {
        let name = entry.file_name();
        name.to_str()
            .filter(|name| name.starts_with("middleware."))
            .map(PathBuf::from)
    });
    selected.and_then(|relative| capture_project_file(project, &relative, diagnostics))
}

fn validate_server_source_root(
    server: &Dir,
    relative_root: &Path,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) -> bool {
    let Some(name) = relative_root.file_name() else {
        return false;
    };
    match server.symlink_metadata(name) {
        Ok(metadata) if metadata.is_symlink() => false,
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => {
            diagnostics.push(diagnostic(
                1001,
                format!(
                    "Source root '{}' is not a directory.",
                    portable(relative_root)
                ),
                None,
                None,
            ));
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            diagnostics.push(diagnostic(
                1001,
                format!("Cannot inspect '{}': {error}", portable(relative_root)),
                None,
                None,
            ));
            false
        }
    }
}

fn files_beneath(files: &[CapturedFile], root: &Path) -> Vec<CapturedFile> {
    files
        .iter()
        .filter(|file| {
            file.relative
                .strip_prefix(root)
                .is_ok_and(|relative| !relative.as_os_str().is_empty())
        })
        .cloned()
        .collect()
}

fn capture_source_root(
    project: &Dir,
    relative_root: &Path,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) -> Vec<CapturedFile> {
    let Some(directory) = open_source_root(project, relative_root, diagnostics) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    visit_capability_directory(&directory, relative_root, diagnostics, &mut files);
    files
}

fn open_source_root(
    project: &Dir,
    relative_root: &Path,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) -> Option<Dir> {
    let mut opened: Option<Dir> = None;
    let mut current = PathBuf::new();
    let components = relative_root.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return None;
        };
        current.push(name);
        let parent = opened.as_ref().unwrap_or(project);
        let metadata = match parent.symlink_metadata(name) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                diagnostics.push(diagnostic(
                    1001,
                    format!("Cannot inspect '{}': {error}", portable(&current)),
                    None,
                    None,
                ));
                return None;
            }
        };
        if metadata.is_symlink() {
            diagnostics.push(unsafe_symlink(&current));
            return None;
        }
        if !metadata.is_dir() {
            let message = if index + 1 == components.len() {
                format!(
                    "Source root '{}' is not a directory.",
                    portable(relative_root)
                )
            } else {
                format!(
                    "Source root '{}' is blocked by '{}', which is not a directory.",
                    portable(relative_root),
                    portable(&current)
                )
            };
            diagnostics.push(diagnostic(1001, message, None, None));
            return None;
        }
        match parent.open_dir_nofollow(name) {
            Ok(directory) => opened = Some(directory),
            Err(error) => {
                diagnostics.push(diagnostic(
                    1001,
                    format!(
                        "Cannot read source directory '{}': {error}",
                        portable(&current)
                    ),
                    None,
                    None,
                ));
                return None;
            }
        }
    }
    opened
}

#[allow(clippy::too_many_lines)]
fn visit_capability_directory(
    directory: &Dir,
    relative_directory: &Path,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
    files: &mut Vec<CapturedFile>,
) {
    let entries = match directory.entries() {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(diagnostic(
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
            diagnostics.push(diagnostic(
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
    entries.sort_by_key(cap_std::fs::DirEntry::file_name);

    for entry in entries {
        let relative = relative_directory.join(entry.file_name());
        let metadata = match directory.symlink_metadata(entry.file_name()) {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(diagnostic(
                    1001,
                    format!("Cannot inspect '{}': {error}", portable(&relative)),
                    None,
                    None,
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            diagnostics.push(unsafe_symlink(&relative));
        } else if metadata.is_dir() {
            match directory.open_dir_nofollow(entry.file_name()) {
                Ok(child) => visit_capability_directory(&child, &relative, diagnostics, files),
                Err(error) => diagnostics.push(diagnostic(
                    1001,
                    format!(
                        "Cannot read source directory '{}': {error}",
                        portable(&relative)
                    ),
                    None,
                    None,
                )),
            }
        } else if metadata.is_file() {
            if metadata.len() > MAX_CAPTURE_FILE_BYTES {
                diagnostics.push(diagnostic(
                    1001,
                    format!(
                        "Source '{}' exceeds the 16 MiB snapshot limit.",
                        portable(&relative)
                    ),
                    None,
                    None,
                ));
                continue;
            }
            if files.len() >= MAX_CAPTURE_FILES {
                diagnostics.push(diagnostic(
                    1001,
                    format!(
                        "Source tree '{}' exceeds the 4,096-file snapshot limit.",
                        portable(relative_directory)
                    ),
                    None,
                    None,
                ));
                return;
            }
            let total = files.iter().fold(metadata.len(), |total, file| {
                total.saturating_add(file.bytes.len() as u64)
            });
            if total > MAX_CAPTURE_TOTAL_BYTES {
                diagnostics.push(diagnostic(
                    1001,
                    format!(
                        "Source tree '{}' exceeds the 64 MiB snapshot limit.",
                        portable(relative_directory)
                    ),
                    None,
                    None,
                ));
                return;
            }
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let bytes = directory
                .open_with(entry.file_name(), &options)
                .and_then(|mut source| {
                    let mut bytes = Vec::new();
                    source.read_to_end(&mut bytes).map(|_| bytes)
                });
            match bytes {
                Ok(bytes) => files.push(CapturedFile { relative, bytes }),
                Err(error) => diagnostics.push(diagnostic(
                    1001,
                    format!("Cannot read source '{}': {error}", portable(&relative)),
                    None,
                    None,
                )),
            }
        } else {
            diagnostics.push(diagnostic(
                1001,
                format!(
                    "Source '{}' is not a regular file or directory.",
                    portable(&relative)
                ),
                None,
                None,
            ));
        }
    }
}

fn check_captured_layers(
    layers: &[(PathBuf, Vec<CapturedFile>)],
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    for (_, files) in layers {
        for file in files
            .iter()
            .filter(|file| crate::stereotype::is_annotated_language(&file.relative))
        {
            match std::str::from_utf8(&file.bytes) {
                Ok(contents) => {
                    if let Err(failure) = crate::stereotype::check(&file.relative, contents) {
                        diagnostics.extend(failure.diagnostics().iter().cloned());
                    }
                }
                Err(error) => diagnostics.push(diagnostic(
                    1001,
                    format!("Cannot read source '{}': {error}", portable(&file.relative)),
                    None,
                    None,
                )),
            }
        }
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

fn inspect_source_file(
    project_root: &Path,
    project: &Dir,
    source_root: &Path,
    file: &CapturedFile,
    kind: SourceKind,
    owned_root: &std::sync::Arc<OwnedSourceRoot>,
    found: &mut Discovered,
) {
    let relative = &file.relative;
    let Some(name) = relative.file_name().and_then(OsStr::to_str) else {
        found.diagnostics.push(diagnostic(
            1005,
            "Source paths must be valid Unicode.",
            None,
            None,
        ));
        return;
    };
    if kind == SourceKind::Tac && name == ViewKind::Tac.file_name() {
        match route_for(relative, source_root) {
            Ok(route) => found.views.push((
                route,
                ViewSource {
                    source_path: portable(relative),
                    absolute_source_path: project_root.join(relative),
                    view_kind: ViewKind::Tac,
                    bytes: file.bytes.clone(),
                },
            )),
            Err(failure) => found.diagnostics.extend_from_slice(failure.diagnostics()),
        }
    } else if kind == SourceKind::Yon && name == "yon.html" {
        let path = portable(relative);
        found.diagnostics.push(diagnostic(
            1008,
            format!("Yon HTML view '{path}' is not supported."),
            Some(String::from(
                "Return the HTML body from a yon.* handler with Content-Type: text/html.",
            )),
            source_span(&path, 0, name.len()),
        ));
    } else if kind == SourceKind::Yon && name.starts_with("yon.") {
        // Any yon.<extension> is a handler candidate. HandlerSource accepts
        // only the framework-owned Yon language set and reports how an
        // unsupported implementation can be reached through @Relay.
        match (
            route_for(relative, source_root),
            HandlerSource::discover_snapshot(
                project_root.to_path_buf(),
                project,
                relative,
                file.bytes.clone(),
                std::sync::Arc::clone(owned_root),
            ),
        ) {
            (Ok(route), Ok(source)) => found.handlers.push((
                route,
                HandlerNode {
                    source_path: String::from(source.relative_path()),
                    absolute_source_path: source.absolute_path().to_path_buf(),
                    language: source.language(),
                    source,
                },
            )),
            (Err(failure), _) | (_, Err(failure)) => {
                found.diagnostics.extend_from_slice(failure.diagnostics());
            }
        }
    } else if kind == SourceKind::Tac
        && let Some(companion_kind) = companion_kind(name)
    {
        match route_for(relative, source_root) {
            Ok(route) => found.companions.push((
                route,
                CompanionSource {
                    source_path: portable(relative),
                    absolute_source_path: project_root.join(relative),
                    kind: companion_kind,
                    bytes: file.bytes.clone(),
                },
            )),
            Err(failure) => found.diagnostics.extend_from_slice(failure.diagnostics()),
        }
    } else if kind == SourceKind::Tac && name.starts_with(ViewKind::Tac.stem()) {
        let path = portable(relative);
        found.diagnostics.push(diagnostic(
            1008,
            format!("Companion source '{path}' has no available adapter."),
            Some(String::from(
                "Supported Tac companions are tac.css, tac.js, and tac.ts.",
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
fn companion_kind(name: &str) -> Option<CompanionKind> {
    match name {
        "tac.css" => Some(CompanionKind::Style),
        "tac.js" => Some(CompanionKind::ClientModule),
        "tac.ts" => Some(CompanionKind::TypeScriptModule),
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
                Some(String::from("Keep exactly one Tac view for each route.")),
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
    use crate::handler::HandlerSource;
    use std::fs;
    #[cfg(unix)]
    use std::path::Path;

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
    fn middleware_and_worker_schedules_share_one_ordered_owned_snapshot() {
        let root = tempfile::tempdir().expect("project");
        fs::create_dir_all(root.path().join("client/pages")).expect("pages");
        fs::write(
            root.path().join("client/pages/tac.html"),
            "<main>Page</main>",
        )
        .expect("page");
        fs::write(
            root.path().join("middleware.py"),
            "@Controller\nclass AccessController:\n    @staticmethod\n    def GET(request):\n        return {'status': 204, 'headers': {}, 'body': ''}\n",
        )
        .expect("middleware");
        fs::create_dir_all(root.path().join("server/workers")).expect("workers");
        for name in ["zeta.py", "alpha.py"] {
            fs::write(
                root.path().join("server/workers").join(name),
                "@Controller\nclass JobController:\n    @staticmethod\n    def POST(request):\n        return {'status': 204, 'headers': {}, 'body': ''}\n",
            )
            .expect("worker");
        }
        fs::write(
            root.path().join(".tachyonrc"),
            r#"{"workers":{"server/workers/zeta.py":{"every_seconds":20},"server/workers/alpha.py":{"every_seconds":10}}}"#,
        )
        .expect("schedules");

        let project = ProjectDiscovery::discover(root.path()).expect("discovery");
        assert_eq!(
            project
                .workers()
                .iter()
                .map(|worker| (worker.relative(), worker.every_seconds()))
                .collect::<Vec<_>>(),
            [
                ("server/workers/alpha.py", 10),
                ("server/workers/zeta.py", 20),
            ]
        );
        let execution_root = project.middleware().expect("middleware").execution_root();
        assert!(
            project
                .workers()
                .iter()
                .all(|worker| worker.source().execution_root() == execution_root)
        );
        assert_eq!(
            project
                .invocation_sources()
                .map(HandlerSource::relative_path)
                .collect::<Vec<_>>(),
            [
                "middleware.py",
                "server/workers/alpha.py",
                "server/workers/zeta.py",
            ]
        );
        assert_eq!(execution_root, project.snapshot_root());
    }

    #[test]
    fn a_scheduled_worker_must_exist_in_the_captured_server_snapshot() {
        let root = tempfile::tempdir().expect("project");
        fs::create_dir_all(root.path().join("client/pages")).expect("pages");
        fs::write(
            root.path().join("client/pages/tac.html"),
            "<main>Page</main>",
        )
        .expect("page");
        fs::write(
            root.path().join(".tachyonrc"),
            r#"{"workers":{"server/workers/missing.py":{"every_seconds":10}}}"#,
        )
        .expect("schedule");

        let failure = ProjectDiscovery::discover(root.path()).expect_err("missing worker");
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2001"), "{rendered}");
        assert!(rendered.contains("server/workers/missing.py"), "{rendered}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_root_middleware_is_rejected_without_following_it() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("project");
        fs::create_dir_all(root.path().join("client/pages")).expect("pages");
        fs::write(
            root.path().join("client/pages/tac.html"),
            "<main>Page</main>",
        )
        .expect("page");
        let outside = tempfile::NamedTempFile::new().expect("outside");
        fs::write(
            outside.path(),
            "@Controller\nclass PlantedController: pass\n",
        )
        .expect("outside source");
        symlink(outside.path(), root.path().join("middleware.py")).expect("middleware link");

        let failure = ProjectDiscovery::discover(root.path()).expect_err("symlink middleware");
        let rendered = failure.to_string();
        assert!(rendered.contains("TY1004"), "{rendered}");
        assert!(rendered.contains("middleware.py"), "{rendered}");
        assert!(
            !rendered.contains("PlantedController"),
            "link target was read"
        );
    }

    #[test]
    fn a_source_root_blocked_by_a_file_is_found_on_every_platform() {
        let root = tempfile::tempdir().expect("workspace");
        fs::write(root.path().join("client"), "not a directory").expect("blocking file");
        let failure = ProjectDiscovery::discover(root.path()).expect_err("blocked root");
        assert!(
            failure.to_string().contains(
                "Source root 'client/pages' is blocked by 'client', which is not a directory."
            ),
            "{failure}"
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

    #[cfg(unix)]
    #[test]
    fn layer_discovery_rejects_external_and_cyclic_links_without_following_them() {
        use std::os::unix::fs::symlink;

        for layer in ["services", "repositories", "clients", "delegates"] {
            let root = tempfile::tempdir().expect("workspace");
            fs::create_dir_all(root.path().join("client/pages")).expect("page root");
            fs::write(
                root.path().join("client/pages/tac.html"),
                "<main>Safe</main>",
            )
            .expect("page");
            let outside = tempfile::tempdir().expect("outside");
            fs::write(
                outside.path().join("source.py"),
                "@Service\nclass XService: pass",
            )
            .expect("outside source");
            let layer_root = root.path().join("server").join(layer);
            fs::create_dir_all(&layer_root).expect("layer root");
            symlink(outside.path(), layer_root.join("external")).expect("external link");
            symlink(&layer_root, layer_root.join("cycle")).expect("cyclic link");

            let error = ProjectDiscovery::discover(root.path()).expect_err("unsafe layer links");
            let rendered = error.to_string();
            assert!(rendered.contains("TY1004"), "{layer}: {rendered}");
            assert!(rendered.contains(&format!("server/{layer}/cycle")));
            assert!(rendered.contains(&format!("server/{layer}/external")));
            assert!(!rendered.contains("source.py"), "link target was traversed");
        }
    }

    #[cfg(unix)]
    #[test]
    fn opened_layer_directory_cannot_be_redirected_by_an_ambient_root_swap() {
        use cap_std::ambient_authority;
        use cap_std::fs::Dir;
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("workspace");
        let services = root.path().join("server/services");
        fs::create_dir_all(&services).expect("services");
        fs::write(
            services.join("owned.py"),
            "@Repository\nclass OwnedRepository: pass",
        )
        .expect("owned source");
        let project = Dir::open_ambient_dir(root.path(), ambient_authority()).expect("project");
        let original = root.path().to_path_buf();
        let opened = original.with_extension("opened");
        fs::rename(&original, &opened).expect("move ambient project");
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir_all(outside.path().join("services")).expect("outside services");
        fs::write(
            outside.path().join("services/decoy.py"),
            "@Service\nclass DecoyService: pass",
        )
        .expect("outside decoy");
        symlink(outside.path(), &original).expect("ambient replacement");

        let mut diagnostics = Vec::new();
        let files =
            super::capture_source_root(&project, Path::new("server/services"), &mut diagnostics);
        super::check_captured_layers(
            &[(std::path::PathBuf::from("server/services"), files)],
            &mut diagnostics,
        );
        let rendered = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("owned.py"), "{rendered}");
        assert!(
            !rendered.contains("decoy.py"),
            "ambient replacement was read"
        );
        drop(project);
        fs::remove_file(&original).expect("remove replacement");
        fs::rename(opened, original).expect("restore temporary root");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn opened_project_discovery_and_execution_ignore_planted_views_and_routes() {
        use crate::{HandlerCancellation, HandlerSupervisor, HandlerSupervisorOptions};
        use cap_std::ambient_authority;
        use cap_std::fs::Dir;
        use std::os::unix::fs::symlink;
        use tachyon_contracts::{HandlerRequest, HttpMethod};

        let workspace = tempfile::tempdir().expect("workspace");
        let root = workspace.path().join("project");
        fs::create_dir_all(root.join("client/pages/owned")).expect("owned page root");
        fs::create_dir_all(root.join("server/routes/owned")).expect("owned route root");
        fs::write(
            root.join("client/pages/owned/tac.html"),
            "<main>owned-view</main>",
        )
        .expect("owned view");
        fs::write(
            root.join("server/routes/owned/yon.py"),
            "@Controller\nclass OwnedController:\n    @staticmethod\n    def GET(request):\n        return {'origin': 'owned-py'}\n",
        )
        .expect("owned Python handler");
        fs::write(
            root.join("server/routes/owned/yon.js"),
            "@Controller\nexport class OwnedController {\n  static GET() { return { origin: 'owned-js' } }\n}\n",
        )
        .expect("owned JavaScript handler");
        let canonical = fs::canonicalize(&root).expect("canonical root");
        let opened =
            Dir::open_ambient_dir(&canonical, ambient_authority()).expect("project handle");

        let retained = workspace.path().join("retained-project");
        fs::rename(&root, &retained).expect("move authored root after opening");
        let planted = tempfile::tempdir().expect("planted root");
        fs::create_dir_all(planted.path().join("client/pages/canary")).expect("canary page root");
        fs::create_dir_all(planted.path().join("server/routes/canary")).expect("canary route root");
        fs::write(
            planted.path().join("client/pages/canary/tac.html"),
            "<main>external-canary-view</main>",
        )
        .expect("canary view");
        fs::write(
            planted.path().join("server/routes/canary/yon.py"),
            "@Controller\nclass CanaryController:\n    pass\n",
        )
        .expect("canary handler");
        fs::create_dir_all(planted.path().join("server/routes/owned"))
            .expect("planted owned route");
        fs::write(
            planted.path().join("server/routes/owned/yon.py"),
            "@Controller\nclass OwnedController:\n    @staticmethod\n    def GET(request):\n        return {'origin': 'planted-py'}\n",
        )
        .expect("planted Python handler");
        fs::write(
            planted.path().join("server/routes/owned/yon.js"),
            "@Controller\nexport class OwnedController {\n  static GET() { return { origin: 'planted-js' } }\n}\n",
        )
        .expect("planted JavaScript handler");
        symlink(planted.path(), &root).expect("replace ambient project root");

        let project = ProjectDiscovery::discover_opened(canonical, &opened).expect("snapshot");
        let routes = project.route_graph().routes();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].route(), "/owned");
        assert_eq!(
            routes[0].view_bytes(),
            Some(b"<main>owned-view</main>".as_slice())
        );
        assert_eq!(routes[0].handlers().len(), 2);
        let supervisor =
            HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
        for handler in routes[0].handlers() {
            let request = HandlerRequest::route(
                format!("project_snapshot_{}", handler.language().name()),
                "/owned",
                HttpMethod::Get,
            );
            let response = supervisor
                .invoke(handler.source(), &request, &HandlerCancellation::default())
                .await
                .expect("owned snapshot response");
            let body = response.body.expect("response body").data;
            assert!(body.contains("owned-"), "{body}");
            assert!(!body.contains("planted"), "{body}");
        }
        let rendered = format!("{project:?}");
        assert!(!rendered.contains("canary"), "planted project was observed");
    }

    #[test]
    fn layer_discovery_reports_unreadable_source_and_sorts_diagnostics() {
        let root = tempfile::tempdir().expect("workspace");
        fs::create_dir_all(root.path().join("client/pages")).expect("page root");
        fs::write(
            root.path().join("client/pages/tac.html"),
            "<main>Safe</main>",
        )
        .expect("page");
        let services = root.path().join("server/services");
        fs::create_dir_all(&services).expect("services");
        fs::write(
            services.join("z.py"),
            "@Repository\nclass ZRepository: pass",
        )
        .expect("z source");
        fs::write(
            services.join("a.py"),
            "@Repository\nclass ARepository: pass",
        )
        .expect("a source");
        for layer in ["services", "repositories", "clients", "delegates"] {
            let directory = root.path().join("server").join(layer);
            fs::create_dir_all(&directory).expect("layer directory");
            fs::write(directory.join("invalid.py"), [0xff, 0xfe]).expect("invalid UTF-8 source");
        }

        let error = ProjectDiscovery::discover(root.path()).expect_err("invalid layer sources");
        let rendered = error.to_string();
        assert!(rendered.contains("TY1001"), "{rendered}");
        for layer in ["services", "repositories", "clients", "delegates"] {
            assert!(
                rendered.contains(&format!("Cannot read source 'server/{layer}/invalid.py'")),
                "{layer}: {rendered}"
            );
        }
        let a = rendered.find("server/services/a.py").expect("a diagnostic");
        let z = rendered.find("server/services/z.py").expect("z diagnostic");
        assert!(a < z, "layer diagnostics are not path sorted: {rendered}");
    }

    #[test]
    fn yon_html_and_unavailable_handlers_are_rejected() {
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
                .expect_err("yon.html")
                .to_string()
                .contains("TY1008")
        );

        // A yon.<extension> outside the eight owned languages is a migration
        // error. Projects keep that program behind an explicit @Relay edge.
        let companion = tempfile::tempdir().expect("companion project");
        fs::create_dir_all(companion.path().join("server/routes")).expect("Yon root");
        fs::write(
            companion.path().join("server/routes/yon.rb"),
            "class LegacyHandler; end",
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
            "@Controller\nclass ApiController: pass",
        )
        .expect("Python");
        fs::write(
            root.path().join("client/pages/products/tac.html"),
            "<main>Products</main>",
        )
        .expect("view");
        fs::write(
            root.path().join("server/routes/products/yon.py"),
            "@Controller\nclass ProductsController: pass",
        )
        .expect("Python");
        fs::write(
            root.path().join("server/routes/products/yon.js"),
            "@Controller\nexport class ProductsController {}",
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
