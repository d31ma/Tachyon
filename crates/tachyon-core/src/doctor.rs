//! Reports whether the toolchains a project needs are actually installed.
//!
//! A build fails with a diagnostic naming one missing tool at a time, which is
//! the wrong shape for the question "can this machine build this project at
//! all". This answers that question up front, for the whole project, before
//! anything is compiled.
//!
//! Only what a project actually uses is reported. A project with no `tac.ts`
//! is not told about the TypeScript compiler, because it does not need one.

use crate::Failure;
use crate::external_command::run_sync;
use crate::failure::{diagnostic, source_span};
use crate::handler::{
    HandlerSupervisorOptions, RuntimeProbeResult, RuntimeProbeState, RuntimeRequirements,
    probe_all_sync,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

const PROBE_DEADLINE: Duration = Duration::from_secs(10);
const PROBE_OUTPUT_BYTES: usize = 64 * 1_024;

/// Directories that never hold project sources.
const IGNORED: [&str; 7] = [
    ".git",
    ".tachyon",
    "dist",
    "dist-bin",
    "node_modules",
    "target",
    "__pycache__",
];

/// How a required toolchain stands on this machine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "detail")]
pub enum ToolchainState {
    /// Present and usable, with what the tool reported about itself.
    Ready(String),
    /// The command runs but cannot yet produce what is needed.
    Incomplete(String),
    /// The command is not on `PATH`.
    Missing,
}

/// One toolchain a project needs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Toolchain {
    /// What in the project requires it.
    pub requirement: String,
    /// The command that provides it.
    pub command: String,
    /// Whether it is usable here.
    pub state: ToolchainState,
    /// How to obtain or complete it.
    pub install: String,
}

/// Everything a project needs, and whether this machine has it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DoctorReport {
    /// Diagnostics contract major version.
    pub contract_version: u8,
    /// Toolchains, in the order they were discovered.
    pub toolchains: Vec<Toolchain>,
}

impl DoctorReport {
    /// Reports whether every required toolchain is ready.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.toolchains
            .iter()
            .all(|entry| matches!(entry.state, ToolchainState::Ready(_)))
    }

    /// Renders the report for a terminal.
    #[must_use]
    pub fn to_text(&self) -> String {
        if self.toolchains.is_empty() {
            return String::from("This project needs no external toolchain.\n");
        }
        let mut text = String::new();
        for entry in &self.toolchains {
            let (mark, detail) = match &entry.state {
                ToolchainState::Ready(version) => ("ready", version.clone()),
                ToolchainState::Incomplete(reason) => ("partial", reason.clone()),
                ToolchainState::Missing => ("missing", String::from("not on PATH")),
            };
            let _ = writeln!(
                text,
                "{mark:<8}  {:<30}{:<24} {detail}",
                entry.requirement, entry.command
            );
            if !matches!(entry.state, ToolchainState::Ready(_)) {
                let _ = writeln!(text, "        {}", entry.install);
            }
        }
        let missing = self
            .toolchains
            .iter()
            .filter(|entry| !matches!(entry.state, ToolchainState::Ready(_)))
            .count();
        let _ = writeln!(
            text,
            "\n{} of {} ready",
            self.toolchains.len() - missing,
            self.toolchains.len()
        );
        text
    }
}

/// Inspects a project and reports the toolchains it needs.
///
/// # Errors
///
/// Returns a diagnostic when the project root cannot be read.
pub fn check(project_root: impl AsRef<Path>) -> Result<DoctorReport, Failure> {
    check_with_options(
        project_root.as_ref(),
        &HandlerSupervisorOptions::from_environment()?,
    )
}

fn check_with_options(
    root: &Path,
    options: &HandlerSupervisorOptions,
) -> Result<DoctorReport, Failure> {
    if !root.is_dir() {
        return Err(Failure::one(diagnostic(
            1601,
            format!("Cannot inspect project '{}'.", root.display()),
            Some(String::from("Point ty doctor at a project directory.")),
            source_span("doctor", 0, 6),
        )));
    }

    let extensions = companion_extensions(root);
    let workers = crate::Workers::discover(root)?;
    let yon = yon_extensions(root, &workers);
    let mut toolchains = Vec::new();
    if extensions.contains("ts") {
        toolchains.push(typescript(root));
    }
    for extension in &extensions {
        if let Some(entry) = companion_toolchain(extension) {
            toolchains.push(entry);
        }
    }
    let requirements = RuntimeRequirements::from_extensions(yon.iter().map(String::as_str));
    let resolved = requirements.resolve(&options.runtimes, &options.isolation)?;
    toolchains.extend(
        probe_all_sync(resolved, &options.environment)?
            .iter()
            .map(runtime_toolchain),
    );
    Ok(DoctorReport {
        contract_version: 1,
        toolchains,
    })
}

