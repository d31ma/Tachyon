//! The `ty` command-line interface.

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use std::hash::{Hash as _, Hasher as _};
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use tachyon_contracts::{
    HandlerBody, HandlerBodyEncoding, HandlerRequest, HttpMethod, NativeTarget,
};
use tachyon_core::{
    BuildOptions, DevServer, DevServerOptions, EnvironmentPolicy, Failure, HandlerCancellation,
    HandlerRuntimePrograms, HandlerSource, HandlerSupervisor, HandlerSupervisorOptions,
    MigrationAnalysis, NativeBuildOptions, NativeCompiler, PreviewServer, PreviewServerOptions,
    Scaffold, WebCompiler, native_target_directory,
};
use tachyon_diagnostics::{Diagnostic, DiagnosticCode, Severity};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum DiagnosticFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum BuildTarget {
    #[value(alias = "browser")]
    Web,
    #[value(aliases = ["mac", "darwin", "osx"])]
    Macos,
    Ios,
    Linux,
    #[value(aliases = ["win", "win32"])]
    Windows,
    Android,
    All,
}

impl BuildTarget {
    /// Returns the Native UI target, or `None` for the web target.
    const fn native(self) -> Option<NativeTarget> {
        match self {
            Self::Web | Self::All => None,
            Self::Macos => Some(NativeTarget::Macos),
            Self::Ios => Some(NativeTarget::Ios),
            Self::Linux => Some(NativeTarget::Linux),
            Self::Windows => Some(NativeTarget::Windows),
            Self::Android => Some(NativeTarget::Android),
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "ty",
    version = tachyon_contracts::PRODUCT_VERSION,
    about = "Tachyon CLI",
    disable_version_flag = true,
    disable_help_subcommand = true,
    help_template = "ty {version} — Tachyon CLI\n\nUsage: ty <command> [options]\n\nCommands:\n  init [name]        Scaffold a new Tachyon app\n  serve              Run the server (dev or production)\n  bundle             Build client + native artifacts\n  native-bundle      Generate the native host only\n  preview            Preview a built bundle\n  cache [status|clean] Inspect or clear standalone runtime cache\n\nRun 'ty <command> --help' for command-specific options.\n"
)]
struct Cli {
    /// Print the Tachyon product version.
    #[arg(short = 'v', long = "version", global = true)]
    version: bool,
    /// Compatibility alias for the former Clap version flag.
    #[arg(short = 'V', global = true, hide = true)]
    version_upper: bool,
    #[arg(long, global = true, value_enum, default_value_t = DiagnosticFormat::Human)]
    diagnostic_format: DiagnosticFormat,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a minimal HTML-only Tachyon project.
    #[command(display_order = 1)]
    Init {
        /// Missing or empty target directory.
        #[arg(default_value = ".")]
        target: PathBuf,
        /// Human-readable application name.
        #[arg(long, visible_alias = "app-name")]
        name: Option<String>,
    },
    /// Build client and native artifacts.
    #[command(display_order = 3)]
    Bundle {
        /// Tachyon project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Project-relative output directory. Defaults to dist/web for web and
        /// dist for native targets.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Disable verified incremental route reuse.
        #[arg(long)]
        no_incremental: bool,
        /// Rebuild when project sources change.
        #[arg(long, visible_alias = "bundle-watch")]
        watch: bool,
        /// Packaging target; non-web targets always use native-first rendering.
        #[arg(long, visible_alias = "targets", value_enum, value_delimiter = ',', num_args = 1..)]
        target: Vec<BuildTarget>,
        /// Compatibility switch; native hosts are always generated now.
        #[arg(long, hide = true)]
        skip_native_host: bool,
        /// Start a watcher without performing its initial build.
        #[arg(long, hide = true)]
        skip_initial_build: bool,
        /// Compatibility switch; strict-CSP safety is always enforced.
        #[arg(long, hide = true)]
        csp_check: bool,
        /// Compatibility switch; packaging is already enabled by default.
        #[arg(long, hide = true)]
        package: bool,
        /// Stage native source and resources without invoking the platform packager.
        #[arg(long, hide = true)]
        skip_package: bool,
        /// Deprecated render-mode switch; native-first rendering is unconditional.
        #[arg(long, hide = true)]
        render_mode: Option<String>,
    },
    /// Rewrite-internal deterministic build command.
    #[command(hide = true)]
    Build {
        /// Tachyon project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Project-relative output directory.
        #[arg(long, default_value = "dist")]
        out_dir: PathBuf,
        /// Disable verified incremental route reuse.
        #[arg(long)]
        no_incremental: bool,
        /// Packaging target; non-web targets always use native-first rendering.
        #[arg(long, value_enum, default_value_t = BuildTarget::Web)]
        target: BuildTarget,
    },
    /// Generate a native host application.
    #[command(display_order = 4)]
    NativeBundle {
        /// Tachyon project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Project-relative output directory.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Disable verified incremental route reuse.
        #[arg(long)]
        no_incremental: bool,
        /// Native packaging target.
        #[arg(long, visible_alias = "targets", value_enum, value_delimiter = ',', num_args = 1..)]
        target: Vec<BuildTarget>,
        /// Deprecated render-mode switch; native-first rendering is unconditional.
        #[arg(long, hide = true)]
        render_mode: Option<String>,
    },
    /// Run the Tachyon server.
    #[command(name = "serve", alias = "dev", display_order = 2)]
    Dev {
        /// Tachyon project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Interface address.
        #[arg(long, visible_alias = "hostname")]
        host: Option<String>,
        /// TCP port; zero requests an ephemeral port.
        #[arg(long)]
        port: Option<u16>,
        /// Explicitly permit exposure beyond the local machine.
        #[arg(long)]
        allow_non_loopback: bool,
        /// Project-relative output directory.
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// Do not rebuild when sources change.
        #[arg(long)]
        no_watch: bool,
        /// Serve the existing bundle without compiling it first.
        #[arg(long)]
        no_bundle: bool,
        /// Compatibility alias; source watching is already enabled by default.
        #[arg(long, hide = true)]
        bundle_watch: bool,
    },
    /// Preview a built bundle.
    #[command(display_order = 5)]
    Preview {
        /// Tachyon project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Interface address.
        #[arg(long, visible_alias = "hostname")]
        host: Option<String>,
        /// TCP port; zero requests an ephemeral port.
        #[arg(long)]
        port: Option<u16>,
        /// Explicitly permit exposure beyond the local machine.
        #[arg(long)]
        allow_non_loopback: bool,
        /// Project-relative output directory.
        #[arg(long, default_value = "dist")]
        out_dir: PathBuf,
        /// Rebuild the selected target when sources change.
        #[arg(long, visible_alias = "bundle-watch")]
        watch: bool,
        /// Built target whose embedded web bundle should be served.
        #[arg(long, value_enum)]
        target: Option<BuildTarget>,
        /// Compatibility switch; the rewrite preview does not require native toolchains.
        #[arg(long, hide = true)]
        skip_native_checks: bool,
    },
    /// Report whether this machine has the toolchains a project needs.
    #[command(hide = true)]
    Doctor {
        /// Tachyon project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Emit the machine-readable report instead of the human summary.
        #[arg(long)]
        json: bool,
    },
    /// Inspect or clear the installation cache.
    #[command(display_order = 6)]
    Cache {
        #[command(subcommand)]
        command: Option<CacheCommand>,
    },
    /// Invoke and inspect supervised Yon handlers.
    #[command(hide = true)]
    Handler {
        #[command(subcommand)]
        command: HandlerCommand,
    },
    /// Analyze a legacy project for migration to this implementation.
    #[command(hide = true)]
    Migrate {
        #[command(subcommand)]
        command: MigrateCommand,
    },
    /// Print command help.
    #[command(hide = true)]
    Help {
        /// Optional command whose help should be printed.
        command: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum CacheCommand {
    /// Report where the cache lives and what it holds.
    Status,
    /// Remove the cache directory.
    Clean,
}

#[derive(Debug, Subcommand)]
enum MigrateCommand {
    /// Report every construct that this implementation does not yet support.
    Check {
        /// Tachyon project root.
        #[arg(default_value = ".")]
        project: PathBuf,
        /// Emit the machine-readable report instead of the human summary.
        #[arg(long)]
        json: bool,
        /// Exit successfully even when unsupported constructs are found.
        #[arg(long)]
        allow_unsupported: bool,
    },
}

#[derive(Debug, Subcommand)]
enum HandlerCommand {
    /// Invoke one JavaScript or Python Yon handler through Handler Protocol v1.
    Invoke {
        /// Project-relative `server/routes/**/yon.js` or `yon.py`.
        source: PathBuf,
        /// Tachyon project root.
        #[arg(long, default_value = ".")]
        project: PathBuf,
        /// Canonical route supplied to the handler.
        #[arg(long, default_value = "/")]
        route: String,
        /// Uppercase HTTP method.
        #[arg(long, value_enum, ignore_case = true, default_value_t = CliHttpMethod::Get)]
        method: CliHttpMethod,
        /// Stable request correlation identifier.
        #[arg(long, default_value = "cli_request")]
        request_id: String,
        /// Optional UTF-8 request body.
        #[arg(long)]
        body: Option<String>,
        /// Repeated Handler Protocol header in `name=value` form.
        #[arg(long = "header")]
        headers: Vec<HeaderArgument>,
        /// Complete invocation deadline in milliseconds.
        #[arg(long, default_value_t = 30_000, value_parser = clap::value_parser!(u64).range(1..=300_000))]
        timeout_ms: u64,
        /// Explicit environment variable name to inherit.
        #[arg(long = "allow-env")]
        allow_environment: Vec<String>,
        /// Node.js executable name or path.
        #[arg(long, default_value = "node")]
        javascript_runtime: PathBuf,
        /// `CPython` executable name or path.
        #[arg(long)]
        python_runtime: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliHttpMethod {
    Delete,
    Get,
    Head,
    Options,
    Patch,
    Post,
    Put,
}

impl From<CliHttpMethod> for HttpMethod {
    fn from(method: CliHttpMethod) -> Self {
        match method {
            CliHttpMethod::Delete => Self::Delete,
            CliHttpMethod::Get => Self::Get,
            CliHttpMethod::Head => Self::Head,
            CliHttpMethod::Options => Self::Options,
            CliHttpMethod::Patch => Self::Patch,
            CliHttpMethod::Post => Self::Post,
            CliHttpMethod::Put => Self::Put,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HeaderArgument {
    name: String,
    value: String,
}

impl FromStr for HeaderArgument {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((name, value)) = value.split_once('=') else {
            return Err(String::from("header must use name=value"));
        };
        if name.is_empty() {
            return Err(String::from("header name cannot be empty"));
        }
        Ok(Self {
            name: String::from(name),
            value: String::from(value),
        })
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if cli.version || cli.version_upper {
        println!("{}", tachyon_contracts::PRODUCT_VERSION);
        return;
    }
    let Some(command) = &cli.command else {
        let mut root = Cli::command();
        let _ = root.print_help();
        println!();
        std::process::exit(1);
    };
    let result = execute(command).await;
    if let Err(failure) = result {
        print_failure(&failure, cli.diagnostic_format);
        std::process::exit(1);
    }
}

async fn execute(command: &Command) -> Result<(), Failure> {
    match command {
        Command::Init { target, name } => {
            let environment_name = std::env::var("TAC_APP_NAME").ok();
            let name = name.as_deref().or(environment_name.as_deref());
            let result = Scaffold::create(target, name)?;
            println!(
                "Initialized '{}' in {}",
                result.app_name(),
                result.root().display()
            );
            println!("Next: ty bundle {}", result.root().display());
            Ok(())
        }
        Command::Bundle { .. } => execute_bundle_command(command).await,
        Command::Build {
            project,
            out_dir,
            no_incremental,
            target,
        } => execute_build(project, out_dir, *no_incremental, *target).await,
        Command::NativeBundle { .. } => execute_native_bundle_command(command).await,
        Command::Dev { .. } => execute_dev_command(command).await,
        Command::Preview { .. } => execute_preview_command(command).await,
        Command::Doctor { project, json } => execute_doctor(project, *json),
        Command::Cache { command } => execute_cache(command.as_ref()),
        Command::Handler { command } => execute_handler(command).await,
        Command::Migrate { command } => execute_migrate(command),
        Command::Help { command } => {
            let mut root = Cli::command();
            if let Some(name) = command
                && let Some(subcommand) = root.find_subcommand_mut(name)
            {
                let _ = subcommand.print_long_help();
                println!();
                return Ok(());
            }
            let _ = root.print_long_help();
            println!();
            Ok(())
        }
    }
}

async fn execute_bundle_command(command: &Command) -> Result<(), Failure> {
    let Command::Bundle {
        project,
        out_dir,
        no_incremental,
        watch,
        target,
        skip_initial_build,
        render_mode,
        skip_package,
        ..
    } = command
    else {
        unreachable!();
    };
    reject_render_mode(render_mode.as_deref())?;
    let targets = resolve_targets(
        target,
        &["TAC_BUNDLE_TARGET", "TAC_TARGET"],
        Some(BuildTarget::Web),
    )?;
    let output = bundle_output(out_dir.as_deref(), &targets);
    let options = BundleExecution {
        no_incremental: *no_incremental,
        watch: *watch,
        initial_build: if *skip_initial_build {
            InitialBuild::Skip
        } else {
            InitialBuild::Run
        },
        package: !*skip_package,
    };
    execute_bundle(project, &output, &targets, options).await
}

async fn execute_native_bundle_command(command: &Command) -> Result<(), Failure> {
    let Command::NativeBundle {
        project,
        out_dir,
        no_incremental,
        target,
        render_mode,
    } = command
    else {
        unreachable!();
    };
    reject_render_mode(render_mode.as_deref())?;
    let targets = resolve_targets(target, &["TAC_BUNDLE_TARGET", "TAC_TARGET"], None)?;
    let target = require_one_native_target(&targets, "native-bundle")?;
    let output = out_dir
        .clone()
        .or_else(|| environment_path("TAC_DIST_PATH"))
        .unwrap_or_else(|| PathBuf::from("dist"));
    execute_build_with_package(project, &output, *no_incremental, target, false).await
}

async fn execute_dev_command(command: &Command) -> Result<(), Failure> {
    let Command::Dev {
        project,
        host,
        port,
        allow_non_loopback,
        out_dir,
        no_watch,
        no_bundle,
        ..
    } = command
    else {
        unreachable!();
    };
    let host = resolve_host(host.as_deref(), &["YON_HOST", "YON_HOSTNAME", "HOST"])?;
    let port = resolve_port(*port, &["YON_PORT", "PORT"], 8080)?;
    let output = out_dir
        .clone()
        .or_else(|| environment_path("TAC_DIST_PATH"))
        .unwrap_or_else(|| PathBuf::from("dist/web"));
    let production = std::env::var("NODE_ENV").is_ok_and(|value| value == "production");
    execute_serve(
        project,
        host,
        port,
        *allow_non_loopback,
        &output,
        *no_watch || production,
        *no_bundle || truthy_environment("YON_SKIP_BUNDLE") || production,
    )
    .await
}

async fn execute_preview_command(command: &Command) -> Result<(), Failure> {
    let Command::Preview {
        project,
        host,
        port,
        allow_non_loopback,
        out_dir,
        watch,
        target,
        ..
    } = command
    else {
        unreachable!();
    };
    let host = resolve_host(host.as_deref(), &[])?;
    let port = resolve_port(*port, &[], 3000)?;
    let explicit_targets: Vec<_> = target.iter().copied().collect();
    let targets = resolve_targets(
        &explicit_targets,
        &["TAC_PREVIEW_TARGET", "TAC_TARGET"],
        Some(BuildTarget::Web),
    )?;
    let target = require_one_target(&targets, "preview")?;
    execute_preview(
        project,
        host,
        port,
        *allow_non_loopback,
        out_dir,
        *watch,
        target,
    )
    .await
}

#[derive(Clone, Copy)]
struct BundleExecution {
    no_incremental: bool,
    watch: bool,
    initial_build: InitialBuild,
    package: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum InitialBuild {
    Run,
    Skip,
}

async fn execute_serve(
    project: &Path,
    host: IpAddr,
    port: u16,
    allow_non_loopback: bool,
    out_dir: &Path,
    no_watch: bool,
    no_bundle: bool,
) -> Result<(), Failure> {
    tachyon_core::cache::ensure_runtime()?;
    let server = DevServer::bind(
        project,
        &DevServerOptions {
            host,
            port,
            allow_non_loopback,
            output_directory: out_dir.to_path_buf(),
            watch: !no_watch,
            build: !no_bundle,
        },
    )
    .await?;
    println!("Tachyon server ready at http://{}/", server.address());
    if !no_watch {
        println!("Watching sources; open pages receive semantic hot updates.");
    }
    let _flush_result = std::io::stdout().flush();
    server
        .run_until(async {
            let _signal_result = tokio::signal::ctrl_c().await;
        })
        .await
}

async fn execute_preview(
    project: &Path,
    host: IpAddr,
    port: u16,
    allow_non_loopback: bool,
    out_dir: &Path,
    watch: bool,
    target: BuildTarget,
) -> Result<(), Failure> {
    if watch {
        let build_output = preview_build_output(out_dir, target);
        execute_build(project, &build_output, false, target).await?;
        spawn_bundle_watcher(project.to_path_buf(), build_output, target);
    }

    let root = resolve_preview_root(project, out_dir, target);
    let server = PreviewServer::bind(
        &root,
        &PreviewServerOptions {
            host,
            port,
            allow_non_loopback,
        },
    )
    .await?;
    println!(
        "Tachyon preview ready at http://{}/ ({})",
        server.address(),
        server.root().display()
    );
    if watch {
        println!("Watching sources and rebuilding the preview.");
    }
    let _flush_result = std::io::stdout().flush();
    server
        .run_until(async {
            let _signal_result = tokio::signal::ctrl_c().await;
        })
        .await
}

async fn execute_bundle(
    project: &Path,
    out_dir: &Path,
    targets: &[BuildTarget],
    options: BundleExecution,
) -> Result<(), Failure> {
    tachyon_core::cache::ensure_runtime()?;
    if !options.watch || options.initial_build == InitialBuild::Run {
        execute_build_targets(
            project,
            out_dir,
            options.no_incremental,
            targets,
            options.package,
        )
        .await?;
    }
    if !options.watch {
        return Ok(());
    }

    println!("Watching sources and rebuilding the bundle.");
    let mut fingerprint = source_fingerprint(project);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            () = tokio::time::sleep(Duration::from_millis(400)) => {
                let next = source_fingerprint(project);
                if next == fingerprint {
                    continue;
                }
                fingerprint = next;
                if let Err(failure) = execute_build_targets(project, out_dir, options.no_incremental, targets, options.package).await {
                    eprint!("{failure}");
                }
            }
        }
    }
}

async fn execute_build_targets(
    project: &Path,
    out_dir: &Path,
    no_incremental: bool,
    targets: &[BuildTarget],
    package: bool,
) -> Result<(), Failure> {
    let multiple = targets.len() > 1;
    for target in targets {
        let output = if multiple && *target == BuildTarget::Web {
            out_dir.join(target_directory(BuildTarget::Web))
        } else {
            out_dir.to_path_buf()
        };
        execute_build_with_package(project, &output, no_incremental, *target, package).await?;
    }
    Ok(())
}

const fn target_directory(target: BuildTarget) -> &'static str {
    match target {
        BuildTarget::Web => "web",
        BuildTarget::Macos => "macos",
        BuildTarget::Ios => "ios",
        BuildTarget::Linux => "linux",
        BuildTarget::Windows => "windows",
        BuildTarget::Android => "android",
        BuildTarget::All => "all",
    }
}

fn bundle_output(explicit: Option<&Path>, targets: &[BuildTarget]) -> PathBuf {
    if let Some(output) = explicit {
        return output.to_path_buf();
    }
    if let Some(output) = environment_path("TAC_DIST_PATH") {
        return output;
    }
    if targets == [BuildTarget::Web] {
        PathBuf::from("dist/web")
    } else {
        PathBuf::from("dist")
    }
}

fn resolve_targets(
    explicit: &[BuildTarget],
    environment_names: &[&str],
    default: Option<BuildTarget>,
) -> Result<Vec<BuildTarget>, Failure> {
    let requested = if explicit.is_empty() {
        let environment = environment_names.iter().find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        });
        if let Some(environment) = environment {
            parse_target_list(&environment)?
        } else {
            default.into_iter().collect()
        }
    } else {
        explicit.to_vec()
    };

    let mut resolved = Vec::new();
    for target in requested {
        let expansion: &[BuildTarget] = if target == BuildTarget::All {
            &[
                BuildTarget::Web,
                BuildTarget::Macos,
                BuildTarget::Windows,
                BuildTarget::Linux,
                BuildTarget::Android,
                BuildTarget::Ios,
            ]
        } else {
            std::slice::from_ref(&target)
        };
        for expanded in expansion {
            if !resolved.contains(expanded) {
                resolved.push(*expanded);
            }
        }
    }
    Ok(resolved)
}

fn parse_target_list(value: &str) -> Result<Vec<BuildTarget>, Failure> {
    value
        .split(|character: char| character == ',' || character.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .map(parse_target)
        .collect()
}

fn parse_target(value: &str) -> Result<BuildTarget, Failure> {
    match value.to_ascii_lowercase().as_str() {
        "web" | "browser" => Ok(BuildTarget::Web),
        "macos" | "mac" | "darwin" | "osx" => Ok(BuildTarget::Macos),
        "ios" => Ok(BuildTarget::Ios),
        "linux" => Ok(BuildTarget::Linux),
        "windows" | "win" | "win32" => Ok(BuildTarget::Windows),
        "android" => Ok(BuildTarget::Android),
        "all" => Ok(BuildTarget::All),
        _ => Err(cli_failure(
            1001,
            format!("Unknown bundle target '{value}'."),
            "Use web, macos, windows, linux, android, ios, or all.",
        )),
    }
}

fn require_one_target(targets: &[BuildTarget], command: &str) -> Result<BuildTarget, Failure> {
    if let [target] = targets {
        return Ok(*target);
    }
    Err(cli_failure(
        1001,
        format!("The {command} command requires exactly one target."),
        format!("Pass one target with `ty {command} --target <target>`."),
    ))
}

fn require_one_native_target(
    targets: &[BuildTarget],
    command: &str,
) -> Result<BuildTarget, Failure> {
    let target = require_one_target(targets, command)?;
    if target.native().is_some() {
        return Ok(target);
    }
    Err(cli_failure(
        1001,
        format!("The {command} command requires a native target."),
        "Use macos, windows, linux, android, or ios.",
    ))
}

fn environment_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn truthy_environment(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| matches!(value.as_str(), "1" | "true"))
}

fn reject_render_mode(explicit: Option<&str>) -> Result<(), Failure> {
    if explicit.is_some()
        || std::env::var("TAC_RENDER_MODE").is_ok_and(|value| !value.trim().is_empty())
    {
        return Err(cli_failure(
            1001,
            "--render-mode and TAC_RENDER_MODE have been removed.",
            "Non-web targets are always native-first and use local WebView boundaries only for unmapped HTML and Web Components.",
        ));
    }
    Ok(())
}

fn resolve_host(explicit: Option<&str>, environment_names: &[&str]) -> Result<IpAddr, Failure> {
    let value = explicit.map(String::from).or_else(|| {
        environment_names.iter().find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
    });
    let Some(value) = value else {
        return Ok(IpAddr::from([127, 0, 0, 1]));
    };
    if value.eq_ignore_ascii_case("localhost") {
        return Ok(IpAddr::from([127, 0, 0, 1]));
    }
    value.parse().map_err(|_| {
        cli_failure(
            1302,
            format!("Invalid host address '{value}'."),
            "Use an IPv4 or IPv6 address, or localhost.",
        )
    })
}

fn resolve_port(
    explicit: Option<u16>,
    environment_names: &[&str],
    default: u16,
) -> Result<u16, Failure> {
    if let Some(port) = explicit {
        return Ok(port);
    }
    let Some((name, value)) = environment_names.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| (*name, value))
    }) else {
        return Ok(default);
    };
    value.parse().map_err(|_| {
        cli_failure(
            1302,
            format!("Invalid port '{value}' from {name}."),
            "Use a TCP port from 0 through 65535.",
        )
    })
}

fn cli_failure(number: u16, message: impl Into<String>, help: impl Into<String>) -> Failure {
    Failure::one(Diagnostic {
        code: DiagnosticCode::from_number(number).unwrap_or_else(|| unreachable!()),
        severity: Severity::Error,
        message: message.into(),
        help: Some(help.into()),
        spans: Vec::new(),
    })
}

fn spawn_bundle_watcher(project: PathBuf, out_dir: PathBuf, target: BuildTarget) {
    tokio::spawn(async move {
        let mut fingerprint = source_fingerprint(&project);
        loop {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let next = source_fingerprint(&project);
            if next == fingerprint {
                continue;
            }
            fingerprint = next;
            if let Err(failure) = execute_build(&project, &out_dir, false, target).await {
                eprint!("{failure}");
            }
        }
    });
}

fn source_fingerprint(project: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut pending = vec![project.join("client"), project.join("server")];
    pending.extend([
        project.join("tachyon.json"),
        project.join("tac.config.js"),
        project.join("package.json"),
    ]);
    while let Some(current) = pending.pop() {
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            continue;
        };
        if metadata.is_dir() {
            let Ok(entries) = std::fs::read_dir(&current) else {
                continue;
            };
            let mut children: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
            children.sort();
            pending.extend(children);
            continue;
        }
        current.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified()
            && let Ok(elapsed) = modified.duration_since(std::time::UNIX_EPOCH)
        {
            elapsed.as_nanos().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn preview_build_output(out_dir: &Path, target: BuildTarget) -> PathBuf {
    if target == BuildTarget::Web && out_dir == Path::new("dist") {
        out_dir.join("web")
    } else {
        out_dir.to_path_buf()
    }
}

fn resolve_preview_root(project: &Path, out_dir: &Path, target: BuildTarget) -> PathBuf {
    let output = project.join(out_dir);
    let direct = output.join("index.html");
    if target != BuildTarget::Web && direct.is_file() {
        return output;
    }

    let target_name = match target {
        BuildTarget::Web | BuildTarget::All => "web",
        BuildTarget::Macos => "macos",
        BuildTarget::Ios => "ios",
        BuildTarget::Linux => "linux",
        BuildTarget::Windows => "windows",
        BuildTarget::Android => "android",
    };
    let target_root = output.join(target_name);
    for candidate in [target_root.join("web"), target_root.clone()] {
        if candidate.join("index.html").is_file() {
            return candidate;
        }
    }

    if let Ok(entries) = std::fs::read_dir(&target_root) {
        for app in entries.flatten().map(|entry| entry.path()) {
            for candidate in [
                app.join("WebBundle"),
                app.join("Contents/Resources/WebBundle"),
                app.join("Resources/WebBundle"),
            ] {
                if candidate.join("index.html").is_file() {
                    return candidate;
                }
            }
        }
    }

    if target == BuildTarget::Web && direct.is_file() {
        return output;
    }
    target_root
}

async fn execute_build(
    project: &Path,
    out_dir: &Path,
    no_incremental: bool,
    target: BuildTarget,
) -> Result<(), Failure> {
    execute_build_with_package(project, out_dir, no_incremental, target, true).await
}

async fn execute_build_with_package(
    project: &Path,
    out_dir: &Path,
    no_incremental: bool,
    target: BuildTarget,
    package: bool,
) -> Result<(), Failure> {
    if target == BuildTarget::All {
        execute_web_build(project, &out_dir.join("web"), no_incremental).await?;
        for native in [
            NativeTarget::Macos,
            NativeTarget::Windows,
            NativeTarget::Linux,
            NativeTarget::Android,
            NativeTarget::Ios,
        ] {
            execute_native_build(project, out_dir, native, package).await?;
        }
        return Ok(());
    }
    if let Some(native) = target.native() {
        return execute_native_build(project, out_dir, native, package).await;
    }

    execute_web_build(project, out_dir, no_incremental).await
}

async fn execute_web_build(
    project: &Path,
    out_dir: &Path,
    no_incremental: bool,
) -> Result<(), Failure> {
    let result = WebCompiler::build_async(
        project,
        &BuildOptions {
            output_directory: out_dir.to_path_buf(),
            incremental: !no_incremental,
        },
    )
    .await?;
    let noun = if result.route_count() == 1 {
        "route"
    } else {
        "routes"
    };
    println!(
        "Built {} {noun} to {} ({}) compiled={} reused={}",
        result.route_count(),
        result.output_directory().display(),
        result.sha256(),
        result.compiled_routes(),
        result.reused_routes()
    );
    Ok(())
}

async fn execute_native_build(
    project: &Path,
    out_dir: &Path,
    native: NativeTarget,
    package: bool,
) -> Result<(), Failure> {
    let result = NativeCompiler::build_async(
        project,
        &NativeBuildOptions {
            output_directory: out_dir.to_path_buf(),
            target: native,
            package,
        },
    )
    .await?;
    println!(
        "Built {} app with {} routes (native_nodes={} web_surfaces={}) to {} ({})",
        native_target_directory(native),
        result.route_count(),
        result.native_node_count(),
        result.web_surface_count(),
        result.application_bundle().display(),
        result.sha256()
    );
    Ok(())
}

fn execute_doctor(project: &PathBuf, json: bool) -> Result<(), Failure> {
    let report = tachyon_core::doctor::check(project)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| String::from("{}"))
        );
    } else {
        print!("{}", report.to_text());
    }
    // A missing toolchain is a fact about the machine, not a failed command,
    // so the report is the answer either way.
    Ok(())
}

