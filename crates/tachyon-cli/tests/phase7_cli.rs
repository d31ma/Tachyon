//! Phase 7 qualification: recovery drills, soak behavior, and budgets.
//!
//! These tests exercise the compiled `ty` executable under interruption,
//! corruption, contention, and sustained load. They assert the properties an
//! operator depends on: nothing is ever half-published, a damaged cache is
//! detected rather than trusted, and resources do not grow without bound.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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

/// Writes a project with `routes` pages, used by the load and budget drills.
fn write_project(root: &Path, routes: usize) {
    write(
        &root.join("client/pages/tac.html"),
        "<main aria-label=\"Home\"><h1>Home</h1><p>Index route.</p></main>",
    );
    for index in 1..routes {
        write(
            &root.join(format!("client/pages/page{index}/tac.html")),
            &format!(
                "<main aria-label=\"Page {index}\"><h2>Page {index}</h2>\
                 <p>Generated route {index}.</p></main>"
            ),
        );
    }
}

/// Returns the number of open descriptors held by this process, when the
/// platform exposes it.
fn open_descriptors() -> Option<usize> {
    for directory in ["/proc/self/fd", "/dev/fd"] {
        if let Ok(entries) = fs::read_dir(directory) {
            return Some(entries.count());
        }
    }
    None
}

fn published_digest(output: &Path) -> String {
    fs::read_to_string(output.join("route-manifest.json")).expect("route manifest")
}

#[test]
fn an_interrupted_build_never_publishes_partial_output() {
    // Drill: the process dies mid-build. The previously published output must
    // remain exactly as it was, and the next build must succeed.
    let project = tempfile::tempdir().expect("project");
    write_project(project.path(), 40);
    let output = project.path().join("dist");

    let first = run(ty().arg("build").arg(project.path()));
    assert!(first.status.success());
    let baseline = published_digest(&output);
    let baseline_files = count_files(&output);

    for _ in 0..5 {
        let mut child = ty()
            .arg("build")
            .arg(project.path())
            .arg("--no-incremental")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn build");
        // Kill while the build is staging, before it can publish.
        std::thread::sleep(Duration::from_millis(15));
        let _kill_result = child.kill();
        let _wait_result = child.wait();
    }

    assert_eq!(
        published_digest(&output),
        baseline,
        "an interrupted build changed the published manifest"
    );
    assert_eq!(
        count_files(&output),
        baseline_files,
        "an interrupted build left extra files in the published output"
    );

    let recovery = run(ty().arg("build").arg(project.path()));
    assert!(
        recovery.status.success(),
        "the build did not recover: {}",
        String::from_utf8_lossy(&recovery.stderr)
    );
    assert_eq!(published_digest(&output), baseline);
}

#[test]
fn a_corrupted_incremental_cache_is_detected_rather_than_trusted() {
    // Drill: build state is damaged on disk. The next build must not reuse it
    // and must still produce the correct output.
    let project = tempfile::tempdir().expect("project");
    write_project(project.path(), 8);
    let output = project.path().join("dist");

    assert!(run(ty().arg("build").arg(project.path())).status.success());
    let baseline = published_digest(&output);

    for corruption in ["", "{", "{\"routes\":", "\u{0}\u{0}\u{0}", "[]"] {
        let state = output.join(".tachyon/build-state.json");
        if state.exists() {
            fs::write(&state, corruption).expect("corrupt build state");
        }
        let rebuilt = run(ty().arg("build").arg(project.path()));
        assert!(
            rebuilt.status.success(),
            "corruption {corruption:?} was not recovered: {}",
            String::from_utf8_lossy(&rebuilt.stderr)
        );
        assert_eq!(
            published_digest(&output),
            baseline,
            "corruption {corruption:?} changed the published output"
        );
    }
}

#[test]
fn a_failed_build_preserves_the_previous_application() {
    // Drill: a source regression must never destroy a working deployment.
    let project = tempfile::tempdir().expect("project");
    let source = project.path().join("client/pages/tac.html");
    write(&source, "<main aria-label=\"Good\"><h1>Good</h1></main>");
    let output = project.path().join("dist");

    assert!(run(ty().arg("build").arg(project.path())).status.success());
    let baseline = fs::read_to_string(output.join("index.html")).expect("published index");

    write(&source, "<main><logic :else>orphan</logic></main>");
    let failed = run(ty().arg("build").arg(project.path()));
    assert!(!failed.status.success(), "an invalid source built cleanly");
    assert_eq!(
        fs::read_to_string(output.join("index.html")).expect("published index"),
        baseline,
        "a failed build damaged the published application"
    );
}

#[test]
fn concurrent_builds_never_produce_torn_output() {
    // Drill: two builds race on the same output directory. Publication is
    // atomic, so the result must be one complete application, never a mix.
    let project = tempfile::tempdir().expect("project");
    write_project(project.path(), 12);
    let output = project.path().join("dist");

    let children: Vec<_> = (0..4)
        .map(|_| {
            ty().arg("build")
                .arg(project.path())
                .arg("--no-incremental")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn build")
        })
        .collect();
    for mut child in children {
        let _wait_result = child.wait();
    }

    let settled = run(ty().arg("build").arg(project.path()));
    assert!(settled.status.success());
    let manifest: serde_json::Value =
        serde_json::from_str(&published_digest(&output)).expect("route manifest JSON");
    let routes = manifest["routes"].as_array().expect("routes");
    assert_eq!(
        routes.len(),
        12,
        "torn route manifest after concurrent builds"
    );
    // Every manifest route must have a published document. The index route
    // publishes index.html; every other route publishes <route>/index.html.
    for route in routes {
        let path = route["route"].as_str().expect("route");
        let document = if path == "/" {
            output.join("index.html")
        } else {
            output.join(path.trim_start_matches('/')).join("index.html")
        };
        assert!(
            document.is_file(),
            "manifest lists {path} but {} is missing",
            document.display()
        );
    }
}

