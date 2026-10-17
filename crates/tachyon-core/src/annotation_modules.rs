use crate::annotation_layout::{brace_depth_after, masked_source};
use crate::failure::{Failure, diagnostic, source_span};
use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _, OpenOptionsSyncExt as _};
use cap_std::fs::{Dir, OpenOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::io::{ErrorKind, Read};
use std::ops::Range;
use std::path::{Path, PathBuf};

mod client_utilities;
mod csharp_test_helpers;
mod helper_callers;
mod selected_test_setups;
mod server_test_closure;
use helper_callers::HelperCallers;
use server_test_closure::ServerTestClosure;
mod module_input;
mod shared_imports;
mod test_configuration;
use module_input::ModuleInput;
use shared_imports::SharedImports;

#[cfg(test)]
mod csharp_test_helpers_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod helper_callers_tests;
#[cfg(test)]
mod shared_utility_tests;

#[cfg(test)]
mod server_test_closure_tests;
#[cfg(test)]
mod test_configuration_tests;

pub(crate) fn role_folders() -> [&'static str; 9] {
    Module::ROLE_FOLDERS
}

pub(crate) fn assemble(
    snapshot_root: &Path,
    entry: &Path,
    source: &str,
    environment: &str,
) -> Result<Option<String>, Failure> {
    Module::assembled(snapshot_root, entry, source, environment, false)
}

fn injected_contracts(
    entry: &Path,
    source: &str,
    modules: &[Module],
    environment: &str,
) -> Option<String> {
    let paths = modules.iter().map(|module| module.path.as_path());
    let contracts = crate::declared_contract::DeclaredContract::injected_names(paths);
    let paths = modules.iter().map(|module| module.path.as_path());
    let remotes = crate::declared_contract::DeclaredContract::injected_remotes(paths);
    let wanted = [environment == "tac", !contracts.is_empty()] == [true, true];
    wanted
        .then(|| {
            crate::declared_contract::DeclaredContract::injected_with_remotes(
                entry, source, &contracts, &remotes,
            )
        })
        .flatten()
}

fn located_modules(modules: Vec<Module>, extension: &str) -> Vec<Module> {
    modules
        .into_iter()
        .map(|module| Module {
            source: crate::fsharp_source::FsharpLines::located_in(
                extension,
                &module.path,
                &module.source,
            ),
            ..module
        })
        .collect()
}

fn refuse_tac_helpers(
    modules: &[Module],
    extension: &str,
    environment: &str,
) -> Result<(), Failure> {
    let composed_after_entry = matches!(extension, "rs" | "cs" | "fs" | "swift" | "kt" | "java");
    if environment != "tac" || !composed_after_entry {
        return Ok(());
    }
    for module in modules {
        crate::behavior::HelperRole::refuse_in_tac(&module.path, &module.source)?;
    }
    Ok(())
}

fn entry_extension<'a>(entry: &'a Path, environment: &str) -> Result<&'a str, Failure> {
    if !matches!(environment, "yon" | "tac") {
        return Err(invalid(entry, "Unknown annotation module environment."));
    }
    let extension = entry
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let supported = Module::EXTENSIONS.contains(&extension)
        && !(environment == "yon" && extension == "swift")
        && !(environment == "tac" && matches!(extension, "groovy" | "php"));
    if !supported {
        return Err(invalid(
            entry,
            "The entry language is not supported in this Tac or Yon realm.",
        ));
    }
    Ok(extension)
}

fn open_entry_root(
    snapshot_root: &Path,
    entry: &Path,
    extension: &str,
    environment: &str,
) -> Result<Dir, Failure> {
    let entry_name = entry
        .to_str()
        .ok_or_else(|| invalid(entry, "Entry path must be UTF-8."))?
        .replace(std::path::MAIN_SEPARATOR, "/");
    let manifest = entry.with_file_name(format!("{environment}.modules.json"));
    let root = open_root(snapshot_root, &manifest)?;
    validate_path(&entry_name, extension, environment, entry)?;
    for realm in ["tac", "yon"] {
        reject_legacy_manifest(
            &root,
            &entry.with_file_name(format!("{realm}.modules.json")),
        )?;
    }
    Ok(root)
}

fn read_modules(
    root: &Dir,
    input: ModuleInput<'_>,
    converters: &mut crate::declared_contract::ConverterModelTable<'_>,
    imported_utilities: &client_utilities::ClientUtilities,
) -> Result<(Vec<Module>, usize), Failure> {
    let ModuleInput {
        entry,
        source: entry_source,
        paths,
        environment,
    } = input;
    let extension = entry_extension(entry, environment)?;
    let mut names = BTreeSet::new();
    register_names(entry, entry_source, &mut names)?;
    let mut total = entry_source.len();
    let mut modules = Vec::new();
    for path in paths {
        if imported_utilities.contains(path) {
            continue;
        }
        let text = read_module_text(root, path, Module::MAX_SOURCE.saturating_sub(total))?;
        total += text.len();
        let shared_source = [
            path.starts_with("server/shared"),
            path.starts_with("client/shared/native"),
        ]
        .contains(&true);
        let error_source = crate::annotation_layout::is_error_source(path);
        let in_controllers = path.starts_with("server/controllers");
        if in_controllers && !wanted_controller_file(entry, path, &text)? {
            continue;
        }
        if error_source || shared_source {
            crate::annotation_layout::validate(path, &text)?;
        }
        test_configuration::reject_role_file(path, &text)?;
        crate::annotation_layout::FolderRoleCheck::check(path, &text)?;
        if !shared_source
            && !error_source
            && !crate::annotation_layout::UtilityLayout::is_utility(path)
            && !crate::annotation_layout::has_primary_role(path, &text)?
            && !crate::annotation_layout::has_framework_annotation(path, &text)?
        {
            continue;
        }
        if modules.len() == Module::MAX_MODULES {
            return Err(invalid(
                entry,
                "An entry may discover at most 512 annotated role files.",
            ));
        }
        register_names(path, &text, &mut names)?;
        validate_imports(path, &text, extension)?;
        if environment == "tac" {
            crate::events::BrowserStreams::refuse_outside_script(path, &text)?;
        }
        let text = SharedImports::rebase(entry, path, &text, extension)?;
        let text = converters.written(path, text)?;
        let text = crate::declared_contract::DeclaredContract::concrete(path, &text, environment)
            .unwrap_or(text);
        let text = ServerTestClosure::discovery_coverage(entry, path, text)?;
        modules.push(Module {
            path: path.clone(),
            source: erase_exports(path, text, extension)?,
        });
    }
    Ok((modules, total))
}

fn wanted_controller_file(entry: &Path, path: &Path, text: &str) -> Result<bool, Failure> {
    if tests_controllers(entry) {
        return crate::annotation_layout::has_tested_controller(path, text);
    }
    crate::annotation_layout::has_error_handler(path, text)
}

fn tests_controllers(entry: &Path) -> bool {
    entry.starts_with("server/tests")
}

fn read_module_text(root: &Dir, path: &Path, limit: usize) -> Result<String, Failure> {
    let bytes = read_file(root, path, limit)?;
    let text = String::from_utf8(bytes)
        .map_err(|_| invalid(path, "Annotation modules must be UTF-8 source files."))?;
    if text.contains('\0') {
        return Err(invalid(path, "Annotation modules cannot contain NUL."));
    }
    Ok(text)
}

fn erase_exports(path: &Path, text: String, extension: &str) -> Result<String, Failure> {
    match extension {
        "java" => erase_java_public(path, &text),
        "js" | "ts" => erase_module_exports(path, &text),
        _ => Ok(text),
    }
}

pub(crate) struct Module {
    path: PathBuf,
    source: String,
}

impl Module {
    fn injected_dependencies(modules: Vec<Self>, extension: &str, environment: &str) -> Vec<Self> {
        let ruby_tac = [extension == "rb", environment == "tac"] == [true, true];
        let kotlin_tac = [extension == "kt", environment == "tac"] == [true, true];
        let paths = modules.iter().map(|module| module.path.as_path());
        let contracts = crate::declared_contract::DeclaredContract::injected_names(paths);
        let paths = modules.iter().map(|module| module.path.as_path());
        let remotes = crate::declared_contract::DeclaredContract::injected_remotes(paths);
        modules
            .into_iter()
            .map(|module| {
                let helper =
                    crate::declared_contract::DeclaredContract::is_kotlin_helper(&module.path);
                let injected = [ruby_tac, [kotlin_tac, helper] == [true, true]]
                    .contains(&true)
                    .then(|| {
                        crate::declared_contract::DeclaredContract::injected_with_remotes(
                            &module.path,
                            &module.source,
                            &contracts,
                            &remotes,
                        )
                    })
                    .flatten();
                Self {
                    source: injected.unwrap_or(module.source),
                    ..module
                }
            })
            .collect()
    }

    pub(crate) fn declared_type(root: &Path, path: &Path) -> Result<Option<String>, Failure> {
        let root = Dir::open_ambient_dir(root, cap_std::ambient_authority())
            .map_err(|error| invalid(path, &format!("Cannot open provider root: {error}")))?;
        let source = read_module_text(&root, path, Self::MAX_SOURCE)?;
        Ok(crate::annotation_layout::ClassFileName::declared(
            path, &source,
        ))
    }

    pub(crate) fn plain_model(path: &Path, source: &str) -> Result<String, Failure> {
        erase_module_exports(path, source)
    }

