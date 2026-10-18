use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::ops::Range;
use std::path::Path;
pub(crate) use test_assert::TestAssert;
use value_source::ValueSource;

mod behaviors_import;
pub(crate) use behaviors_import::BehaviorsImport;
mod clients;
mod configuration;
mod event_frontend;
pub(crate) use event_frontend::event_target;
mod gateway;
mod http_client;
pub(crate) use gateway::{
    GatewayDeclaration, GatewayLimits, GatewayTarget, declarations as gateway_declarations,
    target_failure as gateway_target_failure, target_signature as gateway_target_signature,
};
pub(crate) use polyglot::{
    CsharpHelperGraph, DetachedCsharp, HelperRole, JavaBuiltins, KotlinBuiltins, LuaBehaviorMethod,
    LuaBehaviorPlan, NativeStorage, PlannedMethod, PlannedType, PythonMembers, SwiftPlatform,
    gateway_unit_runtime,
};
#[cfg(test)]
mod extensions_tests;
mod jsdoc_signature;
pub(crate) mod literal;
mod mapper;
mod model_mapping;
mod mutations;
mod native_application_lifetime;
pub(crate) use native_application_lifetime::NativeApplicationLifetime;
mod parameters;
mod polyglot;
pub(crate) use polyglot::ModelSchema;
mod queries;
mod query_frontend;
mod renamed;
pub(crate) use http_client::HttpClientSource;
pub(crate) use queries::derives_name as is_derived_query;
pub(crate) use renamed::{RenamedAnnotation, RoutingRename};
mod call_style;
mod resilience;
mod ruby;
mod service_roles;
pub(crate) use service_roles::ServiceRoles;
mod stored_state;
pub(crate) use stored_state::{SigilField, StoredState};
mod test_assert;
mod testing;
mod unit_table;
pub(crate) use unit_table::UnitTable;
mod validation;
mod value_source;
mod written_policies;
pub(crate) use written_policies::{WrittenMarker, WrittenPolicies, WrittenQuery};
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod value_source_tests;

fn retired(name: &str) -> Option<&'static str> {
    let normalized = name.replace('_', "").to_ascii_lowercase();
    Marker::RETIRED_NAMES
        .iter()
        .find(|(old, _)| old.replace('_', "").to_ascii_lowercase() == normalized)
        .map(|(_, replacement)| *replacement)
}

fn untestable(name: &str) -> Option<&'static str> {
    let normalized = name.replace('_', "").to_ascii_lowercase();
    Marker::UNTESTABLE_NAMES
        .iter()
        .find(|(spring, _)| spring.to_ascii_lowercase() == normalized)
        .map(|(_, instead)| *instead)
}

struct Input<'a> {
    path: &'a Path,
    source: &'a str,
    code: String,
}

impl Input<'_> {
    fn extension(&self) -> &str {
        crate::groovy_source::GroovySource::syntax(
            self.path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or(""),
        )
    }

    fn failure(&self, at: usize, message: impl Into<String>) -> Failure {
        Failure::one(diagnostic(
            1308,
            message,
            Some("Behavior annotations require named types, own-line markers and literal configuration; see docs/BEHAVIOR_ANNOTATIONS.md.".to_owned()),
            source_span(&self.path.to_string_lossy(), at, (at + 1).min(self.source.len())),
        ))
    }
}

#[derive(Clone)]
struct Marker {
    name: String,
    arguments: String,
    span: Range<usize>,
    target: usize,
    parameter: bool,
}

impl Marker {
    const NAMES: [&str; 48] = [
        "Test",
        "ValueSource",
        "TestSetup",
        "Fake",
        "UseProfiles",
        "TestSettings",
        "FreshStart",
        "Client",
        "Id",
        "Service",
        "Lifetime",
        "Helper",
        "Converter",
        "Convert",
        "Needs",
        "Named",
        "Setup",
        "Description",
        "Schedule",
        "NeedsAll",
        "CacheSettings",
        "Cached",
        "CacheKey",
        "ClearCache",
        "UpdateCache",
        "PageDefaults",
        "Retry",
        "MaxAtOnce",
        "AllOrNothing",
        "Entity",
        "Table",
        "WebCall",
        "Gateway",
        "Store",
        "Query",
        "Settings",
        "WhenProfile",
        "WhenSetting",
        "Setting",
        "Checked",
        "Check",
        "Required",
        "NotEmpty",
        "Length",
        "Min",
        "Max",
        "LocalState",
        "SessionState",
    ];
    const RETIRED_NAMES: [(&str, &str); 5] = [
        ("Cache", "CacheSettings"),
        ("SpringBootTest", "Test"),
        ("Pageable", "PageDefaults"),
        ("Delegate", "Helper"),
        ("EventPublisher", "an injected ApplicationEventPublisher"),
    ];
    const SLICE_ADVICE: &str = "write Test: there is no ambient context to slice, because an invocation owns its own instance graph and a Setup is always selected explicitly";
    const SPY_ADVICE: &str =
        "there is no bean to wrap; write Fake and assign the stub the test wants";
    const UNTESTABLE_NAMES: [(&str, &str); 8] = [
        ("WebMvcTest", Self::SLICE_ADVICE),
        ("WebFluxTest", Self::SLICE_ADVICE),
        ("DataJpaTest", Self::SLICE_ADVICE),
        ("JsonTest", Self::SLICE_ADVICE),
        ("RestClientTest", Self::SLICE_ADVICE),
        (
            "AutoConfigureMockMvc",
            "call the controller's verb method directly; there is no MockMvc because the route directory, not an annotation, owns the URL",
        ),
        ("SpyBean", Self::SPY_ADVICE),
        ("MockitoSpyBean", Self::SPY_ADVICE),
    ];
}

#[derive(Default)]
struct Registration {
    name: String,
    start: usize,
    open: usize,
    close: usize,
    inherited: bool,
    markers: BTreeSet<String>,
    data: Map<String, Value>,
    methods: BTreeMap<String, Map<String, Value>>,
    constructor: Option<Vec<String>>,
    inferred: bool,
    declared: bool,
    rewrites: Vec<(Range<usize>, String)>,
}

impl Registration {
    fn emitted(&self) -> bool {
        !self.markers.is_empty() || !self.methods.is_empty() || self.data.contains_key("values")
    }
}

