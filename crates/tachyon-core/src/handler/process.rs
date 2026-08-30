use super::frame::{
    FRAME_PREFIX_BYTES, MAX_FRAME_BYTES, cancel_frame, protocol_failure, request_frame,
    response_frame,
};
use super::isolation::{apply_backend_environment, validate_backend_language};
use super::readiness::{RuntimeRequirements, YonLanguage, probe_all, readiness_failure};
use super::{HandlerLanguage, HandlerSource, YonIsolationPolicy};
use crate::Failure;
use crate::failure::diagnostic;
use command_group::{AsyncCommandGroup, AsyncGroupChild};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tachyon_contracts::{HandlerRequest, HandlerResponse};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until, timeout_at};
use tokio_util::sync::CancellationToken;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TIMEOUT: Duration = Duration::from_mins(5);
const DEFAULT_CANCEL_GRACE: Duration = Duration::from_millis(100);
const PROCESS_REAP_TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_STDERR_LIMIT: usize = 64 * 1024;
const DEFAULT_MAX_CONCURRENCY: usize = 16;
const JAVASCRIPT_RUNNER: &str = include_str!("adapters/javascript_runner.mjs");
const PYTHON_RUNNER: &str = include_str!("adapters/python_runner.py");

/// Explicit executable names or paths for Phase 2 language runtimes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerRuntimePrograms {
    /// Decorator-capable JavaScript executable used by `javascript.v1`.
    pub javascript: PathBuf,
    /// `CPython` executable used by `python.v1`.
    pub python: PathBuf,
}

impl Default for HandlerRuntimePrograms {
    fn default() -> Self {
        Self::from_lookup(|name| std::env::var_os(name))
    }
}

impl HandlerRuntimePrograms {
    pub(crate) fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<std::ffi::OsString>,
    {
        Self {
            javascript: runtime(&lookup, "YON_JAVASCRIPT_RUNTIME", "bun"),
            python: runtime(
                &lookup,
                "YON_PYTHON_RUNTIME",
                if cfg!(windows) { "python" } else { "python3" },
            ),
        }
    }
}

fn runtime<F>(lookup: &F, name: &str, fallback: &str) -> PathBuf
where
    F: Fn(&str) -> Option<std::ffi::OsString>,
{
    lookup(name)
        .filter(|value| !value.is_empty())
        .map_or_else(|| PathBuf::from(fallback), PathBuf::from)
}

/// Deny-by-default child-process environment inheritance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EnvironmentPolicy {
    allowed: BTreeSet<String>,
}

