use super::DeclaredContract;
use std::path::Path;

struct Samples;

impl Samples {
    const CONTRACTS: [(&'static str, &'static str); 13] = [
        (
            "java",
            "@HTTPClient\ninterface CatalogClient {\n    @GetRequest(\"/products\")\n    Object all(@RequestParam(\"page\") long page);\n\n    @Gateway({\"command\":[\"python3\"]})\n    Object ping();\n}\n",
        ),
        (
            "groovy",
            "@HTTPClient\ninterface CatalogClient {\n    @GetRequest(\"/products\")\n    Object all(@RequestParam(\"page\") long page)\n}\n",
        ),
        (
            "kt",
            "@HTTPClient\ninterface CatalogClient {\n    @GetRequest(\"/products\")\n    fun all(@RequestParam(\"page\") page: Long): Any?\n}\n",
        ),
        (
            "cs",
            "[HTTPClient]\npublic interface CatalogClient {\n    [GetRequest(\"/products\")]\n    Task<BehaviorJSON> All([RequestParam(\"page\")] BehaviorJSON page);\n}\n",
        ),
        (
            "php",
            "<?php\n#[HTTPClient]\ninterface CatalogClient {\n    #[GetRequest(\"/products\")]\n    public function all(#[RequestParam(\"page\")] int $page): array;\n}\n",
        ),
        (
            "go",
            "// @HTTPClient\ntype CatalogClient interface {\n\t// @GetRequest(\"/products\")\n\tAll(\n\t\t// @RequestParam(\"page\")\n\t\tpage BehaviorValue,\n\t) (BehaviorValue, error)\n}\n",
        ),
        (
            "dart",
            "@HTTPClient\nabstract interface class CatalogClient {\n  @GetRequest(\"/products\")\n  Future<Object?> all(@RequestParam(\"page\") int page);\n}\n",
        ),
        (
            "rs",
            "#[HTTPClient]\ntrait CatalogClient {\n    #[GetRequest(\"/products\")]\n    fn all(&self, #[RequestParam(\"page\")] page: BehaviorValue) -> BehaviorResult<BehaviorValue>;\n}\n",
        ),
        (
            "swift",
            "@HTTPClient\nprotocol CatalogClient {\n    @GetRequest(\"/products\")\n    func all(@RequestParam(\"page\") page: BehaviorJSON) async throws -> BehaviorJSON\n}\n",
        ),
        (
            "py",
            "from typing import Annotated, Protocol\n\n@http_client\nclass CatalogClient(Protocol):\n    @get_request(\"/products\")\n    async def all(self, page: Annotated[int, request_param(\"page\")]) -> object: ...\n",
        ),
        (
            "js",
            "@HTTPClient\nclass CatalogClient {\n  /** @param {number} page */\n  @GetRequest(\"/products\")\n  async all(@RequestParam(\"page\") page) {}\n}\n",
        ),
        (
            "ts",
            "@HTTPClient\nclass CatalogClient {\n  declare send: SocketSend<Product>;\n\n  @GetRequest(\"/products\")\n  async all(@RequestParam(\"page\") page: number): Promise<Product | void> {}\n}\n",
        ),
        (
            "rb",
            "#[HTTPClient]\nclass CatalogClient\n  # @param page [Integer]\n  # @return [Hash]\n  #[GetRequest(\"/products\")]\n  def all(\n    #[RequestParam(\"page\")]\n    page\n  )\n  end\nend\n",
        ),
    ];

