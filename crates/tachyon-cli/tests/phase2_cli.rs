//! Phase 2 behavior tests against the compiled `ty` executable.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn ty() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ty"))
}

fn run(command: &mut Command) -> Output {
    command.output().expect("the ty process should start")
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
    fs::write(path, contents).expect("fixture source");
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn compiled_binary_invokes_javascript_and_python_through_the_shared_contract() {
    let workspace = tempfile::tempdir().expect("workspace");
    let project = workspace.path().join("Phase 2 🌍 project");
    write(
        &project.join("server/routes/javascript/yon.js"),
        r"
export class Handler {
  static async POST(request) {
    return {
      route: request.route,
      method: request.method,
      header: request.headers['x-phase'][0],
      body: request.body.data,
      language: 'javascript',
    }
  }
}
",
    );
    write(
        &project.join("server/routes/python/yon.py"),
        r#"
class Handler:
    @staticmethod
    async def POST(request):
        return {
            "route": request["route"],
            "method": request["method"],
            "header": request["headers"]["x-phase"][0],
            "body": request["body"]["data"],
            "language": "python",
        }
"#,
    );

    for (source, route, language) in [
        (
            "server/routes/javascript/yon.js",
            "/javascript",
            "javascript",
        ),
        ("server/routes/python/yon.py", "/python", "python"),
    ] {
        let mut command = ty();
        command
            .args(["handler", "invoke", source])
            .arg("--project")
            .arg(&project)
            .args([
                "--route",
                route,
                "--method",
                "POST",
                "--request-id",
                "binary_contract",
                "--header",
                "x-phase=two",
                "--body",
                "héllø",
            ]);
        if language == "python" {
            command.args([
                "--python-runtime",
                if cfg!(windows) { "python" } else { "python3" },
            ]);
        }
        let output = run(&mut command);
        assert!(output.status.success(), "{}", stderr(&output));
        let response: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("protocol response JSON");
        assert_eq!(response["protocol_version"], 1);
        assert_eq!(response["kind"], "response");
        assert_eq!(response["request_id"], "binary_contract");
        assert_eq!(response["status"], 200);
        let body: serde_json::Value =
            serde_json::from_str(response["body"]["data"].as_str().expect("body data"))
                .expect("handler JSON body");
        assert_eq!(body["route"], route);
        assert_eq!(body["method"], "POST");
        assert_eq!(body["header"], "two");
        assert_eq!(body["body"], "héllø");
        assert_eq!(body["language"], language);
    }
}

#[test]
fn handler_error_responses_and_supervisor_diagnostics_remain_distinct() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/yon.js"),
        "export class Handler { static GET() { return { ok: true } } }",
    );

    let method = run(ty()
        .args(["handler", "invoke", "server/routes/yon.js"])
        .arg("--project")
        .arg(project.path())
        .args(["--method", "DELETE"]));
    assert!(method.status.success(), "{}", stderr(&method));
    let response: serde_json::Value =
        serde_json::from_slice(&method.stdout).expect("error response");
    assert_eq!(response["status"], 405);
    assert_eq!(response["error"]["code"], "TY2202");

    let runtime = run(ty()
        .args(["handler", "invoke", "server/routes/yon.js"])
        .arg("--project")
        .arg(project.path())
        .args(["--javascript-runtime", "tachyon-runtime-does-not-exist"]));
    assert!(!runtime.status.success());
    assert!(stderr(&runtime).contains("error[TY2101]"));

    let machine = run(ty()
        .args([
            "--diagnostic-format",
            "json",
            "handler",
            "invoke",
            "../yon.js",
        ])
        .arg("--project")
        .arg(project.path()));
    assert!(!machine.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&machine.stderr).expect("diagnostic JSON");
    assert_eq!(report["contract_version"], 1);
    assert_eq!(report["diagnostics"][0]["code"], "TY2002");
}

#[test]
fn handler_only_builds_emit_deterministic_api_manifest_entries() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/products/yon.py"),
        "class Handler: pass",
    );
    write(
        &project.path().join("server/routes/products/yon.js"),
        "export class Handler {}",
    );

    let first = run(ty().arg("build").arg(project.path()));
    assert!(first.status.success(), "{}", stderr(&first));
    let first_manifest =
        fs::read(project.path().join("dist/route-manifest.json")).expect("manifest");
    let second = run(ty().arg("build").arg(project.path()));
    assert!(second.status.success(), "{}", stderr(&second));
    assert_eq!(
        first_manifest,
        fs::read(project.path().join("dist/route-manifest.json")).expect("manifest")
    );
    assert!(!project.path().join("dist/products/index.html").exists());

    let manifest: serde_json::Value =
        serde_json::from_slice(&first_manifest).expect("manifest JSON");
    assert_eq!(manifest["routes"][0]["kind"], "api");
    assert_eq!(
        manifest["routes"][0]["methods"].as_array().map(Vec::len),
        Some(7)
    );
    assert_eq!(
        manifest["routes"][0]["handlers"][0]["runtime"],
        "javascript.v1"
    );
    assert_eq!(manifest["routes"][0]["handlers"][1]["runtime"], "python.v1");
}

#[test]
fn compiled_binary_enforces_deadline_and_environment_name_policy() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/yon.js"),
        r"
export class Handler {
  static async GET() {
    await new Promise((resolve) => setTimeout(resolve, 5000))
    return { tooLate: true }
  }
}
",
    );
    let timeout = run(ty()
        .args(["handler", "invoke", "server/routes/yon.js"])
        .arg("--project")
        .arg(project.path())
        .args(["--timeout-ms", "40"]));
    assert!(!timeout.status.success());
    assert!(stderr(&timeout).contains("TY2110"));

    let environment = run(ty()
        .args(["handler", "invoke", "server/routes/yon.js"])
        .arg("--project")
        .arg(project.path())
        .args(["--allow-env", "BAD-NAME"]));
    assert!(!environment.status.success());
    assert!(stderr(&environment).contains("TY2006"));

    for header in ["missing-equals", "=empty"] {
        let invalid = run(ty()
            .args(["handler", "invoke", "server/routes/yon.js"])
            .arg("--project")
            .arg(project.path())
            .args(["--header", header]));
        assert_eq!(invalid.status.code(), Some(2));
    }
}

#[test]
fn every_protocol_http_method_reaches_the_compiled_handler_command() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/yon.js"),
        r"
export class Handler {
  static DELETE(request) { return request.method }
  static GET(request) { return request.method }
  static HEAD(request) { return request.method }
  static OPTIONS(request) { return request.method }
  static PATCH(request) { return request.method }
  static POST(request) { return request.method }
  static PUT(request) { return request.method }
}
",
    );
    for method in ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"] {
        let output = run(ty()
            .args(["handler", "invoke", "server/routes/yon.js"])
            .arg("--project")
            .arg(project.path())
            .args(["--method", method]));
        assert!(output.status.success(), "{method}: {}", stderr(&output));
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).expect("response");
        let returned: String =
            serde_json::from_str(response["body"]["data"].as_str().expect("body"))
                .expect("method string");
        assert_eq!(returned, method);
    }
}