pub(crate) fn lower(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Option<String>, Failure> {
    lower_with_resources(path, source, environment, &BTreeMap::new())
}

fn own_markers(input: &Input<'_>, environment: &str) -> Result<Vec<Marker>, Failure> {
    let markers = markers(input)?;
    for marker in &markers {
        if marker.name == "Gateway" {
            gateway::reject_in_tac(input, marker.span.start, environment)?;
        }
    }
    if environment == "tac" {
        return Ok(markers);
    }
    Ok(markers
        .into_iter()
        .filter(|marker| !tac_only_marker(input, marker))
        .collect())
}

pub(crate) fn unit_tables(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Vec<UnitTable>, Failure> {
    polyglot::unit_tables(path, source, environment)
}

pub(crate) fn test_configuration_name(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Option<String>, Failure> {
    let (input, markers) = test_source_markers(path, source, environment)?;
    let mut configuration = None;
    for marker in markers.iter().filter(|marker| marker.name == "Test") {
        if configuration.is_some() {
            return Err(input.failure(
                marker.span.start,
                "A test unit declares exactly one Test class",
            ));
        }
        configuration = testing::unit(&input, marker)?["configuration"]
            .as_str()
            .map(str::to_owned);
    }
    Ok(configuration)
}

pub(crate) fn is_test_configuration(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<bool, Failure> {
    let (input, markers) = test_source_markers(path, source, environment)?;
    let selected: Vec<_> = markers
        .iter()
        .filter(|marker| marker.name == "TestSetup")
        .collect();
    let Some(marker) = selected.first() else {
        return Ok(false);
    };
    if selected.len() != 1 {
        return Err(input.failure(
            marker.span.start,
            "A test configuration declares exactly one TestSetup annotation",
        ));
    }
    flag_without_arguments(&input, marker)?;
    let names = crate::annotation_layout::declaration_names(path, source)?;
    if names.len() != 1 || !configuration_class(&input, marker, &names[0])? {
        return Err(input.failure(
            marker.span.start,
            "TestSetup must annotate the owned top-level configuration class, not a member or nested type",
        ));
    }
    Ok(true)
}

pub(crate) fn is_production_configuration(
    path: &Path,
    source: &str,
    environment: &str,
    name: &str,
) -> Result<bool, Failure> {
    let (input, markers) = test_source_markers(path, source, environment)?;
    for marker in markers.iter().filter(|marker| marker.name == "Setup") {
        flag_without_arguments(&input, marker)?;
        if configuration_class(&input, marker, name)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn configuration_class(input: &Input<'_>, marker: &Marker, name: &str) -> Result<bool, Failure> {
    let declaration = input.source[marker.target..].lines().next().unwrap_or("");
    if crate::annotation_layout::declaration_names(input.path, declaration)?.as_slice() != [name] {
        return Ok(false);
    }
    let before = &input.code[..marker.span.start];
    if input
        .path
        .extension()
        .is_some_and(|extension| extension == "py")
    {
        return Ok(before.rsplit('\n').next().is_none_or(str::is_empty));
    }
    Ok(crate::annotation_layout::brace_depth_after(before, 0) == 0)
}

fn test_source_markers<'a>(
    path: &'a Path,
    source: &'a str,
    environment: &str,
) -> Result<(Input<'a>, Vec<Marker>), Failure> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    BehaviorsImport::refuse(path, source)?;
    let mask = crate::lexical::code_mask(extension, source);
    let extension = crate::groovy_source::GroovySource::syntax(extension);
    let input = Input {
        path,
        source,
        code: visible_code(source, &mask.code),
    };
    if source.len() > 1024 * 1024 {
        return Err(input.failure(0, "Annotated behavior source exceeds 1 MiB"));
    }
    if let Some(error) = mask.error {
        return Err(input.failure(0, error));
    }
    let markers = if matches!(extension, "js" | "ts") {
        own_markers(&input, environment)?
    } else {
        polyglot::annotations(&input, extension, environment)?
    };
    Ok((input, markers))
}

fn tac_only_marker(input: &Input<'_>, marker: &Marker) -> bool {
    if matches!(
        marker.name.as_str(),
        "Client" | "Service" | "Lifetime" | "Helper" | "Converter"
    ) {
        return true;
    }
    if marker.name == "Needs" {
        return targets_type(input, "js", marker.target);
    }
    marker.name == "Store" && marker.arguments.trim().is_empty()
}

fn targets_type(input: &Input<'_>, extension: &str, target: usize) -> bool {
    let line = input.code[target..].lines().next().unwrap_or("");
    crate::annotation_layout::declares_type(extension, line)
}

pub(crate) fn runtime_source() -> &'static str {
    include_str!("runtime.js")
}

pub(crate) fn lower_with_resources(
    path: &Path,
    source: &str,
    environment: &str,
    resources: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Option<String>, Failure> {
    let Some(mut rewritten) = http_client::HttpClientSource::rewrite(path, source)? else {
        return lower_source(path, source, environment, resources);
    };
    let browser_script = path
        .extension()
        .is_some_and(|value| value == "js" || value == "ts");
    if environment == "tac" && !browser_script {
        rewritten = ServiceRoles::read(path, &rewritten)?.source;
    }
    let lowered = lower_source(path, &rewritten, environment, resources)?;
    Ok(Some(lowered.unwrap_or(rewritten)))
}

pub(crate) fn csharp_case_calls(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Vec<(String, String)>, Failure> {
    let unit = without_tac_roles(path, source, environment)?;
    polyglot::csharp_case_calls(path, &unit, environment)
}

pub(crate) use polyglot::{CsharpBrowserConstruction, CsharpTaskModels};

fn without_tac_roles(path: &Path, source: &str, environment: &str) -> Result<String, Failure> {
    if environment == "tac" {
        return Ok(ServiceRoles::read(path, source)?.source);
    }
    Ok(source.to_owned())
}

pub(crate) fn detached_csharp(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Option<DetachedCsharp>, Failure> {
    let Some(lowered) = lower(path, source, environment)? else {
        return Ok(None);
    };
    let rewritten = http_client::HttpClientSource::rewrite(path, source)?;
    let text = match rewritten {
        Some(text) if environment == "tac" => ServiceRoles::read(path, &text)?.source,
        Some(text) => text,
        None => source.to_owned(),
    };
    let detached = polyglot::detached_csharp(path, &text, environment)?;
    Ok(Some(match detached {
        Some(detached) => detached.with_lowered(lowered),
        None => DetachedCsharp::plain(lowered),
    }))
}

fn lower_source(
    path: &Path,
    source: &str,
    environment: &str,
    resources: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Option<String>, Failure> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Failure::one(diagnostic(
            1308,
            "Source exceeds the 16 MiB project capture limit",
            None,
            None,
        )));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !matches!(extension, "js" | "ts") {
        return polyglot::lower(path, source, environment);
    }
    BehaviorsImport::refuse(path, source)?;
    let mask = crate::lexical::code_mask(extension, source);
    let mut input = Input {
        path,
        source,
        code: visible_code(source, &mask.code),
    };
    let markers = own_markers(&input, environment)?;
    for marker in markers.iter().filter(|marker| marker.parameter) {
        let blank = blank_text(&input.code[marker.span.clone()]);
        input.code.replace_range(marker.span.clone(), &blank);
    }
    let (schedules, markers): (Vec<_>, Vec<_>) = markers
        .into_iter()
        .partition(|marker| marker.name == "Schedule" || StoredState::marks(&marker.name));
    let uses_behaviors = words(&input.code).any(|word| word == "Behaviors");
    if markers.is_empty() && !uses_behaviors {
        if schedules.is_empty() {
            return Ok(None);
        }
        return Ok(Some(rewrite_source(&input, &schedules, &[], environment)));
    }
    validate_annotated_module(&input, mask.error, environment)?;
    let annotated = AnnotatedModule { markers, schedules };
    lower_annotated_module(&input, &annotated, environment, resources).map(Some)
}

struct AnnotatedModule {
    markers: Vec<Marker>,
    schedules: Vec<Marker>,
}

fn visible_code(source: &str, visible_bytes: &[bool]) -> String {
    let mut bytes = source.as_bytes().to_vec();
    for (byte, visible) in bytes.iter_mut().zip(visible_bytes) {
        if !visible && !matches!(byte, b'\n' | b'\r') {
            *byte = b' ';
        }
    }
    mask_regular_expressions(&mut bytes);
    String::from_utf8(bytes).unwrap_or_default()
}

fn validate_annotated_module(
    input: &Input<'_>,
    lexical_error: Option<&'static str>,
    environment: &str,
) -> Result<(), Failure> {
    if input.source.len() > 1024 * 1024 {
        return Err(input.failure(0, "Annotated behavior source exceeds 1 MiB"));
    }
    if let Some(error) = lexical_error {
        return Err(input.failure(0, error));
    }
    if !matches!(environment, "tac" | "yon") {
        return Err(input.failure(0, "Unknown behavior environment"));
    }
    if words(&input.code).any(reserved_binding) {
        reject_reserved_bindings(input)?;
    }
    Ok(())
}

fn reserved_binding(word: &str) -> bool {
    word == "Behaviors" || word.starts_with("__tachyon_")
}

fn lower_annotated_module(
    input: &Input<'_>,
    module: &AnnotatedModule,
    environment: &str,
    resources: &BTreeMap<String, BTreeSet<String>>,
) -> Result<String, Failure> {
    let mut registrations = classes(input)?;
    record_cases(input, &mut registrations);
    for marker in &module.markers {
        apply(input, &mut registrations, environment, marker)?;
    }
    supply_declared(input, &mut registrations)?;
    let clients = clients::plan(input, &mut registrations, environment, resources)?;
    if environment == "tac" && clients.is_none() {
        infer_required_fields(input, &mut registrations, resources)?;
    }
    resolve_policies(input, &mut registrations, environment)?;
    let erased = module
        .markers
        .iter()
        .chain(&module.schedules)
        .cloned()
        .collect::<Vec<_>>();
    let mut output = rewrite_source(input, &erased, &registrations, environment);
    let entries = behavior_entries(input, &registrations, clients.is_some(), &mut output)?;
    let module_id = module_id(input.path);
    let _ = write!(
        output,
        "\nconst Behaviors = __tachyon_create_behaviors__([{}], \"{environment}\", \"{module_id}\");\n",
        entries.join(",")
    );
    if let Some(clients) = clients {
        output.push_str(&clients);
    }
    if registrations
        .iter()
        .any(|registration| registration.data.contains_key("test"))
    {
        output.push_str(TestAssert::JAVASCRIPT);
    }
    Ok(output)
}

fn behavior_entries(
    input: &Input<'_>,
    registrations: &[Registration],
    managed_clients: bool,
    output: &mut String,
) -> Result<Vec<String>, Failure> {
    let mut entries = registrations
        .iter()
        .filter(|registration| registration.emitted())
        .map(registration_js)
        .collect::<Vec<_>>();
    if !managed_clients {
        return Ok(entries);
    }
    if entries.len() >= 128 {
        return Err(input.failure(
            0,
            "Managed Tac clients permit at most 127 behavior registrations",
        ));
    }
    output.push_str(clients::ClientRuntime::SOURCE);
    entries.push(String::from("{type:__tachyon_ClientConfiguration,name:'TacClientConfiguration',configuration:true,moduleOwned:true,methods:[]}"));
    Ok(entries)
}

fn record_cases(input: &Input<'_>, registrations: &mut [Registration]) {
    for registration in registrations {
        let mut cases = Vec::new();
        for (name, is_static) in direct_members(input, registration) {
            if !is_static && name.starts_with("test") {
                cases.push(Value::String(name));
            }
        }
        registration
            .data
            .insert("cases".to_owned(), Value::Array(cases));
    }
}

fn rewrite_source(
    input: &Input<'_>,
    markers: &[Marker],
    registrations: &[Registration],
    environment: &str,
) -> String {
    let mut edits = Vec::new();
    for marker in markers {
        let span = marker.span.clone();
        let erased = blank_text(&input.source[span.clone()]);
        edits.push((span, erased));
    }
    for registration in registrations {
        push_registration_edits(input, registration, environment, &mut edits);
    }
    edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start));
    let mut output = input.source.to_owned();
    for (span, replacement) in edits {
        output.replace_range(span, &replacement);
    }
    if environment == "tac" {
        append_required_args_export(input, registrations, &mut output);
    }
    output
}

fn blank_text(text: &str) -> String {
    let mut blank = String::new();
    for byte in text.bytes() {
        if matches!(byte, b'\n' | b'\r') {
            blank.push(char::from(byte));
        } else {
            blank.push(' ');
        }
    }
    blank
}

fn push_registration_edits(
    input: &Input<'_>,
    registration: &Registration,
    environment: &str,
    edits: &mut Vec<(Range<usize>, String)>,
) {
    let body_start = registration.open + 1;
    edits.extend(registration.rewrites.iter().cloned());
    if let Some(members) = testing::members(registration) {
        edits.push((body_start..body_start, members));
        if environment == "yon" {
            let line = input.source[..registration.start]
                .rfind('\n')
                .map_or(0, |at| at + 1);
            edits.push((line..line, String::from("@RequestRoute()\n")));
        }
    }
    if let Some(fields) = &registration.constructor {
        let mut assignments = Vec::new();
        for field in fields {
            assignments.push(format!("this.{field} = {field};"));
        }
        let assignments = assignments.join(" ");
        let parameters = fields.join(", ");
        edits.push((
            body_start..body_start,
            format!(" constructor({parameters}) {{ {assignments} }} "),
        ));
    }
}

fn append_required_args_export(
    input: &Input<'_>,
    registrations: &[Registration],
    output: &mut String,
) {
    let Some(exported) = registrations
        .iter()
        .find(|registration| default_export(input, registration))
    else {
        return;
    };
    let Some(fields) = &exported.constructor else {
        return;
    };
    if fields.is_empty() {
        return;
    }
    let names = serde_json::to_string(fields).unwrap_or_default();
    let _ = write!(
        output,
        "\nexport const __tachyon_required_args__ = {names};\n"
    );
}

fn infer_required_fields(
    input: &Input<'_>,
    registrations: &mut [Registration],
    resources: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), Failure> {
    for registration in registrations.iter_mut() {
        if !registration.inferred {
            continue;
        }
        let supplied = resources.get(&registration.name);
        let class_body = registration.open + 1..registration.close;
        let required = crate::yon::required_fields(input.path, input.source, class_body)?;
        let mut fields = Vec::new();
        for (name, _) in required {
            if !supplied.is_some_and(|supplied_fields| supplied_fields.contains(&name)) {
                fields.push(name);
            }
        }
        registration.constructor = Some(fields);
    }
    Ok(())
}

fn default_export(input: &Input<'_>, registration: &Registration) -> bool {
    let line = input.code[..registration.open]
        .rfind('\n')
        .map_or(0, |at| at + 1);
    input.code[line..registration.open].contains("export default class")
}

fn resolve_policies(
    input: &Input<'_>,
    registrations: &mut [Registration],
    environment: &str,
) -> Result<(), Failure> {
    for registration in &mut *registrations {
        let Some(mut entity) = registration.data.remove("entity") else {
            continue;
        };
        fill_entity_identifier(input, registration.start, &registration.data, &mut entity)?;
        registration.data.insert("entity".to_owned(), entity);
    }
    query_frontend::resolve(input, registrations)?;
    let schemas = validated_schemas(registrations);
    for registration in &mut *registrations {
        resolve_registration_policies(input, registration, environment, &schemas)?;
    }
    validate_registrations(input, registrations)
}

fn validated_schemas(registrations: &[Registration]) -> BTreeMap<String, Value> {
    let mut schemas = BTreeMap::new();
    for registration in registrations {
        if let Some(schema) = registration.data.get("validated") {
            schemas.insert(registration.name.clone(), schema.clone());
        }
    }
    schemas
}

fn resolve_registration_policies(
    input: &Input<'_>,
    registration: &mut Registration,
    environment: &str,
    schemas: &BTreeMap<String, Value>,
) -> Result<(), Failure> {
    for metadata in registration.methods.values_mut() {
        parameters::finish(input, registration.start, metadata)?;
    }
    mutations::resolve(input, registration.start, &mut registration.methods)?;
    validation::identifier_agrees(
        input,
        registration.start,
        &registration.data,
        registration.data.get("entity"),
    )?;
    if registration.data.contains_key("configurationProperties") {
        let fields = javascript_settings_fields(input, registration);
        configuration::derive(input, registration.start, &mut registration.data, &fields)?;
    }
    configuration::validate(
        input,
        registration.start,
        &registration.data,
        registration.data.contains_key("configuration"),
        environment,
    )?;
    for metadata in registration.methods.values_mut() {
        validation::resolve(input, registration.start, metadata, schemas)?;
    }
    Ok(())
}

fn javascript_settings_fields(
    input: &Input<'_>,
    registration: &Registration,
) -> Vec<configuration::SettingsField> {
    let mut fields = Vec::new();
    let mut offset = registration.open + 1;
    let mut depth = 0_i64;
    for line in input.code[registration.open + 1..registration.close].split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let line_depth = depth;
        depth += nesting_depth(line, b'{', b'}');
        if line_depth != 0 {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let trimmed_start = line_start + line.len() - line.trim_start().len();
        let source_line = input.source[trimmed_start..line_start + line.trim_end().len()].trim();
        if let Some(field) = javascript_settings_field(input, trimmed_start, source_line) {
            fields.push(field);
        }
    }
    fields
}

fn javascript_settings_field(
    input: &Input<'_>,
    start: usize,
    line: &str,
) -> Option<configuration::SettingsField> {
    let text = line.trim_end_matches(';').trim();
    if text.is_empty() || text.starts_with(['@', '/', '*']) {
        return None;
    }
    let (declaration, initializer) = match text.split_once('=') {
        Some((declaration, initializer)) => (declaration.trim(), Some(initializer.trim())),
        None => (text, None),
    };
    if declaration.contains(['(', '{', '}']) {
        return None;
    }
    let mut words = declaration.split_whitespace().collect::<Vec<_>>();
    let is_static = words.contains(&"static");
    words.retain(|word| {
        !matches!(
            *word,
            "static" | "readonly" | "declare" | "public" | "private" | "protected"
        )
    });
    let declaration = words.join(" ");
    let (name, written_type) = match declaration.split_once(':') {
        Some((name, kind)) => (name.trim(), kind.trim().to_owned()),
        None => (declaration.trim(), String::new()),
    };
    let name = name.trim_end_matches(['?', '!']);
    if !identifier(name) {
        return None;
    }
    let type_source = if written_type.is_empty() && initializer.is_none() {
        parameters::documented_field_type(input.source, &input.code, start)
    } else {
        written_type
    };
    Some(configuration::SettingsField {
        name: name.to_owned(),
        type_source,
        initializer: initializer.map(str::to_owned),
        is_static,
    })
}

fn module_id(path: &Path) -> String {
    let portable = path.to_string_lossy().replace('\\', "/");
    let normalized = portable
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(normalized.as_bytes()) {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn mask_regular_expressions(code: &mut [u8]) {
    let mut at = 0;
    while at < code.len() {
        if code[at] == b'/'
            && expression_can_start(&code[..at])
            && let Some(end) = regular_expression_end(code, at + 1)
        {
            code[at..end].fill(b' ');
            at = end;
            continue;
        }
        at += 1;
    }
}

fn expression_can_start(prefix: &[u8]) -> bool {
    let Some(last) = prefix.iter().rposition(|byte| !byte.is_ascii_whitespace()) else {
        return true;
    };
    if b"=(:,;{[!?|&+-*%^~".contains(&prefix[last]) {
        return true;
    }
    let start = prefix[..=last]
        .iter()
        .rposition(|byte| !byte.is_ascii_alphabetic())
        .map_or(0, |at| at + 1);
    matches!(
        &prefix[start..=last],
        b"return" | b"throw" | b"yield" | b"case"
    )
}

fn regular_expression_end(code: &[u8], mut at: usize) -> Option<usize> {
    let mut character_class = false;
    while at < code.len() {
        match code[at] {
            b'\n' | b'\r' => return None,
            b'\\' => at += 1,
            b'[' => character_class = true,
            b']' => character_class = false,
            b'/' if !character_class => return Some(at + 1),
            _ => {}
        }
        at += 1;
    }
    None
}

fn identifier(value: &str) -> bool {
    identifier_characters(value)
        && value.len() <= 128
        && !reserved_identifier(value)
        && !value.starts_with("__tachyon_")
}

fn identifier_characters(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && !matches!(first, b'_' | b'$') {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
}

fn reserved_identifier(value: &str) -> bool {
    matches!(
        value,
        "__proto__"
            | "prototype"
            | "constructor"
            | "Behaviors"
            | "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "let"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
    )
}

pub(crate) fn is_framework_marker(name: &str) -> bool {
    if name == "allow" {
        return false;
    }
    let name = name
        .strip_prefix("yon::")
        .or_else(|| name.strip_prefix("tac::"))
        .unwrap_or(name);
    let normalized = name.replace('_', "").to_ascii_lowercase();
    framework_marker_names().any(|marker| marker.to_ascii_lowercase() == normalized)
}

pub(crate) fn is_parameter_marker(name: &str) -> bool {
    let normalized = name.replace('_', "").to_ascii_lowercase();
    parameters::ParameterMarkers::NAMES
        .iter()
        .any(|marker| marker.to_ascii_lowercase() == normalized)
}

pub(crate) fn framework_marker_names() -> impl Iterator<Item = &'static str> {
    let lifecycle_names = [
        "OnEvent",
        "OnStart",
        "OnStop",
        "Resource",
        "Order",
        "InBackground",
        "Behaviors",
        "RequestRoute",
        "ErrorHandler",
        "HandlesError",
        "GetRoute",
        "PostRoute",
        "PutRoute",
        "PatchRoute",
        "DeleteRoute",
        "ResponseStatus",
        "FromPath",
        "FromQuery",
        "FromBody",
        "FromHeader",
        "FromCookie",
        "HTTPClient",
        "GetRequest",
        "PostRequest",
        "PutRequest",
        "PatchRequest",
        "DeleteRequest",
        "PathVariable",
        "RequestParam",
        "RequestBody",
        "RequestHeader",
        "Stream",
        "StreamRoute",
        "StreamRequest",
        "SocketRequest",
        "SocketRoute",
        "OnOpen",
        "OnMessage",
        "OnClose",
        "OnError",
        "Topic",
        "Public",
        "SignedIn",
        "Allow",
        "IdentityProvider",
    ];
    Marker::NAMES.into_iter().chain(lifecycle_names)
}

fn words(source: &str) -> impl Iterator<Item = &str> {
    source
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | '$')
        })
        .filter(|word| !word.is_empty())
}

fn reject_reserved_bindings(input: &Input<'_>) -> Result<(), Failure> {
    for line in input.code.lines() {
        if !line.trim_start().starts_with("import ") {
            continue;
        }
        if words(line).any(reserved_binding) {
            return Err(input.failure(
                0,
                "Reserved behavior bindings cannot be imported from another module",
            ));
        }
    }
    let tokens = words(&input.code).collect::<Vec<_>>();
    for pair in tokens.windows(2) {
        let declares = matches!(
            pair[0],
            "class" | "const" | "let" | "var" | "function" | "import"
        );
        if declares && reserved_binding(pair[1]) {
            return Err(input.failure(
                0,
                "Behaviors and __tachyon_* are reserved generated behavior bindings",
            ));
        }
    }
    Ok(())
}

fn markers(input: &Input<'_>) -> Result<Vec<Marker>, Failure> {
    let mut result = Vec::new();
    let mut offset = 0;
    let bracket_annotations = input
        .path
        .extension()
        .is_some_and(|extension| matches!(extension.to_str(), Some("cs" | "rs" | "php")));
    let mut depth = 0_i64;
    for line in input.code.split_inclusive('\n') {
        let line_start = offset + line.len() - line.trim_start().len();
        for (relative, _) in line.match_indices(['@', '[']) {
            let at = offset + relative;
            if input.code.as_bytes()[at] == b'[' && !bracket_annotations {
                continue;
            }
            let inside_parameters = depth + nesting_depth(&line[..relative], b'(', b')') > 0;
            let mut found = None;
            if inside_parameters {
                found = parameters::read(input, input.extension(), at)?;
            }
            if found.is_none() {
                found = line_marker(input, line, offset, line_start, at)?;
            }
            let Some(marker) = found else {
                continue;
            };
            result.push(marker);
            if result.len() > 512 {
                return Err(input.failure(at, "Behavior module exceeds 512 annotations"));
            }
        }
        depth += nesting_depth(line, b'(', b')');
        offset += line.len();
    }
    Ok(result)
}

fn line_marker(
    input: &Input<'_>,
    line: &str,
    offset: usize,
    line_start: usize,
    at: usize,
) -> Result<Option<Marker>, Failure> {
    let name = annotation_name(&input.code[at..]);
    let known = Marker::NAMES
        .iter()
        .any(|marker| marker.eq_ignore_ascii_case(name));
    if !known {
        reject_foreign_annotation(input, at, name)?;
        return Ok(None);
    }
    if at != line_start || !line.trim_start().starts_with('@') {
        return Err(input.failure(at, "Place behavior annotations on their own line in JavaScript or TypeScript; other language forms are not supported yet"));
    }
    let Some(canonical) = Marker::NAMES.iter().find(|marker| **marker == name) else {
        return Err(input.failure(at, "Behavior annotation names are case-sensitive"));
    };
    let line_end = offset + line.trim_end().len();
    let authored = input.source[line_start..line_end].trim();
    let arguments = marker_arguments(input, at, authored, canonical)?;
    Ok(Some(Marker {
        name: (*canonical).to_owned(),
        arguments: arguments.to_owned(),
        span: line_start..line_end,
        target: next_declaration(input, offset + line.len())?,
        parameter: false,
    }))
}

fn annotation_name(text: &str) -> &str {
    let suffix = text.trim_start_matches(['@', '[', '#']);
    let name = suffix
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '_' && character != ':'
        })
        .next()
        .unwrap_or("");
    name.strip_prefix("yon::")
        .or_else(|| name.strip_prefix("tac::"))
        .unwrap_or(name)
}

fn reject_foreign_annotation(input: &Input<'_>, at: usize, name: &str) -> Result<(), Failure> {
    if let Some(message) = RenamedAnnotation::message(name) {
        return Err(input.failure(at, message));
    }
    if let Some(replacement) = retired(name) {
        return Err(input.failure(
            at,
            format!("'{name}' is not a Spring annotation; write {replacement}"),
        ));
    }
    if let Some(instead) = untestable(name) {
        return Err(input.failure(
            at,
            format!("'{name}' is a Spring or JUnit test annotation, which Tachyon does not run; {instead}"),
        ));
    }
    Ok(())
}

fn marker_arguments<'a>(
    input: &Input<'_>,
    at: usize,
    authored: &'a str,
    canonical: &str,
) -> Result<&'a str, Failure> {
    if authored == format!("@{canonical}") {
        return Ok("");
    }
    authored
        .strip_prefix(&format!("@{canonical}("))
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| {
            input.failure(
                at,
                "Behavior annotation requires one complete own-line argument list",
            )
        })
}

