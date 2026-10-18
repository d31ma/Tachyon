use super::prepared::Prepared;
use super::{Field, Module, Type};
use crate::Failure;
use crate::behavior::HttpClientSource;
use std::collections::BTreeSet;
use std::path::Path;

pub(crate) struct CsharpBrowserConstruction;

impl CsharpBrowserConstruction {
    const MAX_DEPENDENCIES: usize = 5;

    pub(crate) fn factory(
        path: &Path,
        source: &str,
        class: &str,
    ) -> Result<Option<String>, Failure> {
        let rewritten =
            HttpClientSource::rewrite(path, source)?.unwrap_or_else(|| source.to_owned());
        let rewritten = crate::behavior::ServiceRoles::read(path, &rewritten)?.source;
        let prepared = super::prepare(path, &rewritten, "tac")?;
        Self::prepared(prepared.as_ref(), source, class, path)
            .map_err(|message| super::module_failure(path, source, message))
    }

    fn prepared(
        prepared: Option<&Prepared>,
        source: &str,
        class: &str,
        path: &Path,
    ) -> Result<Option<String>, String> {
        let module = Self::module(prepared);
        let target = Self::target(module, class);
        target
            .map(|(module, target)| Self::creation(module, target, (source, path)))
            .transpose()
    }

    fn module(prepared: Option<&Prepared>) -> Option<&Module> {
        match prepared {
            Some(Prepared::Annotated(module)) => Some(module),
            _ => None,
        }
    }

    fn target<'a>(module: Option<&'a Module>, class: &str) -> Option<(&'a Module, &'a Type)> {
        module.and_then(|module| {
            module
                .types
                .iter()
                .find(|target| target.name == class)
                .map(|target| (module, target))
        })
    }

    fn creation(module: &Module, target: &Type, captured: (&str, &Path)) -> Result<String, String> {
        let fields = Self::fields(target)?;
        Self::bounded(&fields)?;
        let providers = Self::providers(&fields);
        let creations = providers
            .iter()
            .enumerate()
            .map(|(index, kind)| Self::provider(module, captured, kind, index))
            .collect::<Result<Vec<_>, _>>()?;
        let arguments = Self::arguments(&fields, &providers)?;
        Ok(Self::source(&target.name, &arguments, &creations))
    }

    fn fields(target: &Type) -> Result<Vec<&Field>, String> {
        target
            .constructor
            .iter()
            .flatten()
            .map(|name| {
                target
                    .fields
                    .iter()
                    .find(|field| field.name == *name)
                    .ok_or_else(|| {
                        format!("C# browser Needs constructor field {name} has no declared type.")
                    })
            })
            .collect()
    }

    fn providers<'a>(fields: &[&'a Field]) -> Vec<&'a str> {
        fields
            .iter()
            .map(|field| field.type_source.trim())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn bounded(fields: &[&Field]) -> Result<(), String> {
        if fields.len() > Self::MAX_DEPENDENCIES {
            return Err(String::from(
                "A C# browser Service permits at most 5 injected dependencies; split larger responsibilities.",
            ));
        }
        Ok(())
    }

    fn provider(
        module: &Module,
        captured: (&str, &Path),
        kind: &str,
        index: usize,
    ) -> Result<String, String> {
        Self::canonical(captured, kind)?;
        Self::declared(module, kind)?.map_or(Ok(()), Self::zero_argument)?;
        Ok(format!("var __tacClient{index} = new {kind}();"))
    }

    fn canonical((source, path): (&str, &Path), kind: &str) -> Result<(), String> {
        HttpClientSource::class_path(path, source, kind)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("C# browser Needs dependency {kind} requires an assembled canonical HTTPClient; other managed providers remain unsupported."))
            .map(|_| ())
    }

    fn declared<'a>(module: &'a Module, kind: &str) -> Result<Option<&'a Type>, String> {
        let declarations = module
            .types
            .iter()
            .filter(|declared| declared.name == kind)
            .collect::<Vec<_>>();
        Self::unique(&declarations, kind)
    }

    fn unique<'a>(declarations: &[&'a Type], kind: &str) -> Result<Option<&'a Type>, String> {
        match declarations {
            [declared] => Ok(Some(declared)),
            [] => Ok(None),
            _ => Err(format!(
                "C# browser HTTPClient {kind} has ambiguous same-language declarations."
            )),
        }
    }

    fn zero_argument(declared: &Type) -> Result<(), String> {
        if declared
            .constructor
            .as_ref()
            .is_some_and(|fields| !fields.is_empty())
        {
            return Err(format!(
                "C# browser HTTPClient {} requires its generated zero-argument Setup context; injected client constructor dependencies remain unsupported.",
                declared.name
            ));
        }
        Ok(())
    }

    fn arguments(fields: &[&Field], providers: &[&str]) -> Result<String, String> {
        fields
            .iter()
            .map(|field| {
                providers
                    .iter()
                    .position(|kind| *kind == field.type_source.trim())
                    .map(|index| format!("__tacClient{index}"))
                    .ok_or_else(|| {
                        format!(
                            "C# browser Needs dependency {} has no captured provider.",
                            field.name
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|values| values.join(", "))
    }

    fn source(class: &str, arguments: &str, creations: &[String]) -> String {
        format!(
            "{}\nvar Instance = new {class}({arguments})",
            creations.join("\n")
        )
    }
}
