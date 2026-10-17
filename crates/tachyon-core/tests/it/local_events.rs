#![allow(clippy::expect_used)]

use crate::support::yon_layout;
use tachyon_contracts::{HandlerRequest, HttpMethod};
use tachyon_core::{
    HandlerCancellation, HandlerSource, HandlerSupervisor, HandlerSupervisorOptions,
};

fn expected() -> serde_json::Value {
    serde_json::json!({
        "log": ["paid", "wide:paid", "wide:free", "derived:one"],
        "tag": "resource",
    })
}

fn runtime(extension: &str) -> &'static str {
    match extension {
        "java" => "java",
        "kt" => "kotlinc",
        "cs" => "dotnet",
        "php" => "php-cgi",
        "py" => "python3",
        "rb" => "ruby",
        "lua" => "lua",
        "rs" => "rustc",
        "dart" => "dart",
        "go" => "go",
        _ => "node",
    }
}

fn available(program: &str) -> bool {
    let paths = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&paths).any(|directory| {
        directory.join(program).is_file()
            || (cfg!(windows)
                && ["exe", "bat", "cmd"]
                    .iter()
                    .any(|suffix| directory.join(format!("{program}.{suffix}")).is_file()))
    })
}

struct ProbeModels;

impl ProbeModels {
    const TYPESCRIPT: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "export interface ProbeReady {\n  status: string\n  total: integer\n}\n",
        ),
        (
            "ProbeDerived",
            "export interface ProbeDerived {\n  tag: string\n}\n",
        ),
        (
            "ProbeTagged",
            "export interface ProbeTagged {\n  tag: string\n}\n",
        ),
    ];

    const PYTHON: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "from dataclasses import dataclass\n\n\n@dataclass\nclass ProbeReady:\n    status: str\n    total: int\n",
        ),
        (
            "ProbeDerived",
            "from dataclasses import dataclass\n\n\n@dataclass\nclass ProbeDerived:\n    tag: str\n",
        ),
        (
            "ProbeTagged",
            "from dataclasses import dataclass\n\n\n@dataclass\nclass ProbeTagged:\n    tag: str\n",
        ),
    ];

    const PHP: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "<?php\nclass ProbeReady {\n    public function __construct(public string $status, public int $total) {}\n}\n",
        ),
        (
            "ProbeDerived",
            "<?php\nclass ProbeDerived {\n    public function __construct(public string $tag) {}\n}\n",
        ),
        (
            "ProbeTagged",
            "<?php\nclass ProbeTagged {\n    public function __construct(public string $tag) {}\n}\n",
        ),
    ];

    const RUBY: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "# @!attribute [r] status\n#   @return [String]\n# @!attribute [r] total\n#   @return [Integer]\nProbeReady = Data.define(:status, :total)\n",
        ),
        (
            "ProbeDerived",
            "# @!attribute [r] tag\n#   @return [String]\nProbeDerived = Data.define(:tag)\n",
        ),
        (
            "ProbeTagged",
            "# @!attribute [r] tag\n#   @return [String]\nProbeTagged = Data.define(:tag)\n",
        ),
    ];

    const JAVA: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "record ProbeReady(String status, long total) {}\n",
        ),
        ("ProbeDerived", "record ProbeDerived(String tag) {}\n"),
        ("ProbeTagged", "record ProbeTagged(String tag) {}\n"),
    ];

    const KOTLIN: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "data class ProbeReady(val status: String, val total: Long)\n",
        ),
        ("ProbeDerived", "data class ProbeDerived(val tag: String)\n"),
        ("ProbeTagged", "data class ProbeTagged(val tag: String)\n"),
    ];

    const CSHARP: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "public record ProbeReady(string Status, long Total);\n",
        ),
        ("ProbeDerived", "public record ProbeDerived(string Tag);\n"),
        ("ProbeTagged", "public record ProbeTagged(string Tag);\n"),
    ];

    const RUST: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "pub struct ProbeReady {\n    pub status: String,\n    pub total: i64,\n}\n",
        ),
        (
            "ProbeDerived",
            "pub struct ProbeDerived {\n    pub tag: String,\n}\n",
        ),
        (
            "ProbeTagged",
            "pub struct ProbeTagged {\n    pub tag: String,\n}\n",
        ),
    ];

    const DART: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "class ProbeReady {\n  final String status;\n  final int total;\n\n  const ProbeReady(this.status, this.total);\n}\n",
        ),
        (
            "ProbeDerived",
            "class ProbeDerived {\n  final String tag;\n\n  const ProbeDerived(this.tag);\n}\n",
        ),
        (
            "ProbeTagged",
            "class ProbeTagged {\n  final String tag;\n\n  const ProbeTagged(this.tag);\n}\n",
        ),
    ];

    const GO: [(&str, &str); 3] = [
        (
            "ProbeReady",
            "type ProbeReady struct {\n\tStatus string `json:\"status\"`\n\tTotal  int64  `json:\"total\"`\n}\n",
        ),
        (
            "ProbeDerived",
            "type ProbeDerived struct {\n\tTag string `json:\"tag\"`\n}\n",
        ),
        (
            "ProbeTagged",
            "type ProbeTagged struct {\n\tTag string `json:\"tag\"`\n}\n",
        ),
    ];

    fn files(extension: &str) -> [(&'static str, &'static str); 3] {
        match extension {
            "ts" => Self::TYPESCRIPT,
            "py" => Self::PYTHON,
            "php" => Self::PHP,
            "rb" => Self::RUBY,
            "java" => Self::JAVA,
            "kt" => Self::KOTLIN,
            "cs" => Self::CSHARP,
            "rs" => Self::RUST,
            "dart" => Self::DART,
            "go" => Self::GO,
            _ => [("", ""), ("", ""), ("", "")],
        }
    }

    fn write(root: &std::path::Path, extension: &str) {
        for (name, source) in Self::files(extension) {
            if name.is_empty() {
                continue;
            }
            let path = root.join(format!("server/models/{name}.{extension}"));
            std::fs::create_dir_all(path.parent().expect("models folder")).expect("models folder");
            std::fs::write(path, source).expect("model source");
        }
    }
}

async fn execute(extension: &str, source: &str) {
    execute_expected(extension, source, expected()).await;
}