fn next_declaration(input: &Input<'_>, mut offset: usize) -> Result<usize, Failure> {
    for line in input.code[offset..].split_inclusive('\n') {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with('@') {
            return Ok(offset + line.len() - line.trim_start().len());
        }
        offset += line.len();
    }
    Err(input.failure(
        offset.min(input.source.len()),
        "Behavior annotation has no following declaration",
    ))
}

fn matching(code: &str, open: usize, left: u8, right: u8) -> Option<usize> {
    let mut depth = 0_u32;
    for (at, byte) in code.bytes().enumerate().skip(open) {
        if byte == left {
            depth = depth.checked_add(1)?;
        }
        if byte == right {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

fn classes(input: &Input<'_>) -> Result<Vec<Registration>, Failure> {
    let mut classes = Vec::new();
    let mut depth = 0_u32;
    let mut cursor = 0;
    let bytes = input.code.as_bytes();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'c' if depth == 0 && input.code[cursor..].starts_with("class ") => {
                if cursor > 0
                    && (bytes[cursor - 1].is_ascii_alphanumeric()
                        || matches!(bytes[cursor - 1], b'_' | b'$'))
                {
                    cursor += 1;
                    continue;
                }
                let registration = class(input, cursor)?;
                cursor = registration.close + 1;
                classes.push(registration);
                if classes.len() > 128 {
                    return Err(input.failure(cursor, "Behavior module exceeds 128 classes"));
                }
                continue;
            }
            _ => {}
        }
        cursor += 1;
    }
    Ok(classes)
}

fn class(input: &Input<'_>, start: usize) -> Result<Registration, Failure> {
    let rest = &input.code[start + 6..];
    let name = rest
        .trim_start()
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | '$')
        })
        .next()
        .unwrap_or("");
    let open = start
        + 6
        + rest
            .find('{')
            .ok_or_else(|| input.failure(start, "Class has no body"))?;
    let close = matching(&input.code, open, b'{', b'}')
        .ok_or_else(|| input.failure(start, "Class body is not balanced"))?;
    let preceding = &input.code[..start];
    let declared = preceding.trim_end().ends_with("abstract");
    let mut rewrites = Vec::new();
    if declared {
        let keyword = preceding.trim_end().len() - "abstract".len();
        rewrites.push((keyword..keyword + "abstract".len(), " ".repeat(8)));
    }
    Ok(Registration {
        name: name.to_owned(),
        start,
        open,
        close,
        inherited: words(&input.code[start..open]).any(|word| word == "extends"),
        declared,
        rewrites,
        ..Registration::default()
    })
}

fn apply(
    input: &Input<'_>,
    registrations: &mut [Registration],
    environment: &str,
    marker: &Marker,
) -> Result<(), Failure> {
    if let Some(index) = class_target(input, registrations, marker) {
        return apply_class(input, &mut registrations[index], environment, marker);
    }
    let Some(registration) = registrations.iter_mut().find(|registration| {
        marker.target > registration.open && marker.target < registration.close
    }) else {
        return Err(input.failure(
            marker.span.start,
            "Behavior annotation requires a named top-level class or its concrete instance method",
        ));
    };
    if matches!(marker.name.as_str(), "Needs" | "Named") {
        return autowired_field(input, registration, marker);
    }
    if marker.name == "Fake" {
        let name = injected_field(input, marker)?;
        return testing::mocked(input, marker, &name, &mut registration.data);
    }
    if marker.name == ValueSource::NAME {
        let method = method(input, registration, marker.target)?;
        return ValueSource::record(input, marker, &method.name, &mut registration.data);
    }
    if validation::field_marker(&marker.name) {
        return apply_validation_field(input, registration, marker);
    }
    if marker.parameter {
        return apply_parameter(input, registration, marker);
    }
    apply_method(input, registration, marker)
}

