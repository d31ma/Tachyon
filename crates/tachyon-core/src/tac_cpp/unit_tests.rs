#![allow(clippy::expect_used)]

use std::path::Path;

struct UnitProject;

impl UnitProject {
    const FIRST_APP: [(&'static str, &'static str); 5] = [
        (
            "client/services/CounterService.hpp",
            include_str!(
                "../../../../website/content/docs/first-app/call-the-server/client/services/CounterService.hpp"
            ),
        ),
        (
            "client/models/Greeting.hpp",
            include_str!(
                "../../../../website/content/docs/first-app/call-the-server/client/models/Greeting.hpp"
            ),
        ),
        (
            "client/remotes/GreetingClient.hpp",
            include_str!(
                "../../../../website/content/docs/first-app/call-the-server/client/remotes/GreetingClient.hpp"
            ),
        ),
        (
            "client/tests/HelloTestSetup.hpp",
            include_str!(
                "../../../../website/content/docs/first-app/add-tests/client/tests/HelloTestSetup.hpp"
            ),
        ),
        (
            "client/tests/CounterServiceTest.hpp",
            include_str!(
                "../../../../website/content/docs/first-app/add-tests/client/tests/CounterServiceTest.hpp"
            ),
        ),
    ];
    const TALLY: &'static str = "class [[Service]] TallyService {\npublic:\n    int count = 0;\n    int read() { return count; }\n};\n";
    const SCORE: &'static str = "class [[Service]] ScoreService {\npublic:\n    [[Needs]] TallyService& tally;\n    int read() { return tally.count; }\n};\n";
    const REPORT: &'static str = "class [[Service]] ReportService {\npublic:\n    [[Needs]] TallyService& tally;\n    [[Needs]] ScoreService& score;\n    int direct() { return tally.count; }\n    int shared() { return score.read(); }\n};\n";
    const PLAIN_SETUP: &'static str = "class [[TestSetup]] ReportTestSetup {};\n";

    fn has_compiler() -> bool {
        let found = std::process::Command::new(super::CppTools::test_compiler())
            .arg("--version")
            .output()
            .is_ok();
        if !found {
            eprintln!("skipping: no C++ test compiler");
        }
        found
    }

    fn write(root: &Path, files: &[(&str, &str)]) {
        for (relative, source) in files {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
            std::fs::write(path, source).expect("file");
        }
    }

    fn report(root: &Path, unit: &str) -> crate::client_tests::TestReport {
        crate::client_tests::run(root, Path::new(unit)).expect("report")
    }

    fn refusal(root: &Path, unit: &str) -> String {
        crate::client_tests::run(root, Path::new(unit))
            .expect_err("refused")
            .to_string()
    }

    fn outcomes(report: &crate::client_tests::TestReport) -> Vec<(String, bool)> {
        report
            .cases
            .iter()
            .map(|case| (case.name.clone(), case.passed))
            .collect()
    }

    fn report_project(root: &Path, unit: &str) {
        Self::write(
            root,
            &[
                ("client/services/TallyService.hpp", Self::TALLY),
                ("client/services/ScoreService.hpp", Self::SCORE),
                ("client/services/ReportService.hpp", Self::REPORT),
                ("client/tests/ReportTestSetup.hpp", Self::PLAIN_SETUP),
                ("client/tests/ReportServiceTest.hpp", unit),
            ],
        );
    }
}

#[test]
fn the_first_app_test_answers_http_through_its_setup_and_records_calls() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::write(root.path(), &UnitProject::FIRST_APP);
    let report = UnitProject::report(root.path(), "client/tests/CounterServiceTest.hpp");
    assert_eq!(
        UnitProject::outcomes(&report),
        [
            (String::from("testIncrementAddsOne"), true),
            (String::from("testAskServerSavesTheMessage"), true),
        ],
        "{report:?}"
    );
    assert_eq!(report.cases[0].called, ["CounterService.increment"]);
    assert_eq!(report.cases[1].called, ["CounterService.askServer"]);
}

#[test]
fn a_request_without_an_answer_fails_its_case_with_the_adapter_name() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::write(root.path(), &UnitProject::FIRST_APP);
    UnitProject::write(
        root.path(),
        &[(
            "client/tests/HelloTestSetup.hpp",
            "class [[TestSetup]] HelloTestSetup {};\n",
        )],
    );
    let report = UnitProject::report(root.path(), "client/tests/CounterServiceTest.hpp");
    assert!(report.cases[0].passed, "{report:?}");
    assert!(!report.cases[1].passed, "{report:?}");
    let error = report.cases[1].error.as_deref().unwrap_or_default();
    assert!(
        error.contains("no HTTP answer for the adapter \"greeting\""),
        "{error}"
    );
}

