use crate::Failure;
use crate::events::{EventModule, EventStream, EventType};
use crate::template::RequiredFields;
use std::path::Path;

pub(super) struct KotlinStreamClients;

impl KotlinStreamClients {
    const MOST_STREAMS: usize = 16;
    const PROPERTY_WORDS: [&'static str; 2] = ["val", "var"];
    const MODIFIERS: [&'static str; 8] = [
        "private",
        "public",
        "internal",
        "protected",
        "lateinit",
        "override",
        "open",
        "final",
    ];

    pub(super) fn holds_only_streams(event_type: &EventType) -> bool {
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

    pub(super) fn is_client_of(event_type: &EventType, owner: &str) -> bool {
        [
            event_type.name != owner,
            Self::holds_only_streams(event_type),
        ] == [true, true]
    }

    pub(super) fn has_streams(module: &EventModule) -> bool {
        module
            .types
            .iter()
            .any(|event_type| !event_type.streams.is_empty())
    }

    pub(super) fn entries(
        module: &EventModule,
        owner: &str,
        path: &Path,
    ) -> Result<String, Failure> {
        let held = Self::held_types(module, owner, path);
        let streams: Vec<(&str, &EventStream)> = module
            .types
            .iter()
            .filter(|event_type| Self::is_client_of(event_type, owner))
            .filter(|client| held.contains(&client.name))
            .flat_map(|client| {
                client
                    .streams
                    .iter()
                    .filter(|stream| !stream.topic)
                    .map(|stream| (client.name.as_str(), stream))
            })
            .collect();
        let within_limit = streams.len() <= Self::MOST_STREAMS;
        within_limit
            .then(|| {
                streams
                    .into_iter()
                    .map(|(client, stream)| Self::entry(client, stream))
                    .collect()
            })
            .ok_or_else(|| Self::too_many_streams(path))
    }

    fn too_many_streams(path: &Path) -> Failure {
        super::host::native_tool_failure(
            1405,
            &format!(
                "{}: A Kotlin service holds at most 16 @StreamRequest and @SocketRequest methods through its clients.",
                path.display()
            ),
        )
    }

    fn held_types(module: &EventModule, owner: &str, path: &Path) -> Vec<String> {
        let code =
            crate::annotation_layout::masked_source(path, &module.source).unwrap_or_default();
        let body = RequiredFields::class_body(&code, owner).map_or("", |(_, body)| body);
        let mut depth = 0usize;
        let mut held = Vec::new();
        for line in body.lines() {
            held.extend((depth == 0).then(|| Self::property_type(line)).flatten());
            let opened = line.matches('{').count();
            let closed = line.matches('}').count();
            depth = (depth + opened).saturating_sub(closed);
        }
        held
    }

    fn property_type(line: &str) -> Option<String> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let keyword = words
            .iter()
            .position(|word| Self::PROPERTY_WORDS.contains(word))?;
        let modified = words
            .iter()
            .take(keyword)
            .all(|word| Self::MODIFIERS.contains(word));
        let declared = words.get(keyword + 1..)?.join(" ");
        let (_, typed) = declared.split_once(':')?;
        let type_name = typed.split(['=', '{']).next()?.trim();
        modified.then(|| type_name.trim_end_matches('?').to_owned())
    }

    fn entry(client: &str, stream: &EventStream) -> String {
        let key = format!("__stream_{client}_{}", stream.method);
        let models = stream.models.as_ref().map_or_else(
            || String::from("null"),
            |models| Self::text(&models.to_string()),
        );
        format!(
            "\"{key}\" to TacStream({}, {}, {}, {}, {models}),\n",
            Self::text(client),
            Self::text(&stream.path),
            Self::text(&stream.event),
            stream.socket
        )
    }

    fn text(value: &str) -> String {
        super::events::string_literal(crate::project::NativeCompanion::Kotlin, value)
    }
}