    const CLASS_FORMS: [(&'static str, &'static str); 10] = [
        (
            "java",
            "@HTTPClient\nclass CatalogClient {\n    @GetRequest(\"/products\")\n    Object all() {}\n}\n",
        ),
        (
            "groovy",
            "@HTTPClient\nclass CatalogClient {\n    @GetRequest(\"/products\")\n    Object all() {}\n}\n",
        ),
        (
            "kt",
            "@HTTPClient\nclass CatalogClient {\n    @GetRequest(\"/products\")\n    fun all(): Any? {}\n}\n",
        ),
        (
            "cs",
            "[HTTPClient]\nclass CatalogClient {\n    [GetRequest(\"/products\")]\n    public async Task<BehaviorJSON> All() {}\n}\n",
        ),
        (
            "php",
            "<?php\n#[HTTPClient]\nclass CatalogClient {\n    #[GetRequest(\"/products\")]\n    public function all(): array {}\n}\n",
        ),
        ("go", "// @HTTPClient\ntype CatalogClient struct {\n}\n"),
        (
            "dart",
            "@HTTPClient\nclass CatalogClient {\n  @GetRequest(\"/products\")\n  Future<Object?> all() async {}\n}\n",
        ),
        ("rs", "#[HTTPClient]\nstruct CatalogClient;\n"),
        (
            "swift",
            "@HTTPClient\nfinal class CatalogClient {\n    @GetRequest(\"/products\")\n    func all() async throws -> BehaviorJSON {}\n}\n",
        ),
        (
            "py",
            "@http_client\nclass CatalogClient:\n    @get_request(\"/products\")\n    async def all(self):\n        pass\n",
        ),
    ];

    const BODIES: [(&'static str, &'static str); 13] = [
        (
            "java",
            "@HTTPClient\ninterface CatalogClient {\n    @GetRequest(\"/products\")\n    default Object all() { return 2; }\n}\n",
        ),
        (
            "groovy",
            "@HTTPClient\ninterface CatalogClient {\n    @GetRequest(\"/products\")\n    default Object all() { return 2 }\n}\n",
        ),
        (
            "kt",
            "@HTTPClient\ninterface CatalogClient {\n    @GetRequest(\"/products\")\n    fun all(): Any? = 2\n}\n",
        ),
        (
            "cs",
            "[HTTPClient]\npublic interface CatalogClient {\n    [GetRequest(\"/products\")]\n    BehaviorJSON All() { return null; }\n}\n",
        ),
        (
            "php",
            "<?php\n#[HTTPClient]\ninterface CatalogClient {\n    #[GetRequest(\"/products\")]\n    public function all(): array { return []; }\n}\n",
        ),
        (
            "go",
            "// @HTTPClient\ntype CatalogClient interface {\n\t// @GetRequest(\"/products\")\n\tAll() (BehaviorValue, error)\n}\n\nfunc (client *CatalogClient) Extra() int {\n\treturn 2\n}\n",
        ),
        (
            "dart",
            "@HTTPClient\nabstract interface class CatalogClient {\n  @GetRequest(\"/products\")\n  Future<Object?> all() async { return 2; }\n}\n",
        ),
        (
            "rs",
            "#[HTTPClient]\ntrait CatalogClient {\n    #[GetRequest(\"/products\")]\n    fn all(&self) -> BehaviorResult<BehaviorValue> { Ok(BehaviorValue::Null) }\n}\n",
        ),
        (
            "swift",
            "@HTTPClient\nprotocol CatalogClient {\n    var count: Int { get }\n}\n",
        ),
        (
            "py",
            "from typing import Protocol\n\n@http_client\nclass CatalogClient(Protocol):\n    @get_request(\"/products\")\n    async def all(self):\n        return {'query': {'page': 1}}\n",
        ),
        (
            "js",
            "@HTTPClient\nclass CatalogClient {\n  @GetRequest(\"/products\")\n  async all() { return { query: { page: 1 } }; }\n}\n",
        ),
        (
            "ts",
            "@HTTPClient\nclass CatalogClient {\n  surcharge(): number { return 2; }\n}\n",
        ),
        (
            "rb",
            "#[HTTPClient]\nclass CatalogClient\n  #[GetRequest(\"/products\")]\n  def all\n    { \"query\" => { \"page\" => 1 } }\n  end\nend\n",
        ),
    ];

