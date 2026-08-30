//! Deployment readiness for the fixed Yon language boundary.

use super::{EnvironmentPolicy, HandlerRuntimePrograms, HandlerSource, YonIsolationPolicy};
use crate::Failure;
use crate::external_command::{ToolError, run, run_sync};
use crate::failure::diagnostic;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};
use tokio::process::Command;

const PREFLIGHT_DEADLINE: Duration = Duration::from_secs(10);
const PROBE_OUTPUT_BYTES: usize = 64 * 1024;

/// The authored language behind one validated Yon invocation source.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum YonLanguage {
    JavaScript,
    TypeScript,
    Python,
    Java,
    Php,
    Kotlin,
    CSharp,
    Rust,
}

impl YonLanguage {
    pub(crate) fn from_extension(extension: &str) -> Option<Self> {
        Some(match extension.to_ascii_lowercase().as_str() {
            "js" => Self::JavaScript,
            "ts" => Self::TypeScript,
            "py" => Self::Python,
            "java" => Self::Java,
            "php" => Self::Php,
            "kt" => Self::Kotlin,
            "cs" => Self::CSharp,
            "rs" => Self::Rust,
            _ => return None,
        })
    }

    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(OsStr::to_str)
            .and_then(Self::from_extension)
    }

    pub(crate) const fn family(self) -> &'static str {
        match self {
            Self::JavaScript | Self::TypeScript => "javascript",
            Self::Python => "python",
            Self::Java => "java",
            Self::Php => "php",
            Self::Kotlin => "kotlin",
            Self::CSharp => "csharp",
            Self::Rust => "rust",
        }
    }
}

/// One external capability needed to start every discovered Yon source.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum RuntimeRequirementKey {
    JavaScript,
    Python,
    Java,
    Php,
    KotlinCompiler,
    Dotnet,
    RustCompiler,
    FirecrackerDriver,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProbeKind {
    JavaScript { typescript: bool },
    Python,
    Java { source_launcher: bool },
    Php,
    Version(&'static [&'static str]),
    DotnetBuild,
    File,
}

/// A resolved requirement. The executable is deliberately private so a
/// configured path can never accidentally enter a report or event.
#[derive(Clone)]
pub(crate) struct RuntimeRequirement {
    key: RuntimeRequirementKey,
    program: PathBuf,
    command_label: String,
    label: String,
    help: String,
    probe: ProbeKind,
}

impl RuntimeRequirement {
    pub(crate) const fn key(&self) -> RuntimeRequirementKey {
        self.key
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn command_label(&self) -> &str {
        &self.command_label
    }

    pub(crate) fn help(&self) -> &str {
        &self.help
    }
}

/// Deduplicated authored languages which require deployment readiness.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeRequirements {
    languages: BTreeSet<YonLanguage>,
}

