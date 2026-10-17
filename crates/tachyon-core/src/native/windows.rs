use super::config::NativeApplication;
use super::csharp_browser_bindings::CsharpBrowserBindings;
use super::csharp_web_apis::CsharpWebApis;
use super::dotnet_web_release::DotnetWebRelease;
use super::host::{
    GeneratedHost, first_line, native_tool_failure, run_tool, stage_application, write,
    write_host_source,
};
use super::routes::NativeRouteIndex;
use super::windows_architecture::WindowsArchitecture;
use super::windows_compiler_plan::WindowsCompilerPlan;
use crate::Failure;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WindowsHostGenerator;

impl WindowsHostGenerator {
    pub(super) async fn generate(
        application: &NativeApplication,
        index: &NativeRouteIndex,
        companions: &[super::registry::NativeCompanionInput],
        web_bundle: &Path,
        stage: &Path,
        options: &super::compiler::NativeBuildOptions,
    ) -> Result<GeneratedHost, Failure> {
        let architecture = options.windows_architecture;
        let bundle = stage.join(&application.executable_name);
        let resources = bundle.join("resources");
        stage_application(index, web_bundle, stage, &resources)?;

        let companion =
            stage_companion(companions, stage, &application.application_id, architecture)?;
        let rust_companion = super::rust::RustCompanion::stage_for_host(
            companions,
            stage,
            application,
            super::rust::Linkage::Shared,
        )?;
        let source_path = stage.join("project").join("tachyon_host.c");
        write_host_source(&source_path, &c_source(application, index, companions))?;
        write(
            &bundle.join("application.manifest"),
            application_manifest(application).as_bytes(),
        )?;
        if !options.package {
            return Ok(GeneratedHost::not_packaged(PathBuf::from(
                "project/tachyon_host.c",
            )));
        }

        let executable = bundle
            .join("bin")
            .join(format!("{}.exe", application.executable_name));
        let resource = icon_resource(application, web_bundle, stage).await?;
        let compiler_version =
            compile_c(&source_path, &executable, resource.as_deref(), architecture).await?;
        if let Some(project) = companion {
            publish_companion(&project, &bundle.join("bin"), architecture).await?;
        }
        compile_rust_companion(
            application,
            rust_companion.as_deref(),
            &bundle,
            architecture,
        )
        .await?;
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(&application.executable_name),
            toolchain_name: String::from("msvc"),
            toolchain_version: compiler_version,
        })
    }
}

impl WindowsHostGenerator {
    const MENU_MNEMONIC: char = '&';
    const WEBVIEW2_SDK: &str = "TAC_WEBVIEW2_SDK";
    const DEVELOPER_TARGET: &str = "VSCMD_ARG_TGT_ARCH";
    pub(super) const LINKER_ARGUMENTS: [&str; 12] = [
        "WebView2LoaderStatic.lib",
        "user32.lib",
        "shell32.lib",
        "shlwapi.lib",
        "comctl32.lib",
        "gdi32.lib",
        "ole32.lib",
        "oleaut32.lib",
        "version.lib",
        "advapi32.lib",
        "/SUBSYSTEM:WINDOWS",
        "/ENTRY:wWinMainCRTStartup",
    ];
    const CSHARP_COMPANION_PRELUDE: &str = concat!(
        include_str!("models/TacModels.cs"),
        include_str!("prelude.cs")
    );
    const CSHARP_BROWSER_TASKS: &str = concat!(
        include_str!("browser/Tac.cs"),
        include_str!("browser/TacTaskAnswer.cs"),
        include_str!("browser/TacTaskInvocation.cs"),
        include_str!("browser/TacHttpTransport.cs"),
        include_str!("browser/TacHttpInvocation.cs")
    );
    const CSHARP_COMPANION_USING: &str = "using System;\nusing System.Collections;\n\
                                      using System.Collections.Generic;\n\
                                      using System.Globalization;\nusing System.Linq;\n\
                                      using System.Runtime.InteropServices;\n\
                                      using System.Text;\nusing System.Text.Json.Nodes;\n";
    const CSHARP_COMPANION_PROJECT: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
    <RuntimeIdentifier>__RUNTIME_IDENTIFIER__</RuntimeIdentifier>
    <OutputType>Library</OutputType>
    <Nullable>disable</Nullable>
    <PublishAot>true</PublishAot>
    <NativeLib>Shared</NativeLib>
    <InvariantGlobalization>true</InvariantGlobalization>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
</Project>
"#;
    const CSHARP_WEB_PROJECT: &str = r#"<Project Sdk="Microsoft.NET.Sdk.WebAssembly">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
    <OutputType>Exe</OutputType>
    <AssemblyName>TachyonServices</AssemblyName>
    <Nullable>disable</Nullable>
    <InvariantGlobalization>true</InvariantGlobalization>
    <PublishTrimmed>true</PublishTrimmed>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <CompressionEnabled>false</CompressionEnabled>
    <BlazorCacheBootResources>false</BlazorCacheBootResources>
    <EnableDefaultCompileItems>false</EnableDefaultCompileItems>
  </PropertyGroup>
  <ItemGroup><Compile Include="TachyonServices.cs" /></ItemGroup>
</Project>
"#;
    const CSHARP_WEB_DEVELOPMENT_SETTINGS: &str = "    <WasmBuildNative>false</WasmBuildNative>\n";
    const CSHARP_WEB_INVARIANT_OPTION: &str = "  <ItemGroup><RuntimeHostConfigurationOption \
        Include=\"System.Globalization.Invariant\" Value=\"true\" Trim=\"true\" /></ItemGroup>\n";

    fn csharp_web_project(profile: crate::BuildProfile, smallest: &DotnetWebRelease) -> String {
        let release = Self::CSHARP_WEB_PROJECT;
        match profile {
            crate::BuildProfile::Release => smallest.project(release),
            crate::BuildProfile::Development => release
                .replace(
                    "    <InvariantGlobalization>true</InvariantGlobalization>\n",
                    Self::CSHARP_WEB_DEVELOPMENT_SETTINGS,
                )
                .replace(
                    "  <ItemGroup><Compile",
                    &format!("{}  <ItemGroup><Compile", Self::CSHARP_WEB_INVARIANT_OPTION),
                ),
        }
    }

    const CSHARP_WEB_ENTRY: &str = "public static partial class TacWeb\n{\n    \
    [JSExport]\n    public static string Invoke(string request)\n    {\n        \
    Tac.WebEmit ??= Emit;\n        TacHttpTransport.Start ??= HttpStart;\n        TacHttpTransport.Answer ??= HttpAnswer;\n        TacHttpTransport.Cancel ??= HttpCancel;\n        TacWebHost.Ask ??= Web;\n        return Tac.Invoke(request);\n    }\n\n    \
    [JSExport]\n    public static System.Threading.Tasks.Task<string> InvokeAsync(string request) => Tac.InvokeAsync(request);\n\n    \
    [JSImport(\"emit\", \"tachyon\")]\n    internal static partial void Emit(string payload);\n\n    \
    [JSImport(\"storage\", \"tachyon\")]\n    internal static partial string Storage(string request);\n\n    \
    [JSImport(\"web\", \"tachyon\")]\n    internal static partial string Web(string request);\n\n    \
    [JSImport(\"httpStart\", \"tachyon\")]\n    internal static partial string HttpStart(string request);\n\n    \
    [JSImport(\"httpAnswer\", \"tachyon\")]\n    [return: JSMarshalAs<JSType.Promise<JSType.String>>]\n    internal static partial System.Threading.Tasks.Task<string> HttpAnswer(string ticket);\n\n    \
    [JSImport(\"httpCancel\", \"tachyon\")]\n    internal static partial void HttpCancel(string ticket);\n\n    \
    public static void Main() { }\n}\n";
}

impl WindowsHostGenerator {
    pub(super) fn companion_project(architecture: WindowsArchitecture) -> String {
        Self::CSHARP_COMPANION_PROJECT
            .replace("__RUNTIME_IDENTIFIER__", architecture.dotnet_runtime())
    }

    pub(super) fn published_companion(
        project: &Path,
        architecture: WindowsArchitecture,
    ) -> PathBuf {
        project
            .join("bin/Release/net9.0")
            .join(architecture.dotnet_runtime())
            .join("publish/TachyonCompanion.dll")
    }

    fn require_compiler_for(architecture: WindowsArchitecture) -> Result<(), Failure> {
        let variable = Self::DEVELOPER_TARGET;
        let chosen = std::env::var(variable).unwrap_or_else(|_| String::from(architecture.name()));
        chosen
            .eq_ignore_ascii_case(architecture.name())
            .then_some(())
            .ok_or_else(|| {
                native_tool_failure(
                    1605,
                    &format!(
                        "This developer prompt builds for {chosen} ({variable}), but the Windows host is \
                         being built for {}. Open a developer prompt with {}, or pass --arch {chosen}.",
                        architecture.name(),
                        architecture.developer_prompt()
                    ),
                )
            })
    }
}

#[cfg(test)]
pub(super) const fn companion_prelude() -> &'static str {
    WindowsHostGenerator::CSHARP_COMPANION_PRELUDE
}

async fn icon_resource(
    application: &NativeApplication,
    web_bundle: &Path,
    stage: &Path,
) -> Result<Option<String>, Failure> {
    let Some(compiled) = stage_icon(application, web_bundle, stage).await? else {
        return Ok(None);
    };
    Ok(Some(compiler_path(&compiled, "Icon resource")?))
}

async fn compile_rust_companion(
    application: &NativeApplication,
    rust_companion: Option<&Path>,
    bundle: &Path,
    architecture: WindowsArchitecture,
) -> Result<(), Failure> {
    let Some(source) = rust_companion else {
        return Ok(());
    };
    let bin = bundle.join("bin");
    super::rust::RustCompanion::compile_companion(
        application,
        source,
        super::rust::Linkage::Shared,
        Some(architecture.rust_target()),
        &bin.join("TachyonRustCompanion.dll"),
        &bin,
    )
    .await?;
    Ok(())
}

