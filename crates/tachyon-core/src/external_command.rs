//! Bounded, process-tree-owned execution for framework toolchains.

use command_group::{AsyncCommandGroup, AsyncGroupChild};
use std::io;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::{Instant, timeout_at};

const MAX_CLEANUP_RESERVE: Duration = Duration::from_millis(250);
const MAX_SYNC_JOIN_RESERVE: Duration = Duration::from_millis(100);

#[derive(Clone, Default)]
struct LifecycleTracker {
    #[cfg(test)]
    active_drains: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(test)]
    active_sync_supervisors: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(test)]
struct ActiveGuard(std::sync::Arc<std::sync::atomic::AtomicUsize>);

#[cfg(test)]
impl ActiveGuard {
    fn new(counter: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Self {
        counter.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Self(counter)
    }
}

#[cfg(test)]
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[derive(Debug)]
pub(crate) struct ToolOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum ToolError {
    Spawn(io::Error),
    Wait(io::Error),
    TimedOut,
    Pipe(io::Error),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "cannot spawn tool: {error}"),
            Self::Wait(error) => write!(formatter, "cannot wait for tool: {error}"),
            Self::TimedOut => formatter.write_str("tool exceeded its deadline"),
            Self::Pipe(error) => write!(formatter, "cannot drain tool output: {error}"),
        }
    }
}

/// Runs a tool inside an owned process group with an absolute wall-clock bound.
///
/// Both pipes are drained concurrently while the process runs. Only `cap`
/// bytes are retained from each pipe, but excess output is still consumed so a
/// noisy tool cannot deadlock on a full OS pipe. The group is terminated and
/// reaped even after a successful leader exit, because descendants may retain
/// inherited pipe handles or continue mutating build output.
pub(crate) fn run<'command>(
    command: &'command mut Command,
    timeout: Duration,
    cap: usize,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ToolOutput, ToolError>> + Send + 'command>,
> {
    Box::pin(run_owned(
        command,
        timeout,
        cap,
        LifecycleTracker::default(),
    ))
}

async fn run_owned(
    command: &mut Command,
    timeout: Duration,
    cap: usize,
    tracker: LifecycleTracker,
) -> Result<ToolOutput, ToolError> {
    let deadline = Instant::now() + timeout;
    let cleanup = cleanup_reserve(timeout);
    let leader_deadline = deadline - cleanup;
    let tree_deadline = deadline - cleanup / 2;
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .group()
        .kill_on_drop(true)
        .spawn()
        .map_err(ToolError::Spawn)?;
    let stdout =
        child.inner().stdout.take().ok_or_else(|| {
            ToolError::Pipe(io::Error::other("tool stdout pipe was not available"))
        })?;
    let stderr =
        child.inner().stderr.take().ok_or_else(|| {
            ToolError::Pipe(io::Error::other("tool stderr pipe was not available"))
        })?;
    let stdout_drain = drain(stdout, cap, tracker.clone());
    let stderr_drain = drain(stderr, cap, tracker);
    tokio::pin!(stdout_drain, stderr_drain);
    let mut stdout_result = None;
    let mut stderr_result = None;
    let leader_outcome = {
        let wait = child.inner().wait();
        tokio::pin!(wait);
        loop {
            tokio::select! {
                biased;
                result = &mut wait => break result.map_err(ToolError::Wait),
                result = &mut stdout_drain, if stdout_result.is_none() => {
                    stdout_result = Some(result);
                }
                result = &mut stderr_drain, if stderr_result.is_none() => {
                    stderr_result = Some(result);
                }
                () = tokio::time::sleep_until(leader_deadline) => {
                    break Err(ToolError::TimedOut);
                }
            }
        }
    };
    // A successful parent is not proof that its descendants exited.
    if !settle_tree_until(&mut child, tree_deadline).await {
        return Err(ToolError::TimedOut);
    }

    while stdout_result.is_none() || stderr_result.is_none() {
        tokio::select! {
            biased;
            result = &mut stdout_drain, if stdout_result.is_none() => {
                stdout_result = Some(result);
            }
            result = &mut stderr_drain, if stderr_result.is_none() => {
                stderr_result = Some(result);
            }
            () = tokio::time::sleep_until(deadline) => return Err(ToolError::TimedOut),
        }
    }
    let status = leader_outcome?;
    let (stdout, _) = stdout_result
        .ok_or_else(|| ToolError::Pipe(io::Error::other("stdout drain did not settle")))?
        .map_err(ToolError::Pipe)?;
    let (stderr, _) = stderr_result
        .ok_or_else(|| ToolError::Pipe(io::Error::other("stderr drain did not settle")))?
        .map_err(ToolError::Pipe)?;
    Ok(ToolOutput {
        status,
        stdout,
        stderr,
    })
}

async fn drain(
    mut pipe: impl AsyncRead + Unpin,
    cap: usize,
    tracker: LifecycleTracker,
) -> io::Result<(Vec<u8>, bool)> {
    #[cfg(test)]
    let _active = ActiveGuard::new(tracker.active_drains);
    #[cfg(not(test))]
    let _ = tracker;
    let mut retained = Vec::with_capacity(cap.min(8 * 1_024));
    let mut buffer = [0_u8; 8 * 1_024];
    let mut truncated = false;
    loop {
        let read = pipe.read(&mut buffer).await?;
        if read == 0 {
            return Ok((retained, truncated));
        }
        let remaining = cap.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(remaining)]);
        truncated |= read > remaining;
    }
}

