use super::PythonWebServices;
use super::python_graph_guard::PythonGraphGuard;
use super::python_provider_source::PythonProviderSource;
use crate::behavior::{PythonMembers, ServiceRoles};
use crate::route_types::ModelTable;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

struct Project;

impl Project {
    fn write(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap_or(root))
            .unwrap_or_else(|error| panic!("source directory: {error}"));
        std::fs::write(path, source).unwrap_or_else(|error| panic!("source: {error}"));
    }

    fn models(root: &Path) -> ModelTable {
        ModelTable::read_realm(root, "client", "py")
            .unwrap_or_else(|error| panic!("models: {error}"))
    }

    fn provider(source: &str) -> PythonProviderSource {
        PythonProviderSource::read(Path::new("client/services/CounterService.py"), source)
            .unwrap_or_else(|error| panic!("provider: {error:?}"))
    }
}

#[test]
fn python_roles_use_the_existing_pascal_and_snake_case_spellings() {
    for roles in ["@Service\n@NeedsAll", "@service\n@needs_all"] {
        let source = format!("{roles}\nclass CounterService:\n    count: int\n");
        let parsed = ServiceRoles::read(Path::new("CounterService.py"), &source)
            .unwrap_or_else(|error| panic!("roles: {error:?}"));
        assert!(parsed.prototype);
        assert_eq!(parsed.required, ["*"]);
        assert_eq!(parsed.source.len(), source.len());
        assert!(!parsed.source.contains('@'));
        let plain = ServiceRoles::without_roles(Path::new("CounterService.py"), &source)
            .unwrap_or_else(|error| panic!("plain roles: {error:?}"));
        assert!(!plain.contains("@Service") && !plain.contains("@service\n"));
        assert!(plain.contains("NeedsAll") || plain.contains("needs_all"));
    }
    let source = "@service_alias\nclass CounterService:\n    count: int = 0\n";
    let plain = ServiceRoles::without_roles(Path::new("CounterService.py"), source)
        .unwrap_or_else(|error| panic!("unknown spelling: {error:?}"));
    assert_eq!(plain, source);
}

#[test]
fn python_members_preserve_final_fields_and_typed_instance_methods() {
    let source = "from typing import Final, ClassVar\n\nclass CounterService:\n    start_count: Final[int]\n    current_count: int = 0\n    _secret: str = 'hidden'\n    shared: ClassVar[int] = 1\n\n    def add_count(self, value: int) -> int:\n        return self.current_count + value\n\n    def _hidden(self) -> str:\n        return self._secret\n";
    let members = PythonMembers::read(Path::new("CounterService.py"), source, "CounterService")
        .unwrap_or_else(|error| panic!("members: {error:?}"));
    assert_eq!(members["fields"].as_array().map(Vec::len), Some(2));
    assert_eq!(members["fields"][0]["page"], "startCount");
    assert_eq!(members["fields"][0]["immutable"], true);
    assert_eq!(members["methods"].as_array().map(Vec::len), Some(1));
    assert_eq!(members["methods"][0]["page"], "addCount");
    assert_eq!(members["methods"][0]["arguments"], json!(["integer"]));
    assert_eq!(members["methods"][0]["returns"], "integer");
}

#[test]
fn python_registry_selects_python_and_separates_props_from_providers() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    Project::write(
        root.path(),
        "client/helpers/CounterHelper.py",
        "@helper\nclass CounterHelper:\n    def amount(self) -> int:\n        return 2\n",
    );
    Project::write(
        root.path(),
        "client/services/CounterService.py",
        "@service\n@needs_all\nclass CounterService:\n    counter: CounterHelper\n    title: str\n",
    );
    let registry = crate::template::ServiceRegistry::discover(root.path())
        .unwrap_or_else(|error| panic!("registry: {error:?}"));
    let service = registry
        .get("counter")
        .unwrap_or_else(|| panic!("counter service"));
    assert_eq!(
        service.web_assembly().map(|(language, _)| language),
        Some(crate::tac_web::WebLanguage::Python)
    );
    assert_eq!(service.required(), ["title"]);
    assert_eq!(service.injected(), ["counter"]);
}

