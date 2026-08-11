use crate::failure::diagnostic;
use crate::handler::{HandlerCancellation, HandlerSource, HandlerSupervisor};
use crate::hot_update::{SourceAction, SourceChanges, SourceWatcher};
use crate::routing::match_route;
use crate::{BuildOptions, BuildResult, Failure, ProjectDiscovery, WebCompiler};
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::header::{
    CACHE_CONTROL, CONNECTION, CONTENT_TYPE, HeaderName, HeaderValue, REFERRER_POLICY,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{Request, Response, StatusCode};
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tachyon_contracts::{
    HandlerBody, HandlerBodyEncoding, HandlerRequest, HotUpdate, HotUpdateKind, HttpMethod,
};
use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::io::ReaderStream;
use tower::ServiceExt as _;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

/// Largest request body accepted by a dispatched handler.
const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
/// Largest generated document the server will rewrite while serving.
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
/// Quiet period used to combine one editor save into one rebuild.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(75);
/// Prefix serving a topic subscription as server-sent events.
const TOPIC_ENDPOINT: &str = "/.tachyon/topics/";
/// Project-relative directory holding append-only topic logs.
const TOPIC_DIRECTORY: &str = ".tachyon/topics";
/// Largest topic log the server will read in one pass.
const MAX_TOPIC_BYTES: u64 = 16 * 1024 * 1024;
/// How often a subscription checks its topic for new records.
const TOPIC_POLL: Duration = Duration::from_millis(250);

/// Compatibility endpoint reporting the latest hot-update sequence.
const LIVE_ENDPOINT: &str = "/.tachyon/live";
/// Server-sent event stream carrying Hot Update Protocol v1 messages.
const HOT_ENDPOINT: &str = "/.tachyon/hot";
/// Endpoint serving the reload client.
const LIVE_SCRIPT_ENDPOINT: &str = "/.tachyon/live-reload.js";
/// Reference injected into served documents so an open page receives semantic
/// hot updates. It is added while serving, never by `ty build`.
///
/// The script is a same-origin file rather than an inline block because the
/// development server sends `default-src 'self'`, which forbids inline script.
const LIVE_RELOAD: &str = r#"<script type="module" src="/.tachyon/live-reload.js"></script>"#;
/// The reload client itself.
const LIVE_RELOAD_CLIENT: &str = r#"const endpoint = '/.tachyon/hot'
let applying = Promise.resolve()

const overlay = () => {
  let root = document.getElementById('tachyon-hot-diagnostics')
  if (root) return root
  root = document.createElement('aside')
  root.id = 'tachyon-hot-diagnostics'
  root.setAttribute('role', 'alert')
  root.setAttribute('aria-live', 'assertive')
  Object.assign(root.style, {
    position: 'fixed', inset: '16px', zIndex: '2147483647', overflow: 'auto',
    padding: '18px', border: '1px solid #ef4444', borderRadius: '8px',
    color: '#fee2e2', background: '#1f1113', font: '13px/1.5 ui-monospace, monospace',
    whiteSpace: 'pre-wrap', boxShadow: '0 16px 48px #0008'
  })
  document.body.append(root)
  return root
}

const showDiagnostics = (report) => {
  const diagnostics = report?.diagnostics || []
  overlay().textContent = diagnostics.map((item) => {
    const spans = (item.spans || []).map((span) => `\n  ${span.file}:${span.start}..${span.end}`).join('')
    const help = item.help ? `\n  help: ${item.help}` : ''
    return `${item.code}: ${item.message}${spans}${help}`
  }).join('\n\n') || 'Tachyon could not rebuild this page.'
  document.documentElement.dataset.tachyonHot = 'diagnostics'
}

const clearDiagnostics = () => document.getElementById('tachyon-hot-diagnostics')?.remove()

const replaceStyles = async (buildId) => {
  const links = [...document.querySelectorAll('link[rel~="stylesheet"][href]')]
    .filter((link) => new URL(link.href, location.href).origin === location.origin)
  await Promise.all(links.map((link) => new Promise((resolve, reject) => {
    const next = link.cloneNode()
    const url = new URL(link.href, location.href)
    url.searchParams.set('tachyon_hot', buildId || String(Date.now()))
    next.href = url.href
    next.addEventListener('load', () => { link.remove(); resolve() }, { once: true })
    next.addEventListener('error', () => { next.remove(); reject(new Error(`Cannot update ${url.pathname}`)) }, { once: true })
    link.after(next)
  })))
}

const updateIslands = async (message) => {
  // Tac owns its complete DOM in the browser (ADR 0015). A companion edit
  // replaces the affected client instances and asks that renderer to rebuild;
  // it never fetches server-rendered component boundaries.
  const tac = globalThis.__tachyonTac
  if (!tac?.hotUpdate) throw new Error('The Tac hot-update runtime is unavailable.')
  await tac.hotUpdate(message.boundaries || [], message.build_id || String(message.sequence))
}

const apply = async (message) => {
  if (message.contract_version !== 1) { location.reload(); return }
  if (message.kind === 'diagnostics') { showDiagnostics(message.diagnostics); return }
  clearDiagnostics()
  if (message.kind === 'css') await replaceStyles(message.build_id)
  else if (message.kind === 'island') await updateIslands(message)
  else { location.reload(); return }
  document.documentElement.dataset.tachyonHot = message.kind
  window.dispatchEvent(new CustomEvent('tachyon:hot-update', { detail: message }))
}

const source = new EventSource(endpoint)
source.addEventListener('hot', (event) => {
  let message
  try { message = JSON.parse(event.data) }
  catch { location.reload(); return }
  applying = applying.then(() => apply(message)).catch((error) => {
    console.error('[Tachyon hot update]', error)
    location.reload()
  })
})
"#;

#[derive(Clone, Debug)]
struct HotUpdateHub {
    sender: broadcast::Sender<String>,
    sequence: Arc<AtomicU64>,
    build_id: Arc<RwLock<Option<String>>>,
}

impl HotUpdateHub {
    fn new(build_id: Option<String>) -> Self {
        let (sender, _) = broadcast::channel(64);
        Self {
            sender,
            sequence: Arc::new(AtomicU64::new(0)),
            build_id: Arc::new(RwLock::new(build_id)),
        }
    }

    fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }

    fn build_id(&self) -> Option<String> {
        self.build_id.read().ok().and_then(|value| value.clone())
    }

    fn subscribe(&self) -> broadcast::Receiver<String> {
        self.sender.subscribe()
    }

    fn publish(
        &self,
        kind: HotUpdateKind,
        build_id: Option<String>,
        paths: Vec<String>,
        boundaries: Vec<String>,
        diagnostics: Option<tachyon_diagnostics::DiagnosticReport>,
    ) {
        if let Some(value) = &build_id
            && let Ok(mut published) = self.build_id.write()
        {
            *published = Some(value.clone());
        }
        let sequence = self.sequence.fetch_add(1, Ordering::AcqRel) + 1;
        let update = HotUpdate::v1(
            sequence,
            kind,
            build_id.or_else(|| self.build_id()),
            paths,
            boundaries,
            diagnostics,
        );
        if let Ok(payload) = serde_json::to_string(&update) {
            let _unused = self.sender.send(sse_frame(&update, &payload));
        }
    }

    fn reload_snapshot(&self) -> Option<String> {
        let sequence = self.sequence();
        if sequence == 0 {
            return None;
        }
        let update = HotUpdate::v1(
            sequence,
            HotUpdateKind::Reload,
            self.build_id(),
            Vec::new(),
            Vec::new(),
            None,
        );
        serde_json::to_string(&update)
            .ok()
            .map(|payload| sse_frame(&update, &payload))
    }
}