fn stage_companion(
    companions: &[super::registry::NativeCompanionInput],
    stage: &Path,
    application_id: &str,
    architecture: WindowsArchitecture,
) -> Result<Option<PathBuf>, Failure> {
    let Some(authored) =
        super::registry::source(companions, crate::project::NativeCompanion::CSharp)?
    else {
        return Ok(None);
    };
    let project = stage.join("project").join("companion");
    let identifier = serde_json::to_string(application_id).map_err(|_| {
        native_tool_failure(1605, "Cannot encode the companion application identifier.")
    })?;
    let application = format!(
        "internal static class TacApplication {{ internal const string Id = {identifier}; }}\n"
    );
    let using = WindowsHostGenerator::CSHARP_COMPANION_USING;
    let prelude = WindowsHostGenerator::CSHARP_COMPANION_PRELUDE;
    let web_apis = CsharpWebApis::native();
    write(
        &project.join("TachyonCompanion.cs"),
        format!("{using}\n{application}\n{authored}\n{prelude}{web_apis}").as_bytes(),
    )?;
    write(
        &project.join("TachyonCompanion.csproj"),
        WindowsHostGenerator::companion_project(architecture).as_bytes(),
    )?;
    Ok(Some(project))
}

pub(crate) async fn web_dotnet(
    snapshot_root: &Path,
    services: &[(String, String, PathBuf)],
    profile: crate::BuildProfile,
) -> Result<(Vec<(String, Vec<u8>)>, Option<String>), Failure> {
    let companions = web_companion_inputs(snapshot_root, services);
    let Some(authored) =
        super::registry::source(&companions, crate::project::NativeCompanion::CSharp)?
    else {
        return Ok((Vec::new(), None));
    };
    let (authored, fsharp) = crate::fsharp_source::FsharpSource::web_split(&authored);
    let (fsharp, runtime) = crate::fsharp_source::FsharpSource::runtime_split(fsharp);
    let fsharp_files =
        super::fsharp_web_project::FsharpWebProject::files(snapshot_root, fsharp, runtime)?;
    let source = web_source(authored, fsharp, runtime);
    let version = run_tool("dotnet", &["--version"]).await.map_err(|_| {
        native_tool_failure(
            1605,
            "A C# web service needs the .NET SDK with the wasm-tools workload: install .NET 9 \
             or later, then run `dotnet workload install wasm-tools`.",
        )
    })?;
    let smallest = DotnetWebRelease::read(snapshot_root);
    let project = super::fsharp_web_project::FsharpWebProject::referenced(
        &WindowsHostGenerator::csharp_web_project(profile, &smallest),
        fsharp,
    );
    let mut keyed_source = source.clone();
    for (_, contents) in &fsharp_files {
        keyed_source.push_str(contents);
    }
    let key = web_cache_key(&version, &project, &keyed_source);
    let cached = crate::cache::root().join("dotnet-web").join(key);
    if !cached.is_dir() {
        publish_web_framework(&project, &source, &fsharp_files, &cached).await?;
    }
    let note = (profile == crate::BuildProfile::Release).then(|| smallest.note());
    Ok((read_web_framework(&cached)?, note))
}

fn web_companion_inputs(
    snapshot_root: &Path,
    services: &[(String, String, PathBuf)],
) -> Vec<super::registry::NativeCompanionInput> {
    let mut companions = Vec::new();
    for (service, class, source) in services {
        companions.push(super::registry::NativeCompanionInput {
            snapshot_root: snapshot_root.to_path_buf(),
            language: crate::project::NativeCompanion::CSharp,
            source: source.clone(),
            service: service.clone(),
            class: class.clone(),
            web_assembly: true,
            application_provider: false,
        });
    }
    companions
}

fn web_source(authored: &str, fsharp: &str, runtime: &str) -> String {
    use std::fmt::Write as _;
    let full_prelude = WindowsHostGenerator::CSHARP_COMPANION_PRELUDE;
    let prelude = full_prelude
        .split("// __TACHYON_NATIVE_ABI__")
        .next()
        .unwrap_or(full_prelude);
    let using = WindowsHostGenerator::CSHARP_COMPANION_USING;
    let mut storage_hosts = String::new();
    for (index, namespace) in authored
        .split("namespace TachyonService")
        .skip(1)
        .enumerate()
    {
        if namespace.contains("public static class BehaviorStorage") {
            let _ = write!(
                storage_hosts,
                "\n        TachyonService{index}.BehaviorStorage.Host ??= Storage;"
            );
        }
    }
    let entry = WindowsHostGenerator::CSHARP_WEB_ENTRY.replace(
        "Tac.WebEmit ??= Emit;",
        &format!(
            "Tac.WebEmit ??= Emit;{storage_hosts}{}",
            CsharpBrowserBindings::read_with_runtime(authored, runtime)
        ),
    );
    format!(
        "{using}using System.Runtime.InteropServices.JavaScript;\n\n\
         internal static class TacApplication {{ internal const string Id = \"web\"; }}\n\n\
         {authored}\n{prelude}\n{}\n{}{entry}",
        WindowsHostGenerator::CSHARP_BROWSER_TASKS,
        CsharpWebApis::in_web_services(fsharp)
    )
}

fn web_cache_key(version: &str, project: &str, source: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let mut hasher = Sha256::new();
    for part in [version, project, source] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut key = String::new();
    for byte in &digest {
        let _ = write!(key, "{byte:02x}");
    }
    key
}

async fn publish_web_framework(
    project: &str,
    source: &str,
    extra_files: &[(String, String)],
    cached: &Path,
) -> Result<(), Failure> {
    let staged = tempfile::Builder::new()
        .prefix(".tachyon-dotnet-web-")
        .tempdir()
        .map_err(|error| {
            native_tool_failure(1605, &format!("Cannot stage C# services: {error}"))
        })?;
    write(&staged.path().join("TachyonServices.cs"), source.as_bytes())?;
    write(
        &staged.path().join("TachyonServices.csproj"),
        project.as_bytes(),
    )?;
    for (relative, contents) in extra_files {
        write(&staged.path().join(relative), contents.as_bytes())?;
    }
    super::host::run_tool_in(
        "dotnet",
        &["publish", "-c", "Release", "--nologo", "-o", "out"],
        Some(staged.path()),
    )
    .await?;
    let framework = staged.path().join("out/wwwroot/_framework");
    let partial = cached.with_extension("partial");
    let _ = std::fs::remove_dir_all(&partial);
    super::host::native_io(std::fs::create_dir_all(&partial), &partial)?;
    for entry in super::host::native_io(std::fs::read_dir(&framework), &framework)?.flatten() {
        let path = entry.path();
        if path.is_file() {
            let target = partial.join(entry.file_name());
            super::host::native_io(std::fs::copy(&path, &target), &target)?;
        }
    }
    super::host::native_io(std::fs::rename(&partial, cached), cached)
}

fn read_web_framework(cached: &Path) -> Result<Vec<(String, Vec<u8>)>, Failure> {
    let mut files = Vec::new();
    for entry in super::host::native_io(std::fs::read_dir(cached), cached)?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        files.push((
            format!("_framework/{name}"),
            super::host::native_io(std::fs::read(&path), &path)?,
        ));
    }
    files.sort();
    Ok(files)
}

async fn publish_companion(
    project: &Path,
    bin: &Path,
    architecture: WindowsArchitecture,
) -> Result<(), Failure> {
    if run_tool("dotnet", &["--version"]).await.is_err() {
        return Err(native_tool_failure(
            1605,
            "A C# companion needs the .NET SDK on the build machine. Install .NET 9 \
             or later, which is what publishes the companion ahead of time.",
        ));
    }
    let directory = project
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Companion project path is not valid Unicode."))?;
    super::host::run_tool_in(
        "dotnet",
        &["publish", "-c", "Release", "--nologo"],
        Some(Path::new(directory)),
    )
    .await?;
    let built = WindowsHostGenerator::published_companion(project, architecture);
    let published = bin.join("TachyonCompanion.dll");
    super::host::native_io(std::fs::copy(&built, &published), &published)?;
    Ok(())
}

const fn windows_compiler() -> &'static str {
    "cl"
}

async fn stage_icon(
    application: &NativeApplication,
    web_bundle: &Path,
    stage: &Path,
) -> Result<Option<PathBuf>, Failure> {
    let Some(source) = application.icon_within(256) else {
        return Ok(None);
    };
    let origin = web_bundle.join(source.trim_start_matches('/'));
    let Ok(png) = std::fs::read(&origin) else {
        return Ok(None);
    };
    let Ok(length) = u32::try_from(png.len()) else {
        return Ok(None);
    };
    let icon = stage.join("icon.ico");
    super::host::write(&icon, &ico_bytes(&png, length))?;
    let script = stage.join("icon.rc");
    super::host::write(&script, b"1 ICON \"icon.ico\"\n")?;

    let compiled = stage.join("icon.res");
    run_tool(
        "rc.exe",
        &[
            "/nologo",
            "/fo",
            &compiler_path(compiled.as_path(), "Icon resource")?,
            &compiler_path(script.as_path(), "Icon script")?,
        ],
    )
    .await?;
    Ok(Some(compiled))
}

fn ico_bytes(png: &[u8], length: u32) -> Vec<u8> {
    let mut ico = Vec::with_capacity(png.len() + 22);
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.push(0);
    ico.push(0);
    ico.push(0);
    ico.push(0);
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&length.to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(png);
    ico
}