#[test]
fn value_source_runs_one_case_for_each_input() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::report_project(
        root.path(),
        "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[ValueSource([3, 0, 2.5])]]\n    void testIsPositive(double value) {\n        Assert::isTrue(value > 0);\n    }\n\n    [[ValueSource([\"say \\\"hi\\\"\", \"two\\nlines\", true])]]\n    void testKeepsText(const std::string& text) {\n        Assert::isFalse(text.empty());\n    }\n};\n",
    );
    let refused = UnitProject::refusal(root.path(), "client/tests/ReportServiceTest.hpp");
    assert!(refused.contains("did not run"), "{refused}");
    UnitProject::report_project(
        root.path(),
        "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[ValueSource([3, 0, 2.5])]]\n    void testIsPositive(double value) {\n        Assert::isTrue(value > 0);\n    }\n\n    [[ValueSource([\"say \\\"hi\\\"\", \"two\\nlines\"])]]\n    void testKeepsText(const std::string& text) {\n        Assert::equals(true, text.size() > 5);\n    }\n};\n",
    );
    let report = UnitProject::report(root.path(), "client/tests/ReportServiceTest.hpp");
    assert_eq!(
        UnitProject::outcomes(&report),
        [
            (String::from("testIsPositive[1]"), true),
            (String::from("testIsPositive[2]"), false),
            (String::from("testIsPositive[3]"), true),
            (String::from("testKeepsText[1]"), true),
            (String::from("testKeepsText[2]"), true),
        ],
        "{report:?}"
    );
}

#[test]
fn a_fake_service_reaches_only_the_constructed_service() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::report_project(
        root.path(),
        "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[Fake]] TallyService tally;\n\n    void testUsesTheFake() {\n        tally.count = 7;\n        ReportService report = construct<ReportService>();\n        Assert::equals(7, report.direct());\n        Assert::equals(0, report.shared());\n    }\n\n    void testPlainConstructionKeepsTheSharedService() {\n        tally.count = 7;\n        ReportService report;\n        Assert::equals(0, report.direct());\n    }\n};\n",
    );
    let report = UnitProject::report(root.path(), "client/tests/ReportServiceTest.hpp");
    assert!(report.cases.iter().all(|case| case.passed), "{report:?}");
    assert_eq!(
        report.cases[0].called,
        [
            "ReportService.direct",
            "ReportService.shared",
            "ScoreService.read"
        ]
    );
}

#[test]
fn fake_and_value_source_forms_are_checked_before_compiling() {
    let root = tempfile::tempdir().expect("project");
    let cases = [
        (
            "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[Fake]] TallyService& tally;\n    void testOne() { Assert::isTrue(true); }\n};\n",
            "no reference",
        ),
        (
            "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[Fake]] int tally = 0;\n    void testOne() { Assert::isTrue(true); }\n};\n",
            "TY1372",
        ),
        (
            "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[ValueSource([1])]]\n    void testTwo(int first, int second) { Assert::isTrue(first < second); }\n};\n",
            "one parameter",
        ),
        (
            "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[ValueSource(1)]]\n    void testOne(int value) { Assert::isTrue(value > 0); }\n};\n",
            "one list of 1 to 256",
        ),
        (
            "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    [[ValueSource([{\"a\": 1}])]]\n    void testOne(int value) { Assert::isTrue(value > 0); }\n};\n",
            "numbers, strings or booleans",
        ),
    ];
    for (unit, expected) in cases {
        UnitProject::report_project(root.path(), unit);
        let refused = UnitProject::refusal(root.path(), "client/tests/ReportServiceTest.hpp");
        assert!(refused.contains(expected), "{expected}: {refused}");
    }
}

#[test]
fn a_fake_in_the_test_setup_is_refused_instead_of_ignored() {
    let root = tempfile::tempdir().expect("project");
    UnitProject::report_project(
        root.path(),
        "class [[Test(ReportTestSetup)]] ReportServiceTest {\npublic:\n    void testOne() { Assert::isTrue(true); }\n};\n",
    );
    UnitProject::write(
        root.path(),
        &[(
            "client/tests/ReportTestSetup.hpp",
            "class [[TestSetup]] ReportTestSetup {\npublic:\n    [[Fake]] TallyService tally;\n};\n",
        )],
    );
    let refused = UnitProject::refusal(root.path(), "client/tests/ReportServiceTest.hpp");
    assert!(
        refused.contains("belongs in the [[Test]] class"),
        "{refused}"
    );
}

