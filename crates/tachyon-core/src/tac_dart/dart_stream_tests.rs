#![allow(clippy::expect_used)]

use std::path::Path;
use std::process::Command;

struct DartStreams;

impl DartStreams {
    const FILES: [(&str, &str); 6] = [
        (
            "client/models/Tick.dart",
            "class Tick {\n  final String sku;\n  final int level;\n\n  const Tick(this.sku, this.level);\n}\n",
        ),
        (
            "client/models/ChatMessage.dart",
            "class ChatMessage {\n  final String text;\n\n  const ChatMessage(this.text);\n}\n",
        ),
        (
            "client/remotes/TicksClient.dart",
            "@HTTPClient(\"/ticks\")\nabstract interface class TicksClient {\n  @StreamRequest\n  void receive(Tick tick);\n}\n",
        ),
        (
            "client/remotes/ChatClient.dart",
            "@HTTPClient(\"/chat\")\nabstract interface class ChatClient {\n  @SocketRequest\n  void receive(ChatMessage message);\n}\n",
        ),
        (
            "client/services/TicksService.dart",
            "@Service\nclass TicksService {\n  @Needs\n  final TicksClient ticksClient;\n  @Needs\n  final ChatClient chatClient;\n  int level = 0;\n  String said = '';\n\n\
             \x20 @OnEvent\n  void heard(Tick tick) {\n    level = tick.level;\n  }\n\n\
             \x20 @OnEvent\n  void answered(ChatMessage message) {\n    said = message.text;\n  }\n\n\
             \x20 Future<void> say(String text) => chatClient.send(ChatMessage(text));\n}\n",
        ),
        (
            "client/services/QuietService.dart",
            "@Service\nclass QuietService {\n  int count = 0;\n}\n",
        ),
    ];

    const SERVICES: [(&str, &str); 2] = [
        ("TicksService", "client/services/TicksService.dart"),
        ("QuietService", "client/services/QuietService.dart"),
    ];

    const DRIVER: &str = "globalThis.location = { href: 'http://127.0.0.1:4000/page' }\n\
         const opened = []\n\
         globalThis.EventSource = class { constructor(url) { this.url = String(url); this.closed = false; opened.push(this) } addEventListener() {} close() { this.closed = true } }\n\
         globalThis.WebSocket = class { constructor(url) { this.url = String(url); this.readyState = 0; this.sent = []; this.closed = false; opened.push(this) } send(text) { this.sent.push(text) } close() { this.closed = true } }\n\
         const { default: TicksService, __tachyon_event_plan__: ticksPlan } = await import('./TicksService.js')\n\
         const { default: QuietService, __tachyon_event_plan__: quietPlan } = await import('./QuietService.js')\n\
         const { default: ApplicationEvents } = await import('./application-events.js')\n\
         let failures = 0\n\
         const published = []\n\
         const bus = new ApplicationEvents({ changed: () => {}, failed: () => { failures += 1 } })\n\
         bus.attach({ receive: (value) => published.push(value) }, { listeners: [{ method: 'receive', event: 'Tick' }] }, {})\n\
         const ticks = new TicksService({}), quiet = new QuietService({})\n\
         bus.attach(ticks, ticksPlan.types[0], { applicationEventPublisher: null })\n\
         bus.attach(quiet, quietPlan.types[0], { applicationEventPublisher: null })\n\
         await bus.initialize(ticks)\n\
         await bus.initialize(quiet)\n\
         const stream = opened.find((source) => !('sent' in source)), socket = opened.find((source) => 'sent' in source)\n\
         const queued = [ticks.say('early')]\n\
         for (let index = 0; index < 15; index += 1) queued.push(ticks.say(`queued ${index}`))\n\
         let refused = ''\n\
         try { await ticks.say('overflow') } catch (error) { refused = error.code ?? error.message }\n\
         await Promise.all(queued)\n\
         socket.readyState = 1\n\
         socket.onopen()\n\
         stream.onmessage({ data: JSON.stringify({ sku: 'anvil', level: 3 }), lastEventId: '' })\n\
         stream.onmessage({ data: JSON.stringify({ sku: 'anvil', level: 'high' }), lastEventId: '' })\n\
         socket.onmessage({ data: JSON.stringify({ text: 'hello' }) })\n\
         await ticks.say('late')\n\
         const report = { urls: opened.map((source) => source.url).sort(), level: ticks.level, said: ticks.said, published, failures, refused, sent: socket.sent }\n\
         bus.dispose(ticks)\n\
         await new Promise((resolve) => setTimeout(resolve, 50))\n\
         report.closed = opened.map((source) => source.closed)\n\
         report.afterDispose = opened.length\n\
         console.log(JSON.stringify(report))\n\
         process.exit(0)\n";