#[test]
fn python_needs_all_has_typed_props_and_hides_injected_fields() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    let provider = Project::provider(
        "@service\n@needs_all\nclass CounterService:\n    counter: CounterHelper\n    title: str\n\n    def answer(self) -> str:\n        return self.title\n",
    );
    let description = provider
        .describe(
            &BTreeSet::from(["CounterHelper".to_owned()]),
            &Project::models(root.path()),
        )
        .unwrap_or_else(|error| panic!("typed constructor: {error:?}"));
    assert_eq!(description["constructor"][0]["provider"], "CounterHelper");
    assert_eq!(description["constructor"][1]["shape"], "text");
    assert!(description["fields"].get("counter").is_none());
    assert_eq!(description["fields"]["title"]["shape"], "text");
    assert_eq!(description["methods"]["answer"]["returns"], "text");
}

#[test]
fn python_inferred_needs_cannot_become_an_unknown_page_property() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    let provider = Project::provider(
        "from typing import Annotated\n\n@service\nclass CounterService:\n    counter: Annotated[MissingHelper, needs]\n",
    );
    let failure = provider
        .describe(&BTreeSet::new(), &Project::models(root.path()))
        .err()
        .unwrap_or_else(|| panic!("unresolved dependency"));
    assert!(
        failure
            .to_string()
            .contains("declared same-language provider")
    );
}

#[test]
fn python_async_and_async_lifecycle_are_explicitly_pending() {
    for source in [
        "@service\nclass CounterService:\n    async def answer(self) -> int:\n        return 1\n",
        "@service\nclass CounterService:\n    @on_start\n    async def _start(self):\n        pass\n",
    ] {
        let failure =
            PythonProviderSource::read(Path::new("client/services/CounterService.py"), source)
                .err()
                .unwrap_or_else(|| panic!("async continuation is pending"));
        assert_eq!(failure.diagnostics()[0].code.number(), 1605);
        assert!(
            failure.to_string().contains("async")
                || failure.to_string().contains("synchronous OnStart")
        );
    }
}

#[test]
fn python_browser_apis_are_refused_at_build_time() {
    for (source, name) in [
        (
            "@service\nclass CounterService:\n    def stamp(self) -> int:\n        return Clock().now()\n",
            "Clock",
        ),
        (
            "@service\n@needs_all\nclass CounterService:\n    random: SecureRandom\n",
            "SecureRandom",
        ),
    ] {
        let failure =
            PythonProviderSource::read(Path::new("client/services/CounterService.py"), source)
                .err()
                .unwrap_or_else(|| panic!("{name} needs the page"));
        assert_eq!(failure.diagnostics()[0].code.number(), 1605);
        assert!(
            failure.to_string().contains(&format!("`{name}`")),
            "{failure}"
        );
    }
    let quoted =
        "@service\nclass CounterService:\n    label: str = 'Clock'\n    # Clock comes later\n";
    assert!(
        PythonProviderSource::read(Path::new("client/services/CounterService.py"), quoted).is_ok()
    );
}

#[test]
fn python_unknown_method_models_fail_before_runtime() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    let provider = Project::provider(
        "@service\nclass CounterService:\n    def accept(self, value: MissingModel) -> MissingModel:\n        return value\n",
    );
    let failure = provider
        .describe(&BTreeSet::new(), &Project::models(root.path()))
        .err()
        .unwrap_or_else(|| panic!("unknown DTO"));
    assert!(failure.to_string().contains("MissingModel"));
}

#[test]
fn python_graph_rejects_cycles_and_excessive_prototype_expansion() {
    let cycle = json!({"A": {"constructor": [{"provider": "B"}]}, "B": {"constructor": [{"provider": "A"}]}});
    assert!(PythonGraphGuard::check(cycle.as_object().unwrap_or_else(|| panic!("graph"))).is_err());
    let mut graph = serde_json::Map::new();
    graph.insert("Leaf".to_owned(), json!({"constructor": []}));
    for level in 0..13 {
        let child = if level == 0 {
            "Leaf".to_owned()
        } else {
            format!("Level{}", level - 1)
        };
        graph.insert(
            format!("Level{level}"),
            json!({"constructor": [{"provider": child}, {"provider": child}]}),
        );
    }
    let failure = PythonGraphGuard::check(&graph)
        .err()
        .unwrap_or_else(|| panic!("bounded graph"));
    assert!(failure.to_string().contains("4096"));
}

