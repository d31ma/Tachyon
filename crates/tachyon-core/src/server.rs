use crate::failure::diagnostic;
use crate::handler::{
    HandlerCancellation, HandlerSource, HandlerSupervisor, RuntimeRequirements, YonLanguage,
};
use crate::hot_update::{SourceAction, SourceChanges, SourceWatcher};
use crate::routing::match_route;
use crate::{BuildOptions, BuildResult, Failure, Project, ProjectDiscovery, WebCompiler};
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::header::{
    CACHE_CONTROL, CONNECTION, CONTENT_TYPE, HeaderName, HeaderValue, REFERRER_POLICY,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{Request, Response, StatusCode};
use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::fs::{Dir, OpenOptions};
use std::collections::{HashMap, VecDeque};
use std::future::{Future, IntoFuture};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tachyon_contracts::{
    HandlerBody, HandlerBodyEncoding, HandlerRequest, HandlerResponse, HotUpdate, HotUpdateKind,
    HttpMethod,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinSet;
use tokio_util::io::ReaderStream;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt as _;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

/// Buffer bridging streamed handler events onto the response body.
const SSE_BRIDGE_BYTES: usize = 16 * 1024;
/// Largest request body accepted by a dispatched handler.
const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
/// Largest generated document the server will rewrite while serving.
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
/// Quiet period used to combine one editor save into one rebuild.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(75);
/// Prefix serving a topic subscription as server-sent events.
const TOPIC_ENDPOINT: &str = "/.tachyon/topics/";
/// Project-relative directory holding append-only topic logs.
#[cfg(test)]
const TOPIC_DIRECTORY: &str = ".tachyon/topics";
/// Largest topic log the server will read in one pass.
const MAX_TOPIC_BYTES: u64 = 16 * 1024 * 1024;
const TOPIC_POLL: Duration = Duration::from_millis(50);
const MAX_TOPIC_SUBSCRIBERS: usize = 128;
const MAX_TOPIC_SUBSCRIBERS_PER_TOPIC: usize = 32;
const MAX_ACTIVE_TOPICS: usize = 32;
const TOPIC_REPLAY_RECORDS: usize = 256;
const TOPIC_BROADCAST_RECORDS: usize = 128;
const MAX_TOPIC_RECORD_BYTES: usize = 64 * 1024;
/// One deadline shared by HTTP graceful close, response producers, scheduled
/// workers, and the source watcher after shutdown begins.
const SERVER_SHUTDOWN: Duration = Duration::from_secs(3);
/// Final portion of the global shutdown deadline reserved for aborting and
/// joining tasks that ignored cooperative cancellation.
const ABORT_SETTLEMENT: Duration = Duration::from_millis(500);
/// Maximum cooperative task-drain phase. The remaining global budget is kept
/// for force-closing and actually joining cancelled process/file tasks under
/// scheduler or blocking-pool contention.
const COOPERATIVE_TASK_SETTLEMENT: Duration = Duration::from_secs(1);
/// Completed response tasks are reaped while the server remains live so their
/// `JoinSet` records cannot grow with request count.
const PRODUCER_REAP_INTERVAL: Duration = Duration::from_millis(25);
/// A running worker receives its handler cancellation protocol before its
/// invocation future is dropped during server shutdown.
const WORKER_CANCELLATION_SETTLE: Duration = Duration::from_secs(1);

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
    contract: Option<Arc<crate::handler::RouteContract>>,
    request_schemas: Arc<std::collections::BTreeMap<String, StagedRequest>>,
    /// Upper-case HTTP methods declared with `@Stream` in the handler source.
    streaming: Arc<std::collections::BTreeSet<String>>,
}

#[derive(Clone)]
struct TopicHub {
    files: TopicFiles,
    topics: Arc<Mutex<HashMap<String, Arc<TopicState>>>>,
    subscribers: Arc<AtomicUsize>,
    cancellation: CancellationToken,
    producers: ProducerTasks,
}

#[derive(Clone)]
struct TopicFiles {
    project: Arc<Dir>,
    directory: Arc<Mutex<Option<Arc<Dir>>>>,
}

struct TopicState {
    sender: broadcast::Sender<(u64, String, bool)>,
    replay: Mutex<VecDeque<(u64, String, bool)>>,
    replay_evicted: AtomicBool,
    subscribers: AtomicUsize,
    started: AtomicBool,
    cancellation: CancellationToken,
}

struct TopicAdmission {
    hub: TopicHub,
    name: String,
    topic: Arc<TopicState>,
}

impl Drop for TopicAdmission {
    fn drop(&mut self) {
        let mut topics = self
            .hub
            .topics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.hub.subscribers.fetch_sub(1, Ordering::AcqRel);
        let previous = self.topic.subscribers.fetch_sub(1, Ordering::AcqRel);
        if previous == 1
            && topics
                .get(&self.name)
                .is_some_and(|current| Arc::ptr_eq(current, &self.topic))
        {
            topics.remove(&self.name);
            self.topic.cancellation.cancel();
        }
    }
}

impl TopicHub {
    fn new(
        project: Arc<Dir>,
        cancellation: CancellationToken,
        producers: ProducerTasks,
    ) -> Result<Self, String> {
        let directory = open_topic_directory(&project)?.map(Arc::new);
        Ok(Self {
            files: TopicFiles {
                project,
                directory: Arc::new(Mutex::new(directory)),
            },
            topics: Arc::new(Mutex::new(HashMap::new())),
            subscribers: Arc::new(AtomicUsize::new(0)),
            cancellation,
            producers,
        })
    }

    fn subscribe(&self, topic: &str) -> Result<(Arc<TopicState>, TopicAdmission), &'static str> {
        if self.subscribers.fetch_add(1, Ordering::AcqRel) >= MAX_TOPIC_SUBSCRIBERS {
            self.subscribers.fetch_sub(1, Ordering::AcqRel);
            return Err("Topic subscriber capacity is exhausted.");
        }
        let (state, start) = {
            let mut topics = self
                .topics
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(state) = topics.get(topic) {
                if state.subscribers.fetch_add(1, Ordering::AcqRel)
                    >= MAX_TOPIC_SUBSCRIBERS_PER_TOPIC
                {
                    state.subscribers.fetch_sub(1, Ordering::AcqRel);
                    self.subscribers.fetch_sub(1, Ordering::AcqRel);
                    return Err("Topic subscriber capacity is exhausted.");
                }
                (Arc::clone(state), false)
            } else {
                if topics.len() >= MAX_ACTIVE_TOPICS {
                    self.subscribers.fetch_sub(1, Ordering::AcqRel);
                    return Err("Topic capacity is exhausted.");
                }
                let (sender, _) = broadcast::channel(TOPIC_BROADCAST_RECORDS);
                let state = Arc::new(TopicState {
                    sender,
                    replay: Mutex::new(VecDeque::with_capacity(TOPIC_REPLAY_RECORDS)),
                    replay_evicted: AtomicBool::new(false),
                    subscribers: AtomicUsize::new(1),
                    started: AtomicBool::new(false),
                    cancellation: self.cancellation.child_token(),
                });
                topics.insert(topic.to_owned(), Arc::clone(&state));
                (state, true)
            }
        };
        if start && !state.started.swap(true, Ordering::AcqRel) {
            let files = self.files.clone();
            let topic_name = topic.to_owned();
            let cancellation = state.cancellation.clone();
            let tail_state = Arc::clone(&state);
            if !self
                .producers
                .spawn(tail_topic_file(files, topic_name, tail_state, cancellation))
            {
                state.started.store(false, Ordering::Release);
                let admission = TopicAdmission {
                    hub: self.clone(),
                    name: topic.to_owned(),
                    topic: Arc::clone(&state),
                };
                drop(admission);
                return Err("Topic stream is unavailable.");
            }
        }
        Ok((
            Arc::clone(&state),
            TopicAdmission {
                hub: self.clone(),
                name: topic.to_owned(),
                topic: state,
            },
        ))
    }
}

