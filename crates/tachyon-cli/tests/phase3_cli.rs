//! Phase 3 behavior tests against the compiled `ty` executable.
#![allow(clippy::expect_used, clippy::too_many_lines)]

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

#[test]
fn client_shared_assets_are_published_at_the_stable_shared_path() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        r#"<main><img src="/shared/assets/mark.svg" alt="mark"></main>"#,
    );
    write(
        &project.path().join("client/shared/assets/mark.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n",
    );
    write(
        &project.path().join("client/shared/data/example.json"),
        "{\"ready\":true}\n",
    );
    write(
        &project.path().join("client/shared/scripts/imports.js"),
        "import '../styles/site.css'\nimport './runtime.js'\n",
    );
    write(
        &project.path().join("client/shared/styles/site.css"),
        "body { color: teal; }\n",
    );

    let output = run(ty().arg("build").arg(project.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/assets/mark.svg"))
            .expect("published SVG"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/data/example.json"))
            .expect("published JSON"),
        "{\"ready\":true}\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/scripts/imports.js"))
            .expect("published browser entry"),
        "import './runtime.js'\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/styles/site.css"))
            .expect("published stylesheet"),
        "body { color: teal; }\n"
    );
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[cfg(unix)]
#[test]
fn csharp_runtime_without_sdk_build_capability_fails_before_serve_readiness() {
    use std::os::unix::fs::PermissionsExt;

    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/yon.cs"),
        "[Controller]\nsealed class RootController { public static YonResponse GET(YonRequest request) => YonResponse.Json(\"{}\"); }\n",
    );
    fs::create_dir_all(project.path().join("dist/web")).expect("existing bundle");
    fs::write(
        project.path().join("dist/web/index.html"),
        "<main>Ready</main>",
    )
    .expect("existing output");

    let tools = tempfile::tempdir().expect("fake tools");
    let canary = tools.path().join("dotnet");
    fs::write(
        &canary,
        "#!/bin/sh\n\
         if [ \"$1\" = \"--version\" ]; then echo 10.0.302; exit 0; fi\n\
         if [ \"$1\" = \"--list-runtimes\" ]; then echo 'Microsoft.NETCore.App 10.0.0 [/fake]'; exit 0; fi\n\
         if [ \"$1\" = \"build\" ] && [ \"$2\" = \"Readiness.csproj\" ]; then exit 9; fi\n\
         if [ \"$1\" = \"build\" ]; then /bin/mkdir -p out; echo x > out/handler.dll; echo x > out/handler.deps.json; echo x > out/handler.runtimeconfig.json; exit 0; fi\n\
         exit 9\n",
    )
    .expect("runtime-only dotnet fake");
    fs::set_permissions(&canary, fs::Permissions::from_mode(0o700)).expect("permissions");

    let occupied =
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("occupied listener");
    let port = occupied
        .local_addr()
        .expect("occupied address")
        .port()
        .to_string();
    let output = run(ty()
        .arg("serve")
        .arg(project.path())
        .args(["--port", &port, "--no-watch", "--no-bundle"])
        .env("PATH", tools.path()));
    assert!(!output.status.success(), "{}", stdout(&output));
    let diagnostic = stderr(&output);
    assert!(diagnostic.contains("TY2101"), "{diagnostic}");
    assert!(!diagnostic.contains("TY1302"), "{diagnostic}");
    assert!(!stdout(&output).contains("server ready"));
    assert!(
        !diagnostic.contains(canary.to_string_lossy().as_ref()),
        "{diagnostic}"
    );
}

