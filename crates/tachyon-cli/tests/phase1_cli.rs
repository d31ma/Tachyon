//! Phase 1 behavior-level acceptance tests against the compiled `ty` binary.

#![allow(clippy::expect_used)]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn ty() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ty"))
}

#[test]
fn version_reports_the_release_product_identity() {
    let output = run(ty().arg("--version"));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output).trim(), tachyon_contracts::PRODUCT_VERSION);

    let short = run(ty().arg("-v"));
    assert!(short.status.success(), "{}", stderr(&short));
    assert_eq!(stdout(&short).trim(), tachyon_contracts::PRODUCT_VERSION);
}

#[test]
fn released_aliases_environment_and_removed_render_mode_are_compatible() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let by_alias = workspace.path().join("by-alias");
    let initialized = run(ty()
        .arg("init")
        .arg(&by_alias)
        .args(["--app-name", "Alias Name"]));
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    assert!(
        fs::read_to_string(by_alias.join("client/pages/tac.js"))
            .expect("generated companion")
            .contains("document.title = \"Alias Name\"")
    );

    let by_environment = workspace.path().join("by-environment");
    let initialized = run(ty()
        .arg("init")
        .arg(&by_environment)
        .env("TAC_APP_NAME", "Environment Name"));
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    let output = workspace.path().join("published");
    let bundled = run(ty()
        .arg("bundle")
        .arg(&by_environment)
        .args([
            "--target",
            "browser",
            "--skip-native-host",
            "--csp-check",
            "--package",
            "--skip-package",
        ])
        .env("TAC_DIST_PATH", &output));
    assert!(bundled.status.success(), "{}", stderr(&bundled));
    assert!(output.join("index.html").is_file());

    let rendered = run(ty()
        .arg("bundle")
        .arg(&by_environment)
        .args(["--render-mode", "native"]));
    assert!(!rendered.status.success());
    assert!(stderr(&rendered).contains("always native-first"));
}

#[test]
fn target_environment_and_native_command_cardinality_match_the_released_cli() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let project = workspace.path().join("targeted");
    assert!(run(ty().arg("init").arg(&project)).status.success());

    let output = workspace.path().join("environment-output");
    let bundled = run(ty()
        .arg("bundle")
        .arg(&project)
        .env("TAC_BUNDLE_TARGET", "browser,web")
        .env("TAC_DIST_PATH", &output));
    assert!(bundled.status.success(), "{}", stderr(&bundled));
    assert!(output.join("index.html").is_file());

    let missing = run(ty().arg("native-bundle").arg(&project));
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("requires exactly one target"));

    let web = run(ty()
        .arg("native-bundle")
        .arg(&project)
        .args(["--target", "web"]));
    assert!(!web.status.success());
    assert!(stderr(&web).contains("requires a native target"));

    let all = run(ty().arg("preview").arg(&project).args(["--target", "all"]));
    assert!(!all.status.success());
    assert!(stderr(&all).contains("requires exactly one target"));
}

#[test]
fn multi_target_bundles_publish_each_target_exactly_one_level_below_the_output() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let project = workspace.path().join("multi-target");
    assert!(run(ty().arg("init").arg(&project)).status.success());
    let output = workspace.path().join("published");

    let bundled = run(ty()
        .arg("bundle")
        .arg(&project)
        .args(["--target", "web,macos,ios,android", "--skip-package"])
        .env("TAC_DIST_PATH", &output));
    assert!(bundled.status.success(), "{}", stderr(&bundled));
    assert!(output.join("web/index.html").is_file());
    for target in ["macos", "ios", "android"] {
        assert!(
            output.join(target).join("artifact-manifest.json").is_file(),
            "missing {target} output"
        );
        assert!(
            !output.join(target).join(target).exists(),
            "{target} output was nested twice"
        );
    }
}

fn run(command: &mut Command) -> Output {
    command.output().expect("the ty process should start")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("fixture file needs a parent"))
        .expect("fixture directory should be created");
    fs::write(path, contents).expect("fixture should be written");
}