async fn tail_topic_file(
    files: TopicFiles,
    topic: String,
    state: Arc<TopicState>,
    cancellation: CancellationToken,
) {
    let mut position = 0_u64;
    let mut bytes = 0_u64;
    let mut reader = loop {
        if cancellation.is_cancelled() {
            return;
        }
        let opened = tokio::task::spawn_blocking({
            let files = files.clone();
            let topic = topic.clone();
            move || open_topic_file(&files, &topic)
        });
        tokio::pin!(opened);
        let opened = tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                opened.abort();
                return;
            }
            result = &mut opened => result,
        };
        match opened {
            Ok(Ok(Some(file))) => break BufReader::new(tokio::fs::File::from_std(file)),
            Ok(Ok(None)) => {
                tokio::select! {
                    () = cancellation.cancelled() => return,
                    () = tokio::time::sleep(TOPIC_POLL) => {}
                }
            }
            Err(_) | Ok(Err(_)) => {
                publish_topic_error(&state, position, TopicFailure::Open);
                return;
            }
        }
    };
    let mut record = String::new();
    loop {
        if cancellation.is_cancelled() {
            return;
        }
        let read = tokio::select! {
            biased;
            () = cancellation.cancelled() => return,
            result = reader.read_line(&mut record) => result,
        };
        match read {
            Ok(0) => {
                tokio::select! {
                    () = cancellation.cancelled() => return,
                    () = tokio::time::sleep(TOPIC_POLL) => {}
                }
                continue;
            }
            Ok(read) => bytes = bytes.saturating_add(read as u64),
            Err(_) => {
                publish_topic_error(&state, position, TopicFailure::Read);
                return;
            }
        }
        if bytes > MAX_TOPIC_BYTES {
            publish_topic_error(&state, position, TopicFailure::LogLimit);
            return;
        }
        if record.len() > MAX_TOPIC_RECORD_BYTES {
            publish_topic_error(&state, position, TopicFailure::RecordLimit);
            return;
        }
        if !record.ends_with('\n') {
            continue;
        }
        let Ok(frame) = topic_frame(position, &record) else {
            publish_topic_error(&state, position, TopicFailure::InvalidJson);
            return;
        };
        retain_topic_frame(&state, position, frame.clone(), false);
        let _ = state.sender.send((position, frame, false));
        position += 1;
        record.clear();
    }
}

fn topic_frame(position: u64, record: &str) -> Result<String, &'static str> {
    let Some(line) = record.strip_suffix('\n') else {
        return Err("Topic record is not valid JSON.");
    };
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() || line.chars().any(char::is_control) {
        return Err("Topic record is not valid JSON.");
    }
    let value = serde_json::from_str::<serde_json::Value>(line)
        .map_err(|_| "Topic record is not valid JSON.")?;
    let canonical = serde_json::to_string(&value).map_err(|_| "Topic record is not valid JSON.")?;
    Ok(format!("id: {position}\ndata: {canonical}\n\n"))
}

#[derive(Clone, Copy)]
enum TopicFailure {
    Open,
    Read,
    LogLimit,
    RecordLimit,
    InvalidJson,
}

fn publish_topic_error(state: &TopicState, position: u64, failure: TopicFailure) {
    let (code, message, guidance) = match failure {
        TopicFailure::Open => (
            "TY_TOPIC_OPEN",
            "Topic stream cannot be opened.",
            "Check that the topic log is a project-contained regular file.",
        ),
        TopicFailure::Read => (
            "TY_TOPIC_READ",
            "Topic stream read failed.",
            "Reconnect after the topic log is readable.",
        ),
        TopicFailure::LogLimit => (
            "TY_TOPIC_LOG_LIMIT",
            "Topic stream exceeded its byte limit.",
            "Rotate the topic log before reconnecting.",
        ),
        TopicFailure::RecordLimit => (
            "TY_TOPIC_RECORD_LIMIT",
            "Topic record exceeded its byte limit.",
            "Publish a smaller JSON record before reconnecting.",
        ),
        TopicFailure::InvalidJson => (
            "TY_TOPIC_INVALID_JSON",
            "Topic record is not valid JSON.",
            "Repair the NDJSON record before reconnecting.",
        ),
    };
    let frame = topic_error_frame(code, message, guidance);
    retain_topic_frame(state, position, frame.clone(), true);
    let _ = state.sender.send((position, frame, true));
}

fn topic_error_frame(code: &str, message: &str, guidance: &str) -> String {
    let payload = serde_json::json!({
        "category": "topic",
        "code": code,
        "guidance": guidance,
        "message": message,
        "terminal": true,
    });
    format!("event: topic-error\ndata: {payload}\n\n")
}

fn retain_topic_frame(state: &TopicState, position: u64, frame: String, terminal: bool) {
    let mut replay = state
        .replay
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if replay.len() == TOPIC_REPLAY_RECORDS {
        replay.pop_front();
        state.replay_evicted.store(true, Ordering::Release);
    }
    replay.push_back((position, frame, terminal));
}

fn open_topic_directory(project: &Dir) -> Result<Option<Dir>, String> {
    let tachyon = match project.open_dir_nofollow(".tachyon") {
        Ok(directory) => directory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(String::from("Topic directory cannot be opened.")),
    };
    match tachyon.open_dir_nofollow("topics") {
        Ok(directory) => Ok(Some(directory)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(String::from("Topic directory cannot be opened.")),
    }
}

fn open_topic_file(files: &TopicFiles, topic: &str) -> Result<Option<std::fs::File>, String> {
    let topics = {
        let mut retained = files
            .directory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if retained.is_none() {
            *retained = open_topic_directory(&files.project)?.map(Arc::new);
        }
        retained.clone()
    };
    let Some(topics) = topics else {
        return Ok(None);
    };
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = match topics.open_with(format!("{topic}.jsonl"), &options) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(String::from("Topic stream cannot be opened.")),
    };
    let metadata = file
        .metadata()
        .map_err(|_| String::from("Topic stream cannot be inspected."))?;
    if !metadata.is_file() || metadata.len() > MAX_TOPIC_BYTES {
        return Err(String::from("Topic stream must be a bounded regular file."));
    }
    Ok(Some(file.into_std()))
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
    topic_hub: TopicHub,
    /// Server lifetime shared by request-scoped response producers.
    cancellation: CancellationToken,
    /// Owns every task that can outlive the request future which created it.
    producers: ProducerTasks,
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
pub struct DevServer {
    listener: Option<TcpListener>,
    application: Option<Router>,
    address: SocketAddr,
    build: Option<BuildResult>,
    runtime: ServerRuntime,
}

impl std::fmt::Debug for DevServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DevServer")
            .field("address", &self.address)
            .field("build", &self.build)
            .field("background_started", &self.runtime.started())
            .finish_non_exhaustive()
    }
}

/// Fallible resources captured at bind time and started only when the server
/// actually runs. A merely bound then dropped server therefore has no detached
/// worker or watcher task.
struct BackgroundPlan {
    supervisor: Arc<HandlerSupervisor>,
    workers: Vec<crate::project::ScheduledWorker>,
    watcher: Option<WatchTask>,
}

struct WatchTask {
    project_root: PathBuf,
    options: BuildOptions,
    hot_updates: HotUpdateHub,
    watcher: SourceWatcher,
}

/// Owns every asynchronous task whose lifetime is the development server's.
///
/// `JoinSet` aborts all contained tasks when dropped. The explicit token gives
/// normal shutdown a cooperative path first, including the handler process
/// cancellation protocol for an in-flight worker.
struct ServerRuntime {
    cancellation: CancellationToken,
    tasks: JoinSet<()>,
    producers: ProducerTasks,
    plan: Option<BackgroundPlan>,
}

#[derive(Clone)]
struct ProducerTasks {
    inner: Arc<Mutex<ProducerTaskState>>,
}

struct ProducerTaskState {
    accepting: bool,
    tasks: JoinSet<()>,
}

impl std::fmt::Debug for ProducerTasks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state();
        formatter
            .debug_struct("ProducerTasks")
            .field("accepting", &state.accepting)
            .field("tasks", &state.tasks.len())
            .finish()
    }
}

impl ProducerTasks {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ProducerTaskState {
                accepting: true,
                tasks: JoinSet::new(),
            })),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ProducerTaskState> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn spawn<F>(&self, task: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut state = self.state();
        reap_finished_producers(&mut state.tasks);
        if !state.accepting {
            return false;
        }
        state.tasks.spawn(task);
        true
    }

    fn close(&self) {
        self.state().accepting = false;
    }

    fn take(&self) -> JoinSet<()> {
        let mut state = self.state();
        state.accepting = false;
        std::mem::take(&mut state.tasks)
    }

    fn close_and_abort(&self) {
        let mut state = self.state();
        state.accepting = false;
        state.tasks.abort_all();
    }

    fn reap_finished(&self) {
        reap_finished_producers(&mut self.state().tasks);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.state().tasks.len()
    }
}

fn reap_finished_producers(tasks: &mut JoinSet<()>) {
    while let Some(result) = tasks.try_join_next() {
        report_server_task("response producer", result);
    }
}

impl std::fmt::Debug for ServerRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServerRuntime")
            .field("started", &self.started())
            .field("tasks", &self.tasks.len())
            .finish_non_exhaustive()
    }
}

impl ServerRuntime {
    fn new(plan: BackgroundPlan) -> Self {
        Self {
            cancellation: CancellationToken::new(),
            tasks: JoinSet::new(),
            producers: ProducerTasks::new(),
            plan: Some(plan),
        }
    }

    fn started(&self) -> bool {
        self.plan.is_none()
    }

    fn start(&mut self) {
        let Some(plan) = self.plan.take() else {
            return;
        };
        spawn_workers(
            &plan.workers,
            &plan.supervisor,
            &self.cancellation,
            &mut self.tasks,
        );
        if let Some(watch) = plan.watcher {
            let cancellation = self.cancellation.clone();
            self.tasks.spawn(watch_sources(
                watch.project_root,
                watch.options,
                watch.hot_updates,
                watch.watcher,
                cancellation,
            ));
        }
        let producers = self.producers.clone();
        let cancellation = self.cancellation.clone();
        self.tasks
            .spawn(reap_producer_tasks(producers, cancellation));
    }

