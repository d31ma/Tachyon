//! Compiles a browser companion with its own language's compiler.
//!
//! Nothing here understands the languages. Each one's compiler is invoked and
//! the artefacts it produces are shipped, which is what keeps this from being
//! the five partial language implementations the legacy build maintains.
//!
//! Two shapes reach the island, both speaking the JSON protocol of ADR 0011:
//!
//! * a **bare module** — `memory`, `tac_alloc`, `tac_invoke`, no imports —
//!   from a toolchain that compiles to linear memory, such as `rustc`;
//! * a **glued module** — a JavaScript module exporting
//!   `tacInvoke(request) -> response` — from a toolchain whose output cannot be
//!   instantiated on its own: `dart compile wasm` and `kotlinc-js` emit
//!   `WasmGC` with a loader beside it, and a .NET publish emits a runtime
//!   rather than a module at all.
//!
//! The language decides which, because its toolchain does. A companion author
//! writes plain code either way: the prelude appended below carries the ABI so
//! that no one hand-writes `tac_invoke`.

use crate::Failure;
use crate::failure::{diagnostic, source_span};
use std::path::Path;
use tokio::process::Command;

/// What the compiler appends to a Dart companion to make it satisfy the ABI.
const DART_PRELUDE: &str = include_str!("wasm/prelude.dart");
/// Imports the prelude needs, placed before the author's own.
const DART_IMPORTS: &str = "import 'dart:convert';\nimport 'dart:js_interop';\n\
                            import 'dart:js_interop_unsafe';\n";
/// Entry module that adapts the Dart compiler's own glue to `tacInvoke`.
const DART_WRAPPER: &str = include_str!("wasm/wrapper.dart.mjs");
/// What the compiler appends to a Kotlin companion to make it satisfy the ABI.
const KOTLIN_PRELUDE: &str = include_str!("wasm/prelude.kt");
/// The opt-in `@JsExport` needs, which must precede everything in the file.
const KOTLIN_HEADER: &str = "@file:OptIn(kotlin.js.ExperimentalJsExport::class)\n";
/// Entry module that adapts Kotlin's own exports to `tacInvoke`.
const KOTLIN_WRAPPER: &str = include_str!("wasm/wrapper.kotlin.mjs");
/// Where the Kotlin standard library for wasm is, which the command-line
/// compiler does not ship. `ty doctor` reports it.
const KOTLIN_STDLIB: &str = "KOTLIN_WASM_STDLIB";
/// What the compiler appends to a Swift companion to make it satisfy the ABI.
const SWIFT_PRELUDE: &str = include_str!("wasm/prelude.swift");
/// What the compiler appends to a C# companion to make it satisfy the ABI.
const CSHARP_PRELUDE: &str = include_str!("wasm/prelude.cs");
/// Namespaces the prelude needs, which C# requires before any type.
const CSHARP_USING: &str = "using System;\nusing System.Collections;\n\
                            using System.Collections.Generic;\nusing System.Globalization;\n\
                            using System.Runtime.InteropServices.JavaScript;\nusing System.Text;\n\
                            using System.Text.Json.Nodes;\n";
/// The project a C# companion is published as. Trimming is what keeps the
/// bundle to megabytes rather than tens of them, and `JSExport` generates
/// unsafe interop stubs, so both are declared rather than left to a default.
const CSHARP_PROJECT: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
    <RuntimeIdentifier>browser-wasm</RuntimeIdentifier>
    <OutputType>Exe</OutputType>
    <Nullable>disable</Nullable>
    <WasmMainJSPath>main.js</WasmMainJSPath>
    <InvariantGlobalization>true</InvariantGlobalization>
    <PublishTrimmed>true</PublishTrimmed>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
  </PropertyGroup>
</Project>
"#;
/// Entry module that boots the .NET runtime and adapts its export.
const CSHARP_WRAPPER: &str = include_str!("wasm/wrapper.cs.mjs");

/// One file to emit for the component, named by what follows the component's
/// own name. A language that emits a bundle rather than a file contributes a
/// path here, not just an extension.
pub(crate) type Artefact = (String, Vec<u8>);

