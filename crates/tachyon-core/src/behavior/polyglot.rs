use super::{Input, Marker};
use crate::Failure;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;

mod csharp;
mod csharp_model_exchange;
#[cfg(test)]
mod csharp_task_model_tests;
mod csharp_task_models;
pub(crate) use csharp_task_models::CsharpTaskModels;
mod csharp_browser_construction;
pub(crate) use csharp_browser_construction::CsharpBrowserConstruction;
mod csharp_helper_graph;
mod csharp_helper_interfaces;
pub(crate) use csharp_helper_graph::CsharpHelperGraph;
mod csharp_tac_helpers;
use csharp_helper_interfaces::CsharpHelperInterfaces;
use csharp_tac_helpers::CsharpTacHelpers;
#[cfg(test)]
mod csharp_tac_helper_tests;
#[cfg(test)]
mod csharp_test_construction_tests;
mod dart;
mod dart_test_construction;
mod detached_csharp;
pub(crate) use detached_csharp::DetachedCsharp;
mod planned_method;
pub(crate) use planned_method::PlannedMethod;
mod planned_type;
pub(crate) use planned_type::PlannedType;
mod prepared;
use prepared::Prepared;
mod go;
mod go_syntax;
mod go_test_construction;
mod go_typed_exchange;
mod java;
mod java_builtins;
mod java_test_construction;
pub(crate) use java_builtins::JavaBuiltins;
mod kotlin;
mod kotlin_builtins;
mod kotlin_test_construction;
mod lua_behavior_method;
mod lua_behavior_plan;
mod typed_test_construction;
mod typed_test_fakes;
pub(crate) use kotlin_builtins::KotlinBuiltins;
pub(crate) use lua_behavior_method::LuaBehaviorMethod;
pub(crate) use lua_behavior_plan::LuaBehaviorPlan;
mod model_schema;
pub(crate) use model_schema::ModelSchema;
mod native_storage;
pub(crate) use native_storage::NativeStorage;
#[cfg(test)]
mod parser_tests;
mod php;
mod python;
mod python_fields;
use python_fields::PythonFields;
mod python_members;
pub(crate) use python_members::PythonMembers;
#[cfg(test)]
mod python_members_tests;
mod query_frontend;
mod ruby;
mod ruby_syntax;
mod ruby_typed_answer;
mod rust;
mod rust_async_client;
#[cfg(test)]
mod rust_runtime_map_tests;
mod rust_test_construction;
mod string_literal;
mod swift;
mod swift_platform;
mod test_clients;
pub(crate) use swift_platform::SwiftPlatform;

#[derive(Clone, Debug)]
pub(super) struct Parameter {
    pub name: String,
    pub type_source: String,
}

#[derive(Clone, Debug)]
pub(super) struct Field {
    pub name: String,
    pub type_source: String,
    pub uninitialized: bool,
    pub initializer: Option<String>,
    pub is_static: bool,
}

#[derive(Clone, Debug)]
pub(super) struct Method {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub signature: Range<usize>,
    pub body: Range<usize>,
    pub return_type: String,
    pub asynchronous: bool,
    pub is_static: bool,
    pub declared: bool,
    pub metadata: serde_json::Map<String, Value>,
}

#[derive(Clone, Debug)]
pub(super) struct Type {
    pub name: String,
    pub declaration: usize,
    pub keyword: Range<usize>,
    pub declared: bool,
    pub body: Range<usize>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub configuration: bool,
    pub cache: Option<Value>,
    pub entity: Option<Value>,
    pub table: Option<String>,
    pub constructor: Option<Vec<String>>,
    pub repository: Option<Value>,
    pub policies: serde_json::Map<String, Value>,
}

#[derive(Clone, Debug)]
pub(super) struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Clone, Debug)]
pub(super) struct Module {
    pub source: String,
    pub language: String,
    pub environment: String,
    #[allow(clippy::struct_field_names)]
    pub module_id: String,
    pub types: Vec<Type>,
    pub edits: Vec<Edit>,
}

pub(super) fn render(
    module: &Module,
    mut edits: Vec<Edit>,
    suffix: &str,
) -> Result<String, String> {
    edits.extend(module.edits.iter().cloned());
    edits.sort_by_key(|edit| std::cmp::Reverse((edit.range.start, edit.range.end)));
    let mut previous_start = module.source.len();
    let mut result = module.source.clone();
    for edit in edits {
        if edit.range.start > edit.range.end
            || edit.range.end > previous_start
            || !result.is_char_boundary(edit.range.start)
            || !result.is_char_boundary(edit.range.end)
        {
            return Err("Overlapping or invalid behavior source edits".to_owned());
        }
        previous_start = edit.range.start;
        result.replace_range(edit.range, &edit.replacement);
    }
    result.push('\n');
    result.push_str(suffix);
    Ok(result)
}

fn blank_out(text: &str) -> String {
    let mut blanked = String::with_capacity(text.len());
    for byte in text.bytes() {
        if matches!(byte, b'\n' | b'\r') {
            blanked.push(char::from(byte));
        } else {
            blanked.push(' ');
        }
    }
    blanked
}

fn erasures(
    source: &str,
    markers: &[Marker],
    schedules: &[Marker],
    types: &[Type],
    language: &str,
    environment: &str,
) -> Vec<Edit> {
    let mut edits = Vec::new();
    let ruby_test_route = language == "rb" && environment == "yon";
    for marker in markers.iter().chain(schedules) {
        if marker.parameter {
            continue;
        }
        let replacement = if ruby_test_route && marker.name == "Test" {
            super::testing::controller_marker(language).to_owned()
        } else {
            blank_out(&source[marker.span.clone()])
        };
        edits.push(Edit {
            range: marker.span.clone(),
            replacement,
        });
    }
    if environment != "yon" || ruby_test_route {
        return edits;
    }
    for ty in types {
        if !ty.policies.contains_key("test") {
            continue;
        }
        edits.push(Edit {
            range: ty.declaration..ty.declaration,
            replacement: format!("{}\n", super::testing::controller_marker(language)),
        });
    }
    edits
}

