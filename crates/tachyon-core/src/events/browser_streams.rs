use super::{Marker, failure, language_of, scan};
use crate::Failure;
use crate::annotation_rules::AnnotationRules;
use std::path::Path;

pub(crate) struct BrowserStreams;

impl BrowserStreams {
    const NAMES: [&'static str; 2] = ["StreamRequest", "SocketRequest"];
    const HOLDER_LANGUAGES: [&'static str; 4] = ["js", "ts", "go", "swift"];
    const BROWSER: &'static str = "tac";

    pub(crate) fn held_by(language: &str, environment: &str) -> bool {
        let written_in_script = Self::HOLDER_LANGUAGES.contains(&language);
        [written_in_script, environment == Self::BROWSER] == [true, true]
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
