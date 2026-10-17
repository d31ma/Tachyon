#![allow(clippy::expect_used)]

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct PageProject;

impl PageProject {
    const OWNER: &'static str = "00000000-0000-4000-8000-00000000000";
    const HOST: &'static str = r#"
#line 1 "tachyon-page-host.cpp"
extern "C" {
void tac_host_emit(const char* text, int length) { std::fprintf(stderr, "event %.*s\n", length, text); }
char* tac_host_storage(const char*, int) { return nullptr; }
char* tac_host_web(const char*, int) { return nullptr; }
double tac_host_epoch_ms() { return tachyonPageClock; }
void tac_host_http_start(std::uint32_t, const char*, int) {}
void tac_host_task_done(std::uint32_t, const char*, int) {}
void tac_host_http_cancel(std::uint32_t) {}
}
int main() {
    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.rfind("clock ", 0) == 0) {
            tachyonPageClock = std::stod(line.substr(6));
            continue;
        }
        char* answer = tac_native_invoke(line.c_str());
        std::printf("%s\n", answer);
        tac_native_free(answer);
    }
    return 0;
}
"#;

    fn has_compiler() -> bool {
        let found = Command::new(super::CppTools::test_compiler())
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

    fn owner(number: usize) -> String {
        format!("{}{number}", Self::OWNER)
    }

    fn init(service: &str, owner: usize, props: &Value) -> Value {
        json!({"op": "init", "route": "web", "service": service, "owner": Self::owner(owner), "props": props})
    }

    fn get(service: &str, owner: usize, name: &str) -> Value {
        json!({"op": "get", "route": "web", "service": service, "owner": Self::owner(owner), "name": name})
    }

    fn call(service: &str, owner: usize, name: &str, args: &Value) -> Value {
        json!({"op": "call", "route": "web", "service": service, "owner": Self::owner(owner), "name": name, "args": args})
    }

    fn program(root: &Path, services: &[(&str, &str)]) -> String {
        let chosen: Vec<(String, String, PathBuf)> = services
            .iter()
            .map(|(instance, class)| {
                (
                    (*instance).to_owned(),
                    (*class).to_owned(),
                    root.join(format!("client/services/{class}.hpp")),
                )
            })
            .collect();
        super::CppProgram::read(root, &chosen)
            .expect("program")
            .web_source()
    }

    fn answers(root: &Path, services: &[(&str, &str)], requests: &[Value]) -> Vec<Value> {
        let lines: Vec<String> = requests.iter().map(Value::to_string).collect();
        Self::answered_lines(root, services, &lines)
    }

    fn answered_lines(root: &Path, services: &[(&str, &str)], lines: &[String]) -> Vec<Value> {
        let source = format!(
            "#include <iostream>\nstatic double tachyonPageClock = 0;\n{}{}",
            Self::program(root, services),
            Self::HOST
        );
        let staged = tempfile::tempdir().expect("staged");
        std::fs::write(staged.path().join("page.cpp"), source).expect("source");
        let compiled = Command::new(super::CppTools::test_compiler())
            .args([
                "-std=c++20",
                "-O0",
                "-w",
                "-fno-color-diagnostics",
                "page.cpp",
                "-o",
                "page",
            ])
            .current_dir(staged.path())
            .output()
            .expect("compiler");
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let mut child = Command::new(staged.path().join("page"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("page");
        let input = format!("{}\n", lines.join("\n"));
        std::io::Write::write_all(&mut child.stdin.take().expect("stdin"), input.as_bytes())
            .expect("requests");
        let output = child.wait_with_output().expect("answers");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| serde_json::from_str(line).expect("answer"))
            .collect()
    }
}

#[test]
fn the_page_program_answers_its_bridge_requests_natively() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[(
            "client/services/TallyService.hpp",
            "class [[Service]] TallyService {\npublic:\n    int count = 0;\n    void add(int amount) { count += amount; }\n};\n",
        )],
    );
    let services = [("tally", "TallyService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("tally", 1, &json!({})),
            PageProject::call("tally", 1, "add", &json!([2])),
            PageProject::init("tally", 2, &json!({})),
            PageProject::get("tally", 2, "count"),
        ],
    );
    assert_eq!(answers[3]["value"], 2, "{answers:?}");
}

