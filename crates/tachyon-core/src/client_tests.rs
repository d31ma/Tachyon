use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::model_sources::ModelSources;
use module_url::ModuleUrl;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TestCase {
    pub name: String,
    pub passed: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub called: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TestReport {
    pub unit: String,
    #[serde(default)]
    pub cases: Vec<TestCase>,
    #[serde(default)]
    pub failure: Option<String>,
}

#[cfg(test)]
mod csharp_task_model_tests;
mod csharp_test_models;
mod dart_test_models;
mod fsharp_unit;
mod hooked_utilities;
mod kotlin_unit_files;
mod module_url;
mod python_unit;
mod test_unit_error;

#[cfg(test)]
mod csharp_helper_tests;
#[cfg(test)]
mod dart_construct_tests;
#[cfg(test)]
mod dart_http_context_tests;
#[cfg(test)]
mod go_construct_tests;
#[cfg(test)]
mod hooked_utilities_tests;
#[cfg(test)]
mod java_construct_tests;
#[cfg(test)]
mod java_http_construct_tests;
#[cfg(test)]
mod java_preference_tests;
#[cfg(test)]
mod module_url_tests;
#[cfg(test)]
mod ruby_http_construct_tests;
#[cfg(test)]
mod rust_construct_tests;

#[cfg(test)]
mod java_publisher_tests;

#[cfg(test)]
mod kotlin_construct_tests;

#[cfg(test)]
mod kotlin_helper_unit_tests;

#[cfg(test)]
mod kotlin_document_title_tests;
#[cfg(test)]
mod kotlin_stream_unit_tests;
#[cfg(test)]
mod swift_construct_tests;

#[must_use]
pub fn units(project: &Path, realm: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![PathBuf::from(realm).join("tests")];
    while let Some(relative) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(project.join(&relative)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let child = relative.join(name);
            if entry.path().is_dir() {
                pending.push(child);
                continue;
            }
            if is_test_unit_file(name) {
                found.push(child);
            }
        }
    }
    found.sort();
    found
}

fn is_test_unit_file(name: &str) -> bool {
    let stem = Path::new(name).file_stem().and_then(|stem| stem.to_str());
    stem.is_some_and(|stem| stem.ends_with("Test"))
}

pub fn run(project: &Path, relative: &Path) -> Result<TestReport, Failure> {
    run_with(project, relative, None)
}

pub fn run_with(
    project: &Path,
    relative: &Path,
    javascript: Option<&Path>,
) -> Result<TestReport, Failure> {
    let portable = relative.to_string_lossy().replace('\\', "/");
    let project = &project.canonicalize().map_err(|error| {
        fail(
            &portable,
            format!("Cannot resolve the project directory: {error}"),
        )
    })?;
    let extension = relative
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let source = std::fs::read_to_string(project.join(relative))
        .map_err(|error| fail(&portable, format!("Cannot read the test unit: {error}")))?;
    crate::annotation_layout::validate(relative, &source)?;
    if extension == "dart" {
        return run_dart(project, relative, &source, &portable);
    }
    if extension == crate::fsharp_source::FsharpSource::EXTENSION {
        let printed = fsharp_unit::FsharpUnit::run(project, relative, &source, &portable)?;
        return checked(&printed, &portable);
    }
    if extension == crate::lua_source::LuaSource::EXTENSION {
        let program = crate::tac_lua::LuaClientUnit::program(project, relative, &source)?;
        return run_lua(&program, &portable);
    }
    if extension == crate::tac_cpp::CppSource::EXTENSION {
        let printed = crate::tac_cpp::CppClientUnit::run(project, relative, &source)?;
        return checked(&printed, &portable);
    }
    let lowered = lower_unit(project, relative, &source, extension, &portable)?;
    if extension == "rb" {
        let name = unit_name(&source, &portable)?;
        return run_ruby(project, &lowered, &name, &portable);
    }
    let models = ModelSources::read(project, "client", extension)?;
    if extension == "go" {
        return run_go(
            &crate::companion::go::without_markers(&lowered, &portable)?,
            &go_unit_name(&source, &portable)?,
            &models,
            &portable,
        );
    }
    let name = unit_name(&source, &portable)?;
    let staging = stage_directory(&portable)?;
    let directory = staging.path();
    let printed = match extension {
        "js" | "ts" => script(
            directory, project, javascript, &portable, extension, &lowered, &name,
        ),
        "swift" => swift(directory, &portable, &lowered, &name, &models),
        "kt" => kotlin(directory, &portable, &lowered, &name, &models),
        "java" => java(
            directory,
            &portable,
            &format!(
                "{}{}",
                models.add_to_program(&lowered),
                crate::native::JavaWebServices::test_answer_readers(project, &lowered)?
            ),
            &name,
        ),
        "py" => python_unit::PythonUnit::run(
            directory,
            &portable,
            &models.add_to_program(&lowered),
            &name,
        ),
        "cs" => csharp(
            directory,
            &portable,
            &csharp_test_models::CsharpTestModels::program(
                project,
                &models.add_to_program(&lowered),
            )?,
            &name,
        ),
        "rs" => rust(
            directory,
            &portable,
            &models.add_to_program(&rust_typed_answers(project, &lowered, &portable)?),
            &name,
        ),
        other => Err(fail(
            &portable,
            format!(
                "A Tac test unit is written in one of the realm's own languages; '{other}' is not one of them."
            ),
        )),
    }?;
    checked(&printed, &portable)
}