impl EnvironmentPolicy {
    /// Creates an allowlist from environment variable names.
    ///
    /// # Errors
    ///
    /// Returns a stable diagnostic when a name is empty, platform-ambiguous, or
    /// contains characters outside portable environment identifiers.
    pub fn from_names<I, S>(names: I) -> Result<Self, Failure>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut policy = Self::default();
        for name in names {
            policy.allow(name)?;
        }
        Ok(policy)
    }

    /// Adds one portable environment variable name.
    ///
    /// # Errors
    ///
    /// Returns `TY2006` when the name is invalid.
    pub fn allow(&mut self, name: impl Into<String>) -> Result<(), Failure> {
        let name = name.into();
        if !valid_environment_name(&name) {
            return Err(Failure::one(diagnostic(
                2006,
                format!("Environment allowlist name '{name}' is invalid."),
                Some(String::from(
                    "Use ASCII letters, digits, and underscores, beginning with a letter or underscore.",
                )),
                None,
            )));
        }
        self.allowed.insert(name);
        Ok(())
    }

    fn environment_names(&self) -> impl Iterator<Item = &str> {
        baseline_environment_names()
            .iter()
            .copied()
            .chain(self.allowed.iter().map(String::as_str))
    }

    fn apply(&self, command: &mut Command) {
        command.env_clear();
        for name in self.environment_names() {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("YON_HANDLER_PROTOCOL", "1");
    }

    pub(crate) fn apply_std(&self, command: &mut std::process::Command) {
        command.env_clear();
        for name in self.environment_names() {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("YON_HANDLER_PROTOCOL", "1");
    }
}

/// Cooperative cancellation handle for one or more handler invocations.
#[derive(Clone, Debug, Default)]
pub struct HandlerCancellation {
    token: CancellationToken,
}

impl HandlerCancellation {
    /// Requests cancellation.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Returns whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

/// Resource and runtime policy for a handler supervisor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerSupervisorOptions {
    /// Deadline used when the request does not provide one.
    pub default_timeout: Duration,
    /// Grace period after a cancellation frame before forced termination.
    pub cancellation_grace: Duration,
    /// Maximum retained stderr bytes.
    pub stderr_limit: usize,
    /// Maximum concurrently running child processes.
    pub max_concurrency: usize,
    /// Explicit language runtime programs.
    pub runtimes: HandlerRuntimePrograms,
    /// Child environment allowlist.
    pub environment: EnvironmentPolicy,
    /// Deployment-owned handler isolation backend.
    pub isolation: YonIsolationPolicy,
}

impl Default for HandlerSupervisorOptions {
    fn default() -> Self {
        Self {
            default_timeout: DEFAULT_TIMEOUT,
            cancellation_grace: DEFAULT_CANCEL_GRACE,
            stderr_limit: DEFAULT_STDERR_LIMIT,
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
            runtimes: HandlerRuntimePrograms::default(),
            environment: EnvironmentPolicy::default(),
            isolation: YonIsolationPolicy::default(),
        }
    }
}

impl HandlerSupervisorOptions {
    /// Creates the default resource policy and reads deployment isolation from
    /// the process environment.
    ///
    /// # Errors
    ///
    /// Returns `TY2010` when the environment isolation policy is invalid or
    /// incomplete.
    pub fn from_environment() -> Result<Self, Failure> {
        Ok(Self {
            isolation: YonIsolationPolicy::from_environment()?,
            ..Self::default()
        })
    }
}

/// Direct-spawn, bounded Handler Protocol v1 process supervisor.
#[derive(Clone, Debug)]
pub struct HandlerSupervisor {
    options: HandlerSupervisorOptions,
    permits: Arc<Semaphore>,
}

impl HandlerSupervisor {
    /// Creates a supervisor using the environment-selected isolation backend.
    ///
    /// # Errors
    ///
    /// Returns stable diagnostics for invalid isolation configuration or
    /// invalid resource bounds.
    pub fn from_environment() -> Result<Self, Failure> {
        Self::new(HandlerSupervisorOptions::from_environment()?)
    }

    /// Creates a validated process supervisor.
    ///
    /// # Errors
    ///
    /// Returns stable diagnostics for zero concurrency, zero stderr capacity,
    /// or deadlines outside Handler Protocol v1 limits.
    pub fn new(options: HandlerSupervisorOptions) -> Result<Self, Failure> {
        if options.max_concurrency == 0
            || options.stderr_limit == 0
            || options.default_timeout.is_zero()
            || options.default_timeout > MAX_TIMEOUT
            || options.cancellation_grace > Duration::from_secs(5)
        {
            return Err(Failure::one(diagnostic(
                2007,
                "Handler supervisor resource limits are invalid.",
                Some(String::from(
                    "Use 1..=300 seconds, positive concurrency/stderr limits, and at most 5 seconds cancellation grace.",
                )),
                None,
            )));
        }
        let permits = Arc::new(Semaphore::new(options.max_concurrency));
        Ok(Self { options, permits })
    }

    pub(crate) async fn preflight(
        &self,
        requirements: &RuntimeRequirements,
    ) -> Result<(), Failure> {
        let resolved = requirements.resolve(&self.options.runtimes, &self.options.isolation)?;
        let results = probe_all(resolved, &self.options.environment).await?;
        readiness_failure(&results).map_or(Ok(()), Err)
    }

    /// Invokes a validated handler through one supervised child process.
    ///
    /// # Errors
    ///
    /// Returns stable diagnostics for invalid requests, admission timeout,
    /// cancellation, runtime startup, process failure, bounded-output failure,
    /// or protocol violations.
    pub async fn invoke(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        cancellation: &HandlerCancellation,
    ) -> Result<HandlerResponse, Failure> {
        validate_backend_language(&self.options.isolation, source.language())?;
        let direct_protocol = source.language() == HandlerLanguage::Direct
            && self.options.isolation.uses_direct_handler_protocol();
        let frame = encode_request(direct_protocol, request)?;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let invocation_timeout = request
            .deadline_ms
            .map_or(self.options.default_timeout, Duration::from_millis)
            .min(self.options.default_timeout);
        let deadline = Instant::now() + invocation_timeout;
        let permit = tokio::select! {
            permit = Arc::clone(&self.permits).acquire_owned() => {
                match permit {
                    Ok(permit) => permit,
                    Err(_) => return Err(Failure::one(diagnostic(
                        2101,
                        "Handler supervisor stopped accepting work.",
                        Some(String::from("Create a new supervisor and retry.")),
                        None,
                    ))),
                }
            }
            () = cancellation.token.cancelled() => return Err(cancelled()),
            () = sleep_until(deadline) => return Err(timed_out()),
        };
        let result = self
            .run_process(
                source,
                request,
                &frame,
                direct_protocol,
                deadline,
                cancellation,
            )
            .await;
        drop(permit);
        result
    }

    async fn run_process(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        request_bytes: &[u8],
        direct_protocol: bool,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<HandlerResponse, Failure> {
        let adapter = materialize_adapter(source.language())?;
        let mut planned = self.command(source, &adapter);
        self.options.environment.apply(&mut planned.command);
        apply_backend_environment(&self.options.isolation, &mut planned.command);
        let mut child = spawn_process(&mut planned.command, planned.runtime)?;
        let (mut stdin, stdout, stderr) = match take_process_pipes(&mut child) {
            Ok(pipes) => pipes,
            Err(failure) => {
                terminate(&mut child, &request.request_id).await;
                return Err(failure);
            }
        };
        let stdout_task = tokio::spawn(drain(stdout, MAX_FRAME_BYTES + FRAME_PREFIX_BYTES));
        let stderr_task = tokio::spawn(drain(stderr, self.options.stderr_limit));

        let write_failure = tokio::select! {
            result = write_request(&mut stdin, request_bytes) => result.err(),
            () = cancellation.token.cancelled() => Some(cancelled()),
            () = sleep_until(deadline) => Some(timed_out()),
        };
        if let Some(failure) = write_failure {
            terminate(&mut child, &request.request_id).await;
            let _settled = settle_tasks(stdout_task, stderr_task, deadline).await;
            return Err(failure);
        }

        // A direct handler reads until end of file, so its input must be
        // closed now. The framed adapters keep it open to receive a
        // cancellation frame.
        let mut stdin = if direct_protocol {
            drop(stdin);
            None
        } else {
            Some(stdin)
        };

        let outcome = tokio::select! {
            status = child.inner().wait() => ProcessOutcome::Exit(status),
            () = cancellation.token.cancelled() => ProcessOutcome::Cancelled,
            () = sleep_until(deadline) => ProcessOutcome::TimedOut,
        };
        match outcome {
            ProcessOutcome::Cancelled => {
                cancel_and_reap(
                    &mut child,
                    stdin.as_mut(),
                    &request.request_id,
                    self.options.cancellation_grace,
                    deadline,
                )
                .await;
                let _settled = settle_tasks(stdout_task, stderr_task, deadline).await;
                Err(cancelled())
            }
            ProcessOutcome::TimedOut => {
                cancel_and_reap(
                    &mut child,
                    stdin.as_mut(),
                    &request.request_id,
                    self.options.cancellation_grace,
                    deadline,
                )
                .await;
                let _settled = settle_tasks(stdout_task, stderr_task, deadline).await;
                Err(timed_out())
            }
            ProcessOutcome::Exit(status) => {
                drop(stdin);
                let status = match status {
                    Ok(status) => status,
                    Err(error) => {
                        terminate(&mut child, &request.request_id).await;
                        let _settled = settle_tasks(stdout_task, stderr_task, deadline).await;
                        return Err(Failure::one(diagnostic(
                            2104,
                            format!("Cannot observe handler process exit: {error}"),
                            None,
                            None,
                        )));
                    }
                };
                // A successful handler may have spawned a descendant that
                // inherited its pipes. The leader's exit is the end of the
                // invocation, so terminate the rest of its process group
                // before waiting for EOF from those pipes.
                terminate(&mut child, &request.request_id).await;
                let (stdout, stderr) = settle_tasks(stdout_task, stderr_task, deadline).await?;
                validate_process_output(status, &stdout, &stderr)?;
                surface_relay_events(&stderr.bytes, &request.request_id);
                if direct_protocol {
                    direct_response(&stdout.bytes, &request.request_id)
                } else {
                    response_frame(&stdout.bytes, &request.request_id)
                }
            }
        }
    }

    fn command(&self, source: &HandlerSource, adapter: &AdapterFiles) -> PlannedCommand {
        if let Some(policy) = self.options.isolation.firecracker() {
            let mut command = Command::new(policy.driver());
            policy.append_arguments(&mut command);
            command
                .arg("--project-root")
                .arg(source.execution_root())
                .arg("--source")
                .arg(source.relative_path())
                .arg("--adapter")
                .arg(source.language().adapter())
                .current_dir(source.execution_root())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            return PlannedCommand {
                command,
                runtime: RuntimeIdentity::Firecracker,
            };
        }
        // A direct handler is run by its Tachyon-owned runtime or compiled
        // artifact. Arbitrary interpreter and executable-file fallbacks are
        // intentionally absent.
        if source.language() == HandlerLanguage::Direct {
            let interpreter = source.interpreter();
            let execution_path = source.execution_path();
            let mut command = interpreter.first().map_or_else(
                || Command::new(&execution_path),
                |program| {
                    let mut command = Command::new(program);
                    command.args(&interpreter[1..]);
                    if !source.prebuilt() {
                        command.arg(&execution_path);
                    }
                    command
                },
            );
            command
                .current_dir(source.execution_working_directory())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            return PlannedCommand {
                command,
                runtime: RuntimeIdentity::from_source(source, &self.options.runtimes),
            };
        }
        let execution_path = source.execution_path();
        let (program, arguments): (&Path, Vec<&OsStr>) = match source.language() {
            HandlerLanguage::JavaScript => (
                &self.options.runtimes.javascript,
                vec![
                    OsStr::new("--no-warnings"),
                    adapter.runner.as_os_str(),
                    execution_path.as_os_str(),
                ],
            ),
            HandlerLanguage::TypeScript => (
                &self.options.runtimes.javascript,
                vec![adapter.runner.as_os_str(), execution_path.as_os_str()],
            ),
            HandlerLanguage::Python => (
                &self.options.runtimes.python,
                vec![
                    OsStr::new("-I"),
                    OsStr::new("-B"),
                    adapter.runner.as_os_str(),
                    execution_path.as_os_str(),
                    source.execution_working_directory().as_os_str(),
                ],
            ),
            HandlerLanguage::Direct => unreachable!("handled above"),
        };
        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(source.execution_working_directory())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        PlannedCommand {
            command,
            runtime: RuntimeIdentity::from_source(source, &self.options.runtimes),
        }
    }
}

struct PlannedCommand {
    command: Command,
    runtime: RuntimeIdentity,
}

#[derive(Clone, Copy)]
enum RuntimeIdentity {
    JavaScript { configured: bool },
    Python { configured: bool },
    Java,
    Php,
    Dotnet,
    Firecracker,
    PreparedArtifact,
}

impl RuntimeIdentity {
    fn from_source(source: &HandlerSource, runtimes: &HandlerRuntimePrograms) -> Self {
        match YonLanguage::from_path(Path::new(source.relative_path())) {
            Some(YonLanguage::JavaScript | YonLanguage::TypeScript) => Self::JavaScript {
                configured: runtimes.javascript != Path::new("bun"),
            },
            Some(YonLanguage::Python) => Self::Python {
                configured: runtimes.python
                    != Path::new(if cfg!(windows) { "python" } else { "python3" }),
            },
            Some(YonLanguage::Java | YonLanguage::Kotlin) => Self::Java,
            Some(YonLanguage::Php) => Self::Php,
            Some(YonLanguage::CSharp) => Self::Dotnet,
            Some(YonLanguage::Rust) | None => Self::PreparedArtifact,
        }
    }

    const fn is_external(self) -> bool {
        !matches!(self, Self::PreparedArtifact)
    }

    fn missing(self) -> Failure {
        let (name, help) = match self {
            Self::JavaScript { configured: false } => (
                "Yon JavaScript runtime 'bun'",
                "Install Bun or configure YON_JAVASCRIPT_RUNTIME.",
            ),
            Self::JavaScript { configured: true } => (
                "configured Yon JavaScript runtime",
                "Correct YON_JAVASCRIPT_RUNTIME or --javascript-runtime.",
            ),
            Self::Python { configured: false } => (
                if cfg!(windows) {
                    "Yon Python runtime 'python'"
                } else {
                    "Yon Python runtime 'python3'"
                },
                "Install Python or configure YON_PYTHON_RUNTIME.",
            ),
            Self::Python { configured: true } => (
                "configured Yon Python runtime",
                "Correct YON_PYTHON_RUNTIME or --python-runtime.",
            ),
            Self::Java => ("Yon Java runtime 'java'", "Install a supported JDK."),
            Self::Php => ("Yon PHP runtime 'php'", "Install PHP."),
            Self::Dotnet => (
                "Yon C# runtime 'dotnet'",
                "Install the .NET SDK and Microsoft.NETCore.App runtime.",
            ),
            Self::Firecracker => (
                "configured Firecracker driver",
                "Correct YON_FIRECRACKER_DRIVER.",
            ),
            Self::PreparedArtifact => (
                "prepared Yon handler artifact",
                "Restart Tachyon so the handler artifact can be prepared again.",
            ),
        };
        Failure::one(diagnostic(
            2112,
            format!("Required {name} was not found."),
            Some(String::from(help)),
            None,
        ))
    }
}

fn surface_relay_events(stderr: &[u8], request_id: &str) {
    for line in String::from_utf8_lossy(stderr).lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let category = event.get("category").and_then(serde_json::Value::as_str);
        if event.get("event").and_then(serde_json::Value::as_str) == Some("handler.relay_failed")
            && event.get("request_id").and_then(serde_json::Value::as_str) == Some(request_id)
            && category.is_some_and(|value| {
                matches!(
                    value,
                    "start" | "timeout" | "overflow" | "exit" | "protocol"
                )
            })
        {
            eprintln!(
                "{}",
                serde_json::json!({
                    "event": "handler.relay_failed",
                    "request_id": request_id,
                    "category": category,
                })
            );
        }
    }
}

/// The response a direct handler writes: a status, optional headers, and an
/// optional body. Everything else in the envelope is supplied by the
/// supervisor and the per-language Yon prelude.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectResponse {
    status: u16,
    #[serde(default)]
    headers: tachyon_contracts::HandlerHeaders,
    #[serde(default)]
    body: Option<String>,
}

