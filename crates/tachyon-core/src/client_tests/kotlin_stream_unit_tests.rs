#![allow(clippy::expect_used)]

use std::path::Path;

struct KotlinStreamUnit;

impl KotlinStreamUnit {
    const FILES: [(&'static str, &'static str); 7] = [
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
        (
            "client/services/TicksService.kt",
            "import models.ChatMessage\nimport models.Tick\n\n@Service\nclass TicksService {\n    @Needs\n    private val ticks: TicksClient\n    @Needs\n    private val chat: ChatClient\n    var level: Int = 0\n\n    \
             @OnEvent\n    fun heard(tick: Tick) {\n        level = tick.level\n    }\n\n    \
             suspend fun say(text: String) {\n        chat.send(ChatMessage(text))\n    }\n}\n",
        ),
        (
            "client/tests/QuietTestSetup.kt",
            "@TestSetup\nclass QuietTestSetup {\n    companion object {\n        fun configure(execution: Behaviors.Execution): Behaviors.Configuration = Behaviors.Configuration()\n    }\n}\n",
        ),
        (
            "client/tests/TicksServiceTest.kt",
            "import models.Tick\n\n@Test(QuietTestSetup::class, {\"partition\":\"tests\"})\nclass TicksServiceTest {\n    @Fake\n    var ticks: TicksClient? = null\n    @Fake\n    var chat: ChatClient? = null\n\n    \
             fun testHeardKeepsTheLevel() {\n        ticks = TicksClient()\n        chat = ChatClient()\n        val service = construct(TicksService::class)\n        service.heard(Tick(\"anvil\", 3))\n        Assert.equals(3, service.level)\n    }\n\n    \
             suspend fun testSayHasNoPageToSendOn() {\n        ticks = TicksClient()\n        chat = ChatClient()\n        val service = construct(TicksService::class)\n        val code = try { service.say(\"hello\"); \"\" } catch (error: RuntimeException) { error.message.orEmpty().substringBefore(\":\") }\n        Assert.equals(\"HTTP_MISSING\", code)\n    }\n}\n",
        ),
    ];

    fn write(root: &Path) {
        for (relative, source) in Self::FILES {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().expect("fixture parent"))
                .expect("fixture directory");
            std::fs::write(path, source).expect("fixture source");
        }
    }
}

#[test]
fn a_kotlin_test_unit_builds_stream_clients_and_its_socket_has_no_page() {
    if std::process::Command::new("kotlinc")
        .arg("-version")
        .output()
        .is_err()
    {
        assert!(std::env::var_os("CI").is_none(), "CI must provide kotlinc");
        eprintln!("skipping: kotlinc is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("test project");
    KotlinStreamUnit::write(root.path());
    let report = super::run(root.path(), Path::new("client/tests/TicksServiceTest.kt"))
        .expect("native Kotlin test unit");
    assert_eq!(report.cases.len(), 2, "{report:?}");
    assert!(report.cases.iter().all(|case| case.passed), "{report:?}");
}
