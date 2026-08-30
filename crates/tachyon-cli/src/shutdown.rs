//! Cross-platform shutdown signals for long-running CLI commands.

use serde_json::{Value, json};
use std::future::{Future, pending};
use std::io::{self, Write as _};
use std::pin::Pin;
use tokio::sync::oneshot;

/// A canonical long-running command name used in lifecycle events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LongRunningCommand {
    /// The development server, including its `dev` compatibility alias.
    Serve,
    /// The bundle preview server.
    Preview,
    /// A bundle command waiting for source changes.
    BundleWatch,
}

impl LongRunningCommand {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Serve => "serve",
            Self::Preview => "preview",
            Self::BundleWatch => "bundle.watch",
        }
    }
}

/// A shutdown request owned by one long-running command.
///
/// Installation is deliberately infallible. If the operating system refuses
/// a handler, the failure is reported and that receiver remains absent; it can
/// never masquerade as a shutdown request.
#[must_use]
pub(crate) struct ShutdownSignals {
    requested: oneshot::Receiver<()>,
}

impl ShutdownSignals {
    /// Installs the platform's supported graceful-shutdown handlers.
    pub(crate) fn install(command: LongRunningCommand) -> Self {
        install_with(
            command,
            PlatformSignalSource::register(),
            StderrEvents,
            ProcessExit,
        )
    }

    /// Resolves after the first installed signal is observed.
    ///
    /// A failed monitor is treated like failed registration: availability is
    /// preserved instead of turning an internal failure into shutdown.
    pub(crate) async fn wait(self) {
        if self.requested.await.is_err() {
            pending::<()>().await;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SignalName {
    #[cfg(unix)]
    SigInt,
    #[cfg(unix)]
    SigTerm,
    #[cfg(windows)]
    CtrlC,
    #[cfg(windows)]
    CtrlBreak,
}

impl SignalName {
    const fn as_str(self) -> &'static str {
        match self {
            #[cfg(unix)]
            Self::SigInt => "SIGINT",
            #[cfg(unix)]
            Self::SigTerm => "SIGTERM",
            #[cfg(windows)]
            Self::CtrlC => "CTRL_C",
            #[cfg(windows)]
            Self::CtrlBreak => "CTRL_BREAK",
        }
    }

    const fn code(self) -> i32 {
        match self {
            #[cfg(unix)]
            Self::SigInt => 2,
            #[cfg(unix)]
            Self::SigTerm => 15,
            #[cfg(windows)]
            Self::CtrlC => 0,
            #[cfg(windows)]
            Self::CtrlBreak => 1,
        }
    }

    const fn forced_exit_code(self) -> i32 {
        match self {
            #[cfg(unix)]
            Self::SigInt => 130,
            #[cfg(unix)]
            Self::SigTerm => 143,
            #[cfg(windows)]
            Self::CtrlC => 130,
            #[cfg(windows)]
            Self::CtrlBreak => 131,
        }
    }
}

#[derive(Clone, Debug)]
struct SignalRegistration {
    signal: SignalName,
    failure: Option<RegistrationFailure>,
}

impl SignalRegistration {
    const fn installed(signal: SignalName) -> Self {
        Self {
            signal,
            failure: None,
        }
    }

    fn unavailable(signal: SignalName, error: &io::Error) -> Self {
        Self {
            signal,
            failure: Some(RegistrationFailure {
                error_kind: format!("{:?}", error.kind()).to_ascii_lowercase(),
                raw_os_error: error.raw_os_error(),
            }),
        }
    }
}

#[derive(Clone, Debug)]
struct RegistrationFailure {
    error_kind: String,
    raw_os_error: Option<i32>,
}

fn registration<T>(signal: SignalName, result: io::Result<T>) -> (Option<T>, SignalRegistration) {
    match result {
        Ok(receiver) => (Some(receiver), SignalRegistration::installed(signal)),
        Err(error) => (None, SignalRegistration::unavailable(signal, &error)),
    }
}

trait SignalSource: Send + 'static {
    fn registrations(&self) -> &[SignalRegistration];

    fn receive(&mut self) -> Pin<Box<dyn Future<Output = SignalName> + Send + '_>>;
}

struct PlatformSignalSource {
    registrations: Vec<SignalRegistration>,
    #[cfg(unix)]
    sigint: Option<tokio::signal::unix::Signal>,
    #[cfg(unix)]
    sigterm: Option<tokio::signal::unix::Signal>,
    #[cfg(windows)]
    ctrl_c: Option<tokio::signal::windows::CtrlC>,
    #[cfg(windows)]
    ctrl_break: Option<tokio::signal::windows::CtrlBreak>,
}

impl PlatformSignalSource {
    #[cfg(unix)]
    fn register() -> Self {
        use tokio::signal::unix::{SignalKind, signal};

        let (sigint, sigint_registration) =
            registration(SignalName::SigInt, signal(SignalKind::interrupt()));
        let (sigterm, sigterm_registration) =
            registration(SignalName::SigTerm, signal(SignalKind::terminate()));
        Self {
            registrations: vec![sigint_registration, sigterm_registration],
            sigint,
            sigterm,
        }
    }