fn run_dart(
    project: &Path,
    relative: &Path,
    source: &str,
    portable: &str,
) -> Result<TestReport, Failure> {
    let lowered = crate::tac_dart::prepare(project, relative, source)?.source;
    let name = unit_name(source, portable)?;
    let directory = stage_directory(portable)?;
    let models = ModelSources::read(project, "client", "dart")?;
    write_model_files(directory.path(), portable, &models)?;
    let lowered = dart_test_models::DartTestModels::program(project, &models, &lowered, portable)?;
    let printed = dart(directory.path(), portable, &lowered, &name)?;
    checked(&printed, portable)
}

fn write_model_files(
    directory: &Path,
    portable: &str,
    models: &ModelSources,
) -> Result<Vec<PathBuf>, Failure> {
    let mut written = Vec::new();
    for (relative, source) in models.separate_files() {
        let path = directory.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| stage_failure(portable, &error))?;
        }
        std::fs::write(&path, source).map_err(|error| stage_failure(portable, &error))?;
        written.push(path);
    }
    Ok(written)
}

fn stage_directory(portable: &str) -> Result<tempfile::TempDir, Failure> {
    tempfile::Builder::new()
        .prefix("tachyon-test-unit-")
        .tempdir()
        .map_err(|error| stage_failure(portable, &error))
}

fn lower_unit(
    project: &Path,
    relative: &Path,
    source: &str,
    extension: &str,
    portable: &str,
) -> Result<String, Failure> {
    let assembled = crate::annotation_modules::assemble(project, relative, source, "tac")?
        .unwrap_or_else(|| source.to_owned());
    let assembled = native_source(relative, extension, assembled)?;
    let events = crate::events::extract(relative, &assembled, "tac")?;
    let resources = event_resources(events.as_ref());
    let assembled = events
        .as_ref()
        .map_or(assembled.as_str(), |events| events.source.as_str());
    let lowered =
        csharp_test_models::CsharpTestModels::lower(project, relative, assembled, &resources)
            .map_err(|failure| {
                if extension == "go" {
                    return crate::go_source::GoFile::locate(&failure, assembled);
                }
                failure
            })?;
    lowered.ok_or_else(|| {
        fail(
            portable,
            "A test unit declares Test on one class; this source declares none.",
        )
    })
}

fn native_source(relative: &Path, extension: &str, assembled: String) -> Result<String, Failure> {
    if !matches!(
        extension,
        "cs" | "rs" | "go" | "swift" | "kt" | "rb" | "java" | "py"
    ) {
        return Ok(assembled);
    }
    let stripped = crate::behavior::ServiceRoles::read(relative, &assembled)?.source;
    let needs_tac_value = stripped.contains("TacValue") && !stripped.contains("enum TacValue");
    if extension == "rs" && needs_tac_value {
        return Ok(format!("{stripped}\n{}", crate::native::tac_value_source()));
    }
    Ok(stripped)
}

fn rust_typed_answers(project: &Path, lowered: &str, portable: &str) -> Result<String, Failure> {
    if !lowered.contains("BehaviorTypedAnswer::") {
        return Ok(lowered.to_owned());
    }
    let values = if lowered.contains("pub enum TacValue") {
        ""
    } else {
        crate::native::tac_value_source()
    };
    let models = crate::route_types::Converters::tac_text("rs", project)
        .map_err(|message| fail(portable, message))?;
    Ok(format!(
        "{lowered}\n{values}\n{}\n{models}",
        crate::native::RustCompanion::model_source()
    ))
}

fn event_resources(
    events: Option<&crate::events::EventModule>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut resources = BTreeMap::new();
    let Some(events) = events else {
        return resources;
    };
    for event_type in &events.types {
        let fields = event_type
            .resources
            .iter()
            .map(|resource| resource.field.clone())
            .collect();
        resources.insert(event_type.name.clone(), fields);
    }
    resources
}