fn sse_frame(update: &HotUpdate, payload: &str) -> String {
    format!("id: {}\nevent: hot\ndata: {payload}\n\n", update.sequence)
}

/// One route the server can dispatch to a supervised handler.
#[derive(Clone, Debug)]
struct DispatchRoute {
    route: String,
    handler: HandlerSource,
}

/// Shared state backing request dispatch.
#[derive(Clone)]
struct Dispatch {
    routes: Arc<Vec<DispatchRoute>>,
    /// Page patterns used to resolve concrete dynamic URLs to `_segment`
    /// template files.
    page_routes: Arc<Vec<String>>,
    supervisor: Arc<HandlerSupervisor>,
    files: ServeDir,
    /// Publishes bounded semantic updates to connected development pages.
    hot_updates: HotUpdateHub,
    /// Whether served documents receive the hot-update client.
    watch: bool,
    /// Root middleware consulted before every request, when present.
    middleware: Option<Arc<HandlerSource>>,
    /// Project root, used to resolve topic logs.
    project_root: Arc<PathBuf>,
}

#[derive(Clone, Debug)]
struct PreviewDispatch {
    files: ServeDir,
    page_routes: Arc<Vec<String>>,
}

const CONTENT_SECURITY_POLICY: HeaderName = HeaderName::from_static("content-security-policy");
const LAST_EVENT_ID: HeaderName = HeaderName::from_static("last-event-id");
const X_FRAME_OPTIONS: HeaderName = HeaderName::from_static("x-frame-options");

/// Development-server network and build options.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevServerOptions {
    /// Interface address to bind.
    pub host: IpAddr,
    /// TCP port, where zero requests an ephemeral port.
    pub port: u16,
    /// Explicit permission to expose the development server beyond loopback.
    pub allow_non_loopback: bool,
    /// Project-relative build output directory.
    pub output_directory: PathBuf,
    /// Whether sources are watched and rebuilt while the server runs.
    pub watch: bool,
    /// Whether the server compiles the project before binding.
    pub build: bool,
}

impl Default for DevServerOptions {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8000,
            allow_non_loopback: false,
            output_directory: PathBuf::from("dist"),
            watch: true,
            build: true,
        }
    }
}

/// A built and bound Phase 1 development server.
#[derive(Debug)]
pub struct DevServer {
    listener: TcpListener,
    application: Router,
    address: SocketAddr,
    build: Option<BuildResult>,
}

/// Network options for serving an already-built bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewServerOptions {
    /// Interface address to bind.
    pub host: IpAddr,
    /// TCP port, where zero requests an ephemeral port.
    pub port: u16,
    /// Explicit permission to expose the preview beyond loopback.
    pub allow_non_loopback: bool,
}

impl Default for PreviewServerOptions {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 3000,
            allow_non_loopback: false,
        }
    }
}

/// A bound static server for an existing bundle.
#[derive(Debug)]
pub struct PreviewServer {
    listener: TcpListener,
    application: Router,
    address: SocketAddr,
    root: PathBuf,
}

