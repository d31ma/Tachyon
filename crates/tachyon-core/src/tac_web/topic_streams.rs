use crate::Failure;
use crate::failure::diagnostic;
use crate::route_types::ModelTable;
use serde_json::Value;
use std::path::Path;

pub(super) struct TopicStreams;

impl TopicStreams {
    const REALM: &'static str = "client";
    const UNREADABLE_MODELS: u16 = 1605;
    const TOPIC_PATH: &'static str = "/.tachyon/topics/";

    pub(super) fn read(snapshot_root: &Path, extension: &str) -> Result<Value, Failure> {
        if extension == crate::tac_cpp::CppSource::EXTENSION {
            let names = crate::tac_cpp::CppModel::topics(snapshot_root)?;
            return Ok(Self::paths(&names));
        }
        let models = ModelTable::read_realm(snapshot_root, Self::REALM, extension)
            .map_err(|message| Self::unreadable(&message))?;
        Ok(Self::paths(&models.topic_names()))
    }

    fn unreadable(message: &str) -> Failure {
        Failure::one(diagnostic(Self::UNREADABLE_MODELS, message, None, None))
    }

    fn paths(names: &[String]) -> Value {
        Value::Object(
            names
                .iter()
                .map(String::as_str)
                .filter_map(Self::entry)
                .collect(),
        )
    }

    fn entry(name: &str) -> Option<(String, Value)> {
        let slug = crate::topics::Topics::slug(name)?;
        let path = format!("{}{slug}", Self::TOPIC_PATH);
        Some((name.to_owned(), Value::String(path)))
    }
}