fn checked(printed: &str, portable: &str) -> Result<TestReport, Failure> {
    let report = report(printed, portable)?;
    if let Some(failure) = &report.failure {
        return Err(fail(
            portable,
            format!("The test unit did not start: {failure}"),
        ));
    }
    if report.cases.is_empty() {
        return Err(fail(
            portable,
            "The test unit declares no case; a case is a method whose name begins with `test`.",
        ));
    }
    Ok(report)
}

fn unit_name(source: &str, portable: &str) -> Result<String, Failure> {
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        if !is_test_marker(line) {
            continue;
        }
        for following in lines.by_ref() {
            if let Some(name) = declared_type_name(following) {
                return Ok(name);
            }
        }
    }
    Err(fail(
        portable,
        "Test must precede the class the unit declares.",
    ))
}

fn go_unit_name(source: &str, portable: &str) -> Result<String, Failure> {
    let code = crate::companion::go::masked_code(source, portable)?;
    for declared in crate::go_source::types(&code) {
        if declared.kind == crate::go_source::GoTypeKind::Struct {
            return Ok(declared.name);
        }
    }
    Err(fail(
        portable,
        "Test must precede the struct the unit declares.",
    ))
}

fn run_go(
    lowered: &str,
    name: &str,
    models: &ModelSources,
    portable: &str,
) -> Result<TestReport, Failure> {
    let staging = stage_directory(portable)?;
    let directory = staging.path();
    for (relative, text) in go_unit_files(lowered, name, models) {
        let path = directory.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| stage_failure(portable, &error))?;
        }
        std::fs::write(&path, text).map_err(|error| stage_failure(portable, &error))?;
    }
    let mut command = Command::new("go");
    command
        .args(["run", "-buildvcs=false", "."])
        .current_dir(directory)
        .envs(ClientTestUnits::GO_ENVIRONMENT);
    for variable in ["GOOS", "GOARCH"] {
        command.env_remove(variable);
    }
    let printed = execute(&mut command, portable)?;
    checked(&printed, portable)
}

fn run_ruby(
    project: &Path,
    lowered: &str,
    name: &str,
    portable: &str,
) -> Result<TestReport, Failure> {
    let staging = stage_directory(portable)?;
    let mut program = String::from("require \"json\"\n");
    program.push_str(crate::tac_ruby::RubyWebServices::PRELUDE);
    program.push_str(&crate::tac_ruby::RubyWebServices::models_program(project)?);
    program.push('\n');
    program.push_str(lowered);
    program.push_str(&ClientTestUnits::RUBY_DRIVER.replace("__UNIT__", name));
    let unit = staging.path().join("unit.rb");
    std::fs::write(&unit, program).map_err(|error| stage_failure(portable, &error))?;
    let mut command = Command::new("ruby");
    command
        .args(["--disable=rubyopt", "-E", "UTF-8:UTF-8"])
        .arg(&unit)
        .current_dir(staging.path());
    let printed = execute(&mut command, portable)?;
    checked(&printed, portable)
}

fn run_lua(program: &str, portable: &str) -> Result<TestReport, Failure> {
    let staging = stage_directory(portable)?;
    let unit = staging.path().join("unit.lua");
    std::fs::write(&unit, program).map_err(|error| stage_failure(portable, &error))?;
    let mut command = Command::new(crate::lua_source::LuaSource::RUNTIME);
    command.arg(&unit).current_dir(staging.path());
    let printed = execute(&mut command, portable)?;
    checked(&printed, portable)
}

fn go_unit_files(lowered: &str, name: &str, models: &ModelSources) -> Vec<(PathBuf, String)> {
    let mut parts = crate::go_source::GoFile::split(lowered);
    let entry = parts.remove(0);
    let mut files = vec![
        (PathBuf::from("go.mod"), crate::native::go_module_file()),
        (
            PathBuf::from("unit.go"),
            crate::native::go_in_package(&entry.text, "main"),
        ),
        (
            PathBuf::from("tac.go"),
            crate::native::go_publisher_alias("main"),
        ),
        (
            PathBuf::from("main.go"),
            ClientTestUnits::GO_DRIVER.replace("__UNIT__", name),
        ),
    ];
    for (index, part) in parts.into_iter().enumerate() {
        files.push((
            PathBuf::from(format!("part{index}.go")),
            crate::native::go_in_package(&part.text, "main"),
        ));
    }
    files.extend(models.separate_files());
    files.extend(crate::native::go_runtime_files());
    files
}

struct ClientTestUnits;