/// Parses the plain JSON a direct handler writes to standard output.
fn direct_response(bytes: &[u8], request_id: &str) -> Result<HandlerResponse, Failure> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(protocol_failure(
            2004,
            "Direct handler wrote more than the frame limit.",
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| protocol_failure(2004, "Direct handler output is not valid UTF-8."))?;
    let parsed: DirectResponse = serde_json::from_str(text.trim()).map_err(|error| {
        protocol_failure(
            2004,
            &format!(
                "Direct handler must write one JSON object with a status: {error}. \
                 Wrote: {}",
                text.trim().chars().take(200).collect::<String>()
            ),
        )
    })?;
    if !(100..=599).contains(&parsed.status) {
        return Err(protocol_failure(
            2004,
            "Direct handler status must be a valid HTTP status.",
        ));
    }
    Ok(HandlerResponse {
        protocol_version: 1,
        kind: tachyon_contracts::HandlerResponseKind::Response,
        request_id: String::from(request_id),
        status: parsed.status,
        headers: parsed.headers,
        body: parsed.body.map(|data| tachyon_contracts::HandlerBody {
            encoding: tachyon_contracts::HandlerBodyEncoding::Utf8,
            data,
        }),
        error: None,
    })
}

/// Encodes one request for the adapter that will receive it.
///
/// A framework-owned direct runtime receives plain JSON terminated by end of
/// file; its Tachyon prelude owns translation into the authored class API.
fn encode_request(direct_protocol: bool, request: &HandlerRequest) -> Result<Vec<u8>, Failure> {
    if direct_protocol {
        return serde_json::to_vec(request).map_err(|error| {
            protocol_failure(2004, &format!("Cannot encode the request: {error}"))
        });
    }
    request_frame(request)
}