    const LOWERED: [(&'static str, [&'static str; 3]); 12] = [
        (
            "java",
            [
                "class CatalogClient",
                "Object all(@RequestParam(\"page\") long page) {}",
                "Behaviors.create(\"CatalogSetup\"",
            ],
        ),
        (
            "groovy",
            [
                "class CatalogClient",
                "Object all(@RequestParam(\"page\") long page) {}",
                "@OnStart",
            ],
        ),
        (
            "kt",
            [
                "class CatalogClient",
                "fun all(@RequestParam(\"page\") page: Long): Any? {}",
                "lateinit var behaviors",
            ],
        ),
        (
            "cs",
            [
                "class CatalogClient",
                "public async Task<BehaviorJSON> All([RequestParam(\"page\")] BehaviorJSON page) {}",
                "typeof(CatalogSetup)",
            ],
        ),
        (
            "php",
            [
                "class CatalogClient",
                "public function all(#[RequestParam(\"page\")] int $page): array {}",
                "CatalogSetup::class",
            ],
        ),
        (
            "go",
            [
                "type CatalogClient struct {",
                "func (client *CatalogClient) All(",
                "BehaviorsCreate[CatalogSetup]",
            ],
        ),
        (
            "dart",
            [
                "class CatalogClient",
                "Future<Object?> all(@RequestParam(\"page\") int page) async {}",
                "late BehaviorContext behaviors;",
            ],
        ),
        (
            "rs",
            [
                "struct CatalogClient { behaviors: BehaviorContext }\nimpl CatalogClient {",
                "BehaviorResult<BehaviorValue> {}",
                "Behaviors::create::<CatalogSetup>",
            ],
        ),
        (
            "py",
            [
                "class CatalogClient:",
                "-> object: ...",
                "\n    @on_start\n    async def start_context(self):\n        self.behaviors = await",
            ],
        ),
        (
            "js",
            [
                "class CatalogClient",
                "async all(@RequestParam(\"page\") page) {}",
                "@OnStart",
            ],
        ),
        (
            "ts",
            [
                "class CatalogClient",
                "behaviors!: BehaviorContext;",
                "partition: 'catalog'",
            ],
        ),
        (
            "rb",
            [
                "class CatalogClient",
                "\n  #[OnStart]\n  def start_context\n",
                "Behaviors.create(CatalogSetup, partition: \"catalog\"",
            ],
        ),
    ];

    fn path(extension: &str) -> String {
        format!("server/remotes/CatalogClient.{extension}")
    }

    fn refusal(extension: &str, source: &str) -> String {
        let path = Self::path(extension);
        let failure = DeclaredContract::check(Path::new(&path), source)
            .expect_err(&format!("{extension} must be refused: {source}"));
        let text = failure.to_string();
        assert!(text.contains("TY1308"), "{extension}: {text}");
        text
    }

    fn concrete(extension: &str, realm: &str) -> String {
        let source = Self::CONTRACTS
            .iter()
            .find(|(language, _)| *language == extension)
            .map(|(_, source)| *source)
            .expect("sample");
        DeclaredContract::concrete(Path::new(&Self::path(extension)), source, realm)
            .unwrap_or_else(|| panic!("{extension} was not lowered"))
    }
}

#[test]
fn every_language_accepts_its_contract_form() {
    for (extension, source) in Samples::CONTRACTS {
        let path = Samples::path(extension);
        let checked = DeclaredContract::check(Path::new(&path), source);
        assert!(checked.is_ok(), "{extension}: {checked:?}");
    }
}

#[test]
fn a_class_client_is_refused_where_the_language_has_a_contract_form() {
    for (extension, source) in Samples::CLASS_FORMS {
        let message = Samples::refusal(extension, source);
        assert!(message.contains("declare it as"), "{extension}: {message}");
    }
}

#[test]
fn any_method_body_is_refused_in_every_language() {
    for (extension, source) in Samples::BODIES {
        let message = Samples::refusal(extension, source);
        assert!(
            message.contains("An HTTPClient has no implementation"),
            "{extension}: {message}"
        );
    }
}