    pub(crate) fn with_models(
        entry: &Path,
        source: &str,
        files: Vec<(PathBuf, String)>,
    ) -> Result<String, Failure> {
        let extension = entry.extension().and_then(OsStr::to_str).unwrap_or("");
        let mut names = BTreeSet::new();
        register_names(entry, source, &mut names)?;
        let modules = files
            .into_iter()
            .map(|(path, source)| {
                register_names(&path, &source, &mut names)?;
                let source = erase_exports(&path, source, extension)?;
                Ok(Self { path, source })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        Self::compose_models(entry, source, &modules, extension)
    }

    fn compose_models(
        entry: &Path,
        source: &str,
        modules: &[Self],
        extension: &str,
    ) -> Result<String, Failure> {
        let compose = Self::model_composer(extension);
        compose(entry, source, modules, extension)
    }

    fn model_composer(
        extension: &str,
    ) -> fn(&Path, &str, &[Self], &str) -> Result<String, Failure> {
        match extension {
            "go" => Self::go_model_program,
            _ => Self::other_model_program,
        }
    }

    fn go_model_program(
        entry: &Path,
        source: &str,
        modules: &[Self],
        _: &str,
    ) -> Result<String, Failure> {
        compose_go(entry, source, modules)
    }

    fn other_model_program(
        entry: &Path,
        source: &str,
        modules: &[Self],
        extension: &str,
    ) -> Result<String, Failure> {
        compose(
            entry,
            source,
            modules.iter().map(|module| module.source.clone()).collect(),
            extension,
            "yon",
        )
    }

    pub(crate) fn captured_units(
        snapshot_root: &Path,
        entry: &Path,
    ) -> Result<Vec<(PathBuf, String)>, Failure> {
        let entry = entry.strip_prefix(snapshot_root).unwrap_or(entry);
        let extension = entry_extension(entry, "tac")?;
        let root = open_entry_root(snapshot_root, entry, extension, "tac")?;
        let captured = read_module_text(&root, entry, Self::MAX_SOURCE)?;
        Self::native_units(snapshot_root, entry, &captured)
    }

    pub(crate) fn native_units(
        snapshot_root: &Path,
        entry: &Path,
        source: &str,
    ) -> Result<Vec<(PathBuf, String)>, Failure> {
        let entry = entry.strip_prefix(snapshot_root).unwrap_or(entry);
        let extension = entry_extension(entry, "tac")?;
        let root = open_entry_root(snapshot_root, entry, extension, "tac")?;
        let captured = read_module_text(&root, entry, Self::MAX_SOURCE)?;
        crate::annotation_layout::FolderRoleCheck::check(entry, &captured)?;
        crate::annotation_layout::validate_transformed(entry, source)?;
        let mut paths = discover_paths(&root, entry, extension, false, true)?;
        paths.extend(Discovery::native_contract_paths(&root, extension)?);
        let mut converters =
            crate::declared_contract::ConverterModelTable::new(snapshot_root, "tac", extension);
        let utilities =
            client_utilities::ClientUtilities::imported(&root, entry, &captured, &paths)?;
        let captured = utilities.with_source(&captured);
        Self::require_source_bound(entry, &captured)?;
        let input = ModuleInput {
            entry,
            source: &captured,
            paths: &paths,
            environment: "tac",
        };
        let (modules, _) = read_modules(&root, input, &mut converters, &utilities)?;
        refuse_tac_helpers(&modules, extension, "tac")?;
        let mut units = vec![(entry.to_path_buf(), captured)];
        units.extend(
            modules
                .into_iter()
                .map(|module| (module.path, module.source)),
        );
        Ok(units)
    }

    pub(crate) fn native_source(
        snapshot_root: &Path,
        entry: &Path,
        source: &str,
    ) -> Result<Option<String>, Failure> {
        Self::assembled(snapshot_root, entry, source, "tac", true)
    }

    fn assembled(
        snapshot_root: &Path,
        entry: &Path,
        source: &str,
        environment: &str,
        native_services: bool,
    ) -> Result<Option<String>, Failure> {
        let entry = entry.strip_prefix(snapshot_root).unwrap_or(entry);
        crate::behavior::BehaviorsImport::refuse(entry, source)?;
        let extension = entry_extension(entry, environment)?;
        let root = open_entry_root(snapshot_root, entry, extension, environment)?;
        let captured = read_module_text(&root, entry, Module::MAX_SOURCE)?;
        crate::annotation_layout::FolderRoleCheck::check(entry, &captured)?;
        crate::annotation_layout::validate_transformed(entry, source)?;
        let include_errors = environment == "yon"
            && entry.starts_with("server/controllers")
            && crate::annotation_layout::has_route_controller(entry, source)?;
        let paths = discover_paths(&root, entry, extension, include_errors, native_services)?;
        Self::require_source_bound(entry, source)?;
        let mut converters = crate::declared_contract::ConverterModelTable::new(
            snapshot_root,
            environment,
            extension,
        );
        let utilities = client_utilities::ClientUtilities::imported(&root, entry, source, &paths)?;
        let source = utilities.with_source(source);
        Self::require_source_bound(entry, &source)?;
        let input = ModuleInput {
            entry,
            source: &source,
            paths: &paths,
            environment,
        };
        let (mut modules, total) = read_modules(&root, input, &mut converters, &utilities)?;
        Self::refuse_native_peer_lifetimes(&modules, native_services)?;
        test_configuration::include(
            &root,
            entry,
            &source,
            extension,
            environment,
            &mut modules,
            total,
        )?;
        ServerTestClosure::select(snapshot_root, input, &mut modules)?;
        if modules.is_empty() {
            return Ok(utilities.changed().then_some(source));
        }
        refuse_tac_helpers(&modules, extension, environment)?;
        let mut modules = Module::injected_dependencies(modules, extension, environment);
        csharp_test_helpers::CsharpTestHelpers::prune(&root, input, &mut modules, total)?;
        HelperCallers::check(entry, &source, &modules, environment)?;
        let injected = injected_contracts(entry, &source, &modules, environment);
        let source = injected.as_deref().unwrap_or(&source);
        let output = if extension == "go" {
            compose_go(entry, source, &modules)?
        } else {
            let modules = declaration_order(entry, located_modules(modules, extension), extension)?;
            let source = crate::fsharp_source::FsharpLines::located_in(extension, entry, source);
            compose(entry, &source, modules, extension, environment)?
        };
        if output.len() > Module::MAX_SOURCE {
            return Err(invalid(entry, "Combined annotation source exceeds 1 MiB."));
        }
        Ok(Some(output))
    }

    fn require_source_bound(entry: &Path, source: &str) -> Result<(), Failure> {
        if source.len() > Self::MAX_SOURCE || source.contains('\0') {
            return Err(invalid(
                entry,
                "Annotation source exceeds 1 MiB or contains NUL.",
            ));
        }
        Ok(())
    }

    fn refuse_native_peer_lifetimes(
        modules: &[Module],
        native_services: bool,
    ) -> Result<(), Failure> {
        modules
            .iter()
            .filter(|_| native_services)
            .try_for_each(|module| {
                crate::behavior::ServiceRoles::refuse_native_peer_lifetime(
                    &module.path,
                    &module.source,
                )
            })
    }

    const ROLE_FOLDERS: [&str; 9] = [
        "remotes",
        "clients",
        "setups",
        "controllers",
        "helpers",
        "entities",
        "converters",
        "stores",
        "services",
    ];
    const MAX_SOURCE: usize = 1024 * 1024;
    const MAX_MODULES: usize = 512;
    const EXTENSIONS: [&str; 15] = [
        "js", "ts", "py", "php", "java", "groovy", "kt", "cs", "fs", "rs", "swift", "dart", "go",
        "rb", "lua",
    ];
}

fn declaration_order(
    entry: &Path,
    modules: Vec<Module>,
    extension: &str,
) -> Result<Vec<String>, Failure> {
    if !matches!(extension, "js" | "ts" | "py" | "rb" | "fs") {
        return Ok(modules.into_iter().map(|module| module.source).collect());
    }
    let owners = owner_by_name(&modules)?;
    let dependencies = module_dependencies(&modules, &owners, extension)?;
    let order = dependency_order(entry, &dependencies)?;
    let mut sources: Vec<Option<String>> = modules
        .into_iter()
        .map(|module| Some(module.source))
        .collect();
    let mut ordered = Vec::new();
    for index in order {
        if let Some(source) = sources[index].take() {
            ordered.push(source);
        }
    }
    Ok(ordered)
}

fn owner_by_name(modules: &[Module]) -> Result<BTreeMap<String, usize>, Failure> {
    let mut owners = BTreeMap::new();
    for (index, module) in modules.iter().enumerate() {
        for name in crate::annotation_layout::declaration_names(&module.path, &module.source)? {
            owners.insert(name, index);
        }
    }
    Ok(owners)
}

fn module_dependencies(
    modules: &[Module],
    owners: &BTreeMap<String, usize>,
    extension: &str,
) -> Result<Vec<BTreeSet<usize>>, Failure> {
    let mut dependencies = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        let code = masked_source(&module.path, &module.source)?;
        validate_static_templates(&module.path, &module.source, &code, extension)?;
        let definitions = if extension == "fs" {
            code.clone()
        } else {
            definition_lines(&code, extension)
        };
        let mut needed = BTreeSet::new();
        for word in definitions.split(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        {
            if let Some(owner) = owners.get(word)
                && *owner != index
            {
                needed.insert(*owner);
            }
        }
        dependencies.push(needed);
    }
    Ok(dependencies)
}

fn dependency_order(entry: &Path, dependencies: &[BTreeSet<usize>]) -> Result<Vec<usize>, Failure> {
    let mut completed = BTreeSet::new();
    let mut order = Vec::new();
    while order.len() < dependencies.len() {
        let Some(next) = (0..dependencies.len())
            .find(|index| !completed.contains(index) && dependencies[*index].is_subset(&completed))
        else {
            return Err(invalid(
                entry,
                "Annotated modules contain a definition-time dependency cycle. Move cyclic initialization into methods invoked after construction.",
            ));
        };
        completed.insert(next);
        order.push(next);
    }
    Ok(order)
}

fn validate_static_templates(
    path: &Path,
    source: &str,
    code: &str,
    extension: &str,
) -> Result<(), Failure> {
    if !matches!(extension, "js" | "ts") {
        return Ok(());
    }
    let mut offset = 0;
    let mut depth = 0_usize;
    for line in code.split_inclusive('\n') {
        if depth == 1
            && line.trim_start().starts_with("static ")
            && let Some(equal) = assignment_before_call(line)
            && direct_template_interpolates(source[offset + equal + 1..].trim_start())
        {
            return Err(invalid(
                path,
                "Interpolated template literals in direct static field initializers cannot be dependency-ordered. Move the computation into a method invoked after construction.",
            ));
        }
        depth = brace_depth_after(line, depth);
        offset += line.len();
    }
    Ok(())
}

fn assignment_before_call(text: &str) -> Option<usize> {
    let equal = text.find('=')?;
    if let Some(call) = text.find('(')
        && call < equal
    {
        return None;
    }
    Some(equal)
}

fn direct_template_interpolates(source: &str) -> bool {
    let Some(template) = source.strip_prefix('`') else {
        return false;
    };
    let bytes = template.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'`' => return false,
            b'$' if bytes.get(at + 1) == Some(&b'{') => return true,
            _ => at += 1,
        }
    }
    false
}

fn definition_lines(code: &str, extension: &str) -> String {
    if extension == "py" {
        return python_definition_lines(code);
    }
    if extension == "rb" {
        return ruby_definition_lines(code);
    }
    brace_definition_lines(code)
}

fn python_definition_lines(code: &str) -> String {
    let mut output = String::new();
    let mut method_indent = None;
    for line in code.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if method_indent.is_some_and(|method| indent <= method) {
            method_indent = None;
        }
        if method_indent.is_some() {
            continue;
        }
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            output.push_str(python_signature(line));
            method_indent = Some(indent);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

fn ruby_definition_lines(code: &str) -> String {
    let mut output = String::new();
    let mut method_indent = None;
    for line in code.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - trimmed.len();
        if let Some(method) = method_indent {
            if indent > method {
                continue;
            }
            method_indent = None;
            if indent == method && trimmed == "end" {
                continue;
            }
        }
        if trimmed.starts_with("def ") {
            method_indent = Some(indent);
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn brace_definition_lines(code: &str) -> String {
    let mut output = String::new();
    let mut depth = 0_usize;
    for line in code.lines() {
        if line.trim_start().is_empty() {
            continue;
        }
        if depth == 0 {
            output.push_str(text_before_brace(line));
            output.push('\n');
        }
        if let Some(initializer) = static_field_initializer(line) {
            output.push_str(initializer);
            output.push('\n');
        }
        depth = brace_depth_after(line, depth);
    }
    output
}

fn text_before_brace(line: &str) -> &str {
    line.split_once('{').map_or(line, |(header, _)| header)
}

fn static_field_initializer(line: &str) -> Option<&str> {
    let (_, tail) = line.split_once("static ")?;
    assignment_before_call(text_before_brace(tail))?;
    Some(tail)
}

fn python_signature(line: &str) -> &str {
    let mut depth = 0_usize;
    for (index, byte) in line.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b':' if depth == 0 => return &line[..index],
            _ => {}
        }
    }
    line
}

fn register_names(path: &Path, source: &str, names: &mut BTreeSet<String>) -> Result<(), Failure> {
    for name in crate::annotation_layout::declaration_names(path, source)? {
        if !names.insert(name) {
            return Err(invalid(
                path,
                "Annotation modules declare a duplicate type name.",
            ));
        }
    }
    Ok(())
}

fn reject_legacy_manifest(root: &Dir, manifest: &Path) -> Result<(), Failure> {
    let directory = open_parent(root, manifest)?;
    match directory.symlink_metadata(manifest.file_name().unwrap_or_default()) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        _ => Err(invalid(
            manifest,
            "Annotation module manifests are no longer supported. Remove this file and move annotated role files beneath the entry directory's plural role folders.",
        )),
    }
}

