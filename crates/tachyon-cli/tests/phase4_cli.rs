//! Phase 4 behavior tests against the compiled `ty` executable.

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

#[cfg(target_os = "macos")]
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[cfg(target_os = "macos")]
fn assert_contract(name: &str, instance: &serde_json::Value) {
    let contract = tachyon_contracts::find(name).expect("registered contract");
    let schema = tachyon_contracts::parse_schema(contract).expect("canonical schema");
    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    assert!(
        validator.is_valid(instance),
        "{name} errors: {:?}",
        validator.iter_errors(instance).collect::<Vec<_>>()
    );
}

#[cfg(target_os = "macos")]
fn write_phase4_project(project: &Path) {
    write(
        &project.join("tachyon.json"),
        r#"{
  "application": {
    "name": "PhaseFour",
    "id": "dev.tachyon.phase-four",
    "version": "0.0.1",
    "entry_route": "/"
  }
}"#,
    );
    write(
        &project.join("server/routes/yon.html"),
        r#"
<!doctype html>
<html>
  <head><title>{title}</title></head>
  <body>
    <w-app-bar role="banner" aria-label="Primary navigation"><strong>{title}</strong></w-app-bar>
    <main aria-label="Phase Four demo">
      <h1>{title}</h1>
      <logic :if="available"><p>{message}</p></logic>
      <logic else><p>Unavailable</p></logic>
      <status-card><span>Slotted native content</span></status-card>
      <button
        aria-label="Increase count"
        data-tachyon-action="increment:count"
      >Increase</button>
      <output
        aria-label="Current count"
        data-tachyon-bind="count"
        data-tachyon-state="0"
      >0</output>
      <input
        aria-label="Customer name"
        data-tachyon-bind="customer"
        data-tachyon-state=""
        placeholder="Name"
      >
      <fancy-chart aria-label="Sales chart"><p>Web chart fallback</p></fancy-chart>
      <footer><small>Native sibling after fallback</small></footer>
    </main>
  </body>
</html>
"#,
    );
    write(
        &project.join("server/routes/yon.js"),
        r"
export class Handler {
  static title = 'Native Catalog'
  static GET() {
    return { available: true, message: 'Resolved Yon context' }
  }
}
",
    );
    write(
        &project.join("client/components/status/card/tac.html"),
        "<article aria-label=\"Status card\"><slot></slot></article>",
    );
}

#[test]
fn unknown_build_targets_are_rejected_before_project_mutation() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>safe</main>",
    );
    let output = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "solaris"]));
    assert!(!output.status.success());
    assert!(!project.path().join("dist").exists());
}

// Publish-or-preserve behavior for every supported target is asserted by
// `phase5_cli::every_native_target_publishes_under_its_own_directory`, which
// does not assume which platform toolchains the build machine has.

#[cfg(target_os = "macos")]
#[test]
fn macos_bundle_is_native_accessible_interactive_and_subtree_hybrid() {
    let project = tempfile::tempdir().expect("project");
    write_phase4_project(project.path());

    let output = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "macos"]));
    assert!(output.status.success(), "{}", stderr(&output));

    let root = project.path().join("dist/macos");
    let bundle = root.join("PhaseFour.app");
    assert!(bundle.join("Contents/MacOS/PhaseFour").is_file());
    assert!(bundle.join("Contents/Info.plist").is_file());
    assert!(root.join("project/TachyonHost.swift").is_file());
    assert!(root.join("capability-manifest.json").is_file());
    assert!(root.join("artifact-manifest.json").is_file());
    assert!(root.join("native-index.json").is_file());
    assert!(root.join("web/index.html").is_file());

    let native: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("native-ui/root.json")).expect("Native UI"))
            .expect("Native UI JSON");
    assert_eq!(native["contract_version"], 1);
    assert_eq!(native["target"], "macos");
    assert_contract("native-ui", &native);
    let text = serde_json::to_string(&native).expect("Native UI text");
    for adapter in [
        "layout.app_bar",
        "text.heading1",
        "control.button",
        "control.text_field",
        "content.output",
    ] {
        assert!(text.contains(adapter), "missing {adapter}");
    }
    assert!(text.contains(r#""role":"main""#));
    assert!(text.contains("Increase count"));
    assert!(text.contains(r#""kind":"web_surface""#));
    assert!(text.contains("WebSurfaces/"));
    assert!(text.contains("Native sibling after fallback"));
    for control in ["logic", "loop", r#""kind":"component""#] {
        assert!(!text.contains(control), "{control} leaked into Native UI");
    }

    let surfaces = fs::read_dir(root.join("web-surfaces"))
        .expect("WebSurfaces")
        .collect::<Result<Vec<_>, _>>()
        .expect("WebSurface entries");
    assert_eq!(surfaces.len(), 1);
    let fallback =
        fs::read_to_string(surfaces[0].path().join("index.html")).expect("fallback document");
    assert!(fallback.contains("Web chart fallback"));
    assert!(fallback.contains("Content-Security-Policy"));

    let capability: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("capability-manifest.json")).expect("capability manifest"),
    )
    .expect("capability JSON");
    assert_eq!(capability["default_policy"], "deny");
    assert_eq!(capability["remote_content_bridge"], false);
    assert_eq!(capability["capabilities"], serde_json::json!([]));
    assert_contract("capability-manifest", &capability);

    let artifact: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("artifact-manifest.json")).expect("artifact manifest"),
    )
    .expect("artifact JSON");
    assert_eq!(artifact["target"]["os"], "macos");
    assert_contract("artifact-manifest", &artifact);
    assert!(
        artifact["outputs"]
            .as_array()
            .is_some_and(|value| !value.is_empty())
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_native_failures_preserve_the_last_complete_application() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("tachyon.json"),
        r#"{"application":{"name":"Rollback","id":"dev.tachyon.rollback","version":"0.0.1","entry_route":"/"}}"#,
    );
    let source = project.path().join("client/pages/tac.html");
    write(
        &source,
        r#"<main><button aria-label="Increase" data-tachyon-action="increment:count">Increase</button><output data-tachyon-bind="count" data-tachyon-state="0">0</output></main>"#,
    );
    let first = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "macos"]));
    assert!(first.status.success(), "{}", stderr(&first));
    let executable = project
        .path()
        .join("dist/macos/Rollback.app/Contents/MacOS/Rollback");
    let published = fs::read(&executable).expect("published executable");

    write(
        &source,
        r#"<main><button aria-label="Broken" data-tachyon-action="increment:missing">Broken</button></main>"#,
    );
    let failed = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "macos"]));
    assert!(!failed.status.success());
    assert!(stderr(&failed).contains("TY1603"));
    assert_eq!(
        fs::read(executable).expect("preserved executable"),
        published
    );
}