#[test]
fn a_method_without_a_request_annotation_is_refused() {
    let source = "@HTTPClient\ninterface CatalogClient {\n    Object all();\n}\n";
    let message = Samples::refusal("java", source);
    assert!(message.contains("has no request annotation"), "{message}");
}

#[test]
fn a_csharp_contract_that_is_not_public_is_refused() {
    for header in ["interface", "internal interface"] {
        let source = format!(
            "[HTTPClient]\n{header} CatalogClient {{\n    [GetRequest(\"/products\")]\n    BehaviorJSON All();\n}}\n"
        );
        let message = Samples::refusal("cs", &source);
        assert!(message.contains("CS0051"), "{header}: {message}");
    }
}

#[test]
fn a_client_that_keeps_state_is_refused() {
    let source = "@HTTPClient\nclass CatalogClient {\n  behaviors;\n\n  @GetRequest(\"/products\")\n  async all() {}\n}\n";
    let message = Samples::refusal("js", source);
    assert!(message.contains("keeps no other state"), "{message}");
}

#[test]
fn a_client_with_another_role_is_refused() {
    let source = "@Service\n@HTTPClient\nclass CatalogService {\n  @GetRequest(\"/products\")\n  async all() {}\n}\n";
    let message = Samples::refusal("js", source);
    assert!(message.contains("is its own role"), "{message}");
}

#[test]
fn every_contract_lowers_into_its_concrete_contract() {
    for (extension, parts) in Samples::LOWERED {
        let lowered = Samples::concrete(extension, "yon");
        for part in parts {
            assert!(
                lowered.contains(part),
                "{extension} lacks {part:?}:\n{lowered}"
            );
        }
        for contract in [
            "interface CatalogClient",
            "trait CatalogClient",
            "(Protocol)",
        ] {
            assert!(
                !lowered.contains(contract),
                "{extension} kept {contract:?}:\n{lowered}"
            );
        }
    }
}

#[test]
fn tac_clients_create_their_context_when_they_are_built() {
    let expected = [
        ("kt", "constructor(context: Behaviors.Context) {"),
        (
            "cs",
            "public CatalogClient() : this(Behaviors.create(typeof(CatalogSetup)",
        ),
        (
            "dart",
            "late final BehaviorContext behaviors = _suppliedContext ?? Behaviors.create(CatalogSetup",
        ),
        ("swift", "init(_ context: BehaviorContext) {"),
        ("go", "func NewCatalogClient() (*CatalogClient, error) {"),
        ("rs", "fn new() -> Self {"),
        ("java", "CatalogClient(Behaviors.Context context) {"),
        (
            "java",
            "this(Behaviors.create(\"CatalogSetup\", new Behaviors.Options(\"catalog\"",
        ),
        ("rb", "def initialize(context = nil)"),
        (
            "rb",
            "@behaviors = context || Behaviors.create(CatalogSetup, partition: \"catalog\"",
        ),
    ];
    for (extension, part) in expected {
        let lowered = Samples::concrete(extension, "tac");
        assert!(
            lowered.contains(part),
            "{extension} lacks {part:?}:\n{lowered}"
        );
        assert!(!lowered.contains("OnStart"), "{extension}:\n{lowered}");
    }
}

#[test]
fn a_gateway_only_client_gets_no_context_and_a_named_setup_is_used() {
    let gateway = "#[HTTPClient]\ntrait ReportClient {\n    #[Gateway({\"command\":[\"python3\"]})]\n    fn ping(&self) -> BehaviorResult<BehaviorValue>;\n}\n";
    let lowered =
        DeclaredContract::concrete(Path::new("server/remotes/ReportClient.rs"), gateway, "yon")
            .expect("lowered");
    assert!(
        lowered.contains("struct ReportClient;\nimpl ReportClient {"),
        "{lowered}"
    );
    assert!(lowered.contains("fn new() -> Self { Self }"), "{lowered}");
    let named = "@HTTPClient({\"adapter\":\"api\",\"setup\":\"RuntimeAdaptersSetup\"})\ninterface CatalogClient {\n    @GetRequest(\"/products\")\n    Object all();\n}\n";
    let lowered =
        DeclaredContract::concrete(Path::new("server/remotes/CatalogClient.java"), named, "yon")
            .expect("lowered");
    assert!(
        lowered
            .contains("Behaviors.create(\"RuntimeAdaptersSetup\", new Behaviors.Options(\"api\""),
        "{lowered}"
    );
}