pub(super) fn lower(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Option<String>, Failure> {
    let Some(prepared) = prepare(path, source, environment)? else {
        return Ok(None);
    };
    match prepared {
        Prepared::Erased(erased) => Ok(Some(erased)),
        Prepared::Annotated(module) => lower_module(&module)
            .map(Some)
            .map_err(|message| module_failure(path, source, message)),
    }
}

pub(super) fn unit_tables(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Vec<super::UnitTable>, Failure> {
    Ok(prepare(path, source, environment)?
        .map_or_else(Vec::new, |prepared| annotated_units(&prepared)))
}

fn annotated_units(prepared: &Prepared) -> Vec<super::UnitTable> {
    match prepared {
        Prepared::Annotated(module) => module_units(module),
        Prepared::Erased(_) => Vec::new(),
    }
}

fn module_units(module: &Module) -> Vec<super::UnitTable> {
    module
        .types
        .iter()
        .filter_map(|ty| Some(super::UnitTable::of(&super::testing::polyglot_unit(ty)?)))
        .collect()
}

pub(super) fn detached_csharp(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Option<DetachedCsharp>, Failure> {
    let Some(Prepared::Annotated(module)) = prepare(path, source, environment)? else {
        return Ok(None);
    };
    DetachedCsharp::of(&module)
        .map(Some)
        .map_err(|message| module_failure(path, source, message))
}

pub(super) fn csharp_case_calls(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Vec<(String, String)>, Failure> {
    let prepared = prepare(path, source, environment)?;
    Ok(annotated_case_calls(prepared.as_ref()))
}

fn annotated_case_calls(prepared: Option<&Prepared>) -> Vec<(String, String)> {
    let Some(Prepared::Annotated(module)) = prepared else {
        return Vec::new();
    };
    module
        .types
        .iter()
        .filter_map(super::testing::polyglot_unit)
        .flat_map(|unit| super::ValueSource::csharp_calls(&unit.cases, &unit.inputs))
        .collect()
}

fn module_failure(path: &Path, source: &str, message: String) -> Failure {
    let input = Input {
        path,
        source,
        code: String::new(),
    };
    input.failure(0, message)
}

fn prepare(path: &Path, source: &str, environment: &str) -> Result<Option<Prepared>, Failure> {
    let language = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mask = crate::lexical::code_mask(language, source);
    let language = crate::groovy_source::GroovySource::syntax(language);
    let mut input = Input {
        path,
        source,
        code: visible_code(source, mask.code),
    };
    let markers = annotations(&input, language, environment)?;
    let markers = ruby_syntax::RubySyntax::lowered_markers(language, environment, markers);
    if markers.is_empty() {
        return Ok(None);
    }
    let plain = python_plain(&input, language);
    let source = plain
        .as_ref()
        .map_or(source, |(plain_source, _)| plain_source.as_str());
    if let Some((_, plain_code)) = plain.as_ref() {
        input = Input {
            path,
            source,
            code: plain_code.clone(),
        };
    }
    let (schedules, markers): (Vec<_>, Vec<_>) = markers
        .into_iter()
        .partition(|marker| marker.name == "Schedule" || super::StoredState::marks(&marker.name));
    if markers.is_empty() {
        return Ok(Some(Prepared::Erased(erase(source, &schedules))));
    }
    check_behavior_source(&input, mask.error, environment)?;
    CsharpTacHelpers::captured(&input, language, environment, &markers)?;
    mask_annotations(&mut input, &markers, language);
    let types = behavior_types(&input, language, environment, &markers)?;
    CsharpTacHelpers::constructed(&input, language, environment, &types)?;
    let edits = erasures(source, &markers, &schedules, &types, language, environment);
    let marked_parameters: Vec<Marker> = markers
        .iter()
        .filter(|marker| marker.parameter)
        .cloned()
        .collect();
    Ok(Some(Prepared::Annotated(Module {
        source: erase(source, &marked_parameters),
        language: language.to_owned(),
        environment: environment.to_owned(),
        module_id: super::module_id(path),
        types,
        edits,
    })))
}

fn visible_code(source: &str, visibility: Vec<bool>) -> String {
    let mut code = Vec::with_capacity(source.len());
    for (byte, visible) in source.bytes().zip(visibility) {
        if visible || matches!(byte, b'\n' | b'\r') {
            code.push(byte);
        } else {
            code.push(b' ');
        }
    }
    String::from_utf8(code).unwrap_or_default()
}

fn python_plain(input: &Input<'_>, language: &str) -> Option<(String, String)> {
    if language != "py" {
        return None;
    }
    let found =
        crate::python_parameters::PythonAnnotated::markers(&input.code, input.source, |written| {
            python_marker(written).is_some()
        });
    if found.markers.is_empty() {
        return None;
    }
    Some((found.blanked(input.source), found.blanked(&input.code)))
}

fn check_behavior_source(
    input: &Input<'_>,
    mask_error: Option<&'static str>,
    environment: &str,
) -> Result<(), Failure> {
    if input.source.len() > 1024 * 1024 {
        return Err(input.failure(0, "Annotated behavior source exceeds 1 MiB"));
    }
    if let Some(error) = mask_error {
        return Err(input.failure(0, error));
    }
    if !matches!(environment, "tac" | "yon") {
        return Err(input.failure(0, "Unknown behavior environment"));
    }
    Ok(())
}

fn behavior_types(
    input: &Input<'_>,
    language: &str,
    environment: &str,
    markers: &[Marker],
) -> Result<Vec<Type>, Failure> {
    let mut types = declarations(input, language)?;
    apply_markers(input, language, environment, &mut types, markers)?;
    for ty in &mut types {
        if let Some(entity) = ty.entity.as_mut() {
            super::fill_entity_identifier(input, ty.declaration, &ty.policies, entity)?;
        }
    }
    query_frontend::resolve(input, language, &mut types)?;
    resolve_policies(input, environment, &mut types)?;
    supply_declared(input, language, &mut types)?;
    record_cases(input, language, &mut types);
    types.retain(has_behavior);
    if language == "swift" {
        discover_swift_configurations(input, &mut types)?;
    }
    if language == "dart" {
        discover_dart_configurations(input, &mut types)?;
    }
    for ty in &types {
        validate_type(input, language, ty)?;
    }
    Ok(types)
}

fn has_behavior(ty: &Type) -> bool {
    ty.configuration
        || ty.cache.is_some()
        || ty.entity.is_some()
        || ty.table.is_some()
        || ty.constructor.is_some()
        || ty.repository.is_some()
        || !ty.policies.is_empty()
        || !ty.methods.is_empty()
}

fn lower_module(module: &Module) -> Result<String, String> {
    match module.language.as_str() {
        "java" => java::lower(module),
        "kt" => kotlin::lower(module),
        "cs" => csharp::lower(module),
        "swift" => swift::lower(module),
        "rs" => rust::lower(module),
        "py" => python::lower(module),
        "php" => php::lower(module),
        "dart" => dart::lower(module),
        "go" => go::lower(module),
        "rb" => ruby::RubyLowering::lower(module),
        _ => Err("This language has no behavior execution adapter".to_owned()),
    }
}

fn erase(source: &str, markers: &[Marker]) -> String {
    let mut output = source.to_owned();
    for marker in markers.iter().rev() {
        let blanked = blank_out(&source[marker.span.clone()]);
        output.replace_range(marker.span.clone(), &blanked);
    }
    output
}

fn mask_annotations(input: &mut Input<'_>, markers: &[Marker], language: &str) {
    for marker in markers {
        let mut masked = String::new();
        for (offset, byte) in input.code[marker.span.clone()].bytes().enumerate() {
            let keep = matches!(byte, b'\n' | b'\r')
                || (language == "py" && offset == 0 && byte == b'@' && !marker.parameter);
            if keep {
                masked.push(char::from(byte));
            } else {
                masked.push(' ');
            }
        }
        input.code.replace_range(marker.span.clone(), &masked);
    }
}

fn resolve_policies(
    input: &Input<'_>,
    environment: &str,
    types: &mut [Type],
) -> Result<(), Failure> {
    let schemas = validated_schemas(types);
    for ty in types {
        super::configuration::derive(
            input,
            ty.declaration,
            &mut ty.policies,
            &settings_fields(&ty.fields),
        )?;
        super::configuration::validate(
            input,
            ty.declaration,
            &ty.policies,
            ty.configuration,
            environment,
        )?;
        super::validation::identifier_agrees(
            input,
            ty.declaration,
            &ty.policies,
            ty.entity.as_ref(),
        )?;
        resolve_method_policies(input, ty, &schemas)?;
    }
    Ok(())
}

fn settings_fields(fields: &[Field]) -> Vec<super::configuration::SettingsField> {
    let mut result = Vec::new();
    for field in fields {
        result.push(super::configuration::SettingsField {
            name: field.name.clone(),
            type_source: field.type_source.clone(),
            initializer: field.initializer.clone(),
            is_static: field.is_static,
        });
    }
    result
}

fn validated_schemas(types: &[Type]) -> BTreeMap<String, Value> {
    let mut schemas = BTreeMap::new();
    for ty in types {
        if let Some(schema) = ty.policies.get("validated") {
            schemas.insert(ty.name.clone(), schema.clone());
        }
    }
    schemas
}

fn resolve_method_policies(
    input: &Input<'_>,
    ty: &mut Type,
    schemas: &BTreeMap<String, Value>,
) -> Result<(), Failure> {
    let mut methods = BTreeMap::new();
    for method in &ty.methods {
        let mut metadata = method.metadata.clone();
        super::parameters::finish(input, method.signature.start, &mut metadata)?;
        let mut names = Vec::new();
        for parameter in &method.parameters {
            names.push(Value::String(parameter.name.clone()));
        }
        metadata.insert("parameterNames".to_owned(), Value::Array(names));
        methods.insert(method.name.clone(), metadata);
    }
    super::mutations::resolve(input, ty.declaration, &mut methods)?;
    for method in &mut ty.methods {
        method.metadata = methods.remove(&method.name).unwrap_or_default();
        super::validation::resolve(input, method.signature.start, &mut method.metadata, schemas)?;
    }
    Ok(())
}

fn has_empty_body(input: &Input<'_>, language: &str, method: &Method) -> bool {
    let body = input.code[method.body.clone()].trim();
    method.declared || body.is_empty() || (language == "py" && matches!(body, "pass" | "..."))
}

fn reject_http_verb(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
    message: &str,
) -> Result<(), Failure> {
    if super::Method::HTTP_VERBS.contains(&method.name.as_str()) {
        return Err(input.failure(marker.span.start, message));
    }
    Ok(())
}

pub(crate) struct HelperRole;

impl HelperRole {
    pub(crate) fn keeps_tac_marker(path: &Path) -> bool {
        CsharpTacHelpers::retains_marker(path)
    }

    fn refusal(path: &Path) -> Option<String> {
        let language = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        crate::annotation_rules::AnnotationRules::unavailable("Helper", language, "tac")
    }

    pub(crate) fn require_callers(path: &Path, source: &str) -> Result<(), Failure> {
        crate::fsharp_source::FsharpTacHelpers::callers(path, source)?;
        CsharpTacHelpers::callers(path, source)
    }

    pub(crate) fn refuse_in_tac(path: &Path, source: &str) -> Result<(), Failure> {
        crate::fsharp_source::FsharpTacHelpers::source(path, source)?;
        CsharpTacHelpers::source(path, source)?;
        let Some(message) = Self::refusal(path) else {
            return Ok(());
        };
        let language = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let mask = crate::lexical::code_mask(language, source);
        let language = crate::groovy_source::GroovySource::syntax(language);
        let input = Input {
            path,
            source,
            code: visible_code(source, mask.code),
        };
        for marker in annotations(&input, language, "tac")? {
            if marker.name == "Helper" {
                return Err(input.failure(marker.span.start, message));
            }
        }
        Ok(())
    }
}

struct GatewayLanguages;

impl GatewayLanguages {
    const SUPPORTED: [&str; 9] = ["py", "php", "java", "kt", "cs", "rs", "dart", "go", "rb"];
}

fn gateway_method_policy(
    input: &Input<'_>,
    language: &str,
    method: &Method,
    marker: &Marker,
    owner: &str,
) -> Result<Value, Failure> {
    if !GatewayLanguages::SUPPORTED.contains(&language) {
        return Err(input.failure(
            marker.span.start,
            format!("Gateway is not available in .{language} sources yet"),
        ));
    }
    let mut parameter_types = Vec::new();
    for parameter in &method.parameters {
        parameter_types.push(parameter.type_source.as_str());
    }
    let declaration = super::gateway::GatewayMethod {
        owner,
        name: &method.name,
        language,
        parameter_types,
        result_type: &method.return_type,
        has_body: !has_empty_body(input, language, method),
    };
    super::gateway::policy(input, marker, &declaration)
}

pub(crate) fn gateway_unit_runtime(extension: &str, unit_source: &str) -> Option<String> {
    match extension {
        "cs" if !unit_source.contains("public sealed class BehaviorJSON {") => {
            Some(csharp::runtime("gateway", "yon", "{}", "", "", ""))
        }
        "rs" if !unit_source.contains("pub enum BehaviorValue {") => {
            Some(rust::runtime("gateway", "yon"))
        }
        "go" if !unit_source.contains("\ntype BehaviorValue = any\n") => {
            Some(go::runtime("gateway", "yon"))
        }
        _ => None,
    }
}

pub(super) fn gateway_policies(path: &Path, source: &str) -> Result<Vec<Value>, Failure> {
    let language = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mask = crate::lexical::code_mask(language, source);
    let language = crate::groovy_source::GroovySource::syntax(language);
    let input = Input {
        path,
        source,
        code: visible_code(source, mask.code),
    };
    let markers = annotations(&input, language, "yon")?;
    if !markers.iter().any(|marker| marker.name == "Gateway") {
        return Ok(Vec::new());
    }
    let types = declarations(&input, language)?;
    let mut policies = Vec::new();
    for marker in markers.iter().filter(|marker| marker.name == "Gateway") {
        let Some(owner) = method_owner(&input, language, &types, marker.target) else {
            continue;
        };
        let method = parse_method(&input, language, marker.target, &types[owner])?;
        let policy = gateway_method_policy(&input, language, &method, marker, &types[owner].name)?;
        policies.push(policy);
    }
    Ok(policies)
}

pub(super) fn target_signatures(
    path: &Path,
    source: &str,
    class: &str,
    method_name: &str,
) -> Result<Vec<super::gateway::TargetSignature>, Failure> {
    let language = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mask = crate::lexical::code_mask(language, source);
    let language = crate::groovy_source::GroovySource::syntax(language);
    let input = Input {
        path,
        source,
        code: visible_code(source, mask.code),
    };
    let lowered_runtime_allowed = matches!(language, "java" | "kt" | "cs" | "rs" | "dart" | "go");
    let types = declarations_with_runtime(&input, language, lowered_runtime_allowed)?;
    let Some(owner) = types.iter().position(|ty| ty.name == class) else {
        return Ok(Vec::new());
    };
    if language == "go" {
        return Ok(go_syntax::target_signatures(
            &input,
            &types[owner],
            method_name,
        ));
    }
    let mut signatures = Vec::new();
    let mut offset = 0;
    for line in input.code.split_inclusive('\n') {
        let at = offset + line.len() - line.trim_start().len();
        offset += line.len();
        if !declares_method_named(line, method_name) {
            continue;
        }
        if method_owner(&input, language, &types, at) != Some(owner) {
            continue;
        }
        let Ok(method) = parse_method(&input, language, at, &types[owner]) else {
            let signature = match language {
                "kt" => kotlin_expression_signature(&input, at),
                "dart" => dart_expression_signature(&input, at, method_name),
                _ => None,
            };
            if let Some(signature) = signature {
                signatures.push(signature);
            }
            continue;
        };
        signatures.push(signature_of(&input, language, &method));
    }
    if language == "py" {
        return Ok(last_definition_only(signatures));
    }
    Ok(signatures)
}

fn last_definition_only(
    mut signatures: Vec<super::gateway::TargetSignature>,
) -> Vec<super::gateway::TargetSignature> {
    match signatures.pop() {
        Some(last) => vec![last],
        None => Vec::new(),
    }
}

fn kotlin_expression_signature(
    input: &Input<'_>,
    at: usize,
) -> Option<super::gateway::TargetSignature> {
    let open = at + input.code[at..].find('(')?;
    let close = super::matching(&input.code, open, b'(', b')')?;
    let rest = &input.code[close + 1..];
    let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
    let (result, _) = line.split_once('=')?;
    let result_type = result.trim().strip_prefix(':').unwrap_or("").trim();
    let (parameters, _) =
        method_parameters(input, "kt", at, &input.source[open + 1..close]).ok()?;
    let mut parameter_types = Vec::new();
    for parameter in parameters {
        parameter_types.push(parameter.type_source);
    }
    Some(super::gateway::TargetSignature {
        parameter_types,
        result_type: result_type.to_owned(),
        is_static: false,
        has_body: true,
        has_supported_receiver: true,
    })
}

fn dart_expression_signature(
    input: &Input<'_>,
    at: usize,
    name: &str,
) -> Option<super::gateway::TargetSignature> {
    let open = at + input.code[at..].find('(')?;
    let close = super::matching(&input.code, open, b'(', b')')?;
    let rest = &input.code[close + 1..];
    let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
    if !line.contains("=>") {
        return None;
    }
    let declaration = input.code[at..open].trim().strip_suffix(name)?.trim();
    let (is_static, result_type) = match declaration.strip_prefix("static ") {
        Some(result_type) => (true, result_type.trim()),
        None => (false, declaration),
    };
    let (parameters, _) =
        method_parameters(input, "dart", at, &input.source[open + 1..close]).ok()?;
    let mut parameter_types = Vec::new();
    for parameter in parameters {
        parameter_types.push(parameter.type_source);
    }
    Some(super::gateway::TargetSignature {
        parameter_types,
        result_type: result_type.to_owned(),
        is_static,
        has_body: true,
        has_supported_receiver: true,
    })
}

fn declares_method_named(line: &str, name: &str) -> bool {
    let Some(open) = line.find('(') else {
        return false;
    };
    line[..open].split_whitespace().last() == Some(name)
}

fn signature_of(
    input: &Input<'_>,
    language: &str,
    method: &Method,
) -> super::gateway::TargetSignature {
    let mut parameter_types = Vec::new();
    for parameter in &method.parameters {
        parameter_types.push(parameter.type_source.clone());
    }
    super::gateway::TargetSignature {
        parameter_types,
        result_type: method.return_type.clone(),
        is_static: method.is_static,
        has_body: !has_empty_body(input, language, method),
        has_supported_receiver: language != "rs" || has_shared_receiver(input, method),
    }
}

fn has_shared_receiver(input: &Input<'_>, method: &Method) -> bool {
    let signature = &input.source[method.signature.clone()];
    let Some((_, parameters)) = signature.split_once('(') else {
        return false;
    };
    parameters.split([',', ')']).next().map(str::trim) == Some("&self")
}

fn method_policy(
    input: &Input<'_>,
    language: &str,
    method: &Method,
    marker: &Marker,
    owner: &str,
) -> Result<(&'static str, Value), Failure> {
    let policy = match marker.name.as_str() {
        "Cached" => {
            reject_http_verb(
                input,
                method,
                marker,
                "Cached cannot wrap HTTP verb endpoints; authorize before calling a service",
            )?;
            let count = method.parameters.len();
            ("cacheable", super::cacheable(input, marker, count)?)
        }
        "Query" => ("query", super::queries::explicit(input, marker)?),
        "Check" | "PageDefaults" | "CacheKey" => {
            return Err(super::parameters::refuse_method_form(input, marker));
        }
        "ClearCache" => ("cacheEvict", super::mutations::policy(input, marker)?),
        "UpdateCache" => ("cachePut", super::mutations::policy(input, marker)?),
        "Retry" | "MaxAtOnce" | "AllOrNothing" => {
            reject_http_verb(
                input,
                method,
                marker,
                "This annotation belongs on a service method, never an HTTP verb endpoint",
            )?;
            super::resilience_policy(input, language, marker)?
        }
        "WebCall" => {
            let body = &input.code[method.body.clone()];
            (
                "httpExchange",
                super::exchange_policy(input, marker, body, language)?,
            )
        }
        "Convert" => {
            if !has_empty_body(input, language, method) {
                return Err(input.failure(
                    marker.span.start,
                    "Convert writes the whole method; declare its signature with an empty body",
                ));
            }
            let mut parameter_types = Vec::new();
            for parameter in &method.parameters {
                parameter_types.push(parameter.type_source.as_str());
            }
            let signature = super::mapper::ConvertSignature {
                method: format!("{owner}.{}", method.name),
                parameter_types,
                return_type: &method.return_type,
            };
            (
                "mapping",
                super::mapper::mapping(input, marker, &signature)?,
            )
        }
        "Gateway" => (
            "gateway",
            gateway_method_policy(input, language, method, marker, owner)?,
        ),
        _ => {
            return Err(input.failure(
                marker.span.start,
                crate::annotation_rules::AnnotationRules::belongs(&marker.name),
            ));
        }
    };
    Ok(policy)
}

fn apply_markers(
    input: &Input<'_>,
    language: &str,
    environment: &str,
    types: &mut [Type],
    markers: &[Marker],
) -> Result<(), Failure> {
    let mut seen = BTreeSet::new();
    for marker in markers {
        if !seen.insert((marker.target, marker.name.clone())) {
            return Err(input.failure(marker.span.start, "Duplicate behavior annotation"));
        }
        apply_marker(input, language, environment, types, marker)?;
    }
    Ok(())
}

fn apply_marker(
    input: &Input<'_>,
    language: &str,
    environment: &str,
    types: &mut [Type],
    marker: &Marker,
) -> Result<(), Failure> {
    if let Some(owner) = types.iter().position(|ty| ty.declaration == marker.target) {
        return apply_type(input, environment, &mut types[owner], marker);
    }
    if matches!(marker.name.as_str(), "Needs" | "Named") {
        let owner = field_owner(input, language, types, marker.target)?;
        return autowired_field(input, language, &mut types[owner], marker);
    }
    if marker.name == "Fake" {
        let owner = field_owner(input, language, types, marker.target)?;
        let name = super::validation::field_name(input, marker, language)?;
        return super::testing::mocked(input, marker, &name, &mut types[owner].policies);
    }
    if super::validation::field_marker(&marker.name) {
        let owner = field_owner(input, language, types, marker.target)?;
        return super::validation::apply_field(input, marker, language, &mut types[owner].policies);
    }
    if marker.parameter {
        return apply_parameter_marker(input, language, types, marker);
    }
    if marker.name == super::ValueSource::NAME {
        return apply_value_source(input, language, types, marker);
    }
    apply_method_marker(input, language, types, marker)
}

fn apply_value_source(
    input: &Input<'_>,
    language: &str,
    types: &mut [Type],
    marker: &Marker,
) -> Result<(), Failure> {
    let owner = method_owner(input, language, types, marker.target).ok_or_else(|| {
        input.failure(
            marker.span.start,
            "ValueSource goes on a test case of the Test class",
        )
    })?;
    let ty = &mut types[owner];
    let method = parse_method(input, language, marker.target, ty)?;
    if matches!(
        language,
        "rs" | "java" | "kt" | "cs" | "go" | "dart" | "swift"
    ) {
        return super::ValueSource::record_scalars(input, marker, &method.name, &mut ty.policies);
    }
    super::ValueSource::record(input, marker, &method.name, &mut ty.policies)
}

fn apply_parameter_marker(
    input: &Input<'_>,
    language: &str,
    types: &mut [Type],
    marker: &Marker,
) -> Result<(), Failure> {
    let parameter = super::parameters::marked_parameter(input, marker)?;
    let owner = method_owner(input, language, types, parameter.declaration).ok_or_else(|| {
        input.failure(
            marker.span.start,
            format!(
                "@{} marks a parameter of a method in a top-level type",
                marker.name
            ),
        )
    })?;
    let ty = &mut types[owner];
    let method = parse_method(input, language, parameter.declaration, ty)?;
    if method.is_static {
        return Err(input.failure(
            marker.span.start,
            "Behavior method annotations require instance methods",
        ));
    }
    let declared_type = method
        .parameters
        .get(parameter.index)
        .map_or("", |declared| declared.type_source.as_str());
    let policy = super::parameters::policy(input, marker, parameter.index, declared_type)?;
    let position = ty
        .methods
        .iter()
        .position(|existing| existing.signature.start == method.signature.start);
    let overloaded = ty
        .methods
        .iter()
        .any(|existing| existing.name == method.name);
    if position.is_none() && overloaded {
        return Err(input.failure(
            parameter.declaration,
            "Overloaded behavior method names are unsupported",
        ));
    }
    let index = position.unwrap_or_else(|| {
        ty.methods.push(method);
        ty.methods.len() - 1
    });
    let metadata = &mut ty.methods[index].metadata;
    super::parameters::record(input, marker.span.start, metadata, policy)
}

fn apply_method_marker(
    input: &Input<'_>,
    language: &str,
    types: &mut [Type],
    marker: &Marker,
) -> Result<(), Failure> {
    let owner = method_owner(input, language, types, marker.target).ok_or_else(|| {
        input.failure(
            marker.span.start,
            "Behavior annotations require a top-level type or its direct instance method",
        )
    })?;
    let ty = &mut types[owner];
    let mut method = parse_method(input, language, marker.target, ty)?;
    if method.is_static {
        return Err(input.failure(
            marker.span.start,
            "Behavior method annotations require instance methods",
        ));
    }
    let (key, metadata) = method_policy(input, language, &method, marker, &ty.name)?;
    let existing = ty
        .methods
        .iter_mut()
        .find(|existing| existing.signature.start == method.signature.start);
    if let Some(existing) = existing {
        existing.metadata.insert(key.to_owned(), metadata);
        return Ok(());
    }
    if ty
        .methods
        .iter()
        .any(|existing| existing.name == method.name)
    {
        return Err(input.failure(
            marker.target,
            "Overloaded behavior method names are unsupported",
        ));
    }
    method.metadata.insert(key.to_owned(), metadata);
    ty.methods.push(method);
    Ok(())
}

fn field_owner(
    input: &Input<'_>,
    language: &str,
    types: &[Type],
    target: usize,
) -> Result<usize, Failure> {
    let owner = if matches!(language, "rs" | "go") {
        types.iter().position(|ty| {
            target >= ty.body.start
                && target < ty.body.end
                && brace_depth(&input.code[ty.body.start..target]) == 0
        })
    } else {
        method_owner(input, language, types, target)
    };
    owner.ok_or_else(|| input.failure(target, "Field annotations require a direct instance field"))
}

fn autowired_field(
    input: &Input<'_>,
    language: &str,
    ty: &mut Type,
    marker: &Marker,
) -> Result<(), Failure> {
    let name = super::validation::field_name(input, marker, language)?;
    if marker.name == "Named" {
        return qualify_field(input, ty, marker, name);
    }
    require_no_arguments(input, marker)?;
    let fields = ty.constructor.get_or_insert_with(Vec::new);
    if fields.contains(&name) {
        return Err(input.failure(marker.span.start, "One Needs per field"));
    }
    fields.push(name);
    Ok(())
}

fn qualify_field(
    input: &Input<'_>,
    ty: &mut Type,
    marker: &Marker,
    name: String,
) -> Result<(), Failure> {
    let provider = marker.arguments.trim().trim_matches(['"', '\'']).trim();
    if !super::identifier(provider) {
        return Err(input.failure(marker.span.start, "Named names one provider type"));
    }
    let qualifiers = ty
        .policies
        .entry("qualifiers")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Value::Object(qualifiers) = qualifiers else {
        return Ok(());
    };
    let previous = qualifiers.insert(name, Value::String(provider.to_owned()));
    if previous.is_some() {
        return Err(input.failure(marker.span.start, "One Named per field"));
    }
    Ok(())
}

fn is_configure_signature(method: &Method) -> bool {
    method.is_static
        && !method.asynchronous
        && method.parameters.len() == 1
        && method.parameters[0].type_source == "BehaviorExecution"
        && method.return_type == "BehaviorAdapters"
}

fn discover_dart_configurations(input: &Input<'_>, types: &mut [Type]) -> Result<(), Failure> {
    for ty in types.iter_mut().filter(|ty| ty.configuration) {
        let offsets = dart_configure_offsets(&input.code[ty.body.clone()]);
        if offsets.len() != 1 {
            return Err(input.failure(
                ty.declaration,
                "Dart Setup requires one static configure method",
            ));
        }
        let start = line_declaration(&input.code, ty.body.start + offsets[0]);
        let method = parse_method(input, "dart", start, ty)?;
        if !is_configure_signature(&method) {
            return Err(input.failure(
                start,
                "Dart configure must be static BehaviorAdapters configure(BehaviorExecution execution), synchronous",
            ));
        }
        ty.methods.push(method);
    }
    Ok(())
}

fn dart_configure_offsets(body: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    for (at, word) in tokens(body) {
        if word != "configure" {
            continue;
        }
        let after_name = body[at + word.len()..].trim_start();
        if after_name.starts_with('(') && brace_depth(&body[..at]) == 0 {
            offsets.push(at);
        }
    }
    offsets
}

fn discover_swift_configurations(input: &Input<'_>, types: &mut [Type]) -> Result<(), Failure> {
    for ty in types.iter_mut().filter(|ty| ty.configuration) {
        let offsets = swift_configure_offsets(&input.code[ty.body.clone()]);
        if offsets.len() != 1 {
            return Err(input.failure(
                ty.declaration,
                "Swift Setup requires one static configure method",
            ));
        }
        let start = line_declaration(&input.code, ty.body.start + offsets[0]);
        let method = parse_method(input, "swift", start, ty)?;
        if !is_configure_signature(&method) {
            return Err(input.failure(start, "Swift configure must be static, accept BehaviorExecution and return BehaviorAdapters synchronously (optionally throws)"));
        }
        ty.methods.push(method);
    }
    Ok(())
}

fn swift_configure_offsets(body: &str) -> Vec<usize> {
    let body_tokens = tokens(body);
    let mut offsets = Vec::new();
    for pair in body_tokens.windows(2) {
        let (at, word) = pair[0];
        if word == "func" && pair[1].1 == "configure" && brace_depth(&body[..at]) == 0 {
            offsets.push(at);
        }
    }
    offsets
}

fn annotation_prefix(language: &str) -> &'static str {
    match language {
        "rs" | "php" | "rb" => "#[",
        "cs" => "[",
        _ => "@",
    }
}

fn marker_name_length(rest: &str) -> usize {
    rest.bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
        .count()
}

fn normalized_marker_name(qualified: &str) -> String {
    let last_segment = qualified.rsplit("::").next().unwrap_or(qualified);
    last_segment.replace('_', "").to_ascii_lowercase()
}

fn known_marker(name: &str) -> Option<&'static &'static str> {
    super::Marker::NAMES
        .iter()
        .find(|marker| marker.to_ascii_lowercase() == name)
}

pub(super) fn annotations(
    input: &Input<'_>,
    language: &str,
    environment: &str,
) -> Result<Vec<Marker>, Failure> {
    let prefix = annotation_prefix(language);
    let mut markers = Vec::new();
    let mut offset = 0;
    let mut depth = 0_i64;
    for line in input.code.split_inclusive('\n') {
        let line_depth = depth;
        depth += super::nesting_depth(line, b'(', b')');
        parameter_markers(input, language, line, offset, line_depth, &mut markers)?;
        reject_inline_marker(input, line, offset, prefix, line_depth)?;
        let start = offset + line.len() - line.trim_start().len();
        let end = offset + line.trim_end_matches(['\r', '\n']).len();
        offset += line.len();
        if start >= end || line_depth > 0 {
            continue;
        }
        let Some(marker) =
            line_annotation(input, language, prefix, environment, start..end, offset)?
        else {
            continue;
        };
        markers.push(marker);
        if markers.len() > 1024 {
            return Err(input.failure(start, "Behavior module exceeds 1024 annotations"));
        }
    }
    if language == "py" {
        markers.extend(python_annotated(input));
        markers.sort_by_key(|marker| marker.span.start);
    }
    if markers.len() > 1024 {
        return Err(input.failure(0, "Behavior module exceeds 1024 annotations"));
    }
    Ok(markers)
}

fn python_annotated(input: &Input<'_>) -> Vec<Marker> {
    let found =
        crate::python_parameters::PythonAnnotated::markers(&input.code, input.source, |written| {
            python_marker(written).is_some()
        });
    found
        .markers
        .into_iter()
        .filter_map(|marker| {
            let name = python_marker(&marker.written)?;
            Some(Marker {
                name: (*name).to_owned(),
                arguments: marker.arguments,
                span: marker.erased,
                target: marker.declaration,
                parameter: marker.parameters.is_some(),
            })
        })
        .collect()
}

fn python_marker(written: &str) -> Option<&'static &'static str> {
    let annotated = crate::annotation_rules::AnnotationRules::find(written)
        .is_some_and(crate::annotation_rules::Rule::python_annotated);
    if !annotated {
        return None;
    }
    known_marker(&normalized_marker_name(written))
}

fn line_annotation(
    input: &Input<'_>,
    language: &str,
    prefix: &str,
    environment: &str,
    span: Range<usize>,
    next_line: usize,
) -> Result<Option<Marker>, Failure> {
    let at = span.start;
    let Some(rest) = input.code[span.clone()].strip_prefix(prefix) else {
        return Ok(None);
    };
    let length = marker_name_length(rest);
    let qualified = &rest[..length];
    if language == "rs" && qualified == "test" {
        return Ok(None);
    }
    let name = normalized_marker_name(qualified);
    if let Some(message) = super::RenamedAnnotation::message(qualified) {
        return Err(input.failure(at, message));
    }
    let Some(canonical) = resolve_marker(input, &name, environment, at)? else {
        return Ok(None);
    };
    if qualified.contains("::") && !qualified.starts_with(&format!("{environment}::")) {
        return Err(input.failure(
            at,
            "Behavior annotation namespace must match its Tac or Yon source",
        ));
    }
    let arguments = annotation_arguments(input, prefix, rest, length, &span)?;
    let target = next_declaration(input, next_line)?;
    if *canonical == "Store" && arguments.trim().is_empty() {
        return Ok(None);
    }
    if *canonical == "Needs" && environment == "yon" && super::targets_type(input, language, target)
    {
        return Ok(None);
    }
    Ok(Some(Marker {
        name: (*canonical).to_owned(),
        arguments,
        span,
        target,
        parameter: false,
    }))
}

fn annotation_arguments(
    input: &Input<'_>,
    prefix: &str,
    rest: &str,
    length: usize,
    span: &Range<usize>,
) -> Result<String, Failure> {
    let at = span.start;
    let end = span.end;
    let mut tail = rest[length..].trim();
    if prefix != "@" {
        tail = tail
            .strip_suffix(']')
            .ok_or_else(|| input.failure(at, "Close the behavior annotation on its own line"))?
            .trim_end();
    }
    if tail.is_empty() {
        return Ok(String::new());
    }
    if !tail.starts_with('(') || !tail.ends_with(')') {
        return Err(input.failure(at, "Behavior annotations occupy their own complete line"));
    }
    let open = at + prefix.len() + length + rest[length..].find('(').unwrap_or(0);
    let close = super::matching(&input.code, open, b'(', b')')
        .ok_or_else(|| input.failure(at, "Unbalanced behavior annotation"))?;
    if close >= end {
        return Err(input.failure(at, "Behavior annotation has trailing source"));
    }
    let trailing = input.code[close + 1..end]
        .trim()
        .trim_end_matches(']')
        .trim();
    if !trailing.is_empty() {
        return Err(input.failure(at, "Behavior annotation has trailing source"));
    }
    Ok(input.source[open + 1..close].to_owned())
}

fn resolve_marker(
    input: &Input<'_>,
    name: &str,
    environment: &str,
    at: usize,
) -> Result<Option<&'static &'static str>, Failure> {
    if name == "client" {
        return Ok(None);
    }
    if matches!(name, "service" | "lifetime" | "helper" | "converter") && environment == "yon" {
        return Ok(None);
    }
    let canonical = known_marker(name);
    if name == "gateway" {
        super::gateway::reject_in_tac(input, at, environment)?;
    }
    if canonical.is_some() {
        return Ok(canonical);
    }
    if let Some(replacement) = super::retired(name) {
        return Err(input.failure(
            at,
            format!("'{name}' is not a Spring annotation; write {replacement}"),
        ));
    }
    if let Some(instead) = super::untestable(name) {
        return Err(input.failure(
            at,
            format!("'{name}' is a Spring or JUnit test annotation, which Tachyon does not run; {instead}"),
        ));
    }
    Ok(None)
}

fn parameter_markers(
    input: &Input<'_>,
    language: &str,
    line: &str,
    offset: usize,
    line_depth: i64,
    markers: &mut Vec<Marker>,
) -> Result<(), Failure> {
    for (position, _) in line.match_indices(annotation_prefix(language)) {
        if line_depth + super::nesting_depth(&line[..position], b'(', b')') <= 0 {
            continue;
        }
        if let Some(marker) = super::parameters::read(input, language, offset + position)? {
            markers.push(marker);
        }
    }
    Ok(())
}

fn reject_inline_marker(
    input: &Input<'_>,
    line: &str,
    offset: usize,
    prefix: &str,
    line_depth: i64,
) -> Result<(), Failure> {
    for (position, _) in line.match_indices(prefix) {
        let inside_parameters =
            line_depth + super::nesting_depth(&line[..position], b'(', b')') > 0;
        if line[..position].trim().is_empty() && !inside_parameters {
            continue;
        }
        let rest = &line[position + prefix.len()..];
        let written = &rest[..marker_name_length(rest)];
        let name = normalized_marker_name(written);
        let parameter_marker = inside_parameters
            && super::parameters::ParameterMarkers::NAMES
                .iter()
                .any(|known| known.to_ascii_lowercase() == name);
        if parameter_marker {
            continue;
        }
        if known_marker(&name).is_some() || super::retired(&name).is_some() {
            return Err(input.failure(
                offset + position,
                "Behavior annotations occupy their own complete line",
            ));
        }
    }
    Ok(())
}

fn next_declaration(input: &Input<'_>, mut offset: usize) -> Result<usize, Failure> {
    for line in input.code[offset..].split_inclusive('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty()
            && !trimmed.starts_with('@')
            && !trimmed.starts_with("#[")
            && !trimmed.starts_with('[')
        {
            return Ok(offset + line.len() - line.trim_start().len());
        }
        offset += line.len();
    }
    Err(input.failure(
        offset.saturating_sub(1),
        "Behavior annotation has no following declaration",
    ))
}

fn tokens(code: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = None;
    for (at, byte) in code.bytes().chain(std::iter::once(b' ')).enumerate() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$') {
            start.get_or_insert(at);
        } else if let Some(begin) = start.take() {
            result.push((begin, &code[begin..at]));
        }
    }
    result
}

fn brace_depth(code: &str) -> i64 {
    let mut depth = 0_i64;
    for byte in code.bytes() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn line_start(code: &str, at: usize) -> usize {
    code[..at].rfind('\n').map_or(0, |end| end + 1)
}

fn line_declaration(code: &str, at: usize) -> usize {
    let start = line_start(code, at);
    start + code[start..at].len() - code[start..at].trim_start().len()
}

fn indentation(code: &str, at: usize) -> usize {
    at - line_start(code, at)
}

fn minimum_indentation(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0)
}

fn python_body(input: &Input<'_>, at: usize) -> Result<Range<usize>, Failure> {
    let newline = input.code[at..]
        .find('\n')
        .map(|end| at + end)
        .ok_or_else(|| {
            input.failure(at, "Python behavior declarations require a multiline body")
        })?;
    let header = input.code[at..newline].trim_end();
    if !header.ends_with(':') {
        return python_ellipsis_body(input, at, header);
    }
    let indent = indentation(&input.code, at);
    let start = newline + 1;
    let mut offset = start;
    for line in input.code[start..].split_inclusive('\n') {
        if !line.trim().is_empty() && line.len() - line.trim_start().len() <= indent {
            return Ok(start..offset);
        }
        offset += line.len();
    }
    Ok(start..input.source.len())
}

fn python_ellipsis_body(
    input: &Input<'_>,
    at: usize,
    header: &str,
) -> Result<Range<usize>, Failure> {
    if header.ends_with("...")
        && let Some(colon) = header.rfind(':')
    {
        return Ok(at + colon + 1..at + header.len());
    }
    Err(input.failure(
        at,
        "Python behavior declarations require a block ending in ':'",
    ))
}

pub(super) fn context_type(language: &str) -> &'static str {
    match language {
        "java" | "kt" => "Behaviors.Context",
        "py" => "object",
        "go" => "*BehaviorContext",
        _ => "BehaviorContext",
    }
}