/// Writes one encoded request to a child's standard input.
async fn write_request(stdin: &mut ChildStdin, request_bytes: &[u8]) -> Result<(), Failure> {
    if let Err(error) = stdin.write_all(request_bytes).await {
        return Err(Failure::one(diagnostic(
            2101,
            format!("Cannot write Handler Protocol request: {error}"),
            Some(String::from("Retry with a healthy language runtime.")),
            None,
        )));
    }
    if let Err(error) = stdin.flush().await {
        return Err(Failure::one(diagnostic(
            2101,
            format!("Cannot flush Handler Protocol request: {error}"),
            None,
            None,
        )));
    }
    Ok(())
}

enum ProcessOutcome {
    Exit(std::io::Result<ExitStatus>),
    Cancelled,
    TimedOut,
}

struct AdapterFiles {
    _directory: tempfile::TempDir,
    runner: PathBuf,
}

fn spawn_process(
    command: &mut Command,
    runtime: RuntimeIdentity,
) -> Result<AsyncGroupChild, Failure> {
    command.group().kill_on_drop(true).spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound && runtime.is_external() {
            return runtime.missing();
        }
        Failure::one(diagnostic(
            2101,
            "Cannot start the selected handler runtime.",
            Some(String::from(
                "Check the runtime installation, permissions, and deployment policy.",
            )),
            None,
        ))
    })
}

fn take_process_pipes(
    child: &mut AsyncGroupChild,
) -> Result<(ChildStdin, ChildStdout, ChildStderr), Failure> {
    let process = child.inner();
    match (
        process.stdin.take(),
        process.stdout.take(),
        process.stderr.take(),
    ) {
        (Some(stdin), Some(stdout), Some(stderr)) => Ok((stdin, stdout, stderr)),
        (None, _, _) => Err(process_pipe_failure("stdin")),
        (_, None, _) => Err(process_pipe_failure("stdout")),
        (_, _, None) => Err(process_pipe_failure("stderr")),
    }
}

fn materialize_adapter(language: HandlerLanguage) -> Result<AdapterFiles, Failure> {
    let directory = match tempfile::Builder::new()
        .prefix("tachyon-handler-")
        .tempdir()
    {
        Ok(directory) => directory,
        Err(error) => return Err(adapter_io(&error)),
    };
    let (name, contents) = match language {
        HandlerLanguage::JavaScript | HandlerLanguage::TypeScript => {
            ("runner.mjs", JAVASCRIPT_RUNNER)
        }
        HandlerLanguage::Python => ("runner.py", PYTHON_RUNNER),
        // A direct handler needs no adapter; an unused placeholder keeps the
        // staging directory shape uniform.
        HandlerLanguage::Direct => ("direct.unused", ""),
    };
    let runner = directory.path().join(name);
    if let Err(error) = fs::write(&runner, contents) {
        return Err(adapter_io(&error));
    }
    Ok(AdapterFiles {
        _directory: directory,
        runner,
    })
}

fn adapter_io(error: &std::io::Error) -> Failure {
    Failure::one(diagnostic(
        2101,
        format!("Cannot materialize the trusted handler adapter: {error}"),
        Some(String::from(
            "Check temporary-directory permissions and available storage.",
        )),
        None,
    ))
}

#[derive(Debug)]
struct Drained {
    bytes: Vec<u8>,
    overflow: bool,
    read_error: bool,
}

async fn drain<R>(mut reader: R, limit: usize) -> Drained
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut overflow = false;
    let mut read_error = false;
    let mut chunk = [0_u8; 8 * 1024];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) => break,
            Err(_) => {
                read_error = true;
                break;
            }
            Ok(read) => {
                let available = limit.saturating_sub(bytes.len());
                let retained = available.min(read);
                bytes.extend_from_slice(&chunk[..retained]);
                overflow |= retained < read;
            }
        }
    }
    Drained {
        bytes,
        overflow,
        read_error,
    }
}

