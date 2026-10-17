#![allow(clippy::expect_used)]

use super::PythonUnit;
use crate::client_tests::{checked, lower_unit, run};
use std::path::Path;

struct PythonProject;

impl PythonProject {
    const UNIT_PATH: &'static str = "client/tests/CounterServiceTest.py";
    const SETUP: &'static str = "import typing\n\n@TestSetup\nclass UnitTestSetup:\n    signals: typing.ClassVar[list[object]] = []\n\n    @staticmethod\n    def configure(execution: object) -> dict[str, object]:\n        UnitTestSetup.signals.append(execution.signal)\n        return {}\n";
    const SERVICE: &'static str = "from typing import ClassVar\n\n@service\nclass CounterService:\n    STEP: ClassVar[int] = 1\n    count: int = 0\n\n    def increment(self) -> int:\n        self.count += self.STEP\n        return self.count\n";
    const MODEL: &'static str =
        "from dataclasses import dataclass\n\n@dataclass\nclass Tally:\n    total: int\n";
    const UNIT: &'static str = "import asyncio\nfrom models import Tally\n\n@Test(UnitTestSetup, {\"partition\":\"tests\"})\nclass CounterServiceTest:\n    def test_01_sync_service_and_model(self) -> None:\n        counter = CounterService()\n        tally = Tally(counter.increment())\n        Assert.equals(1, tally.total, 'included service and model')\n        Assert.equals('tests', self.behaviors.partition)\n\n    async def test_02_async_and_previous_cleanup(self) -> None:\n        await asyncio.sleep(0)\n        Assert.is_true(UnitTestSetup.signals[-2].aborted, 'previous case disposed')\n\n    def test_03_failure_is_reported(self) -> None:\n        Assert.is_true(False, 'expected failure')\n\n    def test_04_failure_still_disposes(self) -> None:\n        Assert.is_true(UnitTestSetup.signals[-2].aborted, 'failed case disposed')\n";

    fn project(unit: &str) -> tempfile::TempDir {
        let project = tempfile::tempdir().expect("project");
        Self::write(project.path(), "client/tests/UnitTestSetup.py", Self::SETUP);
        Self::write(
            project.path(),
            "client/services/CounterService.py",
            Self::SERVICE,
        );
        Self::write(project.path(), "client/models/Tally.py", Self::MODEL);
        Self::write(project.path(), Self::UNIT_PATH, unit);
        project
    }

    fn write(project: &Path, relative: &str, source: &str) {
        let path = project.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, source).expect("source");
    }
}

#[test]
fn python_client_unit_source_uses_owned_awaitability_and_cleanup() {
    let project = PythonProject::project(PythonProject::UNIT);
    let lowered = lower_unit(
        project.path(),
        Path::new(PythonProject::UNIT_PATH),
        PythonProject::UNIT,
        "py",
        PythonProject::UNIT_PATH,
    )
    .expect("lowered unit");
    assert!(lowered.contains("await _behavior_await(getattr(self, method)(*arguments), _BehaviorExecution(self.behaviors))"));
    assert!(!lowered.contains("await getattr(self, method)(*arguments)"));
    assert!(!lowered.contains("@service\n"));
    assert!(lowered.contains("STEP: ClassVar[int] = 1"));
    let driver = PythonUnit::program(&lowered, "CounterServiceTest");
    assert!(driver.contains("finally:\n            await unit.stopBehaviors()"));
    assert!(driver.contains("'failure': str(error)"));
}

#[test]
fn python_client_units_run_sync_async_services_models_and_report_assertion_failures() {
    let project = PythonProject::project(PythonProject::UNIT);
    let report = run(project.path(), Path::new(PythonProject::UNIT_PATH)).expect("Python unit");
    assert_eq!(report.unit, "CounterServiceTest");
    assert_eq!(report.cases.len(), 4);
    assert!(report.cases[0].passed, "{:?}", report.cases[0]);
    assert!(report.cases[1].passed, "{:?}", report.cases[1]);
    assert!(!report.cases[2].passed);
    assert!(
        report.cases[2]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("expected failure")
    );
    assert!(report.cases[3].passed, "{:?}", report.cases[3]);
}

