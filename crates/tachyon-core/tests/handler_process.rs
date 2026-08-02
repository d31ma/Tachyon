//! Phase 2 real-process contract corpus shared by JavaScript and Python.

#![allow(clippy::expect_used)]

use std::fs;
use std::process::Command as StdCommand;
#[cfg(unix)]
use std::process::Stdio;
use std::time::Duration;
use tachyon_contracts::{
    HandlerBody, HandlerBodyEncoding, HandlerRequest, HandlerResponse, HttpMethod,
};
use tachyon_core::{
    EnvironmentPolicy, HandlerCancellation, HandlerSource, HandlerSupervisor,
    HandlerSupervisorOptions,
};

fn source(root: &tempfile::TempDir, extension: &str, contents: &str) -> HandlerSource {
    let relative = format!("server/routes/contract/yon.{extension}");
    let path = root.path().join(&relative);
    fs::create_dir_all(path.parent().expect("handler parent")).expect("handler directory");
    fs::write(path, contents).expect("handler source");
    HandlerSource::discover(root.path(), &relative).expect("validated source")
}

fn request(id: &str, method: HttpMethod, body: Option<&str>) -> HandlerRequest {
    let mut request = HandlerRequest::route(id, "/contract", method);
    request.headers.insert(
        String::from("x-contract"),
        vec![String::from("shared"), String::from("second")],
    );
    request.body = body.map(|data| HandlerBody {
        encoding: HandlerBodyEncoding::Utf8,
        data: String::from(data),
    });
    request
}

fn body_json(response: &HandlerResponse) -> serde_json::Value {
    let body = response.body.as_ref().expect("successful body");
    serde_json::from_str(&body.data).expect("JSON handler result")
}

fn supervisor(options: HandlerSupervisorOptions) -> HandlerSupervisor {
    HandlerSupervisor::new(options).expect("valid supervisor")
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    StdCommand::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    StdCommand::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")))
}

async fn assert_process_gone(pid: u32) {
    for _attempt in 0..100 {
        if !process_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("handler descendant {pid} survived process-group cleanup");
}

fn descendant_request(id: &str, mode: &str, pid_file: &std::path::Path) -> HandlerRequest {
    let mut request = request(id, HttpMethod::Get, Some(mode));
    request.headers.insert(
        String::from("x-pid-file"),
        vec![pid_file.to_string_lossy().into_owned()],
    );
    request
}

fn descendant_pid(pid_file: &std::path::Path) -> u32 {
    fs::read_to_string(pid_file)
        .expect("descendant pid file")
        .trim()
        .parse()
        .expect("descendant pid")
}

#[tokio::test]
async fn javascript_and_python_share_request_async_unicode_and_sideband_behavior() {
    let javascript = r"
export class Handler {
  static async POST(request) {
    console.log('sideband-only')
    await new Promise((resolve) => setTimeout(resolve, 5))
    return {
      method: request.method,
      headers: request.headers['x-contract'],
      body: request.body.data,
      unicode: 'héllø 🌍',
    }
  }
}
";
    let python = r#"
import asyncio

class Handler:
    @staticmethod
    async def POST(request):
        print("sideband-only")
        await asyncio.sleep(0.005)
        return {
            "method": request["method"],
            "headers": request["headers"]["x-contract"],
            "body": request["body"]["data"],
            "unicode": "héllø 🌍",
        }
"#;

    for (extension, contents) in [("js", javascript), ("py", python)] {
        let root = tempfile::tempdir().expect("project");
        let source = source(&root, extension, contents);
        let response = supervisor(HandlerSupervisorOptions::default())
            .invoke(
                &source,
                &request("shared_request", HttpMethod::Post, Some("payload")),
                &HandlerCancellation::default(),
            )
            .await
            .expect("handler response");
        assert_eq!(response.status, 200);
        assert!(response.error.is_none());
        let value = body_json(&response);
        assert_eq!(value["method"], "POST");
        assert_eq!(value["headers"][0], "shared");
        assert_eq!(value["headers"][1], "second");
        assert_eq!(value["body"], "payload");
        assert_eq!(value["unicode"], "héllø 🌍");
    }
}

#[tokio::test]
async fn environment_is_denied_by_default_and_explicitly_allowlisted() {
    let candidates = ["HOME", "USERPROFILE", "CARGO_HOME", "CI"];
    let (name, expected) = candidates
        .iter()
        .find_map(|name| std::env::var(name).ok().map(|value| (*name, value)))
        .expect("test host needs one non-baseline environment variable");
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "js",
        r"
export class Handler {
  static GET(request) {
    const name = request.body.data
    return { present: Object.hasOwn(process.env, name), value: process.env[name] ?? null }
  }
}
",
    );
    let invocation = request("environment", HttpMethod::Get, Some(name));
    let denied = supervisor(HandlerSupervisorOptions::default())
        .invoke(&source, &invocation, &HandlerCancellation::default())
        .await
        .expect("denied environment response");
    assert_eq!(body_json(&denied)["present"], false);

    let environment = EnvironmentPolicy::from_names([name]).expect("allowlist");
    let allowed = supervisor(HandlerSupervisorOptions {
        environment,
        ..HandlerSupervisorOptions::default()
    })
    .invoke(&source, &invocation, &HandlerCancellation::default())
    .await
    .expect("allowed environment response");
    let allowed = body_json(&allowed);
    assert_eq!(allowed["present"], true);
    assert_eq!(allowed["value"], expected);
}