async fn settle_tasks(
    mut stdout: JoinHandle<Drained>,
    mut stderr: JoinHandle<Drained>,
    deadline: Instant,
) -> Result<(Drained, Drained), Failure> {
    let Ok(stdout_result) = timeout_at(deadline, &mut stdout).await else {
        stdout.abort();
        stderr.abort();
        let _stdout = stdout.await;
        let _stderr = stderr.await;
        return Err(timed_out());
    };
    let Ok(stdout) = stdout_result else {
        stderr.abort();
        let _stderr = stderr.await;
        return Err(protocol_failure(2101, "Handler stdout drain task failed."));
    };
    let Ok(stderr_result) = timeout_at(deadline, &mut stderr).await else {
        stderr.abort();
        let _stderr = stderr.await;
        return Err(timed_out());
    };
    let Ok(stderr) = stderr_result else {
        return Err(protocol_failure(2101, "Handler stderr drain task failed."));
    };
    Ok((stdout, stderr))
}

fn validate_process_output(
    status: ExitStatus,
    stdout: &Drained,
    stderr: &Drained,
) -> Result<(), Failure> {
    if stdout.overflow {
        return Err(protocol_failure(
            2103,
            "Handler protocol stdout exceeds the 16 MiB frame limit.",
        ));
    }
    if stdout.read_error {
        return Err(protocol_failure(
            2102,
            "Cannot read the complete handler protocol stdout stream.",
        ));
    }
    if stderr.overflow {
        return Err(Failure::one(diagnostic(
            2107,
            "Handler stderr exceeded its configured diagnostic limit.",
            Some(String::from(
                "Reduce handler logging and retry the invocation.",
            )),
            None,
        )));
    }
    if stderr.read_error {
        return Err(Failure::one(diagnostic(
            2107,
            "Cannot read the complete handler stderr stream.",
            Some(String::from("Retry with a healthy language runtime.")),
            None,
        )));
    }
    if !status.success() {
        let sideband = bounded_sideband(&stderr.bytes);
        let suffix = if sideband.is_empty() {
            String::new()
        } else {
            format!(" Stderr: {sideband}")
        };
        return Err(Failure::one(diagnostic(
            2104,
            format!("Handler process exited unsuccessfully ({status}).{suffix}"),
            Some(String::from(
                "Fix the handler crash; a later invocation will start a clean process.",
            )),
            None,
        )));
    }
    Ok(())
}

async fn cancel_and_reap(
    child: &mut AsyncGroupChild,
    stdin: Option<&mut ChildStdin>,
    request_id: &str,
    grace: Duration,
    deadline: Instant,
) {
    let grace_deadline = deadline.min(Instant::now() + grace);
    // A direct handler has no open input to cancel through; it is reaped by
    // the grace period below instead.
    if let Some(stdin) = stdin
        && let Ok(frame) = cancel_frame(request_id)
    {
        let _cancel = timeout_at(grace_deadline, async {
            let _write = stdin.write_all(&frame).await;
            let _flush = stdin.flush().await;
        })
        .await;
    }
    if Instant::now() < grace_deadline {
        let _exit = timeout_at(grace_deadline, child.inner().wait()).await;
    }
    // Always terminate the group after cooperative cancellation: the leader
    // may have exited while a descendant kept running with inherited handles.
    terminate(child, request_id).await;
}

async fn terminate(child: &mut AsyncGroupChild, request_id: &str) {
    let process_id = child.inner().id();
    let kill_failed = child.start_kill().is_err();
    // Reaping is cleanup, not handler work. Give it its own short bound even
    // when the invocation deadline has already expired; pipe settlement still
    // uses the original absolute deadline and cannot hold a permit open.
    let cleanup_deadline = Instant::now() + PROCESS_REAP_TIMEOUT;
    let wait_failed = !matches!(timeout_at(cleanup_deadline, child.wait()).await, Ok(Ok(_)));
    if wait_failed {
        eprintln!(
            "{}",
            cleanup_unsettled_event(request_id, process_id, kill_failed)
        );
    }
}

fn cleanup_unsettled_event(
    request_id: &str,
    process_id: Option<u32>,
    kill_failed: bool,
) -> serde_json::Value {
    serde_json::json!({
        "event": "handler.cleanup_unsettled",
        "request_id": request_id,
        "process_id": process_id,
        "kill_failed": kill_failed,
        "reap_failed": true,
    })
}

fn process_pipe_failure(name: &str) -> Failure {
    Failure::one(diagnostic(
        2101,
        format!("Handler runtime did not provide a piped {name} stream."),
        None,
        None,
    ))
}

fn timed_out() -> Failure {
    Failure::one(diagnostic(
        2110,
        "Handler invocation exceeded its deadline.",
        Some(String::from(
            "Reduce handler work or choose a larger bounded timeout.",
        )),
        None,
    ))
}

fn cancelled() -> Failure {
    Failure::one(diagnostic(
        2111,
        "Handler invocation was cancelled.",
        Some(String::from(
            "Retry only if the caller still needs the result.",
        )),
        None,
    ))
}

fn bounded_sideband(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(4_096)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Largest number of events one streamed request may emit.
const MAX_STREAM_EVENTS: usize = 100_000;
/// Largest decoded event admitted to the subscriber queue.
const MAX_STREAM_EVENT_BYTES: usize = 256 * 1024;
/// Aggregate decoded payload admitted from one stream before it is stopped.
const MAX_STREAM_TOTAL_BYTES: usize = 64 * 1024 * 1024;

/// One event yielded by a streaming Yon handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerEvent {
    /// The JSON text produced by the handler for this event.
    pub data: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StreamReadOutcome {
    Complete,
    SubscriberClosed,
}

enum StreamProcessOutcome {
    Frames(Result<Result<StreamReadOutcome, Failure>, tokio::task::JoinError>),
    Exit(std::io::Result<ExitStatus>),
    SubscriberClosed,
    Cancelled,
    TimedOut,
}

struct StreamingProcess<'a> {
    source: &'a HandlerSource,
    request: &'a HandlerRequest,
    request_bytes: &'a [u8],
    direct_protocol: bool,
    deadline: Instant,
    events: tokio::sync::mpsc::Sender<HandlerEvent>,
    cancellation: &'a HandlerCancellation,
}