#[test]
fn a_go_socket_client_needs_no_context_and_gets_a_send_method() {
    let source = "// @HTTPClient(\"/chat\")\ntype ChatClient interface {\n\t// @SocketRequest\n\tReceive(message ChatMessage)\n}\n";
    let path = Path::new("client/remotes/ChatClient.go");
    let lowered = DeclaredContract::concrete(path, source, "tac").expect("lowered");
    for part in [
        "type ChatClient struct{}\n",
        "// @SocketRequest\nfunc (client *ChatClient) Receive(message ChatMessage) {\n}\n",
        "\nfunc (client *ChatClient) Send(message any) error {\n\treturn behaviorSocketSend(\"ChatClient\", message)\n}\n",
        "func NewChatClient() (*ChatClient, error) {\n\treturn &ChatClient{}, nil\n}",
    ] {
        assert!(lowered.contains(part), "{part:?}:\n{lowered}");
    }
    let stream = source.replace("SocketRequest", "StreamRequest");
    let lowered = DeclaredContract::concrete(path, &stream, "tac").expect("lowered");
    assert!(!lowered.contains("Send("), "{lowered}");
    assert!(lowered.contains("type ChatClient struct{}\n"), "{lowered}");
}

#[test]
fn a_swift_socket_client_needs_no_context_and_gets_a_send_method() {
    let source = "@HTTPClient(\"/chat\")\nprotocol ChatClient {\n    @SocketRequest\n    func receive(_ message: ChatMessage)\n}\n";
    let path = Path::new("client/remotes/ChatClient.swift");
    let lowered = DeclaredContract::concrete(path, source, "tac").expect("lowered");
    for part in [
        "final class ChatClient {\n",
        "    func receive(_ message: ChatMessage) {}\n",
        "\n    func send<Message: Encodable>(_ message: Message) async throws {\n        try await tacSocketSend(\"ChatClient\", message)\n    }\n",
    ] {
        assert!(lowered.contains(part), "{part:?}:\n{lowered}");
    }
    assert!(!lowered.contains("behaviors"), "{lowered}");
    let stream = source.replace("SocketRequest", "StreamRequest");
    let lowered = DeclaredContract::concrete(path, &stream, "tac").expect("lowered");
    assert!(!lowered.contains("send"), "{lowered}");
    assert!(!lowered.contains("behaviors"), "{lowered}");
}

#[test]
fn tac_browser_clients_stay_as_written() {
    let source = Samples::CONTRACTS[10].1;
    assert!(
        DeclaredContract::concrete(Path::new("client/remotes/CatalogClient.js"), source, "tac")
            .is_none()
    );
}

struct Migrations;