impl ClientTestUnits {
    const GO_ENVIRONMENT: [(&str, &str); 6] = [
        ("GO111MODULE", "on"),
        ("GOFLAGS", ""),
        ("GOPROXY", "off"),
        ("GOTOOLCHAIN", "local"),
        ("GOWORK", "off"),
        ("CGO_ENABLED", "0"),
    ];
    const RUBY_DRIVER: &str =
        "\nSTDOUT.write(JSON.generate(TacValues.plain(__UNIT__.new.POST)) + \"\\n\")\n";
    const GO_DRIVER: &str = "package main\n\nimport (\n\t\"encoding/json\"\n\t\"os\"\n)\n\n\
         func main() {\n\treport, err := json.Marshal((&__UNIT__{}).POST())\n\tif err != nil {\n\t\t_, _ = os.Stderr.WriteString(err.Error())\n\t\tos.Exit(1)\n\t}\n\t_, _ = os.Stdout.Write(append(report, '\\n'))\n}\n";
}

fn is_test_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    ["@Test", "#[Test", "[Test"]
        .iter()
        .any(|marker| trimmed.starts_with(marker))
}

fn declared_type_name(line: &str) -> Option<String> {
    let words = line
        .split(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let index = words
        .iter()
        .position(|word| matches!(*word, "class" | "struct"))?;
    let name = words.get(index + 1)?;
    Some((*name).to_owned())
}

fn script(
    directory: &Path,
    project: &Path,
    javascript: Option<&Path>,
    portable: &str,
    extension: &str,
    lowered: &str,
    name: &str,
) -> Result<String, Failure> {
    hooked_utilities::HookedUtilities::write(project, directory)?;
    let unit_directory = directory.join(Path::new(portable).parent().unwrap_or(Path::new("")));
    let (unit, preload) = write_script_files(&unit_directory, portable, extension, lowered, name)?;
    let mut last_missing = None;
    for program in javascript_runtimes(javascript) {
        let command = runtime_command(&program, directory, project, &unit, &preload)
            .map_err(|error| stage_failure(portable, &error))?;
        let Some(mut command) = command else {
            continue;
        };
        command.current_dir(project).env("TAC_TEST_UNIT", "1");
        match command.output() {
            Ok(output) => return script_output(&program, &output, portable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                last_missing = Some(program);
            }
            Err(error) => {
                return Err(fail(
                    portable,
                    format!("Cannot start the test unit: {error}"),
                ));
            }
        }
    }
    Err(fail(portable, missing_runtime_message(last_missing)))
}

fn write_script_files(
    directory: &Path,
    portable: &str,
    extension: &str,
    lowered: &str,
    name: &str,
) -> Result<(PathBuf, PathBuf), Failure> {
    std::fs::create_dir_all(directory).map_err(|error| stage_failure(portable, &error))?;
    let file_extension = if extension == "ts" { "ts" } else { "mjs" };
    let unit = directory.join(format!("unit.{file_extension}"));
    let entry = format!(
        "\n{}const __tachyon_unit = new {name}();\n\
         __tachyon_unit.POST()\n\
         .then((report) => console.log(JSON.stringify(report)))\n\
         .catch((error) => {{ console.log(JSON.stringify({{ unit: {}, cases: [], \
         failure: String(error && error.message || error) }})); }});\n",
        hooked_utilities::HookedUtilities::RECORDER_PUBLISH,
        serde_json::Value::String(name.to_owned())
    );
    let runtime = crate::behavior::runtime_source();
    std::fs::write(&unit, format!("{lowered}\n{runtime}{entry}"))
        .map_err(|error| stage_failure(portable, &error))?;
    let preload = directory.join("preload.mjs");
    std::fs::write(&preload, include_str!("client_tests_preload.mjs"))
        .map_err(|error| stage_failure(portable, &error))?;
    Ok((unit, preload))
}

fn script_output(
    program: &Path,
    output: &std::process::Output,
    portable: &str,
) -> Result<String, Failure> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    Err(fail(
        portable,
        format!(
            "The test unit did not run under {}: {}",
            program.display(),
            javascript_error_summary(&output.stderr)
        ),
    ))
}

fn javascript_error_summary(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|line| !line.contains("ExperimentalWarning"))
        .take(6)
        .collect::<Vec<_>>()
        .join(" ")
}

fn missing_runtime_message(last_missing: Option<PathBuf>) -> String {
    let last_tried = match last_missing {
        Some(program) => format!(" (last tried {})", program.display()),
        None => String::new(),
    };
    format!(
        "No JavaScript runtime is installed{last_tried}; install Node.js, Bun or Deno, or set TAC_JAVASCRIPT_RUNTIME."
    )
}