impl HandlerSupervisor {
    /// Invokes a handler that emits length-prefixed event frames until EOF.
    ///
    /// The same admission, absolute deadline, environment, isolation, process
    /// group, and bounded-cleanup policy used by [`Self::invoke`] applies.
    ///
    /// # Errors
    ///
    /// Returns stable diagnostics for admission, startup, framing, process,
    /// output-bound, or deadline failures. Handler-provided error text is never
    /// copied into the diagnostic.
    pub async fn invoke_streaming(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        events: tokio::sync::mpsc::Sender<HandlerEvent>,
    ) -> Result<(), Failure> {
        self.invoke_streaming_cancellable(source, request, events, &HandlerCancellation::default())
            .await
    }

    pub(crate) async fn invoke_streaming_cancellable(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        events: tokio::sync::mpsc::Sender<HandlerEvent>,
        cancellation: &HandlerCancellation,
    ) -> Result<(), Failure> {
        validate_backend_language(&self.options.isolation, source.language())?;
        let direct_protocol = source.language() == HandlerLanguage::Direct
            && self.options.isolation.uses_direct_handler_protocol();
        let request_bytes = encode_request(direct_protocol, request)?;
        if cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let invocation_timeout = request
            .deadline_ms
            .map_or(self.options.default_timeout, Duration::from_millis)
            .min(self.options.default_timeout);
        let deadline = Instant::now() + invocation_timeout;
        let permit = tokio::select! {
            permit = Arc::clone(&self.permits).acquire_owned() => permit.map_err(|_| {
                Failure::one(diagnostic(
                    2101,
                    "Handler supervisor stopped accepting work.",
                    Some(String::from("Create a new supervisor and retry.")),
                    None,
                ))
            })?,
            () = cancellation.token.cancelled() => return Err(cancelled()),
            () = sleep_until(deadline) => return Err(timed_out()),
        };
        let result = self
            .run_streaming_process(StreamingProcess {
                source,
                request,
                request_bytes: &request_bytes,
                direct_protocol,
                deadline,
                events,
                cancellation,
            })
            .await;
        drop(permit);
        result
    }

    #[allow(clippy::too_many_lines)] // Process launch, framing, and bounded cleanup are one lifecycle.
    async fn run_streaming_process(&self, process: StreamingProcess<'_>) -> Result<(), Failure> {
        let StreamingProcess {
            source,
            request,
            request_bytes,
            direct_protocol,
            deadline,
            events,
            cancellation,
        } = process;
        let adapter = materialize_adapter(source.language())?;
        let mut planned = self.command(source, &adapter);
        self.options.environment.apply(&mut planned.command);
        apply_backend_environment(&self.options.isolation, &mut planned.command);
        let mut child = spawn_process(&mut planned.command, planned.runtime)?;
        let (mut stdin, stdout, stderr) = match take_process_pipes(&mut child) {
            Ok(pipes) => pipes,
            Err(failure) => {
                terminate(&mut child, &request.request_id).await;
                return Err(failure);
            }
        };
        let stderr_task = tokio::spawn(drain(stderr, self.options.stderr_limit));
        let write_result = tokio::select! {
            result = write_request(&mut stdin, request_bytes) => Some(result),
            () = cancellation.token.cancelled() => None,
            () = sleep_until(deadline) => Some(Err(timed_out())),
        };
        match write_result {
            Some(Ok(())) => {}
            Some(Err(failure)) => {
                terminate(&mut child, &request.request_id).await;
                let _stderr = settle_drain(stderr_task, deadline).await;
                return Err(failure);
            }
            None => {
                cancel_and_reap(
                    &mut child,
                    Some(&mut stdin),
                    &request.request_id,
                    self.options.cancellation_grace,
                    deadline,
                )
                .await;
                let _stderr = settle_drain(stderr_task, deadline).await;
                return Err(cancelled());
            }
        }
        let mut stdin = if direct_protocol {
            drop(stdin);
            None
        } else {
            Some(stdin)
        };
        let subscriber = events.clone();
        let mut events_task = tokio::spawn(read_events(stdout, events, request.request_id.clone()));
        let outcome = tokio::select! {
            frames = &mut events_task => StreamProcessOutcome::Frames(frames),
            status = child.inner().wait() => StreamProcessOutcome::Exit(status),
            () = subscriber.closed() => StreamProcessOutcome::SubscriberClosed,
            () = cancellation.token.cancelled() => StreamProcessOutcome::Cancelled,
            () = sleep_until(deadline) => StreamProcessOutcome::TimedOut,
        };

        match outcome {
            StreamProcessOutcome::Cancelled => {
                cancel_and_reap(
                    &mut child,
                    stdin.as_mut(),
                    &request.request_id,
                    self.options.cancellation_grace,
                    deadline,
                )
                .await;
                events_task.abort();
                let _events = events_task.await;
                let _stderr = settle_drain(stderr_task, deadline).await;
                Err(cancelled())
            }
            StreamProcessOutcome::TimedOut => {
                cancel_and_reap(
                    &mut child,
                    stdin.as_mut(),
                    &request.request_id,
                    self.options.cancellation_grace,
                    deadline,
                )
                .await;
                events_task.abort();
                let _events = events_task.await;
                let _stderr = settle_drain(stderr_task, deadline).await;
                Err(timed_out())
            }
            StreamProcessOutcome::Frames(Ok(Ok(StreamReadOutcome::SubscriberClosed))) => {
                terminate(&mut child, &request.request_id).await;
                let _stderr = settle_drain(stderr_task, deadline).await;
                Ok(())
            }
            StreamProcessOutcome::SubscriberClosed => {
                terminate(&mut child, &request.request_id).await;
                events_task.abort();
                let _events = events_task.await;
                let _stderr = settle_drain(stderr_task, deadline).await;
                Ok(())
            }
            StreamProcessOutcome::Frames(frames) => {
                finish_stream_after_frames(
                    &mut child,
                    stderr_task,
                    deadline,
                    frames,
                    &request.request_id,
                )
                .await
            }
            StreamProcessOutcome::Exit(status) => {
                drop(stdin);
                finish_stream_after_exit(
                    &mut child,
                    events_task,
                    stderr_task,
                    deadline,
                    status,
                    &request.request_id,
                )
                .await
            }
        }
    }
}

