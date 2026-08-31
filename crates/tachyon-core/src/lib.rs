//! Tachyon's project, compiler, handler, scaffold, and development-server core.

pub mod cache;
mod companion;
mod compiler;
pub mod doctor;
mod external_command;
mod failure;
#[cfg(feature = "fuzzing")]
pub mod fuzzing;
mod handler;
mod hot_update;
mod html;
mod lexical;
mod migrate;
mod native;
mod project;
mod routing;
mod scaffold;
mod server;
pub mod stereotype;
mod template;
mod ttid;

pub use compiler::{BuildOptions, BuildResult, WebCompiler};
pub use failure::Failure;
pub use handler::{
    EnvironmentPolicy, FirecrackerIsolation, HandlerCancellation, HandlerLanguage,
    HandlerRuntimePrograms, HandlerSource, HandlerSupervisor, HandlerSupervisorOptions,
    MethodContract, RequestContract, RouteContract, Workers, YonIsolationPolicy,
};
pub use html::{HtmlDocument, HtmlFrontend};
pub use migrate::{MigrationAnalysis, MigrationFinding, MigrationReport, MigrationStatus};
pub use native::{NativeBuildOptions, NativeBuildResult, NativeCompiler, native_target_directory};
pub use project::{
    CompanionKind, CompanionSource, HandlerNode, NativeCompanion, Project, ProjectDiscovery,
    RouteGraph, RouteNode, ViewKind,
};
pub use routing::{RouteMatch, match_route};
pub use scaffold::{Scaffold, ScaffoldResult};
pub use server::{DevServer, DevServerOptions, PreviewServer, PreviewServerOptions};
pub use ttid::{created_at_milliseconds as ttid_created_at, generate as generate_ttid};

/// Removes the optional UTF-8 text-file byte order mark.
pub(crate) fn without_bom(source: &str) -> &str {
    source.strip_prefix('\u{feff}').unwrap_or(source)
}
