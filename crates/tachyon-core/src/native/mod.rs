mod android;
mod compiler;
mod config;
mod host;
mod ios;
mod linux;
mod macos;
mod registry;
mod routes;
mod rust;
mod windows;

pub use compiler::{
    NativeBuildOptions, NativeBuildResult, NativeCompiler, native_target_directory,
};
pub(crate) use config::{
    MANIFEST_NAME, MANIFEST_OUTPUT, PageMetadata, browser_scripts, browser_styles, cache_rules,
    config_module_path, manifest_head, page_metadata,
};

/// Uses the same bounded frontend all native bundle builds consume.
#[cfg(feature = "fuzzing")]
pub(crate) fn plan_for_fuzzing(
    _target: tachyon_contracts::NativeTarget,
    html: &str,
) -> Result<(), crate::Failure> {
    crate::template::TemplateFrontend::compile(
        html,
        "fuzz/tac.html",
        &std::collections::BTreeSet::new(),
    )
    .map(|_| ())
}