impl RuntimeRequirements {
    pub(crate) fn from_sources<'a>(sources: impl IntoIterator<Item = &'a HandlerSource>) -> Self {
        Self {
            languages: sources
                .into_iter()
                .filter_map(|source| YonLanguage::from_path(Path::new(source.relative_path())))
                .collect(),
        }
    }

    pub(crate) fn from_extensions<'a>(extensions: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            languages: extensions
                .into_iter()
                .filter_map(YonLanguage::from_extension)
                .collect(),
        }
    }

    pub(crate) fn resolve(
        &self,
        runtimes: &HandlerRuntimePrograms,
        isolation: &YonIsolationPolicy,
    ) -> Result<Vec<RuntimeRequirement>, Failure> {
        if let Some(policy) = isolation.firecracker() {
            for language in &self.languages {
                super::isolation::validate_backend_yon_language(isolation, *language)?;
            }
            return if self.languages.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![RuntimeRequirement {
                    key: RuntimeRequirementKey::FirecrackerDriver,
                    program: policy.driver().to_path_buf(),
                    command_label: String::from("YON_FIRECRACKER_DRIVER"),
                    label: String::from("configured Firecracker driver (YON_FIRECRACKER_DRIVER)"),
                    help: String::from(
                        "Install or correct YON_FIRECRACKER_DRIVER before starting Tachyon.",
                    ),
                    probe: ProbeKind::File,
                }])
            };
        }

        let mut requirements = Vec::new();
        let has_javascript = self.languages.contains(&YonLanguage::JavaScript);
        let has_typescript = self.languages.contains(&YonLanguage::TypeScript);
        if has_javascript || has_typescript {
            requirements.push(configured_requirement(
                RuntimeRequirementKey::JavaScript,
                &runtimes.javascript,
                "bun",
                "Yon JavaScript runtime",
                "YON_JAVASCRIPT_RUNTIME",
                "Install Bun or correct YON_JAVASCRIPT_RUNTIME.",
                ProbeKind::JavaScript {
                    typescript: has_typescript,
                },
            ));
        }
        if self.languages.contains(&YonLanguage::Python) {
            let default = if cfg!(windows) { "python" } else { "python3" };
            requirements.push(configured_requirement(
                RuntimeRequirementKey::Python,
                &runtimes.python,
                default,
                "Yon Python runtime",
                "YON_PYTHON_RUNTIME",
                "Install Python or correct YON_PYTHON_RUNTIME.",
                ProbeKind::Python,
            ));
        }
        let has_java = self.languages.contains(&YonLanguage::Java);
        let has_kotlin = self.languages.contains(&YonLanguage::Kotlin);
        if has_java || has_kotlin {
            requirements.push(fixed_requirement(
                RuntimeRequirementKey::Java,
                "java",
                "Yon Java runtime 'java'",
                "Install a JDK that provides the Java source launcher and runtime.",
                ProbeKind::Java {
                    source_launcher: has_java,
                },
            ));
        }
        if self.languages.contains(&YonLanguage::Php) {
            requirements.push(fixed_requirement(
                RuntimeRequirementKey::Php,
                "php",
                "Yon PHP runtime 'php'",
                "Install PHP before starting Tachyon.",
                ProbeKind::Php,
            ));
        }
        if has_kotlin {
            requirements.push(fixed_requirement(
                RuntimeRequirementKey::KotlinCompiler,
                "kotlinc",
                "Yon Kotlin compiler 'kotlinc'",
                "Install the Kotlin compiler before starting Tachyon.",
                ProbeKind::Version(&["-version"]),
            ));
        }
        if self.languages.contains(&YonLanguage::CSharp) {
            requirements.push(fixed_requirement(
                RuntimeRequirementKey::Dotnet,
                "dotnet",
                "Yon C# runtime 'dotnet'",
                "Install the .NET SDK and Microsoft.NETCore.App runtime.",
                ProbeKind::DotnetBuild,
            ));
        }
        if self.languages.contains(&YonLanguage::Rust) {
            requirements.push(fixed_requirement(
                RuntimeRequirementKey::RustCompiler,
                "rustc",
                "Yon Rust compiler 'rustc'",
                "Install Rust before starting Tachyon.",
                ProbeKind::Version(&["--version"]),
            ));
        }
        requirements.sort_by_key(RuntimeRequirement::key);
        Ok(requirements)
    }
}

fn configured_requirement(
    key: RuntimeRequirementKey,
    program: &Path,
    default: &str,
    family: &str,
    environment: &str,
    help: &str,
    probe: ProbeKind,
) -> RuntimeRequirement {
    let label = if program == Path::new(default) {
        format!("{family} '{default}'")
    } else {
        format!("configured {family} ({environment})")
    };
    RuntimeRequirement {
        key,
        program: program.to_path_buf(),
        command_label: String::from(if program == Path::new(default) {
            default
        } else {
            environment
        }),
        label,
        help: String::from(help),
        probe,
    }
}

fn fixed_requirement(
    key: RuntimeRequirementKey,
    program: &str,
    label: &str,
    help: &str,
    probe: ProbeKind,
) -> RuntimeRequirement {
    RuntimeRequirement {
        key,
        program: PathBuf::from(program),
        command_label: String::from(program),
        label: String::from(label),
        help: String::from(help),
        probe,
    }
}

/// Safe result of one readiness probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeProbeState {
    Ready,
    Missing,
    Incomplete,
}

#[derive(Clone)]
pub(crate) struct RuntimeProbeResult {
    requirement: RuntimeRequirement,
    state: RuntimeProbeState,
}

impl RuntimeProbeResult {
    pub(crate) fn requirement(&self) -> &RuntimeRequirement {
        &self.requirement
    }

    pub(crate) const fn state(&self) -> RuntimeProbeState {
        self.state
    }
}

struct PreparedProbe {
    command: StdCommand,
    workspace: Option<tempfile::TempDir>,
}