async fn compile_c(
    source: &Path,
    executable: &Path,
    resource: Option<&str>,
    architecture: WindowsArchitecture,
) -> Result<String, Failure> {
    if !cfg!(target_os = "windows") {
        return Err(native_tool_failure(
            1605,
            "The Windows host requires a Windows build machine with MSVC and the WebView2 SDK.",
        ));
    }
    if let Some(parent) = executable.parent() {
        super::host::native_io(std::fs::create_dir_all(parent), parent)?;
    }
    let sdk = webview2_sdk()?;
    let compiler = windows_compiler();
    let version = first_line(&run_tool(compiler, &["/?"]).await?, "MSVC unknown");
    WindowsHostGenerator::require_compiler_for(architecture)?;
    let plan = WindowsCompilerPlan {
        sdk: &sdk,
        source: &compiler_path(source, "Host source")?,
        executable: &compiler_path(executable, "Application")?,
        resource,
        architecture,
    };
    let arguments = plan.arguments(WindowsHostGenerator::LINKER_ARGUMENTS);
    let borrowed = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    run_tool(compiler, &borrowed).await?;
    Ok(version)
}

pub(super) fn object_directory(executable: &str) -> String {
    let directory = match executable.rsplit_once('\\') {
        Some((directory, _)) => Path::new(directory),
        None => Path::new("."),
    };
    compiler_path(directory, "Object directory").unwrap_or_else(|_| String::from("."))
}

fn webview2_sdk() -> Result<PathBuf, Failure> {
    let variable = WindowsHostGenerator::WEBVIEW2_SDK;
    let Some(path) = std::env::var_os(variable).map(PathBuf::from) else {
        return Err(native_tool_failure(
            1605,
            &format!(
                "The Windows host needs the WebView2 SDK. Restore Microsoft.Web.WebView2 \
                 and set {variable} to the extracted package directory."
            ),
        ));
    };
    if path.join("build/native/include/WebView2.h").is_file() {
        return Ok(path);
    }
    Err(native_tool_failure(
        1605,
        &format!(
            "{variable} does not contain build/native/include/WebView2.h. \
             Point it at an extracted Microsoft.Web.WebView2 package."
        ),
    ))
}

fn compiler_path(path: &Path, label: &str) -> Result<String, Failure> {
    let value = path
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, &format!("{label} path is not valid Unicode.")))?;
    if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
        Ok(format!(r"\\{value}"))
    } else {
        Ok(String::from(value.strip_prefix(r"\\?\").unwrap_or(value)))
    }
}

fn application_manifest(application: &NativeApplication) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="{identifier}" version="1.0.0.0"/>
  <dependency><dependentAssembly><assemblyIdentity
    type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
    processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#,
        identifier = application.application_id.replace('.', "-"),
    )
}

pub(super) fn c_source(
    application: &NativeApplication,
    index: &NativeRouteIndex,
    companions: &[super::registry::NativeCompanionInput],
) -> String {
    let local_bundle = super::routes::c_local_bundle(index, companions, "https://tachyon.local");
    let service_companions = super::service_companions::ServiceCompanions::c_table(companions);
    super::desktop_host_source::DesktopHostSource {
        template: WindowsHostGenerator::C_HOST.replace(
            "__WINDOW_CONTROLS__",
            &super::native_window_controls::NativeWindowControls::windows(&application.window),
        ),
        menus: include_str!("windows_menus.c"),
        mnemonic: WindowsHostGenerator::MENU_MNEMONIC,
    }
    .render(
        application,
        index,
        &[
            ("__LOCAL_BUNDLE_HELPERS__", &local_bundle),
            ("__SERVICE_COMPANIONS__", &service_companions),
        ],
    )
}

#[cfg(test)]
pub(super) const fn host_source() -> &'static str {
    WindowsHostGenerator::C_HOST
}

