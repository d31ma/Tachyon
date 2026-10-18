use super::prepared::Prepared;
use super::{Method, Module};
use crate::Failure;
use crate::companion::CsharpTaskResult;
use crate::route_types::ModelTable;
use std::path::Path;

pub(crate) struct CsharpTaskModels;

impl CsharpTaskModels {
    pub(crate) fn lower(
        path: &Path,
        source: &str,
        models: &ModelTable,
    ) -> Result<Option<String>, Failure> {
        Self::prepared(path, source, models)
    }

    pub(crate) fn codec() -> &'static str {
        include_str!("../../native/models/TacModels.cs")
    }

    fn prepared(path: &Path, source: &str, models: &ModelTable) -> Result<Option<String>, Failure> {
        let rewritten = crate::behavior::HttpClientSource::rewrite(path, source)?;
        let written = rewritten.as_deref().unwrap_or(source);
        let roles = crate::behavior::ServiceRoles::read(path, written)?.source;
        let lowered = super::prepare(path, &roles, "tac")?
            .map(|prepared| {
                Self::lower_prepared(prepared, models)
                    .map_err(|message| super::module_failure(path, source, message))
            })
            .transpose()?;
        lowered.map_or_else(
            || Self::unheld_streams(path, source, rewritten.is_some()),
            |text| Ok(Some(text)),
        )
    }

    fn unheld_streams(path: &Path, source: &str, clients: bool) -> Result<Option<String>, Failure> {
        if !clients {
            return Ok(None);
        }
        crate::behavior::HttpClientSource::without_clients(path, source)
    }

    fn lower_prepared(prepared: Prepared, models: &ModelTable) -> Result<String, String> {
        match prepared {
            Prepared::Erased(erased) => Ok(erased),
            Prepared::Annotated(mut module) => {
                Self::describe(&mut module, models)?;
                super::csharp::lower(&module)
            }
        }
    }

    fn describe(module: &mut Module, models: &ModelTable) -> Result<(), String> {
        for declared in &mut module.types {
            Self::describe_methods(&mut declared.methods, models)?;
        }
        Ok(())
    }

    fn describe_methods(methods: &mut [Method], models: &ModelTable) -> Result<(), String> {
        for method in methods {
            Self::describe_method(method, models)?;
        }
        Ok(())
    }

    fn describe_method(method: &mut Method, models: &ModelTable) -> Result<(), String> {
        (!method.metadata.is_empty())
            .then_some(method)
            .map(|selected| Self::describe_result(selected, models))
            .transpose()
            .map(|_| ())
    }

    fn describe_result(method: &mut Method, models: &ModelTable) -> Result<(), String> {
        CsharpTaskResult::model(&method.return_type, models)?
            .map(|name| Self::describe_model(method, models, &name))
            .transpose()
            .map(|_| ())
    }

    fn describe_model(method: &mut Method, models: &ModelTable, name: &str) -> Result<(), String> {
        Self::require_exchange(method)?;
        method.metadata.insert(
            "tacModelResult".to_owned(),
            serde_json::json!({"name": name, "models": models.event_models(name)?}),
        );
        Ok(())
    }

    fn require_exchange(method: &Method) -> Result<(), String> {
        if !method.asynchronous
            || method
                .metadata
                .get("httpExchange")
                .and_then(|exchange| exchange.get("generated"))
                .is_none()
        {
            return Err("C# annotated Task<model> currently requires a generated HTTPClient exchange; other DTO behaviors remain unsupported.".to_owned());
        }
        Ok(())
    }
}
