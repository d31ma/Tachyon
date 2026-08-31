//! Windows Win32 host generation.
//!
//! A Win32 window around one `WebView2` showing the application's own bundle;
//! see `native/routes.rs` for why it is no longer a generated control table.
//! Building requires Windows with MSVC and the `WebView2` SDK, as the macOS
//! and Linux hosts require their own platforms. See ADR 0017.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, c_string_escape, first_line, native_tool_failure, run_tool, stage_application,
    write, write_host_source,
};
use super::routes::NativeRouteIndex;
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
        package: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle = stage.join(&application.executable_name);
        let resources = bundle.join("resources");
        stage_application(index, web_bundle, stage, &resources)?;

        // A C# companion is compiled *as C#* here rather than to WebAssembly,
        // which is the whole point: NativeAOT publishes it as a library the
        // host loads, so it reaches the .NET Windows surface.
        let companion = stage_companion(companions, stage, &application.application_id)?;
        let rust_companion = super::rust::stage(companions, stage, &application.application_id)?;
        let source_path = stage.join("project").join("tachyon_host.c");
        write_host_source(&source_path, &c_source(application, index, companions))?;
        write(
            &bundle.join("application.manifest"),
            application_manifest(application).as_bytes(),
        )?;
        if !package {
            return Ok(GeneratedHost {
                application_bundle: PathBuf::from("project/tachyon_host.c"),
                toolchain_name: String::from("source"),
                toolchain_version: String::from("not-packaged"),
            });
        }

        let executable = bundle
            .join("bin")
            .join(format!("{}.exe", application.executable_name));
        // The manifest's icon becomes the executable's, so the shell and the
        // taskbar show the same artwork the browser tab does.
        let resource = stage_icon(application, web_bundle, stage).await?;
        let resource = match resource.as_deref() {
            Some(path) => Some(compiler_path(path, "Icon resource")?),
            None => None,
        };
        let compiler_version = compile_c(&source_path, &executable, resource.as_deref()).await?;
        if let Some(project) = companion {
            publish_companion(&project, &bundle.join("bin")).await?;
        }
        // Each language keeps its own DLL; route selection must never replace
        // a C# companion with a Rust build or fall back to a different route.
        if let Some(source) = &rust_companion {
            super::rust::compile(
                source,
                super::rust::Linkage::Shared,
                Some("x86_64-pc-windows-msvc"),
                &bundle.join("bin").join("TachyonRustCompanion.dll"),
            )
            .await?;
        }
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(&application.executable_name),
            toolchain_name: String::from("msvc"),
            toolchain_version: compiler_version,
        })
    }
}

/// Environment variable naming an extracted `Microsoft.Web.WebView2` package.
const WEBVIEW2_SDK: &str = "TAC_WEBVIEW2_SDK";

/// The prelude appended to a C# companion compiled into this host.
const CSHARP_COMPANION_PRELUDE: &str = include_str!("prelude.cs");

/// The C# companion prelude, for the publish-channel drift test.
#[cfg(test)]
pub(super) const fn companion_prelude() -> &'static str {
    CSHARP_COMPANION_PRELUDE
}
/// Namespaces the prelude needs, which C# requires before any type.
/// `System.Linq` is the prelude's own: `TacStore` writes its file with one,
/// and without it a companion that persists a field does not compile. It went
/// unnoticed because a C# companion only builds on a Windows machine.
const CSHARP_COMPANION_USING: &str = "using System;\nusing System.Collections;\n\
                                      using System.Collections.Generic;\n\
                                      using System.Globalization;\nusing System.Linq;\n\
                                      using System.Runtime.InteropServices;\n\
                                      using System.Text;\nusing System.Text.Json.Nodes;\n";
/// The project a native C# companion is published as.
///
/// `NativeLib=Shared` because the host is a C executable: a managed assembly
/// would need a runtime host to load, and a static library would drag the whole
/// AOT link into an MSVC command line. A DLL beside the executable is what both
/// toolchains already know how to produce and consume.
const CSHARP_COMPANION_PROJECT: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
    <RuntimeIdentifier>win-x64</RuntimeIdentifier>
    <OutputType>Library</OutputType>
    <Nullable>disable</Nullable>
    <PublishAot>true</PublishAot>
    <NativeLib>Shared</NativeLib>
    <InvariantGlobalization>true</InvariantGlobalization>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
