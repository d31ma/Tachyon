use super::frame::{
    FRAME_PREFIX_BYTES, MAX_FRAME_BYTES, cancel_frame, protocol_failure, request_frame,
    response_frame,
};
use super::{HandlerLanguage, HandlerSource};
use crate::Failure;
use crate::failure::diagnostic;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tachyon_contracts::{HandlerRequest, HandlerResponse};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_util::sync::CancellationToken;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TIMEOUT: Duration = Duration::from_mins(5);
const DEFAULT_CANCEL_GRACE: Duration = Duration::from_millis(100);
const DEFAULT_STDERR_LIMIT: usize = 64 * 1024;
const DEFAULT_MAX_CONCURRENCY: usize = 16;
const JAVASCRIPT_RUNNER: &str = include_str!("adapters/javascript_runner.mjs");
const PYTHON_RUNNER: &str = include_str!("adapters/python_runner.py");

/// Explicit executable names or paths for Phase 2 language runtimes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerRuntimePrograms {
    /// Node.js executable used by `javascript.v1`.
    pub javascript: PathBuf,
    /// `CPython` executable used by `python.v1`.
    pub python: PathBuf,
}

impl Default for HandlerRuntimePrograms {
    fn default() -> Self {
        Self {
            javascript: PathBuf::from("node"),
            python: PathBuf::from(if cfg!(windows) { "python" } else { "python3" }),
        }
    }
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

    fn apply(&self, command: &mut Command) {
        command.env_clear();
        for name in baseline_environment_names()
            .iter()
            .copied()
            .chain(self.allowed.iter().map(String::as_str))
        {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command.env("TACHYON_HANDLER_PROTOCOL", "1");
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
        }
    }
}

/// Direct-spawn, bounded Handler Protocol v1 process supervisor.
#[derive(Clone, Debug)]
pub struct HandlerSupervisor {
    options: HandlerSupervisorOptions,
    permits: Arc<Semaphore>,
}

impl HandlerSupervisor {
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
        let frame = encode_request(source.language(), request)?;
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
            .run_process(source, request, &frame, deadline, cancellation)
            .await;
        drop(permit);
        result
    }

    async fn run_process(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        request_bytes: &[u8],
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<HandlerResponse, Failure> {
        let adapter = materialize_adapter(source.language())?;
        let mut command = self.command(source, &adapter);
        self.options.environment.apply(&mut command);
        let program = command
            .as_std()
            .get_program()
            .to_string_lossy()
            .into_owned();
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return Err(Failure::one(diagnostic(
                    2101,
                    format!("Cannot start handler runtime '{program}': {error}"),
                    Some(String::from(
                        "Install the selected runtime or pass its executable path explicitly.",
                    )),
                    None,
                )));
            }
        };
        let Some(mut stdin) = child.stdin.take() else {
            return Err(process_pipe_failure("stdin"));
        };
        let Some(stdout) = child.stdout.take() else {
            return Err(process_pipe_failure("stdout"));
        };
        let Some(stderr) = child.stderr.take() else {
            return Err(process_pipe_failure("stderr"));
        };
        let stdout_task = tokio::spawn(drain(stdout, MAX_FRAME_BYTES + FRAME_PREFIX_BYTES));
        let stderr_task = tokio::spawn(drain(stderr, self.options.stderr_limit));

        if let Err(failure) = write_request(&mut stdin, request_bytes).await {
            terminate(&mut child).await;
            return Err(failure);
        }

        // A direct handler reads until end of file, so its input must be
        // closed now. The framed adapters keep it open to receive a
        // cancellation frame.
        let mut stdin = if source.language() == HandlerLanguage::Direct {
            drop(stdin);
            None
        } else {
            Some(stdin)
        };

        let outcome = tokio::select! {
            status = child.wait() => ProcessOutcome::Exit(status),
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
                )
                .await;
                settle_tasks(stdout_task, stderr_task).await?;
                Err(cancelled())
            }
            ProcessOutcome::TimedOut => {
                cancel_and_reap(
                    &mut child,
                    stdin.as_mut(),
                    &request.request_id,
                    self.options.cancellation_grace,
                )
                .await;
                settle_tasks(stdout_task, stderr_task).await?;
                Err(timed_out())
            }
            ProcessOutcome::Exit(status) => {
                drop(stdin);
                let status = match status {
                    Ok(status) => status,
                    Err(error) => {
                        return Err(Failure::one(diagnostic(
                            2104,
                            format!("Cannot observe handler process exit: {error}"),
                            None,
                            None,
                        )));
                    }
                };
                let (stdout, stderr) = settle_tasks(stdout_task, stderr_task).await?;
                validate_process_output(status, &stdout, &stderr)?;
                if source.language() == HandlerLanguage::Direct {
                    direct_response(&stdout.bytes, &request.request_id)
                } else {
                    response_frame(&stdout.bytes, &request.request_id)
                }
            }
        }
    }

    fn command(&self, source: &HandlerSource, adapter: &AdapterFiles) -> Command {
        // A direct handler is run by its registered interpreter, or executed
        // itself. No adapter file exists for it, in any language.
        if source.language() == HandlerLanguage::Direct {
            let interpreter = source.interpreter();
            let mut command = interpreter.first().map_or_else(
                || Command::new(source.absolute_path()),
                |program| {
                    let mut command = Command::new(program);
                    command.args(&interpreter[1..]);
                    command.arg(source.absolute_path());
                    command
                },
            );
            command
                .current_dir(source.project_root())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            return command;
        }
        let (program, arguments): (&Path, Vec<&OsStr>) = match source.language() {
            HandlerLanguage::JavaScript => (
                &self.options.runtimes.javascript,
                vec![
                    OsStr::new("--no-warnings"),
                    adapter.runner.as_os_str(),
                    source.absolute_path().as_os_str(),
                ],
            ),
            HandlerLanguage::Python => (
                &self.options.runtimes.python,
                vec![
                    OsStr::new("-I"),
                    OsStr::new("-B"),
                    adapter.runner.as_os_str(),
                    source.absolute_path().as_os_str(),
                    source.project_root().as_os_str(),
                ],
            ),
            HandlerLanguage::Direct => unreachable!("handled above"),
        };
        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(source.project_root())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }
}

