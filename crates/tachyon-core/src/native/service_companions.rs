use super::registry::NativeCompanionInput;
use crate::project::NativeCompanion;
use std::fmt::Write as _;

pub(super) struct ServiceCompanions;

impl ServiceCompanions {
    pub(super) fn number(language: NativeCompanion) -> u8 {
        match language {
            NativeCompanion::Rust => 1,
            NativeCompanion::CSharp => 2,
            _ => 0,
        }
    }

    pub(super) fn c_table(companions: &[NativeCompanionInput]) -> String {
        let mut rows = String::new();
        for item in companions {
            let language = Self::number(item.language);
            if language == 0 || item.web_assembly || item.application_provider {
                continue;
            }
            let service = super::host::c_string_escape(&item.service);
            let _ = writeln!(rows, "  {{\"{service}\", {language}}},");
        }
        include_str!("service_companions.h").replace("__SERVICE_COMPANIONS__", &rows)
    }
}