    #[cfg(windows)]
    fn register() -> Self {
        use tokio::signal::windows::{ctrl_break, ctrl_c};

        let (ctrl_c, ctrl_c_registration) = registration(SignalName::CtrlC, ctrl_c());
        let (ctrl_break, ctrl_break_registration) =
            registration(SignalName::CtrlBreak, ctrl_break());
        Self {
            registrations: vec![ctrl_c_registration, ctrl_break_registration],
            ctrl_c,
            ctrl_break,
        }
    }

    #[cfg(unix)]
    async fn receive_platform(&mut self) -> SignalName {
        tokio::select! {
            () = receive_unix(&mut self.sigint) => SignalName::SigInt,
            () = receive_unix(&mut self.sigterm) => SignalName::SigTerm,
        }
    }

    #[cfg(windows)]
    async fn receive_platform(&mut self) -> SignalName {
        tokio::select! {
            () = receive_ctrl_c(&mut self.ctrl_c) => SignalName::CtrlC,
            () = receive_ctrl_break(&mut self.ctrl_break) => SignalName::CtrlBreak,
        }
    }
}

impl SignalSource for PlatformSignalSource {
    fn registrations(&self) -> &[SignalRegistration] {
        &self.registrations
    }

    fn receive(&mut self) -> Pin<Box<dyn Future<Output = SignalName> + Send + '_>> {
        Box::pin(self.receive_platform())
    }
}

#[cfg(unix)]
async fn receive_unix(receiver: &mut Option<tokio::signal::unix::Signal>) {
    let Some(receiver) = receiver else {
        pending::<()>().await;
        return;
    };
    if receiver.recv().await.is_none() {
        pending::<()>().await;
    }
}

#[cfg(windows)]
async fn receive_ctrl_c(receiver: &mut Option<tokio::signal::windows::CtrlC>) {
    let Some(receiver) = receiver else {
        pending::<()>().await;
        return;
    };
    if receiver.recv().await.is_none() {
        pending::<()>().await;
    }
}

#[cfg(windows)]
async fn receive_ctrl_break(receiver: &mut Option<tokio::signal::windows::CtrlBreak>) {
    let Some(receiver) = receiver else {
        pending::<()>().await;
        return;
    };
    if receiver.recv().await.is_none() {
        pending::<()>().await;
    }
}

trait SignalEventWriter: Clone + Send + Sync + 'static {
    fn write(&self, event: &Value);
}

#[derive(Clone, Copy)]
struct StderrEvents;

impl SignalEventWriter for StderrEvents {
    fn write(&self, event: &Value) {
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        let stderr = io::stderr();
        let mut lock = stderr.lock();
        let _ = writeln!(lock, "{line}");
        let _ = lock.flush();
    }
}

trait ProcessTerminator: Send + 'static {
    fn exit(&self, code: i32);
}

