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
double tac_host_monotonic_ms() { static double ticks = 0; ticks += 1; return ticks; }
char* tac_host_storage(const char* text, int length) {
    static std::map<std::string, tachyon::json::JsonNode> entries;
    tachyon::json::JsonNode request = tachyon::json::JsonNode::parse(std::string_view(text, length));
    std::fprintf(stderr, "storage %s\n", request.toJsonString().c_str());
    std::string operation = request["op"].getValue<std::string>();
    if (operation == "entity") {
        tachyon::json::JsonObject answer;
        answer["value"] = tachyon::TacMemoryEntities::execute(request["command"]);
        std::string written = answer.toJsonString();
        char* copied = static_cast<char*>(std::malloc(written.size() + 1));
        std::memcpy(copied, written.c_str(), written.size() + 1);
        return copied;
    }
    std::string key = request["cache"].toJsonString() + request["key"].toJsonString();
    tachyon::json::JsonObject answer;
    if (operation == "cache.get") {
        tachyon::json::JsonObject entry;
        auto found = entries.find(key);
        entry["hit"] = found != entries.end();
        if (found != entries.end()) {
            entry["value"] = found->second["value"];
            entry["expiresAt"] = found->second["expiresAt"];
        }
        answer["value"] = entry;
    } else if (operation == "cache.set") {
        entries[key] = request;
        answer["value"] = nullptr;
    } else if (operation == "cache.delete") {
        entries.erase(key);
        answer["value"] = nullptr;
    } else {
        answer["error"] = "no such storage request";
    }
    std::string written = answer.toJsonString();
    char* copied = static_cast<char*>(std::malloc(written.size() + 1));
    std::memcpy(copied, written.c_str(), written.size() + 1);
    return copied;
}
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
            "#include <iostream>\nstatic double tachyonPageClock = 0;\n{}{}{}",
            Self::program(root, services),
            super::CppPrelude::MEMORY_ENTITIES,
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

#[test]
fn behavior_markers_wrap_a_synchronous_method() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/models/Page.hpp",
                "struct Page {\n    int page = 0;\n    std::optional<int> size;\n    std::vector<std::string> sort;\n};\n",
            ),
            (
                "client/services/PriceService.hpp",
                "class [[Service, CacheSettings({\"name\":\"prices\",\"ttlMs\":60000,\"maxBytes\":65536,\"maxEntries\":100})]] PriceService {\npublic:\n    int calls = 0;\n    int failures = 2;\n    [[Cached]] int price([[CacheKey]] int item, int ignored) { calls += 1; return item * 10 + calls; }\n    [[UpdateCache({\"method\":\"price\"})]] int reprice(int item) { return item * 100; }\n    [[ClearCache({\"method\":\"price\"})]] void forget(int item) {}\n    [[Retry({\"maxRetries\":3})]] int flaky() {\n        if (failures > 0) {\n            failures -= 1;\n            throw std::runtime_error(\"try again\");\n        }\n        return 7;\n    }\n    [[MaxAtOnce({\"limit\":1,\"policy\":\"reject\"})]] int nested(int depth) { return depth == 0 ? 0 : nested(depth - 1); }\n    int pageSize([[PageDefaults({\"defaultSize\":20,\"maxSize\":100,\"sort\":[\"id\"]})]] const Page& page) { return page.size.value_or(-1); }\n};\n",
            ),
        ],
    );
    let services = [("prices", "PriceService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("prices", 1, &json!({})),
            PageProject::call("prices", 1, "price", &json!([2, 0])),
            PageProject::call("prices", 1, "price", &json!([2, 9])),
            PageProject::call("prices", 1, "reprice", &json!([2])),
            PageProject::call("prices", 1, "price", &json!([2, 0])),
            PageProject::call("prices", 1, "forget", &json!([2])),
            PageProject::call("prices", 1, "price", &json!([2, 0])),
            PageProject::call("prices", 1, "flaky", &json!([])),
            PageProject::call("prices", 1, "nested", &json!([1])),
            PageProject::call("prices", 1, "pageSize", &json!([{}])),
            PageProject::call("prices", 1, "pageSize", &json!([{"size": 500}])),
        ],
    );
    let values: Vec<Value> = answers
        .iter()
        .map(|answer| answer["value"].clone())
        .collect();
    assert_eq!(
        values[1..8],
        [
            json!(21),
            json!(21),
            json!(200),
            json!(200),
            Value::Null,
            json!(22),
            json!(7)
        ],
        "{answers:?}"
    );
    assert_eq!(answers[8]["code"], "capacity", "{answers:?}");
    assert_eq!(answers[9]["value"], 20, "{answers:?}");
    assert_eq!(answers[10]["code"], "invalid", "{answers:?}");
}