/// Reports the file an island loads for this companion, after its name.
///
/// A glued language's entry point is the JavaScript module, not the wasm, so
/// the island is pointed at that instead.
pub(crate) fn asset_suffix(source: &Path) -> &'static str {
    if is_glued(source) { ".mjs" } else { ".wasm" }
}

/// Reports whether a language's toolchain can only emit a module that needs
/// the JavaScript the compiler emits beside it.
pub(crate) fn is_glued(source: &Path) -> bool {
    source
        .extension()
        .is_some_and(|value| value == "dart" || value == "kt" || value == "cs")
}

/// Compiles one companion, returning every file the build must emit.
///
/// # Errors
///
/// Returns `TY1406` when the language's compiler is absent or rejects the
/// companion.
pub(crate) async fn compile(
    project_root: &Path,
    source: &Path,
    component: &str,
) -> Result<Vec<Artefact>, Failure> {
    let portable =
        crate::compiler::portable_path(source.strip_prefix(project_root).unwrap_or(source));
    let staged = tempfile::Builder::new()
        .prefix(".tachyon-wasm-")
        .tempdir()
        .map_err(|error| failure(&portable, &format!("Cannot stage the build: {error}")))?;
    match source.extension().and_then(|value| value.to_str()) {
        Some("dart") => dart(&portable, staged.path(), source).await,
        Some("kt") => kotlin(&portable, staged.path(), source, component).await,
        Some("swift") => swift(&portable, staged.path(), source).await,
        Some("cs") => csharp(&portable, staged.path(), source).await,
        _ => rust(&portable, staged.path(), source).await,
    }
}

/// Stages the author's source with the prelude around it.
///
/// A build must not write into the project, and a prelude's own imports have to
/// precede the author's declarations, so the file the compiler sees is
/// assembled rather than edited in place.
fn stage(
    portable: &str,
    staged: &Path,
    source: &Path,
    name: &str,
    header: &str,
    prelude: &str,
) -> Result<std::path::PathBuf, Failure> {
    let authored = std::fs::read_to_string(source)
        .map_err(|error| failure(portable, &format!("Cannot read the companion: {error}")))?;
    let entry = staged.join(name);
    std::fs::write(&entry, format!("{header}\n{authored}\n{prelude}"))
        .map_err(|error| failure(portable, &format!("Cannot stage the companion: {error}")))?;
    Ok(entry)
}

/// Emits a bare module with `rustc`.
///
/// Symbols are stripped because they, not code, are what makes a wasm module
/// large: the reference fixture is 819 KB unstripped and 21 KB stripped.
async fn rust(portable: &str, staged: &Path, source: &Path) -> Result<Vec<Artefact>, Failure> {
    let output = staged.join("companion.wasm");
    let result = Command::new("rustc")
        .args([
            "--target",
            "wasm32-unknown-unknown",
            "--crate-type",
            "cdylib",
            // rustc defaults to the 2015 edition, which rejects `async fn` and
            // much else a companion author would reasonably write. 2021 is the
            // newest edition that does not also require an author to spell
            // `#[unsafe(no_mangle)]` on every export.
            "--edition",
            "2021",
            "-O",
            "-C",
            "strip=symbols",
            "-o",
        ])
        .arg(&output)
        .arg(source)
        .output()
        .await
        .map_err(|error| ran(portable, "rustc", &error))?;
    if !result.status.success() {
        return Err(rejected(portable, &result.stderr));
    }
    Ok(vec![(String::from(".wasm"), read(portable, &output)?)])
}