fn supply_declared(input: &Input<'_>, language: &str, types: &mut [Type]) -> Result<(), Failure> {
    for ty in types.iter_mut().filter(|ty| ty.declared) {
        if let Some(method) = ty.methods.iter().find(|method| !method.declared) {
            return Err(input.failure(
                method.signature.start,
                "An abstract behavior declaration cannot write a method body",
            ));
        }
        if needs_no_context(ty) {
            continue;
        }
        ty.fields.push(Field {
            name: "behaviors".to_owned(),
            type_source: context_type(language).to_owned(),
            uninitialized: true,
            initializer: None,
            is_static: false,
        });
        ty.constructor = Some(vec!["behaviors".to_owned()]);
    }
    Ok(())
}

fn method_needs_context(method: &Method) -> bool {
    if method.metadata.contains_key("gateway") {
        return false;
    }
    method
        .metadata
        .keys()
        .any(|key| !["valid", "mapping"].contains(&key.as_str()))
}

fn needs_no_context(ty: &Type) -> bool {
    if ty.methods.is_empty() {
        return false;
    }
    for method in &ty.methods {
        let free = ["gateway", "mapping"]
            .iter()
            .any(|key| method.metadata.contains_key(*key));
        if method.metadata.len() != 1 || !free {
            return false;
        }
    }
    true
}