fn javascript_runtimes(preferred: Option<&Path>) -> Vec<PathBuf> {
    if let Some(preferred) = preferred {
        return vec![preferred.to_path_buf()];
    }
    let mut runtimes = Vec::new();
    if let Some(configured) = std::env::var_os("TAC_JAVASCRIPT_RUNTIME") {
        runtimes.push(PathBuf::from(configured));
    }
    for name in ["node", "bun", "deno"] {
        runtimes.push(PathBuf::from(name));
    }
    runtimes
}

fn runtime_command(
    program: &Path,
    directory: &Path,
    project: &Path,
    unit: &Path,
    preload: &Path,
) -> std::io::Result<Option<Command>> {
    let kind = program
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let typescript = unit.extension().is_some_and(|value| value == "ts");
    let mut command = Command::new(program);
    match kind {
        "node" => {
            if typescript {
                command.arg("--experimental-strip-types");
            }
            command
                .arg("--import")
                .arg(ModuleUrl::file(preload)?)
                .arg(unit);
        }
        "bun" => {
            command
                .arg("--preload")
                .arg(preload)
                .arg(unit)
                .env("BUN_RUNTIME_TRANSPILER_CACHE_PATH", "0");
        }
        "deno" => configure_deno(&mut command, directory, project, unit, preload)?,
        _ => return Ok(None),
    }
    set_root_urls(&mut command, directory, project)?;
    Ok(Some(command))
}

fn set_root_urls(command: &mut Command, directory: &Path, project: &Path) -> std::io::Result<()> {
    let hooked = ModuleUrl::directory(&directory.join("client"))?;
    let client = ModuleUrl::directory(&project.join("client"))?;
    let project = ModuleUrl::directory(project)?;
    command
        .env("TAC_UNIT_HOOKED_URL", hooked)
        .env("TAC_UNIT_CLIENT_URL", client)
        .env("TAC_UNIT_PROJECT_URL", project);
    Ok(())
}

fn configure_deno(
    command: &mut Command,
    directory: &Path,
    project: &Path,
    unit: &Path,
    preload: &Path,
) -> std::io::Result<()> {
    let map = write_deno_import_map(directory, project)?;
    let entry = directory.join("entry.mjs");
    std::fs::write(
        &entry,
        format!(
            "await import({});\nawait import({});\n",
            serde_json::Value::String(ModuleUrl::file(preload)?),
            serde_json::Value::String(ModuleUrl::file(unit)?),
        ),
    )?;
    let cache = std::env::var_os("DENO_DIR")
        .map_or_else(|| crate::cache::root().join("deno"), PathBuf::from);
    command
        .args(["run", "--quiet", "--node-modules-dir=manual"])
        .args(["--allow-read", "--allow-env", "--allow-sys"])
        .arg("--import-map")
        .arg(&map)
        .arg(&entry)
        .env("HOME", directory)
        .env("DENO_DIR", cache);
    Ok(())
}

fn write_deno_import_map(directory: &Path, project: &Path) -> std::io::Result<PathBuf> {
    let client = project.join("client");
    let mut imports = serde_json::Map::new();
    for entry in std::fs::read_dir(&client)?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let target = ModuleUrl::directory(&entry.path())?;
        imports.insert(format!("/{name}/"), serde_json::Value::String(target));
    }
    if let Some((prefix, target)) = hooked_utilities::HookedUtilities::import_mapping(directory)? {
        imports.insert(prefix, serde_json::Value::String(target));
    }
    let map = directory.join("imports.json");
    std::fs::write(&map, serde_json::json!({ "imports": imports }).to_string())?;
    Ok(map)
}

fn swift(
    directory: &Path,
    portable: &str,
    lowered: &str,
    name: &str,
    models: &ModelSources,
) -> Result<String, Failure> {
    let path = directory.join("Unit.swift");
    let binary = directory.join("unit");
    let models = models.joined_sources();
    let browser = crate::native::SwiftWebServices::test_unit_browser(lowered);
    let wrapped = format!(
        "import Foundation\n{models}{browser}private enum TachyonTestUnit {{\n{lowered}\n}}\n\
         @main struct TachyonTestMain {{\n\
         static func main() async throws {{\n\
         let unit = TachyonTestUnit.{name}()\n\
         print(try await unit.POST().encoded())\n\
         }}\n}}\n"
    );
    std::fs::write(&path, wrapped).map_err(|error| stage_failure(portable, &error))?;
    execute(
        Command::new("swiftc")
            .arg("-parse-as-library")
            .arg(&path)
            .arg("-o")
            .arg(&binary),
        portable,
    )?;
    execute(&mut Command::new(&binary), portable)
}