struct ProcessExit;

impl ProcessTerminator for ProcessExit {
    fn exit(&self, code: i32) {
        std::process::exit(code);
    }
}

fn install_with<S, W, P>(
    command: LongRunningCommand,
    mut source: S,
    writer: W,
    terminator: P,
) -> ShutdownSignals
where
    S: SignalSource,
    W: SignalEventWriter,
    P: ProcessTerminator,
{
    write_registration_events(command, source.registrations(), &writer);
    let (requested, receiver) = oneshot::channel();
    tokio::spawn(async move {
        let first = source.receive().await;
        writer.write(&shutdown_event(
            "runtime.shutdown_requested",
            command,
            "signal",
            first,
        ));
        let _ = requested.send(());

        let second = source.receive().await;
        writer.write(&shutdown_event(
            "runtime.shutdown_forced",
            command,
            "repeated_signal",
            second,
        ));
        terminator.exit(second.forced_exit_code());
    });
    ShutdownSignals {
        requested: receiver,
    }
}

fn write_registration_events<W: SignalEventWriter>(
    command: LongRunningCommand,
    registrations: &[SignalRegistration],
    writer: &W,
) {
    for registration in registrations {
        let Some(failure) = &registration.failure else {
            continue;
        };
        writer.write(&json!({
            "event": "runtime.signal_handler_unavailable",
            "event_version": 1,
            "command": command.as_str(),
            "platform": std::env::consts::OS,
            "signal": registration.signal.as_str(),
            "error_kind": failure.error_kind,
            "raw_os_error": failure.raw_os_error,
        }));
    }
    let installed = registrations
        .iter()
        .filter(|registration| registration.failure.is_none())
        .map(|registration| registration.signal.as_str())
        .collect::<Vec<_>>();
    let unavailable = registrations
        .iter()
        .filter(|registration| registration.failure.is_some())
        .map(|registration| registration.signal.as_str())
        .collect::<Vec<_>>();
    writer.write(&json!({
        "event": "runtime.signal_handlers_ready",
        "event_version": 1,
        "command": command.as_str(),
        "platform": std::env::consts::OS,
        "installed": installed,
        "unavailable": unavailable,
    }));
}