#[tokio::test]
async fn adapter_authoring_failures_are_bounded_protocol_responses() {
    let root = tempfile::tempdir().expect("project");
    let javascript = source(
        &root,
        "js",
        "export class Handler { static GET() { return { ok: true } } }",
    );
    let missing = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &javascript,
            &request("missing", HttpMethod::Delete, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("protocol error response");
    assert_eq!(missing.status, 405);
    assert_eq!(missing.error.expect("error").code, "TY2202");

    let python = source(
        &root,
        "py",
        r#"
class Handler:
    @staticmethod
    def GET(request):
        raise RuntimeError("bounded failure")
"#,
    );
    let failed = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &python,
            &request("failed", HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("protocol error response");
    let error = failed.error.expect("error");
    assert_eq!(error.code, "TY2201");
    assert_eq!(error.message, "bounded failure");
    assert!(!error.message.contains("Traceback"));

    let instance = source(
        &root,
        "py",
        r"
class Handler:
    def GET(self):
        return {'invalid': True}
",
    );
    let rejected = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &instance,
            &request("instance", HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("authoring response");
    assert_eq!(rejected.error.expect("error").code, "TY2202");

    let missing_class = source(&root, "js", "export const notAHandler = true");
    let rejected = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &missing_class,
            &request("missing_class", HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("adapter load response");
    assert_eq!(rejected.error.expect("error").code, "TY2201");

    let invalid_python = source(&root, "py", "class Handler\n    pass");
    let rejected = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &invalid_python,
            &request("invalid_python", HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("adapter syntax response");
    assert_eq!(rejected.error.expect("error").code, "TY2201");

    let non_serializable = source(
        &root,
        "js",
        "export class Handler { static GET() { return 1n } }",
    );
    let rejected = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &non_serializable,
            &request("non_serializable", HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("serialization response");
    assert_eq!(rejected.error.expect("error").code, "TY2203");
}

#[tokio::test]
async fn timeout_cancellation_crash_and_recovery_reap_each_process() {
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "js",
        r"
export class Handler {
  static async GET(request) {
    const mode = request.body?.data
    if (mode === 'crash') process.exit(23)
    if (mode === 'block') while (true) {}
    if (mode === 'sleep') await new Promise((resolve) => setTimeout(resolve, 5000))
    return { mode: mode ?? 'ok' }
  }
}
",
    );
    let bounded = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_millis(75),
        ..HandlerSupervisorOptions::default()
    });
    let already_cancelled = HandlerCancellation::default();
    already_cancelled.cancel();
    let early = bounded
        .invoke(
            &source,
            &request("early_cancel", HttpMethod::Get, None),
            &already_cancelled,
        )
        .await
        .expect_err("early cancellation");
    assert!(early.to_string().contains("TY2111"));

    let timed_out = bounded
        .invoke(
            &source,
            &request("timeout", HttpMethod::Get, Some("sleep")),
            &HandlerCancellation::default(),
        )
        .await
        .expect_err("timeout");
    assert!(timed_out.to_string().contains("TY2110"));

    let forced = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_millis(40),
        cancellation_grace: Duration::from_millis(20),
        ..HandlerSupervisorOptions::default()
    })
    .invoke(
        &source,
        &request("forced", HttpMethod::Get, Some("block")),
        &HandlerCancellation::default(),
    )
    .await
    .expect_err("forced termination");
    assert!(forced.to_string().contains("TY2110"));

    let cancellation = HandlerCancellation::default();
    let trigger = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(40)).await;
        trigger.cancel();
    });
    let cancelled = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &source,
            &request("cancel", HttpMethod::Get, Some("sleep")),
            &cancellation,
        )
        .await
        .expect_err("cancellation");
    assert!(cancelled.to_string().contains("TY2111"));
    assert!(cancellation.is_cancelled());

    let healthy = supervisor(HandlerSupervisorOptions::default());
    let crashed = healthy
        .invoke(
            &source,
            &request("crash", HttpMethod::Get, Some("crash")),
            &HandlerCancellation::default(),
        )
        .await
        .expect_err("crash");
    assert!(crashed.to_string().contains("TY2104"));
    let recovered = healthy
        .invoke(
            &source,
            &request("recovered", HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("fresh process after crash");
    assert_eq!(body_json(&recovered)["mode"], "ok");
}

#[tokio::test]
async fn inheriting_descendants_are_bounded_and_reaped_for_every_outcome() {
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "js",
        r"
import { spawn } from 'node:child_process'
import { writeFileSync } from 'node:fs'

export class Handler {
  static async GET(request) {
    const descendant = spawn(
      process.execPath,
      ['-e', 'setInterval(() => {}, 1000)'],
      { stdio: 'inherit' },
    )
    writeFileSync(request.headers['x-pid-file'][0], String(descendant.pid))
    if (request.body.data !== 'success') await new Promise(() => {})
    return { descendant: descendant.pid }
  }
}
",
    );

    let success_pid_file = root.path().join("success.pid");
    let success_supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_millis(750),
        cancellation_grace: Duration::from_millis(25),
        ..HandlerSupervisorOptions::default()
    });
    let started = tokio::time::Instant::now();
    let success = tokio::time::timeout(
        Duration::from_secs(2),
        success_supervisor.invoke(
            &source,
            &descendant_request("descendant_success", "success", &success_pid_file),
            &HandlerCancellation::default(),
        ),
    )
    .await
    .expect("successful invocation stayed bounded")
    .expect("successful handler response");
    assert!(started.elapsed() < Duration::from_millis(750));
    let success_pid = descendant_pid(&success_pid_file);
    assert_eq!(body_json(&success)["descendant"], success_pid);
    assert_process_gone(success_pid).await;

    let timeout_pid_file = root.path().join("timeout.pid");
    let timeout_supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_millis(250),
        cancellation_grace: Duration::from_millis(25),
        ..HandlerSupervisorOptions::default()
    });
    let timed_out = tokio::time::timeout(
        Duration::from_secs(2),
        timeout_supervisor.invoke(
            &source,
            &descendant_request("descendant_timeout", "hang", &timeout_pid_file),
            &HandlerCancellation::default(),
        ),
    )
    .await
    .expect("timed-out invocation stayed bounded")
    .expect_err("handler deadline");
    assert!(timed_out.to_string().contains("TY2110"));
    let timeout_pid = descendant_pid(&timeout_pid_file);
    assert_process_gone(timeout_pid).await;

    let cancellation_pid_file = root.path().join("cancel.pid");
    let cancellation = HandlerCancellation::default();
    let trigger = cancellation.clone();
    let trigger_pid_file = cancellation_pid_file.clone();
    tokio::spawn(async move {
        for _attempt in 0..100 {
            if trigger_pid_file.is_file() {
                trigger.cancel();
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        trigger.cancel();
    });
    let cancelled = tokio::time::timeout(
        Duration::from_secs(2),
        supervisor(HandlerSupervisorOptions {
            default_timeout: Duration::from_secs(1),
            cancellation_grace: Duration::from_millis(25),
            ..HandlerSupervisorOptions::default()
        })
        .invoke(
            &source,
            &descendant_request("descendant_cancel", "hang", &cancellation_pid_file),
            &cancellation,
        ),
    )
    .await
    .expect("cancelled invocation stayed bounded")
    .expect_err("handler cancellation");
    assert!(cancelled.to_string().contains("TY2111"));
    let cancellation_pid = descendant_pid(&cancellation_pid_file);
    assert_process_gone(cancellation_pid).await;
}

#[tokio::test]
async fn concurrency_limit_includes_queue_time_in_each_request_deadline() {
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "js",
        r"
export class Handler {
  static async GET() {
    await new Promise((resolve) => setTimeout(resolve, 150))
    return { completed: true }
  }
}
",
    );
    let supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_secs(5),
        max_concurrency: 1,
        ..HandlerSupervisorOptions::default()
    });

    let first_supervisor = supervisor.clone();
    let first_source = source.clone();
    let first = tokio::spawn(async move {
        first_supervisor
            .invoke(
                &first_source,
                &request("first", HttpMethod::Get, None),
                &HandlerCancellation::default(),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(25)).await;

    let mut queued_request = request("queued", HttpMethod::Get, None);
    queued_request.deadline_ms = Some(40);
    let queued = supervisor
        .invoke(&source, &queued_request, &HandlerCancellation::default())
        .await
        .expect_err("queued request deadline");
    assert!(queued.to_string().contains("TY2110"));

    let completed = first.await.expect("first task").expect("first response");
    assert_eq!(body_json(&completed)["completed"], true);
}

#[tokio::test]
async fn protocol_smuggling_oversize_mismatch_and_stderr_flood_fail_closed() {
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "js",
        r"
export class Handler {
  static async GET(request) {
    const mode = request.body.data
    if (mode === 'trailing') {
      process.stdout.write('smuggled')
      return { ok: true }
    }
    if (mode === 'oversized') {
      await new Promise((resolve) => {
        process.stdout.write(Buffer.alloc(16 * 1024 * 1024 + 5), () => process.exit(0))
      })
    }
    if (mode === 'mismatch') {
      const payload = Buffer.from(JSON.stringify({
        protocol_version: 1,
        kind: 'response',
        request_id: 'another_request',
        status: 200,
        headers: {},
        body: { encoding: 'utf8', data: 'null' },
      }))
      const prefix = Buffer.alloc(4)
      prefix.writeUInt32BE(payload.length)
      await new Promise((resolve) => {
        process.stdout.write(Buffer.concat([prefix, payload]), () => process.exit(0))
      })
    }
    if (mode === 'stderr') {
      await new Promise((resolve) => process.stderr.write('x'.repeat(70 * 1024), resolve))
      return { ok: true }
    }
    return { ok: true }
  }
}
",
    );
    let supervisor = supervisor(HandlerSupervisorOptions::default());
    for (mode, code) in [
        ("trailing", "TY2103"),
        ("oversized", "TY2103"),
        ("mismatch", "TY2102"),
        ("stderr", "TY2107"),
    ] {
        let failure = supervisor
            .invoke(
                &source,
                &request(mode, HttpMethod::Get, Some(mode)),
                &HandlerCancellation::default(),
            )
            .await
            .expect_err("protocol failure");
        assert!(failure.to_string().contains(code), "{mode}: {failure}");
    }
}