async fn settle_tree_until(child: &mut AsyncGroupChild, deadline: Instant) -> bool {
    let _ = child.start_kill();
    matches!(timeout_at(deadline, child.wait()).await, Ok(Ok(_)))
}

/// Synchronous facade for discovery/probe call sites that cannot enter Tokio.
/// It uses the same process-group/job-object and bounded-pipe contract as
/// [`run`], without constructing or nesting an async runtime.
pub(crate) fn run_sync(
    command: &mut std::process::Command,
    timeout: Duration,
    cap: usize,
) -> Result<ToolOutput, ToolError> {
    run_sync_owned(command, timeout, cap, LifecycleTracker::default())
}

fn run_sync_owned(
    command: &mut std::process::Command,
    timeout: Duration,
    cap: usize,
    tracker: LifecycleTracker,
) -> Result<ToolOutput, ToolError> {
    let deadline = std::time::Instant::now() + timeout;
    let join_reserve = (timeout / 4).min(MAX_SYNC_JOIN_RESERVE);
    let inner_deadline = deadline.checked_sub(join_reserve).unwrap_or(deadline);
    let owned = std::mem::replace(
        command,
        std::process::Command::new("__tachyon_consumed_command__"),
    );
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let supervisor = std::thread::Builder::new()
        .name(String::from("tachyon-tool-supervisor"))
        .spawn(move || {
            #[cfg(test)]
            let _active = ActiveGuard::new(tracker.active_sync_supervisors.clone());
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(ToolError::Spawn)
                .and_then(|runtime| {
                    let remaining =
                        inner_deadline.saturating_duration_since(std::time::Instant::now());
                    let mut command = Command::from(owned);
                    runtime.block_on(run_owned(&mut command, remaining, cap, tracker))
                });
            let _ = sender.send(result);
        })
        .map_err(ToolError::Spawn)?;
    let message =
        receiver.recv_timeout(inner_deadline.saturating_duration_since(std::time::Instant::now()));
    while !supervisor.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    supervisor
        .join()
        .map_err(|_| ToolError::Pipe(io::Error::other("tool supervisor panicked")))?;
    match message {
        Ok(result) => result,
        Err(_) => receiver.try_recv().unwrap_or(Err(ToolError::TimedOut)),
    }
}

fn cleanup_reserve(timeout: Duration) -> Duration {
    (timeout / 4).min(MAX_CLEANUP_RESERVE)
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::expect_used)]
    use super::{LifecycleTracker, ToolError, run, run_owned, run_sync_owned};
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::process::Command;

    static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn process_exists(pid: u32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    async fn assert_process_gone(pid: u32) {
        for _attempt in 0..100 {
            if !process_exists(pid) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("descendant {pid} survived parent exit");
    }

    #[tokio::test]
    async fn successful_parent_cannot_leave_a_descendant_or_unbounded_output() {
        let _serial = TEST_LOCK.lock().await;
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & echo $!; yes x | head -c 131072"]);
        let output = run(&mut command, Duration::from_secs(2), 1_024)
            .await
            .expect("supervised output");
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 1_024);
        let pid = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .expect("child pid")
            .parse::<u32>()
            .expect("numeric pid");
        assert_process_gone(pid).await;
    }

    #[tokio::test]
    async fn deadline_kills_and_reaps_the_process_group() {
        let _serial = TEST_LOCK.lock().await;
        let started = std::time::Instant::now();
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & echo $!; wait"]);
        let tracker = LifecycleTracker::default();
        let error = Box::pin(run_owned(
            &mut command,
            Duration::from_millis(100),
            1_024,
            tracker.clone(),
        ))
        .await
        .expect_err("deadline");
        assert!(matches!(error, ToolError::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(
            tracker
                .active_drains
                .load(std::sync::atomic::Ordering::Acquire),
            0,
            "drain futures survived async return"
        );
    }

    #[test]
    fn synchronous_deadline_and_success_cleanup_are_bounded() {
        let _serial = TEST_LOCK.blocking_lock();
        let tracker = LifecycleTracker::default();
        let mut success = std::process::Command::new("sh");
        success.args(["-c", "sleep 30 & echo $!; yes x | head -c 131072"]);
        let output = run_sync_owned(&mut success, Duration::from_secs(2), 1_024, tracker.clone())
            .expect("synchronous supervised output");
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 1_024);
        let pid = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .expect("child pid")
            .parse::<u32>()
            .expect("numeric pid");
        assert!(!process_exists(pid), "descendant {pid} survived cleanup");

        let started = std::time::Instant::now();
        let mut timeout = std::process::Command::new("sh");
        timeout.args(["-c", "sleep 30 & wait"]);
        assert!(matches!(
            run_sync_owned(
                &mut timeout,
                Duration::from_millis(100),
                1_024,
                tracker.clone(),
            ),
            Err(ToolError::TimedOut)
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(
            tracker
                .active_drains
                .load(std::sync::atomic::Ordering::Acquire),
            0,
            "sync drain futures survived return"
        );
        assert_eq!(
            tracker
                .active_sync_supervisors
                .load(std::sync::atomic::Ordering::Acquire),
            0,
            "sync supervisor thread survived return"
        );
    }
}