fn dart(directory: &Path, portable: &str, lowered: &str, name: &str) -> Result<String, Failure> {
    let path = directory.join("unit.dart");
    let publisher = dart_publisher_stub(lowered);
    let service = dart_service_stub(lowered);
    std::fs::write(
        &path,
        format!(
            "{lowered}\n{publisher}{service}\nFuture<void> main() async {{\n  \
             try {{\n    print(BehaviorJson.encode(await {name}().POST()));\n  }} catch (error) {{\n    \
             print(BehaviorJson.encode({{'unit': '{name}', 'cases': [], 'failure': '$error'}}));\n  }}\n}}\n"
        ),
    )
    .map_err(|error| stage_failure(portable, &error))?;
    execute(
        Command::new("dart")
            .args(["run", "--enable-asserts"])
            .arg(&path),
        portable,
    )
}

fn dart_publisher_stub(lowered: &str) -> &'static str {
    let uses_publisher = lowered.contains("ApplicationEventPublisher");
    if !uses_publisher || lowered.contains("class ApplicationEventPublisher") {
        return "";
    }
    "\nfinal class ApplicationEventPublisher {\n  final List<Object?> published = [];\n\n  \
     T publishEvent<T>(T value, [bool retain = false]) {\n    \
     published.add({'event': '${value.runtimeType}', 'value': value, 'retain': retain});\n    return value;\n  }\n}\n"
}

fn dart_service_stub(lowered: &str) -> &'static str {
    if !lowered.contains("TacService") || lowered.contains("class TacService") {
        return "";
    }
    "\nfinal class TacService {\n  TacService([Map<String, Object?>? fields, Map<String, Object? Function(List<Object?>)>? methods])\n      \
     : fields = fields ?? {}, methods = methods ?? {};\n\n  final Map<String, Object?> fields;\n  \
     final Map<String, Object? Function(List<Object?>)> methods;\n\n  Object? get(String field) => fields[field];\n\n  \
     void set(String field, Object? value) {\n    fields[field] = value;\n  }\n\n  \
     Future<Object?> invoke(String method, [List<Object?> arguments = const []]) async {\n    \
     final body = methods[method];\n    if (body == null) throw StateError('TacService has no method $method');\n    \
     return body(arguments);\n  }\n}\n"
}

fn kotlin(
    directory: &Path,
    portable: &str,
    lowered: &str,
    name: &str,
    models: &ModelSources,
) -> Result<String, Failure> {
    let path = directory.join("Unit.kt");
    let jar = directory.join("unit.jar");
    std::fs::write(
        &path,
        format!(
            "{lowered}\n{}\nfun main() {{ println({name}().POST()) }}\n",
            kotlin_provider_contracts(lowered)
        ),
    )
    .map_err(|error| stage_failure(portable, &error))?;
    let model_files = write_model_files(directory, portable, models)?;
    let support_files = kotlin_unit_files::KotlinUnitFiles::write(directory)
        .map_err(|error| stage_failure(portable, &error))?;
    execute(
        Command::new("kotlinc")
            .arg(&path)
            .args(&model_files)
            .args(&support_files)
            .arg("-include-runtime")
            .arg("-d")
            .arg(&jar),
        portable,
    )?;
    execute(Command::new("java").arg("-jar").arg(&jar), portable)
}

fn kotlin_provider_contracts(source: &str) -> &'static str {
    if source.contains("DocumentTitle") {
        return include_str!("native/web_apis/DocumentTitle.kt");
    }
    ""
}

fn java(directory: &Path, portable: &str, lowered: &str, name: &str) -> Result<String, Failure> {
    let classes = directory.join("classes");
    std::fs::write(
        directory.join("Unit.java"),
        format!(
            "{}\n{lowered}\nfinal class TachyonTestMain {{\n\
             public static void main(String[] arguments) {{ System.out.println(Behaviors.encode(new {name}().POST())); }}\n}}\n",
            crate::native::JavaWebServices::test_provider_contracts(lowered)
        ),
    )
    .map_err(|error| stage_failure(portable, &error))?;
    execute(
        Command::new("javac")
            .args(["--release", "21", "-nowarn", "-d"])
            .arg(&classes)
            .arg(directory.join("Unit.java")),
        portable,
    )?;
    execute(
        Command::new("java")
            .arg("-ea")
            .arg("-cp")
            .arg(&classes)
            .arg("TachyonTestMain"),
        portable,
    )
}