impl Migrations {
    const CLASS_CLIENTS: [(&'static str, &'static str); 11] = [
        (
            "java",
            "@HTTPClient\nclass CatalogClient {\n    Behaviors.Context behaviors;\n\n    @GetRequest(\"/products\")\n    Object all(@RequestParam(\"page\") long page) {}\n\n    @OnStart\n    void initialize() {\n        behaviors = Behaviors.create(\"CatalogSetup\", new Behaviors.Options(\"catalog\", new Behaviors.Signal()));\n    }\n}\n",
        ),
        (
            "groovy",
            "@HTTPClient\nclass CatalogClient {\n    Behaviors.Context behaviors\n\n    @GetRequest(\"/products\")\n    Object all(@RequestParam(\"page\") long page) {}\n}\n",
        ),
        (
            "kt",
            "@HTTPClient\nclass CatalogClient {\n    lateinit var behaviors: Behaviors.Context\n\n    @GetRequest(\"/products\")\n    fun all(@RequestParam(\"page\") page: Long): Any? {}\n\n    @OnStart\n    fun initialize() {\n        behaviors = Behaviors.create(\"CatalogSetup\", Behaviors.Options(\"catalog\", Behaviors.Signal()))\n    }\n}\n",
        ),
        (
            "cs",
            "[HTTPClient]\nclass CatalogClient {\n    public BehaviorContext behaviors;\n\n    [GetRequest(\"/products\")]\n    public async System.Threading.Tasks.Task<BehaviorJSON> All([RequestParam(\"page\")] BehaviorJSON page) {}\n}\n",
        ),
        (
            "php",
            "<?php\n#[HTTPClient]\nclass CatalogClient {\n    public BehaviorContext $behaviors;\n\n    #[GetRequest(\"/products\")]\n    public function all(#[RequestParam(\"page\")] int $page): array {}\n}\n",
        ),
        (
            "dart",
            "@HTTPClient\nclass CatalogClient {\n  late BehaviorContext behaviors;\n\n  @GetRequest(\"/products\")\n  Future<Object?> all(@RequestParam(\"page\") int page) async {}\n}\n",
        ),
        (
            "swift",
            "@HTTPClient\nfinal class CatalogClient {\n    let behaviors: BehaviorContext = try! Behaviors.create(CatalogSetup.self, partition: \"catalog\", signal: BehaviorSignal())\n\n    @GetRequest(\"/products\")\n    func all() async throws -> BehaviorJSON {}\n}\n",
        ),
        (
            "rs",
            "#[HTTPClient]\nstruct CatalogClient { behaviors: BehaviorContext }\nimpl CatalogClient {\n    fn new() -> Self {\n        let options = BehaviorOptions::new(\"catalog\", BehaviorSignal::new());\n        Self { behaviors: Behaviors::create::<CatalogSetup>(options).expect(\"catalog context\") }\n    }\n\n    #[GetRequest(\"/products\")]\n    fn all(&self, #[RequestParam(\"page\")] page: BehaviorValue) -> BehaviorResult<BehaviorValue> {}\n}\n",
        ),
        (
            "go",
            "// @HTTPClient\ntype CatalogClient struct {\n\tbehaviors *BehaviorContext\n}\n\n// @OnStart\nfunc (client *CatalogClient) Start() error {\n\tcontext, err := BehaviorsCreate[CatalogSetup](NewBehaviorOptions(\"catalog\", NewBehaviorSignal()))\n\tclient.behaviors = context\n\treturn err\n}\n\n// @GetRequest(\"/products\")\nfunc (client *CatalogClient) All(\n\t// @RequestParam(\"page\")\n\tpage BehaviorValue,\n) (BehaviorValue, error) {\n}\n",
        ),
        (
            "py",
            "from typing import Annotated\n\n@http_client\nclass CatalogClient:\n    behaviors: object\n\n    @get_request(\"/products\")\n    async def all(self, page: Annotated[object, request_param(\"page\")]) -> None:\n        pass\n\n    @on_start\n    async def initialize(self):\n        self.behaviors = await Behaviors.create(CatalogSetup, {'partition': 'catalog', 'signal': BehaviorSignal()})\n",
        ),
        (
            "rs",
            "#[HTTPClient]\nstruct ReportClient;\nimpl ReportClient {\n    fn new() -> Self { Self }\n\n    #[Gateway({\"command\":[\"python3\"]})]\n    fn ping(&self) -> BehaviorResult<BehaviorValue> {}\n}\n",
        ),
    ];

