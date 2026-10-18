use super::{Input, Marker};
use crate::Failure;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) struct WrittenMarker<'m> {
    pub(crate) name: &'m str,
    pub(crate) arguments: &'m str,
    pub(crate) at: usize,
}

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
        let marker = Self::marker(written);
        match super::parameters::policy(&self.input, &marker, index, "")? {
            super::parameters::ParameterPolicy::Page(value) => Ok(value),
            _ => Err(self.input.failure(written.at, "PageDefaults takes one JSON object, for example [[PageDefaults({\"defaultSize\":20,\"maxSize\":100,\"sort\":[\"id\"]})]]")),
        }
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