impl WindowsHostGenerator {
    const C_HOST: &str = r#"#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#define WIN32_LEAN_AND_MEAN
#define COBJMACROS

#include <windows.h>

#include <objbase.h>
#include <shellapi.h>
#include <shlwapi.h>
#include <ctype.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <WebView2.h>

__LOCAL_BUNDLE_HELPERS__

__SERVICE_COMPANIONS__

#define TACHYON_APP_NAME "__APP_NAME__"
#define TACHYON_BUNDLE_ID "__BUNDLE_ID__"
#define TACHYON_ENTRY_ROUTE "__ENTRY_ROUTE__"
#define TACHYON_SEPARATOR L'\x1f'
#define TACHYON_WM_PUBLISH (WM_APP + 1)

static const char *TACHYON_NATIVE_SHIM = "__NATIVE_SHIM__";

static ICoreWebView2 *g_webview = NULL;
static ICoreWebView2Controller *g_controller = NULL;
static ICoreWebView2Environment *g_environment = NULL;
static HWND g_window = NULL;
static PVOID volatile g_publish_window = NULL;
static const char *g_route = TACHYON_ENTRY_ROUTE;
static wchar_t g_resource_root[MAX_PATH];
static LONG g_pending_publishes = 0;
static BOOL g_application_stopped = FALSE;

typedef const char *(*TachyonCompanionInvoke)(const char *);
typedef void (*TachyonCompanionFree)(const char *);
typedef void (*TachyonCompanionEmit)(const char *);
typedef void (*TachyonCompanionSetEmit)(TachyonCompanionEmit);
typedef unsigned char *(*TachyonCapabilityCall)(const char *, const char *, size_t *);
typedef void (*TachyonCapabilityFree)(unsigned char *, size_t);
typedef void (*TachyonCompanionSetCapabilityAdapter)(TachyonCapabilityCall, TachyonCapabilityFree);
typedef bool (*TachyonApplicationStart)(void);
typedef void (*TachyonApplicationShutdown)(void);

static TachyonApplicationStart g_application_start = NULL;
static TachyonApplicationShutdown g_application_shutdown = NULL;

__WINDOW_CONTROLS__

static unsigned char *tachyon_native_capability(const char *capability, const char *payload, size_t *length) {
  if (length == NULL) return NULL;
  *length = 0;
  char *answer;
  if (capability == NULL || payload == NULL || strnlen(capability, 65) > 64 || strnlen(payload, 65537) > 65536) answer = _strdup("{\"status\":\"invalid\"}");
  else if (strncmp(capability, "window.", 7) == 0) answer = tachyon_window_control(capability, payload);
  else answer = _strdup("{\"status\":\"unsupported\"}");
  if (answer != NULL) *length = strlen(answer);
  return (unsigned char *)answer;
}

static void tachyon_native_capability_free(unsigned char *answer, size_t length) { (void)length; free(answer); }

static TachyonCompanionInvoke g_companion_invoke[3] = {NULL, NULL, NULL};
static TachyonCompanionFree g_companion_free[3] = {NULL, NULL, NULL};

static char *tachyon_narrow(const wchar_t *value);
static const TachyonLocalRoute *tachyon_current_route(void) {
  LPWSTR source = NULL;
  if (g_webview == NULL || FAILED(ICoreWebView2_get_Source(g_webview, &source)) || source == NULL) return NULL;
  char *uri = tachyon_narrow(source);
  char path[TACHYON_PATH_LIMIT];
  const TachyonLocalRoute *route = uri != NULL && tachyon_local_path(uri, path, sizeof(path)) ? tachyon_document_route(path) : NULL;
  free(uri); CoTaskMemFree(source);
  return route;
}

static wchar_t *tachyon_widen(const char *value);

static BOOL tachyon_menu_window_open(void) {
  return g_window != NULL && g_webview != NULL;
}

static BOOL tachyon_menu_page_shown(void) {
  return tachyon_current_route() != NULL;
}

static void tachyon_menu_open_route(const char *route) {
  wchar_t *wide = g_webview != NULL ? tachyon_widen(route) : NULL;
  if (wide == NULL) return;
  wchar_t document[TACHYON_PATH_LIMIT];
  if (_snwprintf_s(document, TACHYON_PATH_LIMIT, _TRUNCATE, L"https://tachyon.local%s%s", wide,
                   wide[wcslen(wide) - 1] != L'/' ? L"/" : L"") >= 0) ICoreWebView2_Navigate(g_webview, document);
  free(wide);
}

__NATIVE_MENUS__

static void tachyon_record(const char *event) {
  char directory[MAX_PATH];
  if (GetEnvironmentVariableA("LOCALAPPDATA", directory, MAX_PATH) == 0) {
    return;
  }
  char folder[MAX_PATH + 32];
  snprintf(folder, sizeof(folder), "%s\\Tachyon", directory);
  CreateDirectoryA(folder, NULL);
  char path[MAX_PATH + 128];
  snprintf(path, sizeof(path), "%s\\%s.jsonl", folder, TACHYON_BUNDLE_ID);
  FILE *handle = NULL;
  if (fopen_s(&handle, path, "a") != 0 || handle == NULL) {
    return;
  }
  fprintf(handle, "{\"event\":\"%s\",\"route\":\"%s\"}\n", event, g_route);
  fclose(handle);
}

static void tachyon_companion_emit(const char *payload) {
  HWND window = (HWND)InterlockedCompareExchangePointer(&g_publish_window, NULL, NULL);
  if (payload == NULL || window == NULL || strnlen(payload, TACHYON_MESSAGE_LIMIT + 1) > TACHYON_MESSAGE_LIMIT) {
    return;
  }
  if (InterlockedIncrement(&g_pending_publishes) > 128) { InterlockedDecrement(&g_pending_publishes); return; }
  char *copy = _strdup(payload);
  if (copy == NULL) {
    InterlockedDecrement(&g_pending_publishes);
    return;
  }
  if (!PostMessageW(window, TACHYON_WM_PUBLISH, 0, (LPARAM)copy)) {
    free(copy);
    InterlockedDecrement(&g_pending_publishes);
  }
}

static HMODULE tachyon_companion_load_one(const wchar_t *filename, int language) {
  wchar_t path[MAX_PATH];
  DWORD length = GetModuleFileNameW(NULL, path, MAX_PATH);
  if (length == 0 || length >= MAX_PATH) {
    return NULL;
  }
  wchar_t *separator = wcsrchr(path, L'\\');
  if (separator == NULL) {
    return NULL;
  }
  separator[1] = L'\0';
  if (wcslen(path) + wcslen(filename) >= MAX_PATH) {
    return NULL;
  }
  wcscat_s(path, MAX_PATH, filename);
  HMODULE library = LoadLibraryExW(path, NULL, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
  if (library == NULL) {
    return NULL;
  }
  g_companion_invoke[language] = (TachyonCompanionInvoke)GetProcAddress(library, "tac_native_invoke");
  g_companion_free[language] = (TachyonCompanionFree)GetProcAddress(library, "tac_native_free");
  TachyonCompanionSetEmit set_emit =
      (TachyonCompanionSetEmit)GetProcAddress(library, "tac_native_set_emit");
  if (set_emit != NULL) {
    set_emit(tachyon_companion_emit);
  }
  TachyonCompanionSetCapabilityAdapter set_adapter =
      (TachyonCompanionSetCapabilityAdapter)GetProcAddress(library, "tac_native_set_capability_adapter");
  if (set_adapter != NULL) set_adapter(tachyon_native_capability, tachyon_native_capability_free);
  if (g_companion_invoke[language] != NULL) {
    tachyon_record("companion.loaded");
  }
  return library;
}

static void tachyon_companion_load(void) {
  HMODULE rust = tachyon_companion_load_one(L"TachyonRustCompanion.dll", 1);
  tachyon_companion_load_one(L"TachyonCompanion.dll", 2);
  if (rust == NULL) return;
  g_application_start = (TachyonApplicationStart)(void (*)(void))GetProcAddress(rust, "tac_native_application_start");
  g_application_shutdown = (TachyonApplicationShutdown)(void (*)(void))GetProcAddress(rust, "tac_native_application_shutdown");
  tachyon_menus_bind(rust);
}

static void tachyon_application_stop(void) {
  if (g_application_stopped) return;
  g_application_stopped = TRUE;
  if (g_application_shutdown != NULL) g_application_shutdown();
}

static void tachyon_companion_dispose(void) {
  for (int language = 1; language < 3; language += 1) {
    if (g_companion_invoke[language] == NULL) continue;
    const char *answer = g_companion_invoke[language]("{\"op\":\"host-dispose\"}");
    if (answer != NULL && g_companion_free[language] != NULL) g_companion_free[language](answer);
  }
}

static void tachyon_safe_name(const char *value, char *out, size_t size) {
  size_t written = 0;
  for (size_t index = 0; value[index] != '\0' && written + 1 < size && index < 64; index += 1) {
    char character = value[index];
    if ((character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
        (character >= '0' && character <= '9') || character == '.' || character == '_' ||
        character == '-') {
      out[written] = character;
      written += 1;
    }
  }
  if (written == 0) {
    snprintf(out, size, "unnamed");
    return;
  }
  out[written] = '\0';
}

static wchar_t *tachyon_widen(const char *value) {
  int length = MultiByteToWideChar(CP_UTF8, 0, value, -1, NULL, 0);
  if (length <= 0) {
    return NULL;
  }
  wchar_t *wide = (wchar_t *)calloc((size_t)length, sizeof(wchar_t));
  if (wide == NULL) {
    return NULL;
  }
  MultiByteToWideChar(CP_UTF8, 0, value, -1, wide, length);
  return wide;
}

static char *tachyon_narrow(const wchar_t *value) {
  int length = WideCharToMultiByte(CP_UTF8, 0, value, -1, NULL, 0, NULL, NULL);
  if (length <= 0) {
    return NULL;
  }
  char *narrow = (char *)calloc((size_t)length, sizeof(char));
  if (narrow == NULL) {
    return NULL;
  }
  WideCharToMultiByte(CP_UTF8, 0, value, -1, narrow, length, NULL, NULL);
  return narrow;
}

static char *tachyon_handle(const char *capability, const char *payload) {
  if (strncmp(capability, "window.", 7) == 0) return tachyon_window_control(capability, payload);
  if (strcmp(capability, "companion.invoke") == 0) {
    int operation = tachyon_payload_string_matches(payload, "op", "init")
      || tachyon_payload_string_matches(payload, "op", "get")
      || tachyon_payload_string_matches(payload, "op", "set")
      || tachyon_payload_string_matches(payload, "op", "call")
      || tachyon_payload_string_matches(payload, "op", "dispose");
    const TachyonLocalRoute *route = tachyon_current_route();
    if (!operation || route == NULL || !tachyon_payload_route_matches(payload, route->route)) return _strdup("{\"error\":\"Native route mismatch.\"}");
    if (route->language == 0) return _strdup(tachyon_payload_string_matches(payload, "op", "init")
      ? "{\"value\":{\"fields\":[],\"methods\":[]}}" : "{\"error\":\"No native companion for this route.\"}");
    int language = tachyon_service_companion(payload);
    if (language == 0) return _strdup("{\"error\":\"Unknown native service.\",\"code\":\"TY_NATIVE_SERVICE\"}");
    if (g_companion_invoke[language] == NULL) return _strdup("{\"error\":\"This application has no native companion.\"}");
    const char *answer = g_companion_invoke[language](payload);
    char *copy = _strdup(answer == NULL ? TACHYON_EMPTY_REPLY
      : strnlen(answer, TACHYON_MESSAGE_LIMIT + 1) <= TACHYON_MESSAGE_LIMIT ? answer : TACHYON_REPLY_TOO_LARGE);
    if (g_companion_free[language] != NULL && answer != NULL) g_companion_free[language](answer);
    return copy;
  }
  char safe[80];
  tachyon_safe_name(capability, safe, sizeof(safe));
  char *answer = (char *)calloc(256, sizeof(char));
  if (answer == NULL) {
    return _strdup("{\"ok\":false}");
  }
  snprintf(answer, 256,
           "{\"ok\":false,\"error\":\"windows host answers companion.invoke, not '%s'\"}", safe);
  return answer;
}

static void tachyon_on_message(const wchar_t *raw) {
  if (wcsnlen(raw, TACHYON_MESSAGE_LIMIT + 129) > TACHYON_MESSAGE_LIMIT + 128) return;
  char *message = tachyon_narrow(raw);
  if (message == NULL) {
    return;
  }
  char *separator = strchr(message, 0x1f);
  if (separator == NULL || (size_t)(separator - message) > 64) {
    free(message);
    return;
  }
  *separator = '\0';
  char *request_id = separator + 1, *payload = strchr(request_id, 0x1f);
  if (payload == NULL || payload == request_id || (size_t)(payload - request_id) > 64) { free(message); return; }
  *payload++ = '\0';
  for (const char *cursor = request_id; *cursor != '\0'; cursor++) {
    if (!((*cursor >= '0' && *cursor <= '9') || (*cursor >= 'a' && *cursor <= 'f') || *cursor == '-' || *cursor == '.')) { free(message); return; }
  }
  char *answer = strlen(payload) > TACHYON_MESSAGE_LIMIT ? _strdup(TACHYON_REQUEST_TOO_LARGE) : tachyon_handle(message, payload);
  size_t framed_size = strlen(request_id) + strlen(answer != NULL ? answer : "{}") + 2;
  char *framed = calloc(framed_size, 1);
  if (framed == NULL) { free(answer); free(message); return; }
  snprintf(framed, framed_size, "%s\x1f%s", request_id, answer != NULL ? answer : "{}");
  wchar_t *wide = tachyon_widen(framed);
  free(framed);
  if (wide != NULL) {
    ICoreWebView2_PostWebMessageAsString(g_webview, wide);
    free(wide);
  }
  free(answer);
  free(message);
}

static HRESULT tachyon_query_interface(void *self, REFIID iid, void **object, const wchar_t *supported) {
  if (object == NULL || iid == NULL) return E_POINTER;
  *object = NULL;
  wchar_t identifier[40];
  if (StringFromGUID2(iid, identifier, 40) == 0) return E_NOINTERFACE;
  if (_wcsicmp(identifier, supported) != 0 &&
      _wcsicmp(identifier, L"{00000000-0000-0000-c000-000000000046}") != 0) return E_NOINTERFACE;
  *object = self;
  return S_OK;
}

typedef struct {
  ICoreWebView2WebMessageReceivedEventHandlerVtbl *lpVtbl;
} TachyonMessageHandler;

static HRESULT STDMETHODCALLTYPE message_query(
    ICoreWebView2WebMessageReceivedEventHandler *self, REFIID riid, void **object) {
  return tachyon_query_interface(self, riid, object, L"{57213f19-00e6-49fa-8e07-898ea01ecbd2}");
}
static ULONG STDMETHODCALLTYPE message_add_ref(
    ICoreWebView2WebMessageReceivedEventHandler *self) {
  (void)self;
  return 1;
}
static ULONG STDMETHODCALLTYPE message_release(
    ICoreWebView2WebMessageReceivedEventHandler *self) {
  (void)self;
  return 1;
}
static HRESULT STDMETHODCALLTYPE message_invoke(
    ICoreWebView2WebMessageReceivedEventHandler *self, ICoreWebView2 *sender,
    ICoreWebView2WebMessageReceivedEventArgs *arguments) {
  (void)self;
  (void)sender;
  LPWSTR source = NULL, current = NULL;
  if (FAILED(ICoreWebView2WebMessageReceivedEventArgs_get_Source(arguments, &source)) || source == NULL) return S_OK;
  HRESULT located = ICoreWebView2_get_Source(g_webview, &current);
  BOOL trusted = SUCCEEDED(located) && current != NULL && wcscmp(source, current) == 0 && tachyon_current_route() != NULL;
  CoTaskMemFree(source); CoTaskMemFree(current);
  if (!trusted) return S_OK;
  LPWSTR message = NULL;
  if (SUCCEEDED(ICoreWebView2WebMessageReceivedEventArgs_TryGetWebMessageAsString(
          arguments, &message)) &&
      message != NULL) {
    tachyon_on_message(message);
    CoTaskMemFree(message);
  }
  return S_OK;
}

static ICoreWebView2WebMessageReceivedEventHandlerVtbl g_message_vtbl = {
    message_query, message_add_ref, message_release, message_invoke};
static TachyonMessageHandler g_message_handler = {&g_message_vtbl};

typedef struct { ICoreWebView2NavigationStartingEventHandlerVtbl *lpVtbl; int top_level; } TachyonNavigationHandler;
static HRESULT STDMETHODCALLTYPE navigation_query(ICoreWebView2NavigationStartingEventHandler *self, REFIID iid, void **object) { return tachyon_query_interface(self, iid, object, L"{9adbe429-f36d-432b-9ddc-f8881fbd76e3}"); }
static ULONG STDMETHODCALLTYPE navigation_add_ref(ICoreWebView2NavigationStartingEventHandler *self) { (void)self; return 1; }
static ULONG STDMETHODCALLTYPE navigation_release(ICoreWebView2NavigationStartingEventHandler *self) { (void)self; return 1; }
static HRESULT STDMETHODCALLTYPE navigation_invoke(ICoreWebView2NavigationStartingEventHandler *self, ICoreWebView2 *sender, ICoreWebView2NavigationStartingEventArgs *arguments) {
  LPWSTR uri = NULL;
  if (FAILED(ICoreWebView2NavigationStartingEventArgs_get_Uri(arguments, &uri)) || uri == NULL) return E_FAIL;
  char *text = tachyon_narrow(uri), path[TACHYON_PATH_LIMIT];
  const TachyonLocalRoute *route = text != NULL && tachyon_local_path(text, path, sizeof(path)) ? tachyon_document_route(path) : NULL;
  if (route == NULL) ICoreWebView2NavigationStartingEventArgs_put_Cancel(arguments, TRUE);
  else if (((TachyonNavigationHandler *)self)->top_level) {
    g_route = route->route;
    if (path[strlen(path) - 1] != '/' && strcmp(path + 1, route->document) != 0) {
      size_t boundary = wcscspn(uri, L"?#"), length = wcslen(uri);
      wchar_t *normalized = calloc(length + 2, sizeof(wchar_t));
      if (normalized != NULL) {
        memcpy(normalized, uri, boundary * sizeof(wchar_t)); normalized[boundary] = L'/';
        memcpy(normalized + boundary + 1, uri + boundary, (length - boundary + 1) * sizeof(wchar_t));
        ICoreWebView2NavigationStartingEventArgs_put_Cancel(arguments, TRUE);
        ICoreWebView2_Navigate(sender, normalized); free(normalized);
      }
    } else tachyon_companion_dispose();
  }
  free(text); CoTaskMemFree(uri); return S_OK;
}
static ICoreWebView2NavigationStartingEventHandlerVtbl g_navigation_vtbl = {navigation_query, navigation_add_ref, navigation_release, navigation_invoke};
static TachyonNavigationHandler g_navigation_handler = {&g_navigation_vtbl, 1};
static TachyonNavigationHandler g_frame_navigation_handler = {&g_navigation_vtbl, 0};

typedef struct { ICoreWebView2NewWindowRequestedEventHandlerVtbl *lpVtbl; } TachyonNewWindowHandler;
static HRESULT STDMETHODCALLTYPE new_window_query(ICoreWebView2NewWindowRequestedEventHandler *self, REFIID iid, void **object) { return tachyon_query_interface(self, iid, object, L"{d4c185fe-c81c-4989-97af-2d3fa7ab5651}"); }
static ULONG STDMETHODCALLTYPE new_window_add_ref(ICoreWebView2NewWindowRequestedEventHandler *self) { (void)self; return 1; }
static ULONG STDMETHODCALLTYPE new_window_release(ICoreWebView2NewWindowRequestedEventHandler *self) { (void)self; return 1; }
static HRESULT STDMETHODCALLTYPE new_window_invoke(ICoreWebView2NewWindowRequestedEventHandler *self, ICoreWebView2 *sender, ICoreWebView2NewWindowRequestedEventArgs *arguments) {
  (void)self; (void)sender; ICoreWebView2NewWindowRequestedEventArgs_put_Handled(arguments, TRUE); return S_OK;
}
static ICoreWebView2NewWindowRequestedEventHandlerVtbl g_new_window_vtbl = {new_window_query, new_window_add_ref, new_window_release, new_window_invoke};
static TachyonNewWindowHandler g_new_window_handler = {&g_new_window_vtbl};

static IStream *tachyon_resource_stream(const char *relative) {
  wchar_t *wide = tachyon_widen(relative);
  if (wide == NULL) return NULL;
  wchar_t filename[TACHYON_PATH_LIMIT];
  int written = _snwprintf_s(filename, TACHYON_PATH_LIMIT, _TRUNCATE, L"%s\\%s", g_resource_root, wide);
  free(wide);
  if (written < 0) return NULL;
  for (wchar_t *cursor = filename + wcslen(g_resource_root) + 1; *cursor != L'\0'; cursor++) {
    if (*cursor != L'/' && *cursor != L'\\') continue;
    wchar_t separator = *cursor; *cursor = L'\0';
    DWORD ancestor = GetFileAttributesW(filename); *cursor = separator;
    if (ancestor == INVALID_FILE_ATTRIBUTES || (ancestor & FILE_ATTRIBUTE_REPARSE_POINT) != 0) return NULL;
  }
  DWORD attributes = GetFileAttributesW(filename);
  if (attributes == INVALID_FILE_ATTRIBUTES || (attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)) != 0) return NULL;
  IStream *stream = NULL;
  if (FAILED(SHCreateStreamOnFileEx(filename, STGM_READ | STGM_SHARE_DENY_WRITE, FILE_ATTRIBUTE_NORMAL, FALSE, NULL, &stream))) return NULL;
  STATSTG metadata;
  if (FAILED(IStream_Stat(stream, &metadata, STATFLAG_NONAME)) || metadata.cbSize.QuadPart > 16 * 1024 * 1024) { IStream_Release(stream); return NULL; }
  return stream;
}

static const wchar_t *tachyon_content_type(const char *path) {
  const char *extension = strrchr(path, '.');
  if (extension == NULL) return L"application/octet-stream";
  if (strcmp(extension, ".html") == 0) return L"text/html; charset=utf-8";
  if (strcmp(extension, ".js") == 0 || strcmp(extension, ".mjs") == 0) return L"text/javascript; charset=utf-8";
  if (strcmp(extension, ".css") == 0) return L"text/css; charset=utf-8";
  if (strcmp(extension, ".json") == 0) return L"application/json";
  if (strcmp(extension, ".wasm") == 0) return L"application/wasm";
  if (strcmp(extension, ".svg") == 0) return L"image/svg+xml";
  if (strcmp(extension, ".png") == 0) return L"image/png";
  if (strcmp(extension, ".jpg") == 0 || strcmp(extension, ".jpeg") == 0) return L"image/jpeg";
  if (strcmp(extension, ".woff2") == 0) return L"font/woff2";
  return L"application/octet-stream";
}

typedef struct { ICoreWebView2WebResourceRequestedEventHandlerVtbl *lpVtbl; } TachyonResourceHandler;
static HRESULT STDMETHODCALLTYPE resource_query(ICoreWebView2WebResourceRequestedEventHandler *self, REFIID iid, void **object) { return tachyon_query_interface(self, iid, object, L"{ab00b74c-15f1-4646-80e8-e76341d25d71}"); }
static ULONG STDMETHODCALLTYPE resource_add_ref(ICoreWebView2WebResourceRequestedEventHandler *self) { (void)self; return 1; }
static ULONG STDMETHODCALLTYPE resource_release(ICoreWebView2WebResourceRequestedEventHandler *self) { (void)self; return 1; }
static HRESULT STDMETHODCALLTYPE resource_invoke(ICoreWebView2WebResourceRequestedEventHandler *self, ICoreWebView2 *sender, ICoreWebView2WebResourceRequestedEventArgs *arguments) {
  (void)self; (void)sender;
  ICoreWebView2WebResourceRequest *request = NULL;
  LPWSTR uri = NULL;
  if (FAILED(ICoreWebView2WebResourceRequestedEventArgs_get_Request(arguments, &request)) || request == NULL) return E_FAIL;
  ICoreWebView2WebResourceRequest_get_Uri(request, &uri); ICoreWebView2WebResourceRequest_Release(request);
  char *text = uri == NULL ? NULL : tachyon_narrow(uri), path[TACHYON_PATH_LIMIT], relative[TACHYON_PATH_LIMIT];
  IStream *stream = NULL;
  if (text != NULL && tachyon_local_path(text, path, sizeof(path))) {
    strcpy_s(relative, sizeof(relative), path + 1);
    stream = tachyon_resource_stream(relative);
    if (stream == NULL && tachyon_bundle_path(path, relative, sizeof(relative))) stream = tachyon_resource_stream(relative);
  }
  wchar_t headers[1024];
  _snwprintf_s(headers, 1024, _TRUNCATE,
    L"Content-Type: %s\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: __CONTENT_SECURITY_POLICY__\r\n",
    stream == NULL ? L"text/plain" : tachyon_content_type(relative));
  ICoreWebView2WebResourceResponse *response = NULL;
  if (SUCCEEDED(ICoreWebView2Environment_CreateWebResourceResponse(g_environment, stream, stream == NULL ? 404 : 200, stream == NULL ? L"Not Found" : L"OK", headers, &response)) && response != NULL) {
    ICoreWebView2WebResourceRequestedEventArgs_put_Response(arguments, response); ICoreWebView2WebResourceResponse_Release(response);
  }
  if (stream != NULL) IStream_Release(stream);
  free(text); CoTaskMemFree(uri); return S_OK;
}
static ICoreWebView2WebResourceRequestedEventHandlerVtbl g_resource_vtbl = {resource_query, resource_add_ref, resource_release, resource_invoke};
static TachyonResourceHandler g_resource_handler = {&g_resource_vtbl};

typedef struct { ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandlerVtbl *lpVtbl; } TachyonScriptHandler;
static HRESULT STDMETHODCALLTYPE script_query(ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler *self, REFIID iid, void **object) { return tachyon_query_interface(self, iid, object, L"{b99369f3-9b11-47b5-bc6f-8e7895fcea17}"); }
static ULONG STDMETHODCALLTYPE script_add_ref(ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler *self) { (void)self; return 1; }
static ULONG STDMETHODCALLTYPE script_release(ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler *self) { (void)self; return 1; }
static HRESULT STDMETHODCALLTYPE script_invoke(ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler *self, HRESULT result, LPCWSTR identifier) {
  (void)self; (void)identifier;
  if (FAILED(result) || g_webview == NULL) { tachyon_record("bridge.unavailable"); return S_OK; }
  wchar_t *entry = tachyon_widen(TACHYON_ENTRY_ROUTE);
  if (entry == NULL) return E_OUTOFMEMORY;
  wchar_t document[TACHYON_PATH_LIMIT];
  int written = _snwprintf_s(document, TACHYON_PATH_LIMIT, _TRUNCATE, L"https://tachyon.local%s%s",
                           entry, entry[wcslen(entry)-1] != L'/' ? L"/" : L"");
  free(entry);
  if (written < 0) return E_INVALIDARG;
  HRESULT navigation = ICoreWebView2_Navigate(g_webview, document);
  if (FAILED(navigation)) return navigation;
  tachyon_record("bridge.ready");
  tachyon_record("controller.mounted");
  tachyon_record("controller.active");
  return S_OK;
}
static ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandlerVtbl g_script_vtbl = {script_query, script_add_ref, script_release, script_invoke};
static TachyonScriptHandler g_script_handler = {&g_script_vtbl};

typedef struct { ICoreWebView2AcceleratorKeyPressedEventHandlerVtbl *lpVtbl; } TachyonAcceleratorHandler;
static HRESULT STDMETHODCALLTYPE accelerator_query(ICoreWebView2AcceleratorKeyPressedEventHandler *self, REFIID iid, void **object) { return tachyon_query_interface(self, iid, object, L"{b29c7e28-fa79-41a8-8e44-65811c76dcb2}"); }
static ULONG STDMETHODCALLTYPE accelerator_add_ref(ICoreWebView2AcceleratorKeyPressedEventHandler *self) { (void)self; return 1; }
static ULONG STDMETHODCALLTYPE accelerator_release(ICoreWebView2AcceleratorKeyPressedEventHandler *self) { (void)self; return 1; }
static HRESULT STDMETHODCALLTYPE accelerator_invoke(ICoreWebView2AcceleratorKeyPressedEventHandler *self, ICoreWebView2Controller *sender, ICoreWebView2AcceleratorKeyPressedEventArgs *arguments) {
  (void)self; (void)sender;
  COREWEBVIEW2_KEY_EVENT_KIND kind;
  UINT key;
  if (FAILED(ICoreWebView2AcceleratorKeyPressedEventArgs_get_KeyEventKind(arguments, &kind)) ||
      FAILED(ICoreWebView2AcceleratorKeyPressedEventArgs_get_VirtualKey(arguments, &key))) return S_OK;
  BOOL down = kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN || kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN;
  if (down && tachyon_menus_key(g_window, key)) ICoreWebView2AcceleratorKeyPressedEventArgs_put_Handled(arguments, TRUE);
  return S_OK;
}
static ICoreWebView2AcceleratorKeyPressedEventHandlerVtbl g_accelerator_vtbl = {accelerator_query, accelerator_add_ref, accelerator_release, accelerator_invoke};
static TachyonAcceleratorHandler g_accelerator_handler = {&g_accelerator_vtbl};

typedef struct {
  ICoreWebView2CreateCoreWebView2ControllerCompletedHandlerVtbl *lpVtbl;
} TachyonControllerHandler;

static HRESULT STDMETHODCALLTYPE controller_query(
    ICoreWebView2CreateCoreWebView2ControllerCompletedHandler *self, REFIID riid, void **object) {
  return tachyon_query_interface(self, riid, object, L"{6c4819f3-c9b7-4260-8127-c9f5bde7f68c}");
}
static ULONG STDMETHODCALLTYPE controller_add_ref(
    ICoreWebView2CreateCoreWebView2ControllerCompletedHandler *self) {
  (void)self;
  return 1;
}
static ULONG STDMETHODCALLTYPE controller_release(
    ICoreWebView2CreateCoreWebView2ControllerCompletedHandler *self) {
  (void)self;
  return 1;
}
static HRESULT STDMETHODCALLTYPE controller_invoke(
    ICoreWebView2CreateCoreWebView2ControllerCompletedHandler *self, HRESULT result,
    ICoreWebView2Controller *controller) {
  (void)self;
  if (FAILED(result) || controller == NULL) {
    tachyon_record("bridge.unavailable");
    return S_OK;
  }
  g_controller = controller;
  ICoreWebView2Controller_AddRef(controller);
  ICoreWebView2Controller_get_CoreWebView2(controller, &g_webview);
  ICoreWebView2Controller_put_IsVisible(controller, TRUE);

  RECT bounds;
  GetClientRect(g_window, &bounds);
  ICoreWebView2Controller_put_Bounds(controller, bounds);

  EventRegistrationToken token;
  ICoreWebView2Controller_add_AcceleratorKeyPressed(
      controller, (ICoreWebView2AcceleratorKeyPressedEventHandler *)&g_accelerator_handler, &token);
  ICoreWebView2_add_WebMessageReceived(
      g_webview, (ICoreWebView2WebMessageReceivedEventHandler *)&g_message_handler, &token);
  ICoreWebView2_add_NavigationStarting(g_webview, (ICoreWebView2NavigationStartingEventHandler *)&g_navigation_handler, &token);
  ICoreWebView2_add_FrameNavigationStarting(g_webview, (ICoreWebView2NavigationStartingEventHandler *)&g_frame_navigation_handler, &token);
  ICoreWebView2_add_NewWindowRequested(g_webview, (ICoreWebView2NewWindowRequestedEventHandler *)&g_new_window_handler, &token);
  ICoreWebView2_AddWebResourceRequestedFilter(g_webview, L"https://tachyon.local/*", COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);
  ICoreWebView2_add_WebResourceRequested(g_webview, (ICoreWebView2WebResourceRequestedEventHandler *)&g_resource_handler, &token);

  DWORD module_length = GetModuleFileNameW(NULL, g_resource_root, MAX_PATH);
  if (module_length == 0 || module_length >= MAX_PATH) return E_FAIL;
  wchar_t *separator = wcsrchr(g_resource_root, L'\\');
  if (separator == NULL) return E_FAIL;
  *separator = L'\0';
  separator = wcsrchr(g_resource_root, L'\\');
  if (separator == NULL) return E_FAIL;
  *separator = L'\0';
  if (wcscat_s(g_resource_root, MAX_PATH, L"\\resources\\WebBundle") != 0) return E_FAIL;
  size_t script_size = strlen(TACHYON_NATIVE_SHIM) + 2048;
  char *script = calloc(script_size, 1);
  if (script == NULL) return E_OUTOFMEMORY;
  snprintf(script, script_size,
           "if(location.origin==='https://tachyon.local' && window===window.top){\n%s\n"
           "let chain=Promise.resolve(), pending=0, sequence=0;const session=crypto.randomUUID();\n"
           "window.chrome.webview.addEventListener('message',e=>{if(e.data && typeof e.data==='object')globalThis.__tachyonCompanionPublish(e.data)});\n"
           "globalThis.__tachyonHostPost=(capability,payload)=>{\n"
           " if(pending>=128 || capability.length>64 || payload.length>65536)return Promise.reject(new Error('Native message limit'));\n"
           " pending++;const id=session+'.'+(++sequence);const call=()=>new Promise((resolve,reject)=>{\n"
           " const once=e=>{if(typeof e.data!=='string'||!e.data.startsWith(id+'\\u001f'))return;clearTimeout(timer);window.chrome.webview.removeEventListener('message',once);resolve(e.data.slice(id.length+1))};\n"
           " const timer=setTimeout(()=>{window.chrome.webview.removeEventListener('message',once);reject(new Error('Native request timed out'))},10000);\n"
           " window.chrome.webview.addEventListener('message',once);window.chrome.webview.postMessage(capability+'\\u001f'+id+'\\u001f'+payload);});\n"
           " const result=chain.then(call,call).finally(()=>pending--);chain=result.catch(()=>{});return result;};\n"
           "globalThis.__tachyonNativeHostCall=globalThis.__tachyonHostPost;\n}\n",
           TACHYON_NATIVE_SHIM);
  wchar_t *wide_script = tachyon_widen(script);
  free(script);
  if (wide_script == NULL) return E_OUTOFMEMORY;
  HRESULT registration = ICoreWebView2_AddScriptToExecuteOnDocumentCreated(g_webview, wide_script,
    (ICoreWebView2AddScriptToExecuteOnDocumentCreatedCompletedHandler *)&g_script_handler);
  free(wide_script);
  return registration;
}

static ICoreWebView2CreateCoreWebView2ControllerCompletedHandlerVtbl g_controller_vtbl = {
    controller_query, controller_add_ref, controller_release, controller_invoke};
static TachyonControllerHandler g_controller_handler = {&g_controller_vtbl};

typedef struct {
  ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandlerVtbl *lpVtbl;
} TachyonEnvironmentHandler;

static HRESULT STDMETHODCALLTYPE environment_query(
    ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler *self, REFIID riid, void **object) {
  return tachyon_query_interface(self, riid, object, L"{4e8a3389-c9d8-4bd2-b6b5-124fee6cc14d}");
}
static ULONG STDMETHODCALLTYPE environment_add_ref(
    ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler *self) {
  (void)self;
  return 1;
}
static ULONG STDMETHODCALLTYPE environment_release(
    ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler *self) {
  (void)self;
  return 1;
}
static HRESULT STDMETHODCALLTYPE environment_invoke(
    ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler *self, HRESULT result,
    ICoreWebView2Environment *environment) {
  (void)self;
  if (FAILED(result) || environment == NULL) {
    tachyon_record("bridge.unavailable");
    return S_OK;
  }
  g_environment = environment;
  ICoreWebView2Environment_AddRef(environment);
  ICoreWebView2Environment_CreateCoreWebView2Controller(
      environment, g_window,
      (ICoreWebView2CreateCoreWebView2ControllerCompletedHandler *)&g_controller_handler);
  return S_OK;
}

static ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandlerVtbl g_environment_vtbl = {
    environment_query, environment_add_ref, environment_release, environment_invoke};
static TachyonEnvironmentHandler g_environment_handler = {&g_environment_vtbl};

static LRESULT CALLBACK tachyon_window_proc(HWND window, UINT message, WPARAM wparam,
                                            LPARAM lparam) {
  switch (message) {
    case WM_SIZE:
      if (g_controller != NULL) {
        RECT bounds;
        GetClientRect(window, &bounds);
        ICoreWebView2Controller_put_Bounds(g_controller, bounds);
      }
      return 0;
    case TACHYON_WM_PUBLISH: {
      char *payload = (char *)lparam;
      if (payload == NULL) {
        return 0;
      }
      if (tachyon_current_route() != NULL) {
        wchar_t *wide = tachyon_widen(payload);
        if (wide != NULL) { ICoreWebView2_PostWebMessageAsJson(g_webview, wide); free(wide); }
      }
      free(payload);
      InterlockedDecrement(&g_pending_publishes);
      return 0;
    }
    case WM_COMMAND:
      if (tachyon_menus_command(window, LOWORD(wparam))) return 0;
      break;
    case WM_INITMENUPOPUP:
      tachyon_menus_refresh((HMENU)wparam);
      return 0;
    case WM_ENDSESSION:
      if (wparam) { tachyon_companion_dispose(); tachyon_application_stop(); }
      return 0;
    case WM_DESTROY:
      tachyon_companion_dispose();
      InterlockedExchangePointer(&g_publish_window, NULL);
      tachyon_record("controller.destroyed");
      if (g_controller != NULL) { ICoreWebView2Controller_Close(g_controller); ICoreWebView2Controller_Release(g_controller); g_controller = NULL; }
      if (g_webview != NULL) { ICoreWebView2_Release(g_webview); g_webview = NULL; }
      if (g_environment != NULL) { ICoreWebView2Environment_Release(g_environment); g_environment = NULL; }
      g_window = NULL;
      PostQuitMessage(0);
      return 0;
    default:
      break;
  }
  return DefWindowProcW(window, message, wparam, lparam);
}

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE previous, PWSTR arguments, int show) {
  (void)previous;
  (void)arguments;
  if (FAILED(CoInitializeEx(NULL, COINIT_APARTMENTTHREADED))) return 1;
  tachyon_companion_load();
  if (g_application_start != NULL && !g_application_start()) {
    tachyon_record("application.start.failed");
    CoUninitialize();
    return 1;
  }
  tachyon_record("controller.created");

  WNDCLASSEXW window_class = {0};
  window_class.cbSize = sizeof(window_class);
  window_class.lpfnWndProc = tachyon_window_proc;
  window_class.hInstance = instance;
  window_class.lpszClassName = L"TachyonNativeHost";
  window_class.hCursor = LoadCursorW(NULL, (LPCWSTR)IDC_ARROW);
  RegisterClassExW(&window_class);

  wchar_t *title = tachyon_widen(TACHYON_APP_NAME);
  g_window = CreateWindowExW(0, L"TachyonNativeHost", title != NULL ? title : L"Tachyon",
                             WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT, 1024, 768, NULL,
                             NULL, instance, NULL);
  free(title);
  if (g_window == NULL) { tachyon_application_stop(); CoUninitialize(); return 1; }
  InterlockedExchangePointer(&g_publish_window, g_window);
  tachyon_menus_install(g_window);
  ShowWindow(g_window, show);

  CreateCoreWebView2EnvironmentWithOptions(
      NULL, NULL, NULL,
      (ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler *)&g_environment_handler);

  MSG message;
  BOOL message_result;
  while ((message_result = GetMessageW(&message, NULL, 0, 0)) > 0) {
    if (tachyon_menus_translate(g_window, &message)) continue;
    TranslateMessage(&message);
    DispatchMessageW(&message);
  }
  tachyon_application_stop();
  tachyon_menus_destroy();
  CoUninitialize();
  return message_result < 0 ? 1 : 0;
}
"#;
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{application_manifest, c_source, compiler_path, stage_companion, windows_compiler};
    use crate::native::config::NativeApplication;
    use std::path::Path;