impl PreviewServer {
    /// Binds a server without compiling or changing the supplied bundle.
    ///
    /// # Errors
    ///
    /// Returns diagnostics for a missing bundle, unsafe exposure, or bind
    /// failure.
    pub async fn bind(
        root: impl AsRef<Path>,
        options: &PreviewServerOptions,
    ) -> Result<Self, Failure> {
        if !options.host.is_loopback() && !options.allow_non_loopback {
            return Err(Failure::one(diagnostic(
                1301,
                format!(
                    "Refusing to expose the preview server on {} without explicit permission.",
                    options.host
                ),
                Some(String::from(
                    "Use a loopback address or pass --allow-non-loopback.",
                )),
                None,
            )));
        }

        let root = root.as_ref().to_path_buf();
        if !std::fs::metadata(&root).is_ok_and(|metadata| metadata.is_dir()) {
            return Err(Failure::one(diagnostic(
                1304,
                format!(
                    "Cannot preview missing bundle directory {}.",
                    root.display()
                ),
                Some(String::from("Run `ty bundle` before `ty preview`.")),
                None,
            )));
        }

        let listener = TcpListener::bind(SocketAddr::new(options.host, options.port))
            .await
            .map_err(|error| {
                Failure::one(diagnostic(
                    1302,
                    format!(
                        "Cannot bind the preview server to {}:{}: {error}",
                        options.host, options.port
                    ),
                    Some(String::from("Choose an available interface and port.")),
                    None,
                ))
            })?;
        let address = listener.local_addr().map_err(|error| {
            Failure::one(diagnostic(
                1302,
                format!("Cannot read the preview server address: {error}"),
                None,
                None,
            ))
        })?;
        let state = PreviewDispatch {
            files: ServeDir::new(&root).append_index_html_on_directories(true),
            page_routes: Arc::new(preview_route_patterns(&root)),
        };
        let application = defensive_headers(
            Router::new().fallback(axum::routing::any(serve_preview).with_state(state)),
        );
        Ok(Self {
            listener,
            application,
            address,
            root,
        })
    }

    /// Returns the bound socket address.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns the bundle directory being served.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Serves until the supplied shutdown signal resolves.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the HTTP runtime terminates abnormally.
    pub async fn run_until<F>(self, shutdown: F) -> Result<(), Failure>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        axum::serve(self.listener, self.application)
            .with_graceful_shutdown(shutdown)
            .await
            .map_err(|error| {
                Failure::one(diagnostic(
                    1303,
                    format!("Preview server stopped unexpectedly: {error}"),
                    Some(String::from("Restart the preview server.")),
                    None,
                ))
            })
    }
}

impl DevServer {
    /// Builds the project and binds its development server.
    ///
    /// # Errors
    ///
    /// Returns diagnostics for unsafe exposure, build failure, or bind failure.
    pub async fn bind(
        project_root: impl AsRef<Path>,
        options: &DevServerOptions,
    ) -> Result<Self, Failure> {
        if !options.host.is_loopback() && !options.allow_non_loopback {
            return Err(Failure::one(diagnostic(
                1301,
                format!(
                    "Refusing to expose the development server on {} without explicit permission.",
                    options.host
                ),
                Some(String::from(
                    "Use a loopback address or pass --allow-non-loopback.",
                )),
                None,
            )));
        }
        let project_root = project_root.as_ref().to_path_buf();
        let (build, output_directory) = prepare_dev_output(&project_root, options).await?;
        let listener = match TcpListener::bind(SocketAddr::new(options.host, options.port)).await {
            Ok(listener) => listener,
            Err(error) => {
                return Err(Failure::one(diagnostic(
                    1302,
                    format!(
                        "Cannot bind the development server to {}:{}: {error}",
                        options.host, options.port
                    ),
                    Some(String::from("Choose an available interface and port.")),
                    None,
                )));
            }
        };
        let address = match listener.local_addr() {
            Ok(address) => address,
            Err(error) => {
                return Err(Failure::one(diagnostic(
                    1302,
                    format!("Cannot read the development server address: {error}"),
                    None,
                    None,
                )));
            }
        };
        // Discover the routes a handler owns so requests can reach them. A
        // project with no handlers dispatches nothing and serves only
        // generated output, exactly as before.
        let project = ProjectDiscovery::discover(&project_root)?;
        let (routes, page_routes) = discover_dispatch_routes(&project)?;
        let supervisor = Arc::new(HandlerSupervisor::from_environment()?);
        let middleware = discover_middleware(project.root())?;
        let workers = crate::Workers::discover(project.root())?;

        spawn_workers(project.root(), &workers, &supervisor)?;

        let hot_updates =
            HotUpdateHub::new(build.as_ref().map(|result| String::from(result.sha256())));
        if options.watch {
            let watched_root = project.root().to_path_buf();
            let watcher = SourceWatcher::start(&watched_root)?;
            tokio::spawn(watch_sources(
                watched_root,
                BuildOptions {
                    output_directory: options.output_directory.clone(),
                    ..BuildOptions::default()
                },
                hot_updates.clone(),
                watcher,
            ));
        }
        let dispatch_state = Dispatch {
            routes: Arc::new(routes),
            page_routes: Arc::new(page_routes),
            supervisor,
            files: ServeDir::new(&output_directory).append_index_html_on_directories(true),
            hot_updates,
            watch: options.watch,
            middleware,
            project_root: Arc::new(project.root().to_path_buf()),
        };
        let application = application(dispatch_state);
        Ok(Self {
            listener,
            application,
            address,
            build,
        })
    }

    /// Returns the bound socket address.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns evidence for the build served by this process.
    #[must_use]
    pub const fn build(&self) -> Option<&BuildResult> {
        self.build.as_ref()
    }

    /// Serves until the supplied shutdown signal resolves.
    ///
    /// # Errors
    ///
    /// Returns a server diagnostic if the HTTP runtime terminates abnormally.
    pub async fn run_until<F>(self, shutdown: F) -> Result<(), Failure>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        match axum::serve(self.listener, self.application)
            .with_graceful_shutdown(shutdown)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => Err(Failure::one(diagnostic(
                1303,
                format!("Development server stopped unexpectedly: {error}"),
                Some(String::from(
                    "Restart the server and inspect the reported I/O error.",
                )),
                None,
            ))),
        }
    }
}