pub(super) fn declared_fields(ty: &Type, spell: impl Fn(&Field) -> String) -> String {
    let mut output = String::new();
    if !ty.declared {
        return output;
    }
    for field in &ty.fields {
        output.push_str(&spell(field));
    }
    output
}

pub(super) fn block(method: &Method, body: &str) -> String {
    if method.declared {
        format!(" {{ {body} }}")
    } else {
        body.to_owned()
    }
}

pub(super) fn concrete_edit(module: &Module, ty: &Type) -> Option<Edit> {
    if !ty.declared || matches!(module.language.as_str(), "rs" | "go") {
        return None;
    }
    if module.language == "py" {
        return python_protocol_edit(module, ty);
    }
    let replacement = match module.language.as_str() {
        "swift" => "final class",
        "dart" => "class",
        language => concrete(language),
    };
    Some(Edit {
        range: ty.keyword.clone(),
        replacement: replacement.to_owned(),
    })
}

fn python_protocol_edit(module: &Module, ty: &Type) -> Option<Edit> {
    let header = &module.source[ty.keyword.end..ty.body.start];
    let open = ty.keyword.end + header.find('(')?;
    let close = ty.keyword.end + header.rfind(')')?;
    let mut bases = Vec::new();
    for base in module.source[open + 1..close].split(',') {
        let base = base.trim();
        if !base.is_empty() && !is_protocol_base(base) {
            bases.push(base);
        }
    }
    let replacement = if bases.is_empty() {
        String::new()
    } else {
        format!("({})", bases.join(", "))
    };
    Some(Edit {
        range: open..close + 1,
        replacement,
    })
}