fn apply_parameter(
    input: &Input<'_>,
    registration: &mut Registration,
    marker: &Marker,
) -> Result<(), Failure> {
    if !identifier(&registration.name) || registration.inherited {
        return Err(input.failure(
            marker.span.start,
            "Behavior methods require a named non-inheriting class",
        ));
    }
    let parameter = parameters::marked_parameter(input, marker)?;
    let method = method(input, registration, parameter.declaration)?;
    if method.is_static {
        return Err(input.failure(
            marker.span.start,
            "Behavior annotations require instance methods",
        ));
    }
    let declared_type = javascript_declared_type(input, &method, parameter.index);
    let policy = parameters::policy(input, marker, parameter.index, &declared_type)?;
    concrete_member(registration, &method);
    let metadata = registration.methods.entry(method.name).or_default();
    parameters::record(input, marker.span.start, metadata, policy)
}

fn javascript_declared_type(input: &Input<'_>, method: &Method, index: usize) -> String {
    let pieces = polyglot::split_parameters(&input.code[method.parameters.clone()]);
    let Some(piece) = pieces.get(index) else {
        return String::new();
    };
    if let Some((_, kind)) = piece.split_once(':') {
        return kind.trim().to_owned();
    }
    let name = piece.trim();
    let start = input.code[..method.parameters.start]
        .rfind('\n')
        .map_or(0, |end| end + 1);
    parameters::documented_type(input.source, &input.code, start, name)
}

fn class_target(
    input: &Input<'_>,
    registrations: &[Registration],
    marker: &Marker,
) -> Option<usize> {
    registrations.iter().position(|registration| {
        if marker.target > registration.start {
            return false;
        }
        input.code[marker.target..registration.start]
            .split_whitespace()
            .all(|word| matches!(word, "export" | "default" | "abstract"))
    })
}

fn apply_validation_field(
    input: &Input<'_>,
    registration: &mut Registration,
    marker: &Marker,
) -> Result<(), Failure> {
    let text_before_field = &input.code[registration.open + 1..marker.target];
    if nesting_depth(text_before_field, b'{', b'}') != 0 {
        return Err(input.failure(
            marker.target,
            "Field annotations must be direct class members",
        ));
    }
    let language = input
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    validation::apply_field(input, marker, language, &mut registration.data)
}

fn nesting_depth(text: &str, open: u8, close: u8) -> i64 {
    let mut depth = 0_i64;
    for byte in text.bytes() {
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
        }
    }
    depth
}

fn injected_field(input: &Input<'_>, marker: &Marker) -> Result<String, Failure> {
    let end = input.code[marker.target..]
        .find('\n')
        .map_or(input.code.len(), |end| marker.target + end);
    let line = input.source[marker.target..end]
        .trim()
        .trim_end_matches(';')
        .trim();
    let declaration = without_instance_field_modifiers(input, marker.target, &marker.name, line)?;
    let name = declaration
        .split([':', '=', ' '])
        .next()
        .unwrap_or_default()
        .trim_end_matches(['!', '?']);
    if !identifier(name) || declaration.contains(['=', '(', ')']) {
        return Err(input.failure(
            marker.target,
            format!(
                "{} requires an uninitialized named instance field",
                marker.name
            ),
        ));
    }
    Ok(name.to_owned())
}

fn without_instance_field_modifiers<'a>(
    input: &Input<'_>,
    target: usize,
    annotation: &str,
    mut declaration: &'a str,
) -> Result<&'a str, Failure> {
    while let Some((modifier, rest)) = declaration.split_once(' ') {
        if modifier == "static" {
            return Err(input.failure(target, format!("{annotation} requires an instance field")));
        }
        if !matches!(
            modifier,
            "public" | "private" | "protected" | "readonly" | "declare"
        ) {
            break;
        }
        declaration = rest.trim_start();
    }
    Ok(declaration)
}

fn autowired_field(
    input: &Input<'_>,
    registration: &mut Registration,
    marker: &Marker,
) -> Result<(), Failure> {
    let name = injected_field(input, marker)?;
    if marker.name == "Named" {
        return qualifier_field(input, registration, marker, &name);
    }
    if !marker.arguments.trim().is_empty() {
        return Err(input.failure(
            marker.span.start,
            crate::annotation_rules::AnnotationRules::no_arguments("Needs"),
        ));
    }
    let fields = registration.constructor.get_or_insert_with(Vec::new);
    if fields.contains(&name) {
        return Err(input.failure(marker.span.start, "One Needs per field"));
    }
    fields.push(name);
    Ok(())
}

fn qualifier_field(
    input: &Input<'_>,
    registration: &mut Registration,
    marker: &Marker,
    name: &str,
) -> Result<(), Failure> {
    let provider = marker.arguments.trim().trim_matches(['"', '\'']).trim();
    if !identifier(provider) {
        return Err(input.failure(marker.span.start, "Named names one provider type"));
    }
    registration
        .data
        .entry("qualifiers")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(Value::Object(qualifiers)) = registration.data.get_mut("qualifiers") else {
        return Ok(());
    };
    let previous = qualifiers.insert(name.to_owned(), Value::String(provider.to_owned()));
    if previous.is_some() {
        return Err(input.failure(marker.span.start, "One Named per field"));
    }
    Ok(())
}

fn inject_fields(input: &Input<'_>, marker: &Marker) -> Result<Vec<String>, Failure> {
    let mut fields = Vec::new();
    for item in marker.arguments.split(',').map(str::trim) {
        if item.is_empty() {
            continue;
        }
        if !identifier(item) {
            return Err(input.failure(marker.span.start, "Needs lists provider identifiers"));
        }
        fields.push(lowercase_first_letter(item));
    }
    if fields.is_empty() || fields.len() > 32 {
        return Err(input.failure(marker.span.start, "Needs lists 1 through 32 providers"));
    }
    Ok(fields)
}

fn lowercase_first_letter(text: &str) -> String {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_ascii_lowercase().to_string() + characters.as_str()
}

fn flag_without_arguments(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    if marker.arguments.trim().is_empty() {
        return Ok(Value::Bool(true));
    }
    Err(input.failure(
        marker.span.start,
        crate::annotation_rules::AnnotationRules::no_arguments(&marker.name),
    ))
}

fn class_policy(
    input: &Input<'_>,
    registration: &mut Registration,
    environment: &str,
    marker: &Marker,
) -> Result<Option<Value>, Failure> {
    if apply_class_setting(input, registration, marker)? {
        return Ok(None);
    }
    class_value(input, registration, environment, marker).map(Some)
}