#[test]
fn a_prototype_service_gives_each_component_its_own_object() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/services/TallyService.hpp",
                "class [[Service, Lifetime({\"value\":\"prototype\"})]] TallyService {\npublic:\n    int count = 0;\n    void add(int amount) { count += amount; }\n};\n",
            ),
            (
                "client/services/BoardService.hpp",
                "class [[Service]] BoardService {\npublic:\n    [[Needs]] TallyService& tally;\n    int read() { return tally.count; }\n    void add() { tally.add(5); }\n};\n",
            ),
        ],
    );
    let services = [("tally", "TallyService"), ("board", "BoardService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("tally", 1, &json!({})),
            PageProject::call("tally", 1, "add", &json!([2])),
            PageProject::init("tally", 2, &json!({})),
            PageProject::get("tally", 2, "count"),
            PageProject::get("tally", 1, "count"),
            PageProject::init("board", 3, &json!({})),
            PageProject::call("board", 3, "add", &json!([])),
            PageProject::call("board", 3, "read", &json!([])),
            PageProject::get("tally", 1, "count"),
        ],
    );
    assert_eq!(answers[3]["value"], 0, "{answers:?}");
    assert_eq!(answers[4]["value"], 2, "{answers:?}");
    assert_eq!(answers[7]["value"], 5, "{answers:?}");
    assert_eq!(answers[8]["value"], 2, "{answers:?}");
}

#[test]
fn needs_all_fills_props_and_collaborators_for_each_component() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/services/TallyService.hpp",
                "class [[Service]] TallyService {\npublic:\n    int count = 3;\n};\n",
            ),
            (
                "client/services/LabelService.hpp",
                "#include <string>\nclass [[Service, NeedsAll]] LabelService {\npublic:\n    std::string title;\n    int startAt;\n    TallyService& tally;\n    int clicks = 0;\n    std::string shown() { return title + \":\" + std::to_string(startAt + tally.count + clicks); }\nprivate:\n    std::string hidden;\n};\n",
            ),
        ],
    );
    let services = [("label", "LabelService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init(
                "label",
                1,
                &json!({"title": "A", "startAt": 1, "hidden": "x", "clicks": 9}),
            ),
            PageProject::init("label", 2, &json!({"title": "B", "startAt": 10})),
            PageProject::call("label", 1, "shown", &json!([])),
            PageProject::call("label", 2, "shown", &json!([])),
            PageProject::init("label", 3, &json!({"title": 4})),
        ],
    );
    assert_eq!(answers[2]["value"], "A:4", "{answers:?}");
    assert_eq!(answers[3]["value"], "B:13", "{answers:?}");
    assert!(answers[4]["error"].is_string(), "{answers:?}");
}

#[test]
fn named_chooses_the_service_that_fills_a_base_class_field() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/models/Greeter.hpp",
                "#include <string>\nclass Greeter {\npublic:\n    virtual ~Greeter() = default;\n    virtual std::string greet() = 0;\n};\n",
            ),
            (
                "client/services/LoudGreeterService.hpp",
                "class [[Service]] LoudGreeterService : public Greeter {\npublic:\n    std::string greet() override { return \"HI\"; }\n};\n",
            ),
            (
                "client/services/WelcomeService.hpp",
                "class [[Service]] WelcomeService {\npublic:\n    [[Needs, Named(\"LoudGreeterService\")]] Greeter& greeter;\n    std::string welcome() { return greeter.greet(); }\n};\n",
            ),
        ],
    );
    let services = [("welcome", "WelcomeService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("welcome", 1, &json!({})),
            PageProject::call("welcome", 1, "welcome", &json!([])),
        ],
    );
    assert_eq!(answers[1]["value"], "HI", "{answers:?}");
}

#[test]
fn a_checked_argument_is_refused_before_the_method_runs() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/models/SignUp.hpp",
                "#include <optional>\n#include <string>\n#include <vector>\nstruct [[Checked]] SignUp {\n    [[NotEmpty, Length({\"max\":8})]] std::string name;\n    [[Required]] std::optional<std::string> email;\n    [[Min(18), Max(130)]] int age = 0;\n    [[Length({\"min\":1})]] std::vector<std::string> tags;\n    [[Min(0)]] std::optional<double> score;\n};\n",
            ),
            (
                "client/services/JoinService.hpp",
                "class [[Service]] JoinService {\npublic:\n    int joined = 0;\n    int join([[Check]] const SignUp& form) { joined += 1; return form.age; }\n};\n",
            ),
        ],
    );
    let good = json!({"name": "Ada", "email": "a@b.c", "age": 36, "tags": ["x"]});
    let with = |key: &str, value: Value| {
        let mut form = good.clone();
        form[key] = value;
        PageProject::call("join", 1, "join", &json!([form]))
    };
    let mut missing_email = good.clone();
    missing_email.as_object_mut().expect("form").remove("email");
    let answers = PageProject::answers(
        root.path(),
        &[("join", "JoinService")],
        &[
            PageProject::init("join", 1, &json!({})),
            PageProject::call("join", 1, "join", &json!([good])),
            with("name", json!(" \u{a0}")),
            with("name", json!("Ada Lovelace")),
            PageProject::call("join", 1, "join", &json!([missing_email])),
            with("age", json!(17)),
            with("age", json!(131)),
            with("tags", json!([])),
            with("score", json!(-1)),
            with("score", json!(2.5)),
            PageProject::get("join", 1, "joined"),
        ],
    );
    assert_eq!(answers[1]["value"], 36, "{answers:?}");
    let failed: Vec<&str> = answers[2..9]
        .iter()
        .map(|answer| answer["error"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        failed,
        [
            "Behavior validation failed: name.notBlank.",
            "Behavior validation failed: name.size.",
            "Behavior validation failed: email.notNull.",
            "Behavior validation failed: age.min.",
            "Behavior validation failed: age.max.",
            "Behavior validation failed: tags.size.",
            "Behavior validation failed: score.min.",
        ],
        "{answers:?}"
    );
    assert_eq!(answers[2]["code"], "validation", "{answers:?}");
    assert_eq!(answers[9]["value"], 36, "{answers:?}");
    assert_eq!(answers[10]["value"], 2, "{answers:?}");
}

