#![allow(clippy::expect_used)]

use std::path::Path;

struct DartConstructionProject;

impl DartConstructionProject {
    const SETUP: &'static str = "@TestSetup\nclass ChoiceTestSetup {\n static BehaviorAdapters configure(BehaviorExecution execution) { return BehaviorAdapters(); }\n}\n";
    const SERVICE: &'static str = "@Service\nclass ChoiceService {\n @Needs\n final String value;\n String answer() { return value; }\n}\n";
    const UNIT: &'static str = r"@Test(ChoiceTestSetup)
class ChoiceServiceTest {
  @Fake
  String? value;

  Future<void> testCapturedFakeValues() async {
    value = 'first';
    final first = construct<ChoiceService>(ChoiceService);
    value = 'second';
    final second = construct<ChoiceService>(ChoiceService);
    Assert.equals('first', first.answer());
    Assert.equals('second', second.answer());
  }

  Future<void> testMissingFakeRefused() async {
    value = null;
    final error = await Assert.throws(() => construct<ChoiceService>(ChoiceService));
    Assert.isTrue(error is StateError);
    Assert.equals('Test.construct needs an assigned Fake named value.', (error as StateError).message);
  }

  Future<void> testUnsupportedTargetRefused() async {
    final error = await Assert.throws(() => construct<String>(String));
    Assert.isTrue(error is ArgumentError);
  }

  Future<void> testMismatchedTypeArgumentRefused() async {
    value = 'assigned';
    final error = await Assert.throws(() => construct<String>(ChoiceService));
    Assert.isTrue(error is ArgumentError);
    Assert.equals('Test.construct needs a target matching its type argument.', (error as ArgumentError).message);
  }

  Future<void> testDisposedSetupRefused() async {
    value = 'closed';
    stopBehaviors();
    final error = await Assert.throws(() => construct<ChoiceService>(ChoiceService));
    Assert.equals('Test.construct needs an active test setup.', (error as StateError).message);
  }
}
";

    fn write(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, source).expect("source");
    }

    fn passed(root: &Path, relative: &str, cases: usize) {
        let report = super::run(root, Path::new(relative)).expect("supervised Dart Test runner");
        assert!(report.failure.is_none(), "{report:?}");
        assert_eq!(report.cases.len(), cases, "{report:?}");
        assert!(report.cases.iter().all(|case| case.passed), "{report:?}");
    }
}

#[test]
#[ignore = "requires installed Dart; uses the supervised native Test runner"]
fn native_dart_tests_capture_typed_fakes_and_refuse_invalid_construction() {
    let root = tempfile::tempdir().expect("project");
    for (relative, source) in [
        (
            "client/services/ChoiceService.dart",
            DartConstructionProject::SERVICE,
        ),
        (
            "client/tests/ChoiceTestSetup.dart",
            DartConstructionProject::SETUP,
        ),
        (
            "client/tests/ChoiceServiceTest.dart",
            DartConstructionProject::UNIT,
        ),
        (
            "client/tests/WrongFakeTest.dart",
            "@Test(ChoiceTestSetup)\nclass WrongFakeTest {\n @Fake\n int? value;\n Future<void> testWrongTypeRefused() async {\n value = 17;\n final error = await Assert.throws(() => construct<ChoiceService>(ChoiceService));\n Assert.equals('Test.construct needs a matching typed Fake named value for ChoiceService.', (error as StateError).message);\n }\n}\n",
        ),
    ] {
        DartConstructionProject::write(root.path(), relative, source);
    }
    DartConstructionProject::passed(root.path(), "client/tests/ChoiceServiceTest.dart", 5);
    DartConstructionProject::passed(root.path(), "client/tests/WrongFakeTest.dart", 1);
}

#[test]
#[ignore = "requires installed Dart; runs the actual cumulative tutorial sources without network"]
fn first_app_dart_cumulative_service_test_uses_owned_fake_http() {
    let root = tempfile::tempdir().expect("project");
    for (relative, source) in [
        (
            "client/services/CounterService.dart",
            include_str!(
                "../../../../website/content/docs/first-app/call-the-server/client/services/CounterService.dart"
            ),
        ),
        (
            "client/remotes/GreetingClient.dart",
            include_str!(
                "../../../../website/content/docs/first-app/call-the-server/client/remotes/GreetingClient.dart"
            ),
        ),
        (
            "client/models/Greeting.dart",
            include_str!(
                "../../../../website/content/docs/first-app/call-the-server/client/models/Greeting.dart"
            ),
        ),
        (
            "client/setups/GreetingSetup.dart",
            include_str!(
                "../../../../website/content/docs/first-app/call-the-server/client/setups/GreetingSetup.dart"
            ),
        ),
        (
            "client/tests/HelloTestSetup.dart",
            include_str!(
                "../../../../website/content/docs/first-app/add-tests/client/tests/HelloTestSetup.dart"
            ),
        ),
        (
            "client/tests/CounterServiceTest.dart",
            include_str!(
                "../../../../website/content/docs/first-app/add-tests/client/tests/CounterServiceTest.dart"
            ),
        ),
    ] {
        DartConstructionProject::write(root.path(), relative, source);
    }
    DartConstructionProject::passed(root.path(), "client/tests/CounterServiceTest.dart", 2);
}

#[test]
#[ignore = "requires installed Dart; uses the supervised native Test runner"]
fn a_dart_test_unit_sends_on_a_socket_client_without_a_page_and_fakes_it() {
    let root = tempfile::tempdir().expect("project");
    for (relative, source) in [
        (
            "client/models/ChatMessage.dart",
            "class ChatMessage {\n  final String text;\n\n  const ChatMessage(this.text);\n}\n",
        ),
        (
            "client/remotes/ChatClient.dart",
            "@HTTPClient(\"/chat\")\nabstract interface class ChatClient {\n  @SocketRequest\n  void receive(ChatMessage message);\n}\n",
        ),
        (
            "client/services/ChatService.dart",
            "@Service\nclass ChatService {\n @Needs\n final ChatClient chatClient;\n String said = '';\n\n @OnEvent\n void answered(ChatMessage message) {\n  said = message.text;\n }\n\n Future<void> sayHello() => chatClient.send(ChatMessage('Hello'));\n}\n",
        ),
        (
            "client/tests/ChatTestSetup.dart",
            "@TestSetup\nclass ChatTestSetup {\n static BehaviorAdapters configure(BehaviorExecution execution) { return BehaviorAdapters(); }\n}\n",
        ),
        (
            "client/tests/ChatServiceTest.dart",
            "@Test(ChatTestSetup)\nclass ChatServiceTest {\n @Fake\n ChatClient? chatClient;\n\n Future<void> testSendWithoutAPageFails() async {\n  chatClient = ChatClient();\n  final service = construct<ChatService>(ChatService);\n  final error = await Assert.throws(() => service.sayHello());\n  Assert.isTrue('$error'.contains('HTTP_MISSING'));\n }\n\n Future<void> testAMessageArrivesAsAnEvent() async {\n  chatClient = ChatClient();\n  final service = construct<ChatService>(ChatService);\n  service.answered(const ChatMessage('hi'));\n  Assert.equals('hi', service.said);\n }\n}\n",
        ),
    ] {
        DartConstructionProject::write(root.path(), relative, source);
    }
    DartConstructionProject::passed(root.path(), "client/tests/ChatServiceTest.dart", 2);
}
