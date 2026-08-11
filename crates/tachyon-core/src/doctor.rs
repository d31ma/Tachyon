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
use crate::failure::{diagnostic, source_span};
use crate::handler::Interpreters;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

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
    let root = project_root.as_ref();
    if !root.is_dir() {
        return Err(Failure::one(diagnostic(
            1601,
            format!("Cannot inspect project '{}'.", root.display()),
            Some(String::from("Point ty doctor at a project directory.")),
            source_span("doctor", 0, 6),
        )));
    }

    let extensions = companion_extensions(root);
    let mut toolchains = Vec::new();
    if extensions.contains("ts") {
        toolchains.push(typescript(root));
    }
    for extension in &extensions {
        if let Some(entry) = wasm_toolchain(extension) {
            toolchains.push(entry);
        }
    }
    toolchains.extend(interpreters(root));
    Ok(DoctorReport {
        contract_version: 1,
        toolchains,
    })
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

/// Checks the toolchain that would compile one companion language to wasm.
///
/// A browser companion in a language other than JavaScript or TypeScript is
/// not compiled yet; this reports whether the machine could, so the route is
/// blocked by a decision rather than by an unknown.
fn wasm_toolchain(extension: &str) -> Option<Toolchain> {
    let (command, arguments, install) = match extension {
        "rs" => (
            "rustc",
            "--version",
            "Install Rust, then: rustup target add wasm32-unknown-unknown",
        ),
        "dart" => (
            "dart",
            "--version",
            "Install the Dart SDK, which provides: dart compile wasm",
        ),
        "kt" => (
            "kotlinc-js",
            "-version",
            "Download kotlin-stdlib-wasm-js from Maven Central and point \
             TAC_KOTLIN_WASM_STDLIB at the .klib; the command-line compiler ships \
             only the JS one.",
        ),
        "swift" => (
            "swiftc",
            "--version",
            "Install a swift.org toolchain, then: swift sdk install \
             <the wasm SDK for that exact version>; Xcode's own swiftc cannot \
             cross-compile to wasm.",
        ),
        "cs" => (
            "dotnet",
            "--version",
            "Install the .NET SDK, then: dotnet workload install wasm-tools",
        ),
        _ => return None,
    };
    // Swift is asked about the compiler the build will actually run, not the
    // one on PATH: on macOS those are different, and only one can target wasm.
    let command = if extension == "swift" {
        crate::wasm::swift_compiler().to_string_lossy().into_owned()
    } else {
        String::from(command)
    };
    let state = run(&command, &[arguments]).map_or(ToolchainState::Missing, |version| {
        wasm_gap(extension).map_or(ToolchainState::Ready(version.clone()), |gap| {
            ToolchainState::Incomplete(format!("{version}, {gap}"))
        })
    });
    Some(Toolchain {
        requirement: format!("tac.{extension} companions to wasm"),
        command,
        state,
        install: String::from(install),
    })
}

/// Reports why a compiler that is present still cannot emit wasm.
///
/// A version string proves the command exists, which is not the question. Every
/// one of these toolchains ships its wasm support separately — a rustup target,
/// a Swift SDK, a Kotlin wasm standard library, a .NET workload — so the
/// compiler answers `--version` happily on a machine that cannot build a
/// companion at all. Each is therefore asked to do the smallest piece of the
/// real work, and only the language whose own answer is yes is called ready.
fn wasm_gap(extension: &str) -> Option<String> {
    let (ready, gap) = match extension {
        "rs" => (
            rust_wasm_target(),
            "without the wasm32-unknown-unknown target",
        ),
        "swift" => (
            // Xcode's swiftc cannot cross-compile whatever SDK is present, so
            // both halves are checked the way the build uses them.
            crate::wasm::swift_sdk().is_some_and(|sdk| {
                compiles_empty("probe.swift", |_, source| {
                    let mut command = Command::new(crate::wasm::swift_compiler());
                    command
                        .args(["-target", "wasm32-unknown-wasip1", "-sdk"])
                        .arg(sdk.join("WASI.sdk"))
                        .arg("-resource-dir")
                        .arg(sdk.join("swift.xctoolchain/usr/lib/swift_static"))
                        .args(["-parse-as-library", "-typecheck"])
                        .arg(source);
                    command
                })
            }),
            "without a Swift SDK for WebAssembly and a swift.org toolchain",
        ),
        "kt" => (
            // The command-line compiler ships no wasm standard library, so the
            // project points at one and the probe uses the same one the build
            // will.
            std::env::var("TAC_KOTLIN_WASM_STDLIB").is_ok_and(|stdlib| {
                compiles_empty("probe.kt", |staged, source| {
                    let mut command = Command::new("kotlinc-js");
                    command
                        .args(["-Xwasm", "-Xwasm-target=wasm-js", "-libraries"])
                        .arg(&stdlib)
                        .args(["-Xir-produce-klib-file", "-ir-output-name", "probe"])
                        .arg("-ir-output-dir")
                        .arg(staged.join("out"))
                        .arg(source);
                    command
                })
            }),
            "without a Kotlin/Wasm standard library in TAC_KOTLIN_WASM_STDLIB",
        ),
        "cs" => (
            output("dotnet", &["workload", "list"])
                .is_some_and(|installed| installed.contains("wasm-tools")),
            "without the wasm-tools workload",
        ),
        "dart" => (
            output("dart", &["compile", "wasm", "--help"]).is_some(),
            "without the wasm compiler",
        ),
        _ => return None,
    };
    (!ready).then(|| String::from(gap))
}

