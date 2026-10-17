#![allow(clippy::expect_used)]

use super::kotlin_tests::kotlin_web_ready;
use super::{BuildJobs, WebLanguage, build_all};
use std::path::{Path, PathBuf};
use std::process::Command;

struct KotlinStreams;

impl KotlinStreams {
    const FILES: [(&str, &str); 4] = [
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
            "import models.ChatMessage\n\n@HTTPClient(\"/chat\")\ninterface ChatClient {\n    @SocketRequest\n    fun receive(message: ChatMessage)\n}\n",
        ),
    ];

    const SERVICES: [(&str, &str, &str); 2] = [
        (
            "ticks",
            "TicksService",
            "import models.ChatMessage\nimport models.Tick\n\n@Service\nclass TicksService {\n    @Needs\n    private val ticks: TicksClient\n    @Needs\n    private val chat: ChatClient\n    var level: Int = 0\n    var said: String = \"\"\n\n    \
             @OnEvent\n    fun heard(tick: Tick) {\n        level = tick.level\n    }\n\n    \
             @OnEvent\n    fun answered(message: ChatMessage) {\n        said = message.text\n    }\n\n    \
             suspend fun say(text: String) {\n        chat.send(ChatMessage(text))\n    }\n}\n",
        ),
        (
            "quiet",
            "QuietService",
            "@Service\nclass QuietService {\n    var count: Int = 0\n}\n",
        ),
    ];

    const DRIVER: &str = "globalThis.location = { href: 'http://127.0.0.1:4000/page' }\n\
         const opened = []\n\
         globalThis.EventSource = class { constructor(url) { this.url = String(url); this.closed = false; opened.push(this) } addEventListener() {} close() { this.closed = true } }\n\
         globalThis.WebSocket = class { constructor(url) { this.url = String(url); this.readyState = 0; this.sent = []; this.closed = false; opened.push(this) } send(text) { this.sent.push(text) } close() { this.closed = true } }\n\
         const { default: TicksService, __tachyon_event_plan__: ticksPlan } = await import('./TicksService.js')\n\
         const { default: QuietService, __tachyon_event_plan__: quietPlan } = await import('./QuietService.js')\n\
         const { default: ApplicationEvents } = await import('./application-events.mjs')\n\
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
         try { await ticks.say('overflow') } catch (error) { refused = error.message }\n\
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

    fn write(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(&path, bytes).expect("file");
        path
    }

    async fn publish(root: &Path) {
        for (relative, text) in Self::FILES {
            Self::write(root, relative, text.as_bytes());
        }
        let mut services = Vec::new();
        for (instance, class, text) in Self::SERVICES {
            let path = Self::write(
                root,
                &format!("client/services/{class}.kt"),
                text.as_bytes(),
            );
            services.push((
                String::from(instance),
                String::from(class),
                WebLanguage::Kotlin,
                path,
            ));
        }
        let profile = crate::BuildProfile::Development;
        let built = build_all(root, &services, &BuildJobs::new(1), profile)
            .await
            .unwrap_or_else(|failure| panic!("{failure}"));
        for (name, bytes) in built.files {
            Self::write(root, &name, &bytes);
        }
        Self::write(
            root,
            "application-events.mjs",
            concat!(
                include_str!("../template/application-events.js"),
                "\nexport default TacApplicationEvents\n"
            )
            .as_bytes(),
        );
    }

    fn drive(root: &Path) -> serde_json::Value {
        Self::write(root, "drive.mjs", Self::DRIVER.as_bytes());
        let output = Command::new("node")
            .arg("drive.mjs")
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
async fn a_kotlin_service_receives_its_client_streams_as_events_and_sends_on_its_socket() {
    if !kotlin_web_ready() {
        eprintln!("skipping: kotlinc-js, TAC_KOTLIN_WASM_STDLIB or node is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    KotlinStreams::publish(root.path()).await;
    let report = KotlinStreams::drive(root.path());
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
    assert!(
        report["refused"]
            .as_str()
            .is_some_and(|message| message.contains("HTTP_NETWORK")),
        "{report}"
    );
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