fn csharp(directory: &Path, portable: &str, lowered: &str, name: &str) -> Result<String, Failure> {
    let framework = dotnet_framework(portable)?;
    std::fs::write(
        directory.join("unit.cs"),
        format!(
            "{lowered}\nstatic class TachyonTestMain {{\n\
             static void Main() {{ TacWebHost.Ask = BrowserFakes.Reply; System.Console.WriteLine(new {name}().POST()); }}\n}}\n"
        ),
    )
    .map_err(|error| stage_failure(portable, &error))?;
    std::fs::write(
        directory.join("browser.cs"),
        format!(
            "{}{}",
            crate::native::CsharpWebApis::test_unit_library(),
            crate::route_types::WebEventSources::csharp()
        ),
    )
    .map_err(|error| stage_failure(portable, &error))?;
    std::fs::write(directory.join("unit.csproj"), csharp_project(&framework))
        .map_err(|error| stage_failure(portable, &error))?;
    execute(
        Command::new("dotnet")
            .current_dir(directory)
            .args(["build", "-c", "Release", "-o", "out", "--nologo", "-v", "q"])
            .arg("--disable-build-servers"),
        portable,
    )?;
    execute(
        Command::new("dotnet").arg(directory.join("out").join("unit.dll")),
        portable,
    )
}

fn dotnet_framework(portable: &str) -> Result<String, Failure> {
    let listed = Command::new("dotnet")
        .arg("--list-runtimes")
        .output()
        .map_err(|error| fail(portable, format!("Cannot start 'dotnet': {error}")))?;
    let mut newest_major = None;
    for line in String::from_utf8_lossy(&listed.stdout).lines() {
        let Some(version) = line.strip_prefix("Microsoft.NETCore.App ") else {
            continue;
        };
        let Some(major) = version.split('.').next() else {
            continue;
        };
        let Ok(major) = major.parse::<u32>() else {
            continue;
        };
        newest_major = newest_major.max(Some(major));
    }
    let Some(major) = newest_major else {
        return Err(fail(
            portable,
            "No .NET runtime is installed for a C# test unit.",
        ));
    };
    Ok(format!("net{major}.0"))
}

fn csharp_project(framework: &str) -> String {
    format!(
        "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    \
         <OutputType>Exe</OutputType>\n    \
         <TargetFramework>{framework}</TargetFramework>\n    \
         <AssemblyName>unit</AssemblyName>\n    \
         <StartupObject>TachyonTestMain</StartupObject>\n    \
         <Nullable>disable</Nullable>\n    \
         <EnableDefaultCompileItems>false</EnableDefaultCompileItems>\n    \
         <ImplicitUsings>enable</ImplicitUsings>\n  </PropertyGroup>\n  \
         <ItemGroup><Compile Include=\"unit.cs\" /><Compile Include=\"browser.cs\" /></ItemGroup>\n</Project>\n"
    )
}

fn rust(directory: &Path, portable: &str, lowered: &str, name: &str) -> Result<String, Failure> {
    let path = directory.join("unit.rs");
    let binary = directory.join("unit");
    std::fs::write(
        &path,
        format!(
            "{lowered}\nfn main() {{\n\
             match {name}::new().POST() {{\n\
             Ok(report) => println!(\"{{report}}\"),\n\
             Err(error) => {{ eprintln!(\"{{}}\", error.message); std::process::exit(1); }}\n\
             }}\n}}\n"
        ),
    )
    .map_err(|error| stage_failure(portable, &error))?;
    execute(
        Command::new("rustc")
            .args(["--edition", "2024", "-O", "-o"])
            .arg(&binary)
            .arg(&path),
        portable,
    )?;
    execute(&mut Command::new(&binary), portable)
}