fn discover_paths(
    root: &Dir,
    entry: &Path,
    extension: &str,
    include_errors: bool,
    native_services: bool,
) -> Result<Vec<PathBuf>, Failure> {
    let entry_directory = entry.parent().unwrap_or_else(|| Path::new(""));
    let service = entry_directory.starts_with("client/services");
    let routed = entry_directory.starts_with("server/controllers");
    let tested_controllers = tests_controllers(entry);
    let shared_parent = shared_role_parent(entry_directory, service, routed);
    let shared_directory = open_parent(root, &shared_parent.join("entry"))?;
    let mut discovery = Discovery::new(extension);
    for folder in Module::ROLE_FOLDERS {
        if service && folder == "services" && !native_services {
            continue;
        }
        if folder != "controllers" || include_errors || tested_controllers {
            discovery.search_role_folder(&shared_directory, &shared_parent.join(folder), folder)?;
        }
    }
    if include_errors || tested_controllers {
        discovery.search_role_folder(&shared_directory, &shared_parent.join("errors"), "errors")?;
    }
    let realm = entry
        .components()
        .next()
        .map_or(Path::new(""), |part| Path::new(part.as_os_str()));
    let realm_directory = open_parent(root, &realm.join("entry"))?;
    if realm != Path::new("client") {
        discovery.search_role_folder(&realm_directory, &realm.join("utils"), "utils")?;
    }
    discovery.search_client_shared_utilities(&realm_directory, realm)?;
    if realm == Path::new("server") && !matches!(extension, "js" | "ts" | "go") {
        discovery.search_role_folder(&realm_directory, &realm.join("shared"), "shared")?;
    }
    let mut paths = discovery.paths;
    paths.retain(|path| {
        path != entry
            && (!path.starts_with("server/controllers")
                || tested_controllers
                || path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.ends_with("ErrorHandler")
                            || (extension == "rs" && name.ends_with("_error_handler"))
                    }))
    });
    paths.sort_by_key(|path| (!path.starts_with("server/shared"), path.clone()));
    Ok(paths)
}

fn shared_role_parent(entry_directory: &Path, service: bool, routed: bool) -> &Path {
    if entry_directory.starts_with("client/tests") {
        return Path::new("client");
    }
    if entry_directory.starts_with("server/tests") || entry_directory.starts_with("server/gateways")
    {
        return Path::new("server");
    }
    if service {
        return Path::new("client");
    }
    if routed {
        return Path::new("server");
    }
    if entry_directory.components().nth(1).is_some_and(|part| {
        Module::ROLE_FOLDERS
            .iter()
            .any(|folder| part.as_os_str() == *folder)
    }) {
        return entry_directory
            .ancestors()
            .find(|path| path.components().count() == 1)
            .unwrap_or(entry_directory);
    }
    entry_directory
}

pub(crate) struct Discovery<'a> {
    extension: &'a str,
    entry_count: usize,
    paths: Vec<PathBuf>,
}

impl<'a> Discovery<'a> {
    fn search_client_shared_utilities(
        &mut self,
        directory: &Dir,
        realm: &Path,
    ) -> Result<(), Failure> {
        (realm == Path::new("client"))
            .then(|| self.search_shared_utilities(directory))
            .transpose()
            .map(|_| ())
    }

    fn search_shared_utilities(&mut self, directory: &Dir) -> Result<(), Failure> {
        let path = Path::new(crate::annotation_layout::UtilityLayout::CLIENT_ROOT);
        Self::open_shared_utilities(directory, path)?
            .map(|shared| self.search_role_folder(&shared, path, "utils"))
            .transpose()
            .map(|_| ())
    }

    fn open_shared_utilities(directory: &Dir, path: &Path) -> Result<Option<Dir>, Failure> {
        match directory.open_dir_nofollow("shared") {
            Ok(shared) => Ok(Some(shared)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(_) => Err(invalid(
                path,
                "Utility discovery requires a regular client/shared directory, not a symlink.",
            )),
        }
    }

    pub(crate) fn role_paths(project: &Path, folder: &Path) -> Result<Vec<PathBuf>, Failure> {
        let root = open_root(project, folder)?;
        let realm = folder
            .components()
            .next()
            .map(std::path::Component::as_os_str)
            .unwrap_or_default();
        if root
            .symlink_metadata(realm)
            .is_err_and(|error| error.kind() == ErrorKind::NotFound)
        {
            return Ok(Vec::new());
        }
        let parent = open_parent(&root, folder)?;
        let name = folder.file_name().and_then(OsStr::to_str).unwrap_or("");
        let mut discovery = Discovery::new("");
        discovery.search_role_folder(&parent, folder, name)?;
        Ok(discovery.paths)
    }

    fn native_contract_paths(root: &Dir, extension: &str) -> Result<Vec<PathBuf>, Failure> {
        if extension != "rs" {
            return Ok(Vec::new());
        }
        Self::rust_contract_paths(root)
    }

    fn rust_contract_paths(root: &Dir) -> Result<Vec<PathBuf>, Failure> {
        let client = open_parent(root, Path::new("client/entry"))?;
        let shared = Self::optional_shared(&client)?;
        shared.map_or_else(
            || Ok(Vec::new()),
            |shared| Self::collect_contract_paths(&shared),
        )
    }

    fn optional_shared(client: &Dir) -> Result<Option<Dir>, Failure> {
        match client.symlink_metadata("shared") {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            _ => client.open_dir_nofollow("shared").map(Some).map_err(|_| {
                invalid(
                    Path::new("client/shared"),
                    "Native contracts require readable directories, never symlinks.",
                )
            }),
        }
    }

    fn collect_contract_paths(shared: &Dir) -> Result<Vec<PathBuf>, Failure> {
        let mut discovery = Self::new("rs");
        discovery.search_role_folder(shared, Path::new("client/shared/native"), "native")?;
        Ok(discovery.paths)
    }

    const MAX_ENTRIES: usize = 4096;
    const MAX_DEPTH: usize = 32;

    fn new(extension: &'a str) -> Self {
        Self {
            extension,
            entry_count: 0,
            paths: Vec::new(),
        }
    }

    fn search_role_folder(
        &mut self,
        parent: &Dir,
        path: &Path,
        folder: &str,
    ) -> Result<(), Failure> {
        match parent.symlink_metadata(folder) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
            _ => {
                return Err(invalid(
                    path,
                    "Role folders must be readable directories, not symlinks.",
                ));
            }
        }
        let directory = parent
            .open_dir_nofollow(folder)
            .map_err(|_| invalid(path, "Cannot safely open role folder."))?;
        self.collect(&directory, path, 0)
    }

    fn collect(&mut self, directory: &Dir, path: &Path, depth: usize) -> Result<(), Failure> {
        if depth > Self::MAX_DEPTH {
            return Err(invalid(
                path,
                "Role discovery exceeds the 32-directory nesting limit.",
            ));
        }
        let names = self.sorted_names(directory, path)?;
        if names
            .iter()
            .any(|name| is_entry_name(name.to_str().unwrap_or("")))
        {
            return Ok(());
        }
        for name in names {
            self.collect_child(directory, &name, path.join(&name), depth)?;
        }
        Ok(())
    }

    fn sorted_names(&mut self, directory: &Dir, path: &Path) -> Result<Vec<OsString>, Failure> {
        let entries = directory
            .entries()
            .map_err(|_| invalid(path, "Cannot read role folder."))?;
        let mut names = Vec::new();
        for entry in entries {
            self.entry_count += 1;
            if self.entry_count > Self::MAX_ENTRIES {
                return Err(invalid(
                    path,
                    "Role discovery exceeds the 4096-entry limit.",
                ));
            }
            let entry = entry.map_err(|_| invalid(path, "Cannot inspect role folder entry."))?;
            names.push(entry.file_name());
        }
        names.sort();
        Ok(names)
    }

    fn collect_child(
        &mut self,
        directory: &Dir,
        name: &OsStr,
        child_path: PathBuf,
        depth: usize,
    ) -> Result<(), Failure> {
        let metadata = directory
            .symlink_metadata(name)
            .map_err(|_| invalid(&child_path, "Cannot inspect role source."))?;
        if metadata.is_symlink() || (!metadata.is_dir() && !metadata.is_file()) {
            return Err(invalid(
                &child_path,
                "Role discovery does not follow symlinks or special files.",
            ));
        }
        if metadata.is_dir() {
            let child = directory
                .open_dir_nofollow(name)
                .map_err(|_| invalid(&child_path, "Cannot safely open role source directory."))?;
            return self.collect(&child, &child_path, depth + 1);
        }
        if self.extension.is_empty()
            || child_path.extension().and_then(|value| value.to_str()) == Some(self.extension)
        {
            self.paths.push(child_path);
        }
        Ok(())
    }
}

pub(crate) fn is_entry_name(name: &str) -> bool {
    if name == "tac.html" {
        return true;
    }
    let Some((stem, extension)) = name.split_once('.') else {
        return false;
    };
    stem == "tac" && Module::EXTENSIONS.contains(&extension)
}

fn validate_path(
    value: &str,
    extension: &str,
    environment: &str,
    diagnostic_path: &Path,
) -> Result<(), Failure> {
    let boundary = if environment == "yon" {
        "server"
    } else {
        "client"
    };
    let pieces: Vec<_> = value.split('/').collect();
    let invalid_component = pieces
        .iter()
        .any(|part| part.is_empty() || matches!(*part, "." | ".."));
    if invalid_component
        || value.chars().any(char::is_control)
        || value.contains(['\\', ':'])
        || pieces.first().copied() != Some(boundary)
    {
        return Err(invalid(
            diagnostic_path,
            "Module paths must be project-relative, remain in their client/server boundary, and contain no dot segments.",
        ));
    }
    if Path::new(value)
        .extension()
        .and_then(|value| value.to_str())
        != Some(extension)
    {
        return Err(invalid(
            diagnostic_path,
            "Every discovered module must match its entry language extension.",
        ));
    }
    Ok(())
}

fn open_root(root: &Path, path: &Path) -> Result<Dir, Failure> {
    let parent = root
        .parent()
        .ok_or_else(|| invalid(path, "Captured project root is invalid."))?;
    let name = root
        .file_name()
        .ok_or_else(|| invalid(path, "Captured project root is invalid."))?;
    // A one-segment relative root such as `website` has an empty parent; joining it to `.` opens the working directory.
    let directory =
        Dir::open_ambient_dir(Path::new(".").join(parent), cap_std::ambient_authority())
            .map_err(|_| invalid(path, "Cannot open the captured project root."))?;
    directory
        .open_dir_nofollow(name)
        .map_err(|_| invalid(path, "Captured project root must not be a symlink."))
}

fn open_parent(root: &Dir, path: &Path) -> Result<Dir, Failure> {
    let mut directory = root
        .try_clone()
        .map_err(|_| invalid(path, "Cannot access captured sources."))?;
    let Some(parent) = path.parent() else {
        return Ok(directory);
    };
    for component in parent.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(invalid(
                path,
                "Source paths must be relative without dot segments.",
            ));
        };
        directory = directory.open_dir_nofollow(name).map_err(|_| {
            invalid(
                path,
                "Module source directories must exist and must not be symlinks.",
            )
        })?;
    }
    Ok(directory)
}