fn apply_class_setting(
    input: &Input<'_>,
    registration: &mut Registration,
    marker: &Marker,
) -> Result<bool, Failure> {
    match marker.name.as_str() {
        "Client" => {
            if client_streams(input, marker)? {
                registration
                    .data
                    .insert("clientStreams".into(), Value::Bool(true));
            }
        }
        "Needs" => {
            registration.constructor = Some(inject_fields(input, marker)?);
        }
        "NeedsAll" => {
            let named = string_list(input, marker)?;
            registration.inferred = named.is_empty();
            registration.constructor = Some(named);
        }
        "Store" if marker.arguments.trim().is_empty() => {
            registration
                .data
                .insert("repositoryRole".into(), Value::Bool(true));
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn class_value(
    input: &Input<'_>,
    registration: &mut Registration,
    environment: &str,
    marker: &Marker,
) -> Result<Value, Failure> {
    Ok(match marker.name.as_str() {
        "Setup" => {
            if !marker.arguments.trim().is_empty() {
                return Err(input.failure(
                    marker.span.start,
                    crate::annotation_rules::AnnotationRules::no_arguments("Setup"),
                ));
            }
            Value::Bool(true)
        }
        "Service" | "Helper" | "Converter" | "FreshStart" => flag_without_arguments(input, marker)?,
        "Test" => testing::unit(input, marker)?,
        "TestSetup" => {
            let flagged = flag_without_arguments(input, marker)?;
            registration
                .data
                .insert("configuration".into(), Value::Bool(true));
            flagged
        }
        "TestSettings" => testing::properties(input, marker, environment)?,
        "Lifetime" => scope(input, marker)?,
        "CacheSettings" => cache_config(input, marker)?,
        "Entity" => entity(input, marker)?,
        "Table" => Value::String(single_string(input, marker)?),
        "Description" => description(input, marker)?,
        "Store" => {
            if environment == "tac" {
                registration
                    .data
                    .insert("repositoryRole".into(), Value::Bool(true));
            }
            queries::repository(input, marker)?
        }
        "Settings" => configuration::properties(input, marker)?,
        "WhenProfile" | "UseProfiles" => configuration::profiles(input, marker)?,
        "WhenSetting" => configuration::conditional(input, marker)?,
        "Checked" => validation::schema(input, marker)?,
        _ => {
            return Err(input.failure(
                marker.span.start,
                crate::annotation_rules::AnnotationRules::belongs(&marker.name),
            ));
        }
    })
}

fn client_streams(input: &Input<'_>, marker: &Marker) -> Result<bool, Failure> {
    let options = client_options(input, marker)?;
    Ok(options.get("streams") == Some(&Value::Bool(true)))
}

fn client_options(input: &Input<'_>, marker: &Marker) -> Result<Map<String, Value>, Failure> {
    if marker.arguments.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: Value = literal::parse(&marker.arguments).map_err(|_| {
        input.failure(
            marker.span.start,
            "Behavior configuration must be a strict JSON object, never an executable expression",
        )
    })?;
    let known = value
        .as_object()
        .filter(|options| !options.is_empty() && options.keys().all(|key| key == "streams"));
    known.cloned().ok_or_else(|| {
        input.failure(
            marker.span.start,
            "Client takes a JSON object with the key streams",
        )
    })
}

fn apply_class(
    input: &Input<'_>,
    registration: &mut Registration,
    environment: &str,
    marker: &Marker,
) -> Result<(), Failure> {
    if !identifier(&registration.name) || registration.inherited {
        return Err(input.failure(
            marker.span.start,
            "Annotated classes require a safe explicit name and cannot extend another class",
        ));
    }
    if !registration.markers.insert(marker.name.clone()) {
        return Err(input.failure(marker.span.start, "Duplicate behavior class annotation"));
    }
    let Some(value) = class_policy(input, registration, environment, marker)? else {
        return Ok(());
    };
    let key = class_data_key(&marker.name);
    registration.data.insert(key.to_owned(), value);
    Ok(())
}

fn class_data_key(marker_name: &str) -> &'static str {
    match marker_name {
        "Setup" => "configuration",
        "Description" => "description",
        "Service" => "service",
        "Lifetime" => "scope",
        "Helper" => "helper",
        "Converter" => "mapper",
        "Test" => "test",
        "TestSetup" => "testConfiguration",
        "UseProfiles" => "activeProfiles",
        "TestSettings" => "testProperties",
        "FreshStart" => "dirtiesContext",
        "CacheSettings" => "cache",
        "Entity" => "entity",
        "Store" => "repository",
        "Settings" => "configurationProperties",
        "WhenProfile" => "profiles",
        "WhenSetting" => "conditionalOnProperty",
        "Checked" => "validated",
        _ => "table",
    }
}

fn scope(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let options = object(input, marker, &["value"])?;
    let scope = options
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| input.failure(marker.span.start, "Lifetime requires a literal value"))?;
    if !NativeApplicationLifetime::supported(input.path, scope) {
        return Err(input.failure(marker.span.start, NativeApplicationLifetime::advice(scope)));
    }
    Ok(Value::String(scope.to_owned()))
}

struct Method {
    name: String,
    asynchronous: bool,
    is_static: bool,
    parameters: Range<usize>,
    body: Range<usize>,
    declared: bool,
    modifier: Option<Range<usize>>,
}

impl Method {
    const HTTP_VERBS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"];
}

fn method(input: &Input<'_>, registration: &Registration, start: usize) -> Result<Method, Failure> {
    let text_before_method = &input.code[registration.open + 1..start];
    if nesting_depth(text_before_method, b'{', b'}') != 0 {
        return Err(input.failure(
            start,
            "Behavior methods must be declared directly in their class",
        ));
    }
    let remaining = &input.code[start..registration.close];
    let Some(relative_open) = remaining.find('(') else {
        return Err(input.failure(start, "Behavior method must have a concrete parameter list"));
    };
    let prefix = remaining[..relative_open].trim();
    let tokens = prefix.split_whitespace().collect::<Vec<_>>();
    let name = tokens.last().copied().unwrap_or("");
    let modifiers = &tokens[..tokens.len().saturating_sub(1)];
    let modifiers_allowed = modifiers
        .iter()
        .all(|modifier| member_modifier(modifier, registration.declared));
    if !identifier(name) || !modifiers_allowed {
        return Err(input.failure(start, "Behavior methods must be named concrete methods, not generators, accessors, fields or computed members"));
    }
    let open = start + relative_open;
    let Some(close) = matching(&input.code, open, b'(', b')') else {
        return Err(input.failure(start, "Behavior method parameters are not balanced"));
    };
    let (body, declared) = method_body(input, registration, start, close)?;
    Ok(Method {
        name: name.to_owned(),
        asynchronous: modifiers.contains(&"async"),
        is_static: modifiers.contains(&"static"),
        parameters: open + 1..close,
        body,
        declared,
        modifier: abstract_keyword(input, start, open),
    })
}

fn method_body(
    input: &Input<'_>,
    registration: &Registration,
    start: usize,
    close: usize,
) -> Result<(Range<usize>, bool), Failure> {
    let tail = input.code[close + 1..registration.close].trim_start();
    if !tail.starts_with('{') && !tail.starts_with(':') {
        return Err(input.failure(start, "Behavior methods require a concrete body"));
    }
    if let Some(body) = declared_body(input, registration, close) {
        return Ok((body, true));
    }
    let body = concrete_body(input, close + 1, registration.close)?;
    Ok((body, false))
}

fn member_modifier(modifier: &str, abstract_allowed: bool) -> bool {
    if matches!(
        modifier,
        "async" | "static" | "public" | "private" | "protected"
    ) {
        return true;
    }
    modifier == "abstract" && abstract_allowed
}

fn abstract_keyword(input: &Input<'_>, start: usize, open: usize) -> Option<Range<usize>> {
    let relative = input.code[start..open].find("abstract")?;
    let keyword_start = start + relative;
    Some(keyword_start..keyword_start + "abstract".len())
}

fn concrete_member(registration: &mut Registration, method: &Method) {
    if !method.declared {
        return;
    }
    registration
        .rewrites
        .push((method.body.clone(), "{}".to_owned()));
    if let Some(modifier) = &method.modifier {
        registration
            .rewrites
            .push((modifier.clone(), " ".repeat(modifier.len())));
    }
}

fn declared_body(
    input: &Input<'_>,
    registration: &Registration,
    close: usize,
) -> Option<Range<usize>> {
    if !registration.declared {
        return None;
    }
    let tail = &input.code[close + 1..registration.close];
    let terminator = tail.find([';', '{'])?;
    (tail.as_bytes()[terminator] == b';').then(|| close + 1 + terminator..close + 2 + terminator)
}

fn concrete_body(input: &Input<'_>, start: usize, end: usize) -> Result<Range<usize>, Failure> {
    let tail = &input.code[start..end];
    let mut cursor = 0;
    loop {
        let relative = tail[cursor..]
            .find('{')
            .map(|at| cursor + at)
            .ok_or_else(|| input.failure(start, "Behavior methods require a concrete body"))?;
        let prefix = &tail[..relative];
        if tail[cursor..relative].contains(';') {
            return Err(input.failure(
                start,
                "Abstract, declared and overloaded behavior methods are unsupported",
            ));
        }
        let close = matching(&input.code, start + relative, b'{', b'}')
            .filter(|close| *close < end)
            .ok_or_else(|| input.failure(start, "Behavior method body is not balanced"))?;
        if type_literal_prefix(prefix) {
            cursor = close + 1 - start;
            continue;
        }
        return Ok(start + relative + 1..close);
    }
}

fn type_literal_prefix(prefix: &str) -> bool {
    let prefix = prefix.trim();
    if !prefix.starts_with(':') {
        return false;
    }
    let angles = nesting_depth(prefix, b'<', b'>');
    prefix == ":" || angles > 0 || prefix.ends_with(['|', '&', ':'])
}

fn method_policy(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
    owner: &str,
) -> Result<(&'static str, Value), Failure> {
    match marker.name.as_str() {
        "Cached" => cacheable_method_policy(input, method, marker),
        "Query" => Ok(("query", queries::explicit(input, marker)?)),
        "Check" | "PageDefaults" | "CacheKey" => Err(parameters::refuse_method_form(input, marker)),
        "ClearCache" | "UpdateCache" => cache_mutation_method_policy(input, method, marker),
        "Retry" | "MaxAtOnce" | "AllOrNothing" => resilience_method_policy(input, method, marker),
        "Convert" => mapping_method_policy(input, method, marker, owner),
        "WebCall" => http_exchange_method_policy(input, method, marker),
        "Gateway" => gateway_method_policy(input, method, marker, owner),
        _ => Err(input.failure(
            marker.span.start,
            crate::annotation_rules::AnnotationRules::belongs(&marker.name),
        )),
    }
}

fn cacheable_method_policy(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
) -> Result<(&'static str, Value), Failure> {
    if !method.asynchronous || Method::HTTP_VERBS.contains(&method.name.as_str()) {
        return Err(input.failure(marker.span.start, "Cached requires an async service method, never an HTTP verb endpoint; authorize before calling the service"));
    }
    let count = polyglot::split_parameters(&input.code[method.parameters.clone()]).len();
    Ok(("cacheable", cacheable(input, marker, count)?))
}

fn cache_mutation_method_policy(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
) -> Result<(&'static str, Value), Failure> {
    if !method.asynchronous {
        return Err(input.failure(
            marker.span.start,
            "Cache mutations require async service methods",
        ));
    }
    let key = if marker.name == "UpdateCache" {
        "cachePut"
    } else {
        "cacheEvict"
    };
    Ok((key, mutations::policy(input, marker)?))
}

fn resilience_method_policy(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
) -> Result<(&'static str, Value), Failure> {
    if !method.asynchronous {
        return Err(input.failure(
            marker.span.start,
            format!("{} requires an async concrete method", marker.name),
        ));
    }
    if Method::HTTP_VERBS.contains(&method.name.as_str()) {
        return Err(input.failure(
            marker.span.start,
            format!(
                "{} belongs on a service method, never an HTTP verb endpoint",
                marker.name
            ),
        ));
    }
    resilience_policy(input, "js", marker)
}

fn mapping_method_policy(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
    owner: &str,
) -> Result<(&'static str, Value), Failure> {
    if !method.asynchronous && !method.declared {
        return Err(input.failure(
            marker.span.start,
            "Convert requires an async concrete method",
        ));
    }
    let body = if method.declared {
        ""
    } else {
        input.code[method.body.clone()].trim()
    };
    if !body.is_empty() {
        return Err(input.failure(
            marker.span.start,
            "Convert writes the whole method; declare its signature with an empty body",
        ));
    }
    let mut signature = mapper::ConvertSignature {
        method: format!("{owner}.{}", method.name),
        parameter_types: javascript_parameter_types(input, method),
        return_type: javascript_result_type(input, method),
    };
    let documented = jsdoc_signature::JsdocSignature::before(input.source, marker.span.start);
    if let (Some((parameter, result)), Some(first)) =
        (documented, signature.parameter_types.first_mut())
        && first.is_empty()
    {
        *first = parameter;
        signature.return_type = result;
    }
    Ok(("mapping", mapper::mapping(input, marker, &signature)?))
}

fn http_exchange_method_policy(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
) -> Result<(&'static str, Value), Failure> {
    if !method.asynchronous && !method.declared {
        return Err(input.failure(
            marker.span.start,
            "A request method (@GetRequest, @PostRequest, @PutRequest, @PatchRequest or @DeleteRequest) must be async, because the answer arrives later",
        ));
    }
    let body = if method.declared {
        ""
    } else {
        &input.code[method.body.clone()]
    };
    Ok(("httpExchange", exchange_policy(input, marker, body, "js")?))
}

fn gateway_method_policy(
    input: &Input<'_>,
    method: &Method,
    marker: &Marker,
    owner: &str,
) -> Result<(&'static str, Value), Failure> {
    if !method.asynchronous && !method.declared {
        return Err(input.failure(
            marker.span.start,
            "Gateway requires an async method, because the other language answers later",
        ));
    }
    let mut parameter_types = Vec::new();
    for parameter in polyglot::split_parameters(&input.source[method.parameters.clone()]) {
        if parameter.contains(['=', '{', '[']) || parameter.starts_with("...") {
            return Err(input.failure(
                marker.span.start,
                "Gateway parameters must be plain positional names",
            ));
        }
        parameter_types.push(
            parameter
                .split_once(':')
                .map_or("", |(_, kind)| kind.trim()),
        );
    }
    let language = input
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let declaration = gateway::GatewayMethod {
        owner,
        name: &method.name,
        language,
        parameter_types,
        result_type: javascript_result_type(input, method),
        has_body: !method.declared && !input.code[method.body.clone()].trim().is_empty(),
    };
    Ok(("gateway", gateway::policy(input, marker, &declaration)?))
}

fn javascript_result_type<'a>(input: &Input<'a>, method: &Method) -> &'a str {
    let signature_end = if method.declared {
        method.body.start
    } else {
        method.body.start.saturating_sub(1)
    };
    let tail = input.source[method.parameters.end + 1..signature_end].trim();
    tail.strip_prefix(':').unwrap_or(tail).trim()
}

fn javascript_input<'a>(path: &'a Path, source: &'a str) -> Input<'a> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mask = crate::lexical::code_mask(extension, source);
    Input {
        path,
        source,
        code: visible_code(source, &mask.code),
    }
}

fn javascript_parameter_types<'a>(input: &Input<'a>, method: &Method) -> Vec<&'a str> {
    let mut types = Vec::new();
    for parameter in polyglot::split_parameters(&input.source[method.parameters.clone()]) {
        types.push(
            parameter
                .split_once(':')
                .map_or("", |(_, kind)| kind.trim()),
        );
    }
    types
}

fn javascript_target_signature(
    path: &Path,
    source: &str,
    class: &str,
    method_name: &str,
) -> Result<Option<gateway::TargetSignature>, Failure> {
    let input = javascript_input(path, source);
    let registrations = classes(&input)?;
    let Some(registration) = registrations.iter().find(|item| item.name == class) else {
        return Ok(None);
    };
    let mut start = None;
    for (name, _, member_start) in direct_member_entries(&input, registration) {
        if name == method_name {
            start = Some(member_start);
        }
    }
    let Some(start) = start else {
        return Ok(None);
    };
    let Ok(method) = method(&input, registration, start) else {
        return Ok(None);
    };
    let parameter_types = javascript_parameter_types(&input, &method)
        .into_iter()
        .map(str::to_owned)
        .collect();
    Ok(Some(gateway::TargetSignature {
        parameter_types,
        result_type: javascript_result_type(&input, &method).to_owned(),
        is_static: method.is_static,
        has_body: !method.declared,
        has_supported_receiver: true,
    }))
}

fn javascript_gateway_policies(path: &Path, source: &str) -> Result<Vec<Value>, Failure> {
    let input = javascript_input(path, source);
    let registrations = classes(&input)?;
    let mut policies = Vec::new();
    for marker in markers(&input)? {
        if marker.name != "Gateway" {
            continue;
        }
        let Some(registration) = registrations.iter().find(|registration| {
            marker.target > registration.open && marker.target < registration.close
        }) else {
            continue;
        };
        let method = method(&input, registration, marker.target)?;
        let (_, policy) = gateway_method_policy(&input, &method, &marker, &registration.name)?;
        policies.push(policy);
    }
    Ok(policies)
}

fn apply_method(
    input: &Input<'_>,
    registration: &mut Registration,
    marker: &Marker,
) -> Result<(), Failure> {
    if !identifier(&registration.name) || registration.inherited {
        return Err(input.failure(
            marker.span.start,
            "Behavior methods require a named non-inheriting class",
        ));
    }
    let method = method(input, registration, marker.target)?;
    if method.is_static {
        return Err(input.failure(
            marker.span.start,
            "Behavior annotations require instance methods",
        ));
    }
    let (key, value) = method_policy(input, &method, marker, &registration.name)?;
    concrete_member(registration, &method);
    let names = javascript_parameter_names(input, &method);
    let metadata = registration.methods.entry(method.name).or_default();
    if metadata.insert(key.to_owned(), value).is_some() {
        return Err(input.failure(
            marker.span.start,
            "Duplicate behavior method annotation or duplicate annotated method",
        ));
    }
    if mutations::uses_parameter_names(key) {
        metadata.insert("parameterNames".to_owned(), names);
    }
    Ok(())
}