impl RuntimeRequirement {
    fn prepare(&self) -> Result<PreparedProbe, Failure> {
        let mut workspace = None;
        let mut command = StdCommand::new(&self.program);
        match self.probe {
            ProbeKind::JavaScript { typescript } => {
                let directory = probe_workspace()?;
                let name = if typescript { "probe.ts" } else { "probe.js" };
                let source = directory.path().join(name);
                let annotation = if typescript {
                    "function Controller(value: unknown, _context: ClassDecoratorContext) { return value; }\n@Controller\nclass ReadinessController {}\n"
                } else {
                    "function Controller(value, _context) { return value; }\n@Controller\nclass ReadinessController {}\n"
                };
                write_probe(&source, annotation)?;
                if !typescript {
                    command.arg("--no-warnings");
                }
                command.arg(&source);
                workspace = Some(directory);
            }
            ProbeKind::Python => {
                command.args(["-I", "-B", "-c", "class ReadinessController: pass"]);
            }
            ProbeKind::Java { source_launcher } => {
                if source_launcher {
                    let directory = probe_workspace()?;
                    let source = directory.path().join("Readiness.java");
                    write_probe(
                        &source,
                        "final class Readiness { public static void main(String[] args) {} }\n",
                    )?;
                    command.arg(&source);
                    workspace = Some(directory);
                } else {
                    command.arg("--version");
                }
            }
            ProbeKind::Php => {
                command.args(["-r", "exit(0);"]);
            }
            ProbeKind::Version(arguments) => {
                command.args(arguments);
            }
            ProbeKind::DotnetBuild => {
                command.arg("--version");
            }
            ProbeKind::File => {}
        }
        Ok(PreparedProbe { command, workspace })
    }

    fn prepare_dotnet_build(&self, sdk_major: u32) -> Result<PreparedProbe, Failure> {
        let workspace = probe_workspace()?;
        write_probe(
            &workspace.path().join("Readiness.cs"),
            "internal static class Readiness { private static void Main() {} }\n",
        )?;
        write_probe(
            &workspace.path().join("Readiness.csproj"),
            &format!(
                "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    \
                 <OutputType>Exe</OutputType>\n    \
                 <TargetFramework>net{sdk_major}.0</TargetFramework>\n    \
                 <EnableDefaultCompileItems>false</EnableDefaultCompileItems>\n    \
                 <RestoreIgnoreFailedSources>true</RestoreIgnoreFailedSources>\n    \
                 <NuGetAudit>false</NuGetAudit>\n  \
                 </PropertyGroup>\n  <ItemGroup><Compile Include=\"Readiness.cs\" /></ItemGroup>\n\
                 </Project>\n"
            ),
        )?;
        write_probe(
            &workspace.path().join("NuGet.Config"),
            "<configuration><packageSources><clear /></packageSources></configuration>\n",
        )?;
        let mut command = StdCommand::new(&self.program);
        command
            .args([
                "build",
                "Readiness.csproj",
                "--nologo",
                "--verbosity",
                "quiet",
                "--disable-build-servers",
            ])
            .current_dir(workspace.path());
        Ok(PreparedProbe {
            command,
            workspace: Some(workspace),
        })
    }
}

fn dotnet_sdk_major(output: &crate::external_command::ToolOutput) -> Option<u32> {
    output.status.success().then_some(())?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .split('.')
        .next()?
        .parse::<u32>()
        .ok()
        .filter(|major| *major > 0)
}

fn apply_dotnet_build_environment(environment: &EnvironmentPolicy, prepared: &mut PreparedProbe) {
    environment.apply_std(&mut prepared.command);
    if let Some(workspace) = &prepared.workspace {
        prepared
            .command
            .env("DOTNET_CLI_HOME", workspace.path())
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .env("DOTNET_NOLOGO", "1")
            .env("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1")
            .env("NUGET_PACKAGES", workspace.path().join("packages"));
    }
}

fn probe_workspace() -> Result<tempfile::TempDir, Failure> {
    tempfile::Builder::new()
        .prefix("tachyon-readiness-")
        .tempdir()
        .map_err(|_| readiness_internal("Cannot create the runtime readiness workspace."))
}

fn write_probe(path: &Path, source: &str) -> Result<(), Failure> {
    fs::write(path, source)
        .map_err(|_| readiness_internal("Cannot write the runtime readiness probe."))
}

fn readiness_internal(message: &str) -> Failure {
    Failure::one(diagnostic(
        2101,
        message,
        Some(String::from("Check temporary storage and retry startup.")),
        None,
    ))
}

fn probe_state(
    _requirement: &RuntimeRequirement,
    result: Result<crate::external_command::ToolOutput, ToolError>,
) -> RuntimeProbeState {
    match result {
        Ok(output) if output.status.success() => RuntimeProbeState::Ready,
        Err(ToolError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            RuntimeProbeState::Missing
        }
        Ok(_) | Err(_) => RuntimeProbeState::Incomplete,
    }
}