fn read_file(root: &Dir, path: &Path, limit: usize) -> Result<Vec<u8>, Failure> {
    let directory = open_parent(root, path)?;
    let name = path
        .file_name()
        .ok_or_else(|| invalid(path, "Missing source filename."))?;
    let Ok(metadata) = directory.symlink_metadata(name) else {
        return Err(invalid(
            path,
            "A discovered role source is missing or inaccessible.",
        ));
    };
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > limit as u64 {
        return Err(invalid(
            path,
            "Module files must be bounded regular files, not symlinks.",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No).nonblock(true);
    let file = directory
        .open_with(name, &options)
        .map_err(|_| invalid(path, "Cannot safely open module source."))?;
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(invalid(path, "Module source must remain a regular file."));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid(path, "Cannot read module source."))?;
    if bytes.len() > limit {
        return Err(invalid(
            path,
            "Annotation module source exceeds the combined 1 MiB limit.",
        ));
    }
    Ok(bytes)
}

fn validate_imports(path: &Path, source: &str, extension: &str) -> Result<(), Failure> {
    crate::behavior::BehaviorsImport::refuse(path, source)?;
    let masked = masked_source(path, source)?;
    if relocates_imports(path, source, &masked, extension) {
        return Err(invalid(
            path,
            "Included annotation files cannot relocate imports, package declarations, or relative file references; keep ordinary dependency imports in the entry point.",
        ));
    }
    Ok(())
}

fn relocates_imports(path: &Path, source: &str, masked: &str, extension: &str) -> bool {
    let words = code_words(masked);
    match extension {
        "js" | "ts" => javascript_relocates_imports(path, source, masked, &words),
        "php" => has_any_word(
            &words,
            &[
                "require",
                "require_once",
                "include",
                "include_once",
                "__DIR__",
                "__FILE__",
                "namespace",
            ],
        ),
        "dart" => dart_relocates_imports(source, masked, &words),
        "lua" => has_any_word(&words, &["require", "dofile", "loadfile"]),
        "rs" => rust_relocates_imports(masked, &words),
        "py" => python_relocates_imports(source, &words),
        "rb" => has_any_word(&words, &["require_relative", "__FILE__", "__dir__"]),
        "go" => false,
        _ => has_any_word(&words, &["package", "namespace"]),
    }
}

fn code_words(masked: &str) -> BTreeSet<&str> {
    masked
        .split(|value: char| !value.is_ascii_alphanumeric() && value != '_')
        .collect()
}

fn has_any_word(words: &BTreeSet<&str>, wanted: &[&str]) -> bool {
    wanted.iter().any(|word| words.contains(word))
}

fn javascript_relocates_imports(
    path: &Path,
    source: &str,
    masked: &str,
    words: &BTreeSet<&str>,
) -> bool {
    if has_any_word(words, &["require"]) {
        return true;
    }
    if relocatable_import(path, source, masked) {
        return true;
    }
    for tail in masked.split("export").skip(1) {
        if tail.trim_start().starts_with(['{', '*']) {
            return true;
        }
    }
    false
}

fn dart_relocates_imports(source: &str, masked: &str, words: &BTreeSet<&str>) -> bool {
    if has_any_word(words, &["part", "library", "export"]) {
        return true;
    }
    for (line, code) in source.lines().zip(masked.lines()) {
        if !code.trim_start().starts_with("import ") {
            continue;
        }
        let target = line
            .trim()
            .trim_start_matches("import ")
            .trim_start()
            .trim_start_matches(['\'', '"']);
        if !target.starts_with("dart:") && !target.starts_with("models/") {
            return true;
        }
    }
    false
}

fn rust_relocates_imports(masked: &str, words: &BTreeSet<&str>) -> bool {
    if has_any_word(words, &["mod", "include", "include_str", "include_bytes"]) {
        return true;
    }
    let mut compact = String::new();
    for character in masked.chars() {
        if !character.is_whitespace() {
            compact.push(character);
        }
    }
    compact.contains("#[path=")
}

fn python_relocates_imports(source: &str, words: &BTreeSet<&str>) -> bool {
    if has_any_word(words, &["__import__", "__file__"]) {
        return true;
    }
    for line in source.lines() {
        let line = line.trim();
        let is_import = line.starts_with("import ") || line.starts_with("from ");
        let imports_models = line.starts_with("from models import ") || line == "import models";
        let imports_typing = line
            .strip_prefix("from typing import ")
            .is_some_and(|names| {
                names.split(',').all(|name| {
                    matches!(
                        name.trim(),
                        "Any" | "Annotated" | "Protocol" | "Final" | "Optional" | "ClassVar"
                    )
                })
            });
        let allowed = imports_models
            || imports_typing
            || matches!(
                line,
                "import json"
                    | "import os"
                    | "import asyncio"
                    | "import typing"
                    | "from typing import Any"
            );
        if is_import && !allowed {
            return true;
        }
    }
    false
}

fn import_statements(source: &str, masked: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut offset = 0;
    let mut open: Option<(usize, isize)> = None;
    for (line, code) in source
        .split_inclusive('\n')
        .zip(masked.split_inclusive('\n'))
    {
        let start = offset;
        offset += line.len();
        let depth = isize::try_from(code.matches('{').count()).unwrap_or(isize::MAX)
            - isize::try_from(code.matches('}').count()).unwrap_or(isize::MAX);
        if let Some((begin, balance)) = open {
            let balance = balance + depth;
            if balance <= 0 {
                found.push(begin..offset);
                open = None;
            } else {
                open = Some((begin, balance));
            }
            continue;
        }
        let trimmed = code.trim_start();
        if !(trimmed.starts_with("import ") || trimmed.starts_with("import{")) {
            continue;
        }
        if depth > 0 {
            open = Some((start, depth));
        } else {
            found.push(start..offset);
        }
    }
    found
}

fn folded(statement: &str) -> String {
    statement.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn merge_imports(path: &Path, modules: Vec<String>) -> Result<String, Failure> {
    let mut imports = ImportMerge::default();
    let mut bodies = Vec::with_capacity(modules.len());
    for source in modules {
        bodies.push(imports.take_hoisted_imports(path, &source)?);
    }
    Ok(imports.write(&bodies))
}

#[derive(Default)]
struct ImportMerge {
    value_imports: BTreeMap<String, BTreeSet<String>>,
    type_imports: BTreeMap<String, BTreeSet<String>>,
    verbatim: BTreeSet<String>,
    owners: BTreeMap<String, String>,
    spellings: BTreeMap<String, String>,
}

impl ImportMerge {
    fn take_hoisted_imports(&mut self, path: &Path, source: &str) -> Result<String, Failure> {
        let masked = masked_source(path, source)?;
        let mut body = String::with_capacity(source.len());
        let mut kept = 0;
        for range in import_statements(source, &masked) {
            let statement = folded(&source[range.clone()]);
            if !is_hoisted_import(&statement) {
                continue;
            }
            body.push_str(&source[kept..range.start]);
            kept = range.end;
            self.add_statement(path, &statement)?;
        }
        body.push_str(&source[kept..]);
        Ok(body)
    }

    fn add_statement(&mut self, path: &Path, statement: &str) -> Result<(), Failure> {
        let canonical =
            SharedImports::canonical(path, import_specifier(statement).unwrap_or_default());
        let specifier = self.first_spelling(&canonical);
        let statement = statement.trim_end_matches(';');
        if statement.contains(" with {") || statement.contains(" assert {") {
            self.verbatim.insert(statement.to_owned());
            return Ok(());
        }
        let type_only =
            statement.starts_with("import type ") && !statement.starts_with("import type from ");
        let imports = if type_only {
            &mut self.type_imports
        } else {
            &mut self.value_imports
        };
        let specifier_bindings = imports.entry(specifier.clone()).or_default();
        let Some(clause) = import_clause(statement, type_only) else {
            return Ok(());
        };
        for binding in bindings(clause) {
            let local = local_binding_name(&binding);
            if let Some(owner) = self.owners.get(&local)
                && owner != &specifier
            {
                return Err(invalid(
                    path,
                    &format!(
                        "Composed role files bind '{local}' from both '{owner}' and '{specifier}'; rename one of them."
                    ),
                ));
            }
            self.owners.insert(local, specifier.clone());
            specifier_bindings.insert(binding);
        }
        Ok(())
    }

    fn first_spelling(&mut self, specifier: &str) -> String {
        let module = specifier
            .strip_suffix(".ts")
            .or_else(|| specifier.strip_suffix(".js"))
            .unwrap_or(specifier);
        self.spellings
            .entry(module.to_owned())
            .or_insert_with(|| specifier.to_owned())
            .clone()
    }

    fn write(self, bodies: &[String]) -> String {
        let mut output = String::new();
        for statement in self.verbatim {
            output.push_str(&statement);
            output.push_str(";\n");
        }
        write_imports(&mut output, "import", self.value_imports);
        write_imports(&mut output, "import type", self.type_imports);
        output.push_str(&bodies.join("\n\n"));
        output
    }
}

fn is_hoisted_import(statement: &str) -> bool {
    import_specifier(statement).is_some()
}

fn import_clause(statement: &str, type_only: bool) -> Option<&str> {
    let (clause, _) = statement.strip_prefix("import ")?.rsplit_once(" from ")?;
    let clause = clause.trim();
    if type_only {
        return clause.strip_prefix("type ");
    }
    Some(clause)
}

fn local_binding_name(binding: &str) -> String {
    let inner = binding.trim_matches(['{', '}']);
    inner.rsplit(' ').next().unwrap_or(inner).to_owned()
}

fn write_imports(
    output: &mut String,
    keyword: &str,
    specifiers: BTreeMap<String, BTreeSet<String>>,
) {
    for (specifier, bindings) in specifiers {
        let quoted = serde_json::Value::String(specifier).to_string();
        let mut defaults = Vec::new();
        let mut names = Vec::new();
        for binding in bindings {
            if binding.starts_with('{') {
                names.push(binding.trim_matches(['{', '}']).trim().to_owned());
            } else {
                defaults.push(binding);
            }
        }
        for default in &defaults {
            let _ = writeln!(output, "{keyword} {default} from {quoted};");
        }
        if !names.is_empty() {
            let _ = writeln!(
                output,
                "{keyword} {{ {} }} from {quoted};",
                names.join(", ")
            );
        } else if defaults.is_empty() {
            let _ = writeln!(output, "{keyword} {quoted};");
        }
    }
}

fn bindings(clause: &str) -> Vec<String> {
    let (default_part, named_part) = match clause.split_once('{') {
        Some((head, rest)) => (head, rest.split_once('}').map_or("", |(inner, _)| inner)),
        None => (clause, ""),
    };
    let mut found = Vec::new();
    for part in default_part.split(',') {
        let part = part.trim();
        if !part.is_empty() {
            found.push(part.to_owned());
        }
    }
    for part in named_part.split(',') {
        let part = part.trim();
        if !part.is_empty() {
            found.push(format!("{{{part}}}"));
        }
    }
    found
}

fn relocatable_import(path: &Path, source: &str, masked: &str) -> bool {
    let statements = import_statements(source, masked);
    if has_import_outside_statements(masked, &statements) {
        return true;
    }
    for range in statements {
        let statement = &source[range];
        let shared = import_specifier(&folded(statement))
            .and_then(|specifier| SharedImports::path(path, specifier))
            .is_some();
        if !is_safe_import_statement(statement) && !shared {
            return true;
        }
    }
    false
}

fn has_import_outside_statements(masked: &str, statements: &[Range<usize>]) -> bool {
    let mut from = 0;
    while let Some(found) = masked[from..].find("import") {
        let at = from + found;
        from = at + "import".len();
        let inside_word = is_word_character(masked[..at].chars().next_back())
            || is_word_character(masked[from..].chars().next());
        if inside_word {
            continue;
        }
        let claimed = statements.iter().any(|range| range.contains(&at));
        if !claimed {
            return true;
        }
    }
    false
}

fn is_word_character(value: Option<char>) -> bool {
    value.is_some_and(|value| value.is_ascii_alphanumeric() || value == '_' || value == '$')
}

fn is_safe_import_statement(text: &str) -> bool {
    let statement = folded(text);
    let statement = statement.trim_end_matches(';');
    if statement.contains(';') {
        return false;
    }
    import_specifier(statement)
        .is_some_and(|specifier| specifier.starts_with('/') && !specifier.starts_with("//"))
}

fn import_specifier(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("import ")?;
    let rest = rest.rsplit_once(" from ").map_or(rest, |(_, tail)| tail);
    let rest = rest.trim().trim_end_matches(';').trim();
    let quote = rest.chars().next()?;
    if !matches!(quote, '\'' | '"') {
        return None;
    }
    rest[1..].split(quote).next()
}

fn compose(
    path: &Path,
    entry: &str,
    mut modules: Vec<String>,
    extension: &str,
    environment: &str,
) -> Result<String, Failure> {
    let entry_first = matches!(extension, "rb" | "lua")
        || (environment == "tac" && matches!(extension, "swift" | "kt" | "cs" | "rs" | "java"));
    if entry_first {
        modules.insert(0, entry.to_owned());
    } else {
        modules.push(entry.to_owned());
    }
    match extension {
        "js" | "ts" => merge_imports(path, modules),
        "php" => compose_php(path, modules),
        "py" => hoist_python_future(path, &modules.join("\n\n")),
        "java" | "groovy" | "kt" | "cs" | "fs" | "swift" | "dart" => {
            hoist_imports(path, &modules.join("\n\n"), extension)
        }
        "rs" => without_repeated_uses(path, &modules.join("\n\n")),
        _ => Ok(modules.join("\n\n")),
    }
}

fn compose_go(path: &Path, entry: &str, modules: &[Module]) -> Result<String, Failure> {
    let mut program = entry.to_owned();
    for module in modules {
        let code = masked_source(path, &module.source)?;
        let types = crate::go_source::types(&code);
        let name = types
            .first()
            .map_or("role", |declared| declared.name.as_str());
        let portable = module.path.to_string_lossy().replace('\\', "/");
        program.push_str(&crate::go_source::GoFile::role_separator(name, &portable));
        program.push_str(&module.source);
    }
    Ok(program)
}

fn without_repeated_uses(path: &Path, source: &str) -> Result<String, Failure> {
    let masked = masked_source(path, source)?;
    let mut seen = BTreeSet::new();
    let mut output = String::new();
    let mut depth = 0_usize;
    for (line, code) in source
        .split_inclusive('\n')
        .zip(masked.split_inclusive('\n'))
    {
        let statement = code.trim();
        let is_use = depth == 0 && statement.starts_with("use ") && statement.ends_with(';');
        depth = brace_depth_after(code, depth);
        if is_use && !seen.insert(line.trim().to_owned()) {
            output.push('\n');
            continue;
        }
        output.push_str(line);
    }
    Ok(output)
}

fn compose_php(path: &Path, modules: Vec<String>) -> Result<String, Failure> {
    let mut output = String::from("<?php\n");
    for source in modules {
        let body = php_body(&source);
        let masked = masked_source(path, body)?;
        if masked.contains("<?") || masked.contains("?>") {
            return Err(invalid(
                path,
                "PHP annotation files must contain one source block without embedded PHP tags.",
            ));
        }
        output.push_str(body);
        output.push('\n');
    }
    without_repeated_uses(path, &output)
}

fn php_body(source: &str) -> &str {
    let trimmed = source.trim();
    let body = trimmed.strip_prefix("<?php").unwrap_or(trimmed).trim();
    body.strip_suffix("?>").unwrap_or(body)
}

fn hoist_python_future(path: &Path, source: &str) -> Result<String, Failure> {
    let masked = masked_source(path, source)?;
    let mut imports = BTreeSet::new();
    let mut body = String::new();
    for (line, code) in source
        .split_inclusive('\n')
        .zip(masked.split_inclusive('\n'))
    {
        if code.starts_with("from __future__ import ") {
            imports.insert(line.trim());
        } else {
            body.push_str(line);
        }
    }
    Ok(imports_then_body(&imports, &body))
}

fn hoist_imports(path: &Path, source: &str, extension: &str) -> Result<String, Failure> {
    let keyword = match extension {
        "cs" => "using ",
        "fs" => "open ",
        _ => "import ",
    };
    let masked = masked_source(path, source)?;
    let mut imports = BTreeSet::new();
    let mut body = String::new();
    let mut depth = 0_usize;
    for (line, code) in source
        .split_inclusive('\n')
        .zip(masked.split_inclusive('\n'))
    {
        if depth == 0 && code.trim_start().starts_with(keyword) {
            imports.insert(line.trim());
            body.push_str(crate::fsharp_source::FsharpLines::hoisted_gap(extension));
        } else {
            body.push_str(line);
        }
        depth = brace_depth_after(code, depth);
    }
    Ok(imports_then_body(&imports, &body))
}

fn imports_then_body(imports: &BTreeSet<&str>, body: &str) -> String {
    let imports: Vec<&str> = imports.iter().copied().collect();
    format!("{}\n{body}", imports.join("\n"))
}

fn erase_module_exports(path: &Path, source: &str) -> Result<String, Failure> {
    let masked = masked_source(path, source)?;
    let mut bytes = source.as_bytes().to_vec();
    let mut offset = 0;
    let mut depth = 0_usize;
    for line in masked.split_inclusive('\n') {
        if depth == 0 {
            erase_export_keyword(&mut bytes, line, offset);
        }
        depth = brace_depth_after(line, depth);
        offset += line.len();
    }
    String::from_utf8(bytes)
        .map_err(|_| invalid(path, "Annotation modules must be UTF-8 source files."))
}

fn erase_export_keyword(bytes: &mut [u8], line: &str, offset: usize) {
    let indent = line.len() - line.trim_start().len();
    for keyword in ["export default ", "export "] {
        if line[indent..].starts_with(keyword) {
            let start = offset + indent;
            bytes[start..start + keyword.len() - 1].fill(b' ');
            return;
        }
    }
}

fn erase_java_public(path: &Path, source: &str) -> Result<String, Failure> {
    let masked = masked_source(path, source)?;
    let mut bytes = source.as_bytes().to_vec();
    let mut offset = 0;
    let mut depth = 0_usize;
    for line in masked.split_inclusive('\n') {
        if depth == 0
            && let Some(index) = line.find("public ")
        {
            let start = offset + index;
            bytes[start..start + "public".len()].fill(b' ');
        }
        depth = brace_depth_after(line, depth);
        offset += line.len();
    }
    String::from_utf8(bytes).map_err(|_| invalid(path, "Cannot compose Java module declarations."))
}

fn invalid(path: &Path, message: &str) -> Failure {
    Failure::one(diagnostic(2101, message, Some("Place annotated same-language role files in plural role folders beneath this Tac or Yon entry directory; no module manifest or ambient imports are needed.".into()), source_span(&path.to_string_lossy(), 0, 0)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    fn write(root: &Path, name: &str, source: impl AsRef<[u8]>) {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap_or(root))
            .unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(path, source).unwrap_or_else(|error| panic!("{error}"));
    }

    fn write_files(root: &Path, files: &[(&str, &str)]) {
        for (name, source) in files {
            write(root, name, source);
        }
    }

    struct Sample;

    impl Sample {
        const ENTRY: &'static str = "server/controllers/MoneyController.js";
        const CONTROLLER: &'static str = "@RequestRoute(\"/money\")\n@Public\nclass MoneyController {\n  @GetRoute\n  get() {}\n}\n";

        fn controller(name: &str) -> String {
            let path = name.to_ascii_lowercase();
            format!(
                "@RequestRoute(\"/{path}\")\n@Public\nclass {name}Controller {{\n  @GetRoute\n  get() {{}}\n}}\n"
            )
        }
    }

    fn project() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        write(root.path(), Sample::ENTRY, Sample::CONTROLLER);
        root
    }

    fn run(root: &Path) -> Result<Option<String>, Failure> {
        assemble(root, Path::new(Sample::ENTRY), Sample::CONTROLLER, "yon")
    }

    fn composed_controller(root: &Path, name: &str) -> String {
        let entry = format!("server/controllers/{name}Controller.js");
        let source = Sample::controller(name);
        write(root, &entry, &source);
        assemble(root, Path::new(&entry), &source, "yon")
            .expect(name)
            .expect("composed")
    }

    fn composed_entry(root: &Path, entry: &str, source: &str, environment: &str) -> String {
        assemble(root, Path::new(entry), source, environment)
            .expect("discovery")
            .expect("discovered roles")
    }

    fn run_error(root: &Path) -> String {
        run(root).expect_err("discovery must fail").to_string()
    }

    fn assert_contains(text: &str, expected: &str) {
        assert!(text.contains(expected), "{expected}: {text}");
    }

    #[test]
    fn plain_shared_classes_are_composed_before_fsharp_services() {
        let root = tempfile::tempdir().expect("project");
        let entry = "server/controllers/OrdersController.fs";
        let source = "[<RequestRoute(\"/orders\")>]\n[<Public>]\ntype OrdersController() =\n    [<GetRoute>]\n    member this.GetOrders() : string = OrderService.Place(\"book\")\n";
        write(root.path(), entry, source);
        write(
            root.path(),
            "server/shared/orders.fs",
            "type OrderCode() =\n    static member Place(item: string) : string = item\n",
        );
        write(
            root.path(),
            "server/services/OrderService.fs",
            "[<Service>]\ntype OrderService() =\n    static member Place(item: string) : string = OrderCode.Place(item)\n",
        );
        let output = composed_entry(root.path(), entry, source, "yon");
        assert_contains(&output, "type OrderCode()");
        assert!(output.find("type OrderCode()") < output.find("type OrderService()"));
    }

    #[test]
    fn javascript_services_import_shared_classes() {
        let root = project();
        write(
            root.path(),
            "server/services/OrderService.js",
            "import { OrderCode } from '../shared/orders.js';\n@Service\nclass OrderService {}\n",
        );
        let output = run(root.path()).expect("shared import").expect("service");
        assert_contains(
            &output,
            "import { OrderCode } from \"../shared/orders.js\";",
        );
        let rebased = SharedImports::rebase(
            Path::new(Sample::ENTRY),
            Path::new("server/services/nested/OrderService.js"),
            "import { OrderCode } from '../../shared/orders.js';",
            "js",
        )
        .expect("rebased import");
        assert_contains(&rebased, "'../shared/orders.js'");
        assert!(
            validate_imports(
                Path::new("server/services/OrderService.js"),
                "import { Secret } from '../../client/shared/secret.js';",
                "js"
            )
            .is_err()
        );
    }

    #[test]
    fn shared_classes_reject_annotations_extra_classes_and_file_variables() {
        let root = tempfile::tempdir().expect("project");
        let entry = "server/controllers/OrdersController.cs";
        let source = "[RequestRoute(\"/orders\")]\n[Public]\nclass OrdersController {}\n";
        write(root.path(), entry, source);
        for shared in [
            "[Service]\nclass OrderService {}",
            "class OrderCode {}\nclass Other {}",
            "var counter = 0;\nclass OrderCode {}",
        ] {
            write(root.path(), "server/shared/orders.cs", shared);
            assert!(
                assemble(root.path(), Path::new(entry), source, "yon").is_err(),
                "{shared}"
            );
        }
    }

    #[test]
    fn fsharp_shared_dependencies_are_declared_before_their_callers() {
        let root = tempfile::tempdir().expect("project");
        let entry = "server/controllers/OrdersController.fs";
        let source = "[<RequestRoute(\"/orders\")>]\n[<Public>]\ntype OrdersController() =\n    [<GetRoute>]\n    member this.GetOrders() : string = ACode.Text()\n";
        write(root.path(), entry, source);
        write(
            root.path(),
            "server/shared/A.fs",
            "type ACode() =\n    static member Text() : string = ZCode.Text()\n",
        );
        write(
            root.path(),
            "server/shared/Z.fs",
            "type ZCode() =\n    static member Text() : string = \"book\"\n",
        );
        let output = composed_entry(root.path(), entry, source, "yon");
        assert!(output.find("type ZCode()") < output.find("type ACode()"));
        write(
            root.path(),
            "server/shared/Z.fs",
            "type ZCode() =\n    static member Text() : string = ACode.Text()\n",
        );
        assert!(
            assemble(root.path(), Path::new(entry), source, "yon")
                .expect_err("cycle")
                .to_string()
                .contains("dependency cycle")
        );
    }

    #[cfg(unix)]
    #[test]
    fn plain_shared_discovery_refuses_file_and_directory_symlinks() {
        let outside = tempfile::tempdir().expect("outside");
        write(outside.path(), "OrderCode.cs", "class OrderCode {}\n");
        for directory_link in [false, true] {
            let root = tempfile::tempdir().expect("project");
            let entry = "server/controllers/OrdersController.cs";
            let source = "[RequestRoute(\"/orders\")]\n[Public]\nclass OrdersController {}\n";
            write(root.path(), entry, source);
            if directory_link {
                std::os::unix::fs::symlink(outside.path(), root.path().join("server/shared"))
                    .expect("directory link");
            } else {
                std::fs::create_dir_all(root.path().join("server/shared"))
                    .expect("shared directory");
                std::os::unix::fs::symlink(
                    outside.path().join("OrderCode.cs"),
                    root.path().join("server/shared/OrderCode.cs"),
                )
                .expect("file link");
            }
            assert!(
                assemble(root.path(), Path::new(entry), source, "yon")
                    .expect_err("shared link")
                    .to_string()
                    .contains("symlink")
            );
        }
    }

    #[test]
    fn discovers_annotated_roles_in_shared_folders_without_a_manifest() {
        let root = project();
        write_files(
            root.path(),
            &[
                (
                    "server/services/MoneyService.js",
                    "@Service\nclass MoneyService {}\n",
                ),
                (
                    "server/services/Helper.js",
                    "throw new Error('not a role');",
                ),
                (
                    "server/services/OtherService.py",
                    "@Service\nclass OtherService: pass",
                ),
                (
                    "server/controllers/OtherController.js",
                    "@RequestRoute(\"/other\")\n@Public\nclass OtherController {\n  @GetRoute\n  get() {}\n}\n",
                ),
                (
                    "client/services/ClientService.js",
                    "@Service()\nexport default class ClientService {}\n",
                ),
            ],
        );
        let source = run(root.path())
            .expect("discovery")
            .expect("role composition");
        assert!(source.contains("class MoneyService"));
        assert!(source.contains("class MoneyController"));
        for missing in [
            "not a role",
            "OtherService",
            "OtherController",
            "ClientService",
        ] {
            assert!(!source.contains(missing), "{missing}");
        }
    }

    #[test]
    fn shared_roles_compose_into_every_controller_but_never_another_controller() {
        let root = project();
        let shared_roles = [
            ("services", "Service"),
            ("stores", "Store"),
            ("clients", "HTTPClient"),
            ("setups", "Setup"),
            ("entities", "Entity"),
            ("helpers", "Helper"),
        ];
        for (folder, role) in shared_roles {
            let suffix = if role == "HTTPClient" { "Client" } else { role };
            write(
                root.path(),
                &format!("server/{folder}/Shared{suffix}.js"),
                format!("@{role}\nclass Shared{suffix} {{}}\n"),
            );
        }
        let orders = composed_controller(root.path(), "Orders");
        let products = composed_controller(root.path(), "Products");
        for composed in [&orders, &products] {
            for suffix in ["Service", "Store", "Client", "Setup", "Entity", "Helper"] {
                assert!(
                    composed.contains(&format!("class Shared{suffix}")),
                    "{suffix}"
                );
            }
        }
        assert!(orders.contains("class OrdersController"));
        assert!(!orders.contains("ProductsController") && !orders.contains("MoneyController"));
        assert!(products.contains("class ProductsController"));
        assert!(!products.contains("OrdersController"));
    }

    #[test]
    fn misplaced_shared_roles_fail_with_their_shared_folder() {
        let failure = crate::annotation_layout::validate(
            Path::new("server/controllers/orders/services/OrdersService.js"),
            "@Service\nclass OrdersService {}\n",
        )
        .expect_err("a role file inside the controller folder")
        .to_string();
        assert_contains(
            &failure,
            "@Service type 'OrdersService' sits outside its realm's shared role folder",
        );
        assert_contains(&failure, "server/services/OrdersService.js");

        let root = project();
        write(
            root.path(),
            "server/services/group/services/GroupService.js",
            "@Service\nclass GroupService {}\n",
        );
        let output = run(root.path())
            .expect("nested role organization")
            .expect("composed service");
        assert_contains(&output, "class GroupService");

        let root = project();
        write(
            root.path(),
            "server/services/MoneyController.js",
            "@RequestRoute(\"/money\")\n@Public\nclass MoneyController {}\n",
        );
        let failure = run_error(root.path());
        assert_contains(&failure, "server/controllers/MoneyController.js");
    }

    #[test]
    fn tac_services_compose_the_shared_client_roles_but_not_their_siblings() {
        let root = tempfile::tempdir().expect("project");
        let entry = "client/services/BasketService.js";
        let source = "@Service()\nexport default class BasketService {}\n";
        write(root.path(), entry, source);
        write_files(
            root.path(),
            &[
                (
                    "client/services/CatalogService.js",
                    "@Service()\nexport default class CatalogService {}\n",
                ),
                (
                    "client/helpers/PricingHelper.js",
                    "@Helper()\nclass PricingHelper {}\n",
                ),
                (
                    "client/stores/BasketStore.js",
                    "@Store()\nclass BasketStore {}\n",
                ),
                (
                    "client/remotes/ProductsClient.js",
                    "@HTTPClient()\nclass ProductsClient {}\n",
                ),
                (
                    "client/entities/ProductEntity.js",
                    "@Entity({\"adapter\":\"data\"})\nclass ProductEntity {}\n",
                ),
                ("client/setups/ApiSetup.js", "@Setup()\nclass ApiSetup {}\n"),
            ],
        );
        let composed = composed_entry(root.path(), entry, source, "tac");
        for role in [
            "PricingHelper",
            "BasketStore",
            "ProductsClient",
            "ProductEntity",
            "ApiSetup",
        ] {
            assert!(composed.contains(&format!("class {role}")), "{role}");
        }
        assert!(!composed.contains("CatalogService"));
        assert!(composed.ends_with(source));
    }

    #[test]
    fn no_sidecar_preserves_plain_sources_and_checks_inline_roles() {
        let root = project();
        assert_eq!(
            run(root.path()).unwrap_or_else(|error| panic!("{error}")),
            None
        );
        let inline_role = assemble(
            root.path(),
            Path::new(Sample::ENTRY),
            "@Service\nclass MoneyService {}",
            "yon",
        );
        assert!(inline_role.is_err());
    }

    #[test]
    fn composes_selected_language_only_in_deterministic_order() {
        let root = project();
        write_files(
            root.path(),
            &[
                (
                    "server/services/OtherService.py",
                    "@Service\nclass OtherService: pass",
                ),
                (
                    "server/services/MoneyService.js",
                    "@Service\nclass MoneyService {}\n",
                ),
            ],
        );
        let result = run(root.path())
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_default();
        assert!(result.starts_with("@Service\nclass MoneyService"));
        assert!(result.ends_with(Sample::CONTROLLER));
    }

    #[test]
    fn rejects_all_legacy_manifests_with_migration_guidance() {
        let root = project();
        for manifest in [
            r#"{"js":[],"js":[]}"#,
            r#"{"unknown":[]}"#,
            r#"{"js":[["server/a.js"]]}"#,
            r#"{"js":["/server/a.js"]}"#,
            r#"{"js":["server/../client/a.js"]}"#,
            r#"{"js":["server/./a.js"]}"#,
            r#"{"js":["server//a.js"]}"#,
            r#"{"js":["server\\a.js"]}"#,
            r#"{"js":["client/a.js"]}"#,
            r#"{"js":["server/a.py"]}"#,
            r#"{"js":["server/a.js","server/a.js"]}"#,
            r#"{"js":["server/controllers/MoneyController.js"]}"#,
            r#"{"swift":[]}"#,
            r#"{"js":[]} true"#,
        ] {
            write(root.path(), "server/controllers/yon.modules.json", manifest);
            assert_contains(&run_error(root.path()), "no longer supported");
        }
    }

    #[test]
    fn rejects_invalid_duplicate_and_oversized_sources() {
        let root = project();
        for bytes in [vec![255], vec![0], vec![b' '; Module::MAX_SOURCE + 1]] {
            write(root.path(), "server/services/MoneyService.js", bytes);
            assert!(run(root.path()).is_err());
        }
        write(
            root.path(),
            "server/services/MoneyService.js",
            "@Service\nclass MoneyService {}\nclass Handler {}\n",
        );
        assert!(run(root.path()).is_err());
    }

    #[test]
    fn discovered_module_count_is_bounded() {
        let root = project();
        for index in 0..=Module::MAX_MODULES {
            write(
                root.path(),
                &format!("server/services/Money{index}Service.js"),
                format!("@Service\nclass Money{index}Service {{}}"),
            );
        }
        assert!(run(root.path()).is_err());
    }

    #[test]
    fn nested_entries_compose_realm_wide_nested_providers_and_rebase_imports() {
        for (entry, source, realm, environment) in [
            (
                "client/services/ui/ShellService.js",
                "@Service\nexport default class ShellService {}",
                "client",
                "tac",
            ),
            (
                "server/controllers/ui/ShellController.js",
                "@RequestRoute(\"/shell\")\n@Public\nclass ShellController {}",
                "server",
                "yon",
            ),
        ] {
            let root = tempfile::tempdir().expect("project");
            write(root.path(), entry, source);
            write(
                root.path(),
                &format!("{realm}/helpers/text/TextHelper.js"),
                "@Helper\nclass TextHelper {}\n",
            );
            write(
                root.path(),
                &format!("{realm}/remotes/http/MoneyClient.js"),
                "@HTTPClient\nclass MoneyClient {}\n",
            );
            let output = composed_entry(root.path(), entry, source, environment);
            assert_contains(&output, "class TextHelper");
            assert_contains(&output, "class MoneyClient");
        }
        let rebased = SharedImports::rebase(
            Path::new("server/controllers/ui/ShellController.js"),
            Path::new("server/services/orders/OrderService.js"),
            "import { OrderCode } from '../../shared/orders.js';",
            "js",
        )
        .expect("nested imports");
        assert_contains(&rebased, "'../../shared/orders.js'");
    }

    #[test]
    fn native_rust_service_entries_include_their_same_language_service_dependencies() {
        let root = tempfile::tempdir().expect("project");
        let entry = "client/services/ui/SnapshotReaderService.rs";
        let original = "#[Service]\n#[NeedsAll]\nstruct SnapshotReaderService { counter: std::sync::Arc<CounterService> }\n";
        write(root.path(), entry, original);
        write(
            root.path(),
            "client/services/state/CounterService.rs",
            "#[Service]\n#[derive(Default)]\nstruct CounterService { value: i32 }\n",
        );
        let consumed = "struct SnapshotReaderService { counter: std::sync::Arc<CounterService> }\n";
        let ordinary =
            assemble(root.path(), Path::new(entry), consumed, "tac").expect("ordinary composition");
        assert!(ordinary.is_none());
        let native = Module::native_source(root.path(), Path::new(entry), consumed)
            .expect("native composition")
            .expect("peer service");
        assert_contains(&native, "struct CounterService");
        assert_contains(&native, "Arc<CounterService>");
    }

    #[test]
    fn nested_entries_end_discovery_inside_shared_role_folders() {
        let root = tempfile::tempdir().expect("project");
        let entry = "client/services/ShellService.js";
        let service = "@Service()\nexport default class ShellService {}\n";
        write(root.path(), entry, service);
        write_files(
            root.path(),
            &[
                ("client/stores/nested/tac.html", "<main></main>"),
                (
                    "client/stores/nested/stores/PrivateStore.js",
                    "@Store\nclass PrivateStore {}",
                ),
                ("client/stores/MoneyStore.js", "@Store\nclass MoneyStore {}"),
            ],
        );
        let output = composed_entry(root.path(), entry, service, "tac");
        assert!(output.contains("MoneyStore"));
        assert!(!output.contains("PrivateStore"));
    }

    fn assert_role_is_discovered(
        entry: &str,
        environment: &str,
        role_path: &str,
        role_source: &str,
        role_name: &str,
    ) {
        let root = tempfile::tempdir().expect("project");
        write(root.path(), entry, "");
        write(root.path(), role_path, role_source);
        let output = composed_entry(root.path(), entry, "", environment);
        assert!(output.contains(role_name), "{entry}");
    }

    #[test]
    fn discovers_role_syntax_for_every_supported_language() {
        for (extension, role) in [
            ("js", "@Service\nclass MoneyService {}"),
            ("ts", "@Service\nclass MoneyService {}"),
            ("py", "@Service\nclass MoneyService:\n    pass"),
            ("php", "<?php\n#[Service]\nclass MoneyService {}"),
            ("java", "@Service\npublic class MoneyService {}"),
            ("kt", "@Service\nclass MoneyService {}"),
            ("cs", "[Service]\nclass MoneyService {}"),
            ("rs", "#[yon::service]\nstruct MoneyService {}"),
            ("go", "// @yon::Service\ntype MoneyService struct{}"),
        ] {
            assert_role_is_discovered(
                &format!("server/controllers/MoneyController.{extension}"),
                "yon",
                &format!("server/services/MoneyService.{extension}"),
                role,
                "MoneyService",
            );
        }
        for (extension, role) in [
            ("swift", "@Store\nfinal class MoneyStore {}"),
            ("kt", "@Store\nclass MoneyStore {}"),
            ("cs", "[Store]\nclass MoneyStore {}"),
            ("rs", "#[tac::store]\nstruct MoneyStore {}"),
        ] {
            assert_role_is_discovered(
                &format!("client/services/ShellService.{extension}"),
                "tac",
                &format!("client/stores/MoneyStore.{extension}"),
                role,
                "MoneyStore",
            );
        }
    }

    #[test]
    fn a_helper_beside_a_native_tac_service_fails_at_its_own_file() {
        let root = tempfile::tempdir().expect("project");
        let entry = "client/services/ShellService.swift";
        write(root.path(), entry, "");
        let helper = "client/helpers/MoneyHelper.swift";
        write(root.path(), helper, "@Helper\nfinal class MoneyHelper {}");
        let failure = assemble(root.path(), Path::new(entry), "", "tac")
            .expect_err("unsupported compiled Helper role");
        let diagnostic = &failure.diagnostics()[0];
        assert_eq!(diagnostic.code.to_string(), "TY1308", "{failure}");
        assert_eq!(diagnostic.spans[0].file, helper, "{failure}");
    }

    #[test]
    fn a_stream_client_beside_a_java_tac_service_fails_at_its_own_file() {
        for (marker, message) in [
            (
                "@StreamRequest",
                "@StreamRequest is not available in Java Tac code",
            ),
            (
                "@SocketRequest",
                "@SocketRequest is not available in Java Tac code",
            ),
        ] {
            let root = tempfile::tempdir().expect("project");
            let entry = "client/services/TicksService.java";
            write(root.path(), entry, "@Service\nclass TicksService {\n}\n");
            let client = "client/remotes/TicksClient.java";
            let source = format!(
                "import models.Tick;\n\n@HTTPClient(\"/ticks\")\ninterface TicksClient {{\n    {marker}\n    void receive(Tick tick);\n}}\n"
            );
            write(root.path(), client, &source);
            let failure = assemble(root.path(), Path::new(entry), "", "tac")
                .expect_err("the browser script holds streams");
            let diagnostic = &failure.diagnostics()[0];
            assert_eq!(diagnostic.code.to_string(), "TY1308", "{failure}");
            assert_eq!(diagnostic.spans[0].file, client, "{failure}");
            let marker_at = source.find(marker).expect("marker");
            let marker_at = u64::try_from(marker_at).expect("offset");
            assert_eq!(diagnostic.spans[0].start, marker_at, "{failure}");
            assert!(diagnostic.message.contains(message), "{failure}");
        }
    }

    #[test]
    fn discovery_rejects_unsupported_realm_languages() {
        for (realm, entry) in [
            ("yon", "server/controllers/MoneyController.swift"),
            ("tac", "client/services/ShellService.php"),
            ("tac", "client/services/ShellService.groovy"),
        ] {
            let root = project();
            let failure = assemble(root.path(), Path::new(entry), "", realm)
                .expect_err("unsupported realm language");
            assert_contains(
                &failure.to_string(),
                "not supported in this Tac or Yon realm",
            );
        }
    }

    #[test]
    fn included_python_roles_accept_supported_typing_names_without_relocating_packages() {
        let path = Path::new("client/helpers/CounterHelper.py");
        for names in [
            "Final",
            "Optional",
            "ClassVar",
            "Annotated, Final, Optional",
            "Final, ClassVar",
            "Any, Protocol, Final, Optional",
        ] {
            let source =
                format!("from typing import {names}\n\n@helper\nclass CounterHelper:\n    pass\n");
            validate_imports(path, &source, "py").expect("supported standard typing declarations");
        }
        for import in [
            "from .counter import Counter",
            "from vendor import Counter",
            "import vendor",
            "from typing import UnknownTypingName",
            "from typing import Optional as Maybe",
            "from typing import ClassVar as Shared",
            "from __future__ import annotations",
        ] {
            let source = format!("{import}\n\n@helper\nclass CounterHelper:\n    pass\n");
            let failure =
                validate_imports(path, &source, "py").expect_err("the import guard remains narrow");
            assert_eq!(failure.diagnostics()[0].code.to_string(), "TY2101");
        }
    }

    #[test]
    fn python_tac_discovers_formal_helpers_with_final_and_optional_annotations() {
        let root = tempfile::tempdir().expect("project");
        let entry = "client/services/ShellService.py";
        let source = "from typing import Annotated\n\n@service\nclass ShellService:\n    counter: Annotated[CounterHelper, needs]\n\n    def value(self) -> int:\n        return self.counter.read()\n";
        write(root.path(), entry, source);
        write(
            root.path(),
            "client/helpers/CounterHelper.py",
            "from typing import Final, Optional\n\n@helper\nclass CounterHelper:\n    _fixed: Final[int] = 1\n    _empty: Optional[str] = None\n\n    def read(self) -> int:\n        return self._fixed\n",
        );
        let composed = composed_entry(root.path(), entry, source, "tac");
        assert!(composed.contains("class CounterHelper"));
        assert!(composed.contains("class ShellService"));
        assert!(composed.contains("from typing import Final, Optional"));
    }

    #[test]
    fn definition_dependencies_precede_consumers_without_sorting_method_calls() {
        for (extension, service) in [
            (
                "py",
                "@Service\nclass MoneyService:\n    store: MoneyStore\n    def label(self):\n        return self.store",
            ),
            ("js", "@Service\nclass MoneyService extends MoneyStore {}"),
            (
                "ts",
                "@Service\nclass MoneyService {\n static defaults = new MoneyStore();\n}",
            ),
        ] {
            let root = tempfile::tempdir().expect("project");
            let entry = format!("server/controllers/MoneyController.{extension}");
            write(root.path(), &entry, "");
            write(
                root.path(),
                &format!("server/services/MoneyService.{extension}"),
                service,
            );
            let store = if extension == "py" {
                "@Store\nclass MoneyStore:\n    def label(self):\n        return MoneyService.__name__"
            } else {
                "@Store\nclass MoneyStore { label() { return MoneyService.name; } }"
            };
            write(
                root.path(),
                &format!("server/stores/MoneyStore.{extension}"),
                store,
            );
            let output = composed_entry(root.path(), &entry, "", "yon");
            assert!(
                output.find("class MoneyStore") < output.find("class MoneyService"),
                "{extension}: {output}"
            );
        }
    }

    #[test]
    fn definition_time_cycles_fail_before_execution() {
        let root = project();
        write_files(
            root.path(),
            &[
                (
                    "server/services/FirstService.js",
                    "@Service\nclass FirstService extends SecondService {}",
                ),
                (
                    "server/services/SecondService.js",
                    "@Service\nclass SecondService extends FirstService {}",
                ),
            ],
        );
        assert_contains(&run_error(root.path()), "definition-time dependency cycle");
    }

    #[test]
    fn direct_static_template_interpolation_fails_without_rejecting_method_strings() {
        let root = project();
        let service = "server/services/MoneyService.js";
        write(
            root.path(),
            service,
            "@Service\nclass MoneyService {\n static value = `${OtherService.value}`;\n}",
        );
        assert_contains(&run_error(root.path()), "direct static field initializers");
        for initializer in [
            " value = `${OtherService.value}`;",
            " method() { return `${OtherService.value}`; }",
            " static method() { return `${OtherService.value}`; }",
            " static method = () => `${OtherService.value}`;",
            " static literal = `escaped \\${OtherService.value}`;",
        ] {
            write(
                root.path(),
                service,
                format!("@Service\nclass MoneyService {{\n{initializer}\n}}"),
            );
            assert!(
                run(root.path()).is_ok(),
                "rejected deferred or literal expression: {initializer}"
            );
        }
    }

    #[test]
    fn python_future_imports_stay_before_composed_declarations() {
        let output = compose(
            Path::new("server/controllers/MoneyController.py"),
            "from __future__ import annotations\nclass Handler: pass\n",
            vec!["class MoneyService: pass\n".into()],
            "py",
            "yon",
        )
        .expect("future import composition");
        assert!(output.starts_with("from __future__ import annotations\n"));
        assert!(output.find("class MoneyService") < output.find("class Handler"));
    }

    #[test]
    fn entry_count_and_directory_depth_are_bounded() {
        let root = project();
        let mut path = String::from("server/services");
        for _ in 0..=Discovery::MAX_DEPTH {
            path.push_str("/nested");
        }
        write(root.path(), &format!("{path}/helper.js"), "");
        assert_contains(&run_error(root.path()), "nesting limit");
        let root = project();
        for index in 0..=Discovery::MAX_ENTRIES {
            write(root.path(), &format!("server/services/file{index}.txt"), "");
        }
        assert_contains(&run_error(root.path()), "4096-entry");
    }

    #[test]
    fn unmarked_helpers_are_refused_even_when_native_syntax_looks_like_annotations() {
        let root = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        for (extension, helper) in [
            (
                "cs",
                "static class Buffers {\n  static byte[] Make(int size) => new byte[size];\n  static object Get(object[] row, int id) => row[id];\n}\n",
            ),
            (
                "py",
                "class Buffers:\n    def scale(self, matrix, size):\n        return matrix @ size\n",
            ),
            (
                "kt",
                "class Buffers {\n  inner class Value {\n    fun owner() = this@Value\n  }\n}\n",
            ),
        ] {
            let entry = format!("server/controllers/MoneyController.{extension}");
            write(root.path(), &entry, "");
            write(
                root.path(),
                &format!("server/helpers/Buffers.{extension}"),
                helper,
            );
            let failure = assemble(root.path(), Path::new(&entry), "", "yon").expect_err(extension);
            assert_contains(&failure.to_string(), "required");
            assert_contains(&failure.to_string().to_ascii_lowercase(), "helper");
        }
    }

    #[test]
    fn preserves_module_paths_in_layout_diagnostics() {
        let root = project();
        write(
            root.path(),
            "server/services/Wrong.js",
            "@Service\nclass MoneyService {}\n",
        );
        let failure = run(root.path())
            .err()
            .unwrap_or_else(|| panic!("expected layout failure"));
        assert!(failure.to_string().contains("server/services/Wrong.js"));
    }

    #[test]
    fn a_tachyon_behaviors_import_is_refused_in_role_files_and_entries() {
        let path = Path::new("client/remotes/UsersClient.ts");
        for source in [
            "import type { BehaviorJsonValue } from 'tachyon:behaviors';",
            "import { CacheSettings, Behaviors } from \"tachyon:behaviors\"\nclass MoneyService {}",
        ] {
            let failure = validate_imports(path, source, "ts")
                .expect_err("the import is refused")
                .to_string();
            assert_contains(&failure, "TY1308");
            assert_contains(
                &failure,
                "client/remotes/UsersClient.ts:1: Tachyon annotations and types are global; delete this import",
            );
        }
        let root = project();
        let entry = "client/services/UsersService.ts";
        let service = "@Service()\nexport default class UsersService {}\n";
        write(root.path(), entry, service);
        write(
            root.path(),
            "client/remotes/UsersClient.ts",
            "\nimport type { BehaviorJsonValue } from 'tachyon:behaviors';\n@HTTPClient()\nclass UsersClient { value: BehaviorJsonValue = null; }\n",
        );
        let failure = assemble(root.path(), Path::new(entry), service, "tac")
            .expect_err("the role file import is refused")
            .to_string();
        assert_contains(
            &failure,
            "client/remotes/UsersClient.ts:2: Tachyon annotations",
        );
        let imported = format!("import {{ Service }} from 'tachyon:behaviors';\n{service}");
        let failure = assemble(root.path(), Path::new(entry), &imported, "tac")
            .expect_err("the entry import is refused")
            .to_string();
        assert_contains(
            &failure,
            "client/services/UsersService.ts:1: Tachyon annotations",
        );
    }

    #[test]
    fn framework_types_compose_without_an_import() {
        let root = project();
        let entry = "client/services/UsersService.ts";
        let service = "@Service()\nexport default class UsersService {}\n";
        write(root.path(), entry, service);
        write(
            root.path(),
            "client/remotes/UsersClient.ts",
            "@HTTPClient()\nclass UsersClient { value: BehaviorJsonValue = null; }\n",
        );
        let source = composed_entry(root.path(), entry, service, "tac");
        assert!(source.contains("class UsersClient"));
        assert!(!source.contains("import"));
    }

    fn merge_modules(path: &Path, modules: &[&str]) -> Result<String, Failure> {
        merge_imports(
            path,
            modules.iter().map(|module| (*module).to_owned()).collect(),
        )
    }

    #[test]
    fn absolute_imports_compose_once_per_specifier() {
        let path = Path::new("client/services/PagerService.js");
        let absolute =
            "import { neighbours } from '/shared/scripts/navigation.js'\nclass PagerService {}\n";
        assert!(validate_imports(path, absolute, "js").is_ok());
        for refused in [
            "import { neighbours } from 'ordinary-package'\n",
            "import { neighbours } from '//cdn.example/navigation.js'\n",
        ] {
            assert!(validate_imports(path, refused, "js").is_err(), "{refused}");
        }
        let merged = merge_modules(
            path,
            &[
                "import { currentPath } from '/shared/scripts/navigation.js'\nclass A {}\n",
                "import { currentPath, slugify } from '/shared/scripts/navigation.js'\nclass B {}\n",
            ],
        )
        .expect("merged");
        let specifier_count = merged
            .matches("from \"/shared/scripts/navigation.js\"")
            .count();
        assert_eq!(specifier_count, 1, "{merged}");
        assert_contains(&merged, "currentPath, slugify");
        assert_contains(&merged, "class A {}");
        assert_contains(&merged, "class B {}");
        let clash = merge_modules(
            path,
            &[
                "import { format } from '/shared/scripts/money.js'\n",
                "import { format } from '/shared/scripts/dates.js'\n",
            ],
        );
        assert!(clash.is_err());
    }

    #[test]
    fn type_imports_compose_once_and_stay_type_only() {
        let path = Path::new("client/tests/TopicServiceTest.ts");
        let merged = merge_modules(
            path,
            &[
                "import type { Guide } from '/models/Guide.ts'\nclass A {}\n",
                "import type { Guide, Section } from '/models/Guide.ts'\nimport { slugify } from '/shared/scripts/navigation.js'\nclass B {}\n",
            ],
        )
        .expect("merged");
        assert_contains(
            &merged,
            "import type { Guide, Section } from \"/models/Guide.ts\";",
        );
        assert_eq!(merged.matches("/models/Guide.ts").count(), 1, "{merged}");
        assert_contains(
            &merged,
            "import { slugify } from \"/shared/scripts/navigation.js\";",
        );
        assert!(!merged.contains("import type from"), "{merged}");
        let default_named_type = merge_modules(
            path,
            &["import type from '/shared/scripts/type.js'\nclass A {}\n"],
        )
        .expect("merged");
        assert_contains(
            &default_named_type,
            "import type from \"/shared/scripts/type.js\";",
        );
    }

    #[test]
    fn an_import_with_and_without_its_extension_names_one_module() {
        for (path, extension) in [
            ("server/controllers/ProbeController.ts", "ts"),
            ("client/services/ProbeService.js", "js"),
        ] {
            let merged = merge_modules(
                Path::new(path),
                &[
                    "import type { ProbeReady } from '/models/ProbeReady'\nclass A {}\n",
                    &format!(
                        "import type {{ ProbeReady }} from '/models/ProbeReady.{extension}'\nclass B {{}}\n"
                    ),
                    &format!("import {{ total }} from '/shared/scripts/count.{extension}'\n"),
                    "import { total } from '/shared/scripts/count'\n",
                ],
            )
            .expect("merged");
            assert_eq!(merged.matches("import type").count(), 1, "{merged}");
            assert_contains(
                &merged,
                "import type { ProbeReady } from \"/models/ProbeReady\";",
            );
            assert_eq!(merged.matches("{ total }").count(), 1, "{merged}");
            assert_contains(
                &merged,
                &format!("import {{ total }} from \"/shared/scripts/count.{extension}\";"),
            );
        }
    }

    #[test]
    fn modules_cannot_relocate_dependency_imports() {
        let path = Path::new("server/services/MoneyService.js");
        for source in [
            "import './other.js';",
            "import { other } from './other.js';",
            "const other = require('./other');",
            "export { other } from './other.js';",
        ] {
            assert!(validate_imports(path, source, "js").is_err(), "{source}");
        }
        for source in [
            "export class MoneyService { transfer(from, to) { return from + to; } }",
            "// import './other.js';\nclass MoneyService { important() {} }",
        ] {
            assert!(validate_imports(path, source, "js").is_ok(), "{source}");
        }
        let rust = validate_imports(
            Path::new("server/services/MoneyService.rs"),
            "mod relocated;",
            "rs",
        );
        assert!(rust.is_err());
        let php = validate_imports(
            Path::new("server/services/MoneyService.php"),
            "<?php require_once 'other.php';",
            "php",
        );
        assert!(php.is_err());
        let go = validate_imports(
            Path::new("server/services/MoneyService.go"),
            "package main\n\nimport \"strings\"\n\ntype MoneyService struct{}\n",
            "go",
        );
        assert!(go.is_ok());
    }

    #[test]
    fn go_role_files_compose_as_separate_files_of_one_program() {
        let root = tempfile::tempdir().expect("project");
        let entry = "server/controllers/MoneyController.go";
        let controller =
            "// @RequestRoute(\"/money\")\n// @Public\ntype MoneyController struct{}\n";
        write(root.path(), entry, controller);
        write(
            root.path(),
            "server/services/MoneyService.go",
            "import \"strings\"\n\n// @Service\ntype MoneyService struct{}\n\nfunc (service *MoneyService) Name() string { return strings.ToUpper(\"x\") }\n",
        );
        let output = composed_entry(root.path(), entry, controller, "yon");
        assert!(output.starts_with(controller), "{output}");
        assert!(
            output.contains(
                "\n//tachyon:file MoneyService server/services/MoneyService.go\nimport \"strings\""
            ),
            "{output}"
        );
    }

    #[test]
    fn native_tac_keeps_companion_first_and_hoists_imports() {
        let source = compose(
            Path::new("client/services/ShellService.swift"),
            "import Foundation\nfinal class ShellService {}",
            vec!["import Foundation\nfinal class MoneyHelper {}".into()],
            "swift",
            "tac",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(source.matches("import Foundation").count(), 1);
        assert!(source.find("ShellService") < source.find("MoneyHelper"));
    }

    #[test]
    fn native_import_hoisting_preserves_literals_and_nested_using_statements() {
        let kotlin = "class Companion {\n val text = \"\"\"\nimport fake.secret\n\"\"\"\n}\n";
        let result = hoist_imports(Path::new("client/services/ShellService.kt"), kotlin, "kt")
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(result.ends_with(kotlin));
        let csharp = "class Companion {\n void run() {\nusing var signal = new Signal();\n}\n}\n";
        let result = hoist_imports(Path::new("client/services/ShellService.cs"), csharp, "cs")
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(result.ends_with(csharp));
    }

    fn compose_php_module(module: &str) -> Result<String, Failure> {
        compose(
            Path::new("server/controllers/MoneyController.php"),
            "<?php class MoneyController {}",
            vec![module.to_owned()],
            "php",
            "yon",
        )
    }

    #[test]
    fn a_repeated_use_line_composes_once() {
        let php = compose_php_module(
            "<?php\nuse Models\\Guide;\nclass MoneyService {}\nuse Models\\Guide;\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(php.matches("use Models\\Guide;").count(), 1, "{php}");
        let rust = "use crate::models::Guide;\nstruct A;\n\nuse crate::models::Guide;\nuse std::{\n    fmt,\n};\nimpl A { fn f() { use crate::models::Guide; } }\n";
        let composed = compose(
            Path::new("server/controllers/MoneyController.rs"),
            rust,
            Vec::new(),
            "rs",
            "yon",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            composed.matches("use crate::models::Guide;").count(),
            2,
            "{composed}"
        );
        assert_eq!(composed.lines().count(), rust.lines().count());
    }

    #[test]
    fn php_has_one_opening_tag_and_no_embedded_html() {
        let source = compose_php_module("<?php class MoneyService {} ?>")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(source.matches("<?php").count(), 1);
        assert!(!source.contains("?>"));
        assert!(compose_php_module("<?php ?>HTML<?php").is_err());
        assert!(
            compose_php_module("<?php class MoneyService { public string $text = '<?xml ?>'; }")
                .is_ok()
        );
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_module_or_directory_symlinks() {
        let root = project();
        let outside = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        write(
            outside.path(),
            "MoneyService.js",
            "@Service\nclass MoneyService {}\n",
        );
        std::os::unix::fs::symlink(outside.path(), root.path().join("server/services"))
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(run(root.path()).is_err());
        let root = project();
        std::fs::create_dir_all(root.path().join("server/services")).expect("role folder");
        std::os::unix::fs::symlink(
            outside.path().join("MoneyService.js"),
            root.path().join("server/services/MoneyService.js"),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert!(run(root.path()).is_err());
    }
}
