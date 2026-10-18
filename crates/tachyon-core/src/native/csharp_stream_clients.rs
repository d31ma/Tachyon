use super::registry::NativeCompanionInput;
use crate::Failure;
use crate::events::{BrowserStreams, EventModule, EventStream};
use crate::project::NativeCompanion;
use std::path::Path;

pub(super) struct CsharpStreamClients;

impl CsharpStreamClients {
    const MOST_STREAMS: usize = 16;

    pub(super) fn take(
        item: &NativeCompanionInput,
        language: NativeCompanion,
        module: Option<&mut EventModule>,
        path: &Path,
    ) -> Result<String, Failure> {
        module
            .filter(|_| language == NativeCompanion::CSharp)
            .map_or_else(
                || Ok(String::new()),
                |module| Self::taken(item, module, path),
            )
    }

    fn taken(
        item: &NativeCompanionInput,
        module: &mut EventModule,
        path: &Path,
    ) -> Result<String, Failure> {
        let names: Vec<String> = BrowserStreams::clients(module, &item.class)
            .iter()
            .map(|client| client.name.clone())
            .collect();
        Self::refuse_native(item, &names, path)?;
        let entries = Self::entries(module, &item.class, path)?;
        Self::remove(module, &names);
        Ok(entries)
    }

    fn refuse_native(
        item: &NativeCompanionInput,
        names: &[String],
        path: &Path,
    ) -> Result<(), Failure> {
        let unheld = [names.is_empty(), item.web_assembly] == [false, false];
        if !unheld {
            return Ok(());
        }
        Err(Self::failure(
            path,
            "A natively linked C# service has no page to hold its @StreamRequest or @SocketRequest client. Hold the client in a C# service that runs in the browser, or in a JavaScript or TypeScript service.",
        ))
    }

    fn held_clients(path: &Path, source: &str, class: &str) -> Vec<String> {
        crate::template::required_fields::RequiredFields::constructed(path, source, class)
            .into_iter()
            .filter_map(|(_, declared)| declared.split_whitespace().last().map(str::to_owned))
            .collect()
    }

    fn entries(module: &EventModule, class: &str, path: &Path) -> Result<String, Failure> {
        let clients = BrowserStreams::clients(module, class);
        let held = Self::held_clients(path, &module.source, class);
        let streams = BrowserStreams::held(&clients, &held);
        if streams.len() > Self::MOST_STREAMS {
            return Err(Self::failure(
                path,
                "A C# service holds at most 16 @StreamRequest and @SocketRequest methods through its clients.",
            ));
        }
        Ok(streams
            .into_iter()
            .map(|(client, stream)| Self::entry(client, stream))
            .collect())
    }

    fn remove(module: &mut EventModule, names: &[String]) {
        module
            .types
            .retain(|event_type| !names.contains(&event_type.name));
        module
            .methods
            .retain(|method| !names.contains(&method.owner));
    }

    fn entry(client: &str, stream: &EventStream) -> String {
        let description = serde_json::json!({
            "client": client,
            "path": stream.path,
            "event": stream.event,
            "socket": stream.socket,
            "models": stream.models,
        });
        let literal = serde_json::to_string(&description.to_string()).unwrap_or_default();
        format!(
            "[\"__stream_{client}_{}\"] = TacMember.ClientStream({literal}),\n",
            stream.method
        )
    }

    fn failure(path: &Path, message: &str) -> Failure {
        super::host::native_tool_failure(1405, &format!("{}: {message}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::super::registry::NativeCompanionInput;
    use super::CsharpStreamClients;
    use crate::project::NativeCompanion;
    use std::path::{Path, PathBuf};

    struct Samples;

    impl Samples {
        const PATH: &'static str = "client/services/ProbeService.cs";
        const CLIENT: &'static str = "[HTTPClient(\"/ticks\")]\npublic class TicksClient {\n    [StreamRequest]\n    public void Receive(Tick tick) {}\n}\n";

        fn input(web_assembly: bool, class: &str) -> NativeCompanionInput {
            NativeCompanionInput {
                snapshot_root: PathBuf::new(),
                language: NativeCompanion::CSharp,
                source: PathBuf::from(Self::PATH),
                service: String::from("probe"),
                class: String::from(class),
                web_assembly,
                application_provider: false,
            }
        }

        fn taken(source: &str, class: &str, web_assembly: bool) -> (String, Vec<String>) {
            let path = Path::new(Self::PATH);
            let mut module = crate::events::extract(path, source, "tac")
                .expect("events")
                .expect("module");
            let entries = CsharpStreamClients::take(
                &Self::input(web_assembly, class),
                NativeCompanion::CSharp,
                Some(&mut module),
                path,
            )
            .expect("entries");
            let left = module.types.iter().map(|kept| kept.name.clone()).collect();
            (entries, left)
        }
    }

    #[test]
    fn a_held_csharp_stream_client_becomes_a_stream_entry_and_leaves_the_event_types() {
        let held = format!(
            "{}[Service]\npublic sealed class ProbeService {{\n    [Needs]\n    private readonly TicksClient _ticks;\n    [OnEvent]\n    public void Heard(Tick tick) {{}}\n}}\n",
            Samples::CLIENT
        );
        let (entries, left) = Samples::taken(&held, "ProbeService", true);
        assert_eq!(left, ["ProbeService"]);
        assert_eq!(
            entries,
            "[\"__stream_TicksClient_Receive\"] = TacMember.ClientStream(\"{\\\"client\\\":\\\"TicksClient\\\",\\\"event\\\":\\\"Tick\\\",\\\"models\\\":null,\\\"path\\\":\\\"/ticks\\\",\\\"socket\\\":false}\"),\n"
        );
        let unheld = format!(
            "{}[Service]\npublic sealed class QuietService {{\n    public int Count = 0;\n}}\n",
            Samples::CLIENT
        );
        let (entries, left) = Samples::taken(&unheld, "QuietService", true);
        assert!(entries.is_empty(), "{entries}");
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_natively_linked_csharp_service_refuses_a_stream_client() {
        let source = format!(
            "{}[Service]\npublic sealed class ProbeService {{\n    [Needs]\n    private readonly TicksClient _ticks;\n}}\n",
            Samples::CLIENT
        );
        let path = Path::new(Samples::PATH);
        let mut module = crate::events::extract(path, &source, "tac")
            .expect("events")
            .expect("module");
        let refused = CsharpStreamClients::take(
            &Samples::input(false, "ProbeService"),
            NativeCompanion::CSharp,
            Some(&mut module),
            path,
        )
        .expect_err("native refusal");
        assert!(
            refused.to_string().contains("has no page to hold"),
            "{refused}"
        );
    }
}