async fn prepare_dev_output(
    project_root: &Path,
    options: &DevServerOptions,
) -> Result<(Option<BuildResult>, PathBuf), Failure> {
    let build = if options.build {
        Some(
            WebCompiler::build_async(
                project_root,
                &BuildOptions {
                    output_directory: options.output_directory.clone(),
                    ..BuildOptions::default()
                },
            )
            .await?,
        )
    } else {
        None
    };
    let output_directory = build.as_ref().map_or_else(
        || project_root.join(&options.output_directory),
        |result| result.output_directory().to_path_buf(),
    );
    if std::fs::metadata(&output_directory).is_ok_and(|metadata| metadata.is_dir()) {
        return Ok((build, output_directory));
    }
    Err(Failure::one(diagnostic(
        1304,
        format!(
            "Cannot serve missing bundle directory {}.",
            output_directory.display()
        ),
        Some(String::from("Run `ty bundle` or remove --no-bundle.")),
        None,
    )))
}

fn discover_dispatch_routes(
    project: &crate::Project,
) -> Result<(Vec<DispatchRoute>, Vec<String>), Failure> {
    let page_routes = project
        .route_graph()
        .routes()
        .iter()
        .filter(|route| route.source_path().is_some())
        .map(|route| String::from(route.route()))
        .collect();
    let mut routes = Vec::new();
    for route in project.route_graph().routes() {
        for handler in route.handlers() {
            routes.push(DispatchRoute {
                route: String::from(route.route()),
                handler: HandlerSource::discover(project.root(), Path::new(handler.source_path()))?,
            });
        }
    }
    Ok((routes, page_routes))
}

/// Maps an HTTP method onto the protocol's method enumeration.
const fn protocol_method(method: &axum::http::Method) -> Option<HttpMethod> {
    Some(match *method {
        axum::http::Method::DELETE => HttpMethod::Delete,
        axum::http::Method::GET => HttpMethod::Get,
        axum::http::Method::HEAD => HttpMethod::Head,
        axum::http::Method::OPTIONS => HttpMethod::Options,
        axum::http::Method::PATCH => HttpMethod::Patch,
        axum::http::Method::POST => HttpMethod::Post,
        axum::http::Method::PUT => HttpMethod::Put,
        _ => return None,
    })
}

/// Serves one request, preferring a supervised handler over static output.
async fn dispatch(State(state): State<Dispatch>, request: Request<Body>) -> Response<Body> {
    let path = request.uri().path().to_owned();
    let method = protocol_method(request.method()).unwrap_or(HttpMethod::Get);
    // The after phase needs the request it is reasoning about, not just the
    // response, so its headers are captured before the body is consumed.
    let request_headers = request.headers().clone();
    let response = serve(&state, request).await;
    let Some(middleware) = state.middleware.clone() else {
        return response;
    };
    run_middleware_after(
        &state,
        &middleware,
        &path,
        method,
        &request_headers,
        response,
    )
    .await
}

/// Serves one request, before the after phase runs.
async fn serve(state: &Dispatch, request: Request<Body>) -> Response<Body> {
    let path = request.uri().path().to_owned();
    if path == LIVE_ENDPOINT && state.watch {
        return text_response(StatusCode::OK, &state.hot_updates.sequence().to_string());
    }
    if path == HOT_ENDPOINT && state.watch {
        return hot_update_stream(state, &request);
    }
    if let Some(middleware) = state.middleware.clone() {
        let method = protocol_method(request.method()).unwrap_or(HttpMethod::Get);
        let headers = request.headers().clone();
        match run_middleware(state, &middleware, &path, method, &headers).await {
            MiddlewareOutcome::Continue => {}
            MiddlewareOutcome::Respond(response) => return response,
        }
    }
    if let Some(topic) = path.strip_prefix(TOPIC_ENDPOINT) {
        return subscribe_topic(state, topic, &request);
    }
    if path == LIVE_SCRIPT_ENDPOINT && state.watch {
        return Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/javascript; charset=utf-8")
            .body(Body::from(LIVE_RELOAD_CLIENT))
            .unwrap_or_else(|_| {
                text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Cannot serve the client.",
                )
            });
    }
    if path == LIVE_ENDPOINT || path == HOT_ENDPOINT || path == LIVE_SCRIPT_ENDPOINT {
        return text_response(StatusCode::NOT_FOUND, "Not found.");
    }
    let patterns = state.routes.iter().map(|route| route.route.as_str());
    let matched = match_route(patterns, &path);

    let Some(matched) = matched else {
        return serve_static(state.clone(), request).await;
    };
    let Some(entry) = state
        .routes
        .iter()
        .find(|route| route.route == matched.route)
    else {
        return serve_static(state.clone(), request).await;
    };
    let Some(method) = protocol_method(request.method()) else {
        return text_response(StatusCode::METHOD_NOT_ALLOWED, "Unsupported method.");
    };

    let (parts, body) = request.into_parts();

    let Ok(collected) = axum::body::to_bytes(body, MAX_REQUEST_BODY_BYTES).await else {
        return text_response(StatusCode::PAYLOAD_TOO_LARGE, "Request body is too large.");
    };

    // A TTID is time-sortable and unique across restarts, so request ids stay
    // correlatable in logs where a per-process counter would repeat itself.
    let mut protocol_request =
        HandlerRequest::route(crate::ttid::generate(), matched.route, method);
    protocol_request.parameters = matched.parameters;
    for (name, value) in &parts.headers {
        if let Ok(value) = value.to_str() {
            protocol_request
                .headers
                .entry(name.as_str().to_owned())
                .or_default()
                .push(value.to_owned());
        }
    }
    if !collected.is_empty() {
        match std::str::from_utf8(&collected) {
            Ok(text) => {
                protocol_request.body = Some(HandlerBody {
                    encoding: HandlerBodyEncoding::Utf8,
                    data: text.to_owned(),
                });
            }
            Err(_) => {
                return text_response(StatusCode::BAD_REQUEST, "Request body must be UTF-8.");
            }
        }
    }

    let cancellation = HandlerCancellation::default();
    match state
        .supervisor
        .invoke(&entry.handler, &protocol_request, &cancellation)
        .await
    {
        Ok(response) => handler_response(response),
        // A handler failure is an application fault, never a crash of the
        // server. Internal process diagnostics may contain authored stdout or
        // stderr, so only a correlation identifier crosses the HTTP boundary.
        Err(failure) => {
            eprintln!(
                "{}",
                invocation_failure_event("handler", &protocol_request.request_id, &failure)
            );
            invocation_failure_response("Handler execution", &protocol_request.request_id)
        }
    }
}