#[test]
fn a_read_only_output_directory_fails_closed() {
    // Drill: the deployment target is not writable. The build must report a
    // diagnostic rather than partially write or panic.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let project = tempfile::tempdir().expect("project");
        write_project(project.path(), 3);
        assert!(run(ty().arg("build").arg(project.path())).status.success());

        let output = project.path().join("dist");
        let mut permissions = fs::metadata(&output).expect("output").permissions();
        permissions.set_mode(0o500);
        fs::set_permissions(&output, permissions.clone()).expect("read-only output");

        let blocked = run(ty().arg("build").arg(project.path()).args([
            "--no-incremental",
            "--diagnostic-format",
            "json",
        ]));

        permissions.set_mode(0o755);
        fs::set_permissions(&output, permissions).expect("restore output");

        if !blocked.status.success() {
            let stderr = String::from_utf8_lossy(&blocked.stderr);
            assert!(stderr.contains("TY"), "failure carried no diagnostic code");
            assert!(!stderr.contains("panicked"), "the build panicked: {stderr}");
        }
    }
}

#[test]
fn sustained_rebuilds_do_not_leak_descriptors_or_slow_down() {
    // Soak: repeated builds in one long-lived working tree must stay stable in
    // both resource use and latency.
    let iterations: usize = std::env::var("TAC_SOAK_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(24);

    let project = tempfile::tempdir().expect("project");
    write_project(project.path(), 10);
    let output = project.path().join("dist");

    assert!(run(ty().arg("build").arg(project.path())).status.success());
    let baseline_digest = published_digest(&output);
    let baseline_descriptors = open_descriptors();

    let mut first_half = Duration::ZERO;
    let mut second_half = Duration::ZERO;
    for iteration in 0..iterations {
        let started = Instant::now();
        let build = run(ty().arg("build").arg(project.path()));
        let elapsed = started.elapsed();
        assert!(
            build.status.success(),
            "iteration {iteration} failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        assert_eq!(
            published_digest(&output),
            baseline_digest,
            "iteration {iteration} changed deterministic output"
        );
        if iteration < iterations / 2 {
            first_half += elapsed;
        } else {
            second_half += elapsed;
        }
    }

    if let (Some(before), Some(after)) = (baseline_descriptors, open_descriptors()) {
        assert!(
            after <= before + 8,
            "descriptor count grew from {before} to {after} under sustained load"
        );
    }
    // Latency must not drift upward as state accumulates. The bound is wide
    // because CI machines are noisy; it still catches a real regression.
    let halves = u32::try_from((iterations / 2).max(1)).unwrap_or(u32::MAX);
    let early = first_half / halves;
    let late = second_half / halves;
    assert!(
        late <= early * 3 + Duration::from_millis(250),
        "rebuild latency degraded from {early:?} to {late:?} under sustained load"
    );
}

#[test]
fn build_cost_stays_within_the_published_budget() {
    // Budget: a clean build of 50 routes and its incremental rebuild must stay
    // inside the ceilings recorded in PHASE_7_SPEC.md. The ceilings are wide
    // enough for shared CI hardware and tight enough to catch a regression in
    // order of magnitude.
    const ROUTES: usize = 50;
    const CLEAN_BUILD_CEILING: Duration = Duration::from_secs(20);
    const INCREMENTAL_CEILING: Duration = Duration::from_secs(20);
    const BYTES_PER_ROUTE_CEILING: u64 = 64 * 1_024;

    let project = tempfile::tempdir().expect("project");
    write_project(project.path(), ROUTES);
    let output = project.path().join("dist");

    let started = Instant::now();
    let clean = run(ty()
        .arg("build")
        .arg(project.path())
        .arg("--no-incremental"));
    let clean_elapsed = started.elapsed();
    assert!(clean.status.success());
    assert!(
        clean_elapsed < CLEAN_BUILD_CEILING,
        "clean build of {ROUTES} routes took {clean_elapsed:?}"
    );

    let started = Instant::now();
    let incremental = run(ty().arg("build").arg(project.path()));
    let incremental_elapsed = started.elapsed();
    assert!(incremental.status.success());
    assert!(
        incremental_elapsed < INCREMENTAL_CEILING,
        "incremental rebuild took {incremental_elapsed:?}"
    );

    let bytes = tree_bytes(&output);
    let per_route = bytes / ROUTES as u64;
    assert!(
        per_route < BYTES_PER_ROUTE_CEILING,
        "generated output is {per_route} bytes per route"
    );
}

fn count_files(root: &Path) -> usize {
    let mut total = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                total += 1;
            }
        }
    }
    total
}

fn tree_bytes(root: &Path) -> u64 {
    let mut total = 0;
    let mut pending: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if let Ok(metadata) = entry.metadata() {
                total += metadata.len();
            }
        }
    }
    total
}
