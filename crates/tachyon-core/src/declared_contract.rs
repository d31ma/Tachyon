mod concrete_contract;
mod constructor_parameter;
mod context_members;
mod context_template;
mod contract_check;
mod contract_declaration;
mod contract_form;
mod contract_member;
mod contract_rewrite;
mod contract_role;
mod converter_model_table;
mod converter_models;
mod crud_members;
mod crud_template;
mod fsharp_class_type;
mod fsharp_contract;
mod fsharp_contract_rewrite;
mod fsharp_declaration;
mod fsharp_injected_contract;
mod go_contract_rewrite;
mod go_method;
mod indented_members;
mod indented_syntax;
mod injected_contract;
mod injected_field;
mod lua_contract;
mod lua_converter_models;
mod marker_line;
mod member_indent;
mod member_scanner;
mod repository_base;
mod rust_contract_rewrite;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod converter_tests;
#[cfg(test)]
#[allow(clippy::expect_used)]
mod crud_tests;
#[cfg(test)]
mod csharp_tac_context_tests;
#[cfg(test)]
#[allow(clippy::expect_used)]
mod dart_tac_context_tests;
#[cfg(test)]
#[allow(clippy::expect_used)]
mod store_tests;
#[cfg(test)]
#[allow(clippy::expect_used)]
mod tac_injection_tests;
#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;

use crate::Failure;
use concrete_contract::ConcreteContract;
use contract_check::ContractCheck;
use contract_rewrite::ContractRewrite;
pub(crate) use contract_rewrite::SourceEdit;
pub(crate) use converter_model_table::ConverterModelTable;
use fsharp_contract::FsharpContract;
use fsharp_contract_rewrite::FsharpContractRewrite;
use fsharp_injected_contract::FsharpInjectedContract;
use injected_contract::InjectedContract;
use lua_contract::LuaContract;
use std::path::Path;

pub(crate) struct DeclaredContract;

impl DeclaredContract {
    pub(crate) const STORE_CONTEXT: &'static str = "tachyonContext";
    pub(crate) const GO_NEEDS_START: &'static str = InjectedContract::GO_START;

    pub(crate) fn check(path: &Path, source: &str) -> Result<(), Failure> {
        ContractCheck::check(path, source)
    }

    pub(crate) fn rewrite(path: &Path, source: &str) -> Result<Vec<SourceEdit>, String> {
        FsharpContractRewrite::edits(path, source)
            .unwrap_or_else(|| ContractRewrite::edits(path, source))
    }

    pub(crate) fn injected_names<'p>(paths: impl Iterator<Item = &'p Path>) -> Vec<String> {
        InjectedContract::injected_names(paths)
    }

    pub(crate) fn is_kotlin_helper(path: &Path) -> bool {
        InjectedContract::kotlin_helper(path)
    }

    pub(crate) fn injected_remotes<'p>(paths: impl Iterator<Item = &'p Path>) -> Vec<String> {
        InjectedContract::remote_names(paths)
    }

    #[cfg(test)]
    pub(crate) fn injected(path: &Path, source: &str, contracts: &[String]) -> Option<String> {
        Self::injected_with_remotes(path, source, contracts, &[])
    }

    pub(crate) fn injected_with_remotes(
        path: &Path,
        source: &str,
        contracts: &[String],
        remotes: &[String],
    ) -> Option<String> {
        FsharpInjectedContract::filled(path, source, contracts)
            .or_else(|| InjectedContract::filled(path, source, contracts, remotes))
    }

    pub(crate) fn concrete(path: &Path, source: &str, environment: &str) -> Option<String> {
        FsharpContract::lower(path, source, environment)
            .or_else(|| LuaContract::lower(path, source, environment))
            .or_else(|| ConcreteContract::lower(path, source, environment))
    }
}
