//! Phase 5 platform-expansion tests against the compiled `ty` executable.

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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
    fs::write(path, contents).expect("fixture source");
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

/// Writes the shared cross-platform fixture exercising every Phase 5 adapter.
fn write_phase5_project(project: &Path, name: &str, id: &str) {
    write(
        &project.join("tac.config.js"),
        &format!(
            "export const application = {{\n  name: '{name}',\n  id: '{id}',\n  version: '0.1.0',\n  entryRoute: '/',\n}}\n"
        ),
    );
    write(
        &project.join("client/pages/tac.html"),
        r#"<main aria-label="Phase Five demo">
  <h1>Phase Five</h1>
  <p>Cross-platform native adapters.</p>
  <button aria-label="Increase count" data-tachyon-action="increment:count">Add one</button>
  <output aria-label="Count" data-tachyon-bind="count" data-tachyon-state="0">0</output>
  <input aria-label="Your name" data-tachyon-bind="name" data-tachyon-state="" placeholder="Name">
  <details aria-label="More detail"><summary>More detail</summary><p>Disclosure content.</p></details>
  <x-chart aria-label="Sales chart"><p>Chart fallback</p></x-chart>
</main>
"#,
    );
}

/// Asserts the platform-neutral staging every Phase 5 target must publish.
#[cfg(target_os = "macos")]
fn assert_common_staging(root: &Path, target: &str) {
    assert!(root.join("artifact-manifest.json").is_file());
    assert!(root.join("native-index.json").is_file());
    assert!(root.join("web/index.html").is_file());

    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("native-index.json")).expect("route index"))
            .expect("route index JSON");
    assert_eq!(index["contract_version"], 2, "{target}");
    assert_eq!(index["entry_route"], "/", "{target}");
    assert_eq!(index["entry_document"], "index.html", "{target}");

    let host: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("tachyon.host.json")).expect("host descriptor"))
            .expect("host descriptor JSON");
    assert_eq!(host["target"], target);
    // One rendering, described once: there is no adapter table to advertise.
    assert_eq!(host["renderMode"], "bundle");

    let artifact: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("artifact-manifest.json")).expect("artifact manifest"),
    )
    .expect("artifact JSON");
    assert_contract("artifact-manifest", &artifact);
    assert!(
        artifact["outputs"]
            .as_array()
            .is_some_and(|value| !value.is_empty())
    );
}

#[test]
fn every_native_target_publishes_under_its_own_directory() {
    // Targets are isolated so one platform's failure never disturbs another's
    // published output.
    for (target, directory) in [
        ("macos", "macos"),
        ("ios", "ios"),
        ("linux", "linux"),
        ("windows", "windows"),
        ("android", "android"),
    ] {
        let project = tempfile::tempdir().expect("project");
        write_phase5_project(project.path(), "Isolation", "dev.tachyon.isolation");
        let output = run(ty()
            .arg("build")
            .arg(project.path())
            .args(["--target", target]));
        let published = project.path().join("dist").join(directory);
        if output.status.success() {
            assert!(published.is_dir(), "{target} published nothing on success");
        } else {
            assert!(!published.exists(), "{target} published a failed build");
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn ios_bundle_is_launchable_and_hosts_the_application_web_bundle() {
    let project = tempfile::tempdir().expect("project");
    write_phase5_project(project.path(), "PhaseFive", "dev.tachyon.phase-five");

    let output = run(ty()
        .arg("build")
        .arg(project.path())
        .args(["--target", "ios"]));
    assert!(output.status.success(), "{}", stderr(&output));

    let root = project.path().join("dist/ios");
    let bundle = root.join("PhaseFive.app");
    assert!(bundle.join("PhaseFive").is_file(), "missing executable");
    assert!(bundle.join("Info.plist").is_file());
    assert!(bundle.join("NativeIndex.json").is_file());
    assert!(bundle.join("WebBundle/index.html").is_file());
    assert!(bundle.join("_CodeSignature").is_dir(), "bundle is unsigned");
    assert!(root.join("project/TachyonHost.swift").is_file());
    assert_common_staging(&root, "ios");

    let plist = fs::read_to_string(bundle.join("Info.plist")).expect("Info.plist");
    assert!(plist.contains("<string>iPhoneSimulator</string>"));
    assert!(plist.contains("<key>UILaunchScreen</key>"));
    assert!(plist.contains("dev.tachyon.phase-five"));

    let artifact: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("artifact-manifest.json")).expect("artifact manifest"),
    )
    .expect("artifact JSON");
    assert_eq!(artifact["target"]["os"], "ios");
    assert_eq!(artifact["target"]["abi"], "wkwebview-simulator");

    let swift = fs::read_to_string(root.join("project/TachyonHost.swift")).expect("Swift host");
    assert!(swift.contains("import UIKit"));
    assert!(!swift.contains("import AppKit"));
    // The page reaches the host through one bridge, which is the whole native
    // surface an application gets.
    assert!(swift.contains("WKScriptMessageHandlerWithReply"));
    assert!(swift.contains("tachyon-app://bundle/"));
}

#[cfg(target_os = "macos")]
#[test]
fn ios_and_macos_host_the_same_bundle() {
    // Both Apple targets host the application's own bundle, so the route index
    // they publish is identical: there is no per-platform lowering left to
    // diverge.
    let project = tempfile::tempdir().expect("project");
    write_phase5_project(project.path(), "Parity", "dev.tachyon.parity");
    for target in ["macos", "ios"] {
        let output = run(ty()
            .arg("build")
            .arg(project.path())
            .args(["--target", target]));
        assert!(output.status.success(), "{target}: {}", stderr(&output));
    }

    let read = |target: &str| -> serde_json::Value {
        serde_json::from_slice(
            &fs::read(
                project
                    .path()
                    .join("dist")
                    .join(target)
                    .join("native-index.json"),
            )
            .expect("route index"),
        )
        .expect("route index JSON")
    };
    assert_eq!(read("macos"), read("ios"));
}

#[test]
fn native_targets_reject_an_unreachable_entry_route_before_any_toolchain() {
    // The view is no longer lowered, so there is nothing in it left to reject.
    // An entry route with no document still leaves a host nothing to open, and
    // every platform must say so identically without reaching a toolchain.
    for target in ["macos", "ios", "linux", "windows", "android"] {
        let project = tempfile::tempdir().expect("project");
        write(
            &project.path().join("tac.config.js"),
            r#"export const application = {
  name: "Invalid",
  id: "dev.tachyon.invalid",
  version: "0.1.0",
  entryRoute: "/nowhere",
}
"#,
        );
        write(
            &project.path().join("client/pages/tac.html"),
            r#"<main aria-label="Home"><h1>Home</h1></main>"#,
        );
        let output = run(ty().arg("build").arg(project.path()).args([
            "--target",
            target,
            "--diagnostic-format",
            "json",
        ]));
        assert!(
            !output.status.success(),
            "{target} accepted an unreachable entry route"
        );
        let text = stderr(&output);
        assert!(text.contains("TY1601"), "{target} diagnostic was {text}");
    }
}