/// Asks one compiler to build an empty source file for its wasm target.
///
/// Empty is deliberate: what is being tested is whether the target and its
/// standard library are installed, not whether the compiler works, so the
/// smallest input that still forces both to be loaded is the right one. A
/// probe that cannot be staged reports ready rather than inventing a fault the
/// machine does not have.
fn compiles_empty(name: &str, build: impl Fn(&Path, &Path) -> Command) -> bool {
    let Ok(staged) = tempfile::tempdir() else {
        return true;
    };
    let source = staged.path().join(name);
    if fs::write(&source, "").is_err() {
        return true;
    }
    build(staged.path(), &source)
        .output()
        .is_ok_and(|result| result.status.success())
}

/// Reports whether the `rustc` that would run can target wasm.
///
/// Asking `rustup` instead would describe whichever toolchain rustup manages,
/// which is not necessarily the `rustc` first on PATH: a machine with both a
/// package-manager Rust and a rustup Rust will answer for the wrong one and
/// call a toolchain ready that cannot build. So the question is put to the
/// compiler itself, by asking it to prepare a build for that target.
fn rust_wasm_target() -> bool {
    Command::new("rustc")
        .args([
            "--target",
            "wasm32-unknown-unknown",
            "--crate-type",
            "cdylib",
            "--emit=metadata",
            "-o",
            "-",
            "-",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .is_ok_and(|result| {
            // rustc names the missing crate as core or std depending on the
            // crate type, but always adds this note for a target it lacks.
            !String::from_utf8_lossy(&result.stderr).contains("target may not be installed")
        })
}

/// Checks every interpreter a project registered in `.tachyonrc`.
fn interpreters(root: &Path) -> Vec<Toolchain> {
    let Ok(registered) = Interpreters::discover(root) else {
        return Vec::new();
    };
    registered
        .commands()
        .map(|(extension, command)| {
            let program = command.first().cloned().unwrap_or_default();
            let state = run(&program, &["--version"])
                .map_or(ToolchainState::Missing, ToolchainState::Ready);
            Toolchain {
                requirement: format!("yon.{extension} handlers"),
                command: program.clone(),
                state,
                install: format!("Install {program}, or change the interpreter in .tachyonrc."),
            }
        })
        .collect()
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
    let result = Command::new(program).args(arguments).output().ok()?;
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

    use super::{ToolchainState, check};
    use std::fs;

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
    fn a_polyglot_companion_reports_its_wasm_toolchain() {
        let root = tempfile::tempdir().expect("project");
        let component = root.path().join("client/components/demo-card");
        fs::create_dir_all(&component).expect("component");
        fs::write(component.join("tac.html"), "<div>x</div>").expect("view");
        fs::write(component.join("tac.rs"), "fn main() {}").expect("companion");

        let report = check(root.path()).expect("report");
        let entry = report
            .toolchains
            .iter()
            .find(|entry| entry.command == "rustc")
            .expect("rust toolchain reported");
        assert!(entry.requirement.contains("wasm"), "{entry:?}");
        assert!(
            entry.install.contains("wasm32-unknown-unknown"),
            "{entry:?}"
        );
        // Whether it is ready depends on the machine; the report must say
        // which of the three states it is in either way.
        match &entry.state {
            ToolchainState::Ready(version) => assert!(!version.is_empty()),
            ToolchainState::Incomplete(reason) => assert!(reason.contains("wasm")),
            ToolchainState::Missing => {}
        }
    }

    #[test]
    fn a_missing_project_fails_closed() {
        let error = check("/definitely/not/a/project").expect_err("missing");
        assert!(error.to_string().contains("TY1601"), "{error}");
    }
}