fn shutdown_event(
    event: &str,
    command: LongRunningCommand,
    reason: &str,
    signal: SignalName,
) -> Value {
    json!({
        "event": event,
        "event_version": 1,
        "command": command.as_str(),
        "platform": std::env::consts::OS,
        "reason": reason,
        "signal": signal.as_str(),
        "signal_code": signal.code(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    struct FakeSignals {
        registrations: Vec<SignalRegistration>,
        receiver: mpsc::UnboundedReceiver<SignalName>,
    }

    impl SignalSource for FakeSignals {
        fn registrations(&self) -> &[SignalRegistration] {
            &self.registrations
        }

        fn receive(&mut self) -> Pin<Box<dyn Future<Output = SignalName> + Send + '_>> {
            Box::pin(async move {
                loop {
                    if let Some(signal) = self.receiver.recv().await {
                        return signal;
                    }
                    pending::<()>().await;
                }
            })
        }
    }

    #[derive(Clone, Default)]
    struct RecordingEvents(Arc<Mutex<Vec<Value>>>);

    impl SignalEventWriter for RecordingEvents {
        fn write(&self, event: &Value) {
            self.0.lock().expect("events lock").push(event.clone());
        }
    }

    #[derive(Clone, Default)]
    struct RecordingExit(Arc<Mutex<Vec<i32>>>);

    impl ProcessTerminator for RecordingExit {
        fn exit(&self, code: i32) {
            self.0.lock().expect("exit lock").push(code);
        }
    }

    fn installed() -> Vec<SignalRegistration> {
        vec![
            SignalRegistration::installed(primary_signal()),
            SignalRegistration::installed(secondary_signal()),
        ]
    }

    #[cfg(unix)]
    const fn primary_signal() -> SignalName {
        SignalName::SigInt
    }

    #[cfg(windows)]
    const fn primary_signal() -> SignalName {
        SignalName::CtrlC
    }

    #[cfg(unix)]
    const fn secondary_signal() -> SignalName {
        SignalName::SigTerm
    }

    #[cfg(windows)]
    const fn secondary_signal() -> SignalName {
        SignalName::CtrlBreak
    }

    #[tokio::test]
    async fn registration_failure_is_reported_without_requesting_shutdown() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let registrations = vec![
            SignalRegistration::installed(primary_signal()),
            SignalRegistration::unavailable(
                secondary_signal(),
                &io::Error::other("synthetic registration failure"),
            ),
        ];
        let events = RecordingEvents::default();
        let shutdown = install_with(
            LongRunningCommand::Serve,
            FakeSignals {
                registrations,
                receiver,
            },
            events.clone(),
            RecordingExit::default(),
        );
        {
            let recorded = events.0.lock().expect("events lock");
            assert_eq!(recorded.len(), 2);
            assert_eq!(recorded[0]["event"], "runtime.signal_handler_unavailable");
            assert_eq!(recorded[1]["installed"], json!([primary_signal().as_str()]));
            assert_eq!(
                recorded[1]["unavailable"],
                json!([secondary_signal().as_str()])
            );
        }

        let mut waiting = Box::pin(shutdown.wait());
        tokio::select! {
            () = &mut waiting => panic!("registration failure requested shutdown"),
            () = tokio::task::yield_now() => {}
        }
        sender.send(primary_signal()).expect("first signal");
        waiting.await;
    }

    #[tokio::test]
    async fn a_second_signal_uses_the_documented_forced_exit_policy() {
        let (sender, receiver) = mpsc::unbounded_channel();
        let events = RecordingEvents::default();
        let exits = RecordingExit::default();
        let shutdown = install_with(
            LongRunningCommand::Preview,
            FakeSignals {
                registrations: installed(),
                receiver,
            },
            events.clone(),
            exits.clone(),
        );

        sender.send(secondary_signal()).expect("first signal");
        shutdown.wait().await;
        sender.send(primary_signal()).expect("second signal");
        for _ in 0..100 {
            if !exits.0.lock().expect("exit lock").is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }

        assert_eq!(
            *exits.0.lock().expect("exit lock"),
            vec![primary_signal().forced_exit_code()]
        );
        let recorded = events.0.lock().expect("events lock");
        assert_eq!(recorded[1]["event"], "runtime.shutdown_requested");
        assert_eq!(recorded[1]["signal"], secondary_signal().as_str());
        assert_eq!(recorded[2]["event"], "runtime.shutdown_forced");
        assert_eq!(recorded[2]["signal"], primary_signal().as_str());
    }

    #[tokio::test]
    async fn no_registered_signal_keeps_the_command_available() {
        let (_sender, receiver) = mpsc::unbounded_channel();
        let unavailable = |signal| {
            SignalRegistration::unavailable(
                signal,
                &io::Error::other("synthetic registration failure"),
            )
        };
        let shutdown = install_with(
            LongRunningCommand::BundleWatch,
            FakeSignals {
                registrations: vec![
                    unavailable(primary_signal()),
                    unavailable(secondary_signal()),
                ],
                receiver,
            },
            RecordingEvents::default(),
            RecordingExit::default(),
        );
        let mut waiting = Box::pin(shutdown.wait());
        tokio::select! {
            () = &mut waiting => panic!("total registration failure requested shutdown"),
            () = tokio::task::yield_now() => {}
        }
    }

    #[tokio::test]
    async fn a_closed_monitor_never_becomes_a_shutdown_request() {
        let (sender, receiver) = oneshot::channel();
        drop(sender);
        let shutdown = ShutdownSignals {
            requested: receiver,
        };
        let mut waiting = Box::pin(shutdown.wait());
        tokio::select! {
            () = &mut waiting => panic!("closed monitor requested shutdown"),
            () = tokio::task::yield_now() => {}
        }
    }
}