#[test]
fn python_program_uses_real_model_namespaces_without_executing_constructors() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    Project::write(
        root.path(),
        "client/models/Guide.py",
        "from dataclasses import dataclass\n\n@dataclass\nclass Guide:\n    title: str\n    pages: int\n",
    );
    let relative = "client/services/GuideService.py";
    Project::write(
        root.path(),
        relative,
        "from __future__ import annotations\nfrom models import Guide\n\n@service\nclass GuideService:\n    def echo(self, value: Guide) -> Guide:\n        return value\n",
    );
    let program = PythonWebServices::program(
        root.path(),
        &[(
            "guide".to_owned(),
            "GuideService".to_owned(),
            root.path().join(relative),
        )],
    )
    .unwrap_or_else(|error| panic!("compiled source: {error:?}"));
    assert!(
        program
            .trim_start()
            .starts_with("from __future__ import annotations")
    );
    assert!(program.contains("from models import Guide"));
    assert!(program.contains("_TacPythonRuntime.configure"));
    assert!(program.contains("\\\"model\\\":\\\"Guide\\\""));
    assert!(!program.contains("@service"));
    assert_eq!(
        std::fs::read_to_string(root.path().join(relative))
            .unwrap_or_else(|error| panic!("authored: {error}")),
        "from __future__ import annotations\nfrom models import Guide\n\n@service\nclass GuideService:\n    def echo(self, value: Guide) -> Guide:\n        return value\n"
    );
}

#[test]
fn python_missing_opt_in_profile_is_reported_by_doctor_without_installing() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    Project::write(
        root.path(),
        "client/services/CounterService.py",
        "@service\nclass CounterService:\n    count: int = 0\n",
    );
    let row = super::RuntimeDoctor::for_project(root.path())
        .unwrap_or_else(|error| panic!("doctor: {error:?}"))
        .unwrap_or_else(|| panic!("missing-profile row"));
    assert!(matches!(
        row.state,
        crate::doctor::ToolchainState::Incomplete(_)
    ));
    assert!(!root.path().join(".tachyon").exists());
    assert!(
        PythonWebServices::build(
            root.path(),
            &[(
                "counter".to_owned(),
                "CounterService".to_owned(),
                PathBuf::from("client/services/CounterService.py")
            )]
        )
        .is_err()
    );
}

struct PythonPage;

impl PythonPage {
    const OWNERS: [(&'static str, &'static str); 3] = [
        ("shelf", "00000000-0000-4000-8000-000000000001"),
        ("checkout", "00000000-0000-4000-8000-000000000002"),
        ("audit", "00000000-0000-4000-8000-000000000003"),
    ];
    const SHELF: [(&'static str, &'static str); 3] = [
        (
            "client/models/ShelfChanged.py",
            "from dataclasses import dataclass\n\n\n@topic\n@dataclass\nclass ShelfChanged:\n    count: int\n",
        ),
        (
            "client/models/ShelfCounted.py",
            "from dataclasses import dataclass\n\n\n@dataclass\nclass ShelfCounted:\n    total: int\n",
        ),
        (
            "client/services/ShelfService.py",
            "from models import ShelfChanged, ShelfCounted\n\n\n@service\nclass ShelfService:\n    total: int = 0\n    application_event_publisher: ApplicationEventPublisher\n\n    @on_event\n    def changed(self, changed: ShelfChanged) -> ShelfCounted:\n        self.total += changed.count\n        return ShelfCounted(self.total)\n\n    def announce(self) -> int:\n        self.application_event_publisher.publish_event(ShelfCounted(self.total))\n        return self.total\n",
        ),
    ];
    const SHELF_DRIVER: &'static str = "_report = {'init': _ask('shelf', {'op': 'init', 'props': {}}), 'heard': _ask('shelf', {'op': 'event', 'name': 'changed', 'value': {'count': 3}}),\n\
         \x20   'total': _ask('shelf', {'op': 'get', 'name': 'total'}), 'announced': _ask('shelf', {'op': 'call', 'name': 'announce', 'args': []})}\n";
    const CHECKOUT: [(&'static str, &'static str); 4] = [
        (
            "client/models/PlacedOrder.py",
            "from dataclasses import dataclass\n\n\n@dataclass\nclass PlacedOrder:\n    status: str\n    total: int\n",
        ),
        (
            "client/models/SettledOrder.py",
            "from dataclasses import dataclass\n\n\n@dataclass\nclass SettledOrder:\n    total: int\n",
        ),
        (
            "client/services/CheckoutService.py",
            "from models import PlacedOrder\n\n\n@service\nclass CheckoutService:\n    placed: int = 0\n    application_event_publisher: ApplicationEventPublisher\n\n    @on_start\n    def _start(self) -> None:\n        self.place()\n\n    def place(self) -> None:\n        self.placed += 1\n        self.application_event_publisher.publish_event(PlacedOrder(\"paid\", 25), True)\n",
        ),
        (
            "client/services/AuditService.py",
            "from models import PlacedOrder, SettledOrder\n\n\n@service\nclass AuditService:\n    settled: int = 0\n\n    @on_event({\"condition\":\"value.status == 'paid' && value.total >= 10\"})\n    @order(10)\n    def settle(self, order: PlacedOrder) -> SettledOrder:\n        return SettledOrder(order.total)\n\n    @on_event\n    @InBackground\n    def audit(self, order: SettledOrder) -> SettledOrder:\n        self.settled = order.total\n        return order\n",
        ),
    ];
    const CHECKOUT_DRIVER: &'static str = "_report = {'checkout': _ask('checkout', {'op': 'init', 'props': {}}), 'audit': _ask('audit', {'op': 'init', 'props': {}}),\n\
         \x20   'settled': _ask('audit', {'op': 'event', 'name': 'settle', 'value': {'status': 'paid', 'total': 25}}),\n\
         \x20   'audited': _ask('audit', {'op': 'event', 'name': 'audit', 'value': {'total': 25}})}\n";
    const DRIVER_HEAD: &'static str = "import json\n_emitted = []\n_TacPythonRuntime.connect(_emitted.append)\n_owners = {}\n\
         def _ask(service, request):\n    request.update({'route': 'web', 'service': service, 'owner': _owners[service]})\n    return json.loads(_TacPythonRuntime.invoke(json.dumps(request)))\n";
    const DRIVER_TAIL: &'static str = "_report['emitted'] = [json.loads(item) for item in _emitted]\nprint(json.dumps(_report))\n";