fn javascript_parameter_names(input: &Input<'_>, method: &Method) -> Value {
    let mut names = Vec::new();
    for piece in polyglot::split_parameters(&input.code[method.parameters.clone()]) {
        let name = piece
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .trim_end_matches('?');
        names.push(Value::String(name.to_owned()));
    }
    Value::Array(names)
}

fn object(
    input: &Input<'_>,
    marker: &Marker,
    keys: &[&str],
) -> Result<Map<String, Value>, Failure> {
    let value: Value = literal::parse(&marker.arguments).map_err(|_| {
        input.failure(
            marker.span.start,
            "Behavior configuration must be a strict JSON object, never an executable expression",
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        input.failure(
            marker.span.start,
            "Behavior configuration must be a JSON object",
        )
    })?;
    if object.len() != keys.len() || object.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(input.failure(
            marker.span.start,
            format!(
                "{} requires exactly these keys: {}",
                marker.name,
                keys.join(", ")
            ),
        ));
    }
    Ok(object.clone())
}

fn bounded_integer(
    input: &Input<'_>,
    marker: &Marker,
    object: &Map<String, Value>,
    key: &str,
    minimum: u64,
    maximum: u64,
) -> Result<(), Failure> {
    if object
        .get(key)
        .and_then(Value::as_u64)
        .is_some_and(|value| (minimum..=maximum).contains(&value))
    {
        return Ok(());
    }
    Err(input.failure(
        marker.span.start,
        format!(
            "{}.{key} must be an integer from {minimum} to {maximum}",
            marker.name
        ),
    ))
}

fn named(
    input: &Input<'_>,
    marker: &Marker,
    object: &Map<String, Value>,
    key: &str,
) -> Result<(), Failure> {
    if object
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(identifier)
    {
        return Ok(());
    }
    Err(input.failure(
        marker.span.start,
        format!("{}.{key} requires a safe bounded identifier", marker.name),
    ))
}

fn cache_config(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let object = object(input, marker, &["name", "ttlMs", "maxBytes", "maxEntries"])?;
    named(input, marker, &object, "name")?;
    bounded_integer(input, marker, &object, "ttlMs", 1, 86_400_000)?;
    bounded_integer(input, marker, &object, "maxBytes", 1, 1_048_576)?;
    bounded_integer(input, marker, &object, "maxEntries", 1, 10_000)?;
    Ok(Value::Object(object))
}

fn entity(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let written = literal::parse(&marker.arguments).ok();
    if written
        .as_ref()
        .is_some_and(|value| value.get("id").is_some())
    {
        return Err(input.failure(
            marker.span.start,
            "Entity no longer names its identifier: remove \"id\" and mark the identifier field with @Id",
        ));
    }
    let object = object(input, marker, &["adapter"])?;
    named(input, marker, &object, "adapter")?;
    Ok(Value::Object(object))
}

fn fill_entity_identifier(
    input: &Input<'_>,
    at: usize,
    policies: &Map<String, Value>,
    entity: &mut Value,
) -> Result<(), Failure> {
    let Some(field) = policies.get("identifier") else {
        return Err(input.failure(
            at,
            "An Entity needs exactly one field marked @Id; that field is the identifier",
        ));
    };
    entity["id"] = field.clone();
    Ok(())
}

fn cacheable(input: &Input<'_>, marker: &Marker, parameter_count: usize) -> Result<Value, Failure> {
    if !marker.arguments.trim().is_empty() {
        return Err(input.failure(
            marker.span.start,
            "Cached takes no arguments: mark each key parameter with @CacheKey, or mark none to use every parameter",
        ));
    }
    if parameter_count > 32 {
        return Err(input.failure(
            marker.span.start,
            "A Cached method has at most 32 parameters",
        ));
    }
    Ok(parameters::every_parameter(parameter_count))
}

fn resilience_policy(
    input: &Input<'_>,
    language: &str,
    marker: &Marker,
) -> Result<(&'static str, Value), Failure> {
    if marker.name == "Retry" {
        return Ok(("retryable", resilience::retryable(input, marker)?));
    }
    if marker.name == "AllOrNothing" {
        let object = object(input, marker, &["adapter"])?;
        named(input, marker, &object, "adapter")?;
        return Ok(("transactional", Value::Object(object)));
    }
    Ok((
        "concurrencyLimit",
        resilience::concurrency_limit(input, language, marker)?,
    ))
}

fn pageable_default(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let object = object(
        input,
        marker,
        &["argument", "defaultSize", "maxSize", "sort"],
    )?;
    bounded_integer(input, marker, &object, "argument", 0, 31)?;
    bounded_integer(input, marker, &object, "defaultSize", 1, 1000)?;
    bounded_integer(input, marker, &object, "maxSize", 1, 1000)?;
    if object["defaultSize"].as_u64() > object["maxSize"].as_u64() {
        return Err(input.failure(
            marker.span.start,
            "Pageable.defaultSize cannot exceed maxSize",
        ));
    }
    let Some(fields) = object["sort"].as_array() else {
        return Err(input.failure(
            marker.span.start,
            "Pageable.sort requires an explicit field allowlist",
        ));
    };
    if !unique_field_names(fields) {
        return Err(input.failure(
            marker.span.start,
            "Pageable.sort requires up to 32 unique safe field names",
        ));
    }
    Ok(Value::Object(object))
}

fn unique_field_names(fields: &[Value]) -> bool {
    if fields.len() > 32 {
        return false;
    }
    let mut unique = BTreeSet::new();
    for field in fields {
        let Some(name) = field.as_str() else {
            return false;
        };
        if !identifier(name) || !unique.insert(name) {
            return false;
        }
    }
    true
}

fn http_exchange(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let object = resilience::options(
        input,
        marker,
        &["client", "method", "path"],
        &["query", "headers", "body", "pathVariables"],
    )?;
    named(input, marker, &object, "client")?;
    if !object["method"]
        .as_str()
        .is_some_and(|method| Method::HTTP_VERBS.contains(&method))
    {
        return Err(input.failure(
            marker.span.start,
            "A request method sends GET, POST, PUT, PATCH or DELETE; use @GetRequest, @PostRequest, @PutRequest, @PatchRequest or @DeleteRequest",
        ));
    }
    if !object["path"].as_str().is_some_and(safe_path) {
        return Err(input.failure(marker.span.start, "The request path must start with '/' and may not hold a query (?), a fragment (#), a backslash or '..'. Put query values in @RequestParam parameters"));
    }
    for key in ["query", "headers", "pathVariables"] {
        if let Some(mapping) = object.get(key) {
            validate_argument_mapping(input, marker, key, mapping)?;
        }
    }
    validate_path_variables(input, marker, &object)?;
    if let Some(body) = object.get("body") {
        validate_request_body(input, marker, &object, body)?;
    }
    Ok(Value::Object(object))
}

fn validate_path_variables(
    input: &Input<'_>,
    marker: &Marker,
    object: &Map<String, Value>,
) -> Result<(), Failure> {
    let path = object["path"].as_str().unwrap_or("");
    let declared = object.get("pathVariables").and_then(Value::as_object);
    let mut written = Vec::new();
    for segment in path.split('/') {
        if let Some(name) = segment
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
        {
            written.push(name);
        }
    }
    for name in &written {
        if !declared.is_some_and(|declared| declared.contains_key(*name)) {
            return Err(input.failure(
                marker.span.start,
                format!("The request path has {{{name}}}, so one parameter needs @PathVariable(\"{name}\"). @PathVariable means the value fills that part of the path"),
            ));
        }
    }
    for name in declared.into_iter().flat_map(|declared| declared.keys()) {
        if !written.contains(&name.as_str()) {
            return Err(input.failure(
                marker.span.start,
                format!("@PathVariable(\"{name}\") means the value fills {{{name}}} in the path, but the path '{path}' has no {{{name}}}"),
            ));
        }
    }
    Ok(())
}

fn validate_argument_mapping(
    input: &Input<'_>,
    marker: &Marker,
    key: &str,
    mapping: &Value,
) -> Result<(), Failure> {
    let Some(entries) = mapping.as_object().filter(|entries| entries.len() <= 32) else {
        return Err(input.failure(
            marker.span.start,
            format!("A request method maps up to 32 {key} names to its parameters"),
        ));
    };
    for (name, index) in entries {
        let bounded_name = !name.is_empty() && name.len() <= 256;
        let valid_index = index.as_u64().is_some_and(|index| index < 32);
        if !bounded_name || !valid_index {
            return Err(input.failure(
                marker.span.start,
                format!("A request method maps each {key} name (at most 256 characters) to one of its first 32 parameters"),
            ));
        }
    }
    Ok(())
}

fn validate_request_body(
    input: &Input<'_>,
    marker: &Marker,
    object: &Map<String, Value>,
    body: &Value,
) -> Result<(), Failure> {
    if body.as_u64().is_none_or(|index| index >= 32) {
        return Err(input.failure(
            marker.span.start,
            "@RequestBody must be on one of the first 32 parameters",
        ));
    }
    if matches!(object["method"].as_str(), Some("GET" | "HEAD")) {
        return Err(input.failure(
            marker.span.start,
            "@GetRequest means the method sends a GET request, and a GET request has no body. Remove @RequestBody or use @PostRequest",
        ));
    }
    Ok(())
}

fn exchange_policy(
    input: &Input<'_>,
    marker: &Marker,
    body: &str,
    language: &str,
) -> Result<Value, Failure> {
    let mut policy = http_exchange(input, marker)?;
    let body = body.trim();
    if body.is_empty() || (language == "py" && matches!(body, "pass" | "...")) {
        policy["generated"] = Value::Bool(true);
    } else if ["query", "headers", "body", "pathVariables"]
        .iter()
        .any(|key| policy.get(*key).is_some())
    {
        return Err(input.failure(
            marker.span.start,
            "A request method with @PathVariable, @RequestParam, @RequestHeader or @RequestBody parameters has an empty body: Tachyon writes the request",
        ));
    }
    Ok(policy)
}

fn safe_path(path: &str) -> bool {
    if !path.starts_with('/') || path.starts_with("//") || path.len() > 2048 {
        return false;
    }
    let Some(decoded) = decode_path(path) else {
        return false;
    };
    !decoded.starts_with("//")
        && !decoded.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '\\' | '?' | '#' | '%' | '\u{feff}')
        })
        && !decoded
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
}

fn decode_path(path: &str) -> Option<String> {
    let mut bytes = path.bytes();
    let mut output = Vec::with_capacity(path.len());
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = char::from(bytes.next()?).to_digit(16)?;
            let low = char::from(bytes.next()?).to_digit(16)?;
            output.push(u8::try_from(high * 16 + low).ok()?);
        } else {
            output.push(byte);
        }
    }
    String::from_utf8(output).ok()
}

fn single_string(input: &Input<'_>, marker: &Marker) -> Result<String, Failure> {
    let values = string_list(input, marker)?;
    if values.len() != 1 {
        return Err(input.failure(
            marker.span.start,
            "Table requires exactly one literal table identifier",
        ));
    }
    Ok(values[0].clone())
}

fn description(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let text = marker.arguments.trim();
    let Some(unquoted) = quoted_text(text).filter(|text| plain_description(text)) else {
        return Err(input.failure(
            marker.span.start,
            "Description requires one quoted line of at most 256 characters, without quotes, escapes or control characters",
        ));
    };
    Ok(Value::String(unquoted.to_owned()))
}

fn quoted_text(text: &str) -> Option<&str> {
    if let Some(inner) = text
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        return Some(inner);
    }
    text.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
}

fn plain_description(text: &str) -> bool {
    if text.trim().is_empty() || text.len() > 256 {
        return false;
    }
    !text
        .chars()
        .any(|character| character.is_control() || matches!(character, '"' | '\'' | '\\'))
}

