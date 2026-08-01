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
        &project.join("tachyon.json"),
        &format!(
            r#"{{"application":{{"name":"{name}","id":"{id}","version":"0.1.0","entry_route":"/"}}}}"#
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
    assert!(root.join("capability-manifest.json").is_file());
    assert!(root.join("artifact-manifest.json").is_file());
    assert!(root.join("native-index.json").is_file());
    assert!(root.join("web/index.html").is_file());

    let native: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("native-ui/root.json")).expect("Native UI"))
            .expect("Native UI JSON");
    assert_eq!(native["contract_version"], 1);
    assert_eq!(native["target"], target);
    assert_contract("native-ui", &native);

    let text = serde_json::to_string(&native).expect("Native UI text");
    for adapter in [
        "text.heading1",
        "control.button",
        "control.text_field",
        "content.output",
        "control.disclosure",
    ] {
        assert!(text.contains(adapter), "{target} is missing {adapter}");
    }
    assert!(text.contains(r#""kind":"web_surface""#));
    assert!(text.contains(r#""bridge":"none""#));
    assert!(text.contains("Increase count"));

    let capability: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("capability-manifest.json")).expect("capability manifest"),
    )
    .expect("capability JSON");
    assert_eq!(capability["default_policy"], "deny");
    assert_eq!(capability["remote_content_bridge"], false);
    assert_contract("capability-manifest", &capability);

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
fn ios_bundle_is_launchable_accessible_and_subtree_hybrid() {
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
    assert!(bundle.join("NativeUI/root.json").is_file());
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
    assert_eq!(artifact["target"]["abi"], "swiftui-simulator");

    let swift = fs::read_to_string(root.join("project/TachyonHost.swift")).expect("Swift host");
    assert!(swift.contains("import UIKit"));
    assert!(!swift.contains("import AppKit"));
    assert!(!swift.contains("WKScriptMessageHandler"));

    // The fallback subtree stays isolated while its native siblings remain native.
    let surfaces = fs::read_dir(root.join("web-surfaces"))
        .expect("WebSurfaces")
        .collect::<Result<Vec<_>, _>>()
        .expect("WebSurface entries");
    assert_eq!(surfaces.len(), 1);
    let fallback =
        fs::read_to_string(surfaces[0].path().join("index.html")).expect("fallback document");
    assert!(fallback.contains("Chart fallback"));
    assert!(fallback.contains("default-src 'none'"));
}

#[cfg(target_os = "macos")]
#[test]
fn ios_and_macos_share_one_semantic_native_view() {
    // Both Apple targets must lower the same HTML to the same adapters,
    // identities, and accessible names; only the platform tag differs.
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
                    .join("native-ui/root.json"),
            )
            .expect("Native UI"),
        )
        .expect("Native UI JSON")
    };
    let macos = read("macos");
    let ios = read("ios");
    assert_eq!(macos["target"], "macos");
    assert_eq!(ios["target"], "ios");

    // Only the platform tag and the human-readable fallback reason may differ.
    let macos_text = serde_json::to_string(&macos).expect("macOS text");
    let ios_text = serde_json::to_string(&ios).expect("iOS text");
    assert!(macos_text.contains("has no macOS native adapter"));
    assert!(ios_text.contains("has no iOS native adapter"));
    assert_eq!(
        macos_text
            .replace(r#""target":"macos""#, r#""target":"*""#)
            .replace("has no macOS native adapter", "has no * native adapter"),
        ios_text
            .replace(r#""target":"ios""#, r#""target":"*""#)
            .replace("has no iOS native adapter", "has no * native adapter"),
        "Apple targets diverged beyond their platform names"
    );
}

#[test]
fn native_targets_reject_invalid_views_before_running_any_toolchain() {
    // Planning failures must be identical across platforms and must never
    // reach a platform toolchain.
    for target in ["macos", "ios", "linux", "windows", "android"] {
        let project = tempfile::tempdir().expect("project");
        write(
            &project.path().join("tachyon.json"),
            r#"{"application":{"name":"Invalid","id":"dev.tachyon.invalid","version":"0.1.0","entry_route":"/"}}"#,
        );
        write(
            &project.path().join("client/pages/tac.html"),
            r#"<main><button aria-label="Broken" data-tachyon-action="increment:missing">Broken</button></main>"#,
        );
        let output = run(ty().arg("build").arg(project.path()).args([
            "--target",
            target,
            "--diagnostic-format",
            "json",
        ]));
        assert!(!output.status.success(), "{target} accepted invalid state");
        let text = stderr(&output);
        assert!(text.contains("TY1603"), "{target} diagnostic was {text}");
    }
}