    #[test]
    fn csharp_task_model_browser_source_retains_codec_without_native_host_tail() {
        let browser = super::web_source("", "", "");
        let native = super::companion_prelude();
        for source in [browser.as_str(), native] {
            assert!(source.contains("public static T ReadResult<T>"));
            assert!(source.contains("public static bool Fits("));
            assert!(source.contains("TacModelContext.Default.GetTypeInfo(typeof(T))"));
            assert_eq!(source.matches("public static T ReadResult<T>").count(), 1);
            assert_eq!(source.matches("public static bool Fits(").count(), 1);
        }
        assert!(!browser.contains("public static class TacNative"));
        assert!(!browser.contains("[UnmanagedCallersOnly"));
        assert_eq!(native.matches("// __TACHYON_NATIVE_ABI__").count(), 1);
        assert!(native.contains("EntryPoint = \"tac_native_invoke\""));
    }

    #[test]
    fn fsharp_browser_entry_binds_detached_http_runtime_without_native_abi() {
        let authored = "namespace TachyonService0 { class PlainService {} }";
        let runtime = "namespace TachyonService1 { internal static class BehaviorBrowserHttp {} }";
        let browser = super::web_source(authored, "namespace rec TachyonService1\n", runtime);
        assert!(
            browser.contains(
                "TachyonService1.BehaviorBrowserHttp.Capture ??= Tac.CaptureHttpTransport"
            )
        );
        assert!(browser.contains("TachyonService1.BehaviorError failure1 => failure1.Code"));
        assert!(!browser.contains("TachyonService0.BehaviorBrowserHttp"));
        assert!(!browser.contains("DllImport"));
    }