#[test]
fn a_converter_fills_a_model_and_a_json_value_from_its_plan() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/models/Detail.hpp",
                "struct Detail {\n    std::string id;\n};\n",
            ),
            (
                "client/models/Label.hpp",
                "struct Label {\n    std::string text;\n    int size = 0;\n};\n",
            ),
            (
                "client/models/Badge.hpp",
                "struct Badge {\n    std::string text;\n};\n",
            ),
            (
                "client/models/Product.hpp",
                "struct Product {\n    std::string name;\n    int price = 0;\n    Detail detail;\n    std::optional<std::string> note;\n    std::vector<Label> labels;\n};\n",
            ),
            (
                "client/models/Summary.hpp",
                "struct Summary {\n    std::string name;\n    double price = 0;\n    std::string productId;\n    std::string kind;\n    std::optional<std::string> note;\n    std::vector<Badge> labels;\n};\n",
            ),
            (
                "client/converters/ProductConverter.hpp",
                "class [[Converter]] ProductConverter {\npublic:\n    [[Convert({\"fields\":[{\"source\":\"detail.id\",\"target\":\"productId\"},{\"target\":\"kind\",\"constant\":\"product\"}]})]]\n    Summary toSummary(const Product& product);\n    [[Convert({\"fields\":[{\"source\":\"a.b\",\"target\":\"value\"},{\"source\":\"a.missing\",\"target\":\"gone\"},{\"target\":\"fixed\",\"constant\":3}]})]]\n    tachyon::json::JsonNode reshape(const tachyon::json::JsonNode& input);\n};\n",
            ),
            (
                "client/services/ShopService.hpp",
                "class [[Service]] ShopService {\npublic:\n    [[Needs]] ProductConverter& converter;\n    std::string summary() {\n        Product product{\"Lamp\", 12, Detail{\"p-7\"}, std::nullopt, {Label{\"new\", 2}}};\n        return tachyon::json::JsonSerializer::serialize(converter.toSummary(product));\n    }\n    std::string reshaped() {\n        return converter.reshape(tachyon::json::JsonNode::parse(R\"({\"a\":{\"b\":[1,2]}})\")).toJsonString();\n    }\n};\n",
            ),
        ],
    );
    let services = [("shop", "ShopService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("shop", 1, &json!({})),
            PageProject::call("shop", 1, "summary", &json!([])),
            PageProject::call("shop", 1, "reshaped", &json!([])),
        ],
    );
    let summary: Value =
        serde_json::from_str(answers[1]["value"].as_str().expect("summary")).expect("json");
    assert_eq!(
        summary,
        json!({"name": "Lamp", "price": 12, "productId": "p-7", "kind": "product", "note": null, "labels": [{"text": "new"}]}),
        "{answers:?}"
    );
    let reshaped: Value =
        serde_json::from_str(answers[2]["value"].as_str().expect("reshaped")).expect("json");
    assert_eq!(
        reshaped,
        json!({"value": [1, 2], "gone": null, "fixed": 3}),
        "{answers:?}"
    );
}

#[test]
fn a_helper_does_part_of_a_service_s_work() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/helpers/RateHelper.hpp",
                "class [[Helper]] RateHelper {\npublic:\n    int rate = 3;\n};\n",
            ),
            (
                "client/helpers/PriceHelper.hpp",
                "class [[Helper]] PriceHelper {\npublic:\n    [[Needs]] RateHelper& rates;\n    int price(int count) { return count * rates.rate; }\n};\n",
            ),
            (
                "client/services/CartService.hpp",
                "class [[Service]] CartService {\npublic:\n    [[Needs]] PriceHelper& prices;\n    int total(int count) { return prices.price(count); }\n};\n",
            ),
        ],
    );
    let services = [("cart", "CartService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("cart", 1, &json!({})),
            PageProject::call("cart", 1, "total", &json!([4])),
        ],
    );
    assert_eq!(answers[1]["value"], 12, "{answers:?}");
}