/// Emits a glued module with `dart compile wasm`.
///
/// The author's source is staged with the prelude around it rather than
/// compiled in place: the prelude's imports must precede the author's
/// declarations, and a build must not write into the project.
async fn dart(portable: &str, staged: &Path, source: &Path) -> Result<Vec<Artefact>, Failure> {
    let entry = stage(
        portable,
        staged,
        source,
        "companion.dart",
        DART_IMPORTS,
        DART_PRELUDE,
    )?;
    let output = staged.join("companion.wasm");
    let result = Command::new("dart")
        .args(["compile", "wasm", "--no-source-maps", "-o"])
        .arg(&output)
        .arg(&entry)
        .output()
        .await
        .map_err(|error| ran(portable, "dart", &error))?;
    if !result.status.success() {
        // dart reports compilation errors on standard output.
        let detail = if result.stderr.is_empty() {
            &result.stdout
        } else {
            &result.stderr
        };
        return Err(rejected(portable, detail));
    }
    Ok(vec![
        (String::from(".dart.wasm"), read(portable, &output)?),
        (
            String::from(".dart.mjs"),
            read(portable, &staged.join("companion.mjs"))?,
        ),
        (String::from(".mjs"), DART_WRAPPER.as_bytes().to_vec()),
    ])
}

/// Emits a glued module with `kotlinc-js` targeting wasm.
///
/// Two invocations, because the command-line compiler produces an executable
/// only from a library: sources become a klib, then the klib becomes the
/// module. Dead code elimination is not a nicety here — it is the difference
/// between 584 KB and 120 KB for the same fixture.
///
/// The emitted JavaScript names its siblings, so the output is named for the
/// component from the start rather than renamed afterwards.
async fn kotlin(
    portable: &str,
    staged: &Path,
    source: &Path,
    component: &str,
) -> Result<Vec<Artefact>, Failure> {
    let stdlib = std::env::var(KOTLIN_STDLIB).map_err(|_| {
        failure(
            portable,
            "the Kotlin standard library for wasm is not configured. Run ty doctor.",
        )
    })?;
    let entry = stage(
        portable,
        staged,
        source,
        "companion.kt",
        KOTLIN_HEADER,
        KOTLIN_PRELUDE,
    )?;
    let library = staged.join("library");
    let emitted = staged.join("module");
    let name = format!("{component}.kotlin");

    let compiled = Command::new("kotlinc-js")
        .args(["-Xwasm", "-Xwasm-target=wasm-js", "-libraries"])
        .arg(&stdlib)
        .args(["-Xir-produce-klib-file", "-ir-output-dir"])
        .arg(&library)
        .args(["-ir-output-name", "companion"])
        .arg(&entry)
        .output()
        .await
        .map_err(|error| ran(portable, "kotlinc-js", &error))?;
    if !compiled.status.success() {
        return Err(rejected(portable, &compiled.stderr));
    }

    let linked = Command::new("kotlinc-js")
        .args(["-Xwasm", "-Xwasm-target=wasm-js", "-libraries"])
        .arg(&stdlib)
        .args(["-Xir-produce-js", "-Xir-dce"])
        .arg(format!(
            "-Xinclude={}",
            library.join("companion.klib").display()
        ))
        .arg("-ir-output-dir")
        .arg(&emitted)
        .args(["-ir-output-name", &name])
        .output()
        .await
        .map_err(|error| ran(portable, "kotlinc-js", &error))?;
    if !linked.status.success() {
        return Err(rejected(portable, &linked.stderr));
    }

    Ok(vec![
        (
            String::from(".kotlin.wasm"),
            read(portable, &emitted.join(format!("{name}.wasm")))?,
        ),
        (
            String::from(".kotlin.mjs"),
            read(portable, &emitted.join(format!("{name}.mjs")))?,
        ),
        (
            String::from(".kotlin.uninstantiated.mjs"),
            read(
                portable,
                &emitted.join(format!("{name}.uninstantiated.mjs")),
            )?,
        ),
        (String::from(".mjs"), KOTLIN_WRAPPER.as_bytes().to_vec()),
    ])
}

