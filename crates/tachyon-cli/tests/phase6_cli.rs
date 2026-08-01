//! Phase 6 migration tests against the compiled `ty` executable.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn ty() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ty"))
}

fn run(command: &mut Command) -> Output {
    command.output().expect("the ty process should start")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
    fs::write(path, contents).expect("fixture source");
}

/// Returns the repository root, which holds the corpus and legacy fixtures.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root")
        .to_path_buf()
}

#[test]
fn a_supported_project_passes_the_migration_check() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main aria-label=\"Home\"><h1>Home</h1></main>",
    );
    let output = run(ty().arg("migrate").arg("check").arg(project.path()));
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(stdout(&output).contains("0 unsupported"));
}

#[test]
fn unsupported_constructs_fail_closed_with_a_stable_code() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>x</main>",
    );
    // A polyglot view companion has no equivalent here, so it is what the
    // check must refuse to pass over. Middleware is deliberately not used for
    // this: it is supported, and a test that treats a working feature as a
    // blocker stops proving anything the day it starts working.
    write(&project.path().join("client/pages/tac.py"), "x = 1");

    let strict = run(ty().arg("migrate").arg("check").arg(project.path()));
    assert!(!strict.status.success());
    assert!(String::from_utf8_lossy(&strict.stderr).contains("TY1702"));

    let permissive = run(ty()
        .arg("migrate")
        .arg("check")
        .arg(project.path())
        .arg("--allow-unsupported"));
    assert!(permissive.status.success());
    assert!(stdout(&permissive).contains("companion.polyglot"));
}

#[test]
fn the_report_is_deterministic_and_schema_shaped() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>x</main>",
    );
    write(
        &project.path().join("client/pages/tac.js"),
        "export default {}",
    );
    write(
        &project.path().join("server/routes/yon.py"),
        "class Handler: pass",
    );

    let first = run(ty()
        .arg("migrate")
        .arg("check")
        .arg(project.path())
        .args(["--json", "--allow-unsupported"]));
    let second = run(ty()
        .arg("migrate")
        .arg("check")
        .arg(project.path())
        .args(["--json", "--allow-unsupported"]));
    assert!(first.status.success());
    assert_eq!(
        stdout(&first),
        stdout(&second),
        "report is not deterministic"
    );

    let report: serde_json::Value =
        serde_json::from_str(&stdout(&first)).expect("Migration Report v1 JSON");
    assert_eq!(report["contract_version"], 1);
    let findings = report["findings"].as_array().expect("findings");
    assert!(!findings.is_empty());
    for finding in findings {
        assert!(finding["source"].is_string());
        assert!(finding["feature"].is_string());
        assert!(finding["detail"].is_string());
        let status = finding["status"].as_str().expect("status");
        assert!(matches!(status, "supported" | "changed" | "unsupported"));
        if status != "supported" {
            assert!(
                finding["action"].is_string(),
                "{} carries no action",
                finding["feature"]
            );
        }
    }
}

#[test]
fn the_real_legacy_fixture_is_classified_without_being_executed() {
    let fixture = repository_root().join("tests/fixtures/fullstack");
    if !fixture.is_dir() {
        // The legacy oracle is absent from this checkout; nothing to classify.
        return;
    }
    let output = run(ty()
        .arg("migrate")
        .arg("check")
        .arg(&fixture)
        .args(["--json", "--allow-unsupported"]));
    assert!(output.status.success(), "{}", stdout(&output));

    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("report");
    let features: Vec<&str> = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|finding| finding["feature"].as_str())
        .collect();
    // The fixture exercises exactly the surfaces the ledger records as absent.
    for expected in [
        "handler.supervised",
        "handler.other_language",
        "handler.dependency",
        "server.worker",
    ] {
        assert!(features.contains(&expected), "missing {expected}");
    }
    assert!(
        report["unsupported"].as_u64().unwrap_or_default() > 0,
        "the legacy fixture should not be fully supported yet"
    );
}

#[test]
fn every_corpus_project_builds_under_this_implementation() {
    // The compatibility corpus is the intersection both implementations can
    // build. The Rust half of that promise is enforced here; the differential
    // harness enforces the other half.
    let corpus = repository_root().join("corpus");
    if !corpus.is_dir() {
        return;
    }
    let mut projects: Vec<PathBuf> = fs::read_dir(&corpus)
        .expect("corpus")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .collect();
    projects.sort();
    assert!(!projects.is_empty(), "the corpus is empty");

    for project in projects {
        let staged = tempfile::tempdir().expect("staging");
        copy_tree(&project, staged.path());
        let build = run(ty()
            .arg("build")
            .arg(staged.path())
            .args(["--out-dir", "dist-rust"]));
        assert!(
            build.status.success(),
            "{} failed: {}",
            project.display(),
            String::from_utf8_lossy(&build.stderr)
        );
        assert!(
            staged
                .path()
                .join("dist-rust/route-manifest.json")
                .is_file()
        );

        let check = run(ty().arg("migrate").arg("check").arg(staged.path()));
        assert!(
            check.status.success(),
            "{} has unsupported constructs: {}",
            project.display(),
            stdout(&check)
        );
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    for entry in fs::read_dir(source).expect("corpus entries") {
        let entry = entry.expect("corpus entry");
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            fs::create_dir_all(&target).expect("directory");
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("file");
        }
    }
}