async fn execute_expected(extension: &str, source: &str, expected: serde_json::Value) {
    if !runtime_available(extension) {
        return;
    }
    let root = tempfile::tempdir().expect("fixture");
    ProbeModels::write(root.path(), extension);
    let entry = format!("server/controllers/ProbeController.{extension}");
    let entry = yon_layout::write_fixture(root.path(), &entry, source);
    let source = HandlerSource::discover(root.path(), &entry).expect("event discovery");
    let mut options = HandlerSupervisorOptions::default();
    if extension == "ts" {
        options.runtimes.javascript = "bun".into();
    }
    let supervisor = HandlerSupervisor::new(options).expect("supervisor");
    assert_event_results(extension, &source, &supervisor, &expected).await;
}

fn runtime_available(extension: &str) -> bool {
    let program = runtime(extension);
    if !available(program) {
        assert_ne!(
            std::env::var("TAC_EVENTS_REQUIRE_TOOLCHAINS").as_deref(),
            Ok("1"),
            "Required event runtime {program} unavailable"
        );
        eprintln!("Skipping .{extension} event execution: {program} unavailable");
        return false;
    }
    true
}

async fn assert_event_results(
    extension: &str,
    source: &HandlerSource,
    supervisor: &HandlerSupervisor,
    expected: &serde_json::Value,
) {
    for i in 0..2 {
        let request =
            HandlerRequest::route(format!("events_{extension}_{i}"), "/probe", HttpMethod::Get);
        let result = supervisor
            .invoke(source, &request, &HandlerCancellation::default())
            .await
            .expect("event invocation");
        assert_eq!(result.status, 200, "{extension}: {:?}", result.error);
        let json: serde_json::Value =
            serde_json::from_str(&result.body.expect("response").data).expect("JSON");
        assert_eq!(&json, expected, "{extension}");
    }
}

#[tokio::test]
async fn javascript_listener_failures_payload_isolation_and_cleanup_do_not_change_success() {
    execute_expected(
        "js",
        r#"
/**
 * @typedef {Object} Probe
 * @property {integer} value
 */
@Service()
@NeedsAll()
class AuditService {
  applicationEventPublisher;
  report = {values: [], destroyed: 0, failedPublish: false};
  @OnStart()
  initialize() {
    /** @type {Probe} */
    const probe = {value: 7};
    this.applicationEventPublisher.publishEvent(probe);
    // An unencodable payload is refused without disturbing the invocation.
    /** @type {Probe} */
    const cyclic = {}; cyclic.self = cyclic;
    const returned = this.applicationEventPublisher.publishEvent(cyclic);
    this.#requireSameResult(returned, cyclic);
    this.report.failedPublish = true;
  }
  #requireSameResult(returned, published) {
    if (returned !== published) throw new Error('Publisher result identity changed');
  }
  /** @param {Probe} value */
  @OnEvent
  @Order(1)
  first(value) { value.value = 100; throw new Error('private failure detail'); }
  /** @param {Probe} value */
  @OnEvent
  @Order(2)
  async second(value) { this.report.values.push(value.value); }
  @OnStop()
  destroy() { this.report.destroyed++; }
}
@RequestRoute("/probe")
@Public
@NeedsAll()
class AuditController {
  auditService;
  @GetRoute
  getProbe() { return this.auditService.report; }
}
"#,
        serde_json::json!({"values":[7],"destroyed":1,"failedPublish":true}),
    )
    .await;
}