#[test]
fn python_client_units_reject_empty_case_reports() {
    let project =
        PythonProject::project("@Test(UnitTestSetup)\nclass CounterServiceTest:\n    pass\n");
    let failure = run(project.path(), Path::new(PythonProject::UNIT_PATH)).expect_err("no cases");
    assert!(format!("{failure:?}").contains("declares no case"));
}

#[test]
fn python_client_unit_startup_failure_is_reported_and_cleanup_always_runs() {
    let project = PythonProject::project(PythonProject::UNIT);
    let source =
        std::fs::read_to_string(project.path().join(PythonProject::UNIT_PATH)).expect("unit");
    let lowered = lower_unit(
        project.path(),
        Path::new(PythonProject::UNIT_PATH),
        &source,
        "py",
        PythonProject::UNIT_PATH,
    )
    .expect("lowered");
    let models =
        crate::model_sources::ModelSources::read(project.path(), "client", "py").expect("models");
    let lowered = models.add_to_program(&lowered);
    let program = PythonUnit::program(&lowered, "CounterServiceTest");
    let program = format!(
        "{program}\nassert all(signal.aborted for signal in UnitTestSetup.signals), 'last case was not disposed'\n"
    );
    let directory = tempfile::tempdir().expect("staging");
    std::fs::write(directory.path().join(PythonUnit::SCRIPT), program).expect("program");
    let printed =
        PythonUnit::execute(directory.path(), PythonProject::UNIT_PATH).expect("cleanup verified");
    assert_eq!(
        checked(&printed, PythonProject::UNIT_PATH)
            .expect("report")
            .cases
            .len(),
        4
    );

    PythonProject::write(
        project.path(),
        "client/tests/UnitTestSetup.py",
        "@TestSetup\nclass UnitTestSetup:\n    @staticmethod\n    def configure(execution: object) -> dict[str, object]:\n        raise RuntimeError('setup refused')\n",
    );
    let failure =
        run(project.path(), Path::new(PythonProject::UNIT_PATH)).expect_err("setup failure");
    let messages = failure
        .diagnostics()
        .iter()
        .map(|problem| problem.message.as_str())
        .collect::<Vec<_>>();
    assert!(messages.contains(&"The test unit did not start: Behavior adapter failed."));
    assert!(!format!("{failure:?}").contains("setup refused"));
}

#[test]
fn python_client_units_keep_missing_test_and_plain_helper_diagnostics() {
    let project = PythonProject::project(
        "class CounterServiceTest:\n    def test_example(self) -> None:\n        pass\n",
    );
    let failure =
        run(project.path(), Path::new(PythonProject::UNIT_PATH)).expect_err("Test marker required");
    assert!(!failure.diagnostics().is_empty());
    PythonProject::write(
        project.path(),
        PythonProject::UNIT_PATH,
        PythonProject::UNIT,
    );
    PythonProject::write(
        project.path(),
        "client/helpers/Plain.py",
        "class Plain:\n    pass\n",
    );
    let failure = run(project.path(), Path::new(PythonProject::UNIT_PATH))
        .expect_err("formal Helper still required");
    assert!(
        failure
            .diagnostics()
            .iter()
            .any(|problem| problem.code.number() == 1363)
    );
}

#[test]
fn python_client_value_source_runs_the_case_once_for_each_input() {
    let project = PythonProject::project(
        "@Test(UnitTestSetup)\nclass CounterServiceTest:\n    @ValueSource([1, \"two\", [3]])\n    def test_keeps_an_input(self, value: object) -> None:\n        Assert.is_true(value is not None)\n\n    @ValueSource([0, 1])\n    def test_counts_from_zero(self, start: int) -> None:\n        Assert.equals(0, start)\n",
    );
    let report = run(project.path(), Path::new(PythonProject::UNIT_PATH)).expect("Python unit");
    let names = report
        .cases
        .iter()
        .map(|case| case.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "test_keeps_an_input[1]",
            "test_keeps_an_input[2]",
            "test_keeps_an_input[3]",
            "test_counts_from_zero[1]",
            "test_counts_from_zero[2]",
        ]
    );
    let passed = report
        .cases
        .iter()
        .map(|case| case.passed)
        .collect::<Vec<_>>();
    assert_eq!(passed, [true, true, true, true, false]);
}