/// Emits a bare module with the swift.org compiler and the Swift SDK for
/// WebAssembly.
///
/// Xcode's own toolchain cannot cross-compile to wasm, so the compiler is the
/// one `xcrun` finds under the `swift` toolchain, and the target libraries come
/// from the SDK `swift sdk install` unpacked. The module is linked as a
/// reactor: it exports `_initialize` rather than `_start`, because a companion
/// is called into rather than run.
async fn swift(portable: &str, staged: &Path, source: &Path) -> Result<Vec<Artefact>, Failure> {
    let sdk = swift_sdk().ok_or_else(|| {
        failure(
            portable,
            "no Swift SDK for WebAssembly is installed. Run ty doctor.",
        )
    })?;
    let entry = stage(
        portable,
        staged,
        source,
        "companion.swift",
        "",
        SWIFT_PRELUDE,
    )?;
    let output = staged.join("companion.wasm");
    let resources = sdk.join("swift.xctoolchain/usr/lib/swift_static");
    let result = Command::new(swift_compiler())
        .args(["-target", "wasm32-unknown-wasip1", "-sdk"])
        .arg(sdk.join("WASI.sdk"))
        .arg("-resource-dir")
        .arg(&resources)
        // Swift's own driver looks for the clang runtime beside the host
        // compiler, which does not carry a wasm build of it.
        .args(["-Xclang-linker", "-resource-dir", "-Xclang-linker"])
        .arg(resources.join("clang"))
        .args(["-Xclang-linker", "-mexec-model=reactor"])
        .args([
            "-static-stdlib",
            "-Osize",
            "-wmo",
            "-parse-as-library",
            "-Xlinker",
            "--export=tac_alloc",
            "-Xlinker",
            "--export=tac_invoke",
            "-Xlinker",
            "--strip-all",
            "-Xlinker",
            "--gc-sections",
            "-o",
        ])
        .arg(&output)
        .arg(&entry)
        .output()
        .await
        .map_err(|error| ran(portable, "swiftc", &error))?;
    if !result.status.success() {
        return Err(rejected(portable, &result.stderr));
    }
    Ok(vec![(String::from(".wasm"), read(portable, &output)?)])
}

