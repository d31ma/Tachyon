#![allow(clippy::expect_used)]

use super::{CppFile, CppMemberKind, CppService};
use std::path::Path;

struct Sources;

impl Sources {
    const TALLY: &'static str = r#"#pragma once
#include <string>
#include "CounterService.hpp"

class [[Service]] TallyService {
public:
    int count = 0;
    const std::string label = "Tally";
    [[Needs]] CounterService& counter;
    ApplicationEventPublisher applicationEventPublisher;

    void add(int amount) { count += amount; }
    PlacedOrder place(const PlacedOrder& order) {
        applicationEventPublisher.publishEvent(order, true);
        return order;
    }

    [[OnEvent({"replay":true})]] [[Order(5)]]
    void heard(const PlacedOrder& order) { count += order.total; }

    [[OnStart]]
    void start() { count = 1; }

private:
    int hidden = 0;
    void secret() {}
};
"#;

    fn service(source: &str) -> CppService {
        CppService::read(Path::new("client/services/TallyService.hpp"), source).expect("service")
    }

    fn refusal(file: &str, source: &str) -> String {
        match CppService::read(Path::new(file), source) {
            Ok(_) => panic!("{file} should be refused"),
            Err(failure) => failure.to_string(),
        }
    }
}

#[test]
fn a_service_class_shows_its_public_fields_and_methods() {
    let service = Sources::service(Sources::TALLY);
    let fields: Vec<&str> = service
        .page_fields()
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    let methods: Vec<&str> = service
        .page_methods()
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    assert_eq!(fields, ["count", "label"]);
    assert_eq!(methods, ["add", "place"]);
    assert_eq!(
        service.needs(),
        [(String::from("counter"), String::from("CounterService"))]
    );
}

#[test]
fn listeners_and_lifecycle_hooks_are_described_for_the_page() {
    let metadata = Sources::service(Sources::TALLY).metadata();
    assert_eq!(metadata["starts"], serde_json::json!(["start"]));
    assert_eq!(metadata["page"]["listeners"][0]["key"], "heard");
    assert_eq!(metadata["page"]["listeners"][0]["events"][0], "PlacedOrder");
    assert_eq!(metadata["page"]["listeners"][0]["replay"], true);
    assert_eq!(metadata["page"]["listeners"][0]["order"], 5);
}

#[test]
fn lowering_removes_markers_and_fills_needs_fields() {
    let service = Sources::service(Sources::TALLY);
    let lowered = super::CppLowering::service(&service, &[]);
    assert!(!lowered.contains("[[Service]]"), "{lowered}");
    assert!(!lowered.contains("#pragma once"), "{lowered}");
    assert!(!lowered.contains("CounterService.hpp"), "{lowered}");
    assert!(lowered.contains("#include <string>"), "{lowered}");
    assert!(
        lowered.contains("CounterService& counter = ::tachyon::TacNeeds::of<CounterService>();"),
        "{lowered}"
    );
    assert!(
        lowered.contains("{ friend class TachyonBindingTallyService;"),
        "{lowered}"
    );
    assert_eq!(lowered.lines().count(), Sources::TALLY.lines().count());
}

#[test]
fn members_know_their_kind() {
    let file = CppFile::read(Path::new("client/models/Order.hpp"), "struct Order {\n    Order() = default;\n    ~Order() {}\n    bool operator==(const Order& other) const = default;\n    std::vector<std::string> notes{};\n    std::function<int(int)> map = [](int value) { return value; };\n    static int made;\n};\n").expect("model");
    let kinds: Vec<(CppMemberKind, &str)> = file
        .class
        .members
        .iter()
        .map(|member| (member.kind, member.name.as_str()))
        .collect();
    assert_eq!(
        kinds,
        [
            (CppMemberKind::Constructor, "Order"),
            (CppMemberKind::Destructor, "Order"),
            (CppMemberKind::Operator, ""),
            (CppMemberKind::Field, "notes"),
            (CppMemberKind::Field, "map"),
            (CppMemberKind::Field, "made"),
        ]
    );
    assert!(file.class.members[5].is_static);
}