    fn rewritten(extension: &str, source: &str) -> String {
        let path = format!("server/remotes/CatalogClient.{extension}");
        let mut edits = DeclaredContract::rewrite(Path::new(&path), source)
            .unwrap_or_else(|problem| panic!("{extension}: {problem}"));
        assert!(!edits.is_empty(), "{extension} was not rewritten");
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut output = source.to_owned();
        for (range, text) in edits {
            output.replace_range(range, &text);
        }
        output
    }
}

#[test]
fn the_fix_turns_every_class_client_into_a_contract_that_passes_the_check() {
    for (extension, source) in Migrations::CLASS_CLIENTS {
        let rewritten = Migrations::rewritten(extension, source);
        let path = format!("server/remotes/CatalogClient.{extension}");
        let checked = DeclaredContract::check(Path::new(&path), &rewritten);
        assert!(checked.is_ok(), "{extension}: {checked:?}\n{rewritten}");
        assert!(
            !rewritten.contains("Behaviors.create"),
            "{extension}:\n{rewritten}"
        );
        assert!(
            !rewritten.contains("create::<"),
            "{extension}:\n{rewritten}"
        );
    }
}

#[test]
fn the_fix_leaves_a_client_with_logic_to_the_author() {
    let source = "@HTTPClient\nclass BillingClient {\n  int surcharge() { return 2; }\n}\n";
    let problem = DeclaredContract::rewrite(Path::new("server/remotes/BillingClient.java"), source)
        .expect_err("logic stays manual");
    assert!(
        problem.contains("Move the logic into a service"),
        "{problem}"
    );
}

struct FsharpSamples;

impl FsharpSamples {
    const CONTRACT: &'static str = "open System.Threading.Tasks\n\n[<HTTPClient({\"adapter\":\"api\"})>]\ntype CatalogClient =\n    [<GetRequest(\"/products\")>]\n    abstract All: [<RequestParam(\"page\")>] page: BehaviorJSON -> Task<BehaviorJSON>\n";
    const GATEWAY: &'static str = "[<HTTPClient>]\ntype ReportClient =\n    [<Gateway({\"command\":[\"python3\"]})>]\n    abstract Forecast: months: BehaviorJSON -> BehaviorJSON\n";
    const CLASS_FORM: &'static str = "open System.Threading.Tasks\n\n[<HTTPClient>]\ntype CatalogClient() =\n    member val behaviors : BehaviorContext = Behaviors.create(typeof<CatalogSetup>, \"catalog\", new BehaviorSignal())\n\n    [<GetRequest(\"/products\")>]\n    abstract All: page: BehaviorJSON -> Task<BehaviorJSON>\n";
    const PATH: &'static str = "server/remotes/CatalogClient.fs";
}

#[test]
fn an_fsharp_contract_is_an_interface_and_a_class_client_is_refused() {
    let path = Path::new(FsharpSamples::PATH);
    let accepted = crate::annotation_layout::validate(path, FsharpSamples::CONTRACT);
    assert!(accepted.is_ok(), "{accepted:?}");
    let refused = crate::annotation_layout::validate(path, FsharpSamples::CLASS_FORM)
        .expect_err("an F# class client is refused")
        .to_string();
    assert!(refused.contains("TY1308"), "{refused}");
}

#[test]
fn an_fsharp_contract_lowers_into_a_class_that_owns_its_context() {
    let lowered = DeclaredContract::concrete(
        Path::new(FsharpSamples::PATH),
        FsharpSamples::CONTRACT,
        "yon",
    )
    .expect("lowered");
    assert!(
        lowered.contains("type CatalogClient() =\n    member val behaviors : BehaviorContext = Behaviors.create(typeof<CatalogSetup>, \"api\", new BehaviorSignal())\n\n    [<GetRequest"),
        "{lowered}"
    );
    let gateway = DeclaredContract::concrete(
        Path::new("server/remotes/ReportClient.fs"),
        FsharpSamples::GATEWAY,
        "yon",
    )
    .expect("lowered");
    assert!(
        gateway.contains("type ReportClient() =\n    [<Gateway"),
        "{gateway}"
    );
    assert!(!gateway.contains("behaviors"), "{gateway}");
    let class_form = DeclaredContract::concrete(
        Path::new(FsharpSamples::PATH),
        FsharpSamples::CLASS_FORM,
        "yon",
    );
    assert!(class_form.is_none(), "{class_form:?}");
}