    async fn shutdown_until(&mut self, hard_deadline: tokio::time::Instant) {
        self.cancellation.cancel();
        let mut background = std::mem::take(&mut self.tasks);
        let mut producers = self.producers.take();
        let cooperative_deadline = cooperative_shutdown_deadline(hard_deadline)
            .min(tokio::time::Instant::now() + COOPERATIVE_TASK_SETTLEMENT);
        if tokio::time::timeout_at(
            cooperative_deadline,
            drain_server_tasks(&mut background, &mut producers),
        )
        .await
        .is_err()
        {
            background.abort_all();
            producers.abort_all();
            if tokio::time::timeout_at(
                hard_deadline,
                drain_server_tasks(&mut background, &mut producers),
            )
            .await
            .is_err()
            {
                // A non-yielding async task cannot be made safe by waiting
                // forever. Abort once more and let JoinSet's Drop remain the
                // final non-blocking containment boundary.
                background.abort_all();
                producers.abort_all();
                eprintln!("Development server tasks did not settle within the abort deadline.");
            }
        }
    }
}

impl Drop for ServerRuntime {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.tasks.abort_all();
        self.producers.close_and_abort();
    }
}

async fn drain_server_tasks(background: &mut JoinSet<()>, producers: &mut JoinSet<()>) {
    let mut joined = 0_usize;
    while !background.is_empty() || !producers.is_empty() {
        let (kind, result) = tokio::select! {
            result = background.join_next(), if !background.is_empty() => ("background", result),
            result = producers.join_next(), if !producers.is_empty() => ("response producer", result),
        };
        let Some(result) = result else {
            continue;
        };
        report_server_task(kind, result);
        joined += 1;
        if joined.is_multiple_of(16) {
            tokio::task::yield_now().await;
        }
    }
}

fn report_server_task(kind: &str, result: Result<(), tokio::task::JoinError>) {
    if let Err(error) = result
        && !error.is_cancelled()
    {
        eprintln!("Development {kind} task failed: {error}");
    }
}

fn cooperative_shutdown_deadline(hard_deadline: tokio::time::Instant) -> tokio::time::Instant {
    match hard_deadline.checked_sub(ABORT_SETTLEMENT) {
        Some(deadline) => deadline,
        None => hard_deadline,
    }
}