#[test]
fn a_constexpr_service_method_still_compiles_with_recorded_calls() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::write(
        root.path(),
        &[
            (
                "client/services/TallyService.hpp",
                "class [[Service]] TallyService {\npublic:\n    constexpr int doubled(int value) const { return value * 2; }\n};\n",
            ),
            (
                "client/tests/TallyTestSetup.hpp",
                "class [[TestSetup]] TallyTestSetup {};\n",
            ),
            (
                "client/tests/TallyServiceTest.hpp",
                "class [[Test(TallyTestSetup)]] TallyServiceTest {\npublic:\n    void testDoubles() {\n        TallyService tally;\n        Assert::equals(4, tally.doubled(2));\n    }\n};\n",
            ),
        ],
    );
    let report = UnitProject::report(root.path(), "client/tests/TallyServiceTest.hpp");
    assert!(report.cases[0].passed, "{report:?}");
    assert_eq!(report.cases[0].called, ["TallyService.doubled"]);
}

#[test]
fn coverage_lists_the_public_methods_of_cpp_services() {
    let root = tempfile::tempdir().expect("project");
    UnitProject::write(root.path(), &UnitProject::FIRST_APP);
    let mut calls = crate::method_coverage::RecordedCalls::default();
    calls.add(
        "client",
        "CounterServiceTest.testIncrementAddsOne",
        &[String::from("CounterService.increment")],
    );
    calls.add_unit("client", "hpp", true);
    let report =
        crate::method_coverage::CoverageReport::build(root.path(), calls).expect("coverage");
    let uncovered: Vec<(String, String)> = report
        .uncovered()
        .into_iter()
        .map(|missing| (missing.type_name, missing.method))
        .collect();
    assert_eq!(report.total(), 2, "{report:?}");
    assert_eq!(
        uncovered,
        [(String::from("CounterService"), String::from("askServer"))]
    );
}

#[test]
fn fresh_start_gives_every_case_new_shared_services() {
    if !UnitProject::has_compiler() {
        return;
    }
    let unit = |marker: &str| {
        format!(
            "class [[Test(ReportTestSetup){marker}]] ReportServiceTest {{\npublic:\n    ReportService report;\n    void testFirstAdds() {{ report.tally.count += 1; Assert::equals(1, report.direct()); }}\n    void testSecondAdds() {{ report.tally.count += 1; Assert::equals(1, report.shared()); }}\n}};\n"
        )
    };
    let root = tempfile::tempdir().expect("project");
    UnitProject::report_project(root.path(), &unit(", FreshStart"));
    let report = UnitProject::report(root.path(), "client/tests/ReportServiceTest.hpp");
    assert_eq!(
        UnitProject::outcomes(&report),
        [
            (String::from("testFirstAdds"), true),
            (String::from("testSecondAdds"), true),
        ],
        "{report:?}"
    );
    let shared = tempfile::tempdir().expect("project");
    UnitProject::report_project(shared.path(), &unit(""));
    let report = UnitProject::report(shared.path(), "client/tests/ReportServiceTest.hpp");
    assert!(!report.cases[1].passed, "{report:?}");
}

#[test]
fn a_checked_request_body_is_refused_before_the_request_leaves() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::write(
        root.path(),
        &[
            (
                "client/models/Note.hpp",
                "#include <string>\nstruct [[Checked]] Note {\n    [[NotEmpty]] std::string text;\n};\n",
            ),
            (
                "client/remotes/NoteClient.hpp",
                "class [[HTTPClient(\"/api\")]] NoteClient {\npublic:\n    [[PostRequest(\"/notes\")]]\n    Task<Note> save([[Check, RequestBody]] Note note);\n};\n",
            ),
            (
                "client/services/NoteService.hpp",
                "class [[Service]] NoteService {\npublic:\n    [[Needs]] NoteClient& notes;\n    std::string saved;\n    Task<void> keep(std::string text) { Note note = co_await notes.save(Note{text}); saved = note.text; }\n    int length([[Check]] const Note& note) { return static_cast<int>(note.text.size()); }\n};\n",
            ),
            (
                "client/tests/NoteTestSetup.hpp",
                "class [[TestSetup]] NoteTestSetup {\npublic:\n    static BehaviorAdapters configure() {\n        BehaviorAdapters adapters;\n        adapters.http[\"note\"] = [](const tachyon::http::HttpRequestMessage& request) {\n            return tachyon::json::JsonNode::parse(request.content->readAsString());\n        };\n        return adapters;\n    }\n};\n",
            ),
            (
                "client/tests/NoteServiceTest.hpp",
                "class [[Test(NoteTestSetup)]] NoteServiceTest {\npublic:\n    NoteService notes;\n    Task<void> testKeepsANote() { co_await notes.keep(\"hi\"); Assert::equals(\"hi\", notes.saved); }\n    Task<void> testRefusesABlankNote() { co_await notes.keep(\"  \"); }\n    void testChecksALength() { Assert::equals(2, notes.length(Note{\"ab\"})); Assert::equals(\"Behavior validation failed: text.notBlank.\", Assert::throws([this] { notes.length(Note{\"\"}); })); }\n};\n",
            ),
        ],
    );
    let report = UnitProject::report(root.path(), "client/tests/NoteServiceTest.hpp");
    assert_eq!(
        UnitProject::outcomes(&report),
        [
            (String::from("testKeepsANote"), true),
            (String::from("testRefusesABlankNote"), false),
            (String::from("testChecksALength"), true),
        ],
        "{report:?}"
    );
    assert_eq!(
        report.cases[1].error.as_deref(),
        Some("Behavior validation failed: text.notBlank."),
        "{report:?}"
    );
}