#[test]
fn yon_html_is_rejected_without_executing_a_handler() {
    let project = tempfile::tempdir().expect("project");
    let marker = project.path().join("handler-was-executed");
    write(
        &project.path().join("server/routes/products/yon.html"),
        "<main>{products}</main>",
    );
    write(
        &project.path().join("server/routes/products/yon.js"),
        &format!(
            "import {{ writeFileSync }} from 'node:fs';\n@Controller\nexport class ProductsController {{ static GET() {{ writeFileSync({}, 'bad'); return {{ products: [] }} }} }}",
            serde_json::to_string(&marker).expect("marker path")
        ),
    );

    let output = run(ty().arg("build").arg(project.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1008"), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("Content-Type: text/html"),
        "{}",
        stderr(&output)
    );
    assert!(!marker.exists(), "Yon GET ran during compilation");
}
#[test]
fn yon_handlers_can_return_explicit_html_responses_without_framework_rendering() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/javascript/yon.js"),
        r"
@Controller
export class JavascriptController {
  static GET() {
    return {
      status: 203,
      headers: { 'Content-Type': 'text/html; charset=utf-8', 'X-View': 'handler' },
      body: '<main><h1>JavaScript HTML</h1></main>',
    }
  }
}
",
    );
    write(
        &project.path().join("server/routes/python/yon.py"),
        r#"
@Controller
class PythonController:
    @staticmethod
    def GET(request):
        return {
            "status": 203,
            "headers": {
                "Content-Type": "text/html; charset=utf-8",
                "X-View": "handler",
            },
            "body": "<main><h1>Python HTML</h1></main>",
        }
"#,
    );

    for (source, route, expected) in [
        (
            "server/routes/javascript/yon.js",
            "/javascript",
            "<main><h1>JavaScript HTML</h1></main>",
        ),
        (
            "server/routes/python/yon.py",
            "/python",
            "<main><h1>Python HTML</h1></main>",
        ),
    ] {
        let mut command = ty();
        command
            .args(["handler", "invoke", source])
            .arg("--project")
            .arg(project.path())
            .args(["--route", route, "--method", "GET"]);
        if std::path::Path::new(source)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("py"))
        {
            command.args([
                "--python-runtime",
                if cfg!(windows) { "python" } else { "python3" },
            ]);
        }
        let output = run(&mut command);
        assert!(output.status.success(), "{}", stderr(&output));
        let response: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("protocol response");
        assert_eq!(response["status"], 203);
        assert_eq!(
            response["headers"]["content-type"],
            serde_json::json!(["text/html; charset=utf-8"])
        );
        assert_eq!(
            response["headers"]["x-view"],
            serde_json::json!(["handler"])
        );
        assert_eq!(response["body"]["data"], expected);
    }
}
#[test]
fn tac_components_emit_client_plans_modules_and_all_mount_policies() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        r#"
<!doctype html><html><head><title>Islands</title></head><body>
<main>
  <demo-interactive label="SSR" hydrate="interaction"></demo-interactive>
  <demo-idle label="Idle" hydrate="idle"></demo-idle>
  <demo-visible label="Visible" hydrate="visible"></demo-visible>
  <demo-load label="Load" hydrate="load"></demo-load>
  <demo-default label="Default"></demo-default>
  <demo-never label="Never" hydrate="never"></demo-never>
