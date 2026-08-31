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

fn write_phase4_project(project: &Path) {
    write(
        &project.join("tac.config.js"),
        r#"export const application = {
  name: "PhaseFour",
  id: "dev.tachyon.phase-four",
  version: "0.0.1",
  entryRoute: "/",
}
"#,
    );
    write(
        &project.join("client/pages/tac.html"),
        r#"
    <w-app-bar role="banner" aria-label="Primary navigation"><strong>Native Catalog</strong></w-app-bar>
    <main aria-label="Phase Four demo">
      <h1>Native Catalog</h1>
      <p>Native Tac content</p>
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
"#,
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

/// A route may declare several companions, and the build picks one per target.
///
/// The four cardinalities the matrix has to answer are unit-tested in
/// `project.rs`. This is the half that only a real build shows: that the
/// chosen companion is the one staged, and the others are not compiled —
/// which is what stops a `tac.rs` beside a `tac.swift` from demanding a Rust
/// toolchain for a library the macOS host never calls.
#[cfg(target_os = "macos")]
#[test]
fn a_mixture_stages_only_the_companion_that_answers_the_target() {
    let project = tempfile::tempdir().expect("project");
    write_phase4_project(project.path());
    write(
        &project.path().join("client/pages/tac.swift"),
        "final class Companion {\n  let runtime = \"Swift\"\n}\n",
    );
    // Reaches macOS too, and loses to the platform's own language.
    write(
        &project.path().join("client/pages/tac.rs"),
        "pub struct Companion {\n    pub runtime: &'static str,\n}\n\n\
         impl Companion {\n    pub fn new() -> Self {\n        \
         Self { runtime: \"Rust\" }\n    }\n}\n",
    );

    let output = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "macos"]));
    assert!(output.status.success(), "{}", stderr(&output));

    let staged = project.path().join("dist/macos/project");
    assert!(staged.join("TachyonCompanion.swift").is_file());
    // The Rust one is never staged, so `rustc` is never asked for.
    assert!(!staged.join("companion.rs").exists());
    let host = fs::read_to_string(staged.join("TachyonHost.swift")).expect("host");
    assert!(host.contains("tacNativeInvoke(payload)"), "{host}");
    assert!(!host.contains("tacRustInvoke"), "{host}");
}

/// The web is a target too, and the same rule refuses a route it cannot run.
#[cfg(target_os = "macos")]
#[test]
fn multiple_native_routes_compile_and_swift_state_is_isolated() {
    let project = tempfile::tempdir().expect("project");
    for route in ["", "second/"] {
        write(
            &project.path().join(format!("client/pages/{route}tac.html")),
            "<main>{count}</main>",
        );
        write(
            &project
                .path()
                .join(format!("client/pages/{route}tac.swift")),
            "final class Companion {\n var count: Int = 0\n func doubled() -> Int { count * 2 }\n}\n",
        );
    }
    write(
        &project.path().join("client/pages/rust/tac.html"),
        "<main>{count}</main>",
    );
    write(
        &project.path().join("client/pages/rust/tac.rs"),
        "#[derive(Default)]\nstruct Companion {\n count: i64,\n}\nimpl Companion {\n fn doubled(&self) -> i64 { self.count * 2 }\n}\n",
    );
    let output = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "macos"]));
    assert!(output.status.success(), "{}", stderr(&output));
    let stage = project.path().join("dist/macos/project");
    let probe = stage.join("Probe.swift");
    write(
        &probe,
        r#"import Foundation
@main struct Probe {
 static func main() throws {
  func invoke(_ request: String) throws -> [String: Any] {
   try JSONSerialization.jsonObject(with: Data(tacNativeInvoke(request).utf8)) as! [String: Any]
  }
  let first = try invoke("{\"route\":\"/\",\"op\":\"init\"}")
  precondition(first["value"] is [String: Any])
  _ = try invoke("{\"route\":\"/\",\"op\":\"set\",\"name\":\"count\",\"value\":7}")
  let doubled = try invoke("{\"route\":\"/\",\"op\":\"call\",\"name\":\"doubled\",\"args\":[]}")
  precondition(doubled["value"] as? Int == 14)
  let second = try invoke("{\"route\":\"/second\",\"op\":\"get\",\"name\":\"count\"}")
  precondition(second["value"] as? Int == 0)
  print("Swift native route isolation passed")
 }
}
"#,
    );
    let executable = stage.join("probe");
    // The application host supplies these shared JSON definitions. This
    // standalone companion probe supplies the same source without the GUI.
    let json_helper = stage.join("AppleJSON.swift");
    write(
        &json_helper,
        include_str!("../../tachyon-core/src/native/apple_json.swift"),
    );
    let compiled = run(Command::new("/usr/bin/xcrun")
        .arg("swiftc")
        .arg("-O")
        .arg(&json_helper)
        .arg(stage.join("TachyonCompanion.swift"))
        .arg(&probe)
        .arg("-o")
        .arg(&executable));
    assert!(compiled.status.success(), "{}", stderr(&compiled));
    let evidence = run(&mut Command::new(executable));
    assert!(evidence.status.success(), "{}", stderr(&evidence));
    let host = fs::read_to_string(stage.join("TachyonHost.swift")).expect("host");
    assert!(host.contains("tacRouteMembers") && host.contains("tacRustInvoke"));
}