#[tokio::test]
async fn javascript_events_and_lifecycle() {
    execute(
        "js",
        r#"
/**
 * @typedef {Object} ProbeReady
 * @property {string} status
 * @property {integer} total
 */
/**
 * @typedef {Object} ProbeDerived
 * @property {string} tag
 */
/**
 * @typedef {Object} ProbeTagged
 * @property {string} tag
 */
@Service()
class ProbeService {
  log = [];
  tag = null;
  /** @param {ProbeReady} value */
  @OnEvent
  @Order(2)
  wide(value) { this.log.push('wide:' + value.status); }
  /**
   * @param {ProbeReady} value
   * @returns {ProbeDerived}
   */
  @OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})
  @Order(1)
  paid(value) { this.log.push('paid'); return {tag:'one'}; }
  /** @param {ProbeReady} value */
  @OnEvent({"condition":"value.total > 100"})
  @Order(0)
  skipped(value) { this.log.push('skipped'); }
  /** @param {ProbeDerived} value */
  @OnEvent
  derived(value) { this.log.push('derived:' + value.tag); }
  /** @param {ProbeTagged} value */
  @OnEvent({"replay":true})
  tagged(value) { this.tag = value.tag; }
}
@Service()
@NeedsAll()
class PublishService {
  applicationEventPublisher;
  @OnStart()
  initialize() {
    /** @type {ProbeReady} */
    const paid = {status:'paid', total:10};
    /** @type {ProbeReady} */
    const free = {status:'free', total:0};
    this.applicationEventPublisher.publishEvent(paid, true);
    this.applicationEventPublisher.publishEvent(free);
  }
}
@Service()
@NeedsAll()
class ResourceService {
  @Needs
  applicationEventPublisher;
  @OnStart()
  initialize() {
    /** @type {ProbeTagged} */
    const tagged = {tag:'resource'};
    /** @type {ProbeTagged} */
    const misfit = {tag:1};
    this.applicationEventPublisher.publishEvent(tagged);
    this.applicationEventPublisher.publishEvent(misfit);
  }
}
@RequestRoute("/probe")
@Public
@NeedsAll()
class ProbeController {
  probeService;
  publishService;
  resourceService;
  @GetRoute
  getProbe() { return {log: this.probeService.log, tag: this.probeService.tag}; }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn typescript_events_and_lifecycle() {
    execute(
        "ts",
        r#"
@Service()
class ProbeService {
  log: string[] = [];
  tag: string | null = null;
  @OnEvent
  @Order(2)
  wide(value: ProbeReady) { this.log.push('wide:' + value.status); }
  @OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})
  @Order(1)
  paid(value: ProbeReady): ProbeDerived { this.log.push('paid'); return {tag:'one'}; }
  @OnEvent({"condition":"value.total > 100"})
  @Order(0)
  skipped(value: ProbeReady) { this.log.push('skipped'); }
  @OnEvent
  derived(value: ProbeDerived) { this.log.push('derived:' + value.tag); }
  @OnEvent({"replay":true})
  tagged(value: ProbeTagged) { this.tag = value.tag; }
}
@Service()
@NeedsAll()
class PublishService {
  applicationEventPublisher!: ApplicationEventPublisher;
  @OnStart()
  initialize() {
    const paid: ProbeReady = {status:'paid', total:10};
    const free: ProbeReady = {status:'free', total:0};
    this.applicationEventPublisher.publishEvent(paid, true);
    this.applicationEventPublisher.publishEvent(free);
  }
}
@Service()
@NeedsAll()
class ResourceService {
  @Needs
  applicationEventPublisher!: ApplicationEventPublisher;
  @OnStart()
  initialize() {
    const tagged: ProbeTagged = {tag:'resource'};
    const misfit: ProbeTagged = {tag: 1 as unknown as string};
    this.applicationEventPublisher.publishEvent(tagged);
    this.applicationEventPublisher.publishEvent(misfit);
  }
}
@RequestRoute("/probe")
@Public
@NeedsAll()
class ProbeController {
  probeService!: ProbeService;
  publishService!: PublishService;
  resourceService!: ResourceService;
  @GetRoute
  getProbe() { return {log: this.probeService.log, tag: this.probeService.tag}; }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn python_events_and_lifecycle() {
    execute(
        "py",
        r#"
from models import ProbeReady, ProbeDerived, ProbeTagged
@service()
class ProbeService:
    @on_start()
    def initialize(self):
        self.log = []
        self.tag = None
    @on_event
    @order(2)
    def wide(self, value: ProbeReady):
        self.log.append('wide:' + value.status)
    @on_event({"condition":"value.status == 'paid' && value.total >= 10"})
    @order(1)
    def paid(self, value: ProbeReady) -> ProbeDerived:
        self.log.append('paid')
        return ProbeDerived("one")
    @on_event({"condition":"value.total > 100"})
    @order(0)
    def skipped(self, value: ProbeReady):
        self.log.append('skipped')
    @on_event
    def derived(self, value: ProbeDerived):
        self.log.append('derived:' + value.tag)
    @on_event({"replay":true})
    def tagged(self, value: ProbeTagged):
        self.tag = value.tag
from models import ProbeReady
@service()
@needs_all()
class PublishService:
    application_event_publisher: ApplicationEventPublisher
    @on_start()
    def initialize(self):
        self.application_event_publisher.publish_event(ProbeReady("paid", 10), True)
        self.application_event_publisher.publish_event(ProbeReady("free", 0))
from models import ProbeTagged
@service()
@needs_all()
class ResourceService:
    application_event_publisher: Annotated[ApplicationEventPublisher, needs]
    @on_start()
    def initialize(self):
        self.application_event_publisher.publishEvent(ProbeTagged("resource"))
        self.application_event_publisher.publishEvent(ProbeTagged(None))
@request_route("/probe")
@public
@needs_all()
class ProbeController:
    probe_service: ProbeService
    publish_service: PublishService
    resource_service: ResourceService
    @get_route
    def get_probe(self):
        return {"log": self.probe_service.log, "tag": self.probe_service.tag}
"#,
    )
    .await;
}

#[tokio::test]
async fn php_events_and_lifecycle() {
    execute(
        "php",
        r#"<?php
#[Service()]
class ProbeService {
  public $log = [];
  public $tag = null;
  #[OnEvent]
  #[Order(2)]
  function wide(\Models\ProbeReady $value) { $this->log[] = 'wide:' . $value->status; }
  #[OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})]
  #[Order(1)]
  function paid(\Models\ProbeReady $value): \Models\ProbeDerived { $this->log[] = 'paid'; return new \Models\ProbeDerived('one'); }
  #[OnEvent({"condition":"value.total > 100"})]
  #[Order(0)]
  function skipped(\Models\ProbeReady $value) { $this->log[] = 'skipped'; }
  #[OnEvent]
  function derived(\Models\ProbeDerived $value) { $this->log[] = 'derived:' . $value->tag; }
  #[OnEvent({"replay":true})]
  function tagged(\Models\ProbeTagged $value) { $this->tag = $value->tag; }
}
#[Service()]
#[NeedsAll()]
class PublishService {
  private ApplicationEventPublisher $publisher;
  #[OnStart()]
  function initialize() {
    $this->publisher->publishEvent(new \Models\ProbeReady('paid', 10), true);
    $this->publisher->publishEvent(new \Models\ProbeReady('free', 0));
  }
}
#[Service()]
#[NeedsAll()]
class ResourceService {
  #[Needs]
  private ApplicationEventPublisher $applicationEventPublisher;
  #[OnStart()]
  function initialize() { $this->applicationEventPublisher->publishEvent(new \Models\ProbeTagged('resource')); }
}
#[RequestRoute("/probe")]
#[Public]
#[NeedsAll()]
class ProbeController {
  public ProbeService $probe;
  public PublishService $publish;
  public ResourceService $resource;
  #[GetRoute]
  function getProbe() { return ["log" => $this->probe->log, "tag" => $this->probe->tag]; }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn php_listeners_keep_an_empty_object_as_an_object() {
    if !runtime_available("php-cgi") {
        return;
    }
    let root = tempfile::tempdir().expect("fixture");
    let model = root.path().join("server/models/ProbeNote.php");
    std::fs::create_dir_all(model.parent().expect("models folder")).expect("models folder");
    std::fs::write(
        model,
        "<?php\nclass ProbeNote {\n    public function __construct(public object $details, public mixed $extra) {}\n}\n",
    )
    .expect("model source");
    let entry = yon_layout::write_fixture(
        root.path(),
        "server/controllers/ProbeController.php",
        r#"<?php
#[Service()]
class ProbeService {
  public $kinds = null;
  #[OnEvent]
  function noted(\Models\ProbeNote $value) { $this->kinds = [get_debug_type($value->details), json_encode($value->extra)]; }
}
#[Service()]
#[NeedsAll()]
class PublishService {
  private ApplicationEventPublisher $publisher;
  #[OnStart()]
  function initialize() { $this->publisher->publishEvent(new \Models\ProbeNote(new \stdClass(), ['inner' => new \stdClass()])); }
}
#[RequestRoute("/probe")]
#[Public]
#[NeedsAll()]
class ProbeController {
  public ProbeService $probe;
  public PublishService $publish;
  #[GetRoute]
  function getProbe() { return ["kinds" => $this->probe->kinds]; }
}
"#,
    );
    let source = HandlerSource::discover(root.path(), &entry).expect("event discovery");
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    let expected = serde_json::json!({"kinds": ["stdClass", "{\"inner\":{}}"]});
    assert_event_results("php", &source, &supervisor, &expected).await;
}

#[tokio::test]
async fn php_background_listeners_run_after_the_delivery_pass_and_never_republish() {
    execute_expected(
        "php",
        r#"<?php
#[Service()]
class ProbeService {
  public $log = [];
  #[OnEvent]
  #[InBackground]
  function later(\Models\ProbeReady $value): \Models\ProbeDerived { $this->log[] = 'later:' . $value->status; return new \Models\ProbeDerived('never'); }
  #[OnEvent]
  #[Order(5)]
  function first(\Models\ProbeReady $value) { $this->log[] = 'first:' . $value->status; }
  #[OnEvent]
  function derived(\Models\ProbeDerived $value) { $this->log[] = 'derived:' . $value->tag; }
}
#[Service()]
#[NeedsAll()]
class PublishService {
  private ApplicationEventPublisher $publisher;
  #[OnStart()]
  function initialize() {
    $this->publisher->publishEvent(new \Models\ProbeReady('paid', 10));
    $this->publisher->publishEvent(new \Models\ProbeReady('free', 0));
  }
}
#[RequestRoute("/probe")]
#[Public]
#[NeedsAll()]
class ProbeController {
  public ProbeService $probe;
  public PublishService $publish;
  #[GetRoute]
  function getProbe() { return ["log" => $this->probe->log]; }
}
"#,
        serde_json::json!({"log": ["first:paid", "later:paid", "first:free", "later:free"]}),
    )
    .await;
}

#[tokio::test]
async fn ruby_events_and_lifecycle() {
    execute(
        "rb",
        r#"#[Service]
class ProbeService
  attr_reader :log, :tag

  def initialize
    @log = []
    @tag = nil
  end

  # @param value [ProbeReady]
  #[OnEvent]
  #[Order(2)]
  def wide(value)
    @log << "wide:#{value.status}"
  end

  # @param value [ProbeReady]
  # @return [ProbeDerived]
  #[OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})]
  #[Order(1)]
  def paid(value)
    @log << "paid"
    ProbeDerived.new(tag: "one")
  end

  # @param value [ProbeReady]
  #[OnEvent({"condition":"value.total > 100"})]
  #[Order(0)]
  def skipped(value)
    @log << "skipped"
  end

  # @param value [ProbeDerived]
  #[OnEvent]
  def derived(value)
    @log << "derived:#{value.tag}"
  end

  # @param value [ProbeTagged]
  #[OnEvent({"replay":true})]
  def tagged(value)
    @tag = value.tag
  end
end
#[Service]
class PublishService
  #[OnStart]
  def announce
    publisher = ApplicationEventPublisher.new
    publisher.publish_event(ProbeReady.new(status: "paid", total: 10), true)
    publisher.publish_event(ProbeReady.new(status: "free", total: 0))
  end
end
#[Service]
class ResourceService
  #[OnStart]
  def announce
    ApplicationEventPublisher.new.publish_event(ProbeTagged.new(tag: "resource"))
  end
end
#[RequestRoute("/probe")]
#[Public]
class ProbeController
  #[Needs]
  attr_reader :probe_service

  #[Needs]
  attr_reader :publish_service

  #[Needs]
  attr_reader :resource_service

  #[GetRoute]
  def get_probe
    { "log" => probe_service.log, "tag" => probe_service.tag }
  end
end
"#,
    )
    .await;
}

struct LuaProbe;

impl LuaProbe {
    const FILES: [(&str, &str); 7] = [
        (
            "server/models/ProbeReady.lua",
            "---@class ProbeReady\n---@field status string\n---@field total integer\nProbeReady = {}\n",
        ),
        (
            "server/models/ProbeDerived.lua",
            "---@class ProbeDerived\n---@field tag string\nProbeDerived = {}\n",
        ),
        (
            "server/models/ProbeTagged.lua",
            "---@class ProbeTagged\n---@field tag string\nProbeTagged = {}\n",
        ),
        (
            "server/services/ProbeService.lua",
            "-- @Service\n---@class ProbeService\nProbeService = {\n  ---@type string[]\n  log = {},\n  ---@type string?\n  tag = nil,\n}\n\n---@param value ProbeReady\n---@return nil\n-- @OnEvent\n-- @Order(2)\nfunction ProbeService:wide(value)\n  self.log[#self.log + 1] = \"wide:\" .. value.status\nend\n\n---@param value ProbeReady\n---@return ProbeDerived\n-- @OnEvent({\"condition\":\"value.status == 'paid' && value.total >= 10\"})\n-- @Order(1)\nfunction ProbeService:paid(value)\n  self.log[#self.log + 1] = \"paid\"\n  return setmetatable({ tag = \"one\" }, ProbeDerived)\nend\n\n---@param value ProbeReady\n---@return nil\n-- @OnEvent({\"condition\":\"value.total > 100\"})\n-- @Order(0)\nfunction ProbeService:skipped(value)\n  self.log[#self.log + 1] = \"skipped\"\nend\n\n---@param value ProbeDerived\n---@return nil\n-- @OnEvent\nfunction ProbeService:derived(value)\n  self.log[#self.log + 1] = \"derived:\" .. value.tag\nend\n\n---@param value ProbeTagged\n---@return nil\n-- @OnEvent({\"replay\":true})\nfunction ProbeService:tagged(value)\n  self.tag = value.tag\nend\n",
        ),
        (
            "server/services/PublishService.lua",
            "-- @Service\n---@class PublishService\nPublishService = {\n  ---@type ApplicationEventPublisher\n  application_event_publisher = nil,\n}\n\n---@return nil\n-- @OnStart\nfunction PublishService:announce()\n  self.application_event_publisher:publish_event(setmetatable({ status = \"paid\", total = 10 }, ProbeReady), true)\n  self.application_event_publisher:publish_event(setmetatable({ status = \"free\", total = 0 }, ProbeReady))\nend\n",
        ),
        (
            "server/services/ResourceService.lua",
            "-- @Service\n---@class ResourceService\nResourceService = {}\n\n---@return nil\n-- @OnStart\nfunction ResourceService:announce()\n  ApplicationEventPublisher:publish_event(setmetatable({ tag = \"resource\" }, ProbeTagged))\nend\n\n---@return nil\n-- @OnStop\nfunction ResourceService:close()\n  io.stderr:write(\"closed\\n\")\nend\n",
        ),
        (
            "server/controllers/ProbeController.lua",
            "-- @RequestRoute(\"/probe\")\n-- @Public\n---@class ProbeController\nProbeController = {\n  -- @Needs\n  ---@type ProbeService\n  probe_service = nil,\n  -- @Needs\n  ---@type PublishService\n  publish_service = nil,\n  -- @Needs\n  ---@type ResourceService\n  resource_service = nil,\n}\n\n---@return table\n-- @GetRoute\nfunction ProbeController:get_probe()\n  return { log = self.probe_service.log, tag = self.probe_service.tag }\nend\n",
        ),
    ];
}

#[tokio::test]
async fn lua_events_and_lifecycle() {
    if !runtime_available("lua") {
        return;
    }
    let root = tempfile::tempdir().expect("fixture");
    for (relative, text) in LuaProbe::FILES {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(path, text).expect("source");
    }
    let source = HandlerSource::discover(root.path(), "server/controllers/ProbeController.lua")
        .unwrap_or_else(|failure| panic!("{failure}"));
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    assert_event_results("lua", &source, &supervisor, &expected()).await;
}

#[tokio::test]
async fn a_lua_in_background_listener_runs_after_the_others_and_publishes_nothing() {
    if !runtime_available("lua") {
        return;
    }
    let root = tempfile::tempdir().expect("fixture");
    let files = [
        LuaProbe::FILES[0],
        LuaProbe::FILES[1],
        (
            "server/services/ProbeService.lua",
            "-- @Service\n---@class ProbeService\nProbeService = {\n  ---@type string[]\n  log = {},\n}\n\n---@param value ProbeReady\n---@return ProbeDerived\n-- @OnEvent\n-- @Order(-5)\n-- @InBackground\nfunction ProbeService:later(value)\n  self.log[#self.log + 1] = \"later:\" .. value.status\n  return setmetatable({ tag = \"late\" }, ProbeDerived)\nend\n\n---@param value ProbeReady\n---@return nil\n-- @OnEvent\nfunction ProbeService:now(value)\n  self.log[#self.log + 1] = \"now:\" .. value.status\nend\n\n---@param value ProbeDerived\n---@return nil\n-- @OnEvent\nfunction ProbeService:derived(value)\n  self.log[#self.log + 1] = \"derived:\" .. value.tag\nend\n",
        ),
        (
            "server/controllers/ProbeController.lua",
            "-- @RequestRoute(\"/probe\")\n-- @Public\n---@class ProbeController\nProbeController = {\n  -- @Needs\n  ---@type ProbeService\n  probe_service = nil,\n  ---@type ApplicationEventPublisher\n  application_event_publisher = nil,\n}\n\n---@return table\n-- @GetRoute\nfunction ProbeController:get_probe()\n  self.application_event_publisher:publish_event(setmetatable({ status = \"paid\", total = 10 }, ProbeReady))\n  local published = #self.probe_service.log\n  return { published = published, log = self.probe_service.log }\nend\n",
        ),
    ];
    for (relative, text) in files {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("folder")).expect("folder");
        std::fs::write(path, text).expect("source");
    }
    let source = HandlerSource::discover(root.path(), "server/controllers/ProbeController.lua")
        .unwrap_or_else(|failure| panic!("{failure}"));
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    let expected = serde_json::json!({"published": 0, "log": ["now:paid", "later:paid"]});
    assert_event_results("lua", &source, &supervisor, &expected).await;
}

#[tokio::test]
async fn ruby_publishes_an_event_that_no_listener_takes() {
    execute_expected(
        "rb",
        r#"#[RequestRoute("/probe")]
#[Public]
class ProbeController
  #[GetRoute]
  def get_probe
    ApplicationEventPublisher.new.publish_event(ProbeReady.new(status: "paid", total: 10))
    { "published" => true }
  end
end
"#,
        serde_json::json!({"published": true}),
    )
    .await;
}

#[tokio::test]
async fn ruby_runs_in_background_listeners_after_the_pass_without_republishing() {
    execute_expected(
        "rb",
        r#"#[Service]
class ProbeService
  attr_reader :log

  def initialize
    @log = []
  end

  # @param value [ProbeReady]
  # @return [ProbeDerived]
  #[OnEvent]
  #[InBackground]
  #[Order(-5)]
  def later(value)
    @log << "later:#{value.status}"
    ProbeDerived.new(tag: "unpublished")
  end

  # @param value [ProbeReady]
  #[OnEvent]
  #[Order(3)]
  def now(value)
    @log << "now:#{value.status}"
  end

  # @param value [ProbeDerived]
  #[OnEvent]
  def derived(value)
    @log << "derived:#{value.tag}"
  end
end
#[Service]
class PublishService
  #[OnStart]
  def announce
    publisher = ApplicationEventPublisher.new
    publisher.publish_event(ProbeReady.new(status: "paid", total: 10))
    publisher.publish_event(ProbeReady.new(status: "free", total: 0))
  end
end
#[RequestRoute("/probe")]
#[Public]
class ProbeController
  #[Needs]
  attr_reader :probe_service

  #[Needs]
  attr_reader :publish_service

  #[GetRoute]
  def get_probe
    { "log" => probe_service.log }
  end
end
"#,
        serde_json::json!({"log": ["now:paid", "later:paid", "now:free", "later:free"]}),
    )
    .await;
}

#[tokio::test]
async fn java_events_and_lifecycle() {
    execute(
        "java",
        r#"
import models.ProbeReady;
import models.ProbeDerived;
import models.ProbeTagged;
@Service()
class ProbeService {
  java.util.List<String> log = new java.util.ArrayList<>();
  String tag = "";
  @OnEvent
  @Order(2)
  void wide(ProbeReady value) { log.add("wide:" + value.status()); }
  @OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})
  @Order(1)
  ProbeDerived paid(ProbeReady value) { log.add("paid"); return new ProbeDerived("one"); }
  @OnEvent({"condition":"value.total > 100"})
  @Order(0)
  void skipped(ProbeReady value) { log.add("skipped"); }
  @OnEvent
  void derived(ProbeDerived value) { log.add("derived:" + value.tag()); }
  @OnEvent({"replay":true})
  void tagged(ProbeTagged value) { tag = value.tag(); }
}
import models.ProbeReady;
@Service()
@NeedsAll()
class PublishService {
  ApplicationEventPublisher publisher;
  @OnStart()
  void initialize() {
    publisher.publishEvent(new ProbeReady("paid", 10), true);
    publisher.publishEvent(new ProbeReady("free", 0));
  }
}
import models.ProbeTagged;
@Service()
@NeedsAll()
class ResourceService {
  @Needs
  ApplicationEventPublisher applicationEventPublisher;
  @OnStart()
  void initialize() {
    applicationEventPublisher.publishEvent(new ProbeTagged("resource"));
    applicationEventPublisher.publishEvent(new ProbeTagged(null));
  }
}
@RequestRoute("/probe")
@Public
@NeedsAll()
class ProbeController {
  ProbeService probe;
  PublishService publish;
  ResourceService resource;
  @GetRoute
  Object getProbe() { return java.util.Map.of("log", probe.log, "tag", probe.tag); }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn kotlin_events_and_lifecycle() {
    execute(
        "kt",
        r#"
@Service()
class ProbeService {
  val log = mutableListOf<String>()
  var tag: String = ""
  @OnEvent
  @Order(2)
  fun wide(value: models.ProbeReady) { log.add("wide:" + value.status) }
  @OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})
  @Order(1)
  fun paid(value: models.ProbeReady): models.ProbeDerived { log.add("paid"); return models.ProbeDerived("one") }
  @OnEvent({"condition":"value.total > 100"})
  @Order(0)
  fun skipped(value: models.ProbeReady) { log.add("skipped") }
  @OnEvent
  fun derived(value: models.ProbeDerived) { log.add("derived:" + value.tag) }
  @OnEvent({"replay":true})
  fun tagged(value: models.ProbeTagged) { tag = value.tag }
}
@Service()
@NeedsAll()
class PublishService {
  lateinit var publisher: ApplicationEventPublisher
  @OnStart()
  fun initialize() {
    publisher.publishEvent(models.ProbeReady("paid", 10), true)
    publisher.publishEvent(models.ProbeReady("free", 0))
  }
}
@Service()
@NeedsAll()
class ResourceService {
  @Needs
  lateinit var applicationEventPublisher: ApplicationEventPublisher
  @OnStart()
  fun initialize() { applicationEventPublisher.publishEvent(models.ProbeTagged("resource")) }
}
@RequestRoute("/probe")
@Public
@NeedsAll()
class ProbeController {
  lateinit var probe: ProbeService
  lateinit var publish: PublishService
  lateinit var resource: ResourceService
  @GetRoute
  fun getProbe(): Any? { return mapOf("log" to probe.log, "tag" to probe.tag) }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn kotlin_background_listeners_run_after_the_delivery_pass() {
    execute_expected(
        "kt",
        r#"
@Service()
class ProbeService {
  val log = mutableListOf<String>()
  @OnEvent
  @Order(-1)
  @InBackground()
  fun later(value: models.ProbeReady): models.ProbeDerived { log.add("later:" + value.status); return models.ProbeDerived("ignored") }
  @OnEvent
  @Order(1)
  fun first(value: models.ProbeReady) { log.add("first:" + value.status) }
  @OnEvent
  fun derived(value: models.ProbeDerived) { log.add("derived:" + value.tag) }
}
@Service()
@NeedsAll()
class PublishService {
  lateinit var publisher: ApplicationEventPublisher
  @OnStart()
  fun initialize() {
    publisher.publishEvent(models.ProbeReady("paid", 10))
    publisher.publishEvent(models.ProbeReady("free", 0))
  }
}
@RequestRoute("/probe")
@Public
@NeedsAll()
class ProbeController {
  lateinit var probe: ProbeService
  lateinit var publish: PublishService
  @GetRoute
  fun getProbe(): Any? { return mapOf("log" to probe.log) }
}
"#,
        serde_json::json!({"log": ["first:paid", "later:paid", "first:free", "later:free"]}),
    )
    .await;
}

#[tokio::test]
async fn csharp_events_and_lifecycle() {
    execute(
        "cs",
        r#"
[Service()]
class ProbeService {
  public System.Collections.Generic.List<string> Log = new System.Collections.Generic.List<string>();
  public string Tag = "";
  [OnEvent]
  [Order(2)]
  public void Wide(Models.ProbeReady value) { Log.Add("wide:" + value.Status); }
  [OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})]
  [Order(1)]
  public Models.ProbeDerived Paid(Models.ProbeReady value) { Log.Add("paid"); return new Models.ProbeDerived("one"); }
  [OnEvent({"condition":"value.total > 100"})]
  [Order(0)]
  public void Skipped(Models.ProbeReady value) { Log.Add("skipped"); }
  [OnEvent]
  public void Derived(Models.ProbeDerived value) { Log.Add("derived:" + value.Tag); }
  [OnEvent({"replay":true})]
  public void Tagged(Models.ProbeTagged value) { Tag = value.Tag; }
}
[Service()]
[NeedsAll()]
class PublishService {
  readonly ApplicationEventPublisher _publisher;
  [OnStart()]
  public void Initialize() {
    _publisher.PublishEvent(new Models.ProbeReady("paid", 10), true);
    _publisher.publishEvent(new Models.ProbeReady("free", 0));
  }
}
[Service()]
[NeedsAll()]
class ResourceService {
  [Needs]
  readonly ApplicationEventPublisher _applicationEventPublisher;
  [OnStart()]
  public void Initialize() {
    _applicationEventPublisher.PublishEvent(new Models.ProbeTagged("resource"));
    _applicationEventPublisher.PublishEvent(new Models.ProbeTagged(null));
  }
}
[RequestRoute("/probe")]
[Public]
[NeedsAll()]
class ProbeController {
  readonly ProbeService _probe;
  readonly PublishService _publish;
  readonly ResourceService _resource;
  [GetRoute]
  public object GetProbe() { return new { log = _probe.Log, tag = _probe.Tag }; }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn csharp_background_listeners_run_after_the_others() {
    execute_expected(
        "cs",
        r#"
[Service()]
[NeedsAll()]
class ProbeService {
  readonly ApplicationEventPublisher _publisher;
  public System.Collections.Generic.List<string> Log = new System.Collections.Generic.List<string>();
  public string Tag = "";
  [OnEvent]
  [InBackground()]
  [Order(-1)]
  public void Deferred(Models.ProbeReady value) { Log.Add("deferred:" + value.Status); }
  [OnEvent]
  public void Now(Models.ProbeReady value) { Log.Add("now:" + value.Status); }
  [OnEvent]
  [InBackground()]
  public async System.Threading.Tasks.Task Later(Models.ProbeReady value) {
    await System.Threading.Tasks.Task.Yield();
    Log.Add("later:" + value.Status);
    _publisher.PublishEvent(new Models.ProbeTagged("background"));
  }
  [OnEvent]
  public void Tagged(Models.ProbeTagged value) { Tag = value.Tag; }
  [OnStart()]
  public void Initialize() { _publisher.PublishEvent(new Models.ProbeReady("paid", 10)); }
}
[RequestRoute("/probe")]
[Public]
[NeedsAll()]
class ProbeController {
  readonly ProbeService _probe;
  [GetRoute]
  public object GetProbe() { return new { log = _probe.Log, tag = _probe.Tag }; }
}
"#,
        serde_json::json!({
            "log": ["now:paid", "deferred:paid", "later:paid"],
            "tag": "background",
        }),
    )
    .await;
}

#[test]
fn csharp_asynchronous_listeners_need_in_background_and_a_task() {
    let cases = [
        (
            "[OnEvent]\n  public async System.Threading.Tasks.Task Observe(Models.ProbeTagged value) { await System.Threading.Tasks.Task.Yield(); }",
            "This Yon language supports synchronous local event and lifecycle completion only",
        ),
        (
            "[OnEvent]\n  [InBackground()]\n  public async void Observe(Models.ProbeTagged value) { await System.Threading.Tasks.Task.Yield(); }",
            "An asynchronous C# InBackground listener must return Task or Task<T>",
        ),
    ];
    for (listener, expected) in cases {
        let source = format!(
            "[Service()]\nclass ProbeService {{\n  {listener}\n}}\n[RequestRoute(\"/probe\")]\n[Public]\n[NeedsAll()]\nclass ProbeController {{\n  readonly ProbeService _probe;\n  [GetRoute]\n  public object GetProbe() {{ return new {{ ok = true }}; }}\n}}\n"
        );
        let root = tempfile::tempdir().expect("fixture");
        ProbeModels::write(root.path(), "cs");
        let entry = yon_layout::write_fixture(
            root.path(),
            "server/controllers/ProbeController.cs",
            &source,
        );
        let failure = HandlerSource::discover(root.path(), &entry)
            .err()
            .unwrap_or_else(|| panic!("C# accepted {listener}"))
            .to_string();
        assert!(failure.contains(expected), "{failure}");
    }
}

#[tokio::test]
async fn rust_events_and_lifecycle() {
    execute(
        "rs",
        r#"
#[Service()]
struct ProbeService { log: std::sync::Mutex<Vec<String>>, tag: std::sync::Mutex<String> }
impl ProbeService {
  fn new() -> Self { Self { log: std::sync::Mutex::new(Vec::new()), tag: std::sync::Mutex::new(String::new()) } }
  #[OnEvent]
  #[Order(2)]
  fn wide(&self, value: crate::models::ProbeReady) { self.log.lock().unwrap().push(format!("wide:{}", value.status)); }
  #[OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})]
  #[Order(1)]
  fn paid(&self, value: crate::models::ProbeReady) -> crate::models::ProbeDerived {
    let _ = value;
    self.log.lock().unwrap().push(String::from("paid"));
    crate::models::ProbeDerived { tag: String::from("one") }
  }
  #[OnEvent({"condition":"value.total > 100"})]
  #[Order(0)]
  fn skipped(&self, value: crate::models::ProbeReady) { let _ = value; self.log.lock().unwrap().push(String::from("skipped")); }
  #[OnEvent]
  fn derived(&self, value: crate::models::ProbeDerived) { self.log.lock().unwrap().push(format!("derived:{}", value.tag)); }
  #[OnEvent({"replay":true})]
  fn tagged(&self, value: crate::models::ProbeTagged) { *self.tag.lock().unwrap() = value.tag; }
}
#[Service()]
#[NeedsAll()]
struct PublishService { publisher: std::sync::Arc<ApplicationEventPublisher> }
impl PublishService {
  #[OnStart()]
  fn initialize(&self) {
    self.publisher.publish_event(crate::models::ProbeReady { status: String::from("paid"), total: 10 }, true);
    self.publisher.publish_event(crate::models::ProbeReady { status: String::from("free"), total: 0 }, false);
  }
}
#[Service()]
#[NeedsAll()]
struct ResourceService {
  #[Needs]
  application_event_publisher: std::sync::Arc<ApplicationEventPublisher>,
}
impl ResourceService {
  #[OnStart()]
  fn initialize(&self) { self.application_event_publisher.publish_event(crate::models::ProbeTagged { tag: String::from("resource") }, false); }
}
#[RequestRoute("/probe")]
#[Public]
#[NeedsAll()]
struct ProbeController { probe: std::sync::Arc<ProbeService>, publish: std::sync::Arc<PublishService>, resource: std::sync::Arc<ResourceService> }
impl ProbeController {
  #[GetRoute]
  fn get_probe(&self) -> Json {
    Json::map([
      ("log", Json::list(self.probe.log.lock().unwrap().clone())),
      ("tag", Json::from(self.probe.tag.lock().unwrap().clone())),
    ])
  }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn go_events_and_lifecycle() {
    execute(
        "go",
        r#"
// @Service()
type ProbeService struct {
	log []string
	tag string
}

// @OnEvent
// @Order(2)
func (probe *ProbeService) Wide(value ProbeReady) {
	probe.log = append(probe.log, "wide:"+value.Status)
}

// @OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})
// @Order(1)
func (probe *ProbeService) Paid(value ProbeReady) ProbeDerived {
	probe.log = append(probe.log, "paid")
	return ProbeDerived{Tag: "one"}
}

// @OnEvent({"condition":"value.total > 100"})
// @Order(0)
func (probe *ProbeService) Skipped(value ProbeReady) {
	probe.log = append(probe.log, "skipped")
}

// @OnEvent
func (probe *ProbeService) Derived(value ProbeDerived) error {
	probe.log = append(probe.log, "derived:"+value.Tag)
	return nil
}

// @OnEvent({"replay":true})
func (probe *ProbeService) Tagged(value ProbeTagged) {
	probe.tag = value.Tag
}
// @Service()
// @NeedsAll()
type PublishService struct {
	publisher *ApplicationEventPublisher
}

// @OnStart()
func (service *PublishService) Initialize() {
	service.publisher.PublishEvent(ProbeReady{Status: "paid", Total: 10}, true)
	service.publisher.PublishEvent(ProbeReady{Status: "free", Total: 0}, false)
}
// @Service()
// @NeedsAll()
type ResourceService struct {
	// @Needs
	applicationEventPublisher *ApplicationEventPublisher
}

// @OnStart()
func (service *ResourceService) Initialize() error {
	service.applicationEventPublisher.PublishEvent(ProbeTagged{Tag: "resource"}, false)
	return nil
}

// @OnStop()
func (service *ResourceService) Close() {
	service.applicationEventPublisher.PublishEvent(ProbeTagged{Tag: "closed"}, false)
}
// @RequestRoute("/probe")
// @Public
// @NeedsAll()
type ProbeController struct {
	probe    *ProbeService
	publish  *PublishService
	resource *ResourceService
}

// @GetRoute
func (controller *ProbeController) GetProbe() any {
	return map[string]any{"log": controller.probe.log, "tag": controller.probe.tag}
}
"#,
    )
    .await;
}

struct ListenerFixtures;

impl ListenerFixtures {
    const SYNCHRONOUS: &[(&str, &str)] = &[
        (
            "java",
            r#"
@Service()
class ProbeService {
  @OnEvent
  @InBackground()
  void observe(models.ProbeTagged value) { }
}
@RequestRoute("/probe")
@Public
@NeedsAll()
class ProbeController {
  ProbeService probe;
  @GetRoute
  Object getProbe() { return java.util.Map.of("ok", true); }
}
"#,
        ),
        (
            "rs",
            r#"
#[Service()]
struct ProbeService {}
impl ProbeService {
  fn new() -> Self { Self {} }
  #[OnEvent]
  #[InBackground()]
  fn observe(&self, value: crate::models::ProbeTagged) { let _ = value; }
}
#[RequestRoute("/probe")]
#[Public]
#[NeedsAll()]
struct ProbeController { probe: std::sync::Arc<ProbeService> }
impl ProbeController {
  #[GetRoute]
  fn get_probe(&self) -> Json { Json::map([("ok", Json::from(true))]) }
}
"#,
        ),
        (
            "go",
            r#"
// @Service()
type ProbeService struct {
	count int
}

// @OnEvent
// @InBackground()
func (probe *ProbeService) Observe(value ProbeTagged) {}
// @RequestRoute("/probe")
// @Public
// @NeedsAll()
type ProbeController struct {
	probe *ProbeService
}

// @GetRoute
func (controller *ProbeController) GetProbe() any { return map[string]any{"ok": true} }
"#,
        ),
    ];
}

#[tokio::test]
async fn synchronous_languages_refuse_async_listeners() {
    for &(extension, source) in ListenerFixtures::SYNCHRONOUS {
        let root = tempfile::tempdir().expect("fixture");
        ProbeModels::write(root.path(), extension);
        let entry = format!("server/controllers/ProbeController.{extension}");
        let entry = yon_layout::write_fixture(root.path(), &entry, source);
        let failure = HandlerSource::discover(root.path(), &entry)
            .err()
            .unwrap_or_else(|| panic!("{extension} accepted an InBackground listener"))
            .to_string();
        assert!(
            failure.contains("InBackground listeners require a Yon language that defers them"),
            "{extension}: {failure}"
        );
    }
}

#[tokio::test]
async fn dart_events_and_lifecycle() {
    execute(
        "dart",
        r#"
import 'models/ProbeReady.dart';
import 'models/ProbeDerived.dart';
import 'models/ProbeTagged.dart';
@Service()
class ProbeService {
  final List<String> log = [];
  String tag = '';

  @OnEvent
  @Order(2)
  void wide(ProbeReady value) {
    log.add('wide:${value.status}');
  }

  @OnEvent({"condition":"value.status == 'paid' && value.total >= 10"})
  @Order(1)
  ProbeDerived paid(ProbeReady value) {
    log.add('paid');
    return const ProbeDerived('one');
  }

  @OnEvent({"condition":"value.total > 100"})
  @Order(0)
  void skipped(ProbeReady value) {
    log.add('skipped');
  }

  @OnEvent
  void derived(ProbeDerived value) {
    log.add('derived:${value.tag}');
  }

  @OnEvent({"replay":true})
  void tagged(ProbeTagged value) {
    tag = value.tag;
  }
}
import 'models/ProbeReady.dart';
@Service()
class PublishService {
  @Needs()
  final ApplicationEventPublisher publisher;

  @OnStart()
  void initialize() {
    publisher.publishEvent(const ProbeReady('paid', 10), true);
    publisher.publishEvent(const ProbeReady('free', 0));
  }
}
import 'models/ProbeTagged.dart';
@Service()
@NeedsAll()
class ResourceService {
  @Needs
  late ApplicationEventPublisher applicationEventPublisher;

  @OnStart()
  void initialize() {
    applicationEventPublisher.publishEvent(const ProbeTagged('resource'));
  }
}
@RequestRoute("/probe")
@Public
class ProbeController {
  @Needs()
  final ProbeService probe;
  @Needs()
  final PublishService publish;
  @Needs()
  final ResourceService resource;

  // An async verb: the container awaits the Future before draining events.
  @GetRoute
  Future<Object?> getProbe() async {
    return {'log': probe.log, 'tag': probe.tag};
  }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn dart_background_listeners_and_async_hooks() {
    execute_expected(
        "dart",
        r#"
import 'models/ProbeReady.dart';
import 'models/ProbeDerived.dart';
@Service()
class ProbeService {
  final List<String> log = [];
  String tag = '';

  @OnStart()
  Future<void> initialize() async {
    await Future<void>.delayed(Duration.zero);
    log.add('started');
  }

  @OnStop()
  Future<void> stop() async {
    await Future<void>.delayed(Duration.zero);
  }

  // Runs after the other listeners of the event and never republishes.
  @InBackground
  @OnEvent
  @Order(-5)
  Future<ProbeDerived> later(ProbeReady value) async {
    await Future<void>.delayed(Duration.zero);
    log.add('later:${value.status}');
    return const ProbeDerived('never');
  }

  @OnEvent
  void now(ProbeReady value) {
    log.add('now:${value.status}');
  }

  @OnEvent
  void derived(ProbeDerived value) {
    log.add('derived:${value.tag}');
  }
}
import 'models/ProbeReady.dart';
@Service()
class PublishService {
  @Needs()
  final ApplicationEventPublisher publisher;

  @OnStart()
  void initialize() {
    publisher.publishEvent(const ProbeReady('paid', 10));
  }
}
@RequestRoute("/probe")
@Public
class ProbeController {
  @Needs()
  final ProbeService probe;
  @Needs()
  final PublishService publish;

  @GetRoute
  Object? getProbe() {
    return {'log': probe.log, 'tag': probe.tag};
  }
}
"#,
        serde_json::json!({"log": ["started", "now:paid", "later:paid"], "tag": ""}),
    )
    .await;
}