</main>
</body></html>
"#,
    );
    write(
        &project.path().join("client/pages/fragment/tac.html"),
        r#"<section><demo-load label="Fragment" hydrate="load"></demo-load></section>"#,
    );
    for (second, label, companion) in [
        ("interactive", "interactive", true),
        ("idle", "idle", true),
        ("visible", "visible", true),
        ("load", "load", true),
        ("default", "default", true),
        ("never", "never", false),
    ] {
        let directory = project
            .path()
            .join(format!("client/components/demo/{second}"));
        write(
            &directory.join("tac.html"),
            &format!("<button data-kind=\"{label}\">{{label}}</button>"),
        );
        if companion {
            write(
                &directory.join("tac.js"),
                r"
export default class Demo {
  constructor(props) { this.props = props }
  async hydrate(root) { root.dataset.activated = this.props.label }
}
",
            );
        }
    }

    let output = run(ty().arg("build").arg(project.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("compiled=2 reused=0"));
    let html = fs::read_to_string(project.path().join("dist/index.html")).expect("HTML");
    for policy in ["interaction", "idle", "visible", "load", "never"] {
        assert!(html.contains(&format!(r#""mount":"{policy}""#)));
    }
    assert!(!html.contains("<button data-kind="), "{html}");
    assert!(html.contains(r#"src="/.tachyon/tac-client.js""#));
    assert!(project.path().join("dist/.tachyon/tac-client.js").is_file());
    assert!(!project.path().join("dist/.tachyon/islands.js").exists());
    for component in ["demo-interactive", "demo-idle", "demo-visible", "demo-load"] {
        assert!(
            project
                .path()
                .join(format!("dist/.tachyon/components/{component}.js"))
                .is_file()
        );
    }
    assert!(html.contains(r#""mount":"load","name":"demo-default""#));
    assert!(
        project
            .path()
            .join("dist/.tachyon/components/demo-default.js")
            .is_file()
    );
    assert!(
        !project
            .path()
            .join("dist/.tachyon/components/demo-never.js")
            .exists()
    );
    assert!(!html.contains(r#""module":"/.tachyon/components/demo-never.js""#));
    let fragment =
        fs::read_to_string(project.path().join("dist/fragment/index.html")).expect("fragment HTML");
    assert!(fragment.contains(r#"src="/.tachyon/tac-client.js""#));
    assert!(!fragment.contains("<section>"), "{fragment}");
}

#[test]
fn incremental_reuse_is_verified_and_handler_routes_are_not_built() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main><shared-card label=\"One\"></shared-card></main>",
    );
    let component = project
        .path()
        .join("client/components/shared/card/tac.html");
    write(&component, "<p>{label}</p>");

    let first = run(ty().arg("build").arg(project.path()));
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("compiled=1 reused=0"));
    let published = fs::read(project.path().join("dist/index.html")).expect("first");

    let second = run(ty().arg("build").arg(project.path()));
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(stdout(&second).contains("compiled=0 reused=1"));
    assert_eq!(
        published,
        fs::read(project.path().join("dist/index.html")).expect("reused")
    );

    write(&component, "<p class=\"changed\">{label}</p>");
    let changed = run(ty().arg("build").arg(project.path()));
    assert!(changed.status.success(), "{}", stderr(&changed));
    assert!(stdout(&changed).contains("compiled=1 reused=0"));

    write(
        &project.path().join("dist/.tachyon/build-state.json"),
        "{not-json",
    );
    let corrupt = run(ty().arg("build").arg(project.path()));
    assert!(corrupt.status.success(), "{}", stderr(&corrupt));
    assert!(stdout(&corrupt).contains("compiled=1 reused=0"));

    let forced = run(ty()
        .arg("build")
        .arg(project.path())
        .arg("--no-incremental"));
    assert!(forced.status.success(), "{}", stderr(&forced));
    assert!(stdout(&forced).contains("compiled=1 reused=0"));

    write(
        &project.path().join("server/routes/volatile/yon.js"),
        "@Controller\nexport class VolatileController { static GET() { return { value: 'fresh' } } }",
    );
    let volatile_first = run(ty().arg("build").arg(project.path()));
    assert!(
        volatile_first.status.success(),
        "{}",
        stderr(&volatile_first)
    );
    let volatile_second = run(ty().arg("build").arg(project.path()));
    assert!(
        volatile_second.status.success(),
        "{}",
        stderr(&volatile_second)
    );
    assert!(stdout(&volatile_first).contains("compiled=0 reused=1"));
    assert!(stdout(&volatile_second).contains("compiled=0 reused=1"));
}

#[test]
fn diagnostics_recover_across_sources_and_failed_builds_preserve_output() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Known good</main>",
    );
    let baseline = run(ty().arg("build").arg(project.path()));
    assert!(baseline.status.success(), "{}", stderr(&baseline));
    let published = fs::read(project.path().join("dist/index.html")).expect("baseline");

    write(
        &project.path().join("client/pages/tac.html"),
        // A call parses now and fails at render, so it no longer produces a
        // parse diagnostic. Trailing syntax still does, which is what this
        // test needs: two files, each contributing one parse error.
        "<main>{missing value}</main>",
    );
    write(
        &project.path().join("client/pages/second/tac.html"),
        "<logic else>orphan</logic>",
    );
    let failed = run(ty()
        .args(["--diagnostic-format", "json", "build"])
        .arg(project.path()));
    assert!(!failed.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&failed.stderr).expect("Diagnostics v1 JSON");
    let diagnostics = report["diagnostics"].as_array().expect("diagnostics");
    assert!(diagnostics.len() >= 2);
    assert!(diagnostics.iter().any(|value| value["code"] == "TY1303"));
    assert!(diagnostics.iter().any(|value| value["code"] == "TY1302"));
    assert_eq!(
        published,
        fs::read(project.path().join("dist/index.html")).expect("retained output")
    );
}

#[test]
fn component_cycles_and_missing_mount_modules_fail_closed() {
    let cycle = tempfile::tempdir().expect("cycle");
    write(
        &cycle.path().join("client/pages/tac.html"),
        "<cycle-one></cycle-one>",
    );
    write(
        &cycle.path().join("client/components/cycle/one/tac.html"),
        "<cycle-two></cycle-two>",
    );
    write(
        &cycle.path().join("client/components/cycle/two/tac.html"),
        "<cycle-one></cycle-one>",
    );
    let output = run(ty().arg("build").arg(cycle.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1403"));

    let component = tempfile::tempdir().expect("component");
    write(
        &component.path().join("client/pages/tac.html"),
        r#"<missing-module hydrate="visible"></missing-module>"#,
    );
    write(
        &component
            .path()
            .join("client/components/missing/module/tac.html"),
        "<p>Client content</p>",
    );
    let output = run(ty().arg("build").arg(component.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1405"));
}

#[test]
fn adversarial_parser_and_component_shapes_fail_through_the_binary() {
    let parser = tempfile::tempdir().expect("parser");
    write(
        &parser.path().join("client/pages/tac.html"),
        r#"
<main onclick="bad()" :hydrate="policy" :title="bad syntax">
  <script>bad()</script>
  <web-widget hydrate="load"></web-widget>
  <unknown>bad</unknown>
  <logic>bad</logic>
  <loop>bad</loop>
  <slot name="named"></slot>
</main>
"#,
    );
    let output = run(ty()
        .args(["--diagnostic-format", "json", "build"])
        .arg(parser.path()));
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stderr).expect("report");
    let codes = report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|value| value["code"].as_str())
        .collect::<Vec<_>>();
    for code in ["TY1303", "TY1306", "TY1402", "TY1404"] {
        assert!(codes.contains(&code), "missing {code}: {codes:?}");
    }

    let components = tempfile::tempdir().expect("components");
    write(
        &components
            .path()
            .join("client/components/bad/template/tac.html"),
        "<main>",
    );
    // A polyglot component companion has no adapter, so it is what must still
    // raise TY1401. tac.css is deliberately not used here: it is supported, and
    // a test that treats a working feature as a failure stops proving anything
    // the day it starts working.
    write(
        &components
            .path()
            .join("client/components/bad/template/tac.py"),
        "x = 1",
    );
    write(
        &components.path().join("client/pages/tac.html"),
        "<main>page</main>",
    );
    let output = run(ty().arg("build").arg(components.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1301"));
    assert!(stderr(&output).contains("TY1401"));

    let component_shapes = tempfile::tempdir().expect("component shapes");
    write(
        &component_shapes.path().join("client/pages/tac.html"),
        "<main>page</main>",
    );
    write(
        &component_shapes
            .path()
            .join("client/components/orphan/script/tac.js"),
        "export default class Orphan {}",
    );
    write(
        &component_shapes
            .path()
            .join("client/components/single/tac.html"),
        "<p>invalid path</p>",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let target = component_shapes.path().join("outside.html");
        write(&target, "<p>linked</p>");
        let link = component_shapes
            .path()
            .join("client/components/linked/card/tac.html");
        fs::create_dir_all(link.parent().expect("link parent")).expect("link directory");
        symlink(target, link).expect("component symlink");
    }
    let output = run(ty().arg("build").arg(component_shapes.path()));
    assert!(!output.status.success());
    // Project discovery owns the no-follow capability boundary, so the unsafe
    // component link is rejected before compiler-specific shape validation.
    assert!(stderr(&output).contains("TY1004"));

    let missing_slot = tempfile::tempdir().expect("missing slot");
    write(
        &missing_slot.path().join("client/pages/tac.html"),
        "<main><no-slot>child</no-slot></main>",
    );
    write(
        &missing_slot
            .path()
            .join("client/components/no/slot/tac.html"),
        "<p>component</p>",
    );
    let output = run(ty().arg("build").arg(missing_slot.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1404"));

    let root_file = tempfile::tempdir().expect("component root file");
    write(
        &root_file.path().join("client/pages/tac.html"),
        "<main>page</main>",
    );
    write(
        &root_file.path().join("client/components"),
        "not a directory",
    );
    let output = run(ty().arg("build").arg(root_file.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1001"));

    for bytes in [b"bad\0script".to_vec(), vec![b'x'; 1_048_577]] {
        let companion = tempfile::tempdir().expect("companion");
        write(
            &companion.path().join("client/pages/tac.html"),
            "<main><bad-script></bad-script></main>",
        );
        write(
            &companion
                .path()
                .join("client/components/bad/script/tac.html"),
            "<p>safe</p>",
        );
        let script = companion.path().join("client/components/bad/script/tac.js");
        fs::write(script, bytes).expect("invalid companion");
        let output = run(ty().arg("build").arg(companion.path()));
        assert!(!output.status.success());
        assert!(stderr(&output).contains("TY1401"));
    }

    for bytes in [vec![0xff], vec![b'x'; 1_048_577]] {
        let view = tempfile::tempdir().expect("invalid view");
        let path = view.path().join("client/pages/tac.html");
        fs::create_dir_all(path.parent().expect("view parent")).expect("view directory");
        fs::write(path, bytes).expect("invalid view bytes");
        let output = run(ty().arg("build").arg(view.path()));
        assert!(!output.status.success());
        assert!(stderr(&output).contains("TY1301"));
    }

    let deep_view = tempfile::tempdir().expect("deep view");
    let source = "<div>".repeat(65) + "x" + &"</div>".repeat(65);
    write(&deep_view.path().join("client/pages/tac.html"), &source);
    let output = run(ty().arg("build").arg(deep_view.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY13"));
}