/// Finds the Swift compiler that can target wasm.
///
/// On macOS that is never the one on `PATH`: Xcode's driver has no
/// `swift-autolink-extract` and fails before it reaches the linker. `xcrun`
/// knows where a swift.org toolchain was installed, so it is asked first.
pub(crate) fn swift_compiler() -> std::path::PathBuf {
    std::process::Command::new("xcrun")
        .args(["--toolchain", "swift", "--find", "swiftc"])
        .output()
        .ok()
        .filter(|found| found.status.success())
        .map(|found| {
            std::path::PathBuf::from(String::from_utf8_lossy(&found.stdout).trim().to_owned())
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| std::path::PathBuf::from("swiftc"))
}

/// Finds the Swift SDK for WebAssembly that `swift sdk install` unpacked.
///
/// The location is the one `SwiftPM` defines, so a project needs no setting for
/// it: installing the SDK the documented way is all the configuration there is.
pub(crate) fn swift_sdk() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let bundles = std::path::Path::new(&home).join(".swiftpm/swift-sdks");
    for entry in std::fs::read_dir(bundles).ok()?.flatten() {
        for inner in std::fs::read_dir(entry.path()).ok()?.flatten() {
            let candidate = inner.path().join("wasm32-unknown-wasip1");
            if candidate.join("WASI.sdk").is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Emits a glued bundle with a .NET wasm publish.
///
/// .NET does not produce a module: it produces a runtime, the companion's
/// assemblies, and a loader for both. The whole bundle is emitted under the
/// component's name and the wrapper boots it, which is the same glued shape
/// Dart and Kotlin take — only larger, because the runtime is the payload.
async fn csharp(portable: &str, staged: &Path, source: &Path) -> Result<Vec<Artefact>, Failure> {
    let project = staged.join("companion");
    std::fs::create_dir_all(&project)
        .map_err(|error| failure(portable, &format!("Cannot stage the build: {error}")))?;
    std::fs::write(project.join("companion.csproj"), CSHARP_PROJECT)
        .and_then(|()| std::fs::write(project.join("main.js"), ""))
        .map_err(|error| failure(portable, &format!("Cannot stage the project: {error}")))?;
    stage(
        portable,
        &project,
        source,
        "Companion.cs",
        CSHARP_USING,
        CSHARP_PRELUDE,
    )?;

    let result = Command::new("dotnet")
        .args(["publish", "-c", "Release"])
        .current_dir(&project)
        .output()
        .await
        .map_err(|error| ran(portable, "dotnet", &error))?;
    if !result.status.success() {
        // MSBuild writes the build log to stdout but workload and restore
        // notices to stderr. Keep both: choosing stderr whenever it is
        // non-empty hides the actual compiler error behind a harmless notice.
        let mut detail = result.stdout;
        detail.extend_from_slice(&result.stderr);
        return Err(rejected(portable, &detail));
    }

    let bundle = project.join("bin/Release/net9.0/browser-wasm/AppBundle");
    let mut artefacts = vec![(String::from(".mjs"), CSHARP_WRAPPER.as_bytes().to_vec())];
    collect(portable, &bundle, &bundle, &mut artefacts)?;
    Ok(artefacts)
}

/// Collects every file of a bundle, keeping its shape below the component.
fn collect(
    portable: &str,
    root: &Path,
    directory: &Path,
    artefacts: &mut Vec<Artefact>,
) -> Result<(), Failure> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| failure(portable, &format!("Cannot read the bundle: {error}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(portable, root, &path, artefacts)?;
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(&path);
        artefacts.push((
            format!("/{}", crate::compiler::portable_path(relative)),
            read(portable, &path)?,
        ));
    }
    Ok(())
}

fn read(portable: &str, path: &Path) -> Result<Vec<u8>, Failure> {
    std::fs::read(path).map_err(|error| {
        failure(
            portable,
            &format!("Cannot read {}: {error}", path.display()),
        )
    })
}

fn ran(portable: &str, program: &str, error: &std::io::Error) -> Failure {
    failure(
        portable,
        &format!("Cannot run {program}: {error}. Run ty doctor to see what is missing."),
    )
}

fn rejected(portable: &str, output: &[u8]) -> Failure {
    let detail = String::from_utf8_lossy(output);
    let lines = detail.lines().filter(|line| !line.trim().is_empty());
    let meaningful = lines
        .clone()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error") || lower.contains("failed")
        })
        .or_else(|| lines.into_iter().next())
        .unwrap_or("compilation failed");
    failure(portable, meaningful.trim())
}

fn failure(source: &str, message: &str) -> Failure {
    Failure::one(diagnostic(
        1406,
        format!("Cannot compile wasm companion '{source}': {message}"),
        Some(String::from(
            "A wasm companion exports memory, tac_alloc and tac_invoke, or, \
             where the toolchain emits only WasmGC, a module exporting \
             tacInvoke. Run ty doctor to check the toolchain.",
        )),
        source_span(source, 0, source.len()),
    ))
}

#[cfg(test)]
mod tests {
    use super::{asset_suffix, rejected};
    use std::path::Path;

    /// The island is told which file to load before anything is compiled, so an
    /// asset extension that disagrees with what the language emits is a blank
    /// component in the page rather than a build failure. The browser gate
    /// proves each language end to end; this is the wire between the two.
    #[test]
    fn each_language_is_pointed_at_the_file_it_emits() {
        for (companion, expected) in [
            ("tac.rs", ".wasm"),
            ("tac.swift", ".wasm"),
            ("tac.dart", ".mjs"),
            ("tac.kt", ".mjs"),
            ("tac.cs", ".mjs"),
        ] {
            assert_eq!(asset_suffix(Path::new(companion)), expected, "{companion}");
        }
    }

    #[test]
    fn compiler_rejection_reports_the_error_instead_of_restore_chatter() {
        let failure = rejected(
            "client/components/example/tac.cs",
            b"Determining projects to restore...\nwarning: workload manifest is stale\nerror CS1002: ; expected\n",
        );
        let rendered = failure.to_string();
        assert!(rendered.contains("error CS1002: ; expected"), "{rendered}");
        assert!(!rendered.contains("Determining projects"), "{rendered}");
    }
}
