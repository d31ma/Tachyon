//! Process-level signal lifecycle acceptance tests.

#![allow(clippy::expect_used)]

#[cfg(unix)]
mod unix {
    use serde_json::Value;
    use std::fs;
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::{TcpListener, TcpStream};
    use std::process::{Child, Command, Stdio};
    use std::sync::Mutex;
    use std::sync::mpsc::{self, Receiver};
    use std::time::{Duration, Instant};

    static PROCESS_TEST: Mutex<()> = Mutex::new(());

    fn serialize_process_test() -> std::sync::MutexGuard<'static, ()> {
        PROCESS_TEST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct RunningChild {
        child: Child,
    }

    impl RunningChild {
        fn spawn_command(mut command: Command) -> (Self, Receiver<String>, Receiver<String>) {
            let mut child = command
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("long-running command should start");
            let stdout = lines(child.stdout.take().expect("command stdout"));
            let stderr = lines(child.stderr.take().expect("command stderr"));
            (Self { child }, stdout, stderr)
        }

        fn spawn(
            project: &std::path::Path,
            watch: bool,
        ) -> (Self, Receiver<String>, Receiver<String>) {
            let mut command = Command::new(env!("CARGO_BIN_EXE_ty"));
            command
                .arg("serve")
                .arg(project)
                .args(["--host", "127.0.0.1", "--port", "0"]);
            if !watch {
                command.arg("--no-watch");
            }
            Self::spawn_command(command)
        }

        fn signal(&self, signal: &str) {
            let status = Command::new("kill")
                .arg(format!("-{signal}"))
                .arg(self.child.id().to_string())
                .status()
                .expect("the Unix kill utility should run");
            assert!(status.success(), "signal should be delivered");
        }

