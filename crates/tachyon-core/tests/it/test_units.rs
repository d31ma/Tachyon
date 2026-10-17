#![allow(clippy::expect_used)]
use tachyon_contracts::{HandlerRequest, HttpMethod};
use tachyon_core::{
    HandlerCancellation, HandlerSource, HandlerSupervisor, HandlerSupervisorOptions,
};

fn bun_available() -> bool {
    std::process::Command::new("bun")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn tool_available(program: &str) -> bool {
    let version_flag = match program {
        "kotlinc" => "-version",
        "go" => "version",
        "lua" => "-v",
        _ => "--version",
    };
    std::process::Command::new(program)
        .arg(version_flag)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn javascript_supervisor(extension: &str) -> HandlerSupervisor {
    let mut options = HandlerSupervisorOptions::default();
    if extension == "ts" {
        options.runtimes.javascript = "bun".into();
    }
    HandlerSupervisor::new(options).expect("supervisor")
}

fn write(root: &std::path::Path, relative: &str, source: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
    std::fs::write(path, source).expect("source");
}

#[tokio::test]
async fn a_javascript_test_unit_runs_its_cases_through_the_supervisor() {
    for extension in ["js", "ts"] {
        run_javascript_test_unit(extension).await;
    }
}

async fn run_javascript_test_unit(extension: &str) {
    if extension == "ts" && !bun_available() {
        eprintln!("skipping the TypeScript unit: bun is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        &format!("server/tests/CatalogTestSetup.{extension}"),
        "@TestSetup()\nclass CatalogTestSetup {\n  static configure() { return {}; }\n}\n",
    );
    write(
        root.path(),
        &format!("server/helpers/CatalogCounterHelper.{extension}"),
        "@Helper()\nclass CatalogCounterHelper {\n  count() { return 21; }\n}\n",
    );
    write(
        root.path(),
        &format!("server/services/CatalogService.{extension}"),
        &javascript_unit_source(
            "@Service()\nclass CatalogService {\n  @Needs()\n  catalogCounterHelper;\n  total() { return this.catalogCounterHelper.count() * 2; }\n}\n",
            extension,
        ),
    );
    write(
        root.path(),
        &format!("server/tests/CatalogServiceTest.{extension}"),
        &javascript_unit_source(
            "@Test(CatalogTestSetup, {\"partition\":\"tests\"})\n@UseProfiles(\"test\")\nexport default class CatalogServiceTest {\n  @Fake()\n  catalogCounterHelper;\n\n  async testDoublesTheCount() {\n    this.catalogCounterHelper = { count: () => 4 };\n    const service = this.construct(CatalogService);\n    Assert.equals(8, service.total());\n  }\n\n  async testReportsAFailure() {\n    Assert.equals(undefined, this.catalogCounterHelper, 'the fake is empty again');\n    Assert.isTrue(false, 'this case is meant to fail');\n  }\n}\n",
            extension,
        ),
    );

    let relative = format!("server/tests/CatalogServiceTest.{extension}");
    let source = HandlerSource::discover(root.path(), &relative).expect("test unit discovery");
    let supervisor = javascript_supervisor(extension);
    let response = supervisor
        .invoke(
            &source,
            &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
            &HandlerCancellation::default(),
        )
        .await
        .expect("invocation");
    assert_javascript_report(response);
}

fn javascript_unit_source(source: &str, extension: &str) -> String {
    if extension == "ts" {
        return source.replace(
            "catalogCounterHelper;",
            "catalogCounterHelper: CatalogCounterHelper;",
        );
    }
    source.to_owned()
}

#[tokio::test]
async fn a_python_test_unit_runs_its_cases_through_the_supervisor() {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: python3 is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        "server/tests/CatalogTestSetup.py",
        "@TestSetup()\nclass CatalogTestSetup:\n    @staticmethod\n    def configure(execution):\n        return {}\n",
    );
    write(
        root.path(),
        "server/tests/CatalogServiceTest.py",
        "from typing import Annotated\n\n@Test(CatalogTestSetup, {\"partition\":\"tests\"})\n@UseProfiles(\"test\")\nclass CatalogServiceTest:\n    catalog_counter_helper: Annotated[object, fake]\n\n    async def test_opens_a_context(self):\n        Assert.is_true(self.behaviors is not None, 'the case has a context')\n        Assert.equals(None, self.catalog_counter_helper, 'a fake starts empty')\n        self.catalog_counter_helper = object()\n\n    async def test_reports_a_failure(self):\n        Assert.equals(None, self.catalog_counter_helper, 'the fake is empty again')\n        Assert.is_true(False, 'this case is meant to fail')\n",
    );

    let relative = "server/tests/CatalogServiceTest.py";
    let source = HandlerSource::discover(root.path(), relative).expect("test unit discovery");
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    let response = supervisor
        .invoke(
            &source,
            &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
            &HandlerCancellation::default(),
        )
        .await
        .expect("invocation");
    assert_python_report(response);
}

#[tokio::test]
async fn a_php_test_unit_runs_its_cases_through_the_supervisor() {
    if std::process::Command::new("php-cgi")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: php-cgi is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        "server/tests/CatalogTestSetup.php",
        "<?php\n#[TestSetup]\nclass CatalogTestSetup {\n    public static function configure($execution) { return []; }\n}\n",
    );
    write(
        root.path(),
        "server/tests/CatalogServiceTest.php",
        "<?php\n#[Test(CatalogTestSetup::class, {\"partition\":\"tests\"})]\n#[UseProfiles(\"test\")]\nclass CatalogServiceTest {\n    #[Fake]\n    public CatalogCounterHelper $catalogCounterHelper;\n    #[Fake]\n    public $note;\n\n    public function testOpensAContext(): void {\n        Assert::isTrue($this->behaviors !== null, 'the case has a context');\n        Assert::isTrue(!isset($this->catalogCounterHelper), 'a fake starts empty');\n        $this->note = 'set';\n        $this->catalogCounterHelper = new class extends CatalogCounterHelper {\n            public function count(): int { return 4; }\n        };\n        Assert::equals(8, (new CatalogService($this->catalogCounterHelper))->total(), 'the total');\n    }\n    public function testReportsAFailure(): void {\n        Assert::isTrue(!isset($this->catalogCounterHelper), 'the fake is empty again');\n        Assert::equals(null, $this->note, 'an untyped fake is null again');\n        Assert::isTrue(false, 'this case is meant to fail');\n    }\n}\n",
    );
    write(
        root.path(),
        "server/helpers/CatalogCounterHelper.php",
        "<?php\n#[Helper]\nclass CatalogCounterHelper {\n    public function count(): int { return 21; }\n}\n",
    );
    write(
        root.path(),
        "server/services/CatalogService.php",
        "<?php\n#[Service]\nclass CatalogService {\n    #[Needs]\n    public CatalogCounterHelper $catalogCounterHelper;\n\n    public function total(): int { return $this->catalogCounterHelper->count() * 2; }\n}\n",
    );

    let source = HandlerSource::discover(root.path(), "server/tests/CatalogServiceTest.php")
        .expect("test unit discovery");
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    let response = supervisor
        .invoke(
            &source,
            &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
            &HandlerCancellation::default(),
        )
        .await
        .expect("invocation");
    assert!(response.error.is_none(), "{:?}", response.error);
    let body = response.body.expect("body").data;
    let value: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
    let cases = value["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 2, "{body}");
    assert_eq!(cases[0]["passed"], true, "{body}");
    assert_eq!(cases[1]["passed"], false, "{body}");
    assert!(
        cases[1]["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("this case is meant to fail")),
        "{body}"
    );
}

#[tokio::test]
async fn a_java_test_unit_runs_its_cases_through_the_supervisor() {
    if std::process::Command::new("javac")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("skipping: javac is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        "server/tests/CatalogTestSetup.java",
        "@TestSetup\nclass CatalogTestSetup {\n    static Behaviors.Configuration configure(Behaviors.Execution execution) {\n        return new Behaviors.Configuration(java.util.Map.of(), java.util.Map.of(), java.util.Map.of(), null, null);\n    }\n}\n",
    );
    write(
        root.path(),
        "server/tests/CatalogServiceTest.java",
        "@Test(CatalogTestSetup.class, {\"partition\":\"tests\"})\n@UseProfiles(\"test\")\nclass CatalogServiceTest {\n    @Fake\n    CatalogCounterHelper catalogCounterHelper;\n    @Fake\n    long count;\n    @Fake\n    boolean seen;\n\n    void testOpensAContext() {\n        Assert.isTrue(this.behaviors != null, \"the case has a context\");\n        Assert.isTrue(catalogCounterHelper == null, \"a fake starts empty\");\n        count = 3;\n        seen = true;\n        catalogCounterHelper = new CatalogCounterHelper() {\n            int count() { return 4; }\n        };\n        Assert.equals(8, construct(CatalogService.class).total(), \"the total\");\n    }\n    void testReportsAFailure() {\n        Assert.isTrue(catalogCounterHelper == null, \"the fake is empty again\");\n        Assert.equals(0L, count, \"a primitive fake is zero again\");\n        Assert.isTrue(!seen, \"a boolean fake is false again\");\n        Assert.isTrue(false, \"this case is meant to fail\");\n    }\n}\n",
    );
    write(
        root.path(),
        "server/helpers/CatalogCounterHelper.java",
        "@Helper\nclass CatalogCounterHelper {\n    int count() { return 21; }\n}\n",
    );
    write(
        root.path(),
        "server/services/CatalogService.java",
        "@Service\nclass CatalogService {\n    @Needs\n    private final CatalogCounterHelper catalogCounterHelper;\n\n    int total() { return catalogCounterHelper.count() * 2; }\n}\n",
    );

    let source = HandlerSource::discover(root.path(), "server/tests/CatalogServiceTest.java")
        .expect("test unit discovery");
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    let response = supervisor
        .invoke(
            &source,
            &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
            &HandlerCancellation::default(),
        )
        .await
        .expect("invocation");
    assert_java_report(response);
}

#[tokio::test]
async fn every_compiled_language_runs_its_test_unit() {
    for (extension, probe, configuration, unit) in [
        (
            "groovy",
            "groovyc",
            "@TestSetup\nclass CatalogTestSetup {\n    static Behaviors.Configuration configure(Behaviors.Execution execution) {\n        return new Behaviors.Configuration([:], [:], [:], null, null)\n    }\n}\n",
            "@Test(CatalogTestSetup.class, {\"partition\":\"tests\"})\nclass CatalogServiceTest {\n    void testOpensAContext() {\n        Assert.isTrue(this.behaviors != null, 'the case has a context')\n    }\n\n    void testReportsAFailure() {\n        Assert.isTrue(false, 'this case is meant to fail')\n    }\n}\n",
        ),
        (
            "kt",
            "kotlinc",
            "@TestSetup\nclass CatalogTestSetup {\n    companion object {\n        fun configure(execution: Behaviors.Execution): Behaviors.Configuration = Behaviors.Configuration()\n    }\n}\n",
            "@Test(CatalogTestSetup::class, {\"partition\":\"tests\"})\nclass CatalogServiceTest {\n    fun testOpensAContext() {\n        Assert.isTrue(behaviors != null, \"the case has a context\")\n    }\n    fun testReportsAFailure() {\n        Assert.isTrue(false, \"this case is meant to fail\")\n    }\n}\n",
        ),
        (
            "cs",
            "dotnet",
            "[TestSetup]\npublic sealed class CatalogTestSetup {\n    public static BehaviorAdapters configure(BehaviorExecution execution) { return new BehaviorAdapters(); }\n}\n",
            "[Test(typeof(CatalogTestSetup), {\"partition\":\"tests\"})]\nsealed class CatalogServiceTest {\n    void testOpensAContext() {\n        Assert.isTrue(this.behaviors != null, \"the case has a context\");\n    }\n    void testReportsAFailure() {\n        Assert.isTrue(false, \"this case is meant to fail\");\n    }\n}\n",
        ),
        (
            "fs",
            "dotnet",
            "[<TestSetup>]\ntype CatalogTestSetup() =\n    static member configure(execution: BehaviorExecution) : BehaviorAdapters =\n        BehaviorAdapters()\n",
            "[<Test(typeof<CatalogTestSetup>, {\"partition\":\"tests\"})>]\ntype CatalogServiceTest() =\n    member this.testOpensAContext() : unit =\n        Assert.isTrue(not (isNull this.behaviors), \"the case has a context\")\n\n    member this.testReportsAFailure() : unit =\n        Assert.isTrue(false, \"this case is meant to fail\")\n",
        ),
        (
            "dart",
            "dart",
            "@TestSetup\nclass CatalogTestSetup {\n  static BehaviorAdapters configure(BehaviorExecution execution) {\n    return BehaviorAdapters();\n  }\n}\n",
            "@Test(CatalogTestSetup, {\"partition\":\"tests\"})\nclass CatalogServiceTest {\n  Future<void> testOpensAContext() async {\n    Assert.isTrue(behaviors != null, 'the case has a context');\n  }\n  Future<void> testReportsAFailure() async {\n    Assert.isTrue(false, 'this case is meant to fail');\n  }\n}\n",
        ),
        (
            "rs",
            "rustc",
            "#[TestSetup()]\nstruct CatalogTestSetup {}\nimpl CatalogTestSetup {\n    fn configure(_execution: &BehaviorExecution) -> BehaviorResult<BehaviorConfiguration> { Ok(BehaviorConfiguration::default()) }\n}\n",
            "#[Test(CatalogTestSetup, {\"partition\":\"tests\"})]\nstruct CatalogServiceTest { behaviors: std::cell::RefCell<Option<BehaviorContext>> }\nimpl CatalogServiceTest {\n    fn new() -> Self { Self { behaviors: std::cell::RefCell::new(None) } }\n    fn testOpensAContext(&self) {\n        Assert::is_true_with_message(self.behaviors.borrow().is_some(), \"the case has a context\");\n    }\n    fn testReportsAFailure(&self) {\n        Assert::is_true_with_message(false, \"this case is meant to fail\");\n    }\n}\n",
        ),
        (
            "go",
            "go",
            "// @TestSetup\ntype CatalogTestSetup struct{}\n\nfunc (CatalogTestSetup) Configure(execution *BehaviorExecution) (*BehaviorConfiguration, error) {\n\treturn NewBehaviorConfiguration(), nil\n}\n",
            "// @Test(CatalogTestSetup, {\"partition\":\"tests\"})\ntype CatalogServiceTest struct {\n\tbehaviors *BehaviorContext\n}\n\nfunc (test *CatalogServiceTest) TestOpensAContext() {\n\tAssert.IsTrue(test.behaviors != nil, \"the case has a context\")\n}\n\nfunc (test *CatalogServiceTest) TestReportsAFailure() {\n\tAssert.IsTrue(false, \"this case is meant to fail\")\n}\n",
        ),
    ] {
        if !tool_available(probe) {
            eprintln!("skipping {extension}: {probe} is unavailable");
            continue;
        }
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            &format!("server/tests/CatalogTestSetup.{extension}"),
            configuration,
        );
        let relative = format!("server/tests/CatalogServiceTest.{extension}");
        write(root.path(), &relative, unit);
        let source = HandlerSource::discover(root.path(), &relative)
            .unwrap_or_else(|error| panic!("{extension} discovery: {error:?}"));
        let supervisor =
            HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
        let response = supervisor
            .invoke(
                &source,
                &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
                &HandlerCancellation::default(),
            )
            .await
            .unwrap_or_else(|error| panic!("{extension} invocation: {error:?}"));
        assert!(
            response.error.is_none(),
            "{extension}: {:?}",
            response.error
        );
        let body = response.body.expect("body").data;
        let value: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
        let cases = value["cases"].as_array().expect("cases");
        assert_eq!(cases.len(), 2, "{extension}: {body}");
        assert_eq!(cases[0]["passed"], true, "{extension}: {body}");
        assert_eq!(cases[1]["passed"], false, "{extension}: {body}");
    }
}

#[tokio::test]
async fn every_typed_language_clears_its_fakes_before_each_case() {
    for (extension, probe, configuration, unit) in [
        (
            "go",
            "go",
            "// @TestSetup\ntype CatalogTestSetup struct{}\n\nfunc (CatalogTestSetup) Configure(execution *BehaviorExecution) (*BehaviorConfiguration, error) {\n\treturn NewBehaviorConfiguration(), nil\n}\n",
            "// @Test(CatalogTestSetup, {\"partition\":\"tests\"})\ntype CatalogServiceTest struct {\n\tbehaviors *BehaviorContext\n\t// @Fake\n\tnames []string\n\t// @Fake\n\tcount int\n\t// @Fake\n\tseen bool\n}\n\nfunc (test *CatalogServiceTest) TestStartsWithEmptyFakes() {\n\tAssert.IsTrue(test.names == nil, \"the slice fake is nil\")\n\tAssert.Equals(0, test.count, \"the number fake is zero\")\n\tAssert.IsTrue(!test.seen, \"the boolean fake is false\")\n\ttest.names = []string{\"assigned\"}\n\ttest.count = 3\n\ttest.seen = true\n}\n\nfunc (test *CatalogServiceTest) TestStartsWithEmptyFakesAgain() {\n\tAssert.IsTrue(test.names == nil, \"the slice fake is nil again\")\n\tAssert.Equals(0, test.count, \"the number fake is zero again\")\n\tAssert.IsTrue(!test.seen, \"the boolean fake is false again\")\n\ttest.names = []string{\"assigned\"}\n\ttest.count = 3\n\ttest.seen = true\n}\n",
        ),
        (
            "rs",
            "rustc",
            "#[TestSetup()]\nstruct CatalogTestSetup {}\nimpl CatalogTestSetup {\n    fn configure(_execution: &BehaviorExecution) -> BehaviorResult<BehaviorConfiguration> { Ok(BehaviorConfiguration::default()) }\n}\n",
            "#[Test(CatalogTestSetup, {\"partition\":\"tests\"})]\nstruct CatalogServiceTest {\n    behaviors: std::cell::RefCell<Option<BehaviorContext>>,\n    #[Fake]\n    label: std::cell::RefCell<Option<String>>,\n    #[Fake]\n    count: std::cell::Cell<i64>,\n    #[Fake]\n    seen: std::cell::Cell<bool>,\n}\nimpl CatalogServiceTest {\n    fn new() -> Self {\n        Self {\n            behaviors: std::cell::RefCell::new(None),\n            label: std::cell::RefCell::new(None),\n            count: std::cell::Cell::new(0),\n            seen: std::cell::Cell::new(false),\n        }\n    }\n    fn testStartsWithEmptyFakes(&self) {\n        Assert::is_true_with_message(self.label.borrow().is_none(), \"the text fake is empty\");\n        Assert::is_true_with_message(self.count.get() == 0, \"the number fake is zero\");\n        Assert::is_true_with_message(!self.seen.get(), \"the boolean fake is false\");\n        *self.label.borrow_mut() = Some(String::from(\"assigned\"));\n        self.count.set(3);\n        self.seen.set(true);\n    }\n    fn testStartsWithEmptyFakesAgain(&self) {\n        Assert::is_true_with_message(self.label.borrow().is_none(), \"the text fake is empty again\");\n        Assert::is_true_with_message(self.count.get() == 0, \"the number fake is zero again\");\n        Assert::is_true_with_message(!self.seen.get(), \"the boolean fake is false again\");\n        *self.label.borrow_mut() = Some(String::from(\"assigned\"));\n        self.count.set(3);\n        self.seen.set(true);\n    }\n}\n",
        ),
    ] {
        if !tool_available(probe) {
            assert_ne!(
                std::env::var("TAC_TEST_UNITS_REQUIRE_TOOLCHAINS").as_deref(),
                Ok("1"),
                "Required test unit toolchain {probe} is unavailable"
            );
            eprintln!("skipping {extension}: {probe} is unavailable");
            continue;
        }
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            &format!("server/tests/CatalogTestSetup.{extension}"),
            configuration,
        );
        let relative = format!("server/tests/CatalogServiceTest.{extension}");
        write(root.path(), &relative, unit);
        let source = HandlerSource::discover(root.path(), &relative)
            .unwrap_or_else(|error| panic!("{extension} discovery: {error:?}"));
        let supervisor =
            HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
        let response = supervisor
            .invoke(
                &source,
                &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
                &HandlerCancellation::default(),
            )
            .await
            .unwrap_or_else(|error| panic!("{extension} invocation: {error:?}"));
        assert!(
            response.error.is_none(),
            "{extension}: {:?}",
            response.error
        );
        let body = response.body.expect("body").data;
        let value: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
        let cases = value["cases"].as_array().expect("cases");
        assert_eq!(cases.len(), 2, "{extension}: {body}");
        assert_eq!(cases[0]["passed"], true, "{extension}: {body}");
        assert_eq!(cases[1]["passed"], true, "{extension}: {body}");
    }
}

#[tokio::test]
async fn a_tac_test_unit_runs_in_its_own_realm() {
    for (extension, probe, configuration, service, unit) in [
        (
            "ts",
            "node",
            "@TestSetup()\nclass CatalogTestSetup {\n  static configure() { return {}; }\n}\n",
            None,
            "@Test(CatalogTestSetup)\nexport default class CatalogServiceTest {\n  async testHasAContext() {\n    Assert.isTrue(Boolean(this.behaviors), 'the case has a context');\n  }\n  async testReportsAFailure() {\n    Assert.isTrue(false, 'this case is meant to fail');\n  }\n}\n",
        ),
        (
            "js",
            "node",
            "@TestSetup()\nclass CatalogTestSetup {\n  static configure() { return {}; }\n}\n",
            Some(
                "@Service()\nexport default class CatalogService {\n  title = 'Catalog';\n  double(value) { return value * 2; }\n}\n",
            ),
            "@Test(CatalogTestSetup, {\"partition\":\"tests\"})\n@UseProfiles(\"test\")\nexport default class CatalogServiceTest {\n  async testUsesAClientRole() {\n    Assert.equals(8, new CatalogService().double(4));\n  }\n  async testHasAContext() {\n    Assert.isTrue(Boolean(this.behaviors), 'the case has a context');\n  }\n  async testReportsAFailure() {\n    Assert.isTrue(false, 'this case is meant to fail');\n  }\n}\n",
        ),
        (
            "kt",
            "kotlinc",
            "@TestSetup\nclass CatalogTestSetup {\n    companion object {\n        fun configure(execution: Behaviors.Execution): Behaviors.Configuration = Behaviors.Configuration()\n    }\n}\n",
            None,
            "@Test(CatalogTestSetup::class, {\"partition\":\"tests\"})\nclass CatalogServiceTest {\n    fun testHasAContext() {\n        Assert.isTrue(behaviors != null, \"the case has a context\")\n    }\n    fun testReportsAFailure() {\n        Assert.isTrue(false, \"this case is meant to fail\")\n    }\n}\n",
        ),
        (
            "cs",
            "dotnet",
            "[TestSetup]\npublic sealed class CatalogTestSetup {\n    public static BehaviorAdapters configure(BehaviorExecution execution) { return new BehaviorAdapters(); }\n}\n",
            None,
            "[Test(typeof(CatalogTestSetup), {\"partition\":\"tests\"})]\nsealed class CatalogServiceTest {\n    void testHasAContext() {\n        Assert.isTrue(this.behaviors != null, \"the case has a context\");\n    }\n    void testReportsAFailure() {\n        Assert.isTrue(false, \"this case is meant to fail\");\n    }\n}\n",
        ),
        (
            "fs",
            "dotnet",
            "[<TestSetup>]\ntype CatalogTestSetup() =\n    static member configure(execution: BehaviorExecution) : BehaviorAdapters =\n        BehaviorAdapters()\n",
            Some(
                "[<Service>]\ntype CatalogService() =\n    member val Title = \"Catalog\" with get, set\n\n    member this.Double(value: int) : int =\n        value * 2\n",
            ),
            "[<Test(typeof<CatalogTestSetup>, {\"partition\":\"tests\"})>]\ntype CatalogServiceTest() =\n    member this.testUsesAService() : unit =\n        Assert.equals(8, CatalogService().Double(4))\n\n    member this.testHasAContext() : unit =\n        Assert.isTrue(not (isNull this.behaviors), \"the case has a context\")\n\n    member this.testReportsAFailure() : unit =\n        Assert.isTrue(false, \"this case is meant to fail\")\n",
        ),
        (
            "rs",
            "rustc",
            "#[TestSetup()]\nstruct CatalogTestSetup {}\nimpl CatalogTestSetup {\n    fn configure(_execution: &BehaviorExecution) -> BehaviorResult<BehaviorConfiguration> { Ok(BehaviorConfiguration::default()) }\n}\n",
            None,
            "#[Test(CatalogTestSetup, {\"partition\":\"tests\"})]\nstruct CatalogServiceTest { behaviors: std::cell::RefCell<Option<BehaviorContext>> }\nimpl CatalogServiceTest {\n    fn new() -> Self { Self { behaviors: std::cell::RefCell::new(None) } }\n    fn testHasAContext(&self) {\n        Assert::is_true_with_message(self.behaviors.borrow().is_some(), \"the case has a context\");\n    }\n    fn testReportsAFailure(&self) {\n        Assert::is_true_with_message(false, \"this case is meant to fail\");\n    }\n}\n",
        ),
        (
            "dart",
            "dart",
            "@TestSetup()\nclass CatalogTestSetup {\n  static BehaviorAdapters configure(BehaviorExecution execution) {\n    return BehaviorAdapters();\n  }\n}\n",
            Some(
                "import 'models/CatalogDoubled.dart';\n@Service()\nclass CatalogService {\n  @Needs()\n  final ApplicationEventPublisher applicationEventPublisher;\n  String title = 'Catalog';\n\n  int double(int value) {\n    applicationEventPublisher.publishEvent(CatalogDoubled(value));\n    return value * 2;\n  }\n}\n",
            ),
            "@Test(CatalogTestSetup, {\"partition\":\"tests\"})\nclass CatalogServiceTest {\n  Future<void> testUsesAService() async {\n    final publisher = ApplicationEventPublisher();\n    Assert.equals(8, CatalogService(publisher).double(4));\n    Assert.equals(1, publisher.published.length, 'one publication');\n  }\n  Future<void> testHasAContext() async {\n    Assert.isTrue(behaviors != null, 'the case has a context');\n  }\n  Future<void> testReportsAFailure() async {\n    Assert.isTrue(false, 'this case is meant to fail');\n  }\n}\n",
        ),
        (
            "swift",
            "swiftc",
            "@TestSetup()\nfinal class CatalogTestSetup {\n    static func configure(_ execution: BehaviorExecution) -> BehaviorAdapters {\n        var adapters = BehaviorAdapters()\n        adapters.authorize = { _, _ in true }\n        return adapters\n    }\n}\n",
            None,
            "@Test(CatalogTestSetup, {\"partition\":\"tests\"})\n@UseProfiles(\"test\")\nfinal class CatalogServiceTest {\n    func testHasAContext() async throws {\n        try Assert.isTrue(behaviors != nil, \"the case has a context\")\n    }\n    func testReportsAFailure() async throws {\n        try Assert.isTrue(false, \"this case is meant to fail\")\n    }\n}\n",
        ),
    ] {
        assert_tac_unit(extension, probe, configuration, service, unit);
    }
}

fn assert_tac_unit(
    extension: &str,
    probe: &str,
    configuration: &str,
    service: Option<&str>,
    unit: &str,
) {
    if !tool_available(probe) {
        eprintln!("skipping {extension}: {probe} is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        &format!("client/tests/CatalogTestSetup.{extension}"),
        configuration,
    );
    if let Some(service) = service {
        write(
            root.path(),
            &format!("client/services/CatalogService.{extension}"),
            service,
        );
    }
    if extension == "dart" {
        write(
            root.path(),
            "client/models/CatalogDoubled.dart",
            "class CatalogDoubled {\n  final int value;\n\n  const CatalogDoubled(this.value);\n}\n",
        );
    }
    let relative = format!("client/tests/CatalogServiceTest.{extension}");
    write(root.path(), &relative, unit);
    let report = tachyon_core::client_tests::run(root.path(), std::path::Path::new(&relative))
        .unwrap_or_else(|error| panic!("{extension}: {error:?}"));
    assert_eq!(report.unit, "CatalogServiceTest", "{extension}");
    let failing = report
        .cases
        .iter()
        .find(|case| case.name == "testReportsAFailure")
        .unwrap_or_else(|| panic!("{extension} ran no failing case"));
    assert!(!failing.passed, "{extension}");
    assert!(
        report
            .cases
            .iter()
            .filter(|case| case.name != "testReportsAFailure")
            .all(|case| case.passed),
        "{extension}: {:?}",
        report.cases
    );
}

#[tokio::test]
async fn every_javascript_runtime_runs_a_browser_unit() {
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        "client/shared/scripts/pricing.js",
        "export const withTax = (amount) => Math.round(amount * 1.2)\n",
    );
    write(
        root.path(),
        "client/setups/ShopSetup.js",
        "@Setup\nclass ShopSetup {\n  static configure() { return {} }\n}\n",
    );
    write(
        root.path(),
        "client/services/BasketService.js",
        "import { withTax } from '/shared/scripts/pricing.js'\n@Service()\nexport default class BasketService {\n  total(amount) { return withTax(amount) }\n}\n",
    );
    write(
        root.path(),
        "client/tests/BasketServiceTest.js",
        "@Test(ShopSetup, {\"partition\":\"tests\"})\nexport default class BasketServiceTest {\n  async testTotalAddsTax() {\n    Assert.equals(12, new BasketService().total(10))\n  }\n}\n",
    );
    let mut ran = 0;
    for runtime in ["node", "bun", "deno"] {
        if std::process::Command::new(runtime)
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping {runtime}: not installed");
            continue;
        }
        let report = tachyon_core::client_tests::run_with(
            root.path(),
            std::path::Path::new("client/tests/BasketServiceTest.js"),
            Some(std::path::Path::new(runtime)),
        )
        .unwrap_or_else(|error| panic!("{runtime}: {error:?}"));
        assert!(
            report.cases.iter().all(|case| case.passed),
            "{runtime}: {:?}",
            report.cases
        );
        ran += 1;
    }
    assert!(ran > 0, "no JavaScript runtime is installed");
}