fn string_list(input: &Input<'_>, marker: &Marker) -> Result<Vec<String>, Failure> {
    if marker.name == "NeedsAll" && marker.arguments.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    let mut unique = BTreeSet::new();
    for value in marker.arguments.split(',') {
        let Some(name) = quoted_text(value.trim()).filter(|name| identifier(name)) else {
            return Err(input.failure(
                marker.span.start,
                "Required field/table names must be simple quoted identifiers",
            ));
        };
        if !unique.insert(name) || values.len() >= 32 {
            return Err(input.failure(
                marker.span.start,
                "Required field names must be unique and contain at most 32 entries",
            ));
        }
        values.push(name.to_owned());
    }
    Ok(values)
}

fn validate_registrations(
    input: &Input<'_>,
    registrations: &[Registration],
) -> Result<(), Failure> {
    let mut names = BTreeSet::new();
    for registration in registrations {
        if !registration.emitted() {
            continue;
        }
        if !names.insert(&registration.name) {
            return Err(input.failure(registration.start, "Duplicate annotated class name"));
        }
        validate_class_annotations(input, registration)?;
        validate_class_members(input, registration)?;
    }
    Ok(())
}

fn validate_class_annotations(
    input: &Input<'_>,
    registration: &Registration,
) -> Result<(), Failure> {
    let data = &registration.data;
    if data.contains_key("entity") != data.contains_key("table") {
        return Err(input.failure(
            registration.start,
            "Entity and Table must be declared together",
        ));
    }
    let has_cacheable_method = registration
        .methods
        .values()
        .any(|method| method.contains_key("cacheable"));
    if has_cacheable_method && !data.contains_key("cache") {
        return Err(input.failure(
            registration.start,
            "Cached requires a Cache policy on its class",
        ));
    }
    let only_description = registration.markers.len() == 1
        && registration.markers.contains("Description")
        && registration.methods.is_empty();
    if only_description {
        return Err(input.failure(
            registration.start,
            "Description annotates a class that already declares a behavior",
        ));
    }
    Ok(())
}

fn validate_class_members(input: &Input<'_>, registration: &Registration) -> Result<(), Failure> {
    let members = direct_members(input, registration);
    if registration.constructor.is_some() && members.iter().any(|(name, _)| name == "constructor") {
        let language = input
            .path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("js");
        return Err(input.failure(
            registration.start,
            crate::annotation_rules::AnnotationRules::needs_all_constructor(language),
        ));
    }
    if input
        .path
        .extension()
        .is_some_and(|extension| extension == "ts")
    {
        validate_required_fields(input, registration)?;
    }
    if registration.data.contains_key("configuration")
        && !members
            .iter()
            .any(|(name, is_static)| name == "configure" && *is_static)
    {
        return Err(input.failure(
            registration.start,
            "Setup requires a static configure(behaviors) method",
        ));
    }
    for (name, metadata) in &registration.methods {
        gateway::reject_combined(input, registration.start, metadata)?;
        if members.iter().filter(|(member, _)| member == name).count() != 1 {
            return Err(input.failure(
                registration.start,
                "Annotated methods must have one unambiguous concrete declaration",
            ));
        }
    }
    Ok(())
}

fn supply_declared(input: &Input<'_>, registrations: &mut [Registration]) -> Result<(), Failure> {
    for registration in registrations.iter_mut().filter(|item| item.declared) {
        if registration.constructor.is_some() {
            return Err(input.failure(
                registration.start,
                "An abstract behavior declaration carries signatures only; the compiler supplies its members",
            ));
        }
        registration.constructor = Some(vec!["behaviors".to_owned()]);
    }
    Ok(())
}

fn validate_required_fields(input: &Input<'_>, registration: &Registration) -> Result<(), Failure> {
    let Some(fields) = &registration.constructor else {
        return Ok(());
    };
    if registration.declared {
        return Ok(());
    }
    let class_body = &input.code[registration.open + 1..registration.close];
    for field in fields {
        if !declares_field(class_body, field) {
            return Err(input.failure(
                registration.start,
                format!("NeedsAll field '{field}' must be declared on its TypeScript class"),
            ));
        }
    }
    Ok(())
}

fn declares_field(class_body: &str, field: &str) -> bool {
    for line in class_body.lines() {
        let declaration = without_field_modifiers(line.trim());
        let Some(rest) = declaration.strip_prefix(field) else {
            continue;
        };
        if rest.trim_start().starts_with([':', ';', '=', '!', '?']) {
            return true;
        }
    }
    false
}

fn without_field_modifiers(mut declaration: &str) -> &str {
    while let Some((modifier, rest)) = declaration.split_once(' ') {
        if !matches!(
            modifier,
            "public" | "private" | "protected" | "readonly" | "declare"
        ) {
            break;
        }
        declaration = rest.trim_start();
    }
    declaration
}

fn direct_members(input: &Input<'_>, registration: &Registration) -> Vec<(String, bool)> {
    let mut members = Vec::new();
    for (name, is_static, _) in direct_member_entries(input, registration) {
        members.push((name, is_static));
    }
    members
}

fn direct_member_entries(
    input: &Input<'_>,
    registration: &Registration,
) -> Vec<(String, bool, usize)> {
    let code = &input.code;
    let mut members = Vec::new();
    let mut depth = 0_u32;
    let mut start = registration.open + 1;
    for (at, byte) in code
        .bytes()
        .enumerate()
        .take(registration.close)
        .skip(start)
    {
        if byte == b'{' {
            depth += 1;
        }
        if byte == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                start = at + 1;
            }
        }
        if depth != 0 {
            continue;
        }
        if matches!(byte, b';' | b'\n') {
            start = at + 1;
        }
        if byte != b'(' {
            continue;
        }
        if let Some((name, is_static)) =
            member_before_parenthesis(code[start..at].trim(), registration)
        {
            members.push((name, is_static, start));
        }
    }
    members
}

fn member_before_parenthesis(prefix: &str, registration: &Registration) -> Option<(String, bool)> {
    let words = prefix.split_whitespace().collect::<Vec<_>>();
    let name = words.last()?;
    if !identifier(name) && *name != "constructor" {
        return None;
    }
    let modifiers = &words[..words.len() - 1];
    let modifiers_allowed = modifiers
        .iter()
        .all(|word| member_modifier(word, registration.declared));
    if !modifiers_allowed {
        return None;
    }
    Some(((*name).to_owned(), words.contains(&"static")))
}

