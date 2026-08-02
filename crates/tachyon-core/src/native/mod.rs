mod android;
mod compiler;
mod config;
mod host;
mod ios;
mod linux;
mod macos;
mod planner;
mod windows;

pub use compiler::{
    NativeBuildOptions, NativeBuildResult, NativeCompiler, native_target_directory,
};

/// Plans one resolved document for fuzzing. Not a stable API.
#[cfg(feature = "fuzzing")]
pub(crate) fn plan_for_fuzzing(
    target: tachyon_contracts::NativeTarget,
    html: &str,
) -> Result<tachyon_contracts::NativeUi, crate::Failure> {
    planner::NativePlanner::plan(target, "/", "fuzz/tac.html", html, "")
        .map(|planned| planned.native_ui)
}