#[test]
fn lowercase_namespaced_and_unknown_markers_are_refused() {
    let lower = Sources::refusal(
        "client/services/TallyService.hpp",
        "class [[service]] TallyService {};\n",
    );
    assert!(
        lower.contains("TY1371") && lower.contains("write [[Service]], not [[service]]"),
        "{lower}"
    );
    let spaced = Sources::refusal(
        "client/services/TallyService.hpp",
        "class [[tachyon::Service]] TallyService {};\n",
    );
    assert!(spaced.contains("no namespace"), "{spaced}");
    let typo = Sources::refusal(
        "client/services/TallyService.hpp",
        "class [[Servise]] TallyService {};\n",
    );
    assert!(typo.contains("did you mean [[Service]]?"), "{typo}");
    let standard = Sources::service(
        "class [[Service]] TallyService {\npublic:\n    [[nodiscard]] int count() const { return 1; }\n};\n",
    );
    assert_eq!(standard.page_methods().len(), 1);
}

#[test]
fn a_marker_before_the_class_key_is_refused() {
    let refused = Sources::refusal(
        "client/services/TallyService.hpp",
        "[[Service]] class TallyService {};\n",
    );
    assert!(
        refused.contains("write `class [[Service]] OrderService`"),
        "{refused}"
    );
}

#[test]
fn a_file_holds_one_class_named_after_it() {
    let second = Sources::refusal(
        "client/services/TallyService.hpp",
        "class TallyService {};\nclass Other {};\n",
    );
    assert!(second.contains("holds one class"), "{second}");
    let named = Sources::refusal(
        "client/services/TallyService.hpp",
        "class CountService {};\n",
    );
    assert!(named.contains("named after its file"), "{named}");
    let outside = Sources::refusal(
        "client/services/TallyService.hpp",
        "int helper() { return 1; }\nclass TallyService {};\n",
    );
    assert!(outside.contains("outside the class"), "{outside}");
}

#[test]
fn needs_fields_are_references_without_a_value() {
    let refused = Sources::refusal(
        "client/services/TallyService.hpp",
        "class TallyService {\n    [[Needs]] CounterService counter;\n};\n",
    );
    assert!(
        refused.contains("[[Needs]] OrderStore& orders;"),
        "{refused}"
    );
}

#[test]
fn markers_follow_the_shared_rule_table() {
    let refused = Sources::refusal(
        "client/services/TallyService.hpp",
        "class TallyService {\npublic:\n    [[LocalState]] int count = 0;\n};\n",
    );
    assert!(
        refused.contains("TY1308") && refused.contains("is not available in C++ Tac code"),
        "{refused}"
    );
    let pending = Sources::refusal(
        "client/services/TallyService.hpp",
        "class [[Store]] TallyService {};\n",
    );
    assert!(
        pending.contains("C++ gets it in a later step (ADR 0099)"),
        "{pending}"
    );
    let partner = Sources::refusal(
        "client/services/TallyService.hpp",
        "class TallyService {\npublic:\n    [[Order(1)]] void heard(const PlacedOrder& order) {}\n};\n",
    );
    assert!(partner.contains("[[Order]] needs [[OnEvent]]"), "{partner}");
    let misplaced = Sources::refusal(
        "client/services/TallyService.hpp",
        "class TallyService {\npublic:\n    [[Service]] int count = 0;\n};\n",
    );
    assert!(
        misplaced.contains("[[Service]] is on a field, but it belongs on a class"),
        "{misplaced}"
    );
}

#[test]
fn raw_strings_and_digit_separators_stay_inside_their_tokens() {
    let source = "class A {\n    std::string text = R\"json({\"a\": \"}\"})json\";\n    int big = 1'000'000;\n    char quote = '\\'';\n};\n";
    let file = CppFile::read(Path::new("client/models/A.hpp"), source).expect("model");
    let names: Vec<&str> = file
        .class
        .members
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    assert_eq!(names, ["text", "big", "quote"]);
}

#[test]
fn emscripten_interop_is_refused_with_ty1317() {
    let problems = crate::annotation_layout::BrowserInteropCheck::problems(
        Path::new("client/services/TallyService.hpp"),
        "#include <emscripten/val.h>\nclass TallyService {\n    void run() { EM_ASM(alert(1)); }\n};\n",
    );
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems[0]
            .message
            .starts_with("C++ services cannot call browser APIs"),
        "{problems:?}"
    );
}