fn execute(command: &mut Command, portable: &str) -> Result<String, Failure> {
    let output = command
        .env("TAC_TEST_UNIT", "1")
        .output()
        .map_err(|error| fail(portable, format!("Cannot start the test unit: {error}")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let summary = test_unit_error::TestUnitError::summary(&output.stderr, &output.stdout);
    Err(fail(
        portable,
        format!("The test unit did not run: {summary}"),
    ))
}

fn report(printed: &str, portable: &str) -> Result<TestReport, Failure> {
    for line in printed.lines().rev() {
        if let Ok(report) = serde_json::from_str::<TestReport>(line.trim()) {
            return Ok(report);
        }
    }
    Err(fail(
        portable,
        "The test unit printed no report; its driver is generated, so this is a crash.",
    ))
}

fn stage_failure(portable: &str, error: &std::io::Error) -> Failure {
    fail(portable, format!("Cannot stage the test unit: {error}"))
}

fn fail(portable: &str, message: impl Into<String>) -> Failure {
    Failure::one(diagnostic(
        2021,
        message,
        Some(String::from(
            "A Tac test unit is client/tests/<Name>Test.<ext> declaring one Test class.",
        )),
        source_span(portable, 0, 0),
    ))
}

#[cfg(test)]
mod web_fake_tests;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    struct GoUnit;

    impl GoUnit {
        const CONFIGURATION: &str = "// @TestSetup\ntype UnitTestConfiguration struct{}\n\nfunc (UnitTestConfiguration) Configure(execution *BehaviorExecution) (*BehaviorConfiguration, error) {\n\treturn &BehaviorConfiguration{}, nil\n}\n";
        const SERVICE: &str = "// @Service()\ntype CounterService struct {\n\tCount     int\n\tpublisher ApplicationEventPublisher\n}\n\nfunc (counter *CounterService) Increment() int {\n\tcounter.Count++\n\tcounter.publisher.PublishEvent(Tally{Total: counter.Count}, false)\n\treturn counter.Count\n}\n";
        const MODEL: &str = "type Tally struct {\n\tTotal int\n}\n";
        const UNIT: &str = "// @Test(UnitTestConfiguration)\ntype CounterServiceTest struct {\n\tbehaviors *BehaviorContext\n}\n\nfunc (test *CounterServiceTest) TestIncrementCountsOne() {\n\tcounter := &CounterService{}\n\ttally := Tally{Total: counter.Increment()}\n\tAssert.Equals(1, tally.Total, \"one increment counts one\")\n}\n\nfunc (test *CounterServiceTest) TestFailureIsReported() {\n\tAssert.IsTrue(false, \"expected failure\")\n}\n";
    }

    fn go_installed() -> bool {
        Command::new("go")
            .arg("version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn write(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, source).expect("write");
    }

    struct RubyUnit;

    impl RubyUnit {
        const SETUP: &str = "#[TestSetup]\nclass UnitTestSetup\n  def self.configure(execution)\n    {}\n  end\nend\n";
        const SERVICE: &str = "#[Service]\nclass CounterService\n  attr_reader :count\n\n  def initialize\n    @count = 0\n    @events = ApplicationEventPublisher.new\n  end\n\n  def increment\n    @count += 1\n    @events.publish_event(Tally.new(total: @count))\n    @count\n  end\nend\n";
        const MODEL: &str =
            "# @!attribute [r] total\n#   @return [Integer]\nTally = Data.define(:total)\n";
        const UNIT: &str = "#[Test(UnitTestSetup)]\nclass CounterServiceTest\n  def test_increment_counts_one\n    tally = Tally.new(total: CounterService.new.increment)\n    Assert.equals(1, tally.total, \"one increment counts one\")\n  end\n\n  def test_failure_is_reported\n    Assert.is_true(false, \"expected failure\")\n  end\nend\n";
    }

    #[test]
    fn a_ruby_unit_runs_in_cruby_with_its_services_and_models() {
        let ruby_installed = Command::new("ruby")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !ruby_installed {
            assert!(std::env::var_os("CI").is_none(), "CI must provide ruby");
            eprintln!("skipping: ruby is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            "client/tests/UnitTestSetup.rb",
            RubyUnit::SETUP,
        );
        write(
            root.path(),
            "client/services/CounterService.rb",
            RubyUnit::SERVICE,
        );
        write(root.path(), "client/models/Tally.rb", RubyUnit::MODEL);
        write(
            root.path(),
            "client/tests/CounterServiceTest.rb",
            RubyUnit::UNIT,
        );
        let report = run(root.path(), Path::new("client/tests/CounterServiceTest.rb"))
            .expect("the Ruby unit runs");
        assert_eq!(report.unit, "CounterServiceTest");
        let mut results = Vec::new();
        for case in &report.cases {
            results.push((case.name.as_str(), case.passed));
        }
        assert_eq!(
            results,
            [
                ("test_increment_counts_one", true),
                ("test_failure_is_reported", false)
            ]
        );
    }

    #[test]
    fn a_go_unit_runs_natively_with_its_services_and_models() {
        if !go_installed() {
            assert!(std::env::var_os("CI").is_none(), "CI must provide go");
            eprintln!("skipping: go is unavailable");
            return;
        }
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            "client/tests/UnitTestConfiguration.go",
            GoUnit::CONFIGURATION,
        );
        write(
            root.path(),
            "client/services/CounterService.go",
            GoUnit::SERVICE,
        );
        write(root.path(), "client/models/Tally.go", GoUnit::MODEL);
        write(
            root.path(),
            "client/tests/CounterServiceTest.go",
            GoUnit::UNIT,
        );
        let report = run(root.path(), Path::new("client/tests/CounterServiceTest.go"))
            .expect("the Go unit runs");
        assert_eq!(report.unit, "CounterServiceTest");
        let mut results = Vec::new();
        for case in &report.cases {
            results.push((case.name.as_str(), case.passed));
        }
        assert_eq!(
            results,
            [
                ("TestIncrementCountsOne", true),
                ("TestFailureIsReported", false)
            ]
        );
        let error = report.cases[1].error.as_deref().unwrap_or_default();
        assert!(error.contains("expected failure"), "{error}");
    }
}