async fn reap_producer_tasks(producers: ProducerTasks, cancellation: CancellationToken) {
    let mut interval = tokio::time::interval(PRODUCER_REAP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => {
                producers.reap_finished();
                return;
            }
            _ = interval.tick() => producers.reap_finished(),
        }
    }
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
        validate_dev_exposure(options)?;
        let project = ProjectDiscovery::discover(project_root)?;
        Self::bind_project(&project, options).await
    }

    /// Binds a development server from one immutable discovery snapshot.
    ///
    /// The initial web build, route dispatch, middleware, and scheduled
    /// workers all consume the same retained project inputs.
    ///
    /// # Errors
    ///
    /// Returns diagnostics for unsafe exposure, build failure, or bind failure.
    pub async fn bind_project(
        project: &Project,
        options: &DevServerOptions,
    ) -> Result<Self, Failure> {
        validate_dev_exposure(options)?;
        let supervisor = Arc::new(HandlerSupervisor::from_environment()?);
        Self::bind_project_with_supervisor(project, options, supervisor).await
    }

    async fn bind_project_with_supervisor(
        project: &Project,
        options: &DevServerOptions,
        supervisor: Arc<HandlerSupervisor>,
    ) -> Result<Self, Failure> {
        let requirements = RuntimeRequirements::from_sources(project.invocation_sources());
        supervisor.preflight(&requirements).await?;
        // Captured schemas and the selected validator must be ready before
        // output publication, socket binding, or background-task admission.
        let (routes, page_routes) = discover_dispatch_routes(project).await?;
        let (build, output_directory) = prepare_dev_output(project, options).await?;
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
        let middleware = project.middleware().cloned().map(Arc::new);

        let hot_updates =
            HotUpdateHub::new(build.as_ref().map(|result| String::from(result.sha256())));
        let watcher = if options.watch {
            let watched_root = project.root().to_path_buf();
            let watcher = SourceWatcher::start(&watched_root)?;
            Some(WatchTask {
                project_root: watched_root,
                options: BuildOptions {
                    output_directory: options.output_directory.clone(),
                    ..BuildOptions::default()
                },
                hot_updates: hot_updates.clone(),
                watcher,
            })
        } else {
            None
        };
        let runtime = ServerRuntime::new(BackgroundPlan {
            supervisor: Arc::clone(&supervisor),
            workers: project.workers().to_vec(),
            watcher,
        });
        let topic_hub = TopicHub::new(
            project.capability(),
            runtime.cancellation.clone(),
            runtime.producers.clone(),
        )
        .map_err(|message| {
            Failure::one(diagnostic(
                1303,
                message,
                Some(String::from(
                    "Use regular, project-contained .tachyon/topics directories.",
                )),
                None,
            ))
        })?;
        let dispatch_state = Dispatch {
            routes: Arc::new(routes),
            page_routes: Arc::new(page_routes),
            supervisor: Arc::clone(&supervisor),
            files: ServeDir::new(&output_directory).append_index_html_on_directories(true),
            hot_updates,
            watch: options.watch,
            middleware,
            topic_hub,
            cancellation: runtime.cancellation.clone(),
            producers: runtime.producers.clone(),
        };
        let application = application(dispatch_state);
        Ok(Self {
            listener: Some(listener),
            application: Some(application),
            address,
            build,
            runtime,
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
    pub async fn run_until<F>(mut self, shutdown: F) -> Result<(), Failure>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.runtime.start();
        let (Some(listener), Some(application)) = (self.listener.take(), self.application.take())
        else {
            self.runtime
                .shutdown_until(tokio::time::Instant::now() + SERVER_SHUTDOWN)
                .await;
            return Err(Failure::one(diagnostic(
                1303,
                "Development server ownership was already consumed.",
                Some(String::from(
                    "Bind a new development server before serving.",
                )),
                None,
            )));
        };
        let cancellation = self.runtime.cancellation.clone();
        let graceful_cancellation = cancellation.clone();
        let (served, deadline) = {
            let served = axum::serve(listener, application)
                .with_graceful_shutdown(async move {
                    graceful_cancellation.cancelled().await;
                })
                .into_future();
            tokio::pin!(served);
            tokio::pin!(shutdown);
            tokio::select! {
                result = &mut served => {
                    (Some(result), tokio::time::Instant::now() + SERVER_SHUTDOWN)
                }
                () = &mut shutdown => {
                    // Stop admitting response producers and notify every existing
                    // producer before Axum begins waiting for response bodies.
                    self.runtime.producers.close();
                    cancellation.cancel();
                    let deadline = tokio::time::Instant::now() + SERVER_SHUTDOWN;
                    let graceful_deadline = cooperative_shutdown_deadline(deadline);
                    (tokio::time::timeout_at(graceful_deadline, &mut served).await.ok(), deadline)
                }
            }
        };
        self.runtime.shutdown_until(deadline).await;
        match served {
            Some(Ok(())) | None => Ok(()),
            Some(Err(error)) => Err(Failure::one(diagnostic(
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
    project: &Project,
    options: &DevServerOptions,
) -> Result<(Option<BuildResult>, PathBuf), Failure> {
    let build = if options.build {
        Some(
            WebCompiler::build_project_async(
                project,
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
        || project.root().join(&options.output_directory),
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

fn validate_dev_exposure(options: &DevServerOptions) -> Result<(), Failure> {
    if options.host.is_loopback() || options.allow_non_loopback {
        return Ok(());
    }
    Err(Failure::one(diagnostic(
        1301,
        format!(
            "Refusing to expose the development server on {} without explicit permission.",
            options.host
        ),
        Some(String::from(
            "Use a loopback address or pass --allow-non-loopback.",
        )),
        None,
    )))
}

async fn discover_dispatch_routes(
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
    let validator = crate::handler::ChexValidator::from_environment();
    let startup_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    for route in project.route_graph().routes() {
        let request_schemas = Arc::new(
            stage_request_contracts(route.contract(), &validator, startup_deadline).await?,
        );
        for handler in route.handlers() {
            let source_path = Path::new(handler.source_path());
            let streaming = std::str::from_utf8(handler.source().source_bytes())
                .map(|contents| crate::stereotype::streaming_methods(source_path, contents))
                .unwrap_or_default();
            routes.push(DispatchRoute {
                route: String::from(route.route()),
                handler: handler.source().clone(),
                contract: route.contract().cloned().map(Arc::new),
                request_schemas: Arc::clone(&request_schemas),
                streaming: Arc::new(streaming),
            });
        }
    }
    Ok((routes, page_routes))
}

#[derive(Clone, Debug, Default)]
struct StagedRequest {
    headers: Option<crate::handler::ChexSchema>,
    parameters: Option<crate::handler::ChexSchema>,
    body: Option<crate::handler::ChexSchema>,
}

async fn stage_request_contracts(
    contract: Option<&crate::handler::RouteContract>,
    validator: &crate::handler::ChexValidator,
    deadline: tokio::time::Instant,
) -> Result<std::collections::BTreeMap<String, StagedRequest>, Failure> {
    let mut staged = std::collections::BTreeMap::new();
    let Some(contract) = contract else {
        return Ok(staged);
    };
    for (name, method) in &contract.methods {
        let Some(request) = &method.request else {
            continue;
        };
        let stage = |schema: &Option<serde_json::Value>| {
            schema
                .as_ref()
                .map(|schema| crate::handler::ChexSchema::stage(validator, schema))
                .transpose()
        };
        let request = StagedRequest {
            headers: stage(&request.headers)?,
            parameters: stage(&request.parameters)?,
            body: stage(&request.body)?,
        };
        for schema in [&request.headers, &request.parameters, &request.body]
            .into_iter()
            .flatten()
        {
            schema.preflight(deadline).await?;
        }
        staged.insert(name.clone(), request);
    }
    Ok(staged)
}

async fn enforce_request_contract(
    entry: &DispatchRoute,
    method: HttpMethod,
    parameters: &std::collections::BTreeMap<String, String>,
    headers: &axum::http::HeaderMap,
    body: &[u8],
) -> Option<Response<Body>> {
    let name = format!("{method:?}").to_ascii_uppercase();
    let staged = entry.request_schemas.get(&name).or_else(|| {
        (method == HttpMethod::Head)
            .then(|| entry.request_schemas.get("GET"))
            .flatten()
    })?;
    let deadline = tokio::time::Instant::now() + crate::handler::VALIDATION_TIMEOUT;
    if let Some(schema) = &staged.parameters {
        let bytes = serde_json::to_vec(parameters).unwrap_or_default();
        if let Some(response) =
            schema_response(&schema.validate_until(&bytes, deadline).await, "parameters")
        {
            return Some(response);
        }
    }
    if let Some(schema) = &staged.headers {
        let mut offered = std::collections::BTreeMap::new();
        for name in schema.field_names() {
            let values = headers.get_all(name.as_str());
            if values.iter().count() > 1 {
                return schema_response(&Ok(crate::handler::ChexVerdict::Invalid), "headers");
            }
            if let Some(value) = values.iter().next().and_then(|value| value.to_str().ok()) {
                offered.insert(name, value);
            }
        }
        let bytes = serde_json::to_vec(&offered).unwrap_or_default();
        if let Some(response) =
            schema_response(&schema.validate_until(&bytes, deadline).await, "headers")
        {
            return Some(response);
        }
    }
    if let Some(schema) = &staged.body {
        return schema_response(&schema.validate_until(body, deadline).await, "body");
    }
    None
}

fn schema_response(
    result: &Result<crate::handler::ChexVerdict, Failure>,
    part: &str,
) -> Option<Response<Body>> {
    match result {
        Ok(crate::handler::ChexVerdict::Valid) => None,
        Ok(crate::handler::ChexVerdict::Invalid) => Some(
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(CONTENT_TYPE, "application/json; charset=utf-8")
                .body(Body::from(serde_json::json!({"error":"The request does not satisfy its declared schema.","part":part}).to_string()))
                .unwrap_or_else(|_| text_response(StatusCode::BAD_REQUEST, "Invalid request.")),
        ),
        Err(_) => Some(text_response(StatusCode::SERVICE_UNAVAILABLE, "The declared request schema cannot be validated.")),
    }
}

fn contract_response(contract: &crate::handler::RouteContract) -> Response<Body> {
    let allow = contract
        .methods()
        .iter()
        .map(|method| format!("{method:?}").to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(", ");
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .header("allow", allow)
        .body(Body::from(serde_json::to_vec(contract).unwrap_or_default()))
        .unwrap_or_else(|_| {
            text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot serve the route contract.",
            )
        })
}

/// Serves one generator-backed handler as server-sent events.
fn stream_handler_events(
    state: &Dispatch,
    entry: &DispatchRoute,
    request: HandlerRequest,
) -> Response<Body> {
    // Two 256 KiB events is the complete per-request delivery queue budget;
    // the protocol reader blocks here when a subscriber is slow.
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<crate::handler::HandlerEvent>(2);
    let (completion_sender, mut completion_receiver) = tokio::sync::oneshot::channel::<bool>();
    let supervisor = Arc::clone(&state.supervisor);
    let handler = entry.handler.clone();
    let runtime_family = source_runtime_family(&handler);
    let request_id = request.request_id.clone();
    let log_request_id = request_id.clone();
    let cancellation = state.cancellation.clone();
    let producer_cancellation = cancellation.clone();
    let producer = async move {
        let handler_cancellation = HandlerCancellation::default();
        let invocation = supervisor.invoke_streaming_cancellable(
            &handler,
            &request,
            sender,
            &handler_cancellation,
        );
        tokio::pin!(invocation);
        let failure = tokio::select! {
            result = &mut invocation => result.err(),
            () = producer_cancellation.cancelled() => {
                handler_cancellation.cancel();
                // Do not poll a cancellation-uncooperative/infinite producer
                // again on the server runtime thread. Dropping the supervised
                // invocation activates its process-group kill-on-drop guard;
                // runtime shutdown then joins this producer task before it
                // returns.
                None
            }
        };
        if let Some(failure) = &failure
            && !producer_cancellation.is_cancelled()
        {
            log_invocation_failure("handler.stream", &log_request_id, runtime_family, failure);
        }
        let _sent = completion_sender.send(failure.is_none());
    };
    let _accepted = state.producers.spawn(producer);

    let (mut writer, reader) = tokio::io::duplex(SSE_BRIDGE_BYTES);
    let stream_request_id = request_id.clone();
    let bridge = async move {
        loop {
            tokio::select! {
                biased;
                () = cancellation.cancelled() => return,
                completion = &mut completion_receiver => {
                    // Completion is signalled only after the protocol reader
                    // has stopped sending. Drain the bounded data queue to EOF
                    // before closing a successful stream or appending its one
                    // terminal error; completion must never overtake an event
                    // that was already admitted.
                    while let Some(event) = receiver.recv().await {
                        if !write_stream_bytes(
                            &mut writer,
                            format!("data: {}\n\n", event.data).as_bytes(),
                            &cancellation,
                        ).await {
                            return;
                        }
                    }
                    if !matches!(completion, Ok(true)) {
                        let _written = write_stream_error(
                            &mut writer,
                            &stream_request_id,
                            &cancellation,
                        ).await;
                    }
                    break;
                }
                event = receiver.recv() => if let Some(event) = event {
                    if !write_stream_bytes(
                        &mut writer,
                        format!("data: {}\n\n", event.data).as_bytes(),
                        &cancellation,
                    ).await {
                        break;
                    }
                } else {
                    if !matches!(completion_receiver.await, Ok(true)) {
                        let _written = write_stream_error(
                            &mut writer,
                            &stream_request_id,
                            &cancellation,
                        ).await;
                    }
                    break;
                },
            }
        }
    };
    let _accepted = state.producers.spawn(bridge);

    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/event-stream; charset=utf-8")
        .header(CACHE_CONTROL, "no-store")
        .header(CONNECTION, "keep-alive")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(ReaderStream::new(reader)))
        .unwrap_or_else(|_| {
            text_response(StatusCode::INTERNAL_SERVER_ERROR, "Cannot open the stream.")
        });
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-tachyon-request-id"), value);
    }
    response
}

async fn write_stream_error(
    writer: &mut tokio::io::DuplexStream,
    request_id: &str,
    cancellation: &CancellationToken,
) -> bool {
    let data = serde_json::json!({
        "code": "TY2107",
        "message": "The handler stream ended unexpectedly.",
        "request_id": request_id,
    });
    write_stream_bytes(
        writer,
        format!("event: error\ndata: {data}\n\n").as_bytes(),
        cancellation,
    )
    .await
}

async fn write_stream_bytes(
    writer: &mut tokio::io::DuplexStream,
    bytes: &[u8],
    cancellation: &CancellationToken,
) -> bool {
    tokio::select! {
        biased;
        () = cancellation.cancelled() => false,
        result = writer.write_all(bytes) => result.is_ok(),
    }
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

fn live_script_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/javascript; charset=utf-8")
        .body(Body::from(LIVE_RELOAD_CLIENT))
        .unwrap_or_else(|_| {
            text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot serve the client.",
            )
        })
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
        return live_script_response();
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
    if method == HttpMethod::Options
        && let Some(contract) = &entry.contract
    {
        return contract_response(contract);
    }

    let (parts, body) = request.into_parts();

    let Ok(collected) = axum::body::to_bytes(body, MAX_REQUEST_BODY_BYTES).await else {
        return text_response(StatusCode::PAYLOAD_TOO_LARGE, "Request body is too large.");
    };

    let validation = enforce_request_contract(
        entry,
        method,
        &matched.parameters,
        &parts.headers,
        &collected,
    );
    let rejection = tokio::select! {
        response = validation => response,
        () = state.cancellation.cancelled() => Some(text_response(StatusCode::SERVICE_UNAVAILABLE, "The server is stopping.")),
    };
    if let Some(response) = rejection {
        return response;
    }

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

    if entry
        .streaming
        .contains(&format!("{method:?}").to_ascii_uppercase())
    {
        return stream_handler_events(state, entry, protocol_request);
    }

    dispatch_handler_response(state, &entry.handler, &protocol_request).await
}

async fn dispatch_handler_response(
    state: &Dispatch,
    handler: &HandlerSource,
    protocol_request: &HandlerRequest,
) -> Response<Body> {
    match invoke_request_handler(state, handler, protocol_request).await {
        Ok(response) => handler_response(response),
        // A handler failure is an application fault, never a crash of the
        // server. Internal process diagnostics may contain authored stdout or
        // stderr, so only a correlation identifier crosses the HTTP boundary.
        Err(failure) => {
            eprintln!(
                "{}",
                invocation_failure_event(
                    "handler",
                    &protocol_request.request_id,
                    source_runtime_family(handler),
                    &failure,
                )
            );
            invocation_failure_response("Handler execution", &protocol_request.request_id)
        }
    }
}

async fn invoke_request_handler(
    state: &Dispatch,
    source: &HandlerSource,
    request: &HandlerRequest,
) -> Result<HandlerResponse, Failure> {
    let cancellation = HandlerCancellation::default();
    let invocation = state.supervisor.invoke(source, request, &cancellation);
    tokio::pin!(invocation);
    tokio::select! {
        result = &mut invocation => result,
        () = state.cancellation.cancelled() => {
            cancellation.cancel();
            invocation.await
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
    let cancellation = state.cancellation.clone();
    let producer = async move {
        if !write_stream_bytes(&mut writer, b": connected\n\n", &cancellation).await {
            return;
        }
        if reconnect_sequence.is_some_and(|sequence| sequence < hub.sequence())
            && let Some(frame) = hub.reload_snapshot()
            && !write_stream_bytes(&mut writer, frame.as_bytes(), &cancellation).await
        {
            return;
        }
        loop {
            let update = tokio::select! {
                biased;
                () = cancellation.cancelled() => return,
                update = receiver.recv() => update,
            };
            match update {
                Ok(frame) => {
                    if !write_stream_bytes(&mut writer, frame.as_bytes(), &cancellation).await {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let Some(frame) = hub.reload_snapshot() else {
                        continue;
                    };
                    if !write_stream_bytes(&mut writer, frame.as_bytes(), &cancellation).await {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    };
    let _accepted = state.producers.spawn(producer);

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

    match invoke_request_handler(state, middleware, &protocol_request).await {
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
                invocation_failure_event(
                    "middleware",
                    &protocol_request.request_id,
                    source_runtime_family(middleware),
                    &failure,
                )
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
    workers: &[crate::project::ScheduledWorker],
    supervisor: &Arc<HandlerSupervisor>,
    cancellation: &CancellationToken,
    tasks: &mut JoinSet<()>,
) {
    for worker in workers {
        let relative = worker.relative().to_owned();
        tasks.spawn(run_worker(
            Arc::clone(supervisor),
            worker.source().clone(),
            relative,
            Duration::from_secs(worker.every_seconds()),
            cancellation.clone(),
        ));
    }
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
    runtime_cancellation: CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            () = runtime_cancellation.cancelled() => return,
            () = tokio::time::sleep(interval) => {}
        }
        if runtime_cancellation.is_cancelled() {
            return;
        }
        let mut request = HandlerRequest::route(
            crate::ttid::generate(),
            format!("/{relative}"),
            HttpMethod::Post,
        );
        request.operation = String::from("worker.run");
        let cancellation = HandlerCancellation::default();
        let invocation = supervisor.invoke(&source, &request, &cancellation);
        tokio::pin!(invocation);
        let outcome = tokio::select! {
            biased;
            () = runtime_cancellation.cancelled() => {
                cancellation.cancel();
                tokio::time::timeout(WORKER_CANCELLATION_SETTLE, &mut invocation).await.ok()
            }
            result = &mut invocation => Some(result),
        };
        let Some(outcome) = outcome else {
            return;
        };
        match outcome {
            Ok(response) => {
                if !(200..400).contains(&response.status) {
                    eprintln!("Worker '{relative}' reported status {}", response.status);
                }
            }
            Err(failure) => eprintln!(
                "{}",
                invocation_failure_event(
                    "worker",
                    &request.request_id,
                    source_runtime_family(&source),
                    &failure,
                )
            ),
        }
        if runtime_cancellation.is_cancelled() {
            return;
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
    let requested = requested_topic_position(request);

    let (topic_state, admission) = match state.topic_hub.subscribe(topic) {
        Ok(subscription) => subscription,
        Err(message) => return text_response(StatusCode::SERVICE_UNAVAILABLE, message),
    };
    let mut receiver = topic_state.sender.subscribe();
    let (resume, replay, cursor_gap) = topic_replay_snapshot(&topic_state, requested);
    let (mut writer, reader) = tokio::io::duplex(64 * 1024);
    let cancellation = state.cancellation.clone();
    let producer = async move {
        let _admission = admission;
        if cursor_gap {
            close_stale_topic_cursor(writer, cancellation).await;
            return;
        }
        let mut next = resume;
        for (position, frame, terminal) in replay {
            if position < next && !terminal {
                continue;
            }
            if !write_stream_bytes(&mut writer, frame.as_bytes(), &cancellation).await {
                return;
            }
            if terminal {
                return;
            }
            next = position.saturating_add(1);
        }
        loop {
            let message = tokio::select! {
                biased;
                () = cancellation.cancelled() => return,
                result = receiver.recv() => result,
            };
            let (position, frame, terminal) = match message {
                Ok(frame) => frame,
                Err(broadcast::error::RecvError::Closed) => return,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let frame = topic_error_frame(
                        "TY_TOPIC_SLOW_SUBSCRIBER",
                        "Topic subscriber was too slow.",
                        "Close this subscription and reconnect without an explicit cursor.",
                    );
                    let _ = write_stream_bytes(&mut writer, frame.as_bytes(), &cancellation).await;
                    return;
                }
            };
            if position < next {
                continue;
            }
            if !write_stream_bytes(&mut writer, frame.as_bytes(), &cancellation).await {
                return;
            }
            if terminal {
                return;
            }
            next = position.saturating_add(1);
        }
    };
    let _accepted = state.producers.spawn(producer);

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

async fn close_stale_topic_cursor(
    mut writer: tokio::io::DuplexStream,
    cancellation: CancellationToken,
) {
    let frame = topic_error_frame(
        "TY_TOPIC_CURSOR_STALE",
        "Topic cursor is no longer available.",
        "Close this subscription and reconnect without an explicit cursor.",
    );
    let _ = write_stream_bytes(&mut writer, frame.as_bytes(), &cancellation).await;
}

fn requested_topic_position(request: &Request<Body>) -> Option<u64> {
    request
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
            },
            |id| Some(id.saturating_add(1)),
        )
}

fn topic_replay_snapshot(
    state: &TopicState,
    requested: Option<u64>,
) -> (u64, Vec<(u64, String, bool)>, bool) {
    let replay = state
        .replay
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let resume = requested.unwrap_or_else(|| replay.front().map_or(0, |frame| frame.0));
    let cursor_gap = requested.is_some()
        && state.replay_evicted.load(Ordering::Acquire)
        && replay
            .front()
            .is_some_and(|(oldest, _, _)| resume < *oldest);
    let selected = replay
        .iter()
        .filter(|(position, _, terminal)| *terminal || *position >= resume)
        .cloned()
        .collect();
    (resume, selected, cursor_gap)
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

    let decision = match invoke_request_handler(state, middleware, &protocol_request).await {
        Ok(decision) => decision,
        Err(failure) => {
            eprintln!(
                "{}",
                invocation_failure_event(
                    "middleware.after",
                    &protocol_request.request_id,
                    source_runtime_family(middleware),
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
    cancellation: CancellationToken,
) {
    loop {
        let event = tokio::select! {
            biased;
            () = cancellation.cancelled() => return,
            event = watcher.receive() => event,
        };
        let Some(event) = event else {
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
        tokio::select! {
            biased;
            () = cancellation.cancelled() => return,
            () = tokio::time::sleep(WATCH_DEBOUNCE) => {}
        }
        watcher.drain(&mut changes, &project_root);
        if changes.is_empty() {
            continue;
        }
        let paths = changes.paths();
        let action = changes.action();
        let project = match ProjectDiscovery::discover(&project_root) {
            Ok(project) => project,
            Err(failure) => {
                hot_updates.publish(
                    HotUpdateKind::Diagnostics,
                    None,
                    paths,
                    Vec::new(),
                    Some(failure.report()),
                );
                continue;
            }
        };
        let rebuilt = tokio::select! {
            biased;
            () = cancellation.cancelled() => return,
            rebuilt = WebCompiler::build_project_async(&project, &options) => rebuilt,
        };
        match rebuilt {
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

fn source_runtime_family(source: &HandlerSource) -> Option<&'static str> {
    YonLanguage::from_path(Path::new(source.relative_path())).map(YonLanguage::family)
}

fn log_invocation_failure(
    operation: &str,
    request_id: &str,
    runtime_family: Option<&str>,
    failure: &Failure,
) {
    eprintln!(
        "{}",
        invocation_failure_event(operation, request_id, runtime_family, failure)
    );
}

fn invocation_failure_event(
    operation: &str,
    request_id: &str,
    runtime_family: Option<&str>,
    failure: &Failure,
) -> String {
    let diagnostic_codes = failure
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.to_string())
        .collect::<Vec<_>>();
    let runtime_missing = diagnostic_codes.iter().any(|code| code == "TY2112");
    let mut event = serde_json::json!({
        "event": "handler.invocation_failed",
        "operation": operation,
        "request_id": request_id,
        "diagnostic_codes": diagnostic_codes,
    });
    if runtime_missing {
        event["runtime_family"] = serde_json::json!(runtime_family.unwrap_or("unknown"));
        event["failure_kind"] = serde_json::json!("not_found");
    }
    event.to_string()
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
        ABORT_SETTLEMENT, DevServer, DevServerOptions, MAX_TOPIC_BYTES, PreviewServer,
        PreviewServerOptions, ProducerTasks, SERVER_SHUTDOWN, ServerRuntime, TopicHub,
        close_stale_topic_cursor, invocation_failure_event, invocation_failure_response,
        open_topic_file, reap_producer_tasks, requested_topic_position, retain_topic_frame,
        serve_generated, topic_error_frame, topic_frame, topic_replay_snapshot,
    };
    use crate::failure::diagnostic;
    use crate::handler::{HandlerRuntimePrograms, HandlerSupervisor, HandlerSupervisorOptions};
    use crate::{Failure, ProjectDiscovery};
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use std::fs;
    use std::io::Write as _;
    use std::net::{IpAddr, Ipv4Addr, TcpListener as StandardTcpListener};
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use tower_http::services::ServeDir;

    fn topic_project(root: &Path) -> Arc<cap_std::fs::Dir> {
        Arc::new(
            cap_std::fs::Dir::open_ambient_dir(root, cap_std::ambient_authority())
                .expect("topic project capability"),
        )
    }

    #[tokio::test]
    async fn topic_hub_reuses_one_tailer_and_preserves_replay_order() {
        let root = tempfile::tempdir().expect("topic root");
        let directory = root.path().join(super::TOPIC_DIRECTORY);
        fs::create_dir_all(&directory).expect("topic directory");
        let log = directory.join("orders.jsonl");
        fs::write(&log, "{\"value\":1}\n{\"value\":2}\n").expect("initial log");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let producers = ProducerTasks::new();
        let hub = TopicHub::new(
            topic_project(root.path()),
            cancellation.clone(),
            producers.clone(),
        )
        .expect("topic hub");
        let (topic, first) = hub.subscribe("orders").expect("first subscriber");
        let (_same, second) = hub.subscribe("orders").expect("second subscriber");
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if topic.replay.lock().expect("replay").len() == 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("tailer read");
        {
            let replay = topic.replay.lock().expect("replay");
            assert_eq!(replay[0].0, 0);
            assert!(replay[0].1.contains("data: {\"value\":1}"));
            assert_eq!(replay[1].0, 1);
            assert!(replay[1].1.contains("data: {\"value\":2}"));
        }
        assert_eq!(producers.len(), 1, "one physical tailer per topic");
        let retained = directory.join("orders-retained.jsonl");
        fs::rename(&log, &retained).expect("retain opened inode");
        fs::write(&log, "{\"planted\":true}\n").expect("replacement log");
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            topic.replay.lock().expect("replay").len(),
            2,
            "path replacement was followed"
        );
        std::fs::OpenOptions::new()
            .append(true)
            .open(&retained)
            .expect("append log")
            .write_all(b"{\"value\":3}\n")
            .expect("append record");
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if topic.replay.lock().expect("replay").len() == 3 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("hot append");
        drop((first, second));
        cancellation.cancel();
        tokio::time::sleep(Duration::from_millis(100)).await;
        producers.reap_finished();
        assert_eq!(producers.len(), 0, "tailer settled after shutdown");
    }

    #[tokio::test]
    async fn topic_hub_enforces_per_topic_admission() {
        let root = tempfile::tempdir().expect("topic root");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let producers = ProducerTasks::new();
        let hub = TopicHub::new(topic_project(root.path()), cancellation.clone(), producers)
            .expect("topic hub");
        let mut admissions = Vec::new();
        for _ in 0..super::MAX_TOPIC_SUBSCRIBERS_PER_TOPIC {
            admissions.push(hub.subscribe("bounded").expect("within limit").1);
        }
        assert!(hub.subscribe("bounded").is_err());
        drop(admissions);
        cancellation.cancel();
    }

    #[tokio::test]
    async fn topic_hub_retires_more_than_the_active_topic_limit_sequentially() {
        let root = tempfile::tempdir().expect("topic root");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let producers = ProducerTasks::new();
        let hub = TopicHub::new(
            topic_project(root.path()),
            cancellation.clone(),
            producers.clone(),
        )
        .expect("topic hub");
        for index in 0..(super::MAX_ACTIVE_TOPICS * 3) {
            let name = format!("topic-{index}");
            let (_, admission) = hub.subscribe(&name).expect("sequential admission");
            drop(admission);
            assert!(hub.topics.lock().expect("topics").is_empty());
        }
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                producers.reap_finished();
                if producers.len() == 0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("retired tailers settled");
        assert_eq!(hub.subscribers.load(Ordering::Acquire), 0);
        cancellation.cancel();
    }

    #[tokio::test]
    async fn topic_hub_concurrent_resubscribe_keeps_the_current_generation() {
        let root = tempfile::tempdir().expect("topic root");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let producers = ProducerTasks::new();
        let hub = TopicHub::new(
            topic_project(root.path()),
            cancellation.clone(),
            producers.clone(),
        )
        .expect("topic hub");
        let (first_state, first) = hub.subscribe("race").expect("first subscriber");
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let other_hub = hub.clone();
        let subscriber = tokio::spawn(async move {
            let (state, admission) = other_hub.subscribe("race").expect("resubscriber");
            assert!(ready_tx.send(state).is_ok(), "state receiver");
            let _ = release_rx.await;
            drop(admission);
        });
        let second_state = ready_rx.await.expect("resubscriber ready");
        assert!(Arc::ptr_eq(&first_state, &second_state));
        drop(first);
        assert!(
            hub.topics
                .lock()
                .expect("topics")
                .get("race")
                .is_some_and(|state| Arc::ptr_eq(state, &second_state))
        );
        release_tx.send(()).expect("release subscriber");
        subscriber.await.expect("resubscriber task");
        assert!(hub.topics.lock().expect("topics").is_empty());

        let (replacement, replacement_admission) =
            hub.subscribe("race").expect("replacement generation");
        assert!(!Arc::ptr_eq(&first_state, &replacement));
        assert!(replacement.replay.lock().expect("replay").is_empty());
        drop(replacement_admission);
        cancellation.cancel();
    }

    #[tokio::test]
    async fn topic_hub_marks_a_slow_subscriber_lagged_without_growing_its_queue() {
        let root = tempfile::tempdir().expect("topic root");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let producers = ProducerTasks::new();
        let hub = TopicHub::new(topic_project(root.path()), cancellation.clone(), producers)
            .expect("topic hub");
        let (topic, _admission) = hub.subscribe("slow").expect("subscriber");
        let mut receiver = topic.sender.subscribe();
        for position in 0..=super::TOPIC_BROADCAST_RECORDS as u64 {
            let _ = topic.sender.send((position, String::from("frame"), false));
        }
        assert!(matches!(
            receiver.recv().await,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_))
        ));
        cancellation.cancel();
    }

    #[test]
    fn topic_open_rejects_oversize_and_non_regular_inputs() {
        let root = tempfile::tempdir().expect("topic root");
        let topics = root.path().join(super::TOPIC_DIRECTORY);
        fs::create_dir_all(&topics).expect("topics");
        let oversized = topics.join("oversized.jsonl");
        let file = fs::File::create(&oversized).expect("oversized file");
        file.set_len(MAX_TOPIC_BYTES + 1).expect("oversized length");
        let hub = TopicHub::new(
            topic_project(root.path()),
            tokio_util::sync::CancellationToken::new(),
            ProducerTasks::new(),
        )
        .expect("topic hub");
        assert!(open_topic_file(&hub.files, "oversized").is_err());
        fs::create_dir(topics.join("directory.jsonl")).expect("non-regular topic");
        assert!(open_topic_file(&hub.files, "directory").is_err());
    }

    #[test]
    fn topic_records_are_canonical_json_and_cannot_inject_sse_fields() {
        assert_eq!(
            topic_frame(7, " {\"value\":1} \r\n").expect("CRLF JSON"),
            "id: 7\ndata: {\"value\":1}\n\n"
        );
        assert_eq!(
            topic_frame(8, "{\"text\":\"event: forged\\nid: 99\\nretry: 0\"}\n")
                .expect("escaped controls remain JSON"),
            "id: 8\ndata: {\"text\":\"event: forged\\nid: 99\\nretry: 0\"}\n\n"
        );
        for invalid in [
            "\n",
            "not-json\n",
            "event: forged\n",
            "id: 99\n",
            "retry: 0\n",
            "{\"value\":1}\rforged\n",
            "{\"value\":1}",
        ] {
            assert!(topic_frame(9, invalid).is_err(), "accepted {invalid:?}");
        }
        let ordered = [
            topic_frame(0, "1\n").expect("first"),
            topic_frame(1, "2\n").expect("second"),
        ];
        assert!(ordered[0].starts_with("id: 0\n"));
        assert!(ordered[1].starts_with("id: 1\n"));

        let error = topic_error_frame(
            "TY_TOPIC_SLOW_SUBSCRIBER",
            "Topic subscriber was too slow.",
            "Reconnect without an explicit cursor.",
        );
        assert!(error.starts_with("event: topic-error\ndata: "));
        assert!(!error.starts_with("event: error\n"));
        assert!(!error.contains("TY2107"));
        let payload = error
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .expect("error payload");
        let payload: serde_json::Value = serde_json::from_str(payload).expect("canonical JSON");
        assert_eq!(payload["category"], "topic");
        assert_eq!(payload["code"], "TY_TOPIC_SLOW_SUBSCRIBER");
        assert_eq!(payload["terminal"], true);
        assert!(payload["message"].is_string());
        assert!(payload["guidance"].is_string());
    }

    #[tokio::test]
    async fn topic_hub_malformed_json_is_terminal_sanitized_and_not_replayed_as_data() {
        let root = tempfile::tempdir().expect("topic root");
        let topics = root.path().join(super::TOPIC_DIRECTORY);
        fs::create_dir_all(&topics).expect("topics");
        fs::write(
            topics.join("hostile.jsonl"),
            "event: forged\nid: 99\nretry: 0\n",
        )
        .expect("hostile topic");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let producers = ProducerTasks::new();
        let hub = TopicHub::new(
            topic_project(root.path()),
            cancellation.clone(),
            producers.clone(),
        )
        .expect("topic hub");
        let (state, admission) = hub.subscribe("hostile").expect("subscription");
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if !state.replay.lock().expect("replay").is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("terminal record");
        {
            let replay = state.replay.lock().expect("replay");
            assert_eq!(replay.len(), 1);
            assert!(replay[0].2, "record was not terminal");
            assert_eq!(
                replay[0].1,
                topic_error_frame(
                    "TY_TOPIC_INVALID_JSON",
                    "Topic record is not valid JSON.",
                    "Repair the NDJSON record before reconnecting.",
                )
            );
            assert!(!replay[0].1.contains("forged"));
        }
        drop(admission);
        cancellation.cancel();
        tokio::time::sleep(Duration::from_millis(20)).await;
        producers.reap_finished();
        assert_eq!(producers.len(), 0);
    }

    #[tokio::test]
    async fn topic_replay_floor_rejects_only_evicted_cursors_and_closes_once() {
        use tokio::io::AsyncReadExt as _;

        let root = tempfile::tempdir().expect("topic root");
        let cancellation = tokio_util::sync::CancellationToken::new();
        let hub = TopicHub::new(
            topic_project(root.path()),
            cancellation.clone(),
            ProducerTasks::new(),
        )
        .expect("topic hub");
        let (state, admission) = hub.subscribe("cursor").expect("subscription");
        for position in 0..(super::TOPIC_REPLAY_RECORDS as u64 + 3) {
            retain_topic_frame(
                &state,
                position,
                format!("id: {position}\ndata: {position}\n\n"),
                false,
            );
        }

        let explicit_stale = Request::builder()
            .uri("/.tachyon/topics/cursor?position=2")
            .body(Body::empty())
            .expect("request");
        assert_eq!(requested_topic_position(&explicit_stale), Some(2));
        assert!(topic_replay_snapshot(&state, Some(2)).2);

        let header_stale = Request::builder()
            .uri("/.tachyon/topics/cursor?position=99")
            .header("last-event-id", "1")
            .body(Body::empty())
            .expect("request");
        assert_eq!(requested_topic_position(&header_stale), Some(2));
        assert!(topic_replay_snapshot(&state, Some(2)).2);

        let oldest = 3;
        let (covered_resume, covered, cursor_gap) = topic_replay_snapshot(&state, Some(oldest));
        assert_eq!(covered_resume, oldest);
        assert!(!cursor_gap);
        assert_eq!(covered.first().expect("oldest").0, oldest);
        assert!(covered.windows(2).all(|pair| pair[0].0 + 1 == pair[1].0));

        let last = super::TOPIC_REPLAY_RECORDS as u64 + 2;
        let current_request = Request::builder()
            .uri(format!("/.tachyon/topics/cursor?position={last}"))
            .body(Body::empty())
            .expect("current request");
        assert_eq!(requested_topic_position(&current_request), Some(last));
        let (_, current, cursor_gap) = topic_replay_snapshot(&state, Some(last));
        assert!(!cursor_gap);
        assert_eq!(
            current.iter().map(|frame| frame.0).collect::<Vec<_>>(),
            [last]
        );
        let future_position = last + 10;
        let future_request = Request::builder()
            .uri(format!(
                "/.tachyon/topics/cursor?position={future_position}"
            ))
            .body(Body::empty())
            .expect("future request");
        assert_eq!(
            requested_topic_position(&future_request),
            Some(future_position)
        );
        let (_, future, cursor_gap) = topic_replay_snapshot(&state, Some(future_position));
        assert!(!cursor_gap);
        assert!(future.is_empty());

        let last_event = Request::builder()
            .uri("/.tachyon/topics/cursor")
            .header("last-event-id", "2")
            .body(Body::empty())
            .expect("request");
        assert_eq!(requested_topic_position(&last_event), Some(oldest));

        let cursorless = Request::builder()
            .uri("/.tachyon/topics/cursor")
            .body(Body::empty())
            .expect("cursorless request");
        assert_eq!(requested_topic_position(&cursorless), None);
        let (floor, cursorless_replay, cursor_gap) = topic_replay_snapshot(&state, None);
        assert_eq!(floor, oldest);
        assert!(!cursor_gap);
        assert_eq!(cursorless_replay.first().expect("floor").0, oldest);

        let (writer, mut reader) = tokio::io::duplex(1024);
        close_stale_topic_cursor(writer, cancellation.clone()).await;
        let mut body = String::new();
        reader.read_to_string(&mut body).await.expect("closed body");
        assert_eq!(
            body,
            topic_error_frame(
                "TY_TOPIC_CURSOR_STALE",
                "Topic cursor is no longer available.",
                "Close this subscription and reconnect without an explicit cursor.",
            )
        );
        assert_eq!(body.matches("event: topic-error").count(), 1);
        assert_eq!(state.subscribers.load(Ordering::Acquire), 1);
        drop(admission);
        cancellation.cancel();
    }

    #[cfg(unix)]
    #[test]
    fn topic_open_never_follows_a_symlink() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().expect("topic root");
        let outside = root.path().join("outside.jsonl");
        fs::write(&outside, "{\"canary\":true}\n").expect("outside");
        let topics = root.path().join(super::TOPIC_DIRECTORY);
        fs::create_dir_all(&topics).expect("topics");
        let link = topics.join("topic.jsonl");
        symlink(&outside, &link).expect("topic symlink");
        let hub = TopicHub::new(
            topic_project(root.path()),
            tokio_util::sync::CancellationToken::new(),
            ProducerTasks::new(),
        )
        .expect("topic hub");
        assert!(open_topic_file(&hub.files, "topic").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn topic_capabilities_reject_ancestor_symlinks_and_ignore_root_swaps() {
        use std::io::Read as _;
        use std::os::unix::fs::symlink;

        let outside = tempfile::tempdir().expect("outside");
        fs::write(outside.path().join("secret.jsonl"), "{\"sentinel\":true}\n").expect("sentinel");

        let linked_tachyon = tempfile::tempdir().expect("linked project");
        symlink(outside.path(), linked_tachyon.path().join(".tachyon")).expect("tachyon link");
        let error = TopicHub::new(
            topic_project(linked_tachyon.path()),
            tokio_util::sync::CancellationToken::new(),
            ProducerTasks::new(),
        )
        .err()
        .expect("ancestor symlink rejected");
        assert!(!error.contains("sentinel"));

        let root_parent = tempfile::tempdir().expect("root parent");
        let root = root_parent.path().join("project");
        let topics = root.join(super::TOPIC_DIRECTORY);
        fs::create_dir_all(&topics).expect("topics");
        fs::write(topics.join("orders.jsonl"), "{\"owned\":true}\n").expect("owned");
        let hub = TopicHub::new(
            topic_project(&root),
            tokio_util::sync::CancellationToken::new(),
            ProducerTasks::new(),
        )
        .expect("retained topics capability");
        let retained_root = root_parent.path().join("retained");
        fs::rename(&root, &retained_root).expect("root swap");
        fs::create_dir_all(root.join(".tachyon")).expect("planted tachyon");
        symlink(outside.path(), root.join(super::TOPIC_DIRECTORY)).expect("planted topics");
        let mut opened = open_topic_file(&hub.files, "orders")
            .expect("capability open")
            .expect("owned topic");
        let mut content = String::new();
        opened.read_to_string(&mut content).expect("read owned");
        assert_eq!(content, "{\"owned\":true}\n");
        assert!(!content.contains("sentinel"));

        let missing_parent = tempfile::tempdir().expect("missing parent");
        let missing_root = missing_parent.path().join("project");
        fs::create_dir(&missing_root).expect("missing root");
        let missing_hub = TopicHub::new(
            topic_project(&missing_root),
            tokio_util::sync::CancellationToken::new(),
            ProducerTasks::new(),
        )
        .expect("missing topics allowed");
        fs::rename(&missing_root, missing_parent.path().join("retained")).expect("swap missing");
        fs::create_dir_all(missing_root.join(".tachyon")).expect("planted ancestor");
        symlink(outside.path(), missing_root.join(super::TOPIC_DIRECTORY))
            .expect("planted missing topics");
        assert!(
            open_topic_file(&missing_hub.files, "secret")
                .expect("safe missing")
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_background_task_panic_is_contained_by_the_owned_runtime() {
        let mut runtime = ServerRuntime {
            cancellation: tokio_util::sync::CancellationToken::new(),
            tasks: tokio::task::JoinSet::new(),
            producers: ProducerTasks::new(),
            plan: None,
        };
        runtime.tasks.spawn(async {
            panic!("background panic canary");
        });
        tokio::task::yield_now().await;
        runtime
            .shutdown_until(tokio::time::Instant::now() + SERVER_SHUTDOWN)
            .await;
        assert!(runtime.tasks.is_empty());
    }

    #[tokio::test]
    async fn abort_settlement_drops_both_task_registries_before_shutdown_returns() {
        struct DropCanary(Arc<AtomicBool>);
        impl Drop for DropCanary {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        async fn ignore_cancellation(canary: DropCanary) {
            let _held = &canary;
            std::future::pending::<()>().await;
        }

        let background_dropped = Arc::new(AtomicBool::new(false));
        let producer_dropped = Arc::new(AtomicBool::new(false));
        let mut runtime = ServerRuntime {
            cancellation: tokio_util::sync::CancellationToken::new(),
            tasks: tokio::task::JoinSet::new(),
            producers: ProducerTasks::new(),
            plan: None,
        };
        runtime
            .tasks
            .spawn(ignore_cancellation(DropCanary(Arc::clone(
                &background_dropped,
            ))));
        assert!(
            runtime
                .producers
                .spawn(ignore_cancellation(DropCanary(Arc::clone(
                    &producer_dropped
                ),)))
        );
        tokio::task::yield_now().await;

        // This models Axum consuming the entire cooperative phase: only the
        // reserved abort-settlement slice remains when runtime cleanup starts.
        let started = Instant::now();
        runtime
            .shutdown_until(tokio::time::Instant::now() + ABORT_SETTLEMENT)
            .await;
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(background_dropped.load(Ordering::SeqCst));
        assert!(producer_dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn draining_many_ready_tasks_yields_to_the_current_thread_timer() {
        let yielded = Arc::new(AtomicBool::new(false));
        let marker = Arc::clone(&yielded);
        let marker_task = tokio::spawn(async move {
            tokio::task::yield_now().await;
            marker.store(true, Ordering::SeqCst);
        });
        let mut runtime = ServerRuntime {
            cancellation: tokio_util::sync::CancellationToken::new(),
            tasks: tokio::task::JoinSet::new(),
            producers: ProducerTasks::new(),
            plan: None,
        };
        for _ in 0..1_024 {
            runtime.tasks.spawn(async {});
            assert!(runtime.producers.spawn(async {}));
        }
        tokio::task::yield_now().await;
        runtime
            .shutdown_until(tokio::time::Instant::now() + SERVER_SHUTDOWN)
            .await;
        marker_task.await.expect("marker task");
        assert!(yielded.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn completed_producer_records_are_reaped_while_admission_remains_open() {
        let producers = ProducerTasks::new();
        let cancellation = tokio_util::sync::CancellationToken::new();
        let reaper = tokio::spawn(reap_producer_tasks(producers.clone(), cancellation.clone()));
        for _ in 0..512 {
            assert!(producers.spawn(async {}));
        }
        assert!(producers.spawn(async { panic!("producer panic canary") }));
        tokio::time::timeout(Duration::from_secs(1), async {
            while producers.len() != 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("completed response tasks are continuously reaped");
        assert!(producers.spawn(async {}), "admission remains open");
        tokio::time::timeout(Duration::from_secs(1), async {
            while producers.len() != 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("post-baseline task is reaped");
        cancellation.cancel();
        reaper.await.expect("producer reaper settles");
    }

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
            let event =
                invocation_failure_event(operation, request_id, Some("javascript"), &failure);
            assert!(event.contains(r#""event":"handler.invocation_failed""#));
            assert!(event.contains(&format!(r#""operation":"{operation}""#)));
            assert!(event.contains(r#""request_id":"0ABC123DEFG""#));
            assert!(event.contains(r#""diagnostic_codes":["TY2101"]"#));
            assert!(!event.contains("runtime_family"));
            assert!(!event.contains("failure_kind"));
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

    #[test]
    fn missing_runtime_events_are_typed_and_never_disclose_runtime_paths() {
        let failure = Failure::one(diagnostic(
            2112,
            "Required configured Yon JavaScript runtime was not found.",
            Some(String::from(
                "Correct YON_JAVASCRIPT_RUNTIME or --javascript-runtime.",
            )),
            None,
        ));
        let event =
            invocation_failure_event("handler", "0ABC123DEFG", Some("javascript"), &failure);
        assert!(event.contains(r#""diagnostic_codes":["TY2112"]"#));
        assert!(event.contains(r#""runtime_family":"javascript""#));
        assert!(event.contains(r#""failure_kind":"not_found""#));
        assert!(!event.contains("YON_JAVASCRIPT_RUNTIME"));
        assert!(!event.contains("configured Yon"));
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
    async fn handler_readiness_precedes_output_and_binding_but_static_projects_need_no_runtime() {
        let missing_runtime = "/private/credentials/serve-runtime-secret-canary";
        let supervisor = Arc::new(
            HandlerSupervisor::new(HandlerSupervisorOptions {
                runtimes: HandlerRuntimePrograms {
                    javascript: PathBuf::from(missing_runtime),
                    ..HandlerRuntimePrograms::default()
                },
                ..HandlerSupervisorOptions::default()
            })
            .expect("supervisor"),
        );

        let dynamic = tempfile::tempdir().expect("dynamic project");
        let route = dynamic.path().join("server/routes/yon.js");
        fs::create_dir_all(route.parent().expect("route parent")).expect("route directory");
        fs::write(
            route,
            "@Controller\nexport class RootController { static GET() { return {}; } }",
        )
        .expect("route");
        let dynamic = ProjectDiscovery::discover(dynamic.path()).expect("dynamic discovery");
        let failure = DevServer::bind_project_with_supervisor(
            &dynamic,
            &DevServerOptions {
                port: 0,
                build: false,
                watch: false,
                ..DevServerOptions::default()
            },
            Arc::clone(&supervisor),
        )
        .await
        .expect_err("readiness fails before missing output is inspected");
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2112"), "{rendered}");
        assert!(!rendered.contains("TY1304"), "{rendered}");
        assert!(!rendered.contains(missing_runtime), "{rendered}");

        let static_project = tempfile::tempdir().expect("static project");
        fs::create_dir_all(static_project.path().join("client/pages")).expect("pages");
        fs::write(
            static_project.path().join("client/pages/tac.html"),
            "<main>Static</main>",
        )
        .expect("static source");
        fs::create_dir_all(static_project.path().join("dist")).expect("dist");
        fs::write(
            static_project.path().join("dist/index.html"),
            "<main>Static</main>",
        )
        .expect("static output");
        let static_project =
            ProjectDiscovery::discover(static_project.path()).expect("static discovery");
        let server = DevServer::bind_project_with_supervisor(
            &static_project,
            &DevServerOptions {
                port: 0,
                build: false,
                watch: false,
                ..DevServerOptions::default()
            },
            supervisor,
        )
        .await
        .expect("static project starts without a Yon runtime");
        drop(server);
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
