use super::registry::NativeCompanionInput;
use crate::Failure;
use crate::project::NativeCompanion;
use std::path::{Path, PathBuf};
use swift_wasm_program::SwiftWasmProgram;

mod swift_wasm_program;

pub(crate) struct SwiftWebServices;

impl SwiftWebServices {
    const PRELUDE: &'static str = concat!(
        include_str!("prelude.swift"),
        include_str!("TacServicePages.swift"),
        include_str!("prelude_web.swift"),
        include_str!("swift_web_services/TacJSONReader.swift"),
        include_str!("swift_web_services/TacValueDecoder.swift"),
        include_str!("swift_web_services/TacSingleValueDecoder.swift"),
        include_str!("swift_web_services/TacKeyedValueDecoder.swift"),
        include_str!("swift_web_services/TacListValueDecoder.swift"),
        include_str!("swift_web_services/TacEncodedNode.swift"),
        include_str!("swift_web_services/TacValueEncoder.swift"),
        include_str!("swift_web_services/TacSingleValueEncoder.swift"),
        include_str!("swift_web_services/TacKeyedValueEncoder.swift"),
        include_str!("swift_web_services/TacListValueEncoder.swift"),
        include_str!("swift_web_services/TacWebStorage.swift"),
        include_str!("swift_web_services/TacPage.swift"),
        include_str!("swift_web_services/TacWebHost.swift"),
        include_str!("swift_web_services/TacBrowserError.swift"),
        include_str!("swift_web_services/TacRunningTask.swift"),
        include_str!("swift_web_services/TacTasks.swift"),
        include_str!("swift_web_services/TacHttpTickets.swift"),
        include_str!("swift_web_services/TacSocketSend.swift"),
        include_str!("TacAwaited.swift")
    );
    const TEST_UNIT_PAGE: &'static str = concat!(
        include_str!("swift_test_units/TacPage.swift"),
        include_str!("swift_test_units/BrowserFakes.swift"),
        include_str!("swift_web_services/TacWebHost.swift"),
        include_str!("swift_web_services/TacBrowserError.swift")
    );
    const WEB_APIS: [(&'static str, &'static str); 11] = [
        ("Clock", include_str!("web_apis/Clock.swift")),
        ("SecureRandom", include_str!("web_apis/SecureRandom.swift")),
        ("Connectivity", include_str!("web_apis/Connectivity.swift")),
        (
            "PageVisibility",
            include_str!("web_apis/PageVisibility.swift"),
        ),
        ("Preferences", include_str!("web_apis/Preferences.swift")),
        ("Clipboard", include_str!("web_apis/Clipboard.swift")),
        ("Share", include_str!("web_apis/Share.swift")),
        (
            "Notifications",
            include_str!("web_apis/Notifications.swift"),
        ),
        ("Geolocation", include_str!("web_apis/Geolocation.swift")),
        (
            "WindowControls",
            include_str!("web_apis/WindowControls.swift"),
        ),
        (
            "FilePicker",
            concat!(
                include_str!("capabilities/FilePickerStatus.swift"),
                include_str!("capabilities/FilePickerResult.swift"),
                include_str!("capabilities/FilePickerOptions.swift"),
                include_str!("web_apis/FilePicker.swift")
            ),
        ),
    ];
    const BEHAVIOR_PLATFORM: &'static str = concat!(
        include_str!("swift_web_services/BehaviorPlatform.swift"),
        include_str!("swift_web_services/BehaviorLock.swift"),
        include_str!("swift_web_services/BehaviorBrowserStorage.swift"),
        include_str!("swift_web_services/BehaviorTextPattern.swift"),
        include_str!("swift_web_services/BehaviorNumberText.swift"),
        include_str!("swift_web_services/BehaviorBrowserHttp.swift")
    );

    pub(crate) fn sdk_installed() -> bool {
        SwiftWasmProgram::sdk().is_ok()
    }

    pub(crate) fn compiler() -> std::ffi::OsString {
        SwiftWasmProgram::compiler()
    }

    pub(crate) async fn build(
        snapshot_root: &Path,
        services: &[(String, String, PathBuf)],
        profile: crate::BuildProfile,
    ) -> Result<(Vec<(String, Vec<u8>)>, Option<String>), Failure> {
        let companions =
            NativeCompanionInput::web_services(snapshot_root, NativeCompanion::Swift, services);
        let authored = super::registry::source(&companions, NativeCompanion::Swift)?;
        Self::compiled(authored, profile).await
    }

    async fn compiled(
        authored: Option<String>,
        profile: crate::BuildProfile,
    ) -> Result<(Vec<(String, Vec<u8>)>, Option<String>), Failure> {
        let Some(authored) = authored else {
            return Ok((Vec::new(), None));
        };
        let authored =
            crate::behavior::SwiftPlatform::in_browser(&authored, Self::BEHAVIOR_PLATFORM);
        let program = format!(
            "{authored}\n{}{}{}",
            Self::PRELUDE,
            Self::used_web_apis(&authored),
            crate::route_types::WebEventSources::swift_used_by(&authored)
        );
        let built = SwiftWasmProgram::compile(&program, profile).await?;
        let note = built.note(profile);
        Ok((built.published(), note))
    }

    pub(crate) fn test_unit_browser(unit: &str) -> String {
        let apis = Self::used_web_apis(unit);
        let page = Self::test_unit_page(unit, &apis);
        let models = crate::route_types::WebEventSources::swift_used_by(unit);
        format!("{page}{apis}{models}")
    }

    fn test_unit_page(unit: &str, apis: &str) -> &'static str {
        let unused = [apis.is_empty(), unit.contains("BrowserFakes")] == [true, false];
        if unused { "" } else { Self::TEST_UNIT_PAGE }
    }

    fn used_web_apis(authored: &str) -> String {
        Self::WEB_APIS
            .iter()
            .filter(|(name, _)| authored.contains(name))
            .map(|(_, source)| *source)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::SwiftWebServices;

    fn runtime_patterns() -> Vec<String> {
        let runtime = include_str!("../behavior/polyglot/swift_runtime.swift");
        let mut patterns = Vec::new();
        for opening in ["\"[", "\"-?("] {
            for (start, _) in runtime.match_indices(opening) {
                let rest = &runtime[start + 1..];
                let end = rest.find('"').unwrap_or(0);
                if end > 1 {
                    patterns.push(format!("\"{}\"", &rest[..end]));
                }
            }
        }
        patterns
    }

    #[test]
    fn a_program_holds_only_the_browser_apis_and_event_models_it_names() {
        let authored =
            "private let clock = Clock()\nfunc heard(_ changed: ConnectivityChanged) {}\n";
        let apis = SwiftWebServices::used_web_apis(authored);
        assert!(apis.contains("final class Clock"), "{apis}");
        assert!(!apis.contains("final class Geolocation"), "{apis}");
        let models = crate::route_types::WebEventSources::swift_used_by(authored);
        assert_eq!(
            models,
            "\nstruct ConnectivityChanged: Codable {\n    let online: Bool\n}\n"
        );
    }

    #[test]
    fn the_web_pattern_matcher_knows_every_runtime_pattern() {
        let patterns = runtime_patterns();
        assert!(patterns.len() >= 6, "{patterns:?}");
        for pattern in patterns {
            assert!(
                SwiftWebServices::BEHAVIOR_PLATFORM.contains(&pattern),
                "the web BehaviorTextPattern does not know {pattern}"
            );
        }
    }
}
