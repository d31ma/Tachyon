#![allow(clippy::expect_used)]

use std::path::Path;

struct CsharpConstructionFixture;

impl CsharpConstructionFixture {
    fn lower(fake: &str) -> String {
        let source = format!(
            "[Test(typeof(ChoiceTestSetup))]\nclass ChoiceServiceTest {{\n {fake}\n void testRead() {{}}\n}}\n\
             [TestSetup]\nclass ChoiceTestSetup {{\n public static BehaviorAdapters configure(BehaviorExecution execution) {{ return new BehaviorAdapters(); }}\n}}\n\
             [Service]\nclass ChoiceService {{\n [Needs]\n private readonly GreetingClient greetingClient;\n}}\n\
             class GreetingClient {{}}\n"
        );
        let path = Path::new("client/tests/ChoiceServiceTest.cs");
        let source = crate::behavior::ServiceRoles::read(path, &source)
            .expect("ordinary role stripping")
            .source;
        crate::behavior::lower(path, &source, "tac")
            .expect("C# Test lowering")
            .expect("lowered source")
    }
}

#[test]
fn csharp_test_construct_passes_only_matching_typed_fakes_to_real_constructors() {
    let source = CsharpConstructionFixture::lower("[Fake]\n public GreetingClient greetingClient;");
    assert!(source.contains("public ChoiceService(GreetingClient greetingClient)"));
    assert!(source.contains("new ChoiceService(this.__tachyonAssignedFake(this.greetingClient"));
    assert!(source.contains("public T construct<T>() where T : class"));
    assert!(!source.contains("Activator.CreateInstance"));
    assert!(!source.contains("GetField("));
    assert!(!source.contains("dynamic"));
}

#[test]
fn csharp_test_construct_refuses_missing_unmarked_and_incompatible_fakes() {
    for field in [
        "",
        "public GreetingClient greetingClient;",
        "[Fake]\n public string greetingClient;",
    ] {
        let source = CsharpConstructionFixture::lower(field);
        assert!(source.contains("matching typed Fake named greetingClient for ChoiceService"));
        assert!(!source.contains("new ChoiceService("));
    }
}

#[test]
fn csharp_test_construct_requires_active_setup_and_assigned_fake() {
    let source = CsharpConstructionFixture::lower("[Fake]\n public GreetingClient greetingClient;");
    assert!(source.contains("Test.construct needs an active test setup."));
    assert!(source.contains("Test.construct needs an assigned Fake named greetingClient."));
    assert!(source.contains("Test.construct needs a class with a declared Needs constructor."));
}

#[tokio::test]
async fn csharp_test_http_client_fakes_compile_and_execute_per_construction() {
    let path = Path::new("client/tests/GreetingServiceTest.cs");
    let source = concat!(
        include_str!("../../../tests/fixtures/behavior/csharp-http-construction/GreetingClient.cs"),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/GreetingService.cs"
        ),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/GreetingTestSetup.cs"
        ),
        include_str!("../../../tests/fixtures/behavior/csharp-http-construction/GreetingSetup.cs"),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/FirstGreetingSetup.cs"
        ),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/SecondGreetingSetup.cs"
        ),
        include_str!("../../../tests/fixtures/behavior/csharp-http-construction/GreetingAnswer.cs"),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/GreetingServiceTest.cs"
        ),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/MissingGreetingServiceTest.cs"
        ),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/IncompatibleGreetingServiceTest.cs"
        ),
        include_str!(
            "../../../tests/fixtures/behavior/csharp-http-construction/EmittedBehaviorTests.cs"
        )
    );
    let emitted = crate::behavior::lower(path, source, "tac")
        .expect("authored HTTPClient and Test lowering")
        .expect("emitted C#");
    assert!(emitted.contains("public GreetingService(GreetingClient greetingClient)"));
    assert!(emitted.contains("new GreetingService(this.__tachyonAssignedFake(this.greetingClient"));
    assert!(emitted.contains("Test.construct needs an active test setup."));
    assert!(emitted.contains("Test.construct needs an assigned Fake named greetingClient."));
    assert!(emitted.contains("matching typed Fake named greetingClient for GreetingService"));
    let mut probe = tokio::process::Command::new("dotnet");
    probe.arg("--version");
    match crate::external_command::run(&mut probe, std::time::Duration::from_secs(10), 4096).await {
        Err(crate::external_command::ToolError::Spawn(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            eprintln!(
                "deferring C# HTTPClient typed Test construction execution: dotnet is unavailable"
            );
            return;
        }
        Err(error) => panic!("C# SDK availability probe failed: {error}"),
        Ok(result) => assert!(
            result.status.success(),
            "C# SDK availability probe failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        ),
    }
    let directory = tempfile::tempdir().expect("fixture output");
    let output = directory.path().join("Emitted.cs");
    std::fs::write(&output, emitted).expect("lowered fixture");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/behavior-native-runtime-test.mjs");
    let mut command = tokio::process::Command::new("node");
    command.arg(script).arg("cs").arg("--emitted").arg(output);
    let result =
        crate::external_command::run(&mut command, std::time::Duration::from_mins(4), 1024 * 1024)
            .await
            .expect("supervised C# SDK fixture");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        String::from_utf8_lossy(&result.stdout)
            .contains("PASS C# HTTPClient typed Test construction")
    );
}

#[test]
fn every_csharp_fake_is_cleared_to_its_default_before_each_case() {
    let source = CsharpConstructionFixture::lower(
        "[Fake]\n public GreetingClient greetingClient;\n [Fake]\n public int count;",
    );
    assert!(
        source.contains(
            "foreach (var name in new string[] {\"testRead\"}) {\nthis.greetingClient = default;\nthis.count = default;\nstartBehaviors();"
        ),
        "{source}"
    );
}