fn hot_update_stream(state: &Dispatch, request: &Request<Body>) -> Response<Body> {
    let mut receiver = state.hot_updates.subscribe();
    let hub = state.hot_updates.clone();
    let reconnect_sequence = request
        .headers()
        .get(LAST_EVENT_ID)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let (mut writer, reader) = tokio::io::duplex(16 * 1_024);
    tokio::spawn(async move {
        if writer.write_all(b": connected\n\n").await.is_err() {
            return;
        }
        if reconnect_sequence.is_some_and(|sequence| sequence < hub.sequence())
            && let Some(frame) = hub.reload_snapshot()
            && writer.write_all(frame.as_bytes()).await.is_err()
        {
            return;
        }
        loop {
            match receiver.recv().await {
                Ok(frame) => {
                    if writer.write_all(frame.as_bytes()).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let Some(frame) = hub.reload_snapshot() else {
                        continue;
                    };
                    if writer.write_all(frame.as_bytes()).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/event-stream; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .header(CONNECTION, "keep-alive")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(ReaderStream::new(reader)))
        .unwrap_or_else(|_| {
            text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot open the hot-update stream.",
            )
        })
}

/// Maps one protocol response onto an HTTP response.
fn handler_response(response: tachyon_contracts::HandlerResponse) -> Response<Body> {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = Response::builder().status(status);
    let mut has_content_type = false;
    for (name, values) in &response.headers {
        for value in values {
            if name.eq_ignore_ascii_case("content-type") {
                has_content_type = true;
            }
            builder = builder.header(name, value);
        }
    }
    if !has_content_type {
        builder = builder.header(CONTENT_TYPE, "application/json; charset=utf-8");
    }
    let body = response.body.map(|body| body.data).unwrap_or_default();
    builder.body(Body::from(body)).unwrap_or_else(|_| {
        text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid handler response.",
        )
    })
}

/// Finds the optional root middleware source.
///
/// Middleware is resolved exactly as a handler is, so it may be written in any
/// language the project can run.
fn discover_middleware(project_root: &Path) -> Result<Option<Arc<HandlerSource>>, Failure> {
    let mut entries = match std::fs::read_dir(project_root) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(String::from))
            .filter(|name| name.starts_with("middleware."))
            .collect::<Vec<_>>(),
        Err(_) => return Ok(None),
    };
    entries.sort();
    let Some(name) = entries.first() else {
        return Ok(None);
    };
    Ok(Some(Arc::new(HandlerSource::discover(
        project_root,
        Path::new(name),
    )?)))
}

/// What middleware decided about one request.
enum MiddlewareOutcome {
    /// Proceed to the route or generated output.
    Continue,
    /// Answer the request now, without reaching a handler.
    Respond(Response<Body>),
}

/// Consults root middleware before a request is served.
///
/// Middleware speaks the same protocol a handler does. It answers either with
/// a response, which short-circuits the request, or with a continue marker.
/// A middleware failure is reported rather than silently ignored, because
/// middleware is where authorisation lives.
async fn run_middleware(
    state: &Dispatch,
    middleware: &HandlerSource,
    path: &str,
    method: HttpMethod,
    headers: &axum::http::HeaderMap,
) -> MiddlewareOutcome {
    let mut protocol_request =
        HandlerRequest::route(crate::ttid::generate(), String::from(path), method);
    protocol_request.operation = String::from("middleware.before");
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            protocol_request
                .headers
                .entry(name.as_str().to_owned())
                .or_default()
                .push(value.to_owned());
        }
    }

    let cancellation = HandlerCancellation::default();
    match state
        .supervisor
        .invoke(middleware, &protocol_request, &cancellation)
        .await
    {
        // A continue decision is expressed as 204 with no body, so middleware
        // needs no vocabulary beyond the response it already writes.
        Ok(response) if response.status == 204 => MiddlewareOutcome::Continue,
        Ok(response) => {
            let status =
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut builder = Response::builder().status(status);
            for (name, values) in &response.headers {
                for value in values {
                    builder = builder.header(name, value);
                }
            }
            let body = response.body.map(|body| body.data).unwrap_or_default();
            MiddlewareOutcome::Respond(builder.body(Body::from(body)).unwrap_or_else(|_| {
                text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Invalid middleware response.",
                )
            }))
        }
        Err(failure) => {
            eprintln!(
                "{}",
                invocation_failure_event("middleware", &protocol_request.request_id, &failure)
            );
            MiddlewareOutcome::Respond(invocation_failure_response(
                "Middleware execution",
                &protocol_request.request_id,
            ))
        }
    }
}

/// Starts one task per scheduled worker.
///
/// A worker is a handler on a schedule, so it reuses the same supervisor,
/// deadlines, and bounds as a request-driven one.
fn spawn_workers(
    project_root: &Path,
    workers: &crate::Workers,
    supervisor: &Arc<HandlerSupervisor>,
) -> Result<(), Failure> {
    if workers.is_empty() {
        return Ok(());
    }
    for (relative, seconds) in workers.iter() {
        let source = HandlerSource::discover(project_root, Path::new(relative))?;
        tokio::spawn(run_worker(
            Arc::clone(supervisor),
            source,
            String::from(relative),
            Duration::from_secs(seconds),
        ));
    }
    Ok(())
}