#[test]
fn a_cpp_test_unit_runs_its_cases_with_the_native_compiler() {
    if std::process::Command::new(super::CppTools::test_compiler())
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: no C++ test compiler");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    let files = [
        (
            "client/models/Order.hpp",
            "#pragma once\n#include <string>\n\nstruct Order {\n    std::string status;\n    int total = 0;\n};\n",
        ),
        (
            "client/services/BasketService.hpp",
            "#pragma once\n#include \"../models/Order.hpp\"\n\nclass [[Service]] BasketService {\npublic:\n    int count = 0;\n    void add(int amount) { count += amount; }\n};\n",
        ),
        (
            "client/tests/BasketTestSetup.hpp",
            "class [[TestSetup]] BasketTestSetup {};\n",
        ),
        (
            "client/tests/BasketServiceTest.hpp",
            "class [[Test(BasketTestSetup)]] BasketServiceTest {\npublic:\n    void testAdds() {\n        BasketService basket;\n        basket.add(2);\n        Assert::equals(2, basket.count);\n    }\n    void testFails() {\n        Assert::equals(3, 2, \"the total\");\n    }\n    void testReadsAnOrder() {\n        Order order = tachyon::json::JsonSerializer::deserialize<Order>(R\"({\"total\":4})\");\n        Assert::equals(4, order.total);\n    }\n};\n",
        ),
    ];
    for (relative, source) in files {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(path, source).expect("file");
    }
    let report =
        crate::client_tests::run(root.path(), Path::new("client/tests/BasketServiceTest.hpp"))
            .expect("report");
    let results: Vec<(String, bool)> = report
        .cases
        .iter()
        .map(|case| (case.name.clone(), case.passed))
        .collect();
    assert_eq!(
        results,
        [
            (String::from("testAdds"), true),
            (String::from("testFails"), false),
            (String::from("testReadsAnOrder"), true),
        ]
    );
    assert_eq!(
        report.cases[1].error.as_deref(),
        Some("the total ==> expected: <3> but was: <2>")
    );
}

#[test]
fn a_cpp_test_unit_names_its_setup() {
    let root = tempfile::tempdir().expect("project");
    let path = root.path().join("client/tests/BasketServiceTest.hpp");
    std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
    std::fs::write(&path, "class [[Test(BasketTestSetup)]] BasketServiceTest {\npublic:\n    void testAdds() {}\n};\n").expect("file");
    let refused =
        crate::client_tests::run(root.path(), Path::new("client/tests/BasketServiceTest.hpp"))
            .expect_err("no setup");
    assert!(
        refused.to_string().contains("[[TestSetup]] TallyTestSetup"),
        "{refused}"
    );
}

#[test]
fn an_http_client_gets_its_request_methods_written() {
    let source = "class [[HTTPClient(\"/api\")]] OrderClient {\npublic:\n    [[GetRequest(\"/orders/{id}\")]]\n    Task<Order> find(int id, [[RequestParam]] int page) const;\n\n    [[PostRequest(\"/orders\")]]\n    Task<void> save([[RequestBody]] const Order& order, [[RequestHeader(\"X-Trace\")]] std::string trace);\n};\n";
    let service =
        CppService::read(Path::new("client/remotes/OrderClient.hpp"), source).expect("remote");
    assert!(service.is_remote());
    assert_eq!(service.binding(), "");
    let lowered = super::CppLowering::service(&service, &[]);
    assert!(
        lowered.contains("inline Task<Order> OrderClient::find(int id, int page) const {"),
        "{lowered}"
    );
    assert!(lowered.contains("tachyon::http::HttpMethod::Get, std::string() + \"/api/orders/\" + tachyon::http::TacBrowserRequest::segment(id) + \"\""), "{lowered}");
    assert!(
        lowered.contains("tachyon::http::TacBrowserRequest::addQuery(request, \"page\", page);"),
        "{lowered}"
    );
    assert!(
        lowered.contains(
            "co_return co_await tachyon::http::TacRemoteCall::answer<Order>(\"order\", std::move(request));"
        ),
        "{lowered}"
    );
    assert!(
        lowered.contains("request.content = tachyon::http::JsonContent::create(order);"),
        "{lowered}"
    );
    assert!(
        lowered.contains(
            "request.headers.add(\"X-Trace\", tachyon::http::TacBrowserRequest::text(trace));"
        ),
        "{lowered}"
    );
    assert!(lowered.contains("answer<void>"), "{lowered}");
}