async fn probe_dotnet_build(
    requirement: &RuntimeRequirement,
    initial: Result<crate::external_command::ToolOutput, ToolError>,
    environment: &EnvironmentPolicy,
    deadline: tokio::time::Instant,
) -> Result<RuntimeProbeState, Failure> {
    let sdk_major = initial.as_ref().ok().and_then(dotnet_sdk_major);
    let initial_state = probe_state(requirement, initial);
    if initial_state != RuntimeProbeState::Ready {
        return Ok(initial_state);
    }
    let Some(sdk_major) = sdk_major else {
        return Ok(RuntimeProbeState::Incomplete);
    };
    let mut prepared = requirement.prepare_dotnet_build(sdk_major)?;
    apply_dotnet_build_environment(environment, &mut prepared);
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        return Ok(RuntimeProbeState::Incomplete);
    }
    let mut command = Command::from(prepared.command);
    Ok(probe_state(
        requirement,
        run(&mut command, remaining, PROBE_OUTPUT_BYTES).await,
    ))
}

fn probe_dotnet_build_sync(
    requirement: &RuntimeRequirement,
    initial: Result<crate::external_command::ToolOutput, ToolError>,
    environment: &EnvironmentPolicy,
    deadline: Instant,
) -> Result<RuntimeProbeState, Failure> {
    let sdk_major = initial.as_ref().ok().and_then(dotnet_sdk_major);
    let initial_state = probe_state(requirement, initial);
    if initial_state != RuntimeProbeState::Ready {
        return Ok(initial_state);
    }
    let Some(sdk_major) = sdk_major else {
        return Ok(RuntimeProbeState::Incomplete);
    };
    let mut prepared = requirement.prepare_dotnet_build(sdk_major)?;
    apply_dotnet_build_environment(environment, &mut prepared);
    let remaining = deadline.saturating_duration_since(Instant::now());
    let result = if remaining.is_zero() {
        Err(ToolError::TimedOut)
    } else {
        run_sync(&mut prepared.command, remaining, PROBE_OUTPUT_BYTES)
    };
    Ok(probe_state(requirement, result))
}

pub(crate) async fn probe_all(
    requirements: Vec<RuntimeRequirement>,
    environment: &EnvironmentPolicy,
) -> Result<Vec<RuntimeProbeResult>, Failure> {
    let deadline = tokio::time::Instant::now() + PREFLIGHT_DEADLINE;
    let mut results = Vec::with_capacity(requirements.len());
    for requirement in requirements {
        if matches!(requirement.probe, ProbeKind::File) {
            let state = match fs::symlink_metadata(&requirement.program) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    RuntimeProbeState::Ready
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    RuntimeProbeState::Missing
                }
                Ok(_) | Err(_) => RuntimeProbeState::Incomplete,
            };
            results.push(RuntimeProbeResult { requirement, state });
            continue;
        }
        let mut prepared = requirement.prepare()?;
        environment.apply_std(&mut prepared.command);
        let mut command = Command::from(prepared.command);
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let result = if remaining.is_zero() {
            Err(ToolError::TimedOut)
        } else {
            run(&mut command, remaining, PROBE_OUTPUT_BYTES).await
        };
        let state = if matches!(requirement.probe, ProbeKind::DotnetBuild) {
            probe_dotnet_build(&requirement, result, environment, deadline).await?
        } else {
            probe_state(&requirement, result)
        };
        results.push(RuntimeProbeResult { requirement, state });
    }
    Ok(results)
}

pub(crate) fn probe_all_sync(
    requirements: Vec<RuntimeRequirement>,
    environment: &EnvironmentPolicy,
) -> Result<Vec<RuntimeProbeResult>, Failure> {
    let deadline = Instant::now() + PREFLIGHT_DEADLINE;
    let mut results = Vec::with_capacity(requirements.len());
    for requirement in requirements {
        if matches!(requirement.probe, ProbeKind::File) {
            let state = match fs::symlink_metadata(&requirement.program) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    RuntimeProbeState::Ready
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    RuntimeProbeState::Missing
                }
                Ok(_) | Err(_) => RuntimeProbeState::Incomplete,
            };
            results.push(RuntimeProbeResult { requirement, state });
            continue;
        }
        let mut prepared = requirement.prepare()?;
        environment.apply_std(&mut prepared.command);
        let remaining = deadline.saturating_duration_since(Instant::now());
        let result = if remaining.is_zero() {
            Err(ToolError::TimedOut)
        } else {
            run_sync(&mut prepared.command, remaining, PROBE_OUTPUT_BYTES)
        };
        let state = if matches!(requirement.probe, ProbeKind::DotnetBuild) {
            probe_dotnet_build_sync(&requirement, result, environment, deadline)?
        } else {
            probe_state(&requirement, result)
        };
        results.push(RuntimeProbeResult { requirement, state });
    }
    Ok(results)
}