</Project>
"#;

/// Stages the entry route's native C# companion as a publishable project.
///
/// Returns the project directory, or `None` when no route declares a C#
/// companion — which is every project that has not asked for one.
fn stage_companion(
    companions: &[super::registry::NativeCompanionInput],
    stage: &Path,
    application_id: &str,
) -> Result<Option<PathBuf>, Failure> {
    let Some(authored) =
        super::registry::source(companions, crate::project::NativeCompanion::CSharp)?
    else {
        return Ok(None);
    };
    let project = stage.join("project").join("companion");
    // NativeApplication validates the identifier before staging. A C# string
    // literal still receives the same escaping as its JSON representation.
    let identifier = serde_json::to_string(application_id).map_err(|_| {
        native_tool_failure(1605, "Cannot encode the companion application identifier.")
    })?;
    let application = format!(
        "internal static class TacApplication {{ internal const string Id = {identifier}; }}\n"
    );
    write(
        &project.join("TachyonCompanion.cs"),
        format!("{CSHARP_COMPANION_USING}\n{application}\n{authored}\n{CSHARP_COMPANION_PRELUDE}")
            .as_bytes(),
    )?;
    write(
        &project.join("TachyonCompanion.csproj"),
        CSHARP_COMPANION_PROJECT.as_bytes(),
    )?;
    Ok(Some(project))
}

/// Publishes the staged companion beside the executable that loads it.
async fn publish_companion(project: &Path, bin: &Path) -> Result<(), Failure> {
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
    let built = project.join("bin/Release/net9.0/win-x64/publish/TachyonCompanion.dll");
    let published = bin.join("TachyonCompanion.dll");
    super::host::native_io(std::fs::copy(&built, &published), &published)?;
    Ok(())
}

/// Returns the C compiler that produces a Windows executable on this machine.
///
/// MSVC is the platform's own toolchain and the one `WebView2`'s SDK targets, so
/// a Windows host is built the way a Windows application is built. It only
/// runs on Windows, which puts this host on the same footing as the macOS and
/// Linux ones: a native host is built on its own operating system. See
/// ADR 0017.
const fn windows_compiler() -> &'static str {
    "cl"
}