/// The web is a target too, and the same rule refuses a route it cannot run.
#[test]
fn a_route_with_only_compiled_companions_has_no_behaviour_on_the_web() {
    let project = tempfile::tempdir().expect("project");
    write_phase4_project(project.path());
    fs::remove_file(project.path().join("client/pages/tac.js")).ok();
    write(
        &project.path().join("client/pages/tac.swift"),
        "final class Companion {\n  let runtime = \"Swift\"\n}\n",
    );

    let output = run(ty().arg("bundle").arg(project.path()));
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("TY1010"), "{message}");
    assert!(message.contains("on the web"), "{message}");

    // Adding a browser companion is the fix the diagnostic names.
    write(
        &project.path().join("client/pages/tac.js"),
        "export default class {\n  runtime = 'JavaScript'\n}\n",
    );
    let mixed = run(ty().arg("bundle").arg(project.path()));
    assert!(mixed.status.success(), "{}", stderr(&mixed));
}

/// Legal JavaScript module helpers remain available to browser companions.
#[test]
fn browser_companions_preserve_legal_module_level_helpers() {
    let project = tempfile::tempdir().expect("project");
    write_phase4_project(project.path());
    write(
        &project.path().join("client/pages/tac.js"),
        "import '/shared/nothing.js'\n\nconst LIMIT = 3\n\nexport default class {\n  count = LIMIT\n}\n",
    );

    let output = run(ty().arg("bundle").arg(project.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    let module = fs::read_to_string(project.path().join("dist/web/client.js")).expect("module");
    assert!(module.contains("const LIMIT = 3"));
}

/// A companion language only compiles for the targets whose toolchain it
/// belongs to, so building any other one has to fail rather than quietly
/// produce an application whose route has no behaviour at all.
#[test]
fn a_companion_that_cannot_reach_the_target_fails_the_build() {
    let project = tempfile::tempdir().expect("project");
    write_phase4_project(project.path());
    write(
        &project.path().join("client/pages/tac.swift"),
        "final class Page {\n  var seed = 21\n}\n",
    );

    let output = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "windows"]));
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("TY1010"), "{message}");
    // Both halves: which language was declared and what it reaches, and the
    // language that would reach the target being built.
    assert!(message.contains("Swift (macos, ios)"), "{message}");
    assert!(message.contains("a tac.rs or a tac.cs"), "{message}");
}

// Publish-or-preserve behavior for every supported target is asserted by
// `phase5_cli::every_native_target_publishes_under_its_own_directory`, which
// does not assume which platform toolchains the build machine has.

#[cfg(target_os = "macos")]
#[test]
fn macos_bundle_hosts_the_applications_own_web_bundle() {
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
    assert!(root.join("artifact-manifest.json").is_file());
    assert!(root.join("native-index.json").is_file());
    assert!(root.join("web/index.html").is_file());
    // The bundle the browser gets is the bundle the host shows.
    assert!(
        bundle
            .join("Contents/Resources/WebBundle/index.html")
            .is_file()
    );

    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("native-index.json")).expect("route index"))
            .expect("route index JSON");
    assert_eq!(index["contract_version"], 2);
    assert_eq!(index["entry_route"], "/");
    assert_eq!(index["entry_document"], "index.html");

    let artifact: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("artifact-manifest.json")).expect("artifact manifest"),
    )
    .expect("artifact JSON");
    assert_eq!(artifact["target"]["os"], "macos");
    assert_contract("artifact-manifest", &artifact);
    let descriptor: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("tachyon.host.json")).expect("native host descriptor"),
    )
    .expect("native host JSON");
    assert_contract("native-host", &descriptor);
    assert_eq!(artifact["contracts"]["native_host"], 3);
    assert!(artifact["contracts"].get("native_ui").is_none());
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
        &project.path().join("tac.config.js"),
        r#"export const application = {
  name: "Rollback",
  id: "dev.tachyon.rollback",
  version: "0.0.1",
  entryRoute: "/",
}
"#,
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

    // A configuration that names an entry route with no document leaves the
    // host nothing to open, which is the last thing a build can catch.
    write(
        &project.path().join("tac.config.js"),
        r#"export const application = {
  name: "Rollback",
  id: "dev.tachyon.rollback",
  version: "0.0.1",
  entryRoute: "/missing",
}
"#,
    );
    let failed = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "macos"]));
    assert!(!failed.status.success());
    assert!(stderr(&failed).contains("TY1601"), "{}", stderr(&failed));
    assert_eq!(
        fs::read(executable).expect("preserved executable"),
        published
    );
}