fn is_protocol_base(base: &str) -> bool {
    matches!(base, "Protocol" | "typing.Protocol")
}

pub(super) fn concrete(language: &str) -> &'static str {
    match language {
        "rs" => "struct",
        _ => "class",
    }
}

pub(super) fn abstract_keyword(language: &str) -> &'static str {
    match language {
        "rs" => "trait",
        "swift" => "protocol",
        "py" | "dart" => "\0",
        _ => "interface",
    }
}

fn dart_abstract(input: &Input<'_>, declaration: usize, at: usize) -> Option<usize> {
    let modifiers = &input.code[declaration..at];
    if !modifiers.split_whitespace().any(|word| word == "abstract") {
        return None;
    }
    let offset = modifiers.find("abstract")?;
    Some(declaration + offset)
}

fn declarations(input: &Input<'_>, language: &str) -> Result<Vec<Type>, Failure> {
    declarations_with_runtime(input, language, false)
}

fn declarations_with_runtime(
    input: &Input<'_>,
    language: &str,
    skip_runtime: bool,
) -> Result<Vec<Type>, Failure> {
    if language == "go" {
        return go_syntax::declarations(input, skip_runtime);
    }
    if language == "rb" {
        return ruby_syntax::RubySyntax::declarations(input, skip_runtime);
    }
    let mut result = Vec::new();
    let mut names = BTreeSet::new();
    let code_tokens = tokens(&input.code);
    for (index, &(at, word)) in code_tokens.iter().enumerate() {
        if !is_type_keyword(language, word) {
            continue;
        }
        let declaration = line_declaration(&input.code, at);
        if !is_top_level_type(input, language, declaration, at) {
            continue;
        }
        let Some(&(_, name)) = code_tokens.get(index + 1) else {
            continue;
        };
        if skip_runtime && is_reserved_type_name(name) {
            continue;
        }
        if !super::identifier(name) || is_reserved_type_name(name) {
            return Err(input.failure(at, "Behavior type requires a safe explicit name outside the reserved Behavior API namespace"));
        }
        let ty = parse_type(input, language, declaration, at, word, name)?;
        if !names.insert(name.to_owned()) {
            return Err(input.failure(at, "Duplicate top-level behavior type name"));
        }
        result.push(ty);
        if result.len() > 512 {
            return Err(input.failure(at, "Behavior module exceeds 512 types"));
        }
    }
    Ok(result)
}