    const SITE_FILES: [(&str, &str); 6] = [
        (
            "client/models/Tick.dart",
            include_str!("../../../../website/content/features/realtime/client/models/Tick.dart"),
        ),
        (
            "client/remotes/TicksClient.dart",
            include_str!(
                "../../../../website/content/features/realtime/client/remotes/TicksClient.dart"
            ),
        ),
        (
            "client/services/StreamService.dart",
            include_str!(
                "../../../../website/content/features/realtime/client/services/StreamService.dart"
            ),
        ),
        (
            "client/models/ChatMessage.dart",
            include_str!(
                "../../../../website/content/features/sockets/client/models/ChatMessage.dart"
            ),
        ),
        (
            "client/remotes/ChatClient.dart",
            include_str!(
                "../../../../website/content/features/sockets/client/remotes/ChatClient.dart"
            ),
        ),
        (
            "client/services/ChatService.dart",
            include_str!(
                "../../../../website/content/features/sockets/client/services/ChatService.dart"
            ),
        ),
    ];

    const SITE_SERVICES: [(&str, &str); 2] = [
        ("StreamService", "client/services/StreamService.dart"),
        ("ChatService", "client/services/ChatService.dart"),
    ];

    const SITE_DRIVER: &str = "globalThis.location = { href: 'https://example.test/features' }\n\
         const opened = []\n\
         globalThis.EventSource = class { constructor(url) { this.url = String(url); opened.push(this) } addEventListener() {} close() {} }\n\
         globalThis.WebSocket = class { constructor(url) { this.url = String(url); this.readyState = 1; this.sent = []; opened.push(this) } send(text) { this.sent.push(text) } close() {} }\n\
         const { default: StreamService, __tachyon_event_plan__: streamPlan } = await import('./StreamService.js')\n\
         const { default: ChatService, __tachyon_event_plan__: chatPlan } = await import('./ChatService.js')\n\
         const { default: ApplicationEvents } = await import('./application-events.js')\n\
         const bus = new ApplicationEvents({ changed: () => {}, failed: () => {} })\n\
         const stream = new StreamService({}), chat = new ChatService({})\n\
         bus.attach(stream, streamPlan.types[0], {})\n\
         bus.attach(chat, chatPlan.types[0], {})\n\
         await bus.initialize(stream)\n\
         await bus.initialize(chat)\n\
         const ticks = opened.find((source) => !('sent' in source)), socket = opened.find((source) => 'sent' in source)\n\
         for (let level = 0; level < 130; level += 1) ticks.onmessage({ data: JSON.stringify({ sku: 'anvil', level }), lastEventId: '' })\n\
         socket.onmessage({ data: JSON.stringify({ text: 'hi' }) })\n\
         await chat.sayHello()\n\
         console.log(JSON.stringify({ urls: opened.map((source) => source.url).sort(), count: stream.ticks.length, last: stream.ticks.at(-1), messages: chat.messages, sent: socket.sent }))\n\
         process.exit(0)\n";

    fn tools_available() -> bool {
        ["dart", "node"].iter().all(|tool| {
            Command::new(tool)
                .arg("--version")
                .output()
                .is_ok_and(|output| output.status.success())
        })
    }