pub(crate) fn readiness_failure(results: &[RuntimeProbeResult]) -> Option<Failure> {
    let diagnostics = results
        .iter()
        .filter_map(|result| match result.state {
            RuntimeProbeState::Ready => None,
            RuntimeProbeState::Missing => Some(diagnostic(
                2112,
                format!("Required {} was not found.", result.requirement.label),
                Some(result.requirement.help.clone()),
                None,
            )),
            RuntimeProbeState::Incomplete => Some(diagnostic(
                2101,
                format!("Required {} is not usable.", result.requirement.label),
                Some(result.requirement.help.clone()),
                None,
            )),
        })
        .collect::<Vec<_>>();
    (!diagnostics.is_empty()).then(|| Failure::new(diagnostics))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    #[cfg(unix)]
    use super::{ProbeKind, RuntimeProbeState, probe_all, probe_all_sync, readiness_failure};
    use super::{RuntimeRequirement, RuntimeRequirementKey, RuntimeRequirements};
    #[cfg(unix)]
    use crate::handler::EnvironmentPolicy;
    use crate::handler::{HandlerRuntimePrograms, YonIsolationPolicy};
    use std::path::PathBuf;

    #[test]
    fn language_requirements_are_complete_deduplicated_and_safe() {
        let requirements = RuntimeRequirements::from_extensions([
            "js", "ts", "py", "java", "php", "kt", "cs", "rs",
        ]);
        let runtimes = HandlerRuntimePrograms {
            javascript: PathBuf::from("/secret/canary-bun"),
            python: PathBuf::from("/secret/canary-python"),
        };
        let resolved = requirements
            .resolve(&runtimes, &YonIsolationPolicy::Process)
            .expect("requirements");
        let keys = resolved
            .iter()
            .map(RuntimeRequirement::key)
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                RuntimeRequirementKey::JavaScript,
                RuntimeRequirementKey::Python,
                RuntimeRequirementKey::Java,
                RuntimeRequirementKey::Php,
                RuntimeRequirementKey::KotlinCompiler,
                RuntimeRequirementKey::Dotnet,
                RuntimeRequirementKey::RustCompiler,
            ]
        );
        for value in &resolved {
            assert!(!value.label().contains("secret"), "{}", value.label());
            assert!(!value.help().contains("secret"), "{}", value.help());
        }
        assert_eq!(
            keys.iter()
                .filter(|key| **key == RuntimeRequirementKey::Java)
                .count(),
            1
        );
        assert_eq!(
            keys.iter()
                .filter(|key| **key == RuntimeRequirementKey::JavaScript)
                .count(),
            1
        );
    }

    #[test]
    fn unknown_extensions_require_nothing() {
        assert_eq!(
            RuntimeRequirements::from_extensions(["rb", "html"])
                .languages
                .len(),
            0
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runtime_only_dotnet_fake_fails_the_real_build_probe_without_path_disclosure() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("fake runtime directory");
        let canary = directory.path().join("secret-runtime-only-dotnet-canary");
        fs::write(
            &canary,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 10.0.302; exit 0; fi\nif [ \"$1\" = \"--list-runtimes\" ]; then echo 'Microsoft.NETCore.App 10.0.0 [/fake]'; exit 0; fi\nexit 9\n",
        )
        .expect("fake runtime");
        fs::set_permissions(&canary, fs::Permissions::from_mode(0o700)).expect("permissions");
        let requirement = RuntimeRequirement {
            key: RuntimeRequirementKey::Dotnet,
            program: canary.clone(),
            command_label: String::from("dotnet"),
            label: String::from("Yon C# runtime 'dotnet'"),
            help: String::from("Install the .NET SDK and Microsoft.NETCore.App runtime."),
            probe: ProbeKind::DotnetBuild,
        };
        let environment = EnvironmentPolicy::default();

        let asynchronous = probe_all(vec![requirement.clone()], &environment)
            .await
            .expect("asynchronous probe");
        let synchronous =
            probe_all_sync(vec![requirement], &environment).expect("synchronous probe");
        for results in [&asynchronous, &synchronous] {
            assert_eq!(results[0].state(), RuntimeProbeState::Incomplete);
            let failure = readiness_failure(results).expect("readiness failure");
            let rendered = failure.to_string();
            assert!(rendered.contains("TY2101"), "{rendered}");
            assert!(
                !rendered.contains(canary.to_string_lossy().as_ref()),
                "{rendered}"
            );
        }
    }
}