#[test]
fn a_store_saves_and_finds_entities_and_all_or_nothing_rolls_back() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/entities/NoteEntity.hpp",
                "struct [[Entity({\"adapter\":\"indexeddb\"}), Table(\"notes\")]] NoteEntity {\n    [[Id]] std::string id;\n    std::string text;\n    int stars = 0;\n};\n",
            ),
            (
                "client/stores/NoteStore.hpp",
                "class [[Store(NoteEntity, {\"fields\":[\"id\",\"text\",\"stars\"],\"defaultSize\":2,\"maxSize\":10})]] NoteStore : public tachyon::Repository<NoteEntity, std::string> {\npublic:\n    std::vector<NoteEntity> findByText(const std::string& text);\n    long long countByStarsGreaterThan(int stars);\n    bool existsByText(const std::string& text);\n    [[Query({\"operation\":\"find\",\"where\":[{\"field\":\"stars\",\"operator\":\"gte\",\"argument\":0}],\"orderBy\":[{\"field\":\"stars\",\"direction\":\"desc\"}]})]] std::vector<NoteEntity> starred(int least, [[PageDefaults]] tachyon::json::JsonNode page);\n};\n",
            ),
            (
                "client/services/NotesService.hpp",
                "class [[Service]] NotesService {\npublic:\n    [[Needs]] NoteStore& notes;\n    std::string save(std::string id, std::string text, int stars) { return notes.save(NoteEntity{id, text, stars}).text; }\n    std::string one(std::string id) { auto found = notes.findById(id); return found ? found->text : std::string(\"none\"); }\n    int listed() { return static_cast<int>(notes.findAll().size()); }\n    int byText(std::string text) { return static_cast<int>(notes.findByText(text).size()); }\n    long long starredOver(int stars) { return notes.countByStarsGreaterThan(stars); }\n    std::string best(int least) { auto rows = notes.starred(least, tachyon::json::JsonObject()); return rows.empty() ? std::string() : rows.front().id; }\n    long long forget(std::string id) { return notes.deleteById(id); }\n    [[AllOrNothing({\"adapter\":\"indexeddb\"})]] int refuse(std::string id) { notes.save(NoteEntity{id, \"draft\", 0}); throw std::runtime_error(\"refused\"); }\n    bool drafted() { return notes.existsByText(\"draft\"); }\n};\n",
            ),
        ],
    );
    let services = [("notes", "NotesService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("notes", 1, &json!({})),
            PageProject::call("notes", 1, "save", &json!(["a", "first", 1])),
            PageProject::call("notes", 1, "save", &json!(["b", "second", 5])),
            PageProject::call("notes", 1, "save", &json!(["c", "second", 3])),
            PageProject::call("notes", 1, "one", &json!(["b"])),
            PageProject::call("notes", 1, "one", &json!(["z"])),
            PageProject::call("notes", 1, "listed", &json!([])),
            PageProject::call("notes", 1, "byText", &json!(["second"])),
            PageProject::call("notes", 1, "starredOver", &json!([2])),
            PageProject::call("notes", 1, "best", &json!([2])),
            PageProject::call("notes", 1, "forget", &json!(["a"])),
            PageProject::call("notes", 1, "refuse", &json!(["d"])),
            PageProject::call("notes", 1, "drafted", &json!([])),
            PageProject::call("notes", 1, "byText", &json!(["first"])),
        ],
    );
    let values: Vec<Value> = answers
        .iter()
        .map(|answer| answer["value"].clone())
        .collect();
    assert_eq!(
        values[1..11],
        [
            json!("first"),
            json!("second"),
            json!("second"),
            json!("second"),
            json!("none"),
            json!(2),
            json!(2),
            json!(2),
            json!("b"),
            json!(1)
        ],
        "{answers:?}"
    );
    assert!(answers[11].get("value").is_none(), "{answers:?}");
    assert_eq!(values[12], json!(false), "{answers:?}");
    assert_eq!(values[13], json!(0), "{answers:?}");
}