#[test]
fn a_cached_method_keeps_its_value_within_one_case() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::write(
        root.path(),
        &[
            (
                "client/services/PriceService.hpp",
                "class [[Service, CacheSettings({\"name\":\"prices\",\"ttlMs\":60000,\"maxBytes\":65536,\"maxEntries\":100})]] PriceService {\npublic:\n    int calls = 0;\n    [[Cached]] int price(int item) { calls += 1; return item + calls; }\n};\n",
            ),
            (
                "client/tests/PriceTestSetup.hpp",
                "class [[TestSetup]] PriceTestSetup {};\n",
            ),
            (
                "client/tests/PriceServiceTest.hpp",
                "class [[Test(PriceTestSetup), FreshStart]] PriceServiceTest {\npublic:\n    PriceService prices;\n    void testFirstCallIsKept() { Assert::equals(3, prices.price(2)); Assert::equals(3, prices.price(2)); }\n    void testNextCaseStartsEmpty() { Assert::equals(3, prices.price(2)); }\n};\n",
            ),
        ],
    );
    let report = UnitProject::report(root.path(), "client/tests/PriceServiceTest.hpp");
    assert_eq!(
        UnitProject::outcomes(&report),
        [
            (String::from("testFirstCallIsKept"), true),
            (String::from("testNextCaseStartsEmpty"), true),
        ],
        "{report:?}"
    );
}

#[test]
fn a_store_keeps_its_rows_in_memory_within_one_case() {
    if !UnitProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    UnitProject::write(
        root.path(),
        &[
            (
                "client/entities/NoteEntity.hpp",
                "struct [[Entity({\"adapter\":\"indexeddb\"}), Table(\"notes\")]] NoteEntity {\n    [[Id]] std::string id;\n    std::string text;\n};\n",
            ),
            (
                "client/stores/NoteStore.hpp",
                "class [[Store(NoteEntity, {\"fields\":[\"id\",\"text\"],\"defaultSize\":20,\"maxSize\":100})]] NoteStore : public tachyon::Repository<NoteEntity, std::string> {\npublic:\n    long long countByText(const std::string& text);\n};\n",
            ),
            (
                "client/services/NotesService.hpp",
                "class [[Service]] NotesService {\npublic:\n    [[Needs]] NoteStore& notes;\n    long long keep(std::string text) { notes.save(NoteEntity{text, text}); return notes.countByText(text); }\n    [[AllOrNothing({\"adapter\":\"indexeddb\"})]] void refuse() { notes.save(NoteEntity{\"draft\", \"draft\"}); throw std::runtime_error(\"refused\"); }\n};\n",
            ),
            (
                "client/tests/NotesTestSetup.hpp",
                "class [[TestSetup]] NotesTestSetup {};\n",
            ),
            (
                "client/tests/NotesServiceTest.hpp",
                "class [[Test(NotesTestSetup), FreshStart]] NotesServiceTest {\npublic:\n    NotesService service;\n    NoteStore store;\n    void testKeepsANote() { Assert::equals(1LL, service.keep(\"a\")); Assert::equals(1LL, service.keep(\"a\")); }\n    void testRollsBack() { try { service.refuse(); } catch (const std::exception&) {} Assert::equals(0LL, store.countByText(\"draft\")); Assert::equals(0LL, store.countByText(\"a\")); }\n};\n",
            ),
        ],
    );
    let report = UnitProject::report(root.path(), "client/tests/NotesServiceTest.hpp");
    assert_eq!(
        UnitProject::outcomes(&report),
        [
            (String::from("testKeepsANote"), true),
            (String::from("testRollsBack"), true),
        ],
        "{report:?}"
    );
}