fn is_type_keyword(language: &str, word: &str) -> bool {
    let swift_struct = language == "swift" && word == "struct";
    word == "class"
        || word == concrete(language)
        || word == abstract_keyword(language)
        || swift_struct
}

fn is_type_modifier(word: &str) -> bool {
    matches!(
        word,
        "public"
            | "private"
            | "internal"
            | "final"
            | "sealed"
            | "abstract"
            | "data"
            | "pub"
            | "base"
            | "interface"
            | "mixin"
    )
}

fn is_top_level_type(input: &Input<'_>, language: &str, declaration: usize, at: usize) -> bool {
    let modifiers = &input.code[declaration..at];
    if !modifiers.split_whitespace().all(is_type_modifier) {
        return false;
    }
    if language == "py" {
        return indentation(&input.code, declaration) == 0;
    }
    brace_depth(&input.code[..at]) == 0
}

fn is_reserved_type_name(name: &str) -> bool {
    name.starts_with("Behavior") || name.starts_with("_Behavior")
}

fn parse_type(
    input: &Input<'_>,
    language: &str,
    declaration: usize,
    at: usize,
    keyword: &str,
    name: &str,
) -> Result<Type, Failure> {
    let body = type_body(input, language, declaration, at)?;
    let dart_keyword = if language == "dart" {
        dart_abstract(input, declaration, at)
    } else {
        None
    };
    let declared = keyword == abstract_keyword(language)
        || dart_keyword.is_some()
        || (language == "py" && is_python_protocol(input, declaration, body.start));
    let fields = if declared {
        Vec::new()
    } else if language == "py" {
        PythonFields::read(input, body.clone())?
    } else {
        fields(input, language, body.clone())
    };
    let keyword_start = dart_keyword.unwrap_or(at);
    Ok(Type {
        name: name.to_owned(),
        declaration,
        keyword: keyword_start..at + keyword.len(),
        declared,
        body,
        fields,
        methods: Vec::new(),
        configuration: false,
        cache: None,
        entity: None,
        table: None,
        constructor: None,
        repository: None,
        policies: serde_json::Map::new(),
    })
}

fn type_body(
    input: &Input<'_>,
    language: &str,
    declaration: usize,
    at: usize,
) -> Result<Range<usize>, Failure> {
    if language == "py" {
        return python_body(input, declaration);
    }
    let open = input.code[at..]
        .find(['{', ';'])
        .map(|open| at + open)
        .ok_or_else(|| input.failure(at, "Type has no concrete body"))?;
    if input.code.as_bytes()[open] == b'{' {
        let close = super::matching(&input.code, open, b'{', b'}')
            .ok_or_else(|| input.failure(at, "Type body is unbalanced"))?;
        return Ok(open + 1..close);
    }
    if language != "rs" {
        return Err(input.failure(at, "Type has no concrete body"));
    }
    Ok(open..open)
}

fn is_python_protocol(input: &Input<'_>, declaration: usize, body_start: usize) -> bool {
    input.code[declaration..body_start]
        .split(['(', ')', ',', ':'])
        .any(|base| is_protocol_base(base.trim()))
}

fn declaration_pair<'a>(language: &str, declaration: &'a str) -> Option<(&'a str, &'a str)> {
    if language == "go" {
        return declaration
            .split_once(char::is_whitespace)
            .map(|(name, ty)| (name, ty.trim()));
    }
    if matches!(language, "rs" | "swift" | "kt" | "py") {
        return declaration
            .split_once(':')
            .map(|(name, ty)| (name.split_whitespace().last().unwrap_or(""), ty.trim()));
    }
    declaration
        .rsplit_once(char::is_whitespace)
        .map(|(ty, name)| (name.trim_start_matches('$'), ty.trim()))
        .or_else(|| declaration.strip_prefix('$').map(|name| (name, "mixed")))
}

fn innermost_type(types: &[Type], at: usize) -> Option<usize> {
    let mut owner = None;
    let mut smallest_size = 0;
    for (index, ty) in types.iter().enumerate() {
        if at < ty.body.start || at >= ty.body.end {
            continue;
        }
        let size = ty.body.end - ty.body.start;
        if owner.is_none() || size < smallest_size {
            owner = Some(index);
            smallest_size = size;
        }
    }
    owner
}

pub(super) fn event_field(
    input: &Input<'_>,
    language: &str,
    at: usize,
) -> Result<crate::events::EventMethod, Failure> {
    let types = declarations_with_runtime(input, language, true)?;
    let owner = innermost_type(&types, at)
        .ok_or_else(|| input.failure(at, "Resource requires a named class instance field"))?;
    let end = input.code[at..]
        .find('\n')
        .map_or(input.code.len(), |end| at + end);
    let declaration = input.source[at..end]
        .trim()
        .trim_end_matches([',', ';'])
        .trim();
    let field_error = "Resource requires an uninitialized named instance field";
    if declaration.contains(['=', '(', ')']) {
        return Err(input.failure(at, field_error));
    }
    let Some((name, _)) = declaration_pair(language, declaration) else {
        return Err(input.failure(at, field_error));
    };
    if !super::identifier(name) {
        return Err(input.failure(at, field_error));
    }
    Ok(crate::events::EventMethod {
        owner: types[owner].name.clone(),
        name: name.to_owned(),
        parameters: Vec::new(),
        signature: at..end,
        body: end..end,
        return_type: String::new(),
        asynchronous: false,
    })
}

pub(super) fn event_target(
    input: &Input<'_>,
    language: &str,
    at: usize,
) -> Result<crate::events::EventMethod, Failure> {
    let types = declarations_with_runtime(input, language, true)?;
    let owner = method_owner(input, language, &types, at).ok_or_else(|| {
        input.failure(
            at,
            "Event annotations require a direct named instance method",
        )
    })?;
    let method = parse_method(input, language, at, &types[owner])?;
    if method.is_static {
        return Err(input.failure(at, "Event annotations require instance methods"));
    }
    let mut parameters = Vec::new();
    for parameter in method.parameters {
        parameters.push(crate::events::EventParameter {
            name: parameter.name,
            type_source: parameter.type_source,
        });
    }
    Ok(crate::events::EventMethod {
        owner: types[owner].name.clone(),
        name: method.name,
        parameters,
        signature: method.signature,
        body: method.body,
        return_type: method.return_type,
        asynchronous: method.asynchronous,
    })
}

fn record_cases(input: &Input<'_>, language: &str, types: &mut [Type]) {
    for ty in types.iter_mut() {
        if !ty.policies.contains_key("test") {
            continue;
        }
        let cases = test_cases(input, language, ty);
        ty.policies.insert("cases".into(), serde_json::json!(cases));
    }
}

fn test_cases(input: &Input<'_>, language: &str, ty: &Type) -> Vec<String> {
    if language == "go" {
        return go_syntax::test_cases(input, ty);
    }
    if language == "rb" {
        return ruby_syntax::RubySyntax::test_cases(input, ty);
    }
    let body = if language == "rs" {
        let Some(range) = rust_impl_body(input, &ty.name) else {
            return Vec::new();
        };
        range
    } else {
        ty.body.clone()
    };
    let indent = minimum_indentation(&input.code[body.clone()]);
    let mut names = Vec::new();
    for (at, word) in tokens(&input.code[body.clone()]) {
        if !word.starts_with("test") || !super::identifier(word) {
            continue;
        }
        let name_end = body.start + at + word.len();
        let after_name = input.code[name_end..body.end.max(body.start + at)].trim_start();
        if !after_name.starts_with('(') {
            continue;
        }
        if !is_direct_test(input, language, &body, at, indent) {
            continue;
        }
        if !names.iter().any(|name| name == word) {
            names.push(word.to_owned());
        }
    }
    names
}

fn is_direct_test(
    input: &Input<'_>,
    language: &str,
    body: &Range<usize>,
    at: usize,
    indent: usize,
) -> bool {
    if language == "py" {
        let start = line_start(&input.code, body.start + at);
        let line = &input.code[start..];
        return line.len() - line.trim_start().len() == indent;
    }
    brace_depth(&input.code[body.start..body.start + at]) == 0
}

fn rust_impl_body(input: &Input<'_>, name: &str) -> Option<Range<usize>> {
    let code_tokens = tokens(&input.code);
    for (index, &(at, word)) in code_tokens.iter().enumerate() {
        if word != "impl" || brace_depth(&input.code[..at]) != 0 {
            continue;
        }
        let &(target_at, target) = code_tokens.get(index + 1)?;
        if target != name {
            continue;
        }
        let target_end = target_at + target.len();
        let open = input.code[target_end..].find('{')? + target_end;
        let close = super::matching(&input.code, open, b'{', b'}')?;
        return Some(open + 1..close);
    }
    None
}

fn field_delimiter(language: &str) -> char {
    if language == "rs" {
        ','
    } else if matches!(language, "java" | "cs" | "php" | "dart") {
        ';'
    } else {
        '\n'
    }
}

fn fields(input: &Input<'_>, language: &str, body: Range<usize>) -> Vec<Field> {
    let mut result = Vec::new();
    let delimiter = if crate::groovy_source::GroovySource::is_groovy(input.path) {
        '\n'
    } else {
        field_delimiter(language)
    };
    let mut offset = body.start;
    for segment in input.code[body.clone()].split_inclusive(delimiter) {
        let source_start = offset + segment.len() - segment.trim_start().len();
        let code_end = source_start + segment.trim().len();
        let source_end =
            code_end + trailing_text_length(&input.source[code_end..offset + segment.len()]);
        let source = &input.source[source_start..source_end];
        let direct = brace_depth(&input.code[body.start..offset]) == 0;
        offset += segment.len();
        if !direct || segment.contains('(') || segment.trim_start().starts_with(['@', '#', '[']) {
            continue;
        }
        if let Some(field) = parse_field(language, source) {
            result.push(field);
        }
    }
    result
}

fn trailing_text_length(rest: &str) -> usize {
    let text = rest.trim_start();
    let Some(quote) = text
        .chars()
        .next()
        .filter(|first| matches!(first, '"' | '\''))
    else {
        return 0;
    };
    let mut escaped = false;
    for (at, character) in text.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return rest.len() - text.len() + at + 1;
        }
    }
    0
}