fn files_below(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn collect(root: &Path, current: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries = fs::read_dir(current)
            .expect("output directory should be readable")
            .collect::<Result<Vec<_>, _>>()
            .expect("output entries should be readable");
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root)
                        .expect("output path should be relative")
                        .to_path_buf(),
                    fs::read(path).expect("output file should be readable"),
                ));
            }
        }
    }

    let mut files = Vec::new();
    collect(root, root, &mut files);
    files
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

const MAX_REQUEST_ATTEMPTS: usize = 3;
const MAX_CONNECT_ATTEMPTS: usize = 20;

fn connect(socket: &str) -> TcpStream {
    for attempt in 1..=MAX_CONNECT_ATTEMPTS {
        match TcpStream::connect(socket) {
            Ok(connection) => return connection,
            Err(error)
                if error.kind() == std::io::ErrorKind::ConnectionRefused
                    && attempt < MAX_CONNECT_ATTEMPTS =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!("server should accept connections: {error}"),
        }
    }
    unreachable!("the final connection attempt returns or panics")
}

fn request(socket: &str, request: &[u8]) -> String {
    for attempt in 1..=MAX_REQUEST_ATTEMPTS {
        let mut connection = connect(socket);
        connection
            .write_all(request)
            .expect("request should be sent");
        let mut response = Vec::new();
        match connection.read_to_end(&mut response) {
            Ok(_) => return String::from_utf8(response).expect("response should be UTF-8"),
            // Linux may report a reset instead of EOF after Hyper has written
            // and closed a `Connection: close` response. Accept preserved
            // bytes only when framing proves completeness. If no response
            // bytes arrived, retry a fresh idempotent request within a small
            // fixed budget; never discard a partial response.
            Err(error)
                if error.kind() == std::io::ErrorKind::ConnectionReset
                    && reset_response_is_complete(request, &response) =>
            {
                return String::from_utf8(response).expect("response should be UTF-8");
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::ConnectionReset
                    && empty_reset_can_retry(request, &response, attempt) => {}
            Err(error) => panic!("response should be read: {error}"),
        }
    }
    panic!("response retry budget should end in the error branch")
}

fn retryable_test_request(request: &[u8]) -> bool {
    request.starts_with(b"GET ") || request.starts_with(b"HEAD ")
}

fn empty_reset_can_retry(request: &[u8], response: &[u8], attempt: usize) -> bool {
    response.is_empty() && retryable_test_request(request) && attempt < MAX_REQUEST_ATTEMPTS
}

fn reset_response_is_complete(request: &[u8], response: &[u8]) -> bool {
    let Some(header_end) = response.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
        return false;
    };
    let Ok(headers) = std::str::from_utf8(&response[..header_end]) else {
        return false;
    };
    let body = &response[header_end + 4..];
    let mut lines = headers.lines();
    let Some(status_line) = lines.next() else {
        return false;
    };
    let mut status_parts = status_line.split_whitespace();
    if status_parts.next() != Some("HTTP/1.1") {
        return false;
    }
    let Some(status_value) = status_parts.next() else {
        return false;
    };
    if status_value.len() != 3 || !status_value.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let Ok(status) = status_value.parse::<u16>() else {
        return false;
    };
    let mut content_length = None;
    let mut transfer_encoding = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return false;
            }
            content_length = value.trim().parse::<usize>().ok();
            if content_length.is_none() {
                return false;
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            transfer_encoding = true;
        }
    }
    if transfer_encoding && content_length.is_some() {
        return false;
    }
    if transfer_encoding {
        return false;
    }
    let request_is_head = request.starts_with(b"HEAD ");
    let status_is_bodyless = (100..200).contains(&status) || matches!(status, 204 | 304);
    if request_is_head || status_is_bodyless {
        return body.is_empty();
    }
    content_length.is_some_and(|length| body.len() == length)
}