async fn finish_stream_after_frames(
    child: &mut AsyncGroupChild,
    stderr_task: JoinHandle<Drained>,
    deadline: Instant,
    frames: Result<Result<StreamReadOutcome, Failure>, tokio::task::JoinError>,
    request_id: &str,
) -> Result<(), Failure> {
    let frames = match frames {
        Ok(Ok(frames)) => frames,
        Ok(Err(failure)) => {
            terminate(child, request_id).await;
            let _stderr = settle_drain(stderr_task, deadline).await;
            return Err(failure);
        }
        Err(_) => {
            terminate(child, request_id).await;
            let _stderr = settle_drain(stderr_task, deadline).await;
            return Err(stream_failure("Handler event reader failed."));
        }
    };
    debug_assert_eq!(frames, StreamReadOutcome::Complete);
    let status = match timeout_at(deadline, child.inner().wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(_)) => {
            terminate(child, request_id).await;
            let _stderr = settle_drain(stderr_task, deadline).await;
            return Err(stream_failure("Cannot observe handler process exit."));
        }
        Err(_) => {
            terminate(child, request_id).await;
            let _stderr = settle_drain(stderr_task, deadline).await;
            return Err(timed_out());
        }
    };
    terminate(child, request_id).await;
    let stderr = settle_drain(stderr_task, deadline).await?;
    validate_stream_process_output(status, &stderr)
}

async fn finish_stream_after_exit(
    child: &mut AsyncGroupChild,
    mut events_task: JoinHandle<Result<StreamReadOutcome, Failure>>,
    stderr_task: JoinHandle<Drained>,
    deadline: Instant,
    status: std::io::Result<ExitStatus>,
    request_id: &str,
) -> Result<(), Failure> {
    let Ok(status) = status else {
        terminate(child, request_id).await;
        events_task.abort();
        let _events = events_task.await;
        let _stderr = settle_drain(stderr_task, deadline).await;
        return Err(stream_failure("Cannot observe handler process exit."));
    };
    terminate(child, request_id).await;
    let frames = match timeout_at(deadline, &mut events_task).await {
        Ok(Ok(Ok(frames))) => frames,
        Ok(Ok(Err(failure))) => {
            let _stderr = settle_drain(stderr_task, deadline).await;
            return Err(failure);
        }
        Ok(Err(_)) => {
            let _stderr = settle_drain(stderr_task, deadline).await;
            return Err(stream_failure("Handler event reader failed."));
        }
        Err(_) => {
            events_task.abort();
            let _events = events_task.await;
            let _stderr = settle_drain(stderr_task, deadline).await;
            return Err(timed_out());
        }
    };
    if frames == StreamReadOutcome::SubscriberClosed {
        let _stderr = settle_drain(stderr_task, deadline).await;
        return Ok(());
    }
    let stderr = settle_drain(stderr_task, deadline).await?;
    validate_stream_process_output(status, &stderr)
}

async fn settle_drain(
    mut task: JoinHandle<Drained>,
    deadline: Instant,
) -> Result<Drained, Failure> {
    match timeout_at(deadline, &mut task).await {
        Ok(Ok(drained)) => Ok(drained),
        Ok(Err(_)) => Err(stream_failure("Handler diagnostic reader failed.")),
        Err(_) => {
            task.abort();
            let _task = task.await;
            Err(timed_out())
        }
    }
}

fn validate_stream_process_output(status: ExitStatus, stderr: &Drained) -> Result<(), Failure> {
    let empty_stdout = Drained {
        bytes: Vec::new(),
        overflow: false,
        read_error: false,
    };
    validate_process_output(status, &empty_stdout, stderr)
}

async fn read_events(
    mut stdout: ChildStdout,
    events: tokio::sync::mpsc::Sender<HandlerEvent>,
    request_id: String,
) -> Result<StreamReadOutcome, Failure> {
    let mut sent = 0usize;
    let mut sent_bytes = 0usize;
    loop {
        let mut prefix = [0_u8; FRAME_PREFIX_BYTES];
        match stdout.read(&mut prefix[..1]).await {
            Ok(0) => return Ok(StreamReadOutcome::Complete),
            Ok(_) => {}
            Err(_) => return Err(stream_failure("Cannot read a streamed frame prefix.")),
        }
        if stdout.read_exact(&mut prefix[1..]).await.is_err() {
            return Err(stream_failure(
                "A streamed frame ended before its prefix was complete.",
            ));
        }
        let length = u32::from_be_bytes(prefix) as usize;
        if length > MAX_FRAME_BYTES {
            return Err(stream_failure(
                "A streamed frame exceeds the 16 MiB protocol limit.",
            ));
        }
        let mut payload = vec![0_u8; length];
        if stdout.read_exact(&mut payload).await.is_err() {
            return Err(stream_failure(
                "A streamed frame ended before its payload was complete.",
            ));
        }
        let frame: serde_json::Value = serde_json::from_slice(&payload)
            .map_err(|_| stream_failure("A streamed frame is not valid JSON."))?;
        if frame
            .get("protocol_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
            || frame.get("request_id").and_then(serde_json::Value::as_str)
                != Some(request_id.as_str())
        {
            return Err(stream_failure(
                "A streamed frame violates Handler Protocol v1.",
            ));
        }
        if frame.get("error").is_some_and(|error| !error.is_null()) {
            return Err(stream_failure(
                "The handler ended its stream with an error.",
            ));
        }
        if frame.get("kind").and_then(serde_json::Value::as_str) != Some("event")
            || frame
                .pointer("/body/encoding")
                .and_then(serde_json::Value::as_str)
                != Some("utf8")
        {
            return Err(stream_failure("A streamed event has an invalid envelope."));
        }
        let Some(data) = frame
            .pointer("/body/data")
            .and_then(serde_json::Value::as_str)
        else {
            return Err(stream_failure("A streamed event has no UTF-8 body."));
        };
        if data.len() > MAX_STREAM_EVENT_BYTES {
            return Err(stream_failure(
                "A streamed event exceeds the 256 KiB delivery limit.",
            ));
        }
        sent_bytes = sent_bytes.saturating_add(data.len());
        if sent_bytes > MAX_STREAM_TOTAL_BYTES {
            return Err(stream_failure(
                "A streamed handler exceeded the 64 MiB aggregate delivery limit.",
            ));
        }
        sent += 1;
        if sent > MAX_STREAM_EVENTS {
            return Err(stream_failure(
                "A streamed handler exceeded the 100,000 event limit.",
            ));
        }
        if events
            .send(HandlerEvent {
                data: String::from(data),
            })
            .await
            .is_err()
        {
            return Ok(StreamReadOutcome::SubscriberClosed);
        }
    }
}

fn stream_failure(message: &str) -> Failure {
    Failure::one(diagnostic(
        2107,
        format!("Streamed handler failed: {message}"),
        Some(String::from(
            "Fix the handler stream or its protocol adapter, then retry the request.",
        )),
        None,
    ))
}

fn valid_environment_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(windows)]
const fn baseline_environment_names() -> &'static [&'static str] {
    &[
        "PATH",
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
    ]
}