/// Runs one scheduled worker until the process ends.
///
/// A failed run is reported and the schedule continues, because one bad run
/// must not silently stop a recurring job.
async fn run_worker(
    supervisor: Arc<HandlerSupervisor>,
    source: HandlerSource,
    relative: String,
    interval: Duration,
) {
    loop {
        tokio::time::sleep(interval).await;
        let mut request = HandlerRequest::route(
            crate::ttid::generate(),
            format!("/{relative}"),
            HttpMethod::Post,
        );
        request.operation = String::from("worker.run");
        let cancellation = HandlerCancellation::default();
        match supervisor.invoke(&source, &request, &cancellation).await {
            Ok(response) => {
                if !(200..400).contains(&response.status) {
                    eprintln!("Worker '{relative}' reported status {}", response.status);
                }
            }
            Err(failure) => eprint!("Worker '{relative}' failed: {failure}"),
        }
    }
}

/// Streams one topic as server-sent events, resuming from a cursor.
///
/// The legacy implementation established this shape: an append-only NDJSON log
/// per topic, read by an integer-position cursor. Serving it as server-sent
/// events keeps that contract and lets the browser own reconnection, since
/// `EventSource` resends its last event id automatically.
///
/// Publishing is appending a line to the log, so a handler in any language can
/// publish without a client library.
fn subscribe_topic(state: &Dispatch, topic: &str, request: &Request<Body>) -> Response<Body> {
    if topic.is_empty()
        || topic.len() > 64
        || !topic
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return text_response(StatusCode::BAD_REQUEST, "Topic must be a bounded slug.");
    }
    // A reconnecting EventSource sends its last id; an explicit cursor is also
    // accepted so a non-browser client can resume.
    let resume = request
        .headers()
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map_or_else(
            || {
                request
                    .uri()
                    .query()
                    .and_then(|query| {
                        query
                            .split('&')
                            .find_map(|pair| pair.strip_prefix("position="))
                    })
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or(0)
            },
            |id| id.saturating_add(1),
        );

    let log = state
        .project_root
        .join(TOPIC_DIRECTORY)
        .join(format!("{topic}.jsonl"));
    let (mut writer, reader) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt as _;

        let mut position = resume;
        loop {
            let records = tokio::fs::metadata(&log)
                .await
                .ok()
                .filter(|metadata| metadata.is_file() && metadata.len() <= MAX_TOPIC_BYTES)
                .and_then(|_| std::fs::read_to_string(&log).ok())
                .unwrap_or_default();
            for (index, record) in records.lines().enumerate() {
                let index = index as u64;
                if index < position || record.trim().is_empty() {
                    continue;
                }
                let frame = format!("id: {index}\ndata: {record}\n\n");
                if writer.write_all(frame.as_bytes()).await.is_err() {
                    return;
                }
                position = index + 1;
            }
            if writer.flush().await.is_err() {
                return;
            }
            tokio::time::sleep(TOPIC_POLL).await;
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/event-stream")
        .header(CACHE_CONTROL, "no-store")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(tokio_util::io::ReaderStream::new(reader)))
        .unwrap_or_else(|_| {
            text_response(StatusCode::INTERNAL_SERVER_ERROR, "Cannot open the stream.")
        })
}

/// Lets middleware observe and adjust a response before it is sent.
///
/// The response status and headers are passed, but never the body: an after
/// phase is overwhelmingly used to add headers, and buffering every static
/// file to hand it over would cost far more than that is worth.
///
/// Returning 204 merges any headers the middleware supplied. Returning any
/// other status replaces the response entirely.
async fn run_middleware_after(
    state: &Dispatch,
    middleware: &HandlerSource,
    path: &str,
    method: HttpMethod,
    request_headers: &axum::http::HeaderMap,
    response: Response<Body>,
) -> Response<Body> {
    let mut protocol_request =
        HandlerRequest::route(crate::ttid::generate(), String::from(path), method);
    protocol_request.operation = String::from("middleware.after");
    // `headers` means the request's headers in both phases, so middleware can
    // reason about the same request it authorised. The response is reported
    // under namespaced names that cannot collide with them.
    for (name, value) in request_headers {
        if let Ok(value) = value.to_str() {
            protocol_request
                .headers
                .entry(name.as_str().to_owned())
                .or_default()
                .push(value.to_owned());
        }
    }
    protocol_request.headers.insert(
        String::from("x-tachyon-status"),
        vec![response.status().as_u16().to_string()],
    );
    for (name, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            protocol_request
                .headers
                .entry(format!("x-tachyon-response-{name}"))
                .or_default()
                .push(value.to_owned());
        }
    }

    let cancellation = HandlerCancellation::default();
    let decision = match state
        .supervisor
        .invoke(middleware, &protocol_request, &cancellation)
        .await
    {
        Ok(decision) => decision,
        Err(failure) => {
            eprintln!(
                "{}",
                invocation_failure_event(
                    "middleware.after",
                    &protocol_request.request_id,
                    &failure,
                )
            );
            // An after-phase failure must not discard a response the request
            // already earned, so the original is sent unchanged.
            return response;
        }
    };
    if decision.status != 204 {
        return handler_response(decision);
    }

    let (mut parts, body) = response.into_parts();
    for (name, values) in &decision.headers {
        let Ok(name) = axum::http::HeaderName::try_from(name.as_str()) else {
            continue;
        };
        for value in values {
            if let Ok(value) = HeaderValue::from_str(value) {
                parts.headers.insert(name.clone(), value);
            }
        }
    }
    Response::from_parts(parts, body)
}