#[test]
fn an_http_client_method_needs_one_request_marker_and_a_task() {
    let refused = Sources::refusal(
        "client/remotes/OrderClient.hpp",
        "class [[HTTPClient(\"/api\")]] OrderClient {\npublic:\n    Task<int> count();\n};\n",
    );
    assert!(refused.contains("has one request marker"), "{refused}");
    let plain = Sources::refusal(
        "client/remotes/OrderClient.hpp",
        "class [[HTTPClient(\"/api\")]] OrderClient {\npublic:\n    [[GetRequest(\"/count\")]]\n    int count();\n};\n",
    );
    assert!(plain.contains("returns Task<Answer>"), "{plain}");
}

#[test]
fn task_methods_are_asynchronous_on_the_page() {
    let service = Sources::service(
        "class [[Service]] TallyService {\npublic:\n    Task<void> load() { co_return; }\n    int count() { return 1; }\n};\n",
    );
    assert_eq!(
        service.metadata()["page"]["asyncMethods"],
        serde_json::json!(["load"])
    );
}

#[test]
fn a_cpp_model_lives_in_client_models() {
    let source = "#pragma once\n#include <string>\n\nstruct Order {\n    std::string status;\n};\n";
    crate::model_layout::validate(Path::new("client/models/Order.hpp"), source)
        .expect("client model");
    let refused = crate::model_layout::validate(Path::new("server/models/Order.hpp"), source)
        .expect_err("server model");
    assert!(refused.to_string().contains("TY1372"), "{refused}");
}

#[test]
fn doctor_lists_emscripten_and_the_test_compiler_for_cpp_services() {
    let rows = super::CppTools::toolchains();
    let commands: Vec<&str> = rows.iter().map(|row| row.command.as_str()).collect();
    assert_eq!(
        commands,
        [
            crate::web_runtime::EmscriptenTool::EMCC,
            super::CppTools::test_compiler().as_str()
        ]
    );
    assert!(rows[0].required && !rows[1].required);
    assert!(
        rows[0]
            .install
            .contains(super::CppTools::EMSCRIPTEN_VERSION),
        "{}",
        rows[0].install
    );
}

#[test]
fn lifetime_needs_all_and_named_forms_are_checked() {
    let lifetime = |value: &str| {
        Sources::refusal(
            "client/services/TallyService.hpp",
            &format!("class [[Service, Lifetime({value})]] TallyService {{}};\n"),
        )
    };
    assert!(
        lifetime("{\"value\":\"application\"}").contains("no native host"),
        "application"
    );
    assert!(
        lifetime("{\"value\":\"often\"}").contains("singleton"),
        "often"
    );
    assert!(
        lifetime("\"prototype\"").contains("JSON object"),
        "bare string"
    );
    let constructor = Sources::refusal(
        "client/services/TallyService.hpp",
        "class [[Service, NeedsAll]] TallyService {\npublic:\n    TallyService() {}\n    int count;\n};\n",
    );
    assert!(
        constructor.contains("declares no constructor"),
        "{constructor}"
    );
    let empty = Sources::refusal(
        "client/services/TallyService.hpp",
        "class [[Service, NeedsAll]] TallyService {\npublic:\n    std::optional<int> count;\n};\n",
    );
    assert!(
        empty.contains("TY1331") && empty.contains("never empty"),
        "{empty}"
    );
    let named = Sources::refusal(
        "client/services/TallyService.hpp",
        "class [[Service]] TallyService {\npublic:\n    [[Needs, Named(loud)]] Greeter& greeter;\n};\n",
    );
    assert!(named.contains("[[Named]] takes the class name"), "{named}");
    let service = Sources::service(
        "class [[Service, NeedsAll]] TallyService {\npublic:\n    std::string title;\n    CounterService& counter;\n    int clicks = 0;\n    [[Needs, Named(\"LoudGreeterService\")]] Greeter& greeter;\n};\n",
    );
    assert!(service.is_prototype());
    assert_eq!(
        service.needs(),
        [
            (String::from("counter"), String::from("CounterService")),
            (String::from("greeter"), String::from("LoudGreeterService")),
        ]
    );
    let singleton = Sources::service(
        "class [[Service, NeedsAll, Lifetime({\"value\":\"singleton\"})]] TallyService {\npublic:\n    std::string title;\n};\n",
    );
    assert!(!singleton.is_prototype());
}
