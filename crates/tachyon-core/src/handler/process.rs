mod adapter_files;
mod direct_response;
mod drained;
mod environment_policy;
mod event_reader;
mod handler_cancellation;
#[cfg(test)]
mod handler_environment_tests;
mod handler_event;
mod handler_runtime_programs;
#[cfg(test)]
mod handler_runtime_programs_tests;
mod handler_supervisor_options;
mod planned_command;
mod process_failure;
mod process_outcome;
mod process_pipes;
mod process_reaper;
mod running_process;
mod running_stream;
mod runtime_identity;
mod socket;
mod started_process;
mod stream_frame;
mod stream_process_outcome;
mod stream_read_outcome;
mod streaming_process;

pub(in crate::handler) use drained::Drained;
pub use environment_policy::EnvironmentPolicy;
pub use handler_cancellation::HandlerCancellation;
pub use handler_event::HandlerEvent;
pub use handler_runtime_programs::HandlerRuntimePrograms;
#[cfg(test)]
pub(crate) use handler_runtime_programs_tests::InstalledPrograms;
pub use handler_supervisor_options::HandlerSupervisorOptions;
pub(in crate::handler) use process_reaper::ProcessReaper;
pub(crate) use socket::{SocketChannels, SocketInput, SocketOutput};

use super::frame::{FrameLimits, HandlerFrame};
use super::gateway::{GatewayCallSetup, GatewayCalls, GatewayTable};
use super::readiness::{RuntimeProbe, RuntimeProbeResult, RuntimeRequirement, RuntimeRequirements};
use super::resources::ResourceAccess;
use super::topic_bus::{TopicBus, TopicOutbox};
use super::{FirecrackerIsolation, HandlerLanguage, HandlerSource};
use crate::Failure;
use crate::failure::diagnostic;
use crate::storage::ManagedStorage;
use adapter_files::AdapterFiles;
use command_group::AsyncGroupChild;
use event_reader::EventReader;
use futures_util::future::OptionFuture;
use planned_command::PlannedCommand;
use process_failure::ProcessFailure;
use process_outcome::ProcessOutcome;
use process_pipes::ProcessPipes;
use running_process::RunningProcess;
use running_stream::RunningStream;
use runtime_identity::RuntimeIdentity;
use serde_json::Value;
use started_process::StartedProcess;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use stream_frame::StreamFrame;
use stream_process_outcome::StreamProcessOutcome;
use stream_read_outcome::StreamReadOutcome;
use streaming_process::StreamingProcess;
use tachyon_contracts::{HandlerRequest, HandlerResponse};
use tokio::io::AsyncWriteExt;
use tokio::process::{ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep_until};

pub(super) type GatewayUnitCall<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<HandlerResponse, Failure>> + Send + 'a>,
>;

#[derive(Clone, Debug)]
pub struct HandlerSupervisor {
    options: HandlerSupervisorOptions,
    permits: Arc<Semaphore>,
    gateway_permits: Arc<Semaphore>,
    resources: Arc<tokio::sync::OnceCell<Arc<ManagedStorage>>>,
    topics: Arc<std::sync::OnceLock<Arc<TopicBus>>>,
    egress: Arc<std::sync::OnceLock<String>>,
    node_proxy_gate: crate::handler::NodeProxyGate,
}

impl HandlerSupervisor {
    pub fn from_environment() -> Result<Self, Failure> {
        Self::new(HandlerSupervisorOptions::from_environment()?)
    }

    pub fn new(mut options: HandlerSupervisorOptions) -> Result<Self, Failure> {
        options.check_limits()?;
        options
            .environment
            .include(options.application.handler_environment());
        let permits = Arc::new(Semaphore::new(options.max_concurrency));
        let gateway_permits = Arc::new(Semaphore::new(options.max_concurrency));
        Ok(Self {
            options,
            permits,
            gateway_permits,
            resources: Arc::new(tokio::sync::OnceCell::new()),
            topics: Arc::new(std::sync::OnceLock::new()),
            egress: Arc::new(std::sync::OnceLock::new()),
            node_proxy_gate: crate::handler::NodeProxyGate::default(),
        })
    }

    pub(crate) fn send_outgoing_requests_through(&self, proxy: String) {
        let _ = self.egress.set(proxy);
    }

    pub(crate) async fn invoke_socket(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        channels: SocketChannels,
        cancellation: &HandlerCancellation,
        lifetime: Duration,
    ) -> Result<(), Failure> {
        socket::SocketProcess::invoke(self, source, request, channels, cancellation, lifetime).await
    }

    pub(crate) const fn application(&self) -> &crate::ApplicationConfig {
        &self.options.application
    }

    pub(crate) fn connect_topics(&self, bus: Arc<TopicBus>) {
        let _already = self.topics.set(bus);
    }

    pub(crate) async fn preflight(
        &self,
        requirements: &RuntimeRequirements,
    ) -> Result<(), Failure> {
        let resolved = requirements.resolve(&self.options.runtimes, &self.options.isolation)?;
        let uses_javascript = RuntimeRequirement::includes_javascript(&resolved);
        let results = RuntimeProbe::all(resolved, &self.options.environment).await?;
        RuntimeProbeResult::failure(&results).map_or(Ok(()), Err)?;
        self.require_node_proxy_switch(uses_javascript).await?;
        self.storage().await?;
        Ok(())
    }

