//! Compiles a `tac.rs` companion for whichever target is being built.
//!
//! Rust is the desktop companion: the macOS host links a static library and
//! calls it from Swift, and the Win32 and GTK hosts load a shared one. The
//! companion runs in the host's own process with the platform in reach, the
//! same as a Swift, Kotlin or C# one. It stops at the desktop because Android
//! and iOS would each need a build system in service of a language that is not
//! the language of either platform — an NDK and a JNI shim, or an `xcframework`.
//!
//! One file is compiled with `rustc` and no Cargo, so a companion declares no
//! dependencies. That is a real limit, and a deliberate one: it keeps a
//! companion to the thing it is for — reaching the platform — rather than
//! becoming a second application with its own manifest.

use super::host::{native_tool_failure, run_tool, write};
use crate::Failure;
use std::path::{Path, PathBuf};

/// The prelude appended to a Rust companion compiled into a native host.
const RUST_COMPANION_PRELUDE: &str = include_str!("prelude.rs");

/// The Rust companion prelude, for the publish-channel drift test.
#[cfg(test)]
pub(super) const fn companion_prelude() -> &'static str {
    RUST_COMPANION_PRELUDE
}

/// What a host links or loads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Linkage {
    /// A static library, linked into the host binary.
    Static,
    /// A shared library, loaded beside the host at run time.
    Shared,
}

impl Linkage {
    const fn crate_type(self) -> &'static str {
        match self {
            Self::Static => "staticlib",
            Self::Shared => "cdylib",
        }
    }
}

/// Stages one Rust companion with its generated table and prelude.
///
/// Returns the staged source, or `None` when this target's companion is in
/// another language — or when there is none at all.
pub(super) fn stage(
    companions: &[super::registry::NativeCompanionInput],
    stage: &Path,
    application_id: &str,
) -> Result<Option<PathBuf>, Failure> {
    let Some(authored) =
        super::registry::source(companions, crate::project::NativeCompanion::Rust)?
    else {
        return Ok(None);
    };
    let staged = stage.join("project").join("companion.rs");
    write(
        &staged,
        format!("{authored}\nconst TAC_APPLICATION_ID: &str = {application_id:?};\n{RUST_COMPANION_PRELUDE}").as_bytes(),
    )?;
    Ok(Some(staged))
}

/// Compiles one staged companion into a library the host can reach.
///
/// # Errors
///
/// Returns diagnostics when `rustc` is absent or rejects the companion.
pub(super) async fn compile(
    source: &Path,
    linkage: Linkage,
    target_triple: Option<&str>,
    output: &Path,
) -> Result<String, Failure> {
    if let Some(parent) = output.parent() {
        super::host::native_io(std::fs::create_dir_all(parent), parent)?;
    }
    let version =
        super::host::first_line(&run_tool("rustc", &["--version"]).await?, "rustc unknown");
    let source = source
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Companion source path is not valid Unicode."))?;
    let output = output
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Companion output path is not valid Unicode."))?;
    // The 2024 edition, because the prelude's `unsafe extern` blocks are its
    // spelling. Everything an author writes is ordinary Rust either way.
    let mut arguments = vec![
        "--edition",
        "2024",
        "--crate-type",
        linkage.crate_type(),
        "-O",
        source,
        "-o",
        output,
    ];
    if let Some(triple) = target_triple {
        arguments.extend(["--target", triple]);
    }
    run_tool("rustc", &arguments).await?;
    Ok(version)
}