    fn has_python() -> bool {
        let found = std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !found {
            eprintln!("skipping: python3 is unavailable");
        }
        found
    }

    fn run(files: &[(&str, &str)], services: &[&str], driver: &str) -> serde_json::Value {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
        for (relative, source) in files {
            Project::write(root.path(), relative, source);
        }
        let chosen: Vec<(String, String, PathBuf)> = services
            .iter()
            .map(|instance| Self::service(root.path(), instance))
            .collect();
        let program = PythonWebServices::program(root.path(), &chosen)
            .unwrap_or_else(|error| panic!("compiled source: {error:?}"));
        let script = root.path().join("drive.py");
        let owners = serde_json::to_string(&Self::OWNERS).unwrap_or_default();
        let text = format!(
            "{program}\n{}_owners.update(dict(json.loads({owners:?})))\n{driver}{}",
            Self::DRIVER_HEAD,
            Self::DRIVER_TAIL
        );
        std::fs::write(&script, text).unwrap_or_else(|error| panic!("driver: {error}"));
        let output = std::process::Command::new("python3")
            .arg(&script)
            .output()
            .unwrap_or_else(|error| panic!("python3: {error}"));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("report: {error}"))
    }

    fn service(root: &Path, instance: &str) -> (String, String, PathBuf) {
        let class = format!("{}{}Service", instance[..1].to_uppercase(), &instance[1..]);
        let path = root.join(format!("client/services/{class}.py"));
        (instance.to_owned(), class, path)
    }

    fn signal(
        service: &str,
        name: &str,
        value: &serde_json::Value,
        retain: bool,
    ) -> serde_json::Value {
        let owner = Self::OWNERS
            .iter()
            .find(|(name, _)| *name == service)
            .map(|(_, owner)| *owner);
        json!({"route": "web", "service": service, "owner": owner,
               "name": name, "value": value, "retain": retain})
    }
}