#[test]
fn an_object_type_in_a_typescript_signature_is_not_a_body() {
    let path = Path::new("client/remotes/CatalogClient.ts");
    let typed = "@HTTPClient\nexport default class CatalogClient {\n  @GetRequest(\"/api/products\")\n  async all(): Promise<{ items: string[] } | void> {}\n}\n";
    let checked = DeclaredContract::check(path, typed);
    assert!(checked.is_ok(), "{checked:?}");
    let written = "@HTTPClient\nexport default class CatalogClient {\n  @GetRequest(\"/api/products\")\n  async all(): Promise<{ items: string[] } | void> { return { items: [] }; }\n}\n";
    let refused = DeclaredContract::check(path, written)
        .expect_err("a body after an object type is refused")
        .to_string();
    assert!(
        refused.contains("CatalogClient.all has a body"),
        "{refused}"
    );
}

#[test]
fn the_fix_turns_an_fsharp_class_client_into_a_contract() {
    let path = Path::new(FsharpSamples::PATH);
    let mut edits = DeclaredContract::rewrite(path, FsharpSamples::CLASS_FORM).expect("rewritten");
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut rewritten = FsharpSamples::CLASS_FORM.to_owned();
    for (range, text) in edits {
        rewritten.replace_range(range, &text);
    }
    assert!(rewritten.contains("type CatalogClient =\n"), "{rewritten}");
    assert!(!rewritten.contains("behaviors"), "{rewritten}");
    let checked = crate::annotation_layout::validate(path, &rewritten);
    assert!(checked.is_ok(), "{checked:?}\n{rewritten}");
}

struct CachedSamples;

impl CachedSamples {
    const TAC_LANGUAGES: [&'static str; 10] = [
        "java", "kt", "cs", "go", "dart", "rs", "swift", "js", "ts", "rb",
    ];
    const SETTINGS: &'static str =
        "CacheSettings({\"name\":\"catalog\",\"ttlMs\":30000,\"maxBytes\":4096,\"maxEntries\":8})";

    fn cached(source: &str) -> String {
        let mut written = String::new();
        for line in source.split_inclusive('\n') {
            if Self::is_marker(line, "HTTPClient") {
                written.push_str(&line.replacen("HTTPClient", Self::SETTINGS, 1));
            }
            if Self::is_marker(line, "GetRequest(\"/products\")") {
                written.push_str(&line.replacen("GetRequest(\"/products\")", "Cached", 1));
            }
            written.push_str(line);
        }
        written
    }

    fn is_marker(line: &str, marker: &str) -> bool {
        let trimmed = line.trim();
        let written = trimmed.trim_start_matches("// ");
        let opened = ["@", "[", "#["]
            .iter()
            .any(|open| written.starts_with(open));
        [opened, written.contains(marker)] == [true, true]
    }
}

#[test]
fn a_tac_client_accepts_cache_settings_and_cached_reads_in_every_language() {
    for (extension, source) in Samples::CONTRACTS {
        if !CachedSamples::TAC_LANGUAGES.contains(&extension) {
            continue;
        }
        let cached = CachedSamples::cached(source);
        assert!(cached.contains("Cached"), "{extension}:\n{cached}");
        let path = Samples::path(extension).replace("server/", "client/");
        let checked = DeclaredContract::check(Path::new(&path), &cached);
        assert!(checked.is_ok(), "{extension}: {checked:?}\n{cached}");
        let lowered = DeclaredContract::concrete(Path::new(&path), &cached, "tac");
        let keeps = lowered
            .as_deref()
            .is_none_or(|text| text.contains("Cached"));
        assert!(keeps, "{extension} dropped Cached:\n{lowered:?}");
    }
}