/// The response a direct handler writes: a status, optional headers, and an
/// optional body. Everything else in the envelope is supplied by the
/// supervisor, so a handler in any language stays a few lines long.
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
/// A direct handler receives plain JSON terminated by end of file. There is no
/// framing for it to implement, which is what lets any language serve a route
/// without an adapter.
fn encode_request(language: HandlerLanguage, request: &HandlerRequest) -> Result<Vec<u8>, Failure> {
    if language == HandlerLanguage::Direct {
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

fn materialize_adapter(language: HandlerLanguage) -> Result<AdapterFiles, Failure> {
    let directory = match tempfile::Builder::new()
        .prefix("tachyon-handler-")
        .tempdir()
    {
        Ok(directory) => directory,
        Err(error) => return Err(adapter_io(&error)),
    };
    let (name, contents) = match language {
        HandlerLanguage::JavaScript => ("runner.mjs", JAVASCRIPT_RUNNER),
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
    stdout: JoinHandle<Drained>,
    stderr: JoinHandle<Drained>,
) -> Result<(Drained, Drained), Failure> {
    let Ok(stdout) = stdout.await else {
        return Err(protocol_failure(2101, "Handler stdout drain task failed."));
    };
    let Ok(stderr) = stderr.await else {
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
    child: &mut Child,
    stdin: Option<&mut ChildStdin>,
    request_id: &str,
    grace: Duration,
) {
    // A direct handler has no open input to cancel through; it is reaped by
    // the grace period below instead.
    if let Some(stdin) = stdin
        && let Ok(frame) = cancel_frame(request_id)
    {
        let _write = stdin.write_all(&frame).await;
        let _flush = stdin.flush().await;
    }
    if timeout(grace, child.wait()).await.is_err() {
        terminate(child).await;
    }
}

async fn terminate(child: &mut Child) {
    let _kill = child.kill().await;
    let _wait = child.wait().await;
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
        Drained, EnvironmentPolicy, HandlerSupervisor, HandlerSupervisorOptions, adapter_io,
        bounded_sideband, drain, process_pipe_failure, settle_tasks,
    };
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{AsyncRead, ReadBuf};

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
        let failure = settle_tasks(failed, healthy)
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
        let failure = settle_tasks(healthy, failed)
            .await
            .expect_err("join failure");
        assert!(failure.to_string().contains("TY2101"));
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
