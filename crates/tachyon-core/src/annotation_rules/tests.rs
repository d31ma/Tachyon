use super::scan::Messages;
use super::{AnnotationRules, Arguments, Realm, Rule, Target};
use std::path::PathBuf;

struct Sample;

impl Sample {
    const FSHARP_HELPER_PATH: &'static str = "client/helpers/ValueHelper.fs";
    const REALMS: [Realm; 2] = [Realm::Tac, Realm::Yon];
    const TARGETS: [Target; 4] = [
        Target::Type,
        Target::Method,
        Target::Field,
        Target::Parameter,
    ];

    fn rule(name: &str) -> &'static Rule {
        AnnotationRules::rules()
            .find(|rule| rule.name == name)
            .expect("rule")
    }

    fn marker(language: &str, name: &str) -> String {
        let spelled = Messages::spelled(language, name);
        if Self::rule(name).arguments != Arguments::Required {
            return spelled;
        }
        match language {
            "rs" | "php" | "rb" | "cs" => format!("{}(\"x\")]", spelled.trim_end_matches(']')),
            "hpp" => format!("{}(\"x\")]]", spelled.trim_end_matches(']')),
            "fs" => format!("{}(\"x\")>]", spelled.trim_end_matches(">]")),
            _ => format!("{spelled}(\"x\")"),
        }
    }

    fn path(language: &str, realm: Realm) -> PathBuf {
        let folder = match realm {
            Realm::Tac => "client",
            Realm::Yon => "server",
        };
        PathBuf::from(format!("{folder}/services/SampleService.{language}"))
    }

    fn class_markers(language: &str, names: &[&str]) -> Vec<String> {
        let mut classes: Vec<&str> = Vec::new();
        for name in names {
            let rule = Self::rule(name);
            if let Some(first) = rule.class_needs.first()
                && !classes.contains(first)
                && !names.contains(first)
            {
                classes.push(first);
            }
        }
        classes
            .iter()
            .map(|name| Self::marker(language, name))
            .collect()
    }

    fn accepted(rule: &Rule, language: &str) -> String {
        let mut names = vec![rule.name];
        if let Some(partner) = rule.with.first() {
            names.push(partner);
        }
        let class = Self::class_markers(language, &names);
        let markers: Vec<String> = names
            .iter()
            .map(|name| Self::marker(language, name))
            .collect();
        if language == "py"
            && let Some(template) = Self::python_annotated(&markers, rule.targets[0])
        {
            return template.replace("{class}", &class.join("\n"));
        }
        Self::source(language, rule.targets[0], &class, &markers)
    }

    fn python_annotated(markers: &[String], target: Target) -> Option<String> {
        let inside: Vec<String> = markers
            .iter()
            .map(|marker| marker.trim_start_matches('@').to_owned())
            .collect();
        let inside = inside.join(", ");
        match target {
            Target::Parameter => Some(format!(
                "from typing import Annotated\n\n{{class}}\nclass SampleService:\n    async def load(self, value: Annotated[str, {inside}]):\n        return value\n"
            )),
            Target::Field => Some(format!(
                "from typing import Annotated\n\n{{class}}\nclass SampleService:\n    value: Annotated[str, {inside}] = ''\n"
            )),
            _ => None,
        }
    }

    fn refused(rule: &Rule, language: &str, target: Target) -> String {
        let class = Self::class_markers(language, &[rule.name]);
        let markers = vec![Self::marker(language, rule.name)];
        Self::source(language, target, &class, &markers)
    }

    #[allow(
        clippy::too_many_lines,
        clippy::match_same_arms,
        clippy::format_collect
    )]
    fn source(language: &str, target: Target, class: &[String], markers: &[String]) -> String {
        let mut every = class.to_vec();
        every.extend(markers.iter().cloned());
        let all = every.join("\n");
        let class = class.join("\n");
        let lines = |indent: &str| -> String {
            markers
                .iter()
                .map(|marker| format!("{indent}{marker}\n"))
                .collect()
        };
        let inline: String = markers.iter().map(|marker| format!("{marker} ")).collect();
        match (language, target) {
            ("fs", Target::Type) => format!(
                "{all}\ntype SampleService() =\n    member this.Load(value: string) : string = value\n"
            ),
            ("fs", Target::Method) => format!(
                "{class}\ntype SampleService() =\n{}    member this.Load(value: string) : string = value\n",
                lines("    ")
            ),
            ("fs", Target::Field) => format!(
                "{class}\ntype SampleService() =\n{}    member val Value : string = \"\" with get, set\n",
                lines("    ")
            ),
            ("fs", Target::Parameter) => format!(
                "{class}\ntype SampleService() =\n    member this.Load({inline}value: string) : string = value\n"
            ),
            ("js", Target::Type) => format!("{all}\nclass SampleService {{\n}}\n"),
            ("js", Target::Field) => format!(
                "{class}\nclass SampleService {{\n{}  value;\n}}\n",
                lines("  ")
            ),
            ("js", Target::Method) => format!(
                "{class}\nclass SampleService {{\n{}  async load(value) {{\n    return value;\n  }}\n}}\n",
                lines("  ")
            ),
            ("js", Target::Parameter) => format!(
                "{class}\nclass SampleService {{\n  async load({inline}value) {{\n    return value;\n  }}\n}}\n"
            ),
            ("ts", Target::Type) => format!("{all}\nexport class SampleService {{\n}}\n"),
            ("ts", Target::Field) => format!(
                "{class}\nexport class SampleService {{\n{}  value!: string;\n}}\n",
                lines("  ")
            ),
            ("ts", Target::Method) => format!(
                "{class}\nexport class SampleService {{\n{}  async load(value: string): Promise<string> {{\n    return value;\n  }}\n}}\n",
                lines("  ")
            ),
            ("ts", Target::Parameter) => format!(
                "{class}\nexport class SampleService {{\n  async load({inline}value: string): Promise<string> {{\n    return value;\n  }}\n}}\n"
            ),
            ("py", Target::Type) => format!("{all}\nclass SampleService:\n    pass\n"),
            ("py", Target::Field) => format!(
                "{class}\nclass SampleService:\n{}    value: str = ''\n",
                lines("    ")
            ),
            ("py", Target::Method) => format!(
                "{class}\nclass SampleService:\n{}    async def load(self, value):\n        return value\n",
                lines("    ")
            ),
            ("py", Target::Parameter) => format!(
                "{class}\nclass SampleService:\n    async def load(self, {inline}value: str):\n        return value\n"
            ),
            ("php", Target::Type) => format!("<?php\n{all}\nclass SampleService {{\n}}\n"),
            ("php", Target::Field) => format!(
                "<?php\n{class}\nclass SampleService {{\n{}    public string $value = '';\n}}\n",
                lines("    ")
            ),
            ("php", Target::Method) => format!(
                "<?php\n{class}\nclass SampleService {{\n{}    public function load(string $value): string {{\n        return $value;\n    }}\n}}\n",
                lines("    ")
            ),
            ("php", Target::Parameter) => format!(
                "<?php\n{class}\nclass SampleService {{\n    public function load({inline}string $value): string {{\n        return $value;\n    }}\n}}\n"
            ),
            ("java" | "groovy", Target::Type) => {
                format!("{all}\npublic class SampleService {{\n}}\n")
            }
            ("java" | "groovy", Target::Field) => format!(
                "{class}\npublic class SampleService {{\n{}    private String value;\n}}\n",
                lines("    ")
            ),
            ("java" | "groovy", Target::Method) => format!(
                "{class}\npublic class SampleService {{\n{}    public String load(String value) {{\n        return value;\n    }}\n}}\n",
                lines("    ")
            ),
            ("java" | "groovy", Target::Parameter) => format!(
                "{class}\npublic class SampleService {{\n    public String load({inline}String value) {{\n        return value;\n    }}\n}}\n"
            ),
            ("kt", Target::Type) => format!("{all}\nclass SampleService {{\n}}\n"),
            ("kt", Target::Field) => format!(
                "{class}\nclass SampleService {{\n{}    lateinit var value: String\n}}\n",
                lines("    ")
            ),
            ("kt", Target::Method) => format!(
                "{class}\nclass SampleService {{\n{}    fun load(value: String): String {{\n        return value\n    }}\n}}\n",
                lines("    ")
            ),
            ("kt", Target::Parameter) => format!(
                "{class}\nclass SampleService {{\n    fun load({inline}value: String): String {{\n        return value\n    }}\n}}\n"
            ),
            ("cs", Target::Type) => format!("{all}\npublic class SampleService {{\n}}\n"),
            ("cs", Target::Field) => format!(
                "{class}\npublic class SampleService {{\n{}    public string Value {{ get; set; }}\n}}\n",
                lines("    ")
            ),
            ("cs", Target::Method) => format!(
                "{class}\npublic class SampleService {{\n{}    public string Load(string value) {{\n        return value;\n    }}\n}}\n",
                lines("    ")
            ),
            ("cs", Target::Parameter) => format!(
                "{class}\npublic class SampleService {{\n    public string Load({inline}string value) {{\n        return value;\n    }}\n}}\n"
            ),
            ("rs", Target::Type) => format!("{all}\npub struct SampleService {{\n}}\n"),
            ("rs", Target::Field) => format!(
                "{class}\npub struct SampleService {{\n{}    value: String,\n}}\n",
                lines("    ")
            ),
            ("rs", Target::Method) => format!(
                "{class}\npub struct SampleService {{\n}}\n\nimpl SampleService {{\n{}    pub fn load(&self, value: String) -> String {{\n        value\n    }}\n}}\n",
                lines("    ")
            ),
            ("rs", Target::Parameter) => format!(
                "{class}\npub struct SampleService {{\n}}\n\nimpl SampleService {{\n    pub fn load(&self, {inline}value: String) -> String {{\n        value\n    }}\n}}\n"
            ),
            ("dart", Target::Type) => format!("{all}\nclass SampleService {{\n}}\n"),
            ("dart", Target::Field) => format!(
                "{class}\nclass SampleService {{\n{}  final String value = '';\n}}\n",
                lines("  ")
            ),
            ("dart", Target::Method) => format!(
                "{class}\nclass SampleService {{\n{}  Future<String> load(String value) async {{\n    return value;\n  }}\n}}\n",
                lines("  ")
            ),
            ("dart", Target::Parameter) => format!(
                "{class}\nclass SampleService {{\n  Future<String> load({inline}String value) async {{\n    return value;\n  }}\n}}\n"
            ),
            ("go", Target::Type) => {
                format!("package services\n\n{all}\ntype SampleService struct {{\n}}\n")
            }
            ("go", Target::Field) => format!(
                "package services\n\n{class}\ntype SampleService struct {{\n{}\tvalue string\n}}\n",
                lines("\t")
            ),
            ("go", Target::Method) => format!(
                "package services\n\n{class}\ntype SampleService struct {{\n}}\n\n{}func (service *SampleService) Load(value string) string {{\n\treturn value\n}}\n",
                lines("")
            ),
            ("go", Target::Parameter) => format!(
                "package services\n\n{class}\ntype SampleService struct {{\n}}\n\nfunc (service *SampleService) Load(\n{}\tvalue string,\n) string {{\n\treturn value\n}}\n",
                lines("\t")
            ),
            ("swift", Target::Type) => format!("{all}\nfinal class SampleService {{\n}}\n"),
            ("swift", Target::Field) => format!(
                "{class}\nfinal class SampleService {{\n{}    var value: String = \"\"\n}}\n",
                lines("    ")
            ),
            ("swift", Target::Method) => format!(
                "{class}\nfinal class SampleService {{\n{}    func load(value: String) -> String {{\n        return value\n    }}\n}}\n",
                lines("    ")
            ),
            ("swift", Target::Parameter) => format!(
                "{class}\nfinal class SampleService {{\n    func load({inline}value: String) -> String {{\n        return value\n    }}\n}}\n"
            ),
            ("rb", Target::Type) => format!("{all}\nclass SampleService\nend\n"),
            ("rb", Target::Field) => format!(
                "{class}\nclass SampleService\n{}  attr_reader :value\nend\n",
                lines("  ")
            ),
            ("rb", Target::Method) => format!(
                "{class}\nclass SampleService\n{}  def load(value)\n    value\n  end\nend\n",
                lines("  ")
            ),
            ("rb", Target::Parameter) => format!(
                "{class}\nclass SampleService\n  def load(\n{}    value\n  )\n    value\n  end\nend\n",
                lines("    ")
            ),
            ("hpp", Target::Type) => format!(
                "class {} {inline}SampleService {{\npublic:\n    int value = 0;\n}};\n",
                class.replace('\n', " ")
            ),
            ("hpp", Target::Field) => format!(
                "class {} SampleService {{\npublic:\n{}    int value = 0;\n}};\n",
                class.replace('\n', " "),
                lines("    ")
            ),
            ("hpp", Target::Method) => format!(
                "class {} SampleService {{\npublic:\n{}    int load(int value) {{ return value; }}\n}};\n",
                class.replace('\n', " "),
                lines("    ")
            ),
            ("hpp", Target::Parameter) => format!(
                "class {} SampleService {{\npublic:\n    int load({inline}int value) {{ return value; }}\n}};\n",
                class.replace('\n', " ")
            ),
            ("lua", Target::Type) => format!("{all}\nSampleService = {{}}\n"),
            ("lua", Target::Field) => format!(
                "{class}\nSampleService = {{\n{}  value = nil,\n}}\n",
                lines("  ")
            ),
            ("lua", Target::Method) => format!(
                "{class}\nSampleService = {{}}\n\n{}function SampleService:load(value)\n  return value\nend\n",
                lines("")
            ),
            ("lua", Target::Parameter) => format!(
                "{class}\nSampleService = {{}}\n\nfunction SampleService:load(\n{}  value\n)\n  return value\nend\n",
                lines("  ")
            ),
            _ => panic!(
                "add sample classes for the language '{language}' to annotation_rules/tests.rs"
            ),
        }
    }

    fn fsharp_message(path: &str, source: &str) -> Option<String> {
        AnnotationRules::check(std::path::Path::new(path), source)
            .err()
            .map(|failure| failure.diagnostics()[0].message.clone())
    }

    fn message(language: &str, realm: Realm, source: &str) -> Option<String> {
        let path = Self::path(language, realm);
        AnnotationRules::check(&path, source)
            .err()
            .map(|failure| failure.diagnostics()[0].message.clone())
    }
}