fn yon_extensions(root: &Path, workers: &crate::Workers) -> BTreeSet<String> {
    let mut extensions = BTreeSet::new();
    let mut pending: Vec<_> = [
        "server/routes",
        "server/services",
        "server/repositories",
        "server/clients",
        "server/delegates",
    ]
    .into_iter()
    .map(|relative| root.join(relative))
    .filter(|path| {
        fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
    })
    .collect();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !IGNORED.contains(&name.as_str()) && !name.starts_with('.') {
                    pending.push(entry.path());
                }
            } else if file_type.is_file()
                && let Some(extension) = name.strip_prefix("yon.")
            {
                extensions.insert(String::from(extension));
            }
        }
    }
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().is_ok_and(|kind| kind.is_file())
                && let Some(extension) = name.strip_prefix("middleware.")
            {
                extensions.insert(String::from(extension));
            }
        }
    }
    for (worker, _) in workers.iter() {
        if let Some(extension) = Path::new(worker)
            .extension()
            .and_then(|value| value.to_str())
        {
            extensions.insert(String::from(extension));
        }
    }
    extensions
}

fn runtime_toolchain(result: &RuntimeProbeResult) -> Toolchain {
    let requirement = result.requirement();
    let state = match result.state() {
        RuntimeProbeState::Ready => {
            ToolchainState::Ready(String::from("deployment readiness probe succeeded"))
        }
        RuntimeProbeState::Missing => ToolchainState::Missing,
        RuntimeProbeState::Incomplete => ToolchainState::Incomplete(String::from(
            "deployment readiness probe did not complete successfully",
        )),
    };
    Toolchain {
        requirement: requirement.label().to_owned(),
        command: requirement.command_label().to_owned(),
        state,
        install: requirement.help().to_owned(),
    }
}

/// Collects the extensions of every browser companion in a project.
fn companion_extensions(root: &Path) -> BTreeSet<String> {
    let mut extensions = BTreeSet::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if !IGNORED.contains(&name.as_str()) && !name.starts_with('.') {
                    pending.push(entry.path());
                }
            } else if let Some(extension) = name.strip_prefix("tac.") {
                extensions.insert(String::from(extension));
            }
        }
    }
    extensions
}

/// Checks the TypeScript compiler that emits a `tac.ts` companion.
///
/// A project's own dependency is preferred over one on `PATH`, because that is
/// the one the compiler will invoke.
fn typescript(root: &Path) -> Toolchain {
    let local = root.join("node_modules/.bin/tsc");
    let program = if local.is_file() {
        local.to_string_lossy().into_owned()
    } else {
        String::from("tsc")
    };
    let state = match run(&program, &["--version"]) {
        None => ToolchainState::Missing,
        Some(version) => {
            let major = version
                .split_whitespace()
                .next_back()
                .and_then(|value| value.split('.').next())
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            if major >= 6 {
                ToolchainState::Ready(version)
            } else {
                ToolchainState::Incomplete(format!("{version}, but 6 or newer is required"))
            }
        }
    };
    Toolchain {
        requirement: String::from("tac.ts companions"),
        command: if program == "tsc" {
            String::from("tsc")
        } else {
            String::from("node_modules/.bin/tsc")
        },
        state,
        install: String::from("Install typescript 6 or newer as a project dependency or on PATH."),
    }
}