    #[test]
    fn dotnet_browser_tasks_have_a_distinct_export_and_keep_the_synchronous_entry() {
        let source = super::web_source("", "", "");
        assert!(source.contains("public static string Invoke(string request)"));
        assert!(source.contains(
            "public static System.Threading.Tasks.Task<string> InvokeAsync(string request)"
        ));
        assert!(source.contains("await Work.WaitAsync(Cancellation)"));
        assert!(source.contains("active != owner"));
        assert!(
            !super::companion_prelude()
                .contains("public static System.Threading.Tasks.Task<string> InvokeAsync(")
        );
    }

    #[test]
    fn every_csharp_web_namespace_with_the_behavior_runtime_gets_the_page_storage() {
        let authored = "namespace TachyonService0 {\npublic static class BehaviorStorage { }\n}\n\
             namespace TachyonService1 {\npublic class Plain { }\n}\n\
             namespace TachyonService2 {\npublic static class BehaviorStorage { }\n}\n";
        let source = super::web_source(authored, "", "");
        assert!(source.contains("TachyonService0.BehaviorStorage.Host ??= Storage;"));
        assert!(!source.contains("TachyonService1.BehaviorStorage"));
        assert!(source.contains("TachyonService2.BehaviorStorage.Host ??= Storage;"));
        assert!(!source.contains("        BehaviorStorage.Host"));
    }