#[cfg(not(windows))]
const fn baseline_environment_names() -> &'static [&'static str] {
    &["PATH", "LANG", "LC_ALL", "TMPDIR"]
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        Drained, EnvironmentPolicy, HandlerSupervisor, HandlerSupervisorOptions, PYTHON_RUNNER,
        RuntimeIdentity, adapter_io, bounded_sideband, cleanup_unsettled_event, drain,
        process_pipe_failure, settle_tasks, spawn_process,
    };
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{AsyncRead, ReadBuf};
    use tokio::process::Command;
    use tokio::time::Instant;

    #[test]
    fn python_dispatch_requires_the_controller_decorator() {
        assert!(PYTHON_RUNNER.contains("handler = _CONTROLLER[0]"));
        assert!(!PYTHON_RUNNER.contains("getattr(module, \"Handler\""));
    }

    #[test]
    fn unsettled_cleanup_event_is_correlated_and_contains_no_process_output() {
        let event = cleanup_unsettled_event("request-42", Some(123), true);
        assert_eq!(event["request_id"], "request-42");
        assert_eq!(event["process_id"], 123);
        assert_eq!(event["event"], "handler.cleanup_unsettled");
        assert!(!event.to_string().contains("secret-canary"));
    }

    #[test]
    fn missing_external_runtimes_are_ty2112_and_spawn_details_are_redacted() {
        let canary = "tachyon-runtime-does-not-exist-secret-canary";
        let mut command = Command::new(canary);
        let Err(failure) = spawn_process(
            &mut command,
            RuntimeIdentity::JavaScript { configured: true },
        ) else {
            panic!("runtime unexpectedly started");
        };
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2112"), "{rendered}");
        assert!(!rendered.contains(canary), "{rendered}");

        let mut prepared = Command::new(canary);
        let Err(prepared_failure) = spawn_process(&mut prepared, RuntimeIdentity::PreparedArtifact)
        else {
            panic!("prepared artifact unexpectedly started");
        };
        assert!(prepared_failure.to_string().contains("TY2101"));
    }

    #[test]
    fn environment_names_and_resource_limits_fail_closed() {
        assert!(EnvironmentPolicy::from_names(["PUBLIC_VALUE", "_ALSO_OK"]).is_ok());
        for name in ["", "1BAD", "BAD-NAME", "BAD=NAME", "NAÏVE"] {
            let failure = EnvironmentPolicy::from_names([name]).expect_err("invalid name");
            assert!(failure.to_string().contains("TY2006"));
        }

        for options in [
            HandlerSupervisorOptions {
                max_concurrency: 0,
                ..HandlerSupervisorOptions::default()
            },
            HandlerSupervisorOptions {
                stderr_limit: 0,
                ..HandlerSupervisorOptions::default()
            },
            HandlerSupervisorOptions {
                default_timeout: Duration::ZERO,
                ..HandlerSupervisorOptions::default()
            },
            HandlerSupervisorOptions {
                default_timeout: Duration::from_secs(301),
                ..HandlerSupervisorOptions::default()
            },
            HandlerSupervisorOptions {
                cancellation_grace: Duration::from_secs(6),
                ..HandlerSupervisorOptions::default()
            },
        ] {
            let failure = HandlerSupervisor::new(options).expect_err("invalid limits");
            assert!(failure.to_string().contains("TY2007"));
        }
    }

    struct BrokenReader;

    impl AsyncRead for BrokenReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
            _buffer: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other("broken reader")))
        }
    }

    #[tokio::test]
    async fn drain_and_task_failures_are_bounded() {
        let drained = drain(BrokenReader, 8).await;
        assert!(drained.read_error);
        assert!(!drained.overflow);

        let failed = tokio::spawn(std::future::pending::<Drained>());
        failed.abort();
        let healthy = tokio::spawn(async {
            Drained {
                bytes: Vec::new(),
                overflow: false,
                read_error: false,
            }
        });
        let failure = settle_tasks(failed, healthy, Instant::now() + Duration::from_secs(1))
            .await
            .expect_err("join failure");
        assert!(failure.to_string().contains("TY2101"));

        let healthy = tokio::spawn(async {
            Drained {
                bytes: Vec::new(),
                overflow: false,
                read_error: false,
            }
        });
        let failed = tokio::spawn(std::future::pending::<Drained>());
        failed.abort();
        let failure = settle_tasks(healthy, failed, Instant::now() + Duration::from_secs(1))
            .await
            .expect_err("join failure");
        assert!(failure.to_string().contains("TY2101"));

        let pending_stdout = tokio::spawn(std::future::pending::<Drained>());
        let pending_stderr = tokio::spawn(std::future::pending::<Drained>());
        let failure = settle_tasks(
            pending_stdout,
            pending_stderr,
            Instant::now() + Duration::from_millis(10),
        )
        .await
        .expect_err("settlement deadline");
        assert!(failure.to_string().contains("TY2110"));
    }

    #[test]
    fn internal_process_failures_and_sideband_are_sanitized() {
        assert!(
            adapter_io(&io::Error::other("temporary"))
                .to_string()
                .contains("TY2101")
        );
        assert!(
            process_pipe_failure("stdout")
                .to_string()
                .contains("TY2101")
        );
        assert_eq!(
            bounded_sideband(b"visible\x1b[31m\nline"),
            "visible[31m\nline"
        );
    }
}