    pub async fn preflight_languages<'a>(
        &self,
        extensions: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), Failure> {
        self.preflight(&RuntimeRequirements::from_extensions(extensions))
            .await
    }

    pub async fn preflight_gateways<'a>(
        &self,
        sources: impl IntoIterator<Item = &'a HandlerSource>,
    ) -> Result<(), Failure> {
        self.preflight(&RuntimeRequirements::from_gateways(sources))
            .await
    }

    pub(super) async fn acquire_gateway_permit(
        &self,
        cancellation: &HandlerCancellation,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit, Failure> {
        Self::acquire_permit_from(&self.gateway_permits, cancellation, deadline).await
    }

    pub(super) const fn external_environment(&self) -> &EnvironmentPolicy {
        &self.options.environment
    }

    pub(super) const fn stderr_limit(&self) -> usize {
        self.options.stderr_limit
    }

    pub(crate) async fn close_resources(&self) {
        self.permits.close();
        self.gateway_permits.close();
        OptionFuture::from(self.resources.get().map(|storage| storage.close())).await;
    }

    pub async fn invoke(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        cancellation: &HandlerCancellation,
    ) -> Result<HandlerResponse, Failure> {
        self.invoke_with_permits(source, request, cancellation, &self.permits)
            .await
    }

    pub(super) fn invoke_gateway_unit<'a>(
        &'a self,
        source: &'a HandlerSource,
        request: &'a HandlerRequest,
        cancellation: &'a HandlerCancellation,
    ) -> GatewayUnitCall<'a> {
        Box::pin(self.invoke_with_permits(source, request, cancellation, &self.gateway_permits))
    }

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
        self.options
            .isolation
            .validate_backend_language(source.language())?;
        let direct_protocol = self.uses_direct_protocol(source);
        Self::check_request_encodes(direct_protocol, request)?;
        Self::check_not_cancelled(cancellation)?;
        let deadline = self.invocation_deadline(request);
        let permit = Self::acquire_permit_from(&self.permits, cancellation, deadline).await?;
        let result = self
            .run_streaming_process(StreamingProcess {
                source,
                request,
                direct_protocol,
                deadline,
                events,
                cancellation,
            })
            .await;
        drop(permit);
        result
    }

    pub(in crate::handler) fn pipe_all_streams(command: &mut Command) {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
    }

    async fn storage(&self) -> Result<Option<Arc<ManagedStorage>>, Failure> {
        let configuration = self.storage_configuration()?;
        self.check_storage_backend(configuration.as_ref())?;
        self.connected_storage(configuration).await
    }

    fn storage_configuration(&self) -> Result<Option<Value>, Failure> {
        let configuration = self
            .options
            .application
            .handler_json()
            .map(Self::parse_resource_configuration)
            .transpose()?;
        Ok(configuration.filter(Self::declares_storage))
    }

    fn parse_resource_configuration(json: &str) -> Result<Value, Failure> {
        serde_json::from_str(json).map_err(|_| {
            Failure::one(diagnostic(
                2010,
                "Invalid managed resource configuration.",
                None,
                None,
            ))
        })
    }

    fn declares_storage(configuration: &Value) -> bool {
        configuration.pointer("/yon/datasource").is_some()
            || configuration.pointer("/yon/redis").is_some()
    }

    fn check_storage_backend(&self, configuration: Option<&Value>) -> Result<(), Failure> {
        if configuration.is_some() && self.options.isolation.firecracker().is_some() {
            return Err(Failure::one(diagnostic(
                2010,
                "Managed storage requires the Yon process backend.",
                Some(String::from(
                    "The Firecracker transport does not yet transfer invocation-scoped resource capabilities.",
                )),
                None,
            )));
        }
        Ok(())
    }

    async fn connected_storage(
        &self,
        configuration: Option<Value>,
    ) -> Result<Option<Arc<ManagedStorage>>, Failure> {
        match configuration {
            None => Ok(None),
            Some(configuration) => self
                .resources
                .get_or_try_init(|| async {
                    ManagedStorage::connect(&configuration).await.map(Arc::new)
                })
                .await
                .map(|storage| Some(Arc::clone(storage))),
        }
    }

    async fn resource_access(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<Option<ResourceAccess>, Failure> {
        let storage = self.storage().await?;
        let stop = cancellation.child();
        let gateways = self.gateway_calls(source, request, deadline, &stop)?;
        let topics = self
            .topics
            .get()
            .and_then(|bus| TopicOutbox::for_invocation(bus, source, request));
        Self::bind_resources(storage, gateways, topics, stop, deadline).await
    }

    async fn bind_resources(
        storage: Option<Arc<ManagedStorage>>,
        gateways: Option<GatewayCalls>,
        topics: Option<TopicOutbox>,
        stop: HandlerCancellation,
        deadline: Instant,
    ) -> Result<Option<ResourceAccess>, Failure> {
        if storage.is_none() && gateways.is_none() && topics.is_none() {
            return Ok(None);
        }
        ResourceAccess::bind(storage, gateways, topics, stop, deadline)
            .await
            .map(Some)
    }

    fn gateway_calls(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<Option<GatewayCalls>, Failure> {
        source
            .gateways()
            .map(|table| self.gateway_call_setup(table, source, request, deadline, cancellation))
            .transpose()
    }

    fn gateway_call_setup(
        &self,
        table: &Arc<GatewayTable>,
        source: &HandlerSource,
        request: &HandlerRequest,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<GatewayCalls, Failure> {
        self.check_gateway_backend()?;
        let setup = GatewayCallSetup {
            supervisor: self.clone(),
            table: Arc::clone(table),
            request_id: request.request_id.clone(),
            working_directory: source.execution_working_directory().to_path_buf(),
            deadline: deadline
                .checked_sub(self.options.cancellation_grace)
                .unwrap_or(deadline),
            cancellation: cancellation.child(),
        };
        Ok(GatewayCalls::new(setup))
    }

    fn check_gateway_backend(&self) -> Result<(), Failure> {
        if self.options.isolation.firecracker().is_some() {
            return Err(ProcessFailure::gateways_need_process_backend());
        }
        Ok(())
    }

    async fn invoke_with_permits(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        cancellation: &HandlerCancellation,
        permits: &Arc<Semaphore>,
    ) -> Result<HandlerResponse, Failure> {
        self.options
            .isolation
            .validate_backend_language(source.language())?;
        let direct_protocol = self.uses_direct_protocol(source);
        Self::check_request_encodes(direct_protocol, request)?;
        Self::check_not_cancelled(cancellation)?;
        let deadline = self.invocation_deadline(request);
        let permit = Self::acquire_permit_from(permits, cancellation, deadline).await?;
        let result = self
            .run_process(source, request, direct_protocol, deadline, cancellation)
            .await;
        drop(permit);
        result
    }

    fn check_not_cancelled(cancellation: &HandlerCancellation) -> Result<(), Failure> {
        if cancellation.is_cancelled() {
            return Err(ProcessFailure::cancelled());
        }
        Ok(())
    }

    fn check_before_deadline(deadline: Instant) -> Result<(), Failure> {
        if Instant::now() >= deadline {
            return Err(ProcessFailure::timed_out());
        }
        Ok(())
    }

    fn too_late_to_start(deadline: Instant, cancellation: &HandlerCancellation) -> Option<Failure> {
        Self::check_not_cancelled(cancellation)
            .and_then(|()| Self::check_before_deadline(deadline))
            .err()
    }

    fn uses_direct_protocol(&self, source: &HandlerSource) -> bool {
        source.language() == HandlerLanguage::Direct
            && self.options.isolation.uses_direct_handler_protocol()
    }

    fn request_with_time_left(
        &self,
        request: &HandlerRequest,
        direct_protocol: bool,
        deadline: Instant,
    ) -> Result<Vec<u8>, Failure> {
        let time_left = Self::time_left(deadline)?;
        let handler_time = time_left.saturating_sub(self.options.cancellation_grace);
        let milliseconds = u64::try_from(handler_time.as_millis()).unwrap_or(u64::MAX);
        let mut timed_request = request.clone();
        timed_request.deadline_ms = Some(milliseconds.max(1));
        Self::encode_request(direct_protocol, &timed_request)
    }

    fn time_left(deadline: Instant) -> Result<Duration, Failure> {
        let time_left = deadline.saturating_duration_since(Instant::now());
        if time_left.is_zero() {
            return Err(ProcessFailure::timed_out());
        }
        Ok(time_left)
    }

    fn invocation_deadline(&self, request: &HandlerRequest) -> Instant {
        let default_timeout = self.options.default_timeout;
        let requested_timeout = request
            .deadline_ms
            .map_or(default_timeout, Duration::from_millis);
        Instant::now() + requested_timeout.min(default_timeout)
    }

    async fn acquire_permit_from(
        permits: &Arc<Semaphore>,
        cancellation: &HandlerCancellation,
        deadline: Instant,
    ) -> Result<OwnedSemaphorePermit, Failure> {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(ProcessFailure::cancelled()),
            () = sleep_until(deadline) => Err(ProcessFailure::timed_out()),
            permit = Arc::clone(permits).acquire_owned() => {
                permit.map_err(|_| ProcessFailure::stopped_accepting_work())
            }
        }
    }

    async fn start_process(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        deadline: Instant,
        cancellation: &HandlerCancellation,
        socket: bool,
    ) -> Result<StartedProcess, Failure> {
        let adapter = AdapterFiles::materialize(source.language())?;
        let mut planned = self.handler_command(source, &adapter);
        self.require_node_proxy_switch(planned.runtime.runs_javascript())
            .await?;
        Self::mark_socket_frames(&mut planned.command, socket);
        let resource_access = self
            .resource_access_in_time(source, request, deadline, cancellation)
            .await?;
        let resource_access = Self::still_in_time(resource_access, deadline, cancellation).await?;
        Self::apply_resource_access(resource_access.as_ref(), &mut planned.command);
        self.apply_egress(&mut planned.command);
        self.options
            .isolation
            .apply_backend_environment(&mut planned.command);
        let mut child = planned.runtime.spawn(&mut planned.command)?;
        let pipes = Self::take_process_pipes(&mut child);
        let (stdin, stdout, stderr) =
            Self::terminate_on_failure(pipes, &mut child, &request.request_id).await?;
        Ok(StartedProcess {
            child,
            stdin,
            stdout,
            stderr,
            resource_access,
            _adapter: adapter,
        })
    }

    async fn require_node_proxy_switch(&self, uses_javascript: bool) -> Result<(), Failure> {
        let guarded = self.egress.get().is_some();
        let needed = [guarded, uses_javascript] == [true, true];
        if !needed {
            return Ok(());
        }
        self.node_proxy_gate
            .check(&self.options.runtimes, &self.options.environment)
            .await
    }

    fn apply_egress(&self, command: &mut Command) {
        if let Some(proxy) = self.egress.get() {
            crate::handler::EgressProxy::apply(proxy, command);
        }
    }

    fn mark_socket_frames(command: &mut Command, socket: bool) {
        if socket {
            command.env("YON_SOCKET_FRAMES", "1");
        }
    }

    async fn resource_access_in_time(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<Option<ResourceAccess>, Failure> {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(ProcessFailure::cancelled()),
            () = sleep_until(deadline) => Err(ProcessFailure::timed_out()),
            result = self.resource_access(source, request, deadline, cancellation) => result,
        }
    }

    async fn still_in_time(
        resource_access: Option<ResourceAccess>,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<Option<ResourceAccess>, Failure> {
        if let Some(failure) = Self::too_late_to_start(deadline, cancellation) {
            Self::close_resource_access(resource_access).await;
            return Err(failure);
        }
        Ok(resource_access)
    }

    async fn close_resource_access(resource_access: Option<ResourceAccess>) {
        OptionFuture::from(resource_access.map(ResourceAccess::close)).await;
    }

    fn apply_resource_access(resource_access: Option<&ResourceAccess>, command: &mut Command) {
        if let Some(access) = resource_access {
            access.apply(command);
        }
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
            (None, _, _) => Err(ProcessFailure::missing_pipe("stdin")),
            (_, None, _) => Err(ProcessFailure::missing_pipe("stdout")),
            (_, _, None) => Err(ProcessFailure::missing_pipe("stderr")),
        }
    }

    async fn terminate_on_failure<T>(
        result: Result<T, Failure>,
        child: &mut AsyncGroupChild,
        request_id: &str,
    ) -> Result<T, Failure> {
        if result.is_err() {
            ProcessReaper::terminate(child, request_id).await;
        }
        result
    }

    async fn run_process(
        &self,
        source: &HandlerSource,
        request: &HandlerRequest,
        direct_protocol: bool,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<HandlerResponse, Failure> {
        let StartedProcess {
            child,
            stdin,
            stdout,
            stderr,
            resource_access,
            _adapter,
        } = self
            .start_process(source, request, deadline, cancellation, false)
            .await?;
        let pipes = ProcessPipes {
            child,
            stdin,
            stdout,
            stderr,
        };
        let topics = resource_access.as_ref().and_then(ResourceAccess::topics);
        let result = self
            .drive_process(
                pipes,
                request,
                direct_protocol,
                topics,
                deadline,
                cancellation,
            )
            .await;
        Self::close_resource_access(resource_access).await;
        result
    }

    async fn drive_process(
        &self,
        pipes: ProcessPipes,
        request: &HandlerRequest,
        direct_protocol: bool,
        topics: Option<Arc<TopicOutbox>>,
        deadline: Instant,
        cancellation: &HandlerCancellation,
    ) -> Result<HandlerResponse, Failure> {
        let ProcessPipes {
            mut child,
            mut stdin,
            stdout,
            stderr,
        } = pipes;
        let request_id = &request.request_id;
        let request_bytes = self.request_with_time_left(request, direct_protocol, deadline);
        let request_bytes =
            Self::terminate_on_failure(request_bytes, &mut child, request_id).await?;
        let stdout_limit = FrameLimits::MAX_BYTES + FrameLimits::PREFIX_BYTES;
        let stdout_task = tokio::spawn(Drained::read_all(stdout, stdout_limit));
        let stderr_task = tokio::spawn(Drained::read_all(stderr, self.options.stderr_limit));
        let write_failure =
            Self::write_request_until(&mut stdin, &request_bytes, cancellation, deadline).await;
        let (stdout_task, stderr_task) = Self::stop_after_write_failure(
            write_failure,
            &mut child,
            request_id,
            (stdout_task, stderr_task),
            deadline,
        )
        .await?;
        let running = RunningProcess {
            child,
            stdin: Self::stdin_kept_open(stdin, direct_protocol),
            stdout_task,
            stderr_task,
            deadline,
            direct_protocol,
            topics,
        };
        running
            .run(cancellation, request_id, self.options.cancellation_grace)
            .await
    }

    async fn stop_after_write_failure(
        write_failure: Option<Failure>,
        child: &mut AsyncGroupChild,
        request_id: &str,
        tasks: (JoinHandle<Drained>, JoinHandle<Drained>),
        deadline: Instant,
    ) -> Result<(JoinHandle<Drained>, JoinHandle<Drained>), Failure> {
        if let Some(failure) = write_failure {
            ProcessReaper::terminate(child, request_id).await;
            let _settled = Drained::settle_pair(tasks.0, tasks.1, deadline).await;
            return Err(failure);
        }
        Ok(tasks)
    }

    fn apply_application(&self, command: &mut Command) {
        command.env_remove("YON_APPLICATION_JSON");
        command.env_remove("YON_RESOURCE_ENDPOINT");
        command.env_remove("YON_RESOURCE_TOKEN");
        if let Some(json) = self.options.application.handler_json() {
            command.env("YON_APPLICATION_JSON", json);
        }
    }

    fn handler_command(&self, source: &HandlerSource, adapter: &AdapterFiles) -> PlannedCommand {
        let mut planned = self.command(source, adapter);
        self.options.environment.apply(&mut planned.command);
        self.apply_application(&mut planned.command);
        planned
    }

    fn command(&self, source: &HandlerSource, adapter: &AdapterFiles) -> PlannedCommand {
        match self.options.isolation.firecracker() {
            Some(policy) => PlannedCommand {
                command: Self::firecracker_command(policy, source),
                runtime: RuntimeIdentity::Firecracker,
            },
            None => self.local_command(source, adapter),
        }
    }

    fn local_command(&self, source: &HandlerSource, adapter: &AdapterFiles) -> PlannedCommand {
        let mut command = self.language_command(source, adapter);
        command.current_dir(source.execution_working_directory());
        Self::pipe_all_streams(&mut command);
        PlannedCommand {
            command,
            runtime: RuntimeIdentity::from_source(source, &self.options.runtimes),
        }
    }

    fn language_command(&self, source: &HandlerSource, adapter: &AdapterFiles) -> Command {
        if source.language() == HandlerLanguage::Direct {
            return Self::direct_command(source);
        }
        self.adapter_command(source, adapter)
    }

    fn adapter_command(&self, source: &HandlerSource, adapter: &AdapterFiles) -> Command {
        let execution_path = source.execution_path();
        let (program, arguments) = self.adapter_program(source, adapter, &execution_path);
        let mut command = Command::new(program);
        command.args(arguments);
        command
    }

    fn adapter_program<'a>(
        &'a self,
        source: &'a HandlerSource,
        adapter: &'a AdapterFiles,
        execution_path: &'a Path,
    ) -> (&'a Path, Vec<&'a OsStr>) {
        let runner = adapter.runner.as_os_str();
        match source.language() {
            HandlerLanguage::JavaScript => (
                &self.options.runtimes.javascript,
                vec![
                    OsStr::new("--no-warnings"),
                    runner,
                    execution_path.as_os_str(),
                ],
            ),
            HandlerLanguage::TypeScript => (
                &self.options.runtimes.javascript,
                vec![runner, execution_path.as_os_str()],
            ),
            HandlerLanguage::Python => (
                &self.options.runtimes.python,
                vec![
                    OsStr::new("-I"),
                    OsStr::new("-B"),
                    runner,
                    execution_path.as_os_str(),
                    source.execution_working_directory().as_os_str(),
                ],
            ),
            HandlerLanguage::Ruby => (
                &self.options.runtimes.ruby,
                vec![
                    OsStr::new("--disable=rubyopt"),
                    OsStr::new("-E"),
                    OsStr::new("UTF-8:UTF-8"),
                    runner,
                    execution_path.as_os_str(),
                    source.execution_working_directory().as_os_str(),
                ],
            ),
            HandlerLanguage::Direct => unreachable!("handled above"),
        }
    }

    fn firecracker_command(policy: &FirecrackerIsolation, source: &HandlerSource) -> Command {
        let mut command = Command::new(policy.driver());
        policy.append_arguments(&mut command);
        command
            .arg("--project-root")
            .arg(source.execution_root())
            .arg("--source")
            .arg(source.execution_relative_path())
            .arg("--adapter")
            .arg(source.language().adapter())
            .current_dir(source.execution_root());
        Self::pipe_all_streams(&mut command);
        command
    }

    fn direct_command(source: &HandlerSource) -> Command {
        let execution_path = source.execution_path();
        source.interpreter().split_first().map_or_else(
            || Command::new(&execution_path),
            |(program, arguments)| {
                Self::interpreted_command(program, arguments, source.prebuilt(), &execution_path)
            },
        )
    }

    fn interpreted_command(
        program: &str,
        arguments: &[String],
        prebuilt: bool,
        execution_path: &Path,
    ) -> Command {
        let mut command = Command::new(program);
        command.args(arguments);
        if !prebuilt {
            command.arg(execution_path);
        }
        command
    }

    fn check_request_encodes(
        direct_protocol: bool,
        request: &HandlerRequest,
    ) -> Result<(), Failure> {
        Self::encode_request(direct_protocol, request)?;
        Ok(())
    }

    fn encode_request(direct_protocol: bool, request: &HandlerRequest) -> Result<Vec<u8>, Failure> {
        if direct_protocol {
            return serde_json::to_vec(request).map_err(|error| {
                HandlerFrame::protocol_failure(2004, &format!("Cannot encode the request: {error}"))
            });
        }
        HandlerFrame::request(request)
    }

    async fn write_request_until(
        stdin: &mut ChildStdin,
        request_bytes: &[u8],
        cancellation: &HandlerCancellation,
        deadline: Instant,
    ) -> Option<Failure> {
        tokio::select! {
            result = Self::write_request(stdin, request_bytes) => result.err(),
            () = cancellation.cancelled() => Some(ProcessFailure::cancelled()),
            () = sleep_until(deadline) => Some(ProcessFailure::timed_out()),
        }
    }

    fn stdin_kept_open(stdin: ChildStdin, direct_protocol: bool) -> Option<ChildStdin> {
        if direct_protocol {
            drop(stdin);
            return None;
        }
        Some(stdin)
    }

    async fn write_request(stdin: &mut ChildStdin, request_bytes: &[u8]) -> Result<(), Failure> {
        stdin.write_all(request_bytes).await.map_err(|error| {
            Failure::one(diagnostic(
                2101,
                format!("Cannot write Handler Protocol request: {error}"),
                Some(String::from("Retry with a healthy language runtime.")),
                None,
            ))
        })?;
        stdin.flush().await.map_err(|error| {
            Failure::one(diagnostic(
                2101,
                format!("Cannot flush Handler Protocol request: {error}"),
                None,
                None,
            ))
        })
    }

    async fn run_streaming_process(&self, process: StreamingProcess<'_>) -> Result<(), Failure> {
        let StartedProcess {
            child,
            stdin,
            stdout,
            stderr,
            resource_access,
            _adapter,
        } = self
            .start_process(
                process.source,
                process.request,
                process.deadline,
                process.cancellation,
                false,
            )
            .await?;
        let pipes = ProcessPipes {
            child,
            stdin,
            stdout,
            stderr,
        };
        let result = self.drive_streaming_process(pipes, process).await;
        Self::close_resource_access(resource_access).await;
        result
    }

    async fn drive_streaming_process(
        &self,
        pipes: ProcessPipes,
        process: StreamingProcess<'_>,
    ) -> Result<(), Failure> {
        let StreamingProcess {
            source: _,
            request,
            direct_protocol,
            deadline,
            events,
            cancellation,
        } = process;
        let request_id = &request.request_id;
        let ProcessPipes {
            mut child,
            mut stdin,
            stdout,
            stderr,
        } = pipes;
        let request_bytes = self.request_with_time_left(request, direct_protocol, deadline);
        let request_bytes =
            Self::terminate_on_failure(request_bytes, &mut child, request_id).await?;
        let stderr_task = tokio::spawn(Drained::read_all(stderr, self.options.stderr_limit));
        let write_failure = self
            .write_streaming_request(
                &mut child,
                &mut stdin,
                &request_bytes,
                request_id,
                cancellation,
                deadline,
            )
            .await;
        let stderr_task =
            Self::settle_after_stream_write_failure(write_failure, stderr_task, deadline).await?;
        let stdin = Self::stdin_kept_open(stdin, direct_protocol);
        let subscriber = events.clone();
        let mut events_task = EventReader::spawn(stdout, events, request_id.clone());
        let outcome = Self::first_stream_outcome(
            &mut events_task,
            &mut child,
            &subscriber,
            cancellation,
            deadline,
        )
        .await;
        let running = RunningStream {
            child,
            stdin,
            events_task,
            stderr_task,
            deadline,
        };
        self.finish_stream(outcome, running, request_id).await
    }

    async fn settle_after_stream_write_failure(
        write_failure: Option<Failure>,
        stderr_task: JoinHandle<Drained>,
        deadline: Instant,
    ) -> Result<JoinHandle<Drained>, Failure> {
        if let Some(failure) = write_failure {
            let _stderr = Drained::settle(stderr_task, deadline).await;
            return Err(failure);
        }
        Ok(stderr_task)
    }

    async fn first_stream_outcome(
        events_task: &mut JoinHandle<Result<StreamReadOutcome, Failure>>,
        child: &mut AsyncGroupChild,
        subscriber: &tokio::sync::mpsc::Sender<HandlerEvent>,
        cancellation: &HandlerCancellation,
        deadline: Instant,
    ) -> StreamProcessOutcome {
        tokio::select! {
            frames = events_task => StreamProcessOutcome::Frames(frames),
            status = child.inner().wait() => StreamProcessOutcome::Exit(status),
            () = subscriber.closed() => StreamProcessOutcome::SubscriberClosed,
            () = cancellation.cancelled() => StreamProcessOutcome::Cancelled,
            () = sleep_until(deadline) => StreamProcessOutcome::TimedOut,
        }
    }

    async fn finish_stream(
        &self,
        outcome: StreamProcessOutcome,
        running: RunningStream,
        request_id: &str,
    ) -> Result<(), Failure> {
        let grace = self.options.cancellation_grace;
        match outcome {
            StreamProcessOutcome::Cancelled => {
                running
                    .stop(request_id, grace, ProcessFailure::cancelled())
                    .await
            }
            StreamProcessOutcome::TimedOut => {
                running
                    .stop(request_id, grace, ProcessFailure::timed_out())
                    .await
            }
            StreamProcessOutcome::Frames(Ok(Ok(StreamReadOutcome::SubscriberClosed))) => {
                running.end_after_closed_frames(request_id).await
            }
            StreamProcessOutcome::SubscriberClosed => {
                running.end_for_closed_subscriber(request_id).await
            }
            StreamProcessOutcome::Frames(frames) => {
                running.finish_after_frames(frames, request_id).await
            }
            StreamProcessOutcome::Exit(status) => {
                running.finish_after_exit(status, request_id).await
            }
        }
    }

    async fn write_streaming_request(
        &self,
        child: &mut AsyncGroupChild,
        stdin: &mut ChildStdin,
        request_bytes: &[u8],
        request_id: &str,
        cancellation: &HandlerCancellation,
        deadline: Instant,
    ) -> Option<Failure> {
        let write_result =
            Self::write_request_in_time(stdin, request_bytes, cancellation, deadline).await;
        self.after_streaming_write(write_result, child, stdin, request_id, deadline)
            .await
    }

    async fn write_request_in_time(
        stdin: &mut ChildStdin,
        request_bytes: &[u8],
        cancellation: &HandlerCancellation,
        deadline: Instant,
    ) -> Option<Result<(), Failure>> {
        tokio::select! {
            result = Self::write_request(stdin, request_bytes) => Some(result),
            () = cancellation.cancelled() => None,
            () = sleep_until(deadline) => Some(Err(ProcessFailure::timed_out())),
        }
    }

    async fn after_streaming_write(
        &self,
        write_result: Option<Result<(), Failure>>,
        child: &mut AsyncGroupChild,
        stdin: &mut ChildStdin,
        request_id: &str,
        deadline: Instant,
    ) -> Option<Failure> {
        match write_result {
            Some(Ok(())) => None,
            Some(Err(failure)) => {
                ProcessReaper::terminate(child, request_id).await;
                Some(failure)
            }
            None => {
                ProcessReaper::cancel_and_reap(
                    child,
                    Some(stdin),
                    request_id,
                    self.options.cancellation_grace,
                    deadline,
                )
                .await;
                Some(ProcessFailure::cancelled())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        AdapterFiles, Drained, EnvironmentPolicy, HandlerSource, HandlerSupervisor,
        HandlerSupervisorOptions, ProcessFailure, ProcessReaper, RuntimeIdentity,
    };
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;
    use tokio::io::{AsyncRead, ReadBuf};
    use tokio::process::Command;
    use tokio::time::Instant;

    #[test]
    fn application_transport_cannot_inherit_an_untrusted_value() {
        let mut command = Command::new("unused");
        let names = [
            "YON_APPLICATION_JSON",
            "YON_RESOURCE_ENDPOINT",
            "YON_RESOURCE_TOKEN",
        ];
        for name in names {
            command.env(name, "untrusted");
        }
        let supervisor =
            HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
        supervisor.apply_application(&mut command);
        for reserved in names {
            let value = command
                .as_std()
                .get_envs()
                .find(|(name, _)| *name == reserved)
                .expect("explicit reserved environment removal")
                .1;
            assert_eq!(value, None);
        }
    }

    #[test]
    fn ruby_dispatch_uses_the_plain_handler_convention() {
        let runner = AdapterFiles::RUBY_RUNNER;
        assert!(runner.contains("class_name = \"Handler\""));
        assert!(runner.contains("class_name = \"Middleware\""));
        assert!(runner.trim_end().ends_with("YonRunner.main(ARGV)"));
    }

    #[test]
    fn python_dispatch_uses_the_plain_handler_convention() {
        let runner = AdapterFiles::PYTHON_RUNNER;
        assert!(runner.contains("getattr(module, class_name, None)"));
        assert!(runner.contains("return \"Handler\""));
        assert!(!runner.contains("_CONTROLLER"));
    }

    #[test]
    fn unsettled_cleanup_event_is_correlated_and_contains_no_process_output() {
        let event = ProcessReaper::cleanup_unsettled_event("request-42", Some(123), true);
        assert_eq!(event["request_id"], "request-42");
        assert_eq!(event["process_id"], 123);
        assert_eq!(event["event"], "handler.cleanup_unsettled");
        assert!(!event.to_string().contains("secret-canary"));
    }

    #[test]
    fn missing_external_runtimes_are_ty2112_and_spawn_details_are_redacted() {
        let canary = "tachyon-runtime-does-not-exist-secret-canary";
        let mut command = Command::new(canary);
        let Err(failure) = RuntimeIdentity::JavaScript { configured: true }.spawn(&mut command)
        else {
            panic!("runtime unexpectedly started");
        };
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2112"), "{rendered}");
        assert!(!rendered.contains(canary), "{rendered}");

        let mut prepared = Command::new(canary);
        let Err(prepared_failure) = RuntimeIdentity::PreparedArtifact.spawn(&mut prepared) else {
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

    fn aborted_drain_task() -> tokio::task::JoinHandle<Drained> {
        let failed = tokio::spawn(std::future::pending::<Drained>());
        failed.abort();
        failed
    }

    fn empty_drain_task() -> tokio::task::JoinHandle<Drained> {
        tokio::spawn(async {
            Drained {
                bytes: Vec::new(),
                overflow: false,
                read_error: false,
            }
        })
    }

    #[tokio::test]
    async fn a_passed_deadline_wins_over_a_free_permit() {
        let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(1));
        let cancellation = super::HandlerCancellation::default();
        let deadline = Instant::now();
        tokio::time::sleep(Duration::from_millis(5)).await;
        for _attempt in 0..64 {
            let refused = HandlerSupervisor::acquire_permit_from(&permits, &cancellation, deadline)
                .await
                .expect_err("no time left");
            assert!(refused.to_string().contains("TY2110"), "{refused}");
        }
        assert!(HandlerSupervisor::too_late_to_start(deadline, &cancellation).is_some());
        let later = Instant::now() + Duration::from_secs(5);
        assert!(HandlerSupervisor::too_late_to_start(later, &cancellation).is_none());
        cancellation.cancel();
        let cancelled =
            HandlerSupervisor::too_late_to_start(later, &cancellation).expect("cancelled");
        assert!(cancelled.to_string().contains("TY2111"), "{cancelled}");
    }

    #[tokio::test]
    async fn drain_and_task_failures_are_bounded() {
        let drained = Drained::read_all(BrokenReader, 8).await;
        assert!(drained.read_error);
        assert!(!drained.overflow);

        let failed = aborted_drain_task();
        let healthy = empty_drain_task();
        let failure =
            Drained::settle_pair(failed, healthy, Instant::now() + Duration::from_secs(1))
                .await
                .expect_err("join failure");
        assert!(failure.to_string().contains("TY2101"));

        let healthy = empty_drain_task();
        let failed = aborted_drain_task();
        let failure =
            Drained::settle_pair(healthy, failed, Instant::now() + Duration::from_secs(1))
                .await
                .expect_err("join failure");
        assert!(failure.to_string().contains("TY2101"));

        let pending_stdout = tokio::spawn(std::future::pending::<Drained>());
        let pending_stderr = tokio::spawn(std::future::pending::<Drained>());
        let failure = Drained::settle_pair(
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
            AdapterFiles::io_failure(&io::Error::other("temporary"))
                .to_string()
                .contains("TY2101")
        );
        assert!(
            ProcessFailure::missing_pipe("stdout")
                .to_string()
                .contains("TY2101")
        );
        assert_eq!(
            Drained::bounded_sideband(b"visible\x1b[31m\nline"),
            "visible[31m\nline"
        );
    }

    struct FailingStream {
        extension: &'static str,
        program: &'static str,
        version_flag: &'static str,
        source: &'static str,
    }

    impl FailingStream {
        const CASES: [Self; 9] = [
            Self {
                extension: "kt",
                program: "kotlinc",
                version_flag: "-version",
                source: "@RequestRoute(\"/events\")\n@Public\nclass EventsController {\n    @StreamRoute\n    fun streamEvents() = sequence {\n        yield(mapOf(\"tick\" to 1))\n        throw IllegalStateException(\"secret-canary\")\n    }\n}\n",
            },
            Self {
                extension: "cs",
                program: "dotnet",
                version_flag: "--version",
                source: "[RequestRoute(\"/events\")]\n[Public]\nclass EventsController {\n    [StreamRoute]\n    public System.Collections.Generic.IEnumerable<object> StreamEvents() {\n        yield return new { tick = 1 };\n        throw new System.InvalidOperationException(\"secret-canary\");\n    }\n}\n",
            },
            Self {
                extension: "php",
                program: "php-cgi",
                version_flag: "--version",
                source: "<?php\n#[RequestRoute(\"/events\")]\n#[Public]\nclass EventsController {\n    #[StreamRoute]\n    public function streamEvents() {\n        yield ['tick' => 1];\n        throw new RuntimeException('secret-canary');\n    }\n}\n",
            },
            Self {
                extension: "dart",
                program: "dart",
                version_flag: "--version",
                source: "@RequestRoute(\"/events\")\n@Public\nclass EventsController {\n  @StreamRoute\n  Stream<Object?> streamEvents() async* {\n    yield {'tick': 1};\n    throw StateError('secret-canary');\n  }\n}\n",
            },
            Self {
                extension: "js",
                program: "node",
                version_flag: "--version",
                source: "@RequestRoute(\"/events\")\n@Public\nclass EventsController {\n  @StreamRoute\n  *streamEvents() {\n    yield { tick: 1 }\n    throw new Error('secret-canary')\n  }\n}\n",
            },
            Self {
                extension: "py",
                program: "python3",
                version_flag: "--version",
                source: "@request_route('/events')\n@public\nclass EventsController:\n    @stream_route\n    def stream(self, request):\n        yield {\"tick\": 1}\n        raise RuntimeError(\"secret-canary\")\n",
            },
            Self {
                extension: "rb",
                program: "ruby",
                version_flag: "--version",
                source: "#[RequestRoute(\"/events\")]\n#[Public]\nclass EventsController\n  #[StreamRoute]\n  def stream_events\n    yield({ \"tick\" => 1 })\n    raise \"secret-canary\"\n  end\nend\n",
            },
            Self {
                extension: "go",
                program: "go",
                version_flag: "version",
                source: "import (\n\t\"errors\"\n\t\"iter\"\n)\n\n// @RequestRoute(\"/events\")\n// @Public\ntype EventsController struct{}\n\n// @StreamRoute\nfunc (controller *EventsController) StreamEvents() iter.Seq2[map[string]int, error] {\n\treturn func(yield func(map[string]int, error) bool) {\n\t\tif yield(map[string]int{\"tick\": 1}, nil) {\n\t\t\tyield(nil, errors.New(\"secret-canary\"))\n\t\t}\n\t}\n}\n",
            },
            Self {
                extension: "rs",
                program: "cargo",
                version_flag: "--version",
                source: "#[RequestRoute(\"/events\")]\n#[Public]\nstruct EventsController;\nimpl EventsController {\n    fn new() -> Self { Self }\n    #[StreamRoute]\n    fn stream_events(&self) -> impl Iterator<Item = Result<Json, String>> {\n        vec![Ok(Json::map([(\"tick\", Json::from(1_i64))])), Err(String::from(\"secret-canary\"))].into_iter()\n    }\n}\n",
            },
        ];

        fn available(&self) -> bool {
            std::process::Command::new(self.program)
                .arg(self.version_flag)
                .output()
                .is_ok_and(|output| output.status.success())
        }

        async fn protocol_output(&self) -> String {
            let root = tempfile::tempdir().expect("project");
            let relative = format!("server/controllers/EventsController.{}", self.extension);
            let path = root.path().join(&relative);
            std::fs::create_dir_all(path.parent().expect("controllers")).expect("controllers");
            std::fs::write(&path, self.source).expect("controller");
            let source = HandlerSource::discover(root.path(), &relative).expect("stream source");
            let supervisor =
                HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
            let adapter = AdapterFiles::materialize(source.language()).expect("adapter");
            let mut planned = supervisor.command(&source, &adapter);
            let direct = supervisor.uses_direct_protocol(&source);
            let mut request = tachyon_contracts::HandlerRequest::route(
                "events",
                "/events",
                tachyon_contracts::HttpMethod::Get,
            );
            request.deadline_ms = Some(60_000);
            let bytes = HandlerSupervisor::encode_request(direct, &request).expect("request");
            let mut child = planned.command.spawn().expect("handler starts");
            let mut stdin = child.stdin.take().expect("stdin");
            tokio::io::AsyncWriteExt::write_all(&mut stdin, &bytes)
                .await
                .expect("request written");
            let _open = HandlerSupervisor::stdin_kept_open(stdin, direct);
            let mut stdout = child.stdout.take().expect("stdout");
            let mut output = Vec::new();
            tokio::time::timeout(
                Duration::from_mins(2),
                tokio::io::AsyncReadExt::read_to_end(&mut stdout, &mut output),
            )
            .await
            .expect("handler finishes")
            .expect("stdout read");
            String::from_utf8_lossy(&output).into_owned()
        }
    }

    #[tokio::test]
    async fn stream_failures_send_a_fixed_error_without_the_exception_text() {
        let mut wrong = Vec::new();
        for case in FailingStream::CASES {
            if !case.available() {
                eprintln!(
                    "Skipping {} stream failure: {} unavailable",
                    case.extension, case.program
                );
                continue;
            }
            let output = case.protocol_output().await;
            let fixed = output.contains("tick")
                && output.contains("TY2204")
                && output.contains("The handler stream failed.")
                && !output.contains("secret-canary");
            if !fixed {
                wrong.push(format!("{}: {output}", case.extension));
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
