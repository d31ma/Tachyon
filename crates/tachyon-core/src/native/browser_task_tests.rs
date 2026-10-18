#![allow(clippy::expect_used)]

use super::registry::NativeCompanionInput;
use crate::project::NativeCompanion;
use std::path::Path;
use std::time::Duration;

struct TaskOwnerFixture;

impl TaskOwnerFixture {
    const SERVICE: &'static str = include_str!("../../tests/fixtures/behavior/DelayService.cs");
    const PROJECT: &'static str = "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net9.0</TargetFramework><AllowUnsafeBlocks>true</AllowUnsafeBlocks><Nullable>disable</Nullable><EnableDefaultCompileItems>false</EnableDefaultCompileItems></PropertyGroup><ItemGroup><Compile Include=\"Program.cs\" /></ItemGroup></Project>";

    fn registry(root: &Path, browser: bool) -> String {
        let source = root.join("client/services/DelayService.cs");
        std::fs::create_dir_all(source.parent().expect("service parent"))
            .expect("source directory");
        std::fs::write(&source, Self::SERVICE).expect("actual Task service");
        super::registry::source(
            &[NativeCompanionInput {
                snapshot_root: root.to_owned(),
                language: NativeCompanion::CSharp,
                source,
                service: String::from("delay"),
                class: String::from("DelayService"),
                web_assembly: browser,
                application_provider: false,
            }],
            NativeCompanion::CSharp,
        )
        .expect("selected C# service")
        .expect("emitted C# registry")
    }

    fn runtime() -> String {
        concat!(
            include_str!("../behavior/polyglot/csharp_runtime.cs"),
            include_str!("../behavior/polyglot/csharp_browser_http/BehaviorBrowserHttp.cs"),
            include_str!("../behavior/polyglot/csharp_browser_http/BehaviorBrowserHttpAdapter.cs"),
            include_str!("../behavior/polyglot/csharp_browser_http/BehaviorHttpRevalidation.cs"),
            include_str!("../behavior/polyglot/csharp_browser_http/BehaviorBrowserSocket.cs")
        )
        .replace("__TAC_BEHAVIOR_MODULE__", "\"task-fixture\"")
        .replace("__TAC_BEHAVIOR_ENVIRONMENT__", "\"test\"")
        .replace("__TAC_BEHAVIOR_REGISTRY__", "\"{}\"")
        .replace("__TAC_BEHAVIOR_CONFIGURATIONS__", "")
        .replace("__TAC_BEHAVIOR_ENTITIES__", "")
        .replace("__TAC_BEHAVIOR_TYPES__", "")
    }

    fn run() -> String {
        let root = tempfile::tempdir().expect("Task fixture");
        let registry = Self::registry(root.path(), true);
        let program = format!(
            "using System;\nusing System.Collections;\nusing System.Collections.Generic;\nusing System.Globalization;\nusing System.Linq;\nusing System.Runtime.InteropServices;\nusing System.Text;\nusing System.Text.Json.Nodes;\ninternal static class TacApplication {{ internal const string Id = \"task-fixture\"; }}\n{registry}\n{}\n{}\n{}\n{}\n{}\n{}",
            Self::runtime(),
            concat!(
                include_str!("prelude.cs"),
                include_str!("models/TacModels.cs")
            ),
            concat!(
                include_str!("browser/Tac.cs"),
                include_str!("browser/TacTaskAnswer.cs"),
                include_str!("browser/TacTaskInvocation.cs"),
                include_str!("browser/TacHttpTransport.cs"),
                include_str!("browser/TacHttpInvocation.cs")
            ),
            super::csharp_web_apis::CsharpWebApis::with_events(),
            include_str!("../../tests/fixtures/behavior/TaskSignals.cs"),
            include_str!("../../tests/fixtures/behavior/TaskOwnerDriver.cs"),
        );
        std::fs::write(root.path().join("Program.cs"), program).expect("Task program");
        let project = root.path().join("tasks.csproj");
        std::fs::write(&project, Self::PROJECT).expect("Task project");
        let mut command = std::process::Command::new("dotnet");
        command.args(["run", "--project"]).arg(project);
        let output =
            crate::external_command::run_sync(&mut command, Duration::from_mins(2), 65_536)
                .expect("bounded supervised .NET Task fixture");
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

#[test]
fn browser_task_registry_is_selected_without_changing_native_task_dispatch() {
    let root = tempfile::tempdir().expect("Task project");
    let browser = TaskOwnerFixture::registry(root.path(), true);
    let native = TaskOwnerFixture::registry(root.path(), false);
    assert!(browser.contains("TacMember.TaskMethod("));
    assert!(!browser.contains("GetAwaiter().GetResult()"));
    assert!(native.contains("GetAwaiter().GetResult()"));
    assert!(!native.contains("TacMember.TaskMethod("));
}

#[test]
#[ignore = "requires installed .NET 9; real browser Task dispatch and exact activation ownership"]
fn browser_tasks_suspend_once_and_retire_exact_owner_continuations() {
    assert!(TaskOwnerFixture::run().contains("PASS browser Task ownership and continuation"));
}
