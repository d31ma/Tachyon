use super::{Input, Marker, WrittenField, WrittenMarker, WrittenQuery};
use crate::Failure;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) struct WrittenPolicies<'a> {
    input: Input<'a>,
    language: &'a str,
}

impl<'a> WrittenPolicies<'a> {
    pub(crate) fn new(path: &'a Path, source: &'a str, language: &'a str) -> Self {
        Self {
            input: Input {
                path,
                source,
                code: String::new(),
            },
            language,
        }
    }

    pub(crate) fn cache(&self, written: &WrittenMarker<'_>) -> Result<Value, Failure> {
        super::cache_config(&self.input, &Self::marker(written))
    }

    pub(crate) fn method(
        &self,
        written: &WrittenMarker<'_>,
        parameter_count: usize,
    ) -> Result<(&'static str, Value), Failure> {
        self.chosen(written, parameter_count)
    }

    fn chosen(
        &self,
        written: &WrittenMarker<'_>,
        parameter_count: usize,
    ) -> Result<(&'static str, Value), Failure> {
        let marker = Self::marker(written);
        match written.name {
            "Cached" => Ok((
                "cacheable",
                super::cacheable(&self.input, &marker, parameter_count)?,
            )),
            "ClearCache" => Ok((
                "cacheEvict",
                super::mutations::policy(&self.input, &marker)?,
            )),
            "UpdateCache" => Ok(("cachePut", super::mutations::policy(&self.input, &marker)?)),
            _ => super::resilience_policy(&self.input, self.language, &marker),
        }
    }

    pub(crate) fn page(&self, written: &WrittenMarker<'_>, index: usize) -> Result<Value, Failure> {
        self.page_policy(written, index)
    }

    fn page_policy(&self, written: &WrittenMarker<'_>, index: usize) -> Result<Value, Failure> {
        let marker = Self::marker(written);
        match super::parameters::policy(&self.input, &marker, index, "")? {
            super::parameters::ParameterPolicy::Page(value) => Ok(value),
            _ => Err(self.input.failure(written.at, "PageDefaults takes one JSON object, for example [[PageDefaults({\"defaultSize\":20,\"maxSize\":100,\"sort\":[\"id\"]})]]")),
        }
    }

    pub(crate) fn store(&self, written: &WrittenMarker<'_>) -> Result<Value, Failure> {
        super::queries::repository(&self.input, &Self::marker(written))
    }

    pub(crate) fn query(&self, written: &WrittenMarker<'_>) -> Result<Value, Failure> {
        super::queries::explicit(&self.input, &Self::marker(written))
    }

    pub(crate) fn entity(&self, written: &WrittenMarker<'_>) -> Result<Value, Failure> {
        super::entity(&self.input, &Self::marker(written))
    }

    pub(crate) fn table(&self, written: &WrittenMarker<'_>) -> Result<String, Failure> {
        super::single_string(&self.input, &Self::marker(written))
    }

    pub(crate) fn resolve_query(
        &self,
        store: &Value,
        entity: &Value,
        written: &WrittenQuery<'_>,
    ) -> Result<Value, Failure> {
        super::queries::resolve(
            &self.input,
            written.at,
            store,
            entity["name"].as_str().unwrap_or_default(),
            entity,
            entity["table"].as_str().unwrap_or_default(),
            written.method,
            written.arity,
            written.query,
            written.page,
        )
    }

    pub(crate) fn settings(
        &self,
        written: &WrittenMarker<'_>,
        fields: &[WrittenField<'_>],
        bounds: Map<String, Value>,
    ) -> Result<Value, Failure> {
        let marker = Self::marker(written);
        let mut policies = Map::new();
        policies.insert(
            String::from("configurationProperties"),
            super::configuration::properties(&self.input, &marker)?,
        );
        policies.insert(String::from("settingsBounds"), Value::Object(bounds));
        let fields: Vec<super::configuration::SettingsField> = fields
            .iter()
            .map(|field| super::configuration::SettingsField {
                name: field.name.to_owned(),
                type_source: field.type_source.to_owned(),
                initializer: field.initializer.map(str::to_owned),
                is_static: false,
            })
            .collect();
        super::configuration::derive(&self.input, written.at, &mut policies, &fields)?;
        Ok(policies
            .remove("configurationProperties")
            .unwrap_or(Value::Null))
    }

    pub(crate) fn setting(&self, written: &WrittenMarker<'_>) -> Result<Value, Failure> {
        super::configuration::value(&self.input, &Self::marker(written))
    }

    pub(crate) fn resolve_mutations(
        &self,
        at: usize,
        methods: &mut BTreeMap<String, Map<String, Value>>,
    ) -> Result<(), Failure> {
        super::mutations::resolve(&self.input, at, methods)
    }

    pub(crate) fn failure(&self, at: usize, message: &str) -> Failure {
        self.input.failure(at, message.to_owned())
    }

    fn marker(written: &WrittenMarker<'_>) -> Marker {
        Marker {
            name: written.name.to_owned(),
            arguments: written.arguments.to_owned(),
            span: written.at..written.at + 1,
            target: written.at,
            parameter: false,
        }
    }
}