/// Checks the toolchain that compiles one native companion language.
///
/// A companion is compiled by its own platform's compiler now, so the question
/// is simply whether that compiler is here. Nothing is asked about
/// WebAssembly: a companion either belongs to a target and is built by that
/// target's toolchain, or it does not belong to that target at all.
fn companion_toolchain(extension: &str) -> Option<Toolchain> {
    // Which targets a companion reaches is the matrix's answer, not one
    // written out again here: the two had already drifted apart once.
    let targets =
        crate::project::companion_kind(&format!("tac.{extension}")).and_then(
            |kind| match kind {
                crate::project::CompanionKind::Native(language) => Some(language.target_names()),
                _ => None,
            },
        )?;
    let (name, arguments, remedy) = match extension {
        "rs" => (
            "rustc",
            vec!["--version"],
            "Install Rust from https://rustup.rs",
        ),
        "swift" => (
            "swiftc",
            vec!["--version"],
            "Install Xcode, which provides swiftc",
        ),
        "kt" => (
            "kotlinc",
            vec!["-version"],
            "Install Kotlin, or let the Android Gradle plugin provide it",
        ),
        "cs" => (
            "dotnet",
            vec!["--version"],
            "Install the .NET SDK from https://dotnet.microsoft.com",
        ),
        _ => return None,
    };
    Some(Toolchain {
        requirement: format!("tac.{extension} companions for {targets}"),
        command: String::from(name),
        state: output(name, &arguments).map_or(ToolchainState::Missing, ToolchainState::Ready),
        install: String::from(remedy),
    })
}

/// Runs one command and returns its first line of output.
fn run(program: &str, arguments: &[&str]) -> Option<String> {
    output(program, arguments)?
        .lines()
        .next()
        .map(|line| line.trim().to_owned())
}