/// Writes an `.ico` wrapping the manifest's PNG, and compiles it to a `.res`.
///
/// An icon directory may hold a PNG verbatim rather than a DIB, which Windows
/// has read since Vista — so no converter is needed, just a 22-byte header in
/// front of the file the project already ships. The one constraint is size: a
/// PNG-compressed entry may not exceed 256 pixels, so the largest icon within
/// that is the one used.
///
/// Returns the `.res` to hand the linker, or `None` when the project declared
/// no icon small enough.
///
/// # Errors
///
/// Returns diagnostics when the resource compiler is absent or refuses.
async fn stage_icon(
    application: &NativeApplication,
    web_bundle: &Path,
    stage: &Path,
) -> Result<Option<PathBuf>, Failure> {
    // 256 is the format's ceiling for a PNG-compressed entry, not a taste.
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

    let mut ico = Vec::with_capacity(png.len() + 22);
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // an icon, not a cursor
    ico.extend_from_slice(&1u16.to_le_bytes()); // one image
    // Zero means 256 in this field, and every size we accept is at most that.
    ico.push(0);
    ico.push(0);
    ico.push(0); // palette entries: none, it is truecolour
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    ico.extend_from_slice(&length.to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // the image follows the header
    ico.extend_from_slice(&png);

    let icon = stage.join("icon.ico");
    super::host::write(&icon, &ico)?;
    // `1` is the ordinal Windows shows for an executable, which is why the
    // shell picks it up without the application asking.
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

async fn compile_c(
    source: &Path,
    executable: &Path,
    resource: Option<&str>,
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
    // cl.exe writes its banner to stdout and has no --version; the banner line
    // is the version, and /? exits without needing an input file.
    let version = first_line(&run_tool(compiler, &["/?"]).await?, "MSVC unknown");
    let source = compiler_path(source, "Host source")?;
    let executable = compiler_path(executable, "Application")?;
    let object_directory = compiler_path(
        executable
            .rsplit_once('\\')
            .map_or(Path::new("."), |(directory, _)| Path::new(directory)),
        "Object directory",
    )
    .unwrap_or_else(|_| String::from("."));
    run_tool(
        compiler,
        &[
            "/nologo",
            "/std:c17",
            "/W4",
            "/WX",
            "/O2",
            &format!("/I{}", sdk.join("build/native/include").display()),
            &format!("/Fo{object_directory}\\"),
            &source,
            // A resource object is a linker input; `cl` forwards anything it
            // does not recognise as C.
            resource.unwrap_or("/nologo"),
            &format!("/Fe:{executable}"),
            "/link",
            &format!("/LIBPATH:{}", sdk.join("build/native/x64").display()),
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
        ],
    )
    .await?;
    Ok(version)
}

/// Resolves the `WebView2` SDK the generated host compiles against.
///
/// The SDK is a `NuGet` package rather than part of the Windows SDK, so it is a
/// declared prerequisite like GTK4 is on Linux. Naming it explicitly beats
/// hand-declaring its COM interfaces here: a wrong vtable offset is a silent
/// crash, not a compile error.
fn webview2_sdk() -> Result<PathBuf, Failure> {
    let configured = std::env::var_os(WEBVIEW2_SDK).map(PathBuf::from);
    if let Some(path) = configured {
        if path.join("build/native/include/WebView2.h").is_file() {
            return Ok(path);
        }
        return Err(native_tool_failure(
            1605,
            &format!(
                "{WEBVIEW2_SDK} does not contain build/native/include/WebView2.h. \
                 Point it at an extracted Microsoft.Web.WebView2 package."
            ),
        ));
    }
    Err(native_tool_failure(
        1605,
        &format!(
            "The Windows host needs the WebView2 SDK. Restore Microsoft.Web.WebView2 \
             and set {WEBVIEW2_SDK} to the extracted package directory."
        ),
    ))
}

/// Strips the extended-length prefix MSVC does not accept on a command line.
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

fn c_source(
    application: &NativeApplication,
    index: &NativeRouteIndex,
    companions: &[super::registry::NativeCompanionInput],
) -> String {
    C_HOST
        .replace(
            "__LOCAL_BUNDLE_HELPERS__",
            &super::routes::c_local_bundle(index, companions, "https://tachyon.local"),
        )
        .replace("__APP_NAME__", &c_string_escape(&application.name))
        .replace(
            "__BUNDLE_ID__",
            &c_string_escape(&application.application_id),
        )
        .replace("__ENTRY_ROUTE__", &c_string_escape(&index.entry_route))
        .replace(
            "__NATIVE_SHIM__",
            &c_string_escape(&super::host::native_shim(&application.window)),
        )
}

/// The generated host source, for the capability-drift test.
///
/// The dispatch arms live in this string rather than in Rust, so the only way
/// to assert that a host implements what the bundle advertises is to read it.
#[cfg(test)]
pub(super) const fn host_source() -> &'static str {
    C_HOST
}

const C_HOST: &str = r#"#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#define WIN32_LEAN_AND_MEAN
#define COBJMACROS

/* windows.h must precede the other platform headers; they depend on its types. */
#include <windows.h>

#include <objbase.h>
#include <shellapi.h>
#include <shlwapi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* COBJMACROS is what makes the C convenience forms exist at all; without it a
   MIDL header declares only the vtable structs and every call below is an
   implicit declaration, which /W4 /WX turns into an error. */
#include <WebView2.h>

__LOCAL_BUNDLE_HELPERS__

/* A Win32 window around one WebView2 showing the application's own bundle.
   This used to lower a Native UI tree into a generated control table; see
   native/routes.rs for why it does not any more. */

#define TACHYON_APP_NAME "__APP_NAME__"
#define TACHYON_BUNDLE_ID "__BUNDLE_ID__"
#define TACHYON_ENTRY_ROUTE "__ENTRY_ROUTE__"
/* The unit separator the page-side shim joins its arguments with. */
#define TACHYON_SEPARATOR L'\x1f'
/* Carries one companion publish onto the UI thread. WebView2 may only be
   touched from the thread that created its controller, and a companion may
   publish from any thread it started itself. */
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

/* The native companion, published beside this executable as a NativeAOT
   library and reached through two ordinary C entry points. It is looked up
   rather than linked, so an application without one simply finds nothing. */
typedef const char *(*TachyonCompanionInvoke)(const char *);
typedef void (*TachyonCompanionFree)(const char *);
typedef void (*TachyonCompanionEmit)(const char *);
typedef void (*TachyonCompanionSetEmit)(TachyonCompanionEmit);

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

static void tachyon_record(const char *event) {
  /* Sized so the suffixes below always fit: MAX_PATH for the base leaves no
     room for them, which a truncating snprintf is right to complain about. */
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

/* The sink handed to the companion.

   The other direction of the bridge: everything else is the page asking a
   question, and a companion watching the platform has no question to answer
   because nobody asked one.

   The payload is copied because the pointer is borrowed for this call only and
   the window proc that uses it runs later, on another thread. */
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

static void tachyon_companion_load_one(const wchar_t *filename, int language) {
  wchar_t path[MAX_PATH];
  DWORD length = GetModuleFileNameW(NULL, path, MAX_PATH);
  if (length == 0 || length >= MAX_PATH) {
    return;
  }
  wchar_t *separator = wcsrchr(path, L'\\');
  if (separator == NULL) {
    return;
  }
  separator[1] = L'\0';
  if (wcslen(path) + wcslen(filename) >= MAX_PATH) {
    return;
  }
  wcscat_s(path, MAX_PATH, filename);
  HMODULE library = LoadLibraryExW(path, NULL, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
  if (library == NULL) {
    return;
  }
  g_companion_invoke[language] = (TachyonCompanionInvoke)GetProcAddress(library, "tac_native_invoke");
  g_companion_free[language] = (TachyonCompanionFree)GetProcAddress(library, "tac_native_free");
  /* Looked up rather than required: a companion built before this existed
     loads and answers questions, it just never publishes. */
  TachyonCompanionSetEmit set_emit =
      (TachyonCompanionSetEmit)GetProcAddress(library, "tac_native_set_emit");
  if (set_emit != NULL) {
    set_emit(tachyon_companion_emit);
  }
  if (g_companion_invoke[language] != NULL) {
    tachyon_record("companion.loaded");
  }
}

static void tachyon_companion_load(void) {
  tachyon_companion_load_one(L"TachyonRustCompanion.dll", 1);
  tachyon_companion_load_one(L"TachyonCompanion.dll", 2);
}

/* The capability is echoed into a JSON string, so anything that could close
   that string early is dropped rather than escaped. */
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

/* One function is the whole native surface: a capability and a JSON payload
   in, a JSON answer out. */
static char *tachyon_handle(const char *capability, const char *payload) {
  (void)payload;
  if (strcmp(capability, "companion.invoke") == 0) {
    const TachyonLocalRoute *route = tachyon_current_route();
    if (route == NULL || !tachyon_payload_route_matches(payload, route->route)) return _strdup("{\"error\":\"Native route mismatch.\"}");
    int language = route->language;
    if (language == 0) return _strdup(tachyon_payload_string_matches(payload, "op", "init")
      ? "{\"value\":{\"fields\":[],\"methods\":[]}}" : "{\"error\":\"No native companion for this route.\"}");
    if (g_companion_invoke[language] == NULL) {
      return _strdup("{\"error\":\"This application has no native companion.\"}");
    }
    const char *answer = g_companion_invoke[language](payload);
    char *copy = _strdup(answer != NULL && strnlen(answer, TACHYON_MESSAGE_LIMIT + 1) <= TACHYON_MESSAGE_LIMIT ? answer : "{\"error\":\"Invalid native response.\"}");
    if (g_companion_free[language] != NULL && answer != NULL) {
      g_companion_free[language](answer);
    }
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

/* WebView2 delivers a message as one string, so the shim joins the capability
   and its payload with a unit separator and the host splits them here. */
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
  if (payload == NULL || payload == request_id || (size_t)(payload - request_id) > 64 || strlen(payload + 1) > TACHYON_MESSAGE_LIMIT) { free(message); return; }
  *payload++ = '\0';
  for (const char *cursor = request_id; *cursor != '\0'; cursor++) {
    if (!((*cursor >= '0' && *cursor <= '9') || (*cursor >= 'a' && *cursor <= 'f') || *cursor == '-' || *cursor == '.')) { free(message); return; }
  }
  char *answer = tachyon_handle(message, payload);
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

/* These callbacks have static lifetime. Only IUnknown and their declared
   interface are supported; claiming arbitrary COM interfaces permits callers
   to use the wrong vtable (for example an agility or marshaling interface). */
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

typedef struct { ICoreWebView2NavigationStartingEventHandlerVtbl *lpVtbl; } TachyonNavigationHandler;
static HRESULT STDMETHODCALLTYPE navigation_query(ICoreWebView2NavigationStartingEventHandler *self, REFIID iid, void **object) { return tachyon_query_interface(self, iid, object, L"{9adbe429-f36d-432b-9ddc-f8881fbd76e3}"); }
static ULONG STDMETHODCALLTYPE navigation_add_ref(ICoreWebView2NavigationStartingEventHandler *self) { (void)self; return 1; }
static ULONG STDMETHODCALLTYPE navigation_release(ICoreWebView2NavigationStartingEventHandler *self) { (void)self; return 1; }
static HRESULT STDMETHODCALLTYPE navigation_invoke(ICoreWebView2NavigationStartingEventHandler *self, ICoreWebView2 *sender, ICoreWebView2NavigationStartingEventArgs *arguments) {
  (void)self;
  LPWSTR uri = NULL;
  if (FAILED(ICoreWebView2NavigationStartingEventArgs_get_Uri(arguments, &uri)) || uri == NULL) return E_FAIL;
  char *text = tachyon_narrow(uri), path[TACHYON_PATH_LIMIT];
  const TachyonLocalRoute *route = text != NULL && tachyon_local_path(text, path, sizeof(path)) ? tachyon_document_route(path) : NULL;
  if (route == NULL) ICoreWebView2NavigationStartingEventArgs_put_Cancel(arguments, TRUE);
  else {
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
    }
  }
  free(text); CoTaskMemFree(uri); return S_OK;
}
static ICoreWebView2NavigationStartingEventHandlerVtbl g_navigation_vtbl = {navigation_query, navigation_add_ref, navigation_release, navigation_invoke};
static TachyonNavigationHandler g_navigation_handler = {&g_navigation_vtbl};

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
    L"Content-Type: %s\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data: https:; connect-src 'self' https:; frame-src 'none'; object-src 'none'; base-uri 'self'\r\n",
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

/* AddScript is asynchronous: navigate only after the native bridge has been
   registered for document creation, never in a race with its completion. */
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
  ICoreWebView2_add_WebMessageReceived(
      g_webview, (ICoreWebView2WebMessageReceivedEventHandler *)&g_message_handler, &token);
  ICoreWebView2_add_NavigationStarting(g_webview, (ICoreWebView2NavigationStartingEventHandler *)&g_navigation_handler, &token);
  ICoreWebView2_add_FrameNavigationStarting(g_webview, (ICoreWebView2NavigationStartingEventHandler *)&g_navigation_handler, &token);
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
  /* ICoreWebView2_3's published interface identifier; using a local constant
     avoids relying on a C++ __uuidof extension in this C17 host. */
  const IID webview3_iid = {0xa0d6df20,0x3b92,0x416d,{0xaa,0x0c,0x43,0x7a,0x9c,0x72,0x78,0x57}};
  ICoreWebView2_3 *mapped = NULL;
  if (FAILED(ICoreWebView2_QueryInterface(g_webview, &webview3_iid, (void **)&mapped)) || mapped == NULL) return E_NOINTERFACE;
  HRESULT mapping = ICoreWebView2_3_SetVirtualHostNameToFolderMapping(mapped, L"tachyon.local", g_resource_root, COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY);
  ICoreWebView2_3_Release(mapped);
  if (FAILED(mapping)) return mapping;

  /* Injected before the bundle's own scripts, the same shim every host uses,
     with the Windows half of the bridge appended. */
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
      /* Owned from here: the emit that posted it handed the copy over. */
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
    case WM_DESTROY:
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
  tachyon_record("controller.created");
  tachyon_companion_load();

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
  if (g_window == NULL) { CoUninitialize(); return 1; }
  InterlockedExchangePointer(&g_publish_window, g_window);
  ShowWindow(g_window, show);

  CreateCoreWebView2EnvironmentWithOptions(
      NULL, NULL, NULL,
      (ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler *)&g_environment_handler);

  MSG message;
  BOOL message_result;
  while ((message_result = GetMessageW(&message, NULL, 0, 0)) > 0) {
    TranslateMessage(&message);
    DispatchMessageW(&message);
  }
  CoUninitialize();
  return message_result < 0 ? 1 : 0;
}
"#;

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{application_manifest, c_source, compiler_path, stage_companion, windows_compiler};
    use crate::native::config::NativeApplication;
    use std::path::Path;

    fn application() -> NativeApplication {
        NativeApplication {
            name: String::from("Native Catalog"),
            executable_name: String::from("NativeCatalog"),
            application_id: String::from("dev.tachyon.native-catalog"),
            version: String::from("1.0.0"),
            entry_route: String::from("/"),
            icons: Vec::new(),
            window: crate::native::config::WindowConfiguration::default(),
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
    fn the_host_is_a_win32_window_around_one_webview() {
        let source = c_source(&application(), &index(), &[]);
        assert!(source.contains("#define COBJMACROS"));
        assert!(source.contains("CreateCoreWebView2EnvironmentWithOptions"));
        assert!(source.contains("ICoreWebView2_Navigate"));
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains("tac_native_invoke"));
        // Nothing lowers the view into a generated control table any more.
        assert!(!source.contains("TACHYON_ITEMS"));
        assert!(!source.contains("tachyon_create_controls"));
        assert!(source.contains("SetVirtualHostNameToFolderMapping"));
        assert!(source.contains("ICoreWebView2WebMessageReceivedEventArgs_get_Source"));
        assert!(source.contains("tachyon_payload_route_matches"));
        assert!(source.contains("TachyonRustCompanion.dll"));
        assert!(source.contains("CoInitializeEx"));
        assert!(!source.contains("file:///"));
        assert!(!source.contains("char script[8192]"));
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
        // MSVC is what the WebView2 SDK targets, and it only runs on Windows.
        assert_eq!(windows_compiler(), "cl");
    }

    #[test]
    fn csharp_local_storage_is_scoped_to_the_application_identifier() {
        let temporary = tempfile::tempdir().expect("temporary");
        let source = temporary.path().join("tac.cs");
        std::fs::write(
            &source,
            "public class Companion {\npublic int Count = 0;\n}\n",
        )
        .expect("companion");
        let companions = [crate::native::registry::NativeCompanionInput {
            language: crate::project::NativeCompanion::CSharp,
            source,
            route: String::from("/"),
        }];
        for identifier in ["dev.tachyon.first", "dev.tachyon.second"] {
            let output = temporary.path().join(identifier);
            let project = stage_companion(&companions, &output, identifier)
                .expect("stage")
                .expect("C# project");
            let generated = std::fs::read_to_string(project.join("TachyonCompanion.cs"))
                .expect("generated companion");
            assert!(generated.contains(&format!("Id = \"{identifier}\"")));
            assert!(generated.contains("\"Tachyon\", TacApplication.Id, \"store.txt\""));
        }
    }

    #[test]
    fn extended_length_prefixes_are_stripped_for_the_command_line() {
        // cl.exe rejects the \\?\ prefix, and keeps backslashes otherwise.
        assert_eq!(
            compiler_path(Path::new(r"\\?\C:\build\host.c"), "Host source").expect("path"),
            r"C:\build\host.c"
        );
    }
}