#[test]
fn a_python_listener_hears_its_topic_and_publishes_to_the_page() {
    if !PythonPage::has_python() {
        return;
    }
    let report = PythonPage::run(&PythonPage::SHELF, &["shelf"], PythonPage::SHELF_DRIVER);
    let declared = &report["init"]["value"];
    assert_eq!(declared["fields"], json!(["total"]), "{report}");
    assert_eq!(declared["methods"], json!(["announce"]), "{report}");
    assert_eq!(declared["listeners"][0]["key"], "changed", "{report}");
    assert_eq!(declared["listeners"][0]["events"], json!(["ShelfChanged"]));
    assert_eq!(report["heard"], json!({"value": null}), "{report}");
    assert_eq!(report["total"], json!({"value": 3}), "{report}");
    assert_eq!(report["announced"], json!({"value": 3}), "{report}");
    let counted = PythonPage::signal("shelf", "ShelfCounted", &json!({"total": 3}), false);
    assert_eq!(report["emitted"], json!([counted, counted]));
}

#[test]
fn python_listener_options_and_a_retained_start_event_reach_the_page() {
    if !PythonPage::has_python() {
        return;
    }
    let report = PythonPage::run(
        &PythonPage::CHECKOUT,
        &["checkout", "audit"],
        PythonPage::CHECKOUT_DRIVER,
    );
    let listeners = report["audit"]["value"]["listeners"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let listener = |key: &str| {
        listeners
            .iter()
            .find(|listener| listener["key"] == key)
            .cloned()
            .unwrap_or_else(|| panic!("{key}: {report}"))
    };
    assert_eq!(listener("settle")["order"], 10, "{report}");
    assert!(listener("settle")["condition"].is_object(), "{report}");
    assert_eq!(listener("audit")["asynchronous"], true, "{report}");
    assert_eq!(report["audited"], json!({"value": null}), "{report}");
    assert_eq!(
        report["emitted"],
        json!([
            PythonPage::signal(
                "checkout",
                "PlacedOrder",
                &json!({"status": "paid", "total": 25}),
                true
            ),
            PythonPage::signal("audit", "SettledOrder", &json!({"total": 25}), false),
        ])
    );
}

#[test]
fn python_program_lists_fields_whose_default_is_a_model_call() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    Project::write(
        root.path(),
        "client/models/Product.py",
        "from dataclasses import dataclass\n\n@dataclass\nclass Product:\n    name: str\n    price: str\n",
    );
    let relative = "client/services/ShopService.py";
    Project::write(
        root.path(),
        relative,
        "from models import Product\n\n@service\nclass ShopService:\n    count: int = 6\n    label: str = str(5)\n    product: Product = Product(name=\"A\", price=\"1\")\n",
    );
    let program = PythonWebServices::program(
        root.path(),
        &[(
            "shop".to_owned(),
            "ShopService".to_owned(),
            root.path().join(relative),
        )],
    )
    .unwrap_or_else(|error| panic!("compiled source: {error:?}"));
    for name in ["count", "label", "product"] {
        assert!(
            program.contains(&format!("\\\"name\\\":\\\"{name}\\\"")),
            "{name} is missing from the service fields"
        );
    }
    assert!(program.contains("\\\"model\\\":\\\"Product\\\""));
}

#[test]
fn python_saved_fields_are_writable_page_fields() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    let provider = Project::provider(
        "from typing import Annotated\n\n@service\nclass CounterService:\n    draft: Annotated[str, session_state] = ''\n    dark_mode: Annotated[bool, local_state] = False\n",
    );
    let description = provider
        .describe(&BTreeSet::new(), &Project::models(root.path()))
        .unwrap_or_else(|error| panic!("saved fields: {error:?}"));
    assert_eq!(description["fields"]["draft"]["shape"], "text");
    assert_eq!(description["fields"]["darkMode"]["name"], "dark_mode");
    assert_eq!(description["writable"], json!(["darkMode", "draft"]));
}

#[test]
fn python_saved_fields_reach_the_page_service_entry() {
    let root = tempfile::tempdir().unwrap_or_else(|error| panic!("project: {error}"));
    Project::write(
        root.path(),
        "client/services/EditorService.py",
        "from typing import Annotated\n\n@service\nclass EditorService:\n    draft: Annotated[str, session_state] = ''\n    dark_mode: Annotated[bool, local_state] = False\n",
    );
    let registry = crate::template::ServiceRegistry::discover(root.path())
        .unwrap_or_else(|error| panic!("registry: {error:?}"));
    let service = registry
        .get("editor")
        .unwrap_or_else(|| panic!("editor service"));
    let stored = service
        .stored()
        .iter()
        .map(|(name, area)| (name.as_str(), area.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(stored, [("darkMode", "local"), ("draft", "session")]);
}