#[test]
fn every_registry_marker_has_exactly_one_rule() {
    let registry: Vec<&str> = crate::behavior::framework_marker_names().collect();
    for name in &registry {
        let count = AnnotationRules::rules()
            .filter(|rule| rule.name == *name)
            .count();
        assert_eq!(count, 1, "the marker {name} needs exactly one rule");
    }
    for rule in AnnotationRules::rules() {
        assert!(
            registry.contains(&rule.name),
            "the rule {} names no registry marker",
            rule.name
        );
        for realm in Sample::REALMS {
            for language in rule.languages(realm) {
                assert!(
                    AnnotationRules::LANGUAGES.contains(language),
                    "{} names the unknown language {language}",
                    rule.name
                );
            }
        }
        for name in rule.with.iter().chain(rule.clashes).chain(rule.class_needs) {
            assert!(registry.contains(name), "{} names {name}", rule.name);
        }
    }
}

#[test]
fn every_language_has_rules_in_some_realm() {
    for language in AnnotationRules::LANGUAGES {
        let used = AnnotationRules::rules().any(|rule| {
            Sample::REALMS
                .iter()
                .any(|realm| rule.allows(language, *realm))
        });
        assert!(used, "no annotation reaches {language}");
    }
}

#[test]
fn fsharp_helper_attribute_is_checked_through_its_existing_analysis_view() {
    let path = std::path::Path::new(Sample::FSHARP_HELPER_PATH);
    let source = "[<Helper>]\ntype ValueHelper() =\n    member this.Value() : int = 1\n";
    let result = AnnotationRules::check(path, source);
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn fsharp_helper_on_a_member_reports_its_authored_spelling_and_path() {
    let path = std::path::Path::new(Sample::FSHARP_HELPER_PATH);
    let source = "type ValueHelper() =\n    [<Helper>]\n    member this.Value() : int = 1\n";
    let failure =
        AnnotationRules::check(path, source).expect_err("FSharp member Helper must be refused");
    let message = &failure.diagnostics()[0].message;
    assert!(message.contains("[<Helper>]"), "{message}");
    assert!(message.contains("belongs on a class"), "{message}");
    assert!(message.contains(Sample::FSHARP_HELPER_PATH), "{message}");
    assert_eq!(
        failure.diagnostics()[0].spans[0].file,
        Sample::FSHARP_HELPER_PATH
    );
}

#[test]
fn fsharp_yon_annotations_are_checked_where_they_are_written() {
    let controller = "server/controllers/OrdersController.fs";
    let route = "[<RequestRoute(\"/orders\"); Public>]\ntype OrdersController() =\n    [<GetRoute>]\n    member this.GetOrder([<FromPath>] id: int) : int = id\n";
    assert_eq!(Sample::fsharp_message(controller, route), None);
    let service = "server/services/PriceService.fs";
    let cached = "[<Service>]\n[<NeedsAll>]\n[<CacheSettings(\"x\")>]\ntype PriceService(store: PriceStore) =\n    [<Cached>]\n    member this.Price(sku: string) : int = 1\n";
    assert_eq!(Sample::fsharp_message(service, cached), None);
    let background = "[<Service>]\ntype PriceService() =\n    [<OnEvent>]\n    [<InBackground>]\n    member this.Record(order: PlacedOrder) : unit = ()\n";
    assert_eq!(Sample::fsharp_message(service, background), None);
    let refused = [
        (
            controller,
            "[<RequestRoute(\"/orders\"); Public>]\n[<GetRoute>]\ntype OrdersController() =\n    member this.GetOrder(id: int) : int = id\n",
            "[<GetRoute>] is on a class, but it belongs on a method",
        ),
        (
            service,
            "[<Service>]\ntype PriceService() =\n    [<NeedsAll>]\n    member this.Price(sku: string) : int = 1\n",
            "[<NeedsAll>] is on a method, but it belongs on a class",
        ),
    ];
    for (path, source, expected) in refused {
        let message =
            Sample::fsharp_message(path, source).unwrap_or_else(|| panic!("accepted:\n{source}"));
        assert!(message.contains(expected), "{message}");
        assert!(message.contains(path), "{message}");
    }
}

#[test]
fn the_shared_language_check_reaches_fsharp_yon_code() {
    let refusal = AnnotationRules::checked_refusal("SocketRequest", "fs", Realm::Yon)
        .expect("a browser socket client is not Yon code");
    assert!(
        refusal.starts_with("[<SocketRequest>] is not used in Yon code"),
        "{refusal}"
    );
    for name in [
        "Service",
        "RequestRoute",
        "GetRoute",
        "Cached",
        "Topic",
        "HTTPClient",
    ] {
        let refusal = AnnotationRules::checked_refusal(name, "fs", Realm::Yon);
        assert_eq!(refusal, None, "{name}");
    }
}

#[test]
fn every_rule_accepts_and_refuses_in_every_language_that_can_spell_it() {
    let mut cases = 0;
    for rule in AnnotationRules::rules().filter(|rule| !rule.is_retired()) {
        for realm in Sample::REALMS {
            for language in rule.languages(realm) {
                let accepted = Sample::accepted(rule, language);
                let refusal = Sample::message(language, realm, &accepted);
                assert!(
                    refusal.is_none(),
                    "{} in {language} {realm:?} was refused: {refusal:?}\n{accepted}",
                    rule.name
                );
                let wrong = Sample::TARGETS
                    .into_iter()
                    .find(|target| !rule.targets.contains(target))
                    .expect("a wrong target");
                let refused = Sample::refused(rule, language, wrong);
                let message = Sample::message(language, realm, &refused).unwrap_or_else(|| {
                    panic!(
                        "{} on a {} in {language} was accepted\n{refused}",
                        rule.name,
                        wrong.word()
                    )
                });
                let spelled = Messages::spelled(language, rule.name);
                assert!(
                    message.contains(&spelled) && message.contains("belongs on"),
                    "{message}"
                );
                assert!(message.contains(rule.meaning), "{message}");
                assert!(message.contains("services/SampleService"), "{message}");
                cases += 2;
            }
        }
    }
    assert!(cases > 1000, "{cases}");
}

#[test]
fn a_language_outside_the_reach_is_refused_with_the_places_that_work() {
    for rule in AnnotationRules::rules().filter(|rule| !rule.is_retired()) {
        for realm in Sample::REALMS {
            for language in realm.languages() {
                if rule.allows(language, realm) {
                    continue;
                }
                let source = Sample::accepted(rule, language);
                let message = Sample::message(language, realm, &source)
                    .unwrap_or_else(|| panic!("{} reached {language} {realm:?}", rule.name));
                assert!(
                    message.contains("is not available") || message.contains("is not used"),
                    "{message}"
                );
            }
        }
    }
}

#[test]
fn retired_names_are_refused_with_their_replacement() {
    for rule in AnnotationRules::rules().filter(|rule| rule.is_retired()) {
        let source = format!("@{}\nclass SampleService {{\n}}\n", rule.name);
        let message = Sample::message("ts", Realm::Yon, &source).expect("retired");
        assert!(message.contains(rule.advice), "{message}");
    }
}

#[test]
fn arguments_statics_pairs_and_clashes_are_checked() {
    let cases = [
        (
            "ts",
            "@Service(\"x\")\nexport class SampleService {\n}\n",
            "Service accepts no arguments",
        ),
        (
            "py",
            "@lifetime\nclass SampleService:\n    pass\n",
            "@lifetime needs its arguments",
        ),
        (
            "java",
            "@CacheSettings(\"x\")\npublic class SampleService {\n    @Cached\n    public static String load(String value) {\n        return value;\n    }\n}\n",
            "on a static method",
        ),
        (
            "py",
            "class SampleService:\n    @on_start\n    @staticmethod\n    def load():\n        pass\n",
            "on a static method",
        ),
        (
            "rs",
            "#[CacheSettings(\"x\")]\npub struct SampleService {\n}\n\nimpl SampleService {\n    #[Cached]\n    pub fn load(value: String) -> String {\n        value\n    }\n}\n",
            "on a static method",
        ),
        (
            "kt",
            "class SampleService {\n    @Order(1)\n    fun load(value: String): String {\n        return value\n    }\n}\n",
            "@Order needs @OnEvent",
        ),
        (
            "cs",
            "public class SampleService {\n    [OnStart]\n    [OnStop]\n    public void Load() {\n    }\n}\n",
            "cannot be on the same method",
        ),
        (
            "dart",
            "class SampleService {\n  @OnStart\n  @OnStart\n  void load() {\n  }\n}\n",
            "written twice",
        ),
        (
            "php",
            "<?php\nclass SampleService {\n    #[Fake]\n    public string $value = '';\n}\n",
            "belongs in a class marked #[Test] or #[TestSetup]",
        ),
        (
            "go",
            "package services\n\ntype SampleService struct {\n\t// @Named(\"primary\")\n\tValue string\n}\n",
            "// @Named(\"primary\")",
        ),
        (
            "swift",
            "final class SampleService {\n    @Needs\n    @Named\n    var value: String = \"\"\n}\n",
            "@Named needs its arguments",
        ),
    ];
    for (language, source, expected) in cases {
        let message = Sample::message(language, Realm::Tac, source)
            .unwrap_or_else(|| panic!("accepted:\n{source}"));
        let wanted = if expected.starts_with("// @Named") {
            "// @Named needs // @Needs"
        } else {
            expected
        };
        assert!(message.contains(wanted), "{message}");
    }
}

#[test]
fn routes_check_their_verbs_status_and_parameter_sources() {
    let cases = [
        (
            "ts",
            "@RequestRoute(\"/orders\")\nexport class OrdersController {\n  @GetRoute()\n  @PostRoute()\n  getOrders() { return []; }\n}\n",
            "cannot be on the same method",
        ),
        (
            "java",
            "@RequestRoute(\"/orders\")\npublic class OrdersController {\n    @PostRoute\n    public Object postOrder(@FromBody Object first, @FromBody Object second) { return first; }\n}\n",
            "on two parameters of one method",
        ),
        (
            "kt",
            "@RequestRoute(\"/orders\")\nclass OrdersController {\n    @GetRoute\n    fun getOrder(@FromPath @FromQuery id: String): String = id\n}\n",
            "cannot be on the same parameter",
        ),
        (
            "cs",
            "public class OrdersService {\n    [ResponseStatus(201)]\n    public string Save() { return \"\"; }\n}\n",
            "needs [GetRoute] or [PostRoute]",
        ),
        (
            "rb",
            "#[RequestRoute(\"/orders\")]\nclass OrdersController\n  #[GetRoute]\n  #[Stream]\n  def get_orders\n  end\nend\n",
            "#[Stream] is retired: write #[StreamRoute] instead",
        ),
        (
            "ts",
            "@RequestRoute(\"/ticks\")\nexport class TicksController {\n  @StreamRoute()\n  @GetRoute()\n  streamTicks() { return []; }\n}\n",
            "cannot be on the same method",
        ),
        (
            "php",
            "<?php\nclass OrdersService {\n    #[GetRoute]\n    public function getOrders() { return []; }\n}\n",
            "belongs in a class marked #[RequestRoute]",
        ),
    ];
    for (language, source, expected) in cases {
        let message = Sample::message(language, Realm::Yon, source)
            .unwrap_or_else(|| panic!("accepted:\n{source}"));
        assert!(message.contains(expected), "{message}");
    }
    let stream_cases = [
        (
            "@HTTPClient(\"/ticks\")\nexport class TicksClient {\n  @StreamRequest()\n  @GetRequest()\n  streamTicks(tick: Tick) {}\n}\n",
            "cannot be on the same method",
        ),
        (
            "export class TicksService {\n  @StreamRequest(\"/ticks\")\n  streamTicks(tick: Tick) {}\n}\n",
            "@StreamRequest belongs in a class marked @HTTPClient",
        ),
    ];
    for (source, expected) in stream_cases {
        let message = Sample::message("ts", Realm::Tac, source)
            .unwrap_or_else(|| panic!("accepted:\n{source}"));
        assert!(message.contains(expected), "{message}");
    }
    let rust_stream = "#[HTTPClient(\"/ticks\")]\npub struct TicksClient {\n}\n\nimpl TicksClient {\n    #[StreamRequest]\n    pub fn stream_ticks(&self, tick: Tick) {}\n}\n";
    assert_eq!(Sample::message("rs", Realm::Tac, rust_stream), None);
    let kotlin_stream = "@HTTPClient(\"/ticks\")\ninterface TicksClient {\n    @StreamRequest\n    fun streamTicks(tick: Tick)\n}\n";
    let message = Sample::message("kt", Realm::Tac, kotlin_stream).expect("refused in Kotlin");
    assert!(message.contains("receives it with @OnEvent"), "{message}");
    let client_route = "@RequestRoute(\"/orders\")\nexport class OrdersController {\n}\n";
    let message = Sample::message("ts", Realm::Tac, client_route).expect("refused in Tac");
    assert!(message.contains("is not used in Tac code"), "{message}");
}

#[test]
fn sockets_check_their_class_their_markers_and_their_language() {
    let cases = [
        (
            "ts",
            "@SocketRoute(\"/chat\")\n@RequestRoute(\"/chat\")\nexport class ChatController {\n}\n",
            "cannot be on the same class",
        ),
        (
            "ts",
            "export class ChatService {\n  @OnMessage\n  onMessage(session, message) {}\n}\n",
            "@OnMessage belongs in a class marked @SocketRoute",
        ),
        (
            "ts",
            "@SocketRoute(\"/chat\")\nexport class ChatController {\n  @OnOpen\n  @OnMessage\n  onMessage(session, message) {}\n}\n",
            "cannot be on the same method",
        ),
        (
            "ts",
            "@SocketRoute(\"/chat\")\nexport class ChatController {\n  @GetRoute\n  getChat() { return 1; }\n}\n",
            "belongs in a class marked @RequestRoute",
        ),
        (
            "js",
            "@SocketRoute(\"/chat\")\nexport class ChatController {\n  @OnMessage(\"x\")\n  onMessage(session, message) {}\n}\n",
            "accepts no arguments",
        ),
    ];
    for (language, source, expected) in cases {
        let message = Sample::message(language, Realm::Yon, source)
            .unwrap_or_else(|| panic!("accepted:\n{source}"));
        assert!(message.contains(expected), "{message}");
    }
    let accepted = "@SocketRoute(\"/chat\")\nexport class ChatController {\n  @OnOpen\n  onOpen(session) {}\n  @OnMessage\n  onMessage(session, message) {}\n  @OnClose\n  onClose(session) {}\n  @OnError\n  onError(session, error) {}\n}\n";
    assert_eq!(Sample::message("ts", Realm::Yon, accepted), None);
    let python = "@socket_route(\"/chat\")\nclass ChatController:\n    @on_message\n    def on_message(self, session, message: ChatMessage):\n        pass\n";
    assert_eq!(Sample::message("py", Realm::Yon, python), None);
    let rust = "#[SocketRoute(\"/chat\")]\npub struct ChatController {}\n";
    assert_eq!(Sample::message("rs", Realm::Yon, rust), None);
    let browser = "@SocketRoute(\"/chat\")\nexport class ChatController {\n}\n";
    let message = Sample::message("ts", Realm::Tac, browser).expect("refused in Tac");
    assert!(message.contains("@SocketRequest"), "{message}");
    let client = "@HTTPClient(\"/chat\")\nexport class ChatClient {\n  @SocketRequest()\n  @GetRequest()\n  receive(message: ChatMessage) {}\n}\n";
    let message = Sample::message("ts", Realm::Tac, client).expect("refused");
    assert!(
        message.contains("cannot be on the same method"),
        "{message}"
    );
    let rust_client = "#[HTTPClient(\"/chat\")]\npub struct ChatClient {\n}\n\nimpl ChatClient {\n    #[SocketRequest]\n    pub fn receive(&self, message: ChatMessage) {}\n}\n";
    assert_eq!(Sample::message("rs", Realm::Tac, rust_client), None);
    let kotlin_client = "@HTTPClient(\"/chat\")\ninterface ChatClient {\n    @SocketRequest\n    fun receive(message: ChatMessage)\n}\n";
    let message = Sample::message("kt", Realm::Tac, kotlin_client).expect("refused in Kotlin");
    assert!(message.contains("receives it with @OnEvent"), "{message}");
}

#[test]
fn the_layout_gate_runs_the_rules_for_every_file() {
    let source =
        "@Service\nclass SampleService {\n    @Cached\n    lateinit var value: String\n}\n";
    let failure = crate::annotation_layout::validate(
        std::path::Path::new("client/services/SampleService.kt"),
        source,
    )
    .expect_err("Cached on a field");
    let diagnostic = &failure.diagnostics()[0];
    assert_eq!(diagnostic.code.to_string(), "TY1308");
    assert!(
        diagnostic
            .message
            .contains("@Cached is on a field, but it belongs on a method"),
        "{failure}"
    );
    assert!(
        diagnostic
            .message
            .contains("client/services/SampleService.kt:3"),
        "{failure}"
    );
    assert_eq!(diagnostic.spans[0].start, 35);
}

#[test]
fn deeper_paths_share_the_table_messages() {
    assert!(AnnotationRules::no_arguments("Service").starts_with("Service accepts no arguments"));
    assert!(AnnotationRules::belongs("Cached").starts_with("Cached belongs on a method"));
    let refusal = AnnotationRules::unavailable("Helper", "kt", "tac").expect("browser role");
    assert!(
        refusal.contains("qualified same-language Helper constructor"),
        "{refusal}"
    );
    assert_eq!(AnnotationRules::unavailable("Helper", "dart", "tac"), None);
    assert_eq!(
        AnnotationRules::unavailable("StreamRoute", "fs", "yon"),
        None
    );
    assert!(AnnotationRules::unavailable("StreamRoute", "groovy", "yon").is_some());
    assert_eq!(Messages::snake_case("HTTPClient"), "http_client");
    assert_eq!(Messages::snake_case("OnEvent"), "on_event");
}

#[test]
fn python_field_markers_live_in_annotated_types() {
    let annotated = "from typing import Annotated\n\nclass OrderInput:\n    title: Annotated[str, required, not_empty, length(1, 80)] = ''\n    count: Annotated[int, min(1), max(100)] = 1\n";
    assert_eq!(Sample::message("py", Realm::Yon, annotated), None);
    let decorated = "class OrderInput:\n    @not_empty\n    title: str = ''\n";
    let message = Sample::message("py", Realm::Yon, decorated).expect("decorator refused");
    assert!(
        message.contains("Python field markers go inside the field's type")
            && message.contains("`name: Annotated[Type, not_empty]`"),
        "{message}"
    );
    let decorated =
        "class OrdersService:\n    def find(self, @cache_key key: str):\n        return key\n";
    let message = Sample::message("py", Realm::Yon, decorated).expect("inline refused");
    assert!(
        message.contains("`name: Annotated[Type, cache_key]`"),
        "{message}"
    );
    let misplaced = "from typing import Annotated\n\nclass OrdersService:\n    def find(self, key: Annotated[int, min(1)]):\n        return key\n";
    let message =
        Sample::message("py", Realm::Yon, misplaced).expect("field marker on a parameter");
    assert!(message.contains("belongs on a field"), "{message}");
    let twice = "from typing import Annotated\n\nclass OrderInput:\n    count: Annotated[int, min(1), min(2)] = 1\n";
    assert!(Sample::message("py", Realm::Yon, twice).is_some());
    let needs = "from typing import Annotated\n\nclass OrdersService:\n    store: Annotated[OrderStore, needs]\n";
    assert_eq!(Sample::message("py", Realm::Yon, needs), None);
}

#[test]
fn python_parameter_markers_live_in_annotated_types() {
    let annotated = "from typing import Annotated\n\n@request_route(\"/orders\")\nclass OrdersController:\n    @post_route\n    def post_order(self, id: Annotated[int, from_path], order: Annotated[dict, from_body]):\n        return order\n";
    assert_eq!(Sample::message("py", Realm::Yon, annotated), None);
    let inline = "@request_route(\"/orders\")\nclass OrdersController:\n    @post_route\n    def post_order(self, @from_body order: dict):\n        return order\n";
    let message = Sample::message("py", Realm::Yon, inline).expect("inline refused");
    assert!(
        message.contains("`name: Annotated[Type, from_body]`"),
        "{message}"
    );
    let twice = "from typing import Annotated\n\n@request_route(\"/orders\")\nclass OrdersController:\n    @post_route\n    def post_order(self, first: Annotated[dict, from_body], second: Annotated[dict, from_body]):\n        return first\n";
    let message = Sample::message("py", Realm::Yon, twice).expect("two bodies refused");
    assert!(
        message.contains("on two parameters of one method"),
        "{message}"
    );
    let client = "from typing import Annotated\n\nclass OrdersService:\n    async def find(self, id: Annotated[str, path_variable]):\n        pass\n";
    let message = Sample::message("py", Realm::Yon, client).expect("outside a client refused");
    assert!(
        message.contains("belongs in a class marked @http_client"),
        "{message}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn needs_all_takes_no_field_list_and_no_constructor_in_every_language() {
    let constructors = [
        (
            "js",
            "@NeedsAll\nclass SampleService {\n  value;\n  constructor() {}\n}\n",
            "remove the constructor",
        ),
        (
            "ts",
            "@NeedsAll()\nexport class SampleService {\n  value!: string;\n  constructor(value: string) {}\n}\n",
            "remove the constructor",
        ),
        (
            "py",
            "@needs_all\nclass SampleService:\n    value: str\n\n    def __init__(self):\n        pass\n",
            "remove __init__",
        ),
        (
            "php",
            "<?php\n#[NeedsAll]\nclass SampleService {\n    public string $value;\n    public function __construct() {}\n}\n",
            "remove __construct",
        ),
        (
            "java",
            "@NeedsAll\npublic class SampleService {\n    private String value;\n    public SampleService() {}\n}\n",
            "remove the constructor",
        ),
        (
            "kt",
            "@NeedsAll\nclass SampleService(val value: String) {\n}\n",
            "remove the constructor",
        ),
        (
            "kt",
            "@NeedsAll\nclass SampleService {\n    constructor() {}\n}\n",
            "remove the constructor",
        ),
        (
            "cs",
            "[NeedsAll]\npublic class SampleService {\n    public string Value;\n    public SampleService() {}\n}\n",
            "remove the constructor",
        ),
        (
            "rs",
            "#[NeedsAll]\npub struct SampleService {\n    value: String,\n}\n\nimpl SampleService {\n    pub fn new() -> Self {\n        Self { value: String::new() }\n    }\n}\n",
            "remove fn new",
        ),
        (
            "dart",
            "@NeedsAll()\nclass SampleService {\n  final String value;\n  SampleService(this.value);\n}\n",
            "remove the constructor",
        ),
        (
            "swift",
            "@NeedsAll\nfinal class SampleService {\n    var value: String\n    init() { value = \"\" }\n}\n",
            "remove init",
        ),
        (
            "rb",
            "#[NeedsAll]\nclass SampleService\n  attr_reader :value\n\n  def initialize\n    @value = 1\n  end\nend\n",
            "remove initialize",
        ),
    ];
    for (language, source, expected) in constructors {
        let realm = if Realm::Tac.languages().contains(&language) {
            Realm::Tac
        } else {
            Realm::Yon
        };
        let message = Sample::message(language, realm, source)
            .unwrap_or_else(|| panic!("{language} accepted a constructor:\n{source}"));
        assert!(
            message.contains("builds this class for you") && message.contains(expected),
            "{language}: {message}"
        );
    }
    let listed = [
        (
            "js",
            "@NeedsAll('value')\nclass SampleService {\n  value;\n}\n",
        ),
        (
            "py",
            "@needs_all('value')\nclass SampleService:\n    value: str\n",
        ),
        (
            "go",
            "package services\n\n// @NeedsAll(\"Value\")\ntype SampleService struct {\n\tValue string\n}\n",
        ),
        (
            "rb",
            "#[NeedsAll(\"value\")]\nclass SampleService\n  attr_reader :value\nend\n",
        ),
        (
            "cs",
            "[NeedsAll(\"Value\")]\npublic class SampleService {\n    public string Value;\n}\n",
        ),
    ];
    for (language, source) in listed {
        let message = Sample::message(language, Realm::Tac, source)
            .unwrap_or_else(|| panic!("{language} accepted a field list:\n{source}"));
        assert!(
            message.contains("NeedsAll takes no field list; every field can be filled"),
            "{language}: {message}"
        );
    }
    let fine = [
        (
            "js",
            "@NeedsAll()\nclass SampleService {\n  value;\n}\nclass Helper {\n  constructor() {}\n}\n",
        ),
        (
            "rs",
            "#[NeedsAll()]\npub struct SampleService {\n    value: String,\n}\n\nimpl Other {\n    pub fn new() -> Self {\n        Self {}\n    }\n}\n",
        ),
        (
            "rb",
            "#[NeedsAll]\nclass SampleService\n  attr_reader :value\n\n  #[OnStart]\n  def start\n    @count = 0\n  end\nend\n",
        ),
    ];
    for (language, source) in fine {
        assert_eq!(
            Sample::message(language, Realm::Tac, source),
            None,
            "{language}"
        );
    }
}

struct NullableCases;

impl NullableCases {
    const REFUSED: [(&'static str, Realm, &'static str, &'static str); 17] = [
        (
            "ts",
            Realm::Tac,
            "@Service\n@NeedsAll\nexport default class DrawerService {\n  open!: boolean\n  label?: string\n  count = 0\n}\n",
            "`label!: string` instead of `label?: string`",
        ),
        (
            "ts",
            Realm::Tac,
            "@Service\n@NeedsAll\nexport default class DrawerService {\n  private title: string | null\n}\n",
            "`private title!: string`",
        ),
        (
            "ts",
            Realm::Yon,
            "@Service\nexport class OrderService {\n  @Needs\n  store: OrderStore | undefined = undefined\n}\n",
            "`store!: OrderStore` instead of `store: OrderStore | undefined = undefined`",
        ),
        (
            "dart",
            Realm::Tac,
            "@Service\n@NeedsAll\nclass PagerService {\n  final Map<String, Object?>? documents;\n}\n",
            "`late final Map<String, Object?> documents` instead of `final Map<String, Object?>? documents`",
        ),
        (
            "dart",
            Realm::Yon,
            "@Service\n@NeedsAll\nclass OrderService {\n  OrderStore? store;\n}\n",
            "`late final OrderStore store`",
        ),
        (
            "kt",
            Realm::Tac,
            "@Service\n@NeedsAll\nclass TallyService {\n    private val path: String?\n}\n",
            "`private val path: String`",
        ),
        (
            "kt",
            Realm::Yon,
            "@Service\nclass TallyService {\n    @Needs\n    var store: TallyStore? = null\n}\n",
            "`lateinit var store: TallyStore`",
        ),
        (
            "swift",
            Realm::Tac,
            "@Service\n@NeedsAll\nclass BadgeService {\n    var label: String?\n}\n",
            "`let label: String`",
        ),
        (
            "swift",
            Realm::Tac,
            "@Service\nclass BadgeService {\n    @Needs\n    var store: BadgeStore?\n}\n",
            "`var store: BadgeStore!`",
        ),
        (
            "cs",
            Realm::Tac,
            "[Service]\n[NeedsAll]\npublic class LinksService\n{\n    public required string? Title { get; set; }\n}\n",
            "`public required string Title { get; set; }`",
        ),
        (
            "cs",
            Realm::Yon,
            "[Service]\n[NeedsAll]\npublic class OrderService\n{\n    private readonly OrderStore? store;\n}\n",
            "`private readonly OrderStore store`",
        ),
        (
            "php",
            Realm::Yon,
            "<?php\n#[Service]\n#[NeedsAll]\nclass OrderService {\n    private ?OrderStore $store;\n}\n",
            "`private OrderStore $store`",
        ),
        (
            "php",
            Realm::Yon,
            "<?php\n#[Service]\nclass OrderService {\n    #[Needs]\n    private OrderStore|null $store;\n}\n",
            "`private OrderStore $store`",
        ),
        (
            "py",
            Realm::Yon,
            "@service\n@needs_all\nclass OrderService:\n    store: OrderStore | None\n\n    def find(self):\n        found: int | None\n        return found\n",
            "`store: OrderStore`",
        ),
        (
            "py",
            Realm::Yon,
            "from typing import Annotated, Optional\n\nclass OrderService:\n    store: Annotated[Optional[OrderStore], needs]\n",
            "`store: Annotated[OrderStore, needs]`",
        ),
        (
            "rs",
            Realm::Tac,
            "#[Service]\n#[NeedsAll]\npub struct TocService {\n    page_path: String,\n    entries: Option<HashMap<String, TacValue>>,\n}\n",
            "`entries: HashMap<String, TacValue>`",
        ),
        (
            "rs",
            Realm::Yon,
            "#[Service]\npub struct OrderService {\n    #[Needs]\n    store: Option<Arc<OrderStore>>,\n}\n",
            "`store: Arc<OrderStore>`",
        ),
    ];
    const ACCEPTED: [(&'static str, Realm, &'static str); 10] = [
        (
            "ts",
            Realm::Tac,
            "@Service\n@NeedsAll\nexport default class DrawerService {\n  open!: boolean\n  note: string | null = null\n  static shared?: string\n  declare hint?: string\n  toggle(): void {\n    const label: string | null = null\n  }\n}\n",
        ),
        (
            "ts",
            Realm::Tac,
            "@Service\nexport default class ClipboardService {\n  @Needs\n  lifecycle!: AbortSignal\n  timer?: number\n}\n",
        ),
        (
            "dart",
            Realm::Tac,
            "@Service\n@NeedsAll\nclass PagerService {\n  late final String path;\n  final List<Object?> pages;\n  String? note;\n  int? count() => null;\n}\n",
        ),
        (
            "kt",
            Realm::Tac,
            "@Service\n@NeedsAll\nclass TallyService {\n    val path: String\n    lateinit var label: String\n    var note: String? = null\n    fun find(): String? = null\n}\n",
        ),
        (
            "swift",
            Realm::Tac,
            "@Service\n@NeedsAll\nclass BadgeService {\n    let path: String\n    var note: String? = nil\n    @Needs\n    var store: BadgeStore!\n    var doubled: Int? { nil }\n}\n",
        ),
        (
            "cs",
            Realm::Tac,
            "[Service]\n[NeedsAll]\npublic class LinksService\n{\n    public required object Documents;\n    public object? Docs;\n    public string? Title() => null;\n}\n",
        ),
        (
            "cs",
            Realm::Yon,
            "[Service]\n[NeedsAll]\npublic class OrderService\n{\n    private readonly OrderStore _store;\n    public string? Note { get; set; }\n}\n",
        ),
        (
            "rs",
            Realm::Tac,
            "#[Service]\n#[NeedsAll]\n#[derive(Default)]\npub struct TocService {\n    entries: Option<TacValue>,\n}\n",
        ),
        (
            "py",
            Realm::Yon,
            "@service\n@needs_all\nclass OrderService:\n    store: OrderStore\n    note: str | None = None\n",
        ),
        (
            "java",
            Realm::Yon,
            "@Service\n@NeedsAll\npublic class OrderService {\n    private final OrderStore store;\n}\n",
        ),
    ];
}

#[test]
fn a_field_that_tachyon_fills_is_never_declared_nullable() {
    for (language, realm, source, expected) in NullableCases::REFUSED {
        let path = Sample::path(language, realm);
        let failure = AnnotationRules::check(&path, source).expect_err(&format!(
            "{language} accepted a nullable filled field:\n{source}"
        ));
        let diagnostic = &failure.diagnostics()[0];
        assert_eq!(
            diagnostic.code.number(),
            1331,
            "{language}: {}",
            diagnostic.message
        );
        assert!(
            diagnostic.message.contains(expected) && diagnostic.message.contains("never null"),
            "{language}: {}",
            diagnostic.message
        );
    }
}

#[test]
fn a_filled_field_keeps_its_non_null_form_and_state_keeps_its_value() {
    for (language, realm, source) in NullableCases::ACCEPTED {
        assert_eq!(
            Sample::message(language, realm, source),
            None,
            "{language} refused:\n{source}"
        );
    }
}

#[test]
fn python_tac_accepts_saved_fields_behaviors_and_listeners() {
    for saved in ["local_state", "session_state"] {
        let source = format!(
            "from typing import Annotated\n\n@service\nclass SampleService:\n    count: Annotated[int, {saved}] = 0\n"
        );
        assert_eq!(Sample::message("py", Realm::Tac, &source), None);
    }
    for source in [
        "@service\nclass SampleService:\n    @retry({\"attempts\": 3})\n    async def load(self) -> int:\n        return 1\n",
        "@service\n@cache_settings(\"prices\")\nclass SampleService:\n    @cached\n    async def load(self) -> int:\n        return 1\n",
        "@service\nclass SampleService:\n    @on_event\n    def heard(self, event: Placed) -> None:\n        pass\n",
    ] {
        assert_eq!(Sample::message("py", Realm::Tac, source), None, "{source}");
    }
}