#[test]
fn reset_responses_require_complete_unambiguous_http_framing() {
    assert!(retryable_test_request(b"GET / HTTP/1.1\r\n\r\n"));
    assert!(retryable_test_request(b"HEAD / HTTP/1.1\r\n\r\n"));
    assert!(!retryable_test_request(b"POST / HTTP/1.1\r\n\r\n"));
    assert!(empty_reset_can_retry(b"GET / HTTP/1.1\r\n\r\n", b"", 1));
    assert!(!empty_reset_can_retry(
        b"GET /../Cargo.toml HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n[workspace]",
        1,
    ));
    assert!(!empty_reset_can_retry(
        b"GET / HTTP/1.1\r\n\r\n",
        b"",
        MAX_REQUEST_ATTEMPTS,
    ));
    assert!(reset_response_is_complete(
        b"GET / HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok",
    ));
    assert!(!reset_response_is_complete(
        b"GET / HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 11\r\n\r\n[work",
    ));
    assert!(!reset_response_is_complete(
        b"GET / HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n",
    ));
    assert!(!reset_response_is_complete(
        b"GET / HTTP/1.1\r\n\r\n",
        b"not-http 200 OK\r\nContent-Length: 2\r\n\r\nok",
    ));
    assert!(reset_response_is_complete(
        b"HEAD / HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 10\r\n\r\n",
    ));
    assert!(!reset_response_is_complete(
        b"HEAD / HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 10\r\n\r\npartial",
    ));
    assert!(!reset_response_is_complete(
        b"HEAD / HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 405 Method Not Allowed\r\nTransfer-Encoding: chunked\r\n\r\n",
    ));
    assert!(reset_response_is_complete(
        b"GET / HTTP/1.1\r\n\r\n",
        b"HTTP/1.1 204 No Content\r\n\r\n",
    ));
}

#[test]
fn generated_project_builds_deterministically_with_the_real_binary() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let project = workspace.path().join("hello");

    let initialized = run(ty()
        .arg("init")
        .arg(&project)
        .args(["--name", "Hello <Tachyon>"]));
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    assert!(stdout(&initialized).contains("Initialized"));

    let first = run(ty().arg("build").arg(&project));
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("Built 1 route"));
    let first_files = files_below(&project.join("dist"));

    let second = run(ty().arg("build").arg(&project));
    assert!(second.status.success(), "{}", stderr(&second));
    assert_eq!(first_files, files_below(&project.join("dist")));

    let html = fs::read_to_string(project.join("dist/index.html")).expect("HTML should exist");
    assert!(html.starts_with("<!doctype html>"));
    let script = fs::read_to_string(project.join("client/pages/tac.js")).expect("companion");
    assert!(script.contains("document.title = \"Hello <Tachyon>\""));

    let manifest = fs::read_to_string(project.join("dist/route-manifest.json"))
        .expect("manifest should exist");
    assert!(manifest.ends_with('\n'));
    assert!(manifest.contains("\"contract_version\": 1"));
    assert!(manifest.contains("\"route\": \"/\""));
}

#[test]
fn released_command_surface_and_web_bundle_path_remain_compatible() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let project = workspace.path().join("public-surface");
    let initialized = run(ty().arg("init").arg(&project));
    assert!(initialized.status.success(), "{}", stderr(&initialized));

    let help = run(ty().arg("help"));
    assert!(help.status.success(), "{}", stderr(&help));
    let help = stdout(&help);
    for command in [
        "init",
        "serve",
        "bundle",
        "native-bundle",
        "preview",
        "cache",
    ] {
        assert!(help.contains(command), "missing {command}: {help}");
    }
    for internal in ["doctor", "handler", "migrate"] {
        assert!(!help.contains(internal), "leaked {internal}: {help}");
    }

    let bundled = run(ty().arg("bundle").arg(&project));
    assert!(bundled.status.success(), "{}", stderr(&bundled));
    assert!(project.join("dist/web/index.html").is_file());
}