    #[test]
    fn a_page_with_fsharp_services_takes_the_browser_apis_from_the_library_below_them() {
        let authored = "namespace TachyonService0 {\npublic class Plain { }\n}\n";
        let alone = super::web_source(authored, "", "");
        assert!(alone.contains("public static class TacWebHost"));
        assert!(alone.contains("public sealed record ShareFinished();"));
        let with_fsharp = super::web_source(authored, "namespace rec TachyonService1\n", "");
        assert!(!with_fsharp.contains("public static class TacWebHost"));
        assert!(!with_fsharp.contains("public sealed record ShareFinished();"));
        assert!(with_fsharp.contains("TacWebHost.Ask ??= Web;"));
    }

    #[test]
    fn the_development_web_project_skips_the_native_relink_and_stays_invariant() {
        let smallest = super::DotnetWebRelease::read(std::path::Path::new("/definitely/missing"));
        let release = super::WindowsHostGenerator::csharp_web_project(
            crate::BuildProfile::Release,
            &smallest,
        );
        let development = super::WindowsHostGenerator::csharp_web_project(
            crate::BuildProfile::Development,
            &smallest,
        );
        assert!(release.contains("<InvariantGlobalization>true</InvariantGlobalization>"));
        assert!(release.contains("<BlazorCacheBootResources>false</BlazorCacheBootResources>"));
        assert!(!release.contains("WasmBuildNative"));
        assert!(development.contains("<WasmBuildNative>false</WasmBuildNative>"));
        assert!(!development.contains("InvariantGlobalization"));
        assert!(development.contains(
            r#"<RuntimeHostConfigurationOption Include="System.Globalization.Invariant" Value="true" Trim="true" />"#
        ));
        assert!(development.contains("<PublishTrimmed>true</PublishTrimmed>"));
        assert!(release.contains("<EmccLinkOptimizationFlag>-Oz</EmccLinkOptimizationFlag>"));
        assert!(!development.contains("EmccLinkOptimizationFlag"));
        assert_ne!(
            super::web_cache_key("9.0", &release, "source"),
            super::web_cache_key("9.0", &development, "source")
        );
    }