#[tokio::test]
async fn a_dart_unit_reaches_the_services_its_service_injects() {
    if std::process::Command::new("dart")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: dart is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        "client/services/BasketService.dart",
        "@Service()\nclass BasketService {\n  int count = 0;\n\n  int add(int by) => count += by;\n}\n",
    );
    write(
        root.path(),
        "client/services/CheckoutService.dart",
        "@Service()\nclass CheckoutService {\n  late BasketService basket;\n  late TacService shop;\n\n  int buy() => basket.add(2);\n\n  Future<Object?> restock() => shop.invoke('restock', [3]);\n}\n",
    );
    write(
        root.path(),
        "client/setups/ShopSetup.dart",
        "@Setup()\nclass ShopSetup {\n  static BehaviorAdapters configure(BehaviorExecution execution) {\n    return BehaviorAdapters();\n  }\n}\n",
    );
    let relative = "client/tests/CheckoutServiceTest.dart";
    write(
        root.path(),
        relative,
        "@Test(ShopSetup, {\"partition\":\"tests\"})\nclass CheckoutServiceTest {\n  Future<void> testBuyAddsToTheBasket() async {\n    final checkout = CheckoutService()..basket = BasketService();\n    checkout.buy();\n    Assert.equals(2, checkout.basket.count);\n  }\n\n  Future<void> testRestockCallsTheShop() async {\n    final checkout = CheckoutService()\n      ..shop = TacService({'stock': 1}, {'restock': (arguments) => 1 + (arguments.first as int)});\n    Assert.equals(4, await checkout.restock());\n  }\n}\n",
    );
    let report = tachyon_core::client_tests::run(root.path(), std::path::Path::new(relative))
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(
        report.cases.iter().all(|case| case.passed),
        "{:?}",
        report.cases
    );
    assert_eq!(report.cases.len(), 2);
}

