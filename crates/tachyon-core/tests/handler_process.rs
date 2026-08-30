//! Phase 2 real-process contract corpus shared by JavaScript and Python.

#![allow(clippy::expect_used)]

use std::fs;
#[cfg(unix)]
use std::path::Path;
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
    let contents = match extension {
        "js" | "ts" => contents.replace(
            "export class Handler",
            "@Controller\nexport class ContractController",
        ),
        "py" => contents.replace("class Handler", "@Controller\nclass ContractController"),
        _ => String::from(contents),
    };
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

fn available(program: &str) -> bool {
    StdCommand::new(program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
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

#[cfg(unix)]
#[tokio::test]
async fn interpreted_handlers_execute_owned_dependencies_after_the_project_root_is_swapped() {
    use std::os::unix::fs::symlink;

    let mut cases = vec![
        (
            "js",
            "import { origin } from './origin.js'\n@Controller\nexport class ContractController {\n  static GET() { return { origin } }\n}\n",
            "export const origin = 'owned-js'\n",
            "import { origin } from './origin.js'\n@Controller\nexport class ContractController {\n  static GET() { return { origin } }\n}\n",
            "export const origin = 'planted-js'\n",
            "owned-js",
        ),
        (
            "py",
            "from origin import value\n@Controller\nclass ContractController:\n    @staticmethod\n    def GET(request):\n        return {'origin': value}\n",
            "value = 'owned-py'\n",
            "from origin import value\n@Controller\nclass ContractController:\n    @staticmethod\n    def GET(request):\n        return {'origin': value}\n",
            "value = 'planted-py'\n",
            "owned-py",
        ),
    ];
    if available("bun") {
        cases.push((
            "ts",
            "import { origin } from './origin.ts'\n@Controller\nexport class ContractController {\n  static GET() { return { origin } }\n}\n",
            "export const origin: string = 'owned-ts'\n",
            "import { origin } from './origin.ts'\n@Controller\nexport class ContractController {\n  static GET() { return { origin } }\n}\n",
            "export const origin: string = 'planted-ts'\n",
            "owned-ts",
        ));
    }

    for (extension, owned, owned_dependency, planted, planted_dependency, expected) in cases {
        let workspace = tempfile::tempdir().expect("workspace");
        let project = workspace.path().join(format!("project-{extension}"));
        let relative = format!("server/routes/contract/yon.{extension}");
        let authored = project.join(&relative);
        fs::create_dir_all(authored.parent().expect("handler parent")).expect("handler root");
        fs::write(&authored, owned).expect("owned source");
        fs::write(
            authored
                .parent()
                .expect("handler parent")
                .join(format!("origin.{extension}")),
            owned_dependency,
        )
        .expect("owned dependency");
        let source = HandlerSource::discover(&project, &relative).expect("snapshot source");

        let retained = workspace.path().join(format!("retained-{extension}"));
        fs::rename(&project, &retained).expect("move opened project");
        let outside = tempfile::tempdir().expect("planted project");
        let planted_source = outside.path().join(&relative);
        fs::create_dir_all(planted_source.parent().expect("planted parent")).expect("planted root");
        fs::write(&planted_source, planted).expect("planted source");
        fs::write(
            planted_source
                .parent()
                .expect("planted parent")
                .join(format!("origin.{extension}")),
            planted_dependency,
        )
        .expect("planted dependency");
        symlink(outside.path(), &project).expect("ambient project replacement");

        let response = supervisor(HandlerSupervisorOptions::default())
            .invoke(
                &source,
                &request(
                    &format!("owned_snapshot_{extension}"),
                    HttpMethod::Get,
                    None,
                ),
                &HandlerCancellation::default(),
            )
            .await
            .expect("owned handler response");
        assert_eq!(body_json(&response)["origin"], expected, "{extension}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn relay_executes_the_owned_delegate_after_the_project_root_is_swapped() {
    use std::os::unix::fs::symlink;

    let workspace = tempfile::tempdir().expect("workspace");
    let project = workspace.path().join("project-relay");
    let handler = project.join("server/routes/contract/yon.py");
    let delegate = project.join("server/delegates/origin.py");
    fs::create_dir_all(handler.parent().expect("handler parent")).expect("handler root");
    fs::create_dir_all(delegate.parent().expect("delegate parent")).expect("delegate root");
    fs::write(
        &handler,
        "@Controller\nclass ContractController:\n    @staticmethod\n    def GET(request):\n        return OriginDelegate.GET(request)\n\n@Delegate\nclass OriginDelegate:\n    @staticmethod\n    @Relay('python3', 'server/delegates/origin.py')\n    def GET(request):\n        raise RuntimeError('placeholder')\n",
    )
    .expect("handler");
    fs::write(
        &delegate,
        "import json\nprint(json.dumps({'status': 200, 'headers': {'content-type': ['application/json']}, 'body': json.dumps({'origin': 'owned-relay'})}))\n",
    )
    .expect("owned delegate");
    let source = HandlerSource::discover(&project, "server/routes/contract/yon.py")
        .expect("snapshot source");

    let retained = workspace.path().join("retained-relay");
    fs::rename(&project, &retained).expect("move opened project");
    let outside = tempfile::tempdir().expect("planted project");
    let planted = outside.path().join("server/delegates/origin.py");
    fs::create_dir_all(planted.parent().expect("planted parent")).expect("planted root");
    fs::write(
        &planted,
        "import json\nprint(json.dumps({'status': 200, 'headers': {'content-type': ['application/json']}, 'body': json.dumps({'origin': 'planted-relay'})}))\n",
    )
    .expect("planted delegate");
    symlink(outside.path(), &project).expect("replace ambient project");

    let response = supervisor(HandlerSupervisorOptions::default())
        .invoke(
            &source,
            &request("owned_relay", HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("relay response");
    assert_eq!(body_json(&response)["origin"], "owned-relay");
}

#[cfg(unix)]
#[tokio::test]
async fn prepared_handlers_ignore_an_ambient_project_root_swap_during_real_invocation() {
    use std::os::unix::fs::symlink;

    let cases = [
        (
            "rs",
            "rustc",
            r#"#[Controller]
struct ContractController;
impl ContractController {
    fn GET(_request: &YonRequest) -> YonResponse {
        YonResponse::json("{\"kind\":\"rust\"}")
    }
}
"#,
            "rust",
        ),
        (
            "cs",
            "dotnet",
            r#"[Controller]
sealed class ContractController {
    public static YonResponse GET(YonRequest request) =>
        YonResponse.Json("{\"kind\":\"csharp\"}");
}
"#,
            "csharp",
        ),
        (
            "php",
            "php",
            r#"<?php
#[Controller]
final class ContractController
{
    public static function GET(YonRequest $request): YonResponse
    {
        return YonResponse::json('{"kind":"php"}');
    }
}
"#,
            "php",
        ),
    ];

    for (extension, tool, contents, expected) in cases {
        if !available(tool) {
            continue;
        }
        let root = tempfile::tempdir().expect("project");
        let prepared = source(&root, extension, contents);
        let absolute_artifacts = prepared
            .interpreter()
            .iter()
            .filter_map(|argument| {
                let candidate = argument
                    .split_once('=')
                    .map_or(argument.as_str(), |(_, value)| value);
                Path::new(candidate).is_absolute().then_some(candidate)
            })
            .collect::<Vec<_>>();
        assert!(!absolute_artifacts.is_empty(), "{extension}");
        for artifact in absolute_artifacts {
            assert!(Path::new(artifact).is_file(), "{extension}: {artifact}");
            assert!(
                !Path::new(artifact).starts_with(root.path()),
                "{extension} runtime still points into the authored project: {artifact}"
            );
        }

        let authored = root.path().to_path_buf();
        let retained = authored.with_extension(format!("retained-{extension}"));
        fs::rename(&authored, &retained).expect("move authored project");
        let outside = tempfile::tempdir().expect("planted project");
        let planted = outside
            .path()
            .join(format!("server/routes/contract/yon.{extension}"));
        fs::create_dir_all(planted.parent().expect("planted parent")).expect("planted root");
        fs::write(&planted, "planted source must never execute").expect("planted source");
        symlink(outside.path(), &authored).expect("replace authored project");
        let response = supervisor(HandlerSupervisorOptions {
            default_timeout: Duration::from_secs(10),
            ..HandlerSupervisorOptions::default()
        })
        .invoke(
            &prepared,
            &request(&format!("root_swap_{extension}"), HttpMethod::Get, None),
            &HandlerCancellation::default(),
        )
        .await
        .expect("prepared handler must use its owned runtime workspace");
        assert_eq!(response.status, 200, "{extension}");
        assert_eq!(body_json(&response)["kind"], expected, "{extension}");
        assert_eq!(
            fs::read_to_string(&planted).expect("planted source remains"),
            "planted source must never execute",
            "{extension}"
        );
        fs::remove_file(&authored).expect("remove replacement symlink");
        fs::rename(&retained, &authored).expect("restore authored project for cleanup");
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

    let missing_class_path = root.path().join("server/routes/missing/yon.js");
    fs::create_dir_all(missing_class_path.parent().expect("handler parent"))
        .expect("handler directory");
    fs::write(&missing_class_path, "export const notAHandler = true")
        .expect("invalid handler source");
    let rejected = HandlerSource::discover(root.path(), "server/routes/missing/yon.js")
        .expect_err("missing stereotype is rejected before runtime");
    assert!(rejected.to_string().contains("TY2015"), "{rejected}");

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

fn streaming_descendant_source(root: &tempfile::TempDir) -> HandlerSource {
    source(
        root,
        "py",
        r#"
import subprocess
import sys
import time

class Handler:
    @staticmethod
    @Stream
    def GET(request):
        descendant = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(60)"],
            stdin=sys.stdin,
            stdout=sys.stdout,
            stderr=sys.stderr,
        )
        with open(request["headers"]["x-pid-file"][0], "w") as target:
            target.write(str(descendant.pid))
        yield {"descendant": descendant.pid}
        while True:
            time.sleep(1)
"#,
    )
}

#[tokio::test]
async fn streaming_deadlines_reap_inheriting_descendants() {
    let root = tempfile::tempdir().expect("project");
    let source = streaming_descendant_source(&root);
    let pid_file = root.path().join("stream-timeout.pid");
    let supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_millis(250),
        cancellation_grace: Duration::from_millis(25),
        ..HandlerSupervisorOptions::default()
    });
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    let request = descendant_request("stream_timeout", "hang", &pid_file);
    let invocation = supervisor.invoke_streaming(&source, &request, sender);
    let (result, first) = tokio::join!(invocation, receiver.recv());
    assert!(first.is_some(), "stream should yield before its deadline");
    let failure = result.expect_err("stream deadline");
    assert!(failure.to_string().contains("TY2110"), "{failure}");
    assert_process_gone(descendant_pid(&pid_file)).await;
}

#[tokio::test]
async fn closing_a_stream_subscription_reaps_the_process_group() {
    let root = tempfile::tempdir().expect("project");
    let source = streaming_descendant_source(&root);
    let pid_file = root.path().join("stream-disconnect.pid");
    let supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_secs(3),
        ..HandlerSupervisorOptions::default()
    });
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let request = descendant_request("stream_disconnect", "hang", &pid_file);
    let task =
        tokio::spawn(async move { supervisor.invoke_streaming(&source, &request, sender).await });
    receiver.recv().await.expect("first streamed event");
    drop(receiver);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("disconnect cleanup stayed bounded")
        .expect("stream task joined")
        .expect("subscriber close is a clean stop");
    assert_process_gone(descendant_pid(&pid_file)).await;
}

#[tokio::test]
async fn oversized_python_stream_event_emits_only_a_bounded_failure_frame() {
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "py",
        r#"
class Handler:
    @staticmethod
    @Stream
    def GET(_request):
        yield "x" * (300 * 1024)
"#,
    );
    let supervisor = supervisor(HandlerSupervisorOptions::default());
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let result = supervisor
        .invoke_streaming(
            &source,
            &request("oversized_python_stream", HttpMethod::Get, None),
            sender,
        )
        .await
        .expect_err("oversized stream event");
    assert!(result.to_string().contains("TY2107"), "{result}");
    assert!(
        receiver.recv().await.is_none(),
        "oversized event must not escape"
    );
}

#[tokio::test]
async fn a_slow_subscriber_applies_backpressure_until_the_stream_deadline() {
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "py",
        r#"
class Handler:
    @staticmethod
    @Stream
    def GET(_request):
        while True:
            yield "x" * (200 * 1024)
"#,
    );
    let supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_millis(200),
        ..HandlerSupervisorOptions::default()
    });
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    let started = std::time::Instant::now();
    let failure = supervisor
        .invoke_streaming(
            &source,
            &request("slow_subscriber", HttpMethod::Get, None),
            sender,
        )
        .await
        .expect_err("a full subscriber queue must remain deadline bounded");
    assert!(failure.to_string().contains("TY2110"), "{failure}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_fast_javascript_generator_obeys_stdout_and_subscriber_backpressure() {
    if !available("bun") {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    let source = source(
        &root,
        "js",
        r#"
export class Handler {
  @Stream
  static async *GET(_request) {
    while (true) yield "x".repeat(200 * 1024)
  }
}
"#,
    );
    let supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_millis(250),
        ..HandlerSupervisorOptions::default()
    });
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    let started = std::time::Instant::now();
    let failure = supervisor
        .invoke_streaming(
            &source,
            &request("javascript_slow_subscriber", HttpMethod::Get, None),
            sender,
        )
        .await
        .expect_err("a fast generator must remain deadline bounded");
    assert!(failure.to_string().contains("TY2110"), "{failure}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn compiled_relay_bounds_sideband_redacts_errors_and_reaps_descendants() {
    if !available("php") || !available("python3") {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    let delegate = root.path().join("server/delegates/probe.py");
    fs::create_dir_all(delegate.parent().expect("delegate parent")).expect("delegates");
    fs::write(
        &delegate,
        r#"import json
import subprocess
import sys
import time

request = json.load(sys.stdin)
mode = request.get("body", {}).get("data", "")
if mode == "sideband":
    sys.stderr.write("secret-canary:" + "x" * (128 * 1024))
    sys.stderr.flush()
    sys.exit(9)
else:
    descendant = subprocess.Popen(
        [sys.executable, "-c", "import time; time.sleep(60)"],
        stdin=sys.stdin,
        stdout=sys.stdout,
        stderr=sys.stderr,
    )
    with open(request["headers"]["x-pid-file"][0], "w") as target:
        target.write(str(descendant.pid))
    time.sleep(60)
"#,
    )
    .expect("delegate");
    let relay_sources = [
        (
            "php",
            r#"<?php
#[Controller]
final class ContractController
{
    public static function POST(YonRequest $request): YonResponse
    {
        return ContractDelegate::POST($request);
    }
}

#[Delegate]
final class ContractDelegate
{
    #[Relay("python3", "server/delegates/probe.py")]
    public static function POST(YonRequest $request): YonResponse
    {
        throw new RuntimeException("relay method body must not execute");
    }
}
"#,
        ),
        (
            "java",
            r#"@Controller
class ContractController {
    static YonResponse POST(YonRequest request) { return ContractDelegate.POST(request); }
}
@Delegate
class ContractDelegate {
    @Relay({"python3", "server/delegates/probe.py"})
    static YonResponse POST(YonRequest request) { throw new RuntimeException("proxy body"); }
}
"#,
        ),
        (
            "kt",
            r#"@Controller
object ContractController {
    @JvmStatic fun POST(request: YonRequest): YonResponse = ContractDelegate.POST(request)
}
@Delegate
object ContractDelegate {
    @Relay("python3", "server/delegates/probe.py")
    fun POST(request: YonRequest): YonResponse { throw RuntimeException("proxy body") }
}
"#,
        ),
        (
            "cs",
            r#"[Controller]
sealed class ContractController {
    public static YonResponse POST(YonRequest request) => ContractDelegate.POST(request);
}
[Delegate]
sealed class ContractDelegate {
    [Relay("python3", "server/delegates/probe.py")]
    public static YonResponse POST(YonRequest request) { throw new Exception("proxy body"); }
}
"#,
        ),
        (
            "rs",
            r#"#[Controller]
struct ContractController;
impl ContractController {
    fn POST(request: &YonRequest) -> YonResponse { ContractDelegate::POST(request) }
}
#[Delegate]
struct ContractDelegate;
impl ContractDelegate {
    #[Relay("python3", "server/delegates/probe.py")]
    fn POST(_request: &YonRequest) -> YonResponse { panic!("proxy body") }
}
"#,
        ),
    ];
    let supervisor = supervisor(HandlerSupervisorOptions {
        default_timeout: Duration::from_secs(2),
        ..HandlerSupervisorOptions::default()
    });
    for &(extension, contents) in &relay_sources {
        let tool = match extension {
            "java" => "java",
            "kt" => "kotlinc",
            "cs" => "dotnet",
            "rs" => "rustc",
            _ => "php",
        };
        if !available(tool) {
            continue;
        }
        let relay = source(&root, extension, contents);
        let response = supervisor
            .invoke(
                &relay,
                &request(
                    &format!("relay_sideband_{extension}"),
                    HttpMethod::Post,
                    Some("sideband"),
                ),
                &HandlerCancellation::default(),
            )
            .await
            .expect("relay produces a sanitized upstream response");
        assert_eq!(response.status, 502, "{extension}");
        let rendered = response.body.expect("error body").data;
        assert!(
            !rendered.contains("secret-canary"),
            "{extension}: {rendered}"
        );
        assert!(!rendered.contains("xxxx"), "{extension}: {rendered}");
    }

    let source = source(&root, "php", relay_sources[0].1);
    let pid_file = root.path().join("relay-descendant.pid");
    let mut invocation = descendant_request("relay_timeout", "hang", &pid_file);
    invocation.method = HttpMethod::Post;
    invocation.deadline_ms = Some(250);
    let failure = supervisor
        .invoke(&source, &invocation, &HandlerCancellation::default())
        .await
        .expect_err("relay deadline");
    assert!(failure.to_string().contains("TY2110"), "{failure}");
    assert_process_gone(descendant_pid(&pid_file)).await;
}

#[tokio::test]
async fn interpreted_relays_bound_both_streams_and_redact_process_details() {
    if !available("python3") {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    let delegate = root.path().join("server/delegates/probe.py");
    fs::create_dir_all(delegate.parent().expect("delegate parent")).expect("delegates");
    fs::write(
        &delegate,
        r#"import json
import sys
request = json.load(sys.stdin)
mode = request.get("body", {}).get("data", "")
if mode == "stdout":
    sys.stdout.write("secret-canary:" + "x" * (17 * 1024 * 1024))
    sys.stdout.flush()
else:
    sys.stderr.write("secret-canary:" + "x" * (128 * 1024))
    sys.stderr.flush()
    sys.exit(9)
"#,
    )
    .expect("delegate");
    let cases = [
        (
            "py",
            r#"@Controller
class ContractController:
    @staticmethod
    def POST(request):
        return ContractDelegate.POST(request)

@Delegate
class ContractDelegate:
    @staticmethod
    @Relay("python3", "server/delegates/probe.py")
    def POST(request):
        raise RuntimeError("placeholder")
"#,
            true,
        ),
        (
            "js",
            r"@Controller
export class ContractController {
  static POST(request) { return ContractDelegate.POST(request) }
}
@Delegate
class ContractDelegate {
  @Relay('python3', 'server/delegates/probe.py')
  static POST(_request) { throw new Error('placeholder') }
}
",
            available("bun"),
        ),
    ];
    let supervisor = supervisor(HandlerSupervisorOptions::default());
    for (extension, contents, supported) in cases {
        if !supported {
            continue;
        }
        let relay = source(&root, extension, contents);
        for mode in ["sideband", "stdout"] {
            let response = supervisor
                .invoke(
                    &relay,
                    &request(
                        &format!("interpreted_relay_{extension}_{mode}"),
                        HttpMethod::Post,
                        Some(mode),
                    ),
                    &HandlerCancellation::default(),
                )
                .await
                .expect("relay returns a sanitized upstream response");
            assert_eq!(response.status, 502, "{extension}:{mode}");
            let rendered = response.body.expect("error body").data;
            assert!(
                !rendered.contains("secret-canary"),
                "{extension}: {rendered}"
            );
            assert!(!rendered.contains("python3"), "{extension}: {rendered}");
            assert!(rendered.len() < 1024, "{extension}: {rendered}");
        }
    }
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