/// Serves generated output for a request no handler owns.
async fn serve_static(state: Dispatch, request: Request<Body>) -> Response<Body> {
    let Some(response) = serve_generated(&state.files, &state.page_routes, request).await else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot serve generated output.",
        );
    };
    let is_document = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"));
    if !state.watch || !is_document {
        return response;
    }

    // The reload script is added while serving, so published output never
    // carries it and `ty build` stays byte-identical.
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, MAX_DOCUMENT_BYTES).await else {
        return text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Generated document is too large to serve.",
        );
    };
    let Ok(document) = std::str::from_utf8(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    let injected = document.rfind("</body>").map_or_else(
        || format!("{document}{LIVE_RELOAD}"),
        |position| {
            format!(
                "{}{LIVE_RELOAD}{}",
                &document[..position],
                &document[position..]
            )
        },
    );
    parts.headers.remove(axum::http::header::CONTENT_LENGTH);
    Response::from_parts(parts, Body::from(injected))
}

async fn serve_preview(
    State(state): State<PreviewDispatch>,
    request: Request<Body>,
) -> Response<Body> {
    serve_generated(&state.files, &state.page_routes, request)
        .await
        .unwrap_or_else(|| {
            text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot serve the preview bundle.",
            )
        })
}

/// Serves an authored output path before considering a dynamic page rewrite.
///
/// A route such as `/docs/_topic` also matches `/docs/client.js`. Generated
/// assets are concrete public files, so they must win; only a real static 404
/// is eligible for the dynamic page fallback.
async fn serve_generated(
    files: &ServeDir,
    patterns: &[String],
    request: Request<Body>,
) -> Option<Response<Body>> {
    let mut fallback = request_without_body(&request);
    let response = files.clone().oneshot(request).await.ok()?.map(Body::new);
    if response.status() != StatusCode::NOT_FOUND
        || !rewrite_dynamic_request(patterns, &mut fallback)
    {
        return Some(response);
    }
    files
        .clone()
        .oneshot(fallback)
        .await
        .ok()
        .map(|response| response.map(Body::new))
}

fn request_without_body(request: &Request<Body>) -> Request<Body> {
    let mut copy = Request::new(Body::empty());
    *copy.method_mut() = request.method().clone();
    *copy.uri_mut() = request.uri().clone();
    *copy.version_mut() = request.version();
    *copy.headers_mut() = request.headers().clone();
    copy
}

fn rewrite_dynamic_request(patterns: &[String], request: &mut Request<Body>) -> bool {
    let path = request.uri().path();
    let Some(matched) = match_route(patterns.iter().map(String::as_str), path) else {
        return false;
    };
    if matched.route == path || !matched.route.contains("/_") {
        return false;
    }
    let query = request
        .uri()
        .query()
        .map_or_else(String::new, |value| format!("?{value}"));
    let rewritten = format!("{}/{}", matched.route.trim_end_matches('/'), query);
    if let Ok(uri) = rewritten.parse() {
        *request.uri_mut() = uri;
        return true;
    }
    false
}

fn preview_route_patterns(root: &Path) -> Vec<String> {
    let path = root.join("route-manifest.json");
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return Vec::new();
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 4 * 1024 * 1024
    {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    manifest
        .get("routes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("route").and_then(serde_json::Value::as_str))
        .map(String::from)
        .collect()
}

/// Rebuilds the project whenever its sources change.
///
/// A failed rebuild is reported and the previous output is left published, so
/// a syntax error never takes the running site down.
async fn watch_sources(
    project_root: PathBuf,
    options: BuildOptions,
    hot_updates: HotUpdateHub,
    mut watcher: SourceWatcher,
) {
    loop {
        let Some(event) = watcher.receive().await else {
            return;
        };
        let mut changes = SourceChanges::new();
        match event {
            Ok(event) => changes.record_event(&project_root, event),
            Err(error) => {
                eprintln!("Source watcher reported an error: {error}");
                changes.force_reload();
            }
        }
        tokio::time::sleep(WATCH_DEBOUNCE).await;
        watcher.drain(&mut changes, &project_root);
        if changes.is_empty() {
            continue;
        }
        let paths = changes.paths();
        let action = changes.action();
        match WebCompiler::build_async(&project_root, &options).await {
            Ok(result) => {
                let (kind, boundaries) = match action {
                    SourceAction::Css => (HotUpdateKind::Css, Vec::new()),
                    SourceAction::Island { boundaries } => (HotUpdateKind::Island, boundaries),
                    SourceAction::Reload => (HotUpdateKind::Reload, Vec::new()),
                };
                hot_updates.publish(
                    kind,
                    Some(String::from(result.sha256())),
                    paths,
                    boundaries,
                    None,
                );
                println!(
                    "Rebuilt {} route(s) ({})",
                    result.route_count(),
                    result.sha256()
                );
            }
            Err(failure) => {
                hot_updates.publish(
                    HotUpdateKind::Diagnostics,
                    None,
                    paths,
                    Vec::new(),
                    Some(failure.report()),
                );
                eprint!("{failure}");
            }
        }
    }
}

fn text_response(status: StatusCode, message: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(String::from(message)))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn invocation_failure_response(kind: &str, request_id: &str) -> Response<Body> {
    let mut response = text_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        &format!("{kind} failed. Reference: {request_id}."),
    );
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-tachyon-request-id"), value);
    }
    response
}

fn invocation_failure_event(operation: &str, request_id: &str, failure: &Failure) -> String {
    let diagnostic_codes = failure
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect::<Vec<_>>();
    serde_json::json!({
        "event": "handler.invocation_failed",
        "operation": operation,
        "request_id": request_id,
        "diagnostic_codes": diagnostic_codes,
    })
    .to_string()
}

fn defensive_headers(application: Router) -> Router {
    application
        .layer(SetResponseHeaderLayer::if_not_present(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
                 script-src 'self' 'wasm-unsafe-eval' 'inline-speculation-rules'; object-src 'none'; \
                 base-uri 'none'; frame-ancestors 'none'",
            ),
        ))
}

