use super::{EventModule, EventStream, EventType, Marker, failure, language_of, scan};
use crate::Failure;
use crate::annotation_rules::AnnotationRules;
use std::path::Path;

pub(crate) struct BrowserStreams;

impl BrowserStreams {
    const NAMES: [&'static str; 2] = ["StreamRequest", "SocketRequest"];
    const HOLDER_LANGUAGES: [&'static str; 4] = ["js", "ts", "go", "cs"];
    const BROWSER: &'static str = "tac";

    pub(crate) fn held_by(language: &str, environment: &str) -> bool {
        let written_in_script = Self::HOLDER_LANGUAGES.contains(&language);
        [written_in_script, environment == Self::BROWSER] == [true, true]
    }

    pub(crate) fn holds_only_streams(event_type: &EventType) -> bool {
        [
            event_type.listeners.is_empty(),
            event_type.resources.is_empty(),
            event_type.post_construct.is_none(),
            event_type.pre_destroy.is_none(),
            !event_type.streams.is_empty(),
        ]
        .iter()
        .all(|holds| *holds)
    }

    pub(crate) fn clients<'a>(module: &'a EventModule, class: &str) -> Vec<&'a EventType> {
        module
            .types
            .iter()
            .filter(|event_type| Self::is_client(event_type, class))
            .collect()
    }

    pub(crate) fn held<'a>(
        clients: &[&'a EventType],
        held: &[String],
    ) -> Vec<(&'a str, &'a EventStream)> {
        clients
            .iter()
            .filter(|client| held.contains(&client.name))
            .flat_map(|client| {
                client
                    .streams
                    .iter()
                    .filter(|stream| !stream.topic)
                    .map(|stream| (client.name.as_str(), stream))
            })
            .collect()
    }

    fn is_client(event_type: &EventType, class: &str) -> bool {
        [
            event_type.name != class,
            Self::holds_only_streams(event_type),
        ] == [true, true]
    }

    pub(crate) fn refusal(name: &str, language: &str, environment: &str) -> String {
        AnnotationRules::unavailable(name, language, environment)
            .unwrap_or_else(|| AnnotationRules::belongs(name))
    }

    pub(crate) fn refuse_outside_script(path: &Path, source: &str) -> Result<(), Failure> {
        let language = language_of(path);
        Self::stream_outside_script(source, language).map_or(Ok(()), |marker| {
            let refusal = Self::refusal(&marker.name, language, Self::BROWSER);
            Err(failure(path, marker.span.start, refusal))
        })
    }

    fn stream_outside_script(source: &str, language: &str) -> Option<Marker> {
        let outside_script = !Self::held_by(language, Self::BROWSER);
        let markers = outside_script.then(|| scan(source, language).markers)?;
        markers
            .into_iter()
            .find(|marker| Self::NAMES.contains(&marker.name.as_str()))
    }
}
