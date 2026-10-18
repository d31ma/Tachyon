mod android;
mod browser_api_grants;
#[cfg(test)]
mod browser_task_tests;
mod compiler;
mod config;
mod cpp_web_services;
mod csharp_web_apis;
mod desktop_host_source;
mod desktop_menu_items;
mod dotnet_web_release;
mod events;
mod file_picker_sources;
mod fsharp_behaviors_project;
mod fsharp_service;
mod fsharp_service_events;
mod fsharp_task_results;
mod fsharp_web_project;
mod go;
mod go_compiled_with;
mod go_compiler;
mod go_compiler_choice;
mod go_events;
mod go_needs;
mod go_web_build;
mod host;
mod ios;
mod java_web_services;
mod kotlin_web_services;

#[cfg(test)]
mod kotlin_document_title_tests;

#[cfg(test)]
mod kotlin_document_title_scope_tests;
#[cfg(test)]
mod kotlin_page_graph_tests;
mod linux;
#[cfg(test)]
mod linux_host_tests;
#[cfg(test)]
mod linux_page_call_tests;
mod linux_window_controls;
mod mac_os_menu_action_source;
mod mac_os_menu_source;
mod mac_os_title_bar_source;
mod macos;
#[cfg(test)]
mod macos_menu_tests;
#[cfg(test)]
mod managed_capability_tests;
mod managed_native_capabilities;
mod manifest_setting;
#[cfg(test)]
mod model_tests;
mod native_application_capture;
#[cfg(all(test, target_os = "macos"))]
mod native_application_host_tests;
mod native_application_inputs;
#[cfg(test)]
mod native_application_tests;
mod native_capabilities;
#[cfg(test)]
mod native_capability_tests;
#[cfg(test)]
mod native_cfg_impl_tests;
#[cfg(all(test, target_os = "macos"))]
mod native_file_picker_permission_tests;
#[cfg(test)]
mod native_helper_tests;
mod native_menu_action;
mod native_menu_action_check;
#[cfg(test)]
mod native_menu_action_tests;
mod native_menu_config;
#[cfg(test)]
mod native_menu_config_tests;
mod native_menu_entry;
#[cfg(test)]
mod native_menu_host_tests;
mod native_menu_shortcut;
#[cfg(test)]
mod native_packaged_utility_tests;
#[cfg(test)]
mod native_page_call_contract_tests;
#[cfg(test)]
mod native_page_graph_tests;
#[cfg(test)]
mod native_rust_capability_tests;
#[cfg(test)]
mod native_rust_crate_tests;
mod native_rust_project;
mod native_service_roots;
#[cfg(test)]
mod native_swift_page_lifetime_tests;
mod native_title_bar;
mod native_title_bar_button_inset;
#[cfg(all(test, target_os = "macos"))]
mod native_title_bar_host_tests;
mod native_type_program;
mod native_ui_artifact;
#[cfg(test)]
mod native_ui_artifact_tests;
mod native_ui_asset;
mod native_ui_bounds;
mod native_ui_capture;
mod native_ui_change;
mod native_ui_contract;
mod native_ui_descriptor;
mod native_ui_generation;
#[cfg(test)]
mod native_ui_generation_tests;
#[cfg(all(test, target_os = "macos"))]
mod native_ui_host_sources;
#[cfg(all(test, target_os = "macos"))]
mod native_ui_host_tests;
mod native_ui_problem;
mod native_ui_profile;
#[cfg(test)]
mod native_ui_profile_tests;
#[cfg(all(test, target_os = "macos"))]
mod native_ui_runtime_tests;
#[cfg(all(test, target_os = "macos"))]
mod native_ui_styles_sources;
#[cfg(all(test, target_os = "macos"))]
mod native_ui_styles_tests;
#[cfg(test)]
mod native_web_api_tests;
mod native_web_stage;
#[cfg(test)]
mod native_window_control_tests;
mod native_window_controls;
#[cfg(all(test, target_os = "macos"))]
mod native_window_example_tests;
mod placeholder_fill;
mod registry;
mod routes;
mod rust;
mod rust_companion_crate;
mod rust_companion_output;
mod rust_crate_build;
mod rust_dependency;
#[cfg(test)]
mod rust_filename_tests;
mod rust_imports;
mod rust_local_paths;
mod rust_native_libraries;
mod rust_page_graph;
mod rust_service_plan;
mod rust_web_module;
mod site;
mod site_map;
#[cfg(test)]
mod site_tests;
#[cfg(test)]
mod storage_tests;
mod style_source;
mod swift_web_services;
mod vendor_provenance;
mod web_program_build;
mod windows;
mod windows_architecture;
mod windows_compiler_plan;
#[cfg(test)]
mod windows_host_tests;

pub(crate) use browser_api_grants::BrowserApiGrants;
pub use compiler::{
    NativeBuildOptions, NativeBuildResult, NativeCompiler, native_target_directory,
};
pub(crate) use config::{TacMember, WebAppManifest};
pub(crate) use cpp_web_services::CppWebServices;
pub(crate) use csharp_web_apis::CsharpWebApis;
pub(crate) use go::{
    in_package as go_in_package, module_file as go_module_file,
    publisher_alias as go_publisher_alias, runtime_files as go_runtime_files, web_go,
};
pub(crate) use java_web_services::JavaWebServices;
pub(crate) use kotlin_web_services::KotlinWebServices;
pub use native_ui_change::NativeUiChange;
pub use native_ui_contract::NativeUiContract;
pub use native_ui_generation::NativeUiGeneration;
#[cfg(test)]
pub(crate) use rust::companion_prelude;
pub(crate) use rust::{RustCompanion, tac_value_source, web_assembly};
pub(crate) use site::{CacheRule, PageMetadata, SiteSettings};
pub(crate) use site_map::SiteMap;
pub(crate) use style_source::StyleSource;
pub(crate) use swift_web_services::SwiftWebServices;
pub(crate) use windows::web_dotnet;
pub use windows_architecture::WindowsArchitecture;

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

mod csharp_browser_bindings;
#[cfg(test)]
mod csharp_browser_client_tests;
mod csharp_browser_factory;
#[cfg(test)]
mod csharp_page_lifetime_tests;
mod csharp_stream_clients;
mod csharp_task_inventory;

#[cfg(test)]
mod csharp_browser_bindings_tests;