fn execute_cache(command: Option<&CacheCommand>) -> Result<(), Failure> {
    if matches!(command, Some(CacheCommand::Clean)) {
        // Report what was removed, not what is there, since nothing is.
        let removed = tachyon_core::cache::clean()?;
        println!(
            "Cleared Tachyon runtime cache: {}",
            removed.root.join("runtime").display()
        );
        return Ok(());
    }

    let status = tachyon_core::cache::status();
    println!(
        "Tachyon cache\n  Root: {}\n  Runtime entries: {}",
        status.root.display(),
        status.runtime_entries
    );
    Ok(())
}

fn execute_migrate(command: &MigrateCommand) -> Result<(), Failure> {
    let MigrateCommand::Check {
        project,
        json,
        allow_unsupported,
    } = command;
    let report = MigrationAnalysis::check(project)?;
    if *json {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        if serde_json::to_writer_pretty(&mut lock, &report).is_ok() {
            let _newline_result = writeln!(lock);
        }
    } else {
        print!("{}", report.to_text());
    }
    if *allow_unsupported {
        return Ok(());
    }
    report.unsupported_failure().map_or(Ok(()), Err)
}

async fn execute_handler(command: &HandlerCommand) -> Result<(), Failure> {
    match command {
        HandlerCommand::Invoke {
            source,
            project,
            route,
            method,
            request_id,
            body,
            headers,
            timeout_ms,
            allow_environment,
            javascript_runtime,
            python_runtime,
        } => {
            let source = HandlerSource::discover(project, source)?;
            let environment = EnvironmentPolicy::from_names(allow_environment.clone())?;
            let mut runtimes = HandlerRuntimePrograms {
                javascript: javascript_runtime.clone(),
                ..HandlerRuntimePrograms::default()
            };
            if let Some(python_runtime) = python_runtime {
                runtimes.python.clone_from(python_runtime);
            }
            let mut options = HandlerSupervisorOptions::from_environment()?;
            options.default_timeout = Duration::from_millis(*timeout_ms);
            options.runtimes = runtimes;
            options.environment = environment;
            let supervisor = HandlerSupervisor::new(options)?;
            let mut request =
                HandlerRequest::route(request_id.clone(), route.clone(), HttpMethod::from(*method));
            request.deadline_ms = Some(*timeout_ms);
            for header in headers {
                request
                    .headers
                    .entry(header.name.clone())
                    .or_default()
                    .push(header.value.clone());
            }
            request.body = body.as_ref().map(|data| HandlerBody {
                encoding: HandlerBodyEncoding::Utf8,
                data: data.clone(),
            });
            let cancellation = HandlerCancellation::default();
            let signal = cancellation.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    signal.cancel();
                }
            });
            let response = supervisor.invoke(&source, &request, &cancellation).await?;
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            if serde_json::to_writer_pretty(&mut lock, &response).is_err() {
                return Ok(());
            }
            let _newline_result = writeln!(lock);
            Ok(())
        }
    }
}

fn print_failure(failure: &Failure, format: DiagnosticFormat) {
    match format {
        DiagnosticFormat::Human => eprint!("{failure}"),
        DiagnosticFormat::Json => {
            let stderr = std::io::stderr();
            let mut lock = stderr.lock();
            if serde_json::to_writer_pretty(&mut lock, &failure.report()).is_err() {
                eprint!("{failure}");
                return;
            }
            let _newline_result = writeln!(lock);
        }
    }
}