#[test]
fn preview_serves_the_published_web_bundle_without_recompiling_sources() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let project = workspace.path().join("previewed");
    let initialized = run(ty().arg("init").arg(&project).args(["--name", "Published"]));
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    let bundled = run(ty().arg("bundle").arg(&project));
    assert!(bundled.status.success(), "{}", stderr(&bundled));
    let published = fs::read_to_string(project.join("dist/web/index.html")).expect("bundle");

    write(
        &project.join("client/pages/tac.html"),
        "<main>This source must not be compiled by preview</main>",
    );
    let mut child = ty()
        .arg("preview")
        .arg(&project)
        .args(["--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("preview should start");
    let output = child.stdout.take().expect("preview stdout should be piped");
    let mut ready = String::new();
    BufReader::new(output)
        .read_line(&mut ready)
        .expect("readiness line should be readable");
    let address = ready
        .split_whitespace()
        .find(|part| part.starts_with("http://"))
        .expect("readiness line should include an address")
        .trim_end_matches('/');
    let response = request(
        address.trim_start_matches("http://"),
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert!(response.contains(&published), "{response}");
    assert_eq!(
        fs::read_to_string(project.join("dist/web/index.html")).expect("unchanged bundle"),
        published
    );
    stop(&mut child);
}

#[test]
fn serve_no_bundle_uses_the_existing_web_bundle_without_recompiling_sources() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let project = workspace.path().join("served-without-build");
    let initialized = run(ty().arg("init").arg(&project).args(["--name", "Published"]));
    assert!(initialized.status.success(), "{}", stderr(&initialized));
    let bundled = run(ty().arg("bundle").arg(&project));
    assert!(bundled.status.success(), "{}", stderr(&bundled));
    let published = fs::read_to_string(project.join("dist/web/index.html")).expect("bundle");

    write(
        &project.join("client/pages/tac.html"),
        "<main>This source must not be compiled by serve --no-bundle</main>",
    );
    fs::remove_file(project.join("server/routes/yon.js")).expect("root handler removal");
    let mut child = ty()
        .arg("serve")
        .arg(&project)
        .args(["--port", "0", "--no-watch", "--no-bundle"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("development server should start");
    let output = child.stdout.take().expect("server stdout should be piped");
    let mut ready = String::new();
    BufReader::new(output)
        .read_line(&mut ready)
        .expect("readiness line should be readable");
    let address = ready
        .split_whitespace()
        .find(|part| part.starts_with("http://"))
        .expect("readiness line should include an address")
        .trim_end_matches('/');
    let response = request(
        address.trim_start_matches("http://"),
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    stop(&mut child);

    assert!(response.contains(&published), "{response}");
    assert_eq!(
        fs::read_to_string(project.join("dist/web/index.html")).expect("unchanged bundle"),
        published
    );
}

#[test]
fn static_tac_pages_compile_in_canonical_route_order() {
    let project = tempfile::tempdir().expect("project should be created");
    write(
        &project.path().join("client/pages/zeta/tac.html"),
        "<main>Zeta</main>",
    );
    write(
        &project.path().join("client/pages/about/tac.html"),
        "<main>About</main>",
    );

    let output = run(ty().arg("build").arg(project.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(project.path().join("dist/about/index.html").is_file());
    assert!(project.path().join("dist/zeta/index.html").is_file());

    let manifest =
        fs::read_to_string(project.path().join("dist/route-manifest.json")).expect("manifest");
    assert!(
        manifest.find("\"/about\"").expect("about route")
            < manifest.find("\"/zeta\"").expect("zeta route")
    );
}

#[test]
fn diagnostics_are_stable_in_human_and_json_formats() {
    let empty = tempfile::tempdir().expect("empty project should be created");
    let human = run(ty().arg("build").arg(empty.path()));
    assert!(!human.status.success());
    assert!(stderr(&human).contains("error[TY1002]"));

    write(
        &empty.path().join("client/pages/tac.html"),
        "<main><if condition=\"ready\">Later</if></main>",
    );
    let json = run(ty()
        .args(["--diagnostic-format", "json"])
        .arg("build")
        .arg(empty.path()));
    assert!(!json.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&json.stderr).expect("diagnostic output should be JSON");
    assert_eq!(report["contract_version"], 1);
    assert_eq!(report["diagnostics"][0]["code"], "TY1103");
    assert_eq!(
        report["diagnostics"][0]["spans"][0]["file"],
        "client/pages/tac.html"
    );
}

#[test]
fn yon_html_and_unsupported_companions_fail_without_replacing_output() {
    let project = tempfile::tempdir().expect("project should be created");
    let tac = project.path().join("client/pages/tac.html");
    write(&tac, "<main>Known good</main>");

    let first = run(ty().arg("build").arg(project.path()));
    assert!(first.status.success(), "{}", stderr(&first));
    let published = files_below(&project.path().join("dist"));

    write(
        &project.path().join("server/routes/yon.html"),
        "<main>Collision</main>",
    );
    let yon_html = run(ty().arg("build").arg(project.path()));
    assert!(!yon_html.status.success());
    assert!(stderr(&yon_html).contains("TY1008"));
    assert!(stderr(&yon_html).contains("Content-Type: text/html"));
    assert_eq!(published, files_below(&project.path().join("dist")));

    fs::remove_file(project.path().join("server/routes/yon.html")).expect("fixture removal");
    write(
        &project.path().join("client/pages/tac.rs"),
        "struct Handler;",
    );
    let companion = run(ty().arg("build").arg(project.path()));
    assert!(!companion.status.success());
    assert!(stderr(&companion).contains("TY1008"));
    assert_eq!(published, files_below(&project.path().join("dist")));
}

#[test]
fn init_refuses_to_overwrite_and_non_loopback_serving_requires_opt_in() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    write(&workspace.path().join("occupied/keep.txt"), "keep");

    let init = run(ty().arg("init").arg(workspace.path().join("occupied")));
    assert!(!init.status.success());
    assert!(stderr(&init).contains("TY1402"));
    assert_eq!(
        fs::read_to_string(workspace.path().join("occupied/keep.txt")).expect("kept"),
        "keep"
    );

    let serve = run(ty()
        .arg("dev")
        .arg(workspace.path().join("occupied"))
        .args(["--host", "0.0.0.0"]));
    assert!(!serve.status.success());
    assert!(stderr(&serve).contains("TY1301"));
}

#[test]
fn development_server_builds_and_serves_with_defensive_headers() {
    let workspace = tempfile::tempdir().expect("workspace should be created");
    let project = workspace.path().join("served");
    let initialized = run(ty().arg("init").arg(&project));
    assert!(initialized.status.success(), "{}", stderr(&initialized));

    let mut child = ty()
        .arg("serve")
        .arg(&project)
        .args(["--port", "0", "--no-watch"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("development server should start");
    let output = child.stdout.take().expect("server stdout should be piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(output).read_line(&mut line).map(|_| line);
        let _ = sender.send(result);
    });
    let ready = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("server should announce readiness")
        .expect("readiness line should be readable");
    let address = ready
        .split_whitespace()
        .find(|part| part.starts_with("http://"))
        .expect("readiness line should include an address")
        .trim_end_matches('/');
    let socket = address.trim_start_matches("http://");

    let response = request(
        socket,
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    let head = request(
        socket,
        b"HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    let missing = request(
        socket,
        b"GET /missing HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    let traversal = request(
        socket,
        b"GET /../Cargo.toml HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    stop(&mut child);

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("x-content-type-options: nosniff"));
    assert!(response.contains("content-security-policy:"));
    assert!(response.contains("'wasm-unsafe-eval'"));
    assert!(response.contains(r#"{"ok":true,"framework":"Tachyon"}"#));
    assert!(
        head.starts_with("HTTP/1.1 405 Method Not Allowed"),
        "{head}"
    );
    assert!(!head.contains(r#"{"ok":true,"framework":"Tachyon"}"#));
    assert!(missing.starts_with("HTTP/1.1 404 Not Found"), "{missing}");
    assert!(!traversal.starts_with("HTTP/1.1 200 OK"), "{traversal}");
    assert!(!traversal.contains("[workspace]"), "{traversal}");
}