    fn write(root: &Path, relative: &str, bytes: &[u8]) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(&path, bytes).expect("file");
    }

    async fn publish(
        root: &Path,
        files: &[(&str, &str)],
        classes: &[(&str, &str)],
        names: &[&str],
    ) {
        for (relative, text) in files {
            Self::write(root, relative, text.as_bytes());
        }
        let paths: Vec<(&str, std::path::PathBuf)> = classes
            .iter()
            .map(|(class, relative)| (*class, root.join(relative)))
            .collect();
        let services: Vec<(&str, &Path)> = paths
            .iter()
            .map(|(class, path)| (*class, path.as_path()))
            .collect();
        let instances = names.iter().map(|name| (*name).to_owned()).collect();
        let built = super::build_all(root, &services, &instances, crate::BuildProfile::Release)
            .await
            .unwrap_or_else(|failure| panic!("{failure}"));
        for (name, bytes) in built {
            Self::write(root, &name, &bytes);
        }
        Self::write(root, "package.json", b"{\"type\":\"module\"}");
        Self::write(
            root,
            "application-events.js",
            concat!(
                include_str!("../template/application-events.js"),
                "\nexport default TacApplicationEvents\n"
            )
            .as_bytes(),
        );
        Self::write(
            root,
            "fetch-files.js",
            b"import { readFile } from 'node:fs/promises'\nglobalThis.fetch = async (url) => new Response(await readFile(url))\n",
        );
    }

    fn drive(root: &Path, driver: &str) -> serde_json::Value {
        Self::write(root, "drive.js", driver.as_bytes());
        let output = Command::new("node")
            .args(["--import", "./fetch-files.js", "drive.js"])
            .current_dir(root)
            .output()
            .expect("node");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_str(stdout.trim()).expect("report")
    }
}

#[tokio::test]
async fn a_dart_service_receives_its_client_streams_as_events_and_sends_on_its_socket() {
    if !DartStreams::tools_available() {
        eprintln!("skipping: dart or node is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    DartStreams::publish(
        root.path(),
        &DartStreams::FILES,
        &DartStreams::SERVICES,
        &["ticks", "quiet"],
    )
    .await;
    let report = DartStreams::drive(root.path(), DartStreams::DRIVER);
    assert_eq!(
        report["urls"],
        serde_json::json!(["/ticks", "ws://127.0.0.1:4000/chat"]),
        "only the service that needs the clients opens them: {report}"
    );
    assert_eq!(report["level"], 3, "{report}");
    assert_eq!(report["said"], "hello", "{report}");
    assert_eq!(
        report["published"],
        serde_json::json!([{"sku": "anvil", "level": 3}]),
        "{report}"
    );
    assert_eq!(report["failures"], 1, "{report}");
    assert_eq!(report["refused"], "HTTP_NETWORK", "{report}");
    let sent = report["sent"].as_array().expect("sent");
    assert_eq!(sent.len(), 17, "{report}");
    assert_eq!(sent[0], "{\"text\":\"early\"}", "{report}");
    assert_eq!(sent[15], "{\"text\":\"queued 14\"}", "{report}");
    assert_eq!(sent[16], "{\"text\":\"late\"}", "{report}");
    assert_eq!(
        report["closed"],
        serde_json::json!([true, true]),
        "{report}"
    );
    assert_eq!(report["afterDispose"], 2, "{report}");
}

#[tokio::test]
async fn the_feature_page_dart_samples_hold_their_stream_and_socket() {
    if !DartStreams::tools_available() {
        eprintln!("skipping: dart or node is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    DartStreams::publish(
        root.path(),
        &DartStreams::SITE_FILES,
        &DartStreams::SITE_SERVICES,
        &["stream", "chat"],
    )
    .await;
    let report = DartStreams::drive(root.path(), DartStreams::SITE_DRIVER);
    assert_eq!(
        report["urls"],
        serde_json::json!(["/ticks", "wss://example.test/chat"]),
        "{report}"
    );
    assert_eq!(report["count"], 128, "{report}");
    assert_eq!(
        report["last"],
        serde_json::json!({"sku": "anvil", "level": 129}),
        "{report}"
    );
    assert_eq!(
        report["messages"],
        serde_json::json!([{"text": "hi"}]),
        "{report}"
    );
    assert_eq!(
        report["sent"],
        serde_json::json!(["{\"text\":\"Hello from a tab\"}"]),
        "{report}"
    );
}