fn parse_field(language: &str, source: &str) -> Option<Field> {
    let uninitialized = !source.contains('=');
    let text = source.trim().trim_end_matches([',', ';']);
    let (declaration, initializer) = match text.split_once('=') {
        Some((declaration, initializer)) => (declaration.trim(), Some(initializer.trim())),
        None => (text.trim(), None),
    };
    let (name, type_source) = declaration_pair(language, declaration)?;
    if !super::identifier(name) {
        return None;
    }
    let is_static = declaration
        .split_whitespace()
        .any(|word| matches!(word, "static" | "const"))
        || type_source.starts_with("ClassVar");
    Some(Field {
        name: name.to_owned(),
        type_source: without_field_modifiers(type_source),
        uninitialized,
        initializer: initializer.map(str::to_owned),
        is_static,
    })
}

fn without_field_modifiers(type_source: &str) -> String {
    let mut words = Vec::new();
    for word in type_source.split_whitespace() {
        if !is_field_modifier(word) {
            words.push(word);
        }
    }
    words.join(" ")
}

fn is_field_modifier(word: &str) -> bool {
    matches!(
        word,
        "public"
            | "private"
            | "protected"
            | "internal"
            | "final"
            | "readonly"
            | "static"
            | "volatile"
            | "late"
            | "const"
            | "covariant"
    )
}

fn rust_impl_owner(input: &Input<'_>, types: &[Type], at: usize) -> Option<usize> {
    if let Some(owner) = types.iter().position(|ty| {
        ty.declared
            && at > ty.body.start
            && at < ty.body.end
            && brace_depth(&input.code[ty.body.start..at]) == 0
    }) {
        return Some(owner);
    }
    let code_tokens = tokens(&input.code[..at]);
    for (index, &(start, word)) in code_tokens.iter().enumerate().rev() {
        if word != "impl" || brace_depth(&input.code[..start]) != 0 {
            continue;
        }
        let &(name_at, name) = code_tokens.get(index + 1)?;
        let name_end = name_at + name.len();
        let open = input.code[name_end..].find('{')? + name_end;
        if !input.code[name_end..open].trim().is_empty() {
            continue;
        }
        let end = super::matching(&input.code, open, b'{', b'}')?;
        if at > open && at < end && brace_depth(&input.code[open + 1..at]) == 0 {
            return types.iter().position(|ty| ty.name == name);
        }
    }
    None
}

fn method_owner(input: &Input<'_>, language: &str, types: &[Type], at: usize) -> Option<usize> {
    if language == "rs" {
        return rust_impl_owner(input, types, at);
    }
    if language == "go" {
        return go_syntax::method_owner(input, types, at);
    }
    if language == "rb" {
        return ruby_syntax::RubySyntax::method_owner(input, types, at);
    }
    types
        .iter()
        .position(|ty| is_direct_member(input, language, ty, at))
}

fn is_direct_member(input: &Input<'_>, language: &str, ty: &Type, at: usize) -> bool {
    if at < ty.body.start || at >= ty.body.end {
        return false;
    }
    if language != "py" {
        return brace_depth(&input.code[ty.body.start..at]) == 0;
    }
    let first_line = input.code[ty.body.clone()]
        .lines()
        .find(|line| !line.trim().is_empty());
    let Some(first_line) = first_line else {
        return false;
    };
    indentation(&input.code, at) == first_line.len() - first_line.trim_start().len()
}

fn require_no_arguments(input: &Input<'_>, marker: &Marker) -> Result<(), Failure> {
    if marker.arguments.trim().is_empty() {
        return Ok(());
    }
    Err(input.failure(
        marker.span.start,
        crate::annotation_rules::AnnotationRules::no_arguments(&marker.name),
    ))
}

fn apply_type(
    input: &Input<'_>,
    environment: &str,
    ty: &mut Type,
    marker: &Marker,
) -> Result<(), Failure> {
    match marker.name.as_str() {
        "Setup" => {
            require_no_arguments(input, marker)?;
            ty.configuration = true;
        }
        "NeedsAll" => {
            ty.constructor = Some(required_constructor(input, environment, ty, marker)?);
        }
        "CacheSettings" => ty.cache = Some(super::cache_config(input, marker)?),
        "Entity" => ty.entity = Some(super::entity(input, marker)?),
        "Table" => ty.table = Some(super::single_string(input, marker)?),
        "Store" => ty.repository = Some(super::queries::repository(input, marker)?),
        "TestSetup" => {
            require_no_arguments(input, marker)?;
            ty.configuration = true;
            ty.policies
                .insert("testConfiguration".into(), Value::Bool(true));
        }
        _ => {
            let (key, policy) = type_policy(input, environment, marker)?;
            ty.policies.insert(key.into(), policy);
        }
    }
    Ok(())
}

fn required_constructor(
    input: &Input<'_>,
    environment: &str,
    ty: &Type,
    marker: &Marker,
) -> Result<Vec<String>, Failure> {
    let named = super::string_list(input, marker)?;
    if !named.is_empty() || environment != "tac" {
        return Ok(named);
    }
    let mut uninitialized_fields = Vec::new();
    for field in &ty.fields {
        if field.uninitialized {
            uninitialized_fields.push(field.name.clone());
        }
    }
    Ok(uninitialized_fields)
}

fn type_policy(
    input: &Input<'_>,
    environment: &str,
    marker: &Marker,
) -> Result<(&'static str, Value), Failure> {
    let policy = match marker.name.as_str() {
        "Description" => ("description", super::description(input, marker)?),
        "Converter" => {
            require_no_arguments(input, marker)?;
            ("mapper", Value::Bool(true))
        }
        "Test" => ("test", super::testing::unit(input, marker)?),
        "UseProfiles" => (
            "activeProfiles",
            super::configuration::profiles(input, marker)?,
        ),
        "TestSettings" => (
            "testProperties",
            super::testing::properties(input, marker, environment)?,
        ),
        "FreshStart" => {
            require_no_arguments(input, marker)?;
            ("dirtiesContext", Value::Bool(true))
        }
        "Checked" => ("validated", super::validation::schema(input, marker)?),
        "Settings" => (
            "configurationProperties",
            super::configuration::properties(input, marker)?,
        ),
        "WhenProfile" => ("profiles", super::configuration::profiles(input, marker)?),
        "WhenSetting" => (
            "conditionalOnProperty",
            super::configuration::conditional(input, marker)?,
        ),
        "Helper" => {
            require_no_arguments(input, marker)?;
            HelperRole::refusal(input.path).map_or(Ok(()), |message| {
                Err(input.failure(marker.span.start, message))
            })?;
            ("helper", Value::Bool(true))
        }
        _ => {
            return Err(input.failure(
                marker.span.start,
                crate::annotation_rules::AnnotationRules::belongs(&marker.name),
            ));
        }
    };
    Ok(policy)
}

pub(super) fn split_parameters(source: &str) -> Vec<&str> {
    let mut depth = 0_i64;
    let mut start = 0;
    let mut result = Vec::new();
    for (at, byte) in source.bytes().enumerate() {
        match byte {
            b'<' | b'(' | b'[' | b'{' => depth += 1,
            b'>' | b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                result.push(source[start..at].trim());
                start = at + 1;
            }
            _ => {}
        }
    }
    if !source[start..].trim().is_empty() {
        result.push(source[start..].trim());
    }
    result
}

fn parse_method(
    input: &Input<'_>,
    language: &str,
    at: usize,
    ty: &Type,
) -> Result<Method, Failure> {
    if language == "go" {
        return go_syntax::parse_method(input, at, ty);
    }
    if language == "rb" {
        return ruby_syntax::RubySyntax::parse_method(input, at, ty);
    }
    let open = input.code[at..]
        .find('(')
        .map(|open| at + open)
        .ok_or_else(|| input.failure(at, "Behavior method has no parameter list"))?;
    let close = super::matching(&input.code, open, b'(', b')')
        .ok_or_else(|| input.failure(at, "Behavior method parameters are unbalanced"))?;
    let prefix = input.code[at..open].trim();
    let words = prefix.split_whitespace().collect::<Vec<_>>();
    let name = words.last().copied().unwrap_or("");
    if !super::identifier(name) || prefix.contains(['\n', ';', '{', '}']) {
        return Err(input.failure(at, "Behavior methods must be named concrete declarations"));
    }
    let (body, declared) = method_body(input, language, at, close, ty)?;
    let (parameters, receiver) =
        method_parameters(input, language, at, &input.code[open + 1..close])?;
    let signature_end = if declared {
        body.start
    } else {
        body.start.saturating_sub(1)
    };
    let tail = input.source[close + 1..signature_end].trim();
    let return_type = method_return_type(language, tail, &words);
    let asynchronous = words.contains(&"async")
        || words.contains(&"suspend")
        || tail.split_whitespace().any(|word| word == "async");
    let is_static = words.contains(&"static") || (matches!(language, "rs" | "py") && !receiver);
    Ok(Method {
        name: name.to_owned(),
        parameters,
        signature: at..signature_end,
        body,
        return_type,
        asynchronous,
        is_static,
        declared,
        metadata: serde_json::Map::new(),
    })
}

fn method_body(
    input: &Input<'_>,
    language: &str,
    at: usize,
    close: usize,
    ty: &Type,
) -> Result<(Range<usize>, bool), Failure> {
    if language == "py" {
        let range = python_body(input, at)?;
        let declared = input.code[range.clone()].trim() == "...";
        return Ok((range, declared));
    }
    if let Some(range) = declared_body(input, ty, close) {
        return Ok((range, true));
    }
    let range = block_body(input, language, at, close, ty)?;
    Ok((range, false))
}

fn block_body(
    input: &Input<'_>,
    language: &str,
    at: usize,
    close: usize,
    ty: &Type,
) -> Result<Range<usize>, Failure> {
    let opening = input.code[close + 1..]
        .find(['{', ';'])
        .map(|open| close + 1 + open)
        .ok_or_else(|| input.failure(at, "Behavior method has no body"))?;
    if input.code.as_bytes()[opening] != b'{' || input.code[close + 1..opening].contains('=') {
        return Err(input.failure(
            at,
            "Behavior methods require a block body, not an abstract or expression body",
        ));
    }
    let closing = super::matching(&input.code, opening, b'{', b'}')
        .ok_or_else(|| input.failure(at, "Behavior method body is unbalanced"))?;
    if language != "rs" && closing > ty.body.end {
        return Err(input.failure(at, "Behavior method exceeds its owner"));
    }
    Ok(opening + 1..closing)
}