/// Runs one command and returns everything it wrote.
///
/// A tool that reports its version on standard error is common enough that
/// falling back to it is the difference between "missing" and "found".
fn output(program: &str, arguments: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(arguments);
    let result = run_sync(&mut command, PROBE_DEADLINE, PROBE_OUTPUT_BYTES).ok()?;
    if !result.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&result.stdout);
    if text.trim().is_empty() {
        Some(String::from_utf8_lossy(&result.stderr).into_owned())
    } else {
        Some(text.into_owned())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{ToolchainState, check, check_with_options};
    use crate::handler::{HandlerRuntimePrograms, HandlerSupervisorOptions};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn a_project_is_told_only_about_what_it_uses() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(pages.join("tac.html"), "<main>x</main>").expect("view");

        // A project with only a view needs nothing external, and is not told
        // about compilers it will never invoke.
        let report = check(root.path()).expect("report");
        assert!(report.toolchains.is_empty(), "{:?}", report.toolchains);
        assert!(report.is_ready());
        assert!(report.to_text().contains("no external toolchain"));
    }

    #[test]
    fn a_polyglot_companion_reports_its_native_toolchain() {
        let root = tempfile::tempdir().expect("project");
        let component = root.path().join("client/pages");
        fs::create_dir_all(&component).expect("component");
        fs::write(component.join("tac.html"), "<div>x</div>").expect("view");
        fs::write(component.join("tac.rs"), "fn main() {}").expect("companion");

        let report = check(root.path()).expect("report");
        let entry = report
            .toolchains
            .iter()
            .find(|entry| entry.command == "rustc")
            .expect("rust toolchain reported");
        assert!(
            entry.requirement.contains("macos, windows, linux"),
            "{entry:?}"
        );
        assert!(entry.install.contains("rustup.rs"), "{entry:?}");
        // Whether it is ready depends on the machine; the report must say
        // which of the three states it is in either way.
        match &entry.state {
            ToolchainState::Ready(version) => assert!(!version.is_empty()),
            ToolchainState::Incomplete(reason) => assert!(!reason.is_empty()),
            ToolchainState::Missing => {}
        }
    }

    #[test]
    fn yon_sources_report_only_the_runtime_toolchains_the_project_uses() {
        let root = tempfile::tempdir().expect("project");
        for relative in ["server/routes/a/yon.py", "server/routes/b/yon.java"] {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, "source").expect("source");
        }
        let report = check(root.path()).expect("report");
        let commands: Vec<_> = report
            .toolchains
            .iter()
            .map(|toolchain| toolchain.command.as_str())
            .collect();
        let python = if cfg!(windows) { "python" } else { "python3" };
        assert!(commands.contains(&python), "{commands:?}");
        assert!(commands.contains(&"java"), "{commands:?}");
        assert!(!commands.contains(&"bun"), "{commands:?}");
    }

    #[test]
    fn yon_runtime_checks_use_resolved_overrides_without_disclosing_them() {
        let root = tempfile::tempdir().expect("project");
        let route = root.path().join("server/routes/example/yon.js");
        fs::create_dir_all(route.parent().expect("parent")).expect("directory");
        fs::write(route, "@Controller class ExampleController {}").expect("source");
        let canary = "/private/credentials/doctor-runtime-canary";
        let options = HandlerSupervisorOptions {
            runtimes: HandlerRuntimePrograms {
                javascript: PathBuf::from(canary),
                ..HandlerRuntimePrograms::from_lookup(|_| None)
            },
            ..HandlerSupervisorOptions::default()
        };
        let report = check_with_options(root.path(), &options).expect("report");
        let javascript = report.toolchains.first().expect("javascript");
        assert_eq!(javascript.command, "YON_JAVASCRIPT_RUNTIME");
        assert_eq!(javascript.state, ToolchainState::Missing);
        let human = report.to_text();
        let json = serde_json::to_string(&report).expect("json");
        assert!(!human.contains(canary), "{human}");
        assert!(!json.contains(canary), "{json}");
        assert!(human.contains("YON_JAVASCRIPT_RUNTIME"), "{human}");
    }

    #[test]
    fn yon_discovery_is_limited_to_maintained_source_roots_and_configured_workers() {
        let root = tempfile::tempdir().expect("project");
        for (relative, source) in [
            ("server/routes/a/yon.js", "source"),
            ("server/services/a/yon.py", "source"),
            ("server/services/typescript/yon.ts", "source"),
            ("server/repositories/a/yon.java", "source"),
            ("server/clients/a/yon.cs", "source"),
            ("server/delegates/a/yon.kt", "source"),
            ("middleware.php", "source"),
            ("server/workers/cleanup.rs", "source"),
            ("fixtures/decoy/yon.ts", "source"),
            ("docs/examples/yon.py", "source"),
            ("generated/preludes/yon.php", "source"),
        ] {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, source).expect("source");
        }
        fs::write(
            root.path().join(".tachyonrc"),
            r#"{"workers":{"server/workers/cleanup.rs":{"every_seconds":60}}}"#,
        )
        .expect("workers");
        let report = check(root.path()).expect("report");
        let commands: Vec<_> = report
            .toolchains
            .iter()
            .map(|toolchain| toolchain.command.as_str())
            .collect();
        for expected in ["bun", "java", "dotnet", "kotlinc", "php", "rustc"] {
            assert!(commands.contains(&expected), "{expected}: {commands:?}");
        }
        assert!(
            commands.contains(&if cfg!(windows) { "python" } else { "python3" }),
            "{commands:?}"
        );
        assert_eq!(
            commands.iter().filter(|command| **command == "php").count(),
            1
        );
        assert_eq!(
            commands.iter().filter(|command| **command == "bun").count(),
            1
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| **command == "java")
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn yon_discovery_never_follows_outside_or_cyclic_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("project");
        let outside = tempfile::tempdir().expect("outside");
        fs::create_dir_all(outside.path().join("nested")).expect("outside directory");
        fs::write(outside.path().join("nested/yon.py"), "source").expect("outside source");
        fs::create_dir_all(root.path().join("server/routes/live")).expect("route");
        fs::write(root.path().join("server/routes/live/yon.js"), "source").expect("source");
        symlink(outside.path(), root.path().join("server/routes/outside")).expect("outside link");
        symlink(
            root.path().join("server/routes"),
            root.path().join("server/routes/live/parent"),
        )
        .expect("cycle");
        let report = check(root.path()).expect("bounded discovery");
        let commands: Vec<_> = report
            .toolchains
            .iter()
            .map(|toolchain| toolchain.command.as_str())
            .collect();
        assert!(commands.contains(&"bun"), "{commands:?}");
        let python = if cfg!(windows) { "python" } else { "python3" };
        assert!(!commands.contains(&python), "{commands:?}");
    }

    #[test]
    fn a_missing_project_fails_closed() {
        let error = check("/definitely/not/a/project").expect_err("missing");
        assert!(error.to_string().contains("TY1601"), "{error}");
    }
}
