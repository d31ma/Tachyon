#![allow(clippy::expect_used)]

use super::registry::NativeCompanionInput;
use crate::project::NativeCompanion;

struct StreamClientProject;

impl StreamClientProject {
    const FILES: [(&'static str, &'static str); 5] = [
        (
            "client/models/Tick.kt",
            "data class Tick(val sku: String, val level: Int)\n",
        ),
        (
            "client/models/ChatMessage.kt",
            "data class ChatMessage(val text: String)\n",
        ),
        (
            "client/remotes/TicksClient.kt",
            "import models.Tick\n\n@HTTPClient(\"/ticks\")\ninterface TicksClient {\n    @StreamRequest\n    fun receive(tick: Tick)\n}\n",
        ),
        (
            "client/remotes/ChatClient.kt",
            "import models.ChatMessage\n\n@HTTPClient(\"/chat\")\ninterface ChatClient {\n    @SocketRequest(\"/room\")\n    fun receive(message: ChatMessage)\n}\n",
        ),
        (
            "client/services/TicksService.kt",
            "import models.ChatMessage\nimport models.Tick\n\n@Service\nclass TicksService {\n    @Needs\n    private val ticks: TicksClient\n    @Needs\n    private val chat: ChatClient\n    var level: Int = 0\n\n    \
             @OnEvent\n    fun heard(tick: Tick) {\n        level = tick.level\n    }\n\n    \
             suspend fun say(text: String) {\n        chat.send(ChatMessage(text))\n    }\n}\n",
        ),
    ];

    fn registry(browser: bool) -> Result<String, crate::Failure> {
        let root = tempfile::tempdir().expect("project");
        for (relative, text) in Self::FILES {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("folder");
            std::fs::write(path, text).expect("file");
        }
        let source = root.path().join("client/services/TicksService.kt");
        super::registry::source(
            &[NativeCompanionInput {
                snapshot_root: root.path().to_owned(),
                language: NativeCompanion::Kotlin,
                source,
                service: String::from("ticks"),
                class: String::from("TicksService"),
                web_assembly: browser,
                application_provider: false,
            }],
            NativeCompanion::Kotlin,
        )
        .map(|registry| registry.expect("emitted Kotlin registry"))
    }
}

#[test]
fn a_kotlin_browser_service_declares_the_streams_of_the_clients_it_holds() {
    let registry = StreamClientProject::registry(true).expect("browser build");
    for expected in [
        "\"__stream_TicksClient_receive\" to TacStream(\"TicksClient\", \"/ticks\", \"Tick\", false, \"{\\\"Tick\\\":",
        "\"__stream_ChatClient_receive\" to TacStream(\"ChatClient\", \"/chat/room\", \"ChatMessage\", true, \"{\\\"ChatMessage\\\":",
        "suspend fun send(message: Any) {",
        "behaviorSocketSend(\"ChatClient\", message)",
    ] {
        assert!(
            registry.contains(expected),
            "missing {expected}\n{registry}"
        );
    }
    assert!(
        !registry.contains("tacInstance.receive("),
        "a client's stream method is not a listener of the service\n{registry}"
    );
    assert!(
        !registry.contains("Behaviors.create(\"TicksSetup\""),
        "a client with only stream methods needs no Setup\n{registry}"
    );
}

#[test]
fn a_native_kotlin_build_refuses_stream_clients_with_the_browser_reason() {
    let Err(error) = StreamClientProject::registry(false) else {
        panic!("a native Kotlin build has no page to hold the stream");
    };
    let message = error.to_string();
    assert!(message.contains("only in its browser build"), "{message}");
    assert!(message.contains("TicksService"), "{message}");
}