fn method_return_type(language: &str, tail: &str, words: &[&str]) -> String {
    match language {
        "rs" | "swift" => tail
            .split_once("->")
            .map_or("", |(_, ty)| ty.trim())
            .to_owned(),
        "kt" | "php" => tail.strip_prefix(':').unwrap_or(tail).trim().to_owned(),
        "py" => tail
            .split_once("->")
            .map_or("", |(_, ty)| ty.trim().trim_end_matches(':').trim_end())
            .to_owned(),
        "java" | "cs" | "dart" => leading_return_type(words),
        _ => String::new(),
    }
}

fn leading_return_type(words: &[&str]) -> String {
    let words_before_name = &words[..words.len().saturating_sub(1)];
    let mut type_words = Vec::new();
    for word in words_before_name {
        if !is_method_modifier(word) {
            type_words.push(*word);
        }
    }
    type_words.join(" ")
}

fn is_method_modifier(word: &str) -> bool {
    matches!(
        word,
        "public"
            | "private"
            | "protected"
            | "internal"
            | "static"
            | "abstract"
            | "async"
            | "final"
            | "synchronized"
            | "virtual"
            | "override"
    )
}

fn declared_body(input: &Input<'_>, ty: &Type, close: usize) -> Option<Range<usize>> {
    if !ty.declared {
        return None;
    }
    let tail = &input.code[close + 1..ty.body.end];
    let line = &tail[..tail.find('\n').unwrap_or(tail.len())];
    let Some(terminator) = line.find([';', '{']) else {
        let end = close + 1 + line.trim_end().len();
        return Some(end..end);
    };
    if line.as_bytes()[terminator] == b'{' {
        return None;
    }
    Some(close + 1 + terminator..close + 2 + terminator)
}

fn parameter_pair<'a>(language: &str, part: &'a str) -> Option<(&'a str, &'a str)> {
    if matches!(language, "rs" | "swift" | "kt" | "py") {
        let pair = part
            .split_once(':')
            .map(|(name, ty)| (name.split_whitespace().last().unwrap_or(""), ty.trim()));
        if pair.is_none() && language == "py" {
            return Some((part, "object"));
        }
        return pair;
    }
    let pair = part
        .rsplit_once(char::is_whitespace)
        .map(|(ty, name)| (name.trim_start_matches('$'), ty.trim()));
    if pair.is_none() && language == "php" {
        return Some((part.trim_start_matches('$'), "mixed"));
    }
    pair
}

fn is_reserved_parameter(language: &str, name: &str) -> bool {
    (language == "php" && name.starts_with("__behavior_"))
        || (language == "rs" && name.starts_with("__tachyon_"))
}

fn method_parameters(
    input: &Input<'_>,
    language: &str,
    at: usize,
    raw: &str,
) -> Result<(Vec<Parameter>, bool), Failure> {
    let mut parameters = Vec::new();
    let mut receiver = false;
    for part in split_parameters(raw) {
        if matches!(part, "self" | "&self" | "&mut self") {
            receiver = true;
            continue;
        }
        if part.contains('=') || part.contains("...") || part.starts_with('*') {
            return Err(input.failure(at, "Behavior methods require explicit positional parameters without defaults or rest arguments"));
        }
        let Some((name, type_source)) = parameter_pair(language, part) else {
            return Err(input.failure(
                at,
                "Behavior parameters require explicit names and native types",
            ));
        };
        if !super::identifier(name) {
            return Err(input.failure(at, "Invalid behavior parameter name"));
        }
        if is_reserved_parameter(language, name) {
            return Err(input.failure(at, "Behavior parameter uses a compiler-reserved name"));
        }
        parameters.push(Parameter {
            name: name.to_owned(),
            type_source: type_source.to_owned(),
        });
    }
    if parameters.len() > 32 {
        return Err(input.failure(at, "Behavior methods exceed 32 parameters"));
    }
    Ok((parameters, receiver))
}

fn validate_type(input: &Input<'_>, language: &str, ty: &Type) -> Result<(), Failure> {
    let header = &input.code[ty.declaration..ty.body.start];
    let interfaces = CsharpHelperInterfaces::allowed(input, language, ty)?;
    if inherits_from_other_type(language, header) && !interfaces {
        return Err(input.failure(
            ty.declaration,
            "Behavior types cannot inherit from other types",
        ));
    }
    if ty.entity.is_some() != ty.table.is_some() {
        return Err(input.failure(ty.declaration, "Entity and Table must be declared together"));
    }
    if let Some(table) = &ty.table
        && !super::identifier(table)
    {
        return Err(input.failure(ty.declaration, "Table requires a bounded identifier"));
    }
    for method in &ty.methods {
        super::gateway::reject_combined(input, method.signature.start, &method.metadata)?;
        validate_method_policies(input, ty, method)?;
    }
    let Some(constructor) = &ty.constructor else {
        return Ok(());
    };
    validate_constructor(input, language, ty, header, constructor)
}

fn inherits_from_other_type(language: &str, header: &str) -> bool {
    header.contains("extends")
        || header.contains("implements")
        || (language == "dart" && header.split_whitespace().any(|word| word == "with"))
        || (matches!(language, "swift" | "kt" | "cs") && header.contains(':'))
        || (language == "py" && has_python_base(header))
        || (language == "rb" && header.contains('<'))
}

fn has_python_base(header: &str) -> bool {
    let Some((_, after_open)) = header.split_once('(') else {
        return false;
    };
    let Some((bases, _)) = after_open.rsplit_once(')') else {
        return false;
    };
    bases
        .split(',')
        .map(str::trim)
        .any(|base| !base.is_empty() && !is_protocol_base(base))
}

fn validate_method_policies(input: &Input<'_>, ty: &Type, method: &Method) -> Result<(), Failure> {
    if method.metadata.contains_key("cacheable") && ty.cache.is_none() {
        return Err(input.failure(
            method.signature.start,
            "Cached requires its class Cache policy",
        ));
    }
    for key in ["cacheable", "pageable", "cacheEvict", "cachePut", "valid"] {
        let Some(policy) = method.metadata.get(key) else {
            continue;
        };
        let parameter_count = method.parameters.len() as u64;
        let indexes = policy_argument_indexes(key, policy);
        let references_missing_argument = indexes
            .iter()
            .any(|index| index.as_u64().is_none_or(|index| index >= parameter_count));
        if references_missing_argument {
            return Err(input.failure(
                method.signature.start,
                "Behavior policy references a missing positional argument",
            ));
        }
    }
    Ok(())
}

fn policy_argument_indexes(key: &str, policy: &Value) -> Vec<Value> {
    match key {
        "cacheable" => policy["keyArgs"].as_array().cloned().unwrap_or_default(),
        "pageable" | "valid" => vec![policy["argument"].clone()],
        _ => policy["arguments"].as_array().cloned().unwrap_or_default(),
    }
}

fn validate_constructor(
    input: &Input<'_>,
    language: &str,
    ty: &Type,
    header: &str,
    constructor: &[String],
) -> Result<(), Failure> {
    let field_names = ty
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    for name in constructor {
        if !field_names.contains(name.as_str()) {
            return Err(input.failure(
                ty.declaration,
                "NeedsAll fields must have explicit native declarations",
            ));
        }
    }
    if has_existing_constructor(input, language, ty, header) {
        return Err(input.failure(
            ty.declaration,
            crate::annotation_rules::AnnotationRules::needs_all_constructor(language),
        ));
    }
    Ok(())
}

fn has_existing_constructor(input: &Input<'_>, language: &str, ty: &Type, header: &str) -> bool {
    let body = &input.code[ty.body.clone()];
    match language {
        "py" => body.contains("def __init__("),
        "php" => body.contains("function __construct("),
        "kt" => body.contains("constructor(") || header.contains('('),
        "swift" => body.contains("init("),
        "rs" => has_rust_new_function(input, ty),
        "go" => false,
        "rb" => body.contains("def initialize"),
        _ => body.contains(&format!("{}(", ty.name)),
    }
}

fn has_rust_new_function(input: &Input<'_>, ty: &Type) -> bool {
    let code_tokens = tokens(&input.code);
    for pair in code_tokens.windows(2) {
        let (at, word) = pair[0];
        if word == "fn"
            && pair[1].1 == "new"
            && rust_impl_owner(input, std::slice::from_ref(ty), at) == Some(0)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod gateway_runtime_tests {
    #![allow(clippy::expect_used)]

    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    fn digest(text: &str) -> String {
        let mut hex = String::new();
        for byte in Sha256::digest(text.as_bytes()) {
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }

    #[test]
    fn the_gateway_runtime_text_is_unchanged() {
        let csharp = super::gateway_unit_runtime("cs", "").expect("C# runtime");
        assert_eq!(csharp.len(), 76_909);
        assert_eq!(
            digest(&csharp),
            "43733838cf302d6470aea36f93c1a3e4fe8374481814733b2ef2d816028192a9"
        );
        let rust = super::gateway_unit_runtime("rs", "").expect("Rust runtime");
        assert_eq!(rust.len(), 87_486);
        assert_eq!(
            digest(&rust),
            "82a7816a8a0a3dff566fca77c2e2eb6daf08349624e8cff748ca1fe6393d97ff"
        );
        let go = super::gateway_unit_runtime("go", "").expect("Go runtime");
        assert_eq!(go.len(), 89_898);
        assert_eq!(
            digest(&go),
            "49a166de4674ad4a8f2ad8ac50d37a7d68ef12ca14252a3c1497ce80be6d3db3"
        );
    }

    #[test]
    fn a_unit_that_already_has_the_runtime_gets_none() {
        let csharp = super::gateway_unit_runtime("cs", "").expect("C# runtime");
        assert!(super::gateway_unit_runtime("cs", &csharp).is_none());
        let rust = super::gateway_unit_runtime("rs", "").expect("Rust runtime");
        assert!(super::gateway_unit_runtime("rs", &rust).is_none());
        let go = super::gateway_unit_runtime("go", "").expect("Go runtime");
        assert!(super::gateway_unit_runtime("go", &go).is_none());
    }

    #[test]
    fn other_languages_get_no_gateway_runtime() {
        for extension in ["java", "kt", "dart", "js", "py", "php"] {
            assert!(
                super::gateway_unit_runtime(extension, "").is_none(),
                "{extension}"
            );
        }
    }
}

#[cfg(test)]
mod csharp_browser_http_tests;