fn registration_js(registration: &Registration) -> String {
    let mut data = registration.data.clone();
    if let Some(fields) = &registration.constructor {
        data.insert(
            "requiredArgs".to_owned(),
            Value::Array(fields.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(table) = data.remove("table")
        && let Some(entity) = data.get_mut("entity").and_then(Value::as_object_mut)
    {
        entity.insert("table".to_owned(), table);
    }
    data.entry("configuration".to_owned())
        .or_insert(Value::Bool(false));
    data.insert("name".to_owned(), Value::String(registration.name.clone()));
    data.insert("methods".to_owned(), methods_json(registration));
    let json = Value::Object(data).to_string();
    format!("{{type:{},{}", registration.name, &json[1..])
}

fn methods_json(registration: &Registration) -> Value {
    let mut methods = Vec::new();
    for (name, method) in &registration.methods {
        let mut method = method.clone();
        method.insert("name".to_owned(), Value::String(name.clone()));
        methods.push(Value::Object(method));
    }
    Value::Array(methods)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::lower;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    fn lowered(path: &str, source: &str, environment: &str) -> String {
        lower(Path::new(path), source, environment)
            .expect("valid behavior source")
            .expect("lowered behavior source")
    }

    fn valid_output(path: &str, source: &str, environment: &str) -> Option<String> {
        lower(Path::new(path), source, environment).expect("valid behavior source")
    }

    fn rejected(path: &str, source: &str, environment: &str) -> bool {
        lower(Path::new(path), source, environment).is_err()
    }

    fn environment_for(path: &str) -> &'static str {
        if path.starts_with("tac") {
            return "tac";
        }
        "yon"
    }

    #[test]
    fn behavior_metadata_is_inert_and_shared_between_tac_and_yon() {
        let source = "@CacheSettings({\"name\":\"products\",\"ttlMs\":30000,\"maxBytes\":65536,\"maxEntries\":256})\nclass Products {\n@Cached\nasync find(id) { return id; }\n}\n";
        for environment in ["tac", "yon"] {
            let output = lowered("source.js", source, environment);
            assert!(!output.contains("@Cache"));
            assert!(output.contains("type:Products"));
            assert!(output.contains("\"keyArgs\":[0]"));
            assert!(output.contains("__tachyon_create_behaviors__"));
        }
    }

    #[test]
    fn value_binds_application_paths_and_rejects_the_rest() {
        let source = "class Settings {\n@Setting(\"${yon.catalog.page.size:20}\")\npageSize;\n@Setting(\"${yon.server.port:8080}\")\nport;\n@Setting({\"path\":\"yon.catalog.enabled\",\"type\":\"boolean\",\"default\":true})\nenabled;\n}\n";
        let output = lowered("yon.js", source, "yon");
        assert!(!output.contains("@Setting"), "{output}");
        assert!(
            output.contains("\"path\":\"yon.catalog.page.size\""),
            "{output}"
        );
        assert!(output.contains("\"default\":\"20\""), "{output}");
        assert!(output.contains("\"path\":\"yon.server.port\""), "{output}");
        assert!(output.contains("\"default\":8080"), "{output}");
        assert!(output.contains("\"type\":\"integer\""), "{output}");
        assert!(output.contains("\"type\":\"boolean\""), "{output}");
        for (path, source) in [
            (
                "tac.js",
                "class S {\n@Setting(\"${yon.server.port}\")\nport;\n}\n",
            ),
            ("tac.js", "class S {\n@Setting(\"${yon.x}\")\nx;\n}\n"),
            ("yon.js", "class S {\n@Setting(\"${tac.x}\")\nx;\n}\n"),
            (
                "yon.js",
                "class S {\n@Setting(\"${secrets.token}\")\nx;\n}\n",
            ),
            (
                "yon.js",
                "class S {\n@Setting(\"yon.server.host\")\nx;\n}\n",
            ),
            (
                "yon.js",
                "class S {\n@Setting(\"${yon.server.port:eighty}\")\nx;\n}\n",
            ),
            (
                "yon.js",
                "class S {\n@Setting({\"path\":\"yon.server.port\",\"type\":\"string\"})\nx;\n}\n",
            ),
            (
                "yon.js",
                "class S {\n@Setting({\"path\":\"yon.server.port\",\"default\":70000,\"min\":0,\"max\":65535})\nx;\n}\n",
            ),
            (
                "yon.js",
                "class S {\n@Setting(\"${yon.server.host}\")\nstatic host;\n}\n",
            ),
            (
                "yon.js",
                "class S {\n@Setting(\"${yon.server.host}\")\nhost() {}\n}\n",
            ),
            (
                "yon.js",
                "class S {\n@Setting(\"${yon.server.host}\")\n@Setting(\"${yon.redis.url}\")\nhost;\n}\n",
            ),
            ("yon.js", "@Setting(\"${yon.server.host}\")\nclass S {}\n"),
        ] {
            assert!(rejected(path, source, environment_for(path)), "{source}");
        }
    }

    #[test]
    fn a_tac_service_exports_its_required_fields() {
        let service = "@Service()\n@Lifetime({\"value\":\"prototype\"})\n@NeedsAll('path', 'title')\nexport default class PagerService {\npath;\ntitle;\n}\n";
        let output = lowered("client/services/PagerService.js", service, "tac");
        assert!(
            output.contains("constructor(path, title) { this.path = path; this.title = title; }")
        );
        assert!(output.contains("export const __tachyon_required_args__ = [\"path\",\"title\"];"));
        let role = "@Service()\n@NeedsAll('path')\nclass Helper {\npath;\n}\n";
        let output = lowered("client/services/PagerService.js", role, "tac");
        assert!(!output.contains("__tachyon_required_args__"));
        let yon = lowered("yon.js", service, "yon");
        assert!(!yon.contains("__tachyon_required_args__"));
    }

    #[test]
    fn a_bare_required_args_constructor_takes_every_uninitialised_field() {
        let service = "@Service\n@NeedsAll\nexport default class PagerService {\n  path\n  docs\n  label = 'Next'\n  taskScheduler\n}\n";
        let resources = BTreeMap::from([(
            String::from("PagerService"),
            BTreeSet::from([String::from("taskScheduler")]),
        )]);
        let output = super::lower_with_resources(
            Path::new("client/services/PagerService.js"),
            service,
            "tac",
            &resources,
        )
        .expect("valid")
        .expect("lowered");
        assert!(
            output.contains("constructor(path, docs) { this.path = path; this.docs = docs; }"),
            "{output}"
        );
        assert!(
            output.contains("export const __tachyon_required_args__ = [\"path\",\"docs\"];"),
            "{output}"
        );
    }

    #[test]
    fn constructor_generation_keeps_yon_markers_for_the_di_frontend() {
        let source = "@Service()\n@NeedsAll('repository')\n@Needs(Repository)\nclass Products {\nfind() { return this.repository; }\n}\n";
        let output = lowered("yon.js", source, "yon");
        assert!(output.contains("@Service()"));
        assert!(output.contains("@Needs(Repository)"));
        assert!(output.contains("constructor(repository) { this.repository = repository; }"));
    }

    #[test]
    fn all_behaviors_lower_without_imports_and_without_running_configuration() {
        let source = include_str!("../../tests/fixtures/behavior/providers.js");
        let output = lowered("tac.js", source, "tac");
        assert!(output.contains("\"requiredArgs\":[\"behaviors\"]"));
        assert!(output.contains("\"httpExchange\""));
        assert!(output.contains("\"retryable\""));
        assert!(output.contains("\"concurrencyLimit\""));
        assert!(output.contains("\"transactional\""));
        assert!(output.contains("\"table\":\"products\""));
        assert!(output.contains("\"configuration\":true"));
    }

    #[test]
    fn strings_comments_and_unannotated_source_are_untouched() {
        for source in [
            "const message = '@CacheSettings({})';\nclass Plain {}",
            "// @Setup()\nclass Plain {}",
            "const expression = /@Cache/;\nclass Plain {}",
            "const nativeCaches = [Cache];\nclass Plain {}",
        ] {
            assert!(valid_output("tac.js", source, "tac").is_none());
        }
    }

    #[test]
    fn invalid_targets_and_unsafe_constructor_fields_fail_closed() {
        for source in [
            "@Setup()\nexport default class { static configure() {} }",
            "@Setup()\nclass Config extends Base { static configure() {} }",
            "@Setup()\nclass Config { configure() {} }",
            "@NeedsAll('item')\nclass Item { constructor() {} }",
            "@NeedsAll('return')\nclass Item {}",
            "@NeedsAll('__proto__')\nclass Item {}",
            "@NeedsAll('item','item')\nclass Item {}",
            "@Table('products')\nclass Product {}",
            "@Entity({\"adapter\":\"data\",\"id\":\"id\"})\nclass Product {}",
        ] {
            assert!(rejected("tac.js", source, "tac"), "{source}");
        }
    }

    #[test]
    fn cache_metadata_requires_json_and_explicit_bounds() {
        for arguments in [
            "{name:'products',ttlMs:10,maxBytes:10,maxEntries:10}",
            "{\"name\":\"products\",\"ttlMs\":0,\"maxBytes\":10,\"maxEntries\":10}",
            "{\"name\":\"products\",\"ttlMs\":10,\"maxBytes\":1048577,\"maxEntries\":10}",
            "{\"name\":\"products\",\"ttlMs\":10,\"maxBytes\":10,\"maxEntries\":10001}",
            "{\"name\":\"products\",\"ttlMs\":10,\"maxBytes\":10,\"maxEntries\":10,\"unknown\":true}",
            "readEnvironment()",
        ] {
            let source = format!("@CacheSettings({arguments})\nclass Products {{}}");
            assert!(rejected("yon.js", &source, "yon"), "{arguments}");
        }
    }

    #[test]
    fn cache_wrappers_do_not_bypass_endpoints_or_streams() {
        for method in [
            "async GET() {}",
            "find() {}",
            "async *find() {}",
            "static async find() {}",
            "get find() {}",
            "async ['find']() {}",
            "find = async () => {}",
        ] {
            let source = format!(
                "@CacheSettings({{\"name\":\"products\",\"ttlMs\":1,\"maxBytes\":1,\"maxEntries\":1}})\nclass Products {{\n@Cached\n{method}\n}}"
            );
            assert!(rejected("yon.js", &source, "yon"), "{method}");
        }
        let source = "class Products {\n@Cached\nasync find() {}\n}";
        assert!(rejected("yon.js", source, "yon"));
    }

    #[test]
    fn duplicate_and_nested_method_declarations_fail_closed() {
        for source in [
            "class Product {\nfind(@PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]}) page) {}\nfind(page) {}\n}",
            "class Product {\nhelper() {\nfind(@PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]}) page) {}\n}\n}",
            "class Product {\nfind(@PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]}) @PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]}) page) {}\n}",
            "class Product {\nfind(@PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]}) page, @PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]}) other) {}\n}",
        ] {
            assert!(rejected("tac.js", source, "tac"), "{source}");
        }
    }

    #[test]
    fn invalid_native_policies_and_inline_markers_are_actionable() {
        for (path, source) in [
            ("yon.py", "@CacheSettings({})\nclass Product: pass"),
            ("yon.rs", "#[yon::cache_settings({})]\nstruct Product;"),
            ("yon.cs", "[Cache]\nclass Product {}"),
            ("tac.js", "@Setup() class Configuration {}"),
            ("tac.js", "class Product { @PageDefaults({}) find() {} }"),
        ] {
            assert!(rejected(path, source, "tac"), "{path}: {source}");
        }
    }

    #[test]
    fn a_tachyon_behaviors_import_is_refused_because_the_names_are_global() {
        for (path, environment) in [
            ("client/services/CartService.js", "tac"),
            ("client/services/CartService.ts", "tac"),
            ("server/controllers/CartController.js", "yon"),
            ("server/controllers/CartController.ts", "yon"),
        ] {
            for source in [
                "import { Service } from 'tachyon:behaviors';\n@Service()\nexport default class CartService {}\n",
                "import type { BehaviorContext } from 'tachyon:behaviors';\nexport const size = 1;\n",
                "import { Behaviors } from 'tachyon:behaviors';\nexport function use() { return Behaviors; }\n",
            ] {
                let failure = lower(Path::new(path), source, environment)
                    .expect_err("the import is refused")
                    .to_string();
                assert!(failure.contains("TY1308"), "{failure}");
                assert!(
                    failure.contains(&format!(
                        "{path}:1: Tachyon annotations and types are global; delete this import"
                    )),
                    "{failure}"
                );
            }
        }
    }

    #[test]
    fn behaviors_stays_a_reserved_generated_binding() {
        let source = "@Service()\nclass CartService {}\nconst Behaviors = {};";
        assert!(rejected("tac.js", source, "tac"));
        let imported = "import { Behaviors } from '/shared/scripts/behaviors.js';\nfunction use() { return Behaviors; }";
        assert!(rejected("tac.js", imported, "tac"));
    }

    #[test]
    fn typescript_required_fields_preserve_the_authored_shape() {
        let missing = "@NeedsAll('item')\nclass Product {}";
        assert!(rejected("tac.ts", missing, "tac"));
        let declared = "@NeedsAll('item')\nclass Product {\nreadonly item: unknown;\n}";
        assert!(valid_output("tac.ts", declared, "tac").is_some());
    }

    #[test]
    fn a_module_that_only_uses_behaviors_is_lowered() {
        for prefix in ["", "\n", "\r\n", "  \r\n", "// comment\n\n"] {
            let source = format!("{prefix}export function use() {{ return Behaviors; }}\r\n");
            let output = lowered("tac.js", &source, "tac");
            assert!(
                output.contains("const Behaviors = __tachyon_create_behaviors__([]"),
                "{output}"
            );
        }
        let mentioned = "// Behaviors\nconst text = 'Behaviors';\nclass Plain {}";
        assert!(valid_output("tac.js", mentioned, "tac").is_none());
    }

    #[test]
    fn outbound_paths_reject_encoded_origin_and_traversal_changes() {
        for path in [
            "https://example.com",
            "//example.com",
            "/../secret",
            "/./secret",
            "/%2e%2e/secret",
            "/%252e%252e/secret",
            "/%2fexample.com",
            "/%5csecret",
            "/%00secret",
            "/%20secret",
            "/?query",
            "/%3fquery",
            "/%zz",
        ] {
            assert!(!super::safe_path(path), "{path}");
        }
        for path in ["/products", "/products/123", "/products/%E2%9C%93"] {
            assert!(super::safe_path(path), "{path}");
        }
    }

    #[test]
    fn a_typescript_abstract_declaration_lowers_into_a_concrete_class() {
        let source = "@Entity({\"adapter\":\"data\"})\n@Table(\"products\")\nclass Product {\n  @Id\n  id: string;\n}\n@Store(Product, {\"fields\":[\"id\",\"status\"],\"defaultSize\":20,\"maxSize\":100})\nabstract class ProductRepository {\n  abstract findByStatus(status: unknown): Promise<unknown>;\n}\n";
        let output = lowered("tac.ts", source, "tac");
        assert!(output.contains("class ProductRepository"), "{output}");
        assert!(!output.contains("abstract"), "{output}");
        assert!(
            output.contains("constructor(behaviors) { this.behaviors = behaviors; }"),
            "{output}"
        );
        assert!(
            output.contains("findByStatus(status: unknown): Promise<unknown>{}"),
            "{output}"
        );
    }

    #[test]
    fn autowired_reads_past_typescript_field_modifiers() {
        let source = "@Client()\nclass UsersClient {\n@WebCall({\"client\":\"users\",\"method\":\"GET\",\"path\":\"/users\"})\nasync list(): Promise<unknown> { return {}; }\n}\n@Service()\nexport default class UserService {\n@Needs()\nreadonly usersClient!: UsersClient;\n}\n";
        let output = lowered("tac.ts", source, "tac");
        assert!(
            output.contains("constructor(usersClient) { this.usersClient = usersClient; }"),
            "{output}"
        );
        let statics = source.replace("readonly usersClient", "static usersClient");
        assert!(rejected("tac.ts", &statics, "tac"));
    }

    #[test]
    fn typescript_declarations_without_method_bodies_are_rejected() {
        for method in [
            "find(PAGE page): unknown;",
            "abstract find(PAGE page): unknown;",
            "declare find(PAGE page): unknown;",
            "find(PAGE page): unknown;\nfind(page) { return 1; }",
            "find(PAGE page): { value: number };",
        ] {
            let source = format!("class Product {{\n{method}\n}}").replace(
                "PAGE",
                "@PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]})",
            );
            assert!(rejected("tac.ts", &source, "tac"), "{method}");
        }
        let concrete = "class Product {\nfind(@PageDefaults({\"defaultSize\":1,\"maxSize\":1,\"sort\":[]}) page): { value: number; label: string } { return { value: 1, label: 'one' }; }\n}";
        assert!(valid_output("tac.ts", concrete, "tac").is_some());
    }

    #[test]
    fn behavior_bounds_preserve_large_unannotated_module_compatibility() {
        let source = format!(
            "/* @Cache literal documentation {} */\nclass Plain {{}}",
            "x".repeat(1024 * 1024)
        );
        assert!(valid_output("tac.js", &source, "tac").is_none());
        let annotated = format!("{source}\n@Setup()\nclass Settings {{ static configure() {{}} }}");
        assert!(rejected("tac.js", &annotated, "tac"));
    }

    #[test]
    fn module_namespaces_are_deterministic_portable_and_do_not_expose_paths() {
        let source = "export function use() { return Behaviors; }\n";
        let first = lowered("client/products/tac.js", source, "tac");
        let repeated = lowered("./client/products/tac.js", source, "tac");
        let windows = lowered("client\\products\\tac.js", source, "tac");
        assert_eq!(first, repeated);
        assert_eq!(first, windows);
        assert!(!first.contains("client/products"));
        let id = super::module_id(Path::new("client/products/tac.js"));
        assert_eq!(id.len(), 64);
        assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(first.contains(&format!("\"tac\", \"{id}\"")));
    }

    #[test]
    fn identical_class_metadata_from_different_modules_has_separate_namespaces() {
        let source = "@CacheSettings({\"name\":\"products\",\"ttlMs\":30000,\"maxBytes\":65536,\"maxEntries\":256})\nclass Products {}";
        let first = lowered("client/first/tac.js", source, "tac");
        let second = lowered("client/second/tac.js", source, "tac");
        assert_ne!(first, second);
        assert_ne!(
            super::module_id(Path::new("client/first/tac.js")),
            super::module_id(Path::new("client/second/tac.js"))
        );
    }
}