    fn application() -> NativeApplication {
        NativeApplication {
            name: String::from("Native Catalog"),
            executable_name: String::from("NativeCatalog"),
            application_id: String::from("dev.tachyon.native-catalog"),
            version: String::from("1.0.0"),
            entry_route: String::from("/"),
            icons: Vec::new(),
            window: crate::native::config::WindowConfiguration::default(),
            native_capabilities: Vec::new(),
            menus: crate::native::native_menu_config::NativeMenuConfig::default(),
            rust: crate::native::native_rust_project::NativeRustProject::default(),
        }
    }

    fn index() -> crate::native::routes::NativeRouteIndex {
        crate::native::routes::NativeRouteIndex {
            contract_version: 2,
            entry_route: String::from("/"),
            entry_document: String::from("index.html"),
            routes: Vec::new(),
        }
    }

    #[test]
    fn an_application_name_that_looks_like_a_placeholder_stays_text() {
        let mut named = application();
        named.name = String::from("__BUNDLE_ID__ __NATIVE_SHIM__");
        let source = c_source(&named, &index(), &[]);
        assert!(source.contains(r#"#define TACHYON_APP_NAME "__BUNDLE_ID__ __NATIVE_SHIM__""#));
    }

    fn catalog_host_source() -> String {
        c_source(&application(), &index(), &[])
    }

    fn write_owned_card_service(
        snapshot_root: &Path,
    ) -> crate::native::registry::NativeCompanionInput {
        let source = snapshot_root.join("client/services/OwnedCardService.cs");
        std::fs::create_dir_all(source.parent().expect("companion parent"))
            .expect("service directory");
        std::fs::write(&source, owned_card_service_source()).expect("companion");
        crate::native::registry::NativeCompanionInput {
            snapshot_root: snapshot_root.to_path_buf(),
            language: crate::project::NativeCompanion::CSharp,
            source,
            service: String::from("ownedCard"),
            class: String::from("OwnedCardService"),
            web_assembly: false,
            application_provider: false,
        }
    }

    fn owned_card_service_source() -> &'static str {
        "[Service]\npublic class OwnedCardService {\npublic int Count = 0;\n}\n"
    }

    fn staged_companion_source(
        companions: &[crate::native::registry::NativeCompanionInput],
        output: &Path,
        identifier: &str,
    ) -> String {
        let project = stage_companion(
            companions,
            output,
            identifier,
            super::WindowsArchitecture::X64,
        )
        .expect("stage")
        .expect("C# project");
        std::fs::read_to_string(project.join("TachyonCompanion.cs")).expect("generated companion")
    }

    #[test]
    fn the_host_is_a_win32_window_around_one_webview() {
        let source = catalog_host_source();
        assert!(source.contains("#define COBJMACROS"));
        assert!(source.contains("CreateCoreWebView2EnvironmentWithOptions"));
        assert!(source.contains("ICoreWebView2_Navigate"));
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains("tac_native_invoke"));
        assert!(!source.contains("TACHYON_ITEMS"));
        assert!(!source.contains("tachyon_create_controls"));
        assert!(source.contains("ICoreWebView2WebMessageReceivedEventArgs_get_Source"));
        assert!(source.contains("tachyon_payload_route_matches"));
        assert!(source.contains("g_frame_navigation_handler = {&g_navigation_vtbl, 0}"));
        assert!(source.contains("else tachyon_companion_dispose()"));
        assert!(source.contains("if (!operation || route == NULL"));
        assert!(source.contains("int language = tachyon_service_companion(payload);"));
        assert!(!source.contains("TY_NATIVE_SERVICE\\\"\") != NULL"));
        assert!(source.contains("TachyonRustCompanion.dll"));
        assert!(source.contains("CoInitializeEx"));
        assert!(!source.contains("file:///"));
        assert!(!source.contains("char script[8192]"));
    }

    #[test]
    fn bundle_requests_reach_the_canonical_route_responder() {
        let source = catalog_host_source();
        assert!(!source.contains("SetVirtualHostNameToFolderMapping"));
        assert!(source.contains("ICoreWebView2_AddWebResourceRequestedFilter"));
        assert!(source.contains("ICoreWebView2_add_WebResourceRequested"));
        assert!(source.contains("tachyon_bundle_path(path, relative, sizeof(relative))"));
        assert!(source.contains("ICoreWebView2WebResourceRequestedEventArgs_put_Response"));
        assert!(source.contains("Content-Security-Policy: default-src 'self'"));
    }

    #[test]
    fn the_host_policy_allows_the_framework_inline_scripts_by_their_hash() {
        let source = catalog_host_source();
        let hash = format!(
            "{} {}",
            crate::compiler::SpeculationRules::script_source(),
            crate::compiler::ViewTransitionGuard::script_source()
        );
        assert!(
            source.contains(&format!("script-src 'self' 'wasm-unsafe-eval' {hash};")),
            "{source}"
        );
    }

    #[test]
    fn manifest_requests_common_controls_and_per_monitor_dpi() {
        let manifest = application_manifest(&application());
        assert!(manifest.contains("Microsoft.Windows.Common-Controls"));
        assert!(manifest.contains("PerMonitorV2"));
        assert!(manifest.contains("dev-tachyon-native-catalog"));
    }

    #[test]
    fn the_host_is_built_with_the_platform_toolchain() {
        assert_eq!(windows_compiler(), "cl");
    }

    #[test]
    fn csharp_local_storage_is_scoped_to_the_application_identifier() {
        let temporary_directory = tempfile::tempdir().expect("temporary");
        let companions = [write_owned_card_service(temporary_directory.path())];
        for identifier in ["dev.tachyon.first", "dev.tachyon.second"] {
            let output = temporary_directory.path().join(identifier);
            let generated = staged_companion_source(&companions, &output, identifier);
            assert!(generated.contains(&format!("Id = \"{identifier}\"")));
            assert!(generated.contains("\"Tachyon\", TacApplication.Id, \"store.txt\""));
        }
    }

    #[test]
    fn a_failing_csharp_companion_call_answers_a_fixed_error_without_the_exception_text() {
        let Some(version) = std::process::Command::new("dotnet")
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        else {
            eprintln!("skipping: dotnet is unavailable");
            return;
        };
        let major = version.split('.').next().expect("major version");
        let temporary_directory = tempfile::tempdir().expect("temporary");
        let source = temporary_directory
            .path()
            .join("client/services/LeakyService.cs");
        std::fs::create_dir_all(source.parent().expect("companion parent"))
            .expect("service directory");
        std::fs::write(
            &source,
            "[Service]\npublic class LeakyService {\npublic int Fail() { throw new System.InvalidOperationException(\"secret connection string\"); }\n}\n",
        )
        .expect("companion");
        let companions = [crate::native::registry::NativeCompanionInput {
            snapshot_root: temporary_directory.path().to_path_buf(),
            language: crate::project::NativeCompanion::CSharp,
            source,
            service: String::from("leaky"),
            class: String::from("LeakyService"),
            web_assembly: false,
            application_provider: false,
        }];
        let project = stage_companion(
            &companions,
            temporary_directory.path(),
            "dev.tachyon.leaky",
            super::WindowsArchitecture::X64,
        )
        .expect("stage")
        .expect("C# project");
        std::fs::write(
            project.join("TachyonCompanion.csproj"),
            format!(
                "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net{major}.0</TargetFramework><OutputType>Exe</OutputType><Nullable>disable</Nullable><AllowUnsafeBlocks>true</AllowUnsafeBlocks><InvariantGlobalization>true</InvariantGlobalization></PropertyGroup></Project>\n"
            ),
        )
        .expect("project file");
        std::fs::write(
            project.join("Program.cs"),
            "string Ask(string body) => Tac.Invoke(\"{\\\"route\\\":\\\"/\\\",\\\"owner\\\":\\\"00000000-0000-4000-8000-000000000001\\\",\\\"service\\\":\\\"leaky\\\",\" + body + \"}\");\n\
             var methods = System.Text.Json.Nodes.JsonNode.Parse(Ask(\"\\\"op\\\":\\\"init\\\"\"))[\"value\"][\"methods\"].AsArray();\n\
             System.Console.Write(Ask(\"\\\"op\\\":\\\"call\\\",\\\"args\\\":[],\\\"name\\\":\\\"\" + methods[0].GetValue<string>() + \"\\\"\"));\n",
        )
        .expect("program");
        let output = std::process::Command::new("dotnet")
            .arg("run")
            .current_dir(&project)
            .output()
            .expect("dotnet run");
        let printed = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{printed}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            printed.lines().last().expect("answer"),
            "{\"error\":\"Native companion operation failed.\"}"
        );
    }

    #[test]
    fn extended_length_prefixes_are_stripped_for_the_command_line() {
        assert_eq!(
            compiler_path(Path::new(r"\\?\C:\build\host.c"), "Host source").expect("path"),
            r"C:\build\host.c"
        );
    }
}