fn assert_javascript_report(response: tachyon_contracts::HandlerResponse) {
    assert!(response.error.is_none(), "{:?}", response.error);
    let body = response.body.expect("body").data;
    let value: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
    assert_eq!(value["unit"], "CatalogServiceTest", "{body}");
    let cases = value["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 2, "{body}");
    let doubles = cases
        .iter()
        .find(|case| case["name"] == "testDoublesTheCount")
        .expect("the passing case");
    assert_eq!(doubles["passed"], true, "{body}");
    let failing = cases
        .iter()
        .find(|case| case["name"] == "testReportsAFailure")
        .expect("the failing case");
    assert_eq!(failing["passed"], false, "{body}");
    assert!(
        failing["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("this case is meant to fail")),
        "{body}"
    );
}

#[tokio::test]
async fn a_ruby_test_unit_runs_its_cases_through_the_supervisor() {
    if !tool_available("ruby") {
        assert!(std::env::var_os("CI").is_none(), "CI must provide ruby");
        eprintln!("skipping: ruby is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        "server/tests/CatalogTestSetup.rb",
        "#[TestSetup]\nclass CatalogTestSetup\n  def self.configure(execution)\n    {}\n  end\nend\n",
    );
    write(
        root.path(),
        "server/helpers/CatalogCounterHelper.rb",
        "#[Helper]\nclass CatalogCounterHelper\n  def count\n    21\n  end\nend\n",
    );
    write(
        root.path(),
        "server/services/CatalogService.rb",
        "#[Service]\nclass CatalogService\n  #[Needs]\n  attr_reader :catalog_counter_helper\n\n  def total\n    catalog_counter_helper.count * 2\n  end\nend\n",
    );
    write(
        root.path(),
        "server/tests/CatalogServiceTest.rb",
        "#[Test(CatalogTestSetup, {\"partition\":\"tests\"})]\n#[UseProfiles(\"test\")]\nclass CatalogServiceTest\n  #[Fake]\n  attr_accessor :catalog_counter_helper\n\n  def test_opens_a_context\n    Assert.is_true(!@behaviors.nil?, \"the case has a context\")\n    Assert.equals(nil, catalog_counter_helper, \"a fake starts empty\")\n    self.catalog_counter_helper = Struct.new(:count).new(4)\n    Assert.equals(8, construct(CatalogService).total, \"the total\")\n  end\n\n  def test_reports_a_failure\n    self.catalog_counter_helper = Struct.new(:count).new(1)\n    Assert.is_true(false, \"this case is meant to fail\")\n  end\nend\n",
    );
    let relative = "server/tests/CatalogServiceTest.rb";
    let source = HandlerSource::discover(root.path(), relative).expect("test unit discovery");
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    let response = supervisor
        .invoke(
            &source,
            &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
            &HandlerCancellation::default(),
        )
        .await
        .expect("invocation");
    assert_python_report(response);
}

#[tokio::test]
async fn a_lua_test_unit_runs_its_cases_through_the_supervisor() {
    if !tool_available("lua") {
        eprintln!("skipping: lua is unavailable");
        return;
    }
    let root = tempfile::tempdir().expect("project");
    write(
        root.path(),
        "server/tests/CatalogTestSetup.lua",
        "-- @TestSetup\nCatalogTestSetup = {}\n\nfunction CatalogTestSetup.configure(execution)\n  return {}\nend\n",
    );
    write(
        root.path(),
        "server/helpers/CatalogCounterHelper.lua",
        "-- @Helper\nCatalogCounterHelper = {}\n\nfunction CatalogCounterHelper:count()\n  return 21\nend\n",
    );
    write(
        root.path(),
        "server/services/CatalogService.lua",
        "-- @Service\nCatalogService = {\n  -- @Needs\n  catalog_counter_helper = nil,\n}\n\nfunction CatalogService:total()\n  return self.catalog_counter_helper:count() * 2\nend\n",
    );
    write(
        root.path(),
        "server/tests/CatalogServiceTest.lua",
        "-- @Test(CatalogTestSetup, {\"partition\":\"tests\"})\n-- @UseProfiles(\"test\")\nCatalogServiceTest = {\n  -- @Fake\n  catalog_counter_helper = nil,\n}\n\nfunction CatalogServiceTest:test_opens_a_context()\n  Assert.equals(nil, self.catalog_counter_helper, \"a fake starts empty\")\n  self.catalog_counter_helper = { count = function() return 4 end }\n  Assert.equals(8, self:construct(CatalogService):total(), \"the total\")\nend\n\n-- @ValueSource([1, 2])\nfunction CatalogServiceTest:test_reports_a_failure(value)\n  Assert.is_true(value > 1, \"this case is meant to fail\")\nend\n",
    );
    let relative = "server/tests/CatalogServiceTest.lua";
    let source = HandlerSource::discover(root.path(), relative).expect("test unit discovery");
    let supervisor =
        HandlerSupervisor::new(HandlerSupervisorOptions::default()).expect("supervisor");
    let response = supervisor
        .invoke(
            &source,
            &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
            &HandlerCancellation::default(),
        )
        .await
        .expect("invocation");
    assert!(response.error.is_none(), "{:?}", response.error);
    let body = response.body.expect("body").data;
    let value: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
    let cases = value["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 3, "{body}");
    assert_eq!(cases[0]["passed"], true, "{body}");
    assert_eq!(cases[1]["passed"], false, "{body}");
    assert_eq!(cases[2]["passed"], true, "{body}");
    assert!(
        cases[1]["error"]
            .as_str()
            .is_some_and(|error| error.contains("meant to fail")),
        "{body}"
    );
}

fn assert_python_report(response: tachyon_contracts::HandlerResponse) {
    assert!(response.error.is_none(), "{:?}", response.error);
    let body = response.body.expect("body").data;
    let value: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
    let cases = value["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 2, "{body}");
    assert_eq!(cases[0]["name"], "test_opens_a_context", "{body}");
    assert_eq!(cases[0]["passed"], true, "{body}");
    assert_eq!(cases[1]["passed"], false, "{body}");
    assert!(
        cases[1]["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("this case is meant to fail")),
        "{body}"
    );
}

fn assert_java_report(response: tachyon_contracts::HandlerResponse) {
    assert!(response.error.is_none(), "{:?}", response.error);
    let body = response.body.expect("body").data;
    let value: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
    let cases = value["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 2, "{body}");
    assert_eq!(cases[0]["passed"], true, "{body}");
    assert_eq!(cases[1]["passed"], false, "{body}");
    assert!(
        cases[1]["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("this case is meant to fail")),
        "{body}"
    );
}

struct ControllerProject;

impl ControllerProject {
    const LANGUAGES: [(&str, &str); 13] = [
        ("js", "node"),
        ("ts", "bun"),
        ("rb", "ruby"),
        ("lua", "lua"),
        ("py", "python3"),
        ("php", "php-cgi"),
        ("java", "javac"),
        ("groovy", "groovyc"),
        ("kt", "kotlinc"),
        ("cs", "dotnet"),
        ("rs", "cargo"),
        ("go", "go"),
        ("dart", "dart"),
    ];

    fn copy(from: &std::path::Path, to: &std::path::Path) {
        for entry in std::fs::read_dir(from).expect("fixture folder").flatten() {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                std::fs::create_dir_all(&target).expect("folder");
                Self::copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).expect("fixture file");
            }
        }
    }

    fn units(root: &std::path::Path, extension: &str) -> Vec<String> {
        let mut units: Vec<String> = std::fs::read_dir(root.join("server/tests"))
            .expect("tests folder")
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(&format!("ControllerTest.{extension}")))
            .map(|name| format!("server/tests/{name}"))
            .collect();
        units.sort();
        units
    }

    async fn assert_units_pass(extension: &str) {
        let root = tempfile::tempdir().expect("project");
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/controller-test-units")
            .join(extension);
        Self::copy(&fixture, root.path());
        let supervisor = javascript_supervisor(extension);
        let units = Self::units(root.path(), extension);
        assert!(!units.is_empty(), "{extension}: no controller test units");
        for relative in units {
            let source = HandlerSource::discover(root.path(), &relative)
                .unwrap_or_else(|error| panic!("{relative}: {error:?}"));
            let response = supervisor
                .invoke(
                    &source,
                    &HandlerRequest::route("test_unit", "/", HttpMethod::Post),
                    &HandlerCancellation::default(),
                )
                .await
                .expect("invocation");
            assert!(response.error.is_none(), "{relative}: {:?}", response.error);
            let body = response.body.expect("body").data;
            let report: serde_json::Value = serde_json::from_str(&body).expect("JSON report");
            let cases = report["cases"].as_array().expect("cases");
            assert_eq!(cases.len(), 1, "{relative}: {body}");
            assert_eq!(cases[0]["passed"], true, "{relative}: {body}");
        }
    }
}

#[tokio::test]
async fn a_test_unit_reaches_a_controller_in_every_yon_language() {
    for (extension, program) in ControllerProject::LANGUAGES {
        if !tool_available(program) {
            eprintln!("skipping the {extension} controller unit: {program} is unavailable");
            continue;
        }
        ControllerProject::assert_units_pass(extension).await;
    }
}
