use crate::Failure;
use crate::events::{EventModule, EventStream, EventType};
use crate::project::NativeCompanion;
use std::path::Path;

pub(super) struct SwiftStreams {
    pub(super) module: Option<EventModule>,
    pub(super) entries: String,
}

impl SwiftStreams {
    const MOST_STREAMS: usize = 16;
    const DECLARATIONS: [&'static str; 2] = ["let ", "var "];
    const MODIFIERS: [&'static str; 6] = [
        "private ",
        "fileprivate ",
        "internal ",
        "public ",
        "final ",
        "lazy ",
    ];
    const TOO_MANY: &'static str = "A Swift service holds at most 16 @StreamRequest and @SocketRequest methods through its clients.";
    const NATIVE: &'static str = "A Swift service in a native app cannot hold a @StreamRequest or @SocketRequest client. The browser page holds the stream for a Swift web service only; receive the messages with @OnEvent instead";

    pub(super) fn held(
        language: NativeCompanion,
        web_assembly: bool,
        module: Option<&EventModule>,
        (class, path): (&str, &Path),
    ) -> Result<Self, Failure> {
        let unchanged = Self {
            module: None,
            entries: String::new(),
        };
        let Some(module) = module.filter(|_| language == NativeCompanion::Swift) else {
            return Ok(unchanged);
        };
        let clients: Vec<&EventType> = module
            .types
            .iter()
            .filter(|event_type| Self::is_stream_client(event_type, class))
            .collect();
        if clients.is_empty() {
            return Ok(unchanged);
        }
        let held = Self::held_types(&module.source, class);
        let streams: Vec<(&str, &EventStream)> = clients
            .iter()
            .filter(|client| held.contains(&client.name))
            .flat_map(|client| {
                client
                    .streams
                    .iter()
                    .filter(|stream| !stream.topic)
                    .map(|stream| (client.name.as_str(), stream))
            })
            .collect();
        if [streams.is_empty(), web_assembly] == [false, false] {
            return Err(Self::failure(path, Self::NATIVE));
        }
        if streams.len() > Self::MOST_STREAMS {
            return Err(Self::failure(path, Self::TOO_MANY));
        }
        Ok(Self {
            module: Some(Self::without_clients(module, &clients)),
            entries: streams
                .into_iter()
                .map(|(client, stream)| Self::entry(client, stream))
                .collect(),
        })
    }

    fn is_stream_client(event_type: &EventType, class: &str) -> bool {
        [
            event_type.name != class,
            event_type.listeners.is_empty(),
            event_type.resources.is_empty(),
            event_type.post_construct.is_none(),
            event_type.pre_destroy.is_none(),
            !event_type.streams.is_empty(),
        ]
        .iter()
        .all(|holds| *holds)
    }

    fn without_clients(module: &EventModule, clients: &[&EventType]) -> EventModule {
        let is_client = |name: &str| clients.iter().any(|client| client.name == name);
        let mut kept = module.clone();
        kept.types.retain(|event_type| !is_client(&event_type.name));
        kept.methods.retain(|method| !is_client(&method.owner));
        kept
    }

    fn held_types(source: &str, class: &str) -> Vec<String> {
        let Some(body) = crate::companion::class_body(source, class) else {
            return Vec::new();
        };
        body.lines().filter_map(Self::property_type).collect()
    }

    fn property_type(line: &str) -> Option<String> {
        let mut rest = line.trim_start();
        while let Some(modifier) = Self::MODIFIERS
            .iter()
            .find(|modifier| rest.starts_with(**modifier))
        {
            rest = rest[modifier.len()..].trim_start();
        }
        let declared = Self::DECLARATIONS
            .iter()
            .find_map(|keyword| rest.strip_prefix(keyword))?;
        let (_, typed) = declared.split_once(':')?;
        let written = typed.split('=').next().unwrap_or_default();
        Some(written.trim().trim_end_matches(['!', '?']).to_owned())
    }

    fn entry(client: &str, stream: &EventStream) -> String {
        let models = stream.models.as_ref().map_or_else(
            || String::from("nil"),
            |models| Self::text(&models.to_string()),
        );
        format!(
            "\"__stream_{client}_{}\": .stream(TacStream({}, {}, {}, {}, {models})),\n",
            stream.method,
            Self::text(client),
            Self::text(&stream.path),
            Self::text(&stream.event),
            stream.socket,
        )
    }

    fn text(value: &str) -> String {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }

    fn failure(path: &Path, message: &str) -> Failure {
        super::host::native_tool_failure(1405, &format!("{}: {message}", path.display()))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::SwiftStreams;
    use crate::project::NativeCompanion;
    use std::path::Path;

    struct Fixtures;

    impl Fixtures {
        const PATH: &'static str = "client/services/TicksService.swift";
        const CLIENT: &'static str = "@HTTPClient(\"/ticks\")\nfinal class TicksClient {\n    @StreamRequest\n    func receive(_ tick: Tick) {}\n}\n\n";

        fn held(service: &str, web_assembly: bool) -> Result<SwiftStreams, crate::Failure> {
            let source = format!("{}{service}", Self::CLIENT);
            let path = Path::new(Self::PATH);
            let module = crate::events::extract(path, &source, "tac").expect("events");
            let class = if service.contains("QuietService") {
                "QuietService"
            } else {
                "TicksService"
            };
            SwiftStreams::held(
                NativeCompanion::Swift,
                web_assembly,
                module.as_ref(),
                (class, path),
            )
        }
    }

    #[test]
    fn a_held_stream_client_becomes_a_stream_entry_and_leaves_the_module() {
        let service = "@Service\nfinal class TicksService {\n    private let ticks: TicksClient = TicksClient()\n    var level: Int = 0\n\n    @OnEvent\n    func heard(_ tick: Tick) {\n        level = tick.level\n    }\n}\n";
        let held = Fixtures::held(service, true).expect("held");
        assert_eq!(
            held.entries,
            "\"__stream_TicksClient_receive\": .stream(TacStream(\"TicksClient\", \"/ticks\", \"Tick\", false, nil)),\n"
        );
        let module = held.module.expect("module without the client");
        let names: Vec<&str> = module.types.iter().map(|kind| kind.name.as_str()).collect();
        assert_eq!(names, ["TicksService"]);
        assert!(
            module
                .methods
                .iter()
                .all(|method| method.owner == "TicksService")
        );
    }

    #[test]
    fn an_unheld_stream_client_opens_nothing() {
        let service = "@Service\nfinal class QuietService {\n    var count: Int = 0\n}\n";
        let held = Fixtures::held(service, false).expect("unheld");
        assert!(held.entries.is_empty(), "{}", held.entries);
    }

    #[test]
    fn a_native_swift_service_cannot_hold_a_stream_client() {
        let service = "@Service\nfinal class TicksService {\n    private let ticks: TicksClient = TicksClient()\n    var level: Int = 0\n}\n";
        let Err(failure) = Fixtures::held(service, false) else {
            panic!("a native service holds no browser stream");
        };
        assert!(
            failure.to_string().contains("cannot hold a @StreamRequest"),
            "{failure}"
        );
    }
}