        fn wait_bounded(&mut self) -> std::process::ExitStatus {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = self.child.try_wait().expect("child status") {
                    return status;
                }
                assert!(Instant::now() < deadline, "server did not stop within 10s");
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    impl Drop for RunningChild {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn lines<R>(reader: R) -> Receiver<String>
    where
        R: std::io::Read + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else {
                    return;
                };
                if sender.send(line).is_err() {
                    return;
                }
            }
        });
        receiver
    }

    fn project() -> tempfile::TempDir {
        let project = tempfile::tempdir().expect("project directory");
        let page = project.path().join("client/pages/tac.html");
        fs::create_dir_all(page.parent().expect("page parent")).expect("page directory");
        fs::write(page, "<main aria-label=\"Signal\"><h1>Signal</h1></main>").expect("page source");
        project
    }

    fn event(receiver: &Receiver<String>) -> Value {
        let line = receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("lifecycle event within 30s");
        serde_json::from_str(&line).expect("lifecycle event should be JSON")
    }

    fn line_containing(receiver: &Receiver<String>, expected: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let line = receiver
                .recv_timeout(remaining)
                .unwrap_or_else(|_| panic!("output containing {expected:?} within 30s"));
            if line.contains(expected) {
                return line;
            }
        }
    }

    fn assert_graceful_signal(delivery_name: &str, event_name: &str, event_code: i64) {
        let project = project();
        let (mut server, stdout, stderr) = RunningChild::spawn(project.path(), false);

        let startup = event(&stderr);
        assert_eq!(startup["event"], "runtime.signal_handlers_ready");
        assert_eq!(startup["command"], "serve");
        assert_eq!(
            startup["installed"],
            serde_json::json!(["SIGINT", "SIGTERM"])
        );

        let ready = stdout
            .recv_timeout(Duration::from_secs(10))
            .expect("readiness line within 10s");
        assert!(ready.contains("Tachyon server ready at http://127.0.0.1:"));

        server.signal(delivery_name);
        let receipt = event(&stderr);
        assert_eq!(receipt["event"], "runtime.shutdown_requested");
        assert_eq!(receipt["signal"], event_name);
        assert_eq!(receipt["signal_code"], event_code);
        assert!(server.wait_bounded().success());
    }

    #[test]
    fn sigterm_is_reported_and_stops_the_server_gracefully() {
        let _serial = serialize_process_test();
        assert_graceful_signal("TERM", "SIGTERM", 15);
    }

    #[test]
    fn sigint_is_reported_and_stops_the_server_gracefully() {
        let _serial = serialize_process_test();
        assert_graceful_signal("INT", "SIGINT", 2);
    }

    fn assert_command_shutdown(command: Command, command_name: &str, announcement: &str) {
        let (mut running, stdout, stderr) = RunningChild::spawn_command(command);
        let startup = event(&stderr);
        assert_eq!(startup["event"], "runtime.signal_handlers_ready");
        assert_eq!(startup["command"], command_name);
        let ready = stdout
            .recv_timeout(Duration::from_secs(10))
            .expect("command announcement within 10s");
        assert!(ready.contains(announcement), "{ready}");
        running.signal("TERM");
        let receipt = event(&stderr);
        assert_eq!(receipt["command"], command_name);
        assert_eq!(receipt["signal"], "SIGTERM");
        assert!(running.wait_bounded().success());
    }

    #[test]
    fn bundle_watch_uses_the_shared_signal_lifecycle() {
        let _serial = serialize_process_test();
        let project = project();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ty"));
        command
            .arg("bundle")
            .arg(project.path())
            .args(["--watch", "--skip-initial-build"]);
        assert_command_shutdown(command, "bundle.watch", "Watching sources");
    }

    #[test]
    fn preview_uses_the_shared_signal_lifecycle() {
        let _serial = serialize_process_test();
        let project = project();
        let bundled = Command::new(env!("CARGO_BIN_EXE_ty"))
            .arg("bundle")
            .arg(project.path())
            .output()
            .expect("bundle command should run");
        assert!(
            bundled.status.success(),
            "{}",
            String::from_utf8_lossy(&bundled.stderr)
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_ty"));
        command
            .arg("preview")
            .arg(project.path())
            .args(["--host", "127.0.0.1", "--port", "0"]);
        assert_command_shutdown(command, "preview", "Tachyon preview ready");
    }

    #[test]
    fn preview_watch_cancels_and_joins_its_rebuilder_before_exit() {
        let _serial = serialize_process_test();
        let project = project();
        let mut command = Command::new(env!("CARGO_BIN_EXE_ty"));
        command.arg("preview").arg(project.path()).args([
            "--watch",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
        ]);
        let (mut running, stdout, stderr) = RunningChild::spawn_command(command);

        let startup = event(&stderr);
        assert_eq!(startup["event"], "runtime.signal_handlers_ready");
        assert_eq!(startup["command"], "preview");
        let ready = line_containing(&stdout, "Tachyon preview ready");
        let address = ready
            .split_whitespace()
            .find(|part| part.starts_with("http://"))
            .expect("readiness address")
            .trim_end_matches('/')
            .trim_start_matches("http://")
            .to_owned();
        let announcement = line_containing(&stdout, "Watching sources");
        assert!(announcement.contains("rebuilding the preview"));
        fs::write(
            project.path().join("client/pages/tac.html"),
            "<main><h1>Watcher rebuild before shutdown</h1></main>",
        )
        .expect("pre-signal source change");
        let rebuilt = line_containing(&stdout, "Built");
        assert!(rebuilt.contains("dist/web"), "{rebuilt}");
        let published_path = project.path().join("dist/web/index.html");
        let published = fs::read(&published_path).expect("initial preview bundle");
        assert!(
            String::from_utf8_lossy(&published).contains("Watcher rebuild before shutdown"),
            "the watcher should publish its pre-signal rebuild"
        );

        running.signal("TERM");
        let receipt = event(&stderr);
        assert_eq!(receipt["event"], "runtime.shutdown_requested");
        assert_eq!(receipt["command"], "preview");
        assert_eq!(receipt["signal"], "SIGTERM");
        fs::write(
            project.path().join("client/pages/tac.html"),
            "<main><h1>Must not publish after shutdown</h1></main>",
        )
        .expect("post-signal source change");

        assert!(running.wait_bounded().success());
        let listener = TcpListener::bind(&address).expect("preview listener should be released");
        drop(listener);
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(
            fs::read(published_path).expect("published preview should remain readable"),
            published,
            "the joined watcher must not publish a post-signal rebuild"
        );
    }

    fn ready_address(receiver: &Receiver<String>) -> String {
        let ready = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("readiness line within 10s");
        ready
            .split_whitespace()
            .find(|part| part.starts_with("http://"))
            .expect("readiness address")
            .trim_end_matches('/')
            .trim_start_matches("http://")
            .to_owned()
    }

    fn probe(address: &str) {
        let mut stream = TcpStream::connect(address).expect("server should accept a probe");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("probe read timeout");
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("probe request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("probe response");
        assert!(response.contains("200 OK"), "{response}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn three_supervised_servers_remain_healthy_for_sixty_seconds() {
        let _serial = serialize_process_test();
        let projects = (0..3).map(|_| project()).collect::<Vec<_>>();
        let mut servers = Vec::new();
        for project in &projects {
            let (server, stdout, stderr) = RunningChild::spawn(project.path(), true);
            let startup = event(&stderr);
            assert_eq!(startup["event"], "runtime.signal_handlers_ready");
            let address = ready_address(&stdout);
            probe(&address);
            servers.push((server, stderr, address));
        }

        let started = Instant::now();
        let mut midpoint_probed = false;
        while started.elapsed() < Duration::from_mins(1) {
            for (server, _, _) in &mut servers {
                assert!(
                    server.child.try_wait().expect("server status").is_none(),
                    "supervised server exited during idle observation"
                );
            }
            if !midpoint_probed && started.elapsed() >= Duration::from_secs(30) {
                for (_, _, address) in &servers {
                    probe(address);
                }
                midpoint_probed = true;
            }
            std::thread::sleep(Duration::from_millis(250));
        }

        for (server, stderr, address) in &mut servers {
            probe(address);
            server.signal("TERM");
            let receipt = event(stderr);
            assert_eq!(receipt["signal"], "SIGTERM");
            assert!(server.wait_bounded().success());
        }
    }
}

#[cfg(windows)]
#[test]
fn ctrl_break_is_reported_and_stops_the_server_gracefully() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/windows/signal-lifecycle.py");
    let output = std::process::Command::new("python")
        .arg(script)
        .arg(env!("CARGO_BIN_EXE_ty"))
        .output()
        .expect("Windows signal lifecycle script should run");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