fn application(dispatch_state: Dispatch) -> Router {
    defensive_headers(
        Router::new().fallback(axum::routing::any(dispatch).with_state(dispatch_state)),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        DevServer, DevServerOptions, PreviewServer, PreviewServerOptions, invocation_failure_event,
        invocation_failure_response, serve_generated,
    };
    use crate::Failure;
    use crate::failure::diagnostic;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr, TcpListener as StandardTcpListener};
    use tower_http::services::ServeDir;

    #[tokio::test]
    async fn invocation_failures_are_correlatable_and_redacted_for_every_http_path() {
        let failure = Failure::one(diagnostic(
            2101,
            "secret-canary from process stderr",
            Some(String::from("secret-canary recovery detail")),
            None,
        ));
        for operation in ["handler", "middleware", "middleware.after"] {
            let request_id = "0ABC123DEFG";
            let event = invocation_failure_event(operation, request_id, &failure);
            assert!(event.contains(r#""event":"handler.invocation_failed""#));
            assert!(event.contains(&format!(r#""operation":"{operation}""#)));
            assert!(event.contains(r#""request_id":"0ABC123DEFG""#));
            assert!(event.contains(r#""diagnostic_codes":["TY2101"]"#));
            assert!(!event.contains("secret-canary"));
        }

        for kind in ["Handler execution", "Middleware execution"] {
            let request_id = "0ABC123DEFG";
            let response = invocation_failure_response(kind, request_id);
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(
                response
                    .headers()
                    .get("x-tachyon-request-id")
                    .and_then(|value| value.to_str().ok()),
                Some(request_id)
            );
            let body = to_bytes(response.into_body(), 1024)
                .await
                .expect("redacted body");
            let body = std::str::from_utf8(&body).expect("UTF-8 body");
            assert!(body.contains(kind));
            assert!(body.contains(request_id));
            assert!(!body.contains("secret-canary"));
        }
    }

    #[tokio::test]
    async fn generated_assets_take_precedence_over_dynamic_page_patterns() {
        let root = tempfile::tempdir().expect("bundle");
        fs::create_dir_all(root.path().join("docs/_topic")).expect("dynamic directory");
        fs::write(
            root.path().join("docs/_topic/index.html"),
            "<main>Dynamic topic</main>",
        )
        .expect("dynamic page");
        fs::write(
            root.path().join("docs/client.js"),
            "document.title = 'Static client';",
        )
        .expect("static asset");
        let files = ServeDir::new(root.path()).append_index_html_on_directories(true);
        let patterns = vec![String::from("/docs/_topic")];

        let asset = serve_generated(
            &files,
            &patterns,
            Request::builder()
                .uri("/docs/client.js")
                .body(Body::empty())
                .expect("asset request"),
        )
        .await
        .expect("asset response");
        let asset_body = to_bytes(asset.into_body(), 1024).await.expect("asset body");
        assert_eq!(asset_body, "document.title = 'Static client';");

        let dynamic = serve_generated(
            &files,
            &patterns,
            Request::builder()
                .uri("/docs/introduction")
                .body(Body::empty())
                .expect("dynamic request"),
        )
        .await
        .expect("dynamic response");
        let dynamic_body = to_bytes(dynamic.into_body(), 1024)
            .await
            .expect("dynamic body");
        assert_eq!(dynamic_body, "<main>Dynamic topic</main>");
    }

    #[tokio::test]
    async fn ephemeral_loopback_binding_reports_the_real_address() {
        let root = tempfile::tempdir().expect("project");
        let source = root.path().join("client/pages/tac.html");
        fs::create_dir_all(source.parent().expect("parent")).expect("directory");
        fs::write(source, "<main>Page</main>").expect("source");
        let server = DevServer::bind(
            root.path(),
            &DevServerOptions {
                port: 0,
                ..DevServerOptions::default()
            },
        )
        .await
        .expect("server");
        assert!(server.address().ip().is_loopback());
        assert_ne!(server.address().port(), 0);
        assert_eq!(
            server
                .build()
                .expect("initial build evidence")
                .route_count(),
            1
        );
        server.run_until(async {}).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn preview_serves_existing_output_without_rebuilding_it() {
        let root = tempfile::tempdir().expect("bundle");
        fs::write(root.path().join("index.html"), "<main>Published</main>").expect("output");
        let before = fs::read(root.path().join("index.html")).expect("before");
        let server = PreviewServer::bind(
            root.path(),
            &PreviewServerOptions {
                port: 0,
                ..PreviewServerOptions::default()
            },
        )
        .await
        .expect("preview");
        assert_eq!(server.root(), root.path());
        assert_ne!(server.address().port(), 0);
        assert_eq!(
            fs::read(root.path().join("index.html")).expect("after"),
            before
        );
        server.run_until(async {}).await.expect("clean shutdown");
    }

    #[tokio::test]
    async fn unsafe_and_occupied_bindings_have_stable_diagnostics() {
        let root = tempfile::tempdir().expect("project");
        let unsafe_options = DevServerOptions {
            host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            ..DevServerOptions::default()
        };
        assert!(
            DevServer::bind(root.path(), &unsafe_options)
                .await
                .expect_err("unsafe bind")
                .to_string()
                .contains("TY1301")
        );

        let source = root.path().join("client/pages/tac.html");
        fs::create_dir_all(source.parent().expect("parent")).expect("directory");
        fs::write(source, "<main>Page</main>").expect("source");
        let occupied = StandardTcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("listener");
        let port = occupied.local_addr().expect("address").port();
        let options = DevServerOptions {
            port,
            ..DevServerOptions::default()
        };
        assert!(
            DevServer::bind(root.path(), &options)
                .await
                .expect_err("occupied")
                .to_string()
                .contains("TY1302")
        );
    }
}