#[test]
fn settings_and_setting_bind_an_explicit_snapshot() {
    if !PageProject::has_compiler() {
        return;
    }
    let root = tempfile::tempdir().expect("project");
    PageProject::write(
        root.path(),
        &[
            (
                "client/models/ShopSettings.hpp",
                "struct [[Settings({\"prefix\":\"TAC_SHOP_\",\"names\":{\"pageSize\":\"SIZE\"}})]] ShopSettings {\n    [[Min(1)]] [[Max(50)]] int pageSize = 20;\n    std::string title = \"Shop\";\n    bool open = true;\n};\n",
            ),
            (
                "client/models/Endpoint.hpp",
                "struct Endpoint {\n    [[Setting(\"${tac.shop.host:localhost}\")]] std::string host;\n    [[Setting({\"path\":\"tac.shop.retries\",\"type\":\"integer\",\"default\":2,\"min\":0,\"max\":5})]] int retries = 0;\n    std::string untouched = \"kept\";\n};\n",
            ),
            (
                "client/services/ShopService.hpp",
                "class [[Service]] ShopService {\npublic:\n    int size(tachyon::json::JsonNode application) { return tachyon::Behaviors::properties<ShopSettings>(application).pageSize; }\n    std::string title(std::string size) { return tachyon::Behaviors::properties<ShopSettings>(std::map<std::string, std::string>{{\"TAC_SHOP_SIZE\", size}}).title; }\n    std::string endpoint(tachyon::json::JsonNode application) { Endpoint bound = tachyon::Behaviors::values<Endpoint>(application); return bound.host + \":\" + std::to_string(bound.retries) + \":\" + bound.untouched; }\n};\n",
            ),
        ],
    );
    let services = [("shop", "ShopService")];
    let answers = PageProject::answers(
        root.path(),
        &services,
        &[
            PageProject::init("shop", 1, &json!({})),
            PageProject::call("shop", 1, "size", &json!([{"tac": {"shop": {"size": 30}}}])),
            PageProject::call("shop", 1, "size", &json!([{"tac": {}}])),
            PageProject::call("shop", 1, "size", &json!([{"tac": {"shop": {"size": 99}}}])),
            PageProject::call("shop", 1, "title", &json!(["7"])),
            PageProject::call("shop", 1, "title", &json!(["7.5"])),
            PageProject::call(
                "shop",
                1,
                "endpoint",
                &json!([{"tac": {"shop": {"host": "example.org"}}}]),
            ),
            PageProject::call(
                "shop",
                1,
                "endpoint",
                &json!([{"tac": {"shop": {"retries": 9}}}]),
            ),
        ],
    );
    let values: Vec<Value> = answers
        .iter()
        .map(|answer| answer["value"].clone())
        .collect();
    assert_eq!(values[1..3], [json!(30), json!(20)], "{answers:?}");
    assert_eq!(
        answers[3]["code"], "configuration-properties",
        "{answers:?}"
    );
    assert_eq!(values[4], json!("Shop"), "{answers:?}");
    assert_eq!(
        answers[5]["code"], "configuration-properties",
        "{answers:?}"
    );
    assert_eq!(values[6], json!("example.org:2:kept"), "{answers:?}");
    assert_eq!(
        answers[7]["code"], "configuration-properties",
        "{answers:?}"
    );
}
