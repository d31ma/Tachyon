use super::{Input, Marker};
use crate::Failure;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(super) fn environment_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 256 {
        return false;
    }
    if !name.as_bytes()[0].is_ascii_uppercase() {
        return false;
    }
    for byte in name.bytes() {
        if !byte.is_ascii_uppercase() && !byte.is_ascii_digit() && byte != b'_' {
            return false;
        }
    }
    for segment in name.trim_end_matches('_').split('_') {
        let starts_uppercase = segment
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_uppercase);
        if !starts_uppercase {
            return false;
        }
    }
    !name.contains("__")
}

pub(super) struct SettingsField {
    pub name: String,
    pub type_source: String,
    pub initializer: Option<String>,
    pub is_static: bool,
}

struct SettingsLimits;

impl SettingsLimits {
    const OPTION_KEYS: [&str; 3] = ["prefix", "names", "defaults"];
    const WITHOUT_INITIALIZERS: [&str; 3] = ["rs", "go", "rb"];
}

pub(super) fn properties(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let parsed = super::literal::parse(&marker.arguments).ok();
    let Some(Value::Object(object)) = parsed else {
        return Err(input.failure(
            marker.span.start,
            "Settings takes one JSON object, for example @Settings({\"prefix\":\"YON_CATALOG_\"})",
        ));
    };
    if object.contains_key("fields") {
        return Err(input.failure(
            marker.span.start,
            "Settings reads its fields from the class now: remove \"fields\" and declare each setting as a field with its type and default value; put @Min and @Max on the field",
        ));
    }
    if object
        .keys()
        .any(|key| !SettingsLimits::OPTION_KEYS.contains(&key.as_str()))
    {
        return Err(input.failure(
            marker.span.start,
            "Settings accepts only prefix, names and (Rust, Go and Ruby) defaults",
        ));
    }
    let language = input.extension();
    if object.contains_key("defaults") && !SettingsLimits::WITHOUT_INITIALIZERS.contains(&language)
    {
        return Err(input.failure(
            marker.span.start,
            "Settings.defaults is only for Rust, Go and Ruby; write the default as the field's initial value",
        ));
    }
    let prefix = object
        .get("prefix")
        .and_then(Value::as_str)
        .ok_or_else(|| input.failure(marker.span.start, "Settings.prefix requires a string"))?;
    if !valid_property_prefix(prefix) {
        return Err(input.failure(
            marker.span.start,
            "Settings prefix must be a bounded TAC_/YON_ environment prefix ending with underscore",
        ));
    }
    for key in ["names", "defaults"] {
        if object.get(key).is_some_and(|value| !value.is_object()) {
            return Err(input.failure(
                marker.span.start,
                format!("Settings.{key} maps field names to values"),
            ));
        }
    }
    Ok(Value::Object(object))
}

pub(super) fn field_bounds<'policies>(
    input: &Input<'_>,
    marker: &Marker,
    policies: &'policies mut Map<String, Value>,
    name: &str,
) -> Result<&'policies mut Map<String, Value>, Failure> {
    if !matches!(marker.name.as_str(), "Min" | "Max") {
        return Err(input.failure(marker.target, "Settings fields take only @Min and @Max"));
    }
    let bounds = policies
        .entry("settingsBounds")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(bounds) = bounds else {
        return Err(input.failure(marker.target, "Invalid Settings bounds"));
    };
    let field = bounds
        .entry(name.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    field
        .as_object_mut()
        .ok_or_else(|| input.failure(marker.target, "Invalid Settings bounds"))
}

pub(super) fn derive(
    input: &Input<'_>,
    at: usize,
    policies: &mut Map<String, Value>,
    fields: &[SettingsField],
) -> Result<(), Failure> {
    let bounds = policies.remove("settingsBounds");
    let Some(options) = policies.get("configurationProperties").cloned() else {
        return Ok(());
    };
    let language = input.extension();
    let mut descriptors = Map::new();
    for field in fields.iter().filter(|field| !field.is_static) {
        let descriptor = descriptor(input, at, language, &options, bounds.as_ref(), field)?;
        descriptors.insert(field.name.clone(), descriptor);
    }
    let known = |name: &str| {
        fields
            .iter()
            .any(|field| field.name == name && !field.is_static)
    };
    for key in ["names", "defaults"] {
        let Some(Value::Object(entries)) = options.get(key) else {
            continue;
        };
        if let Some(unknown) = entries.keys().find(|name| !known(name)) {
            return Err(input.failure(
                at,
                format!("Settings.{key} names '{unknown}', which is not a field of this class"),
            ));
        }
    }
    if let Some(Value::Object(bounds)) = &bounds
        && let Some(unknown) = bounds.keys().find(|name| !descriptors.contains_key(*name))
    {
        return Err(input.failure(
            at,
            format!("@Min/@Max on '{unknown}' needs a Settings field"),
        ));
    }
    let prefix = options["prefix"].as_str().unwrap_or_default();
    let object = serde_json::json!({"prefix": prefix, "fields": descriptors});
    validate_fields(input, at, prefix, &object["fields"])?;
    policies.insert("configurationProperties".to_owned(), object);
    Ok(())
}

fn descriptor(
    input: &Input<'_>,
    at: usize,
    language: &str,
    options: &Value,
    bounds: Option<&Value>,
    field: &SettingsField,
) -> Result<Value, Failure> {
    let name = options["names"]
        .get(&field.name)
        .and_then(Value::as_str)
        .map_or_else(|| environment_suffix(&field.name), str::to_owned);
    let initializer = field.initializer.as_deref().map(str::trim);
    let Some(kind) = settings_type(language, &field.type_source, initializer) else {
        return Err(input.failure(
            at,
            format!(
                "The Settings field '{}' must be text, a whole number, a number or true/false",
                field.name
            ),
        ));
    };
    let mut descriptor = serde_json::json!({"name": name, "type": kind});
    let default = match options["defaults"].get(&field.name) {
        Some(value) => Some(value.clone()),
        None => initializer
            .map(|text| literal_default(language, text))
            .transpose()
            .map_err(|message| {
                input.failure(
                    at,
                    format!("The Settings field '{}': {message}", field.name),
                )
            })?
            .flatten(),
    };
    if let Some(default) = default {
        descriptor["default"] = default;
    }
    if let Some(field_bounds) = bounds.and_then(|bounds| bounds.get(&field.name)) {
        for key in ["min", "max"] {
            if let Some(value) = field_bounds.get(key) {
                descriptor[key] = value.clone();
            }
        }
    }
    Ok(descriptor)
}

fn environment_suffix(field: &str) -> String {
    let mut name = String::new();
    let mut previous_lowercase = false;
    for character in field.chars() {
        if character.is_ascii_uppercase() && previous_lowercase {
            name.push('_');
        }
        previous_lowercase = character.is_ascii_lowercase() || character.is_ascii_digit();
        name.push(character.to_ascii_uppercase());
    }
    name
}

fn settings_type(
    language: &str,
    type_source: &str,
    initializer: Option<&str>,
) -> Option<&'static str> {
    let written = without_nullable(type_source.trim());
    if written.is_empty() {
        return initializer.and_then(|text| literal_type(language, text));
    }
    let (texts, integers, numbers, booleans): (&[&str], &[&str], &[&str], &[&str]) = match language
    {
        "ts" | "js" => (&["string"], &["integer"], &["number"], &["boolean"]),
        "py" => (&["str"], &["int"], &["float"], &["bool"]),
        "php" => (&["string"], &["int"], &["float"], &["bool"]),
        "java" => (
            &["String"],
            &["int", "long", "Integer", "Long"],
            &["double", "float", "Double", "Float"],
            &["boolean", "Boolean"],
        ),
        "kt" => (
            &["String"],
            &["Int", "Long"],
            &["Double", "Float"],
            &["Boolean"],
        ),
        "cs" => (
            &["string", "String"],
            &["int", "long"],
            &["double", "float", "decimal"],
            &["bool"],
        ),
        "swift" => (
            &["String"],
            &["Int", "Int64", "Int32"],
            &["Double", "Float"],
            &["Bool"],
        ),
        "dart" => (&["String"], &["int"], &["double", "num"], &["bool"]),
        "rb" => (
            &["String"],
            &["Integer"],
            &["Float", "Numeric"],
            &["Boolean"],
        ),
        "rs" => (
            &["String"],
            &["i32", "i64", "u32", "u64", "usize", "isize"],
            &["f32", "f64"],
            &["bool"],
        ),
        "go" => (
            &["string"],
            &["int", "int32", "int64", "uint", "uint32", "uint64"],
            &["float32", "float64"],
            &["bool"],
        ),
        "hpp" => (
            &["std::string"],
            &[
                "int",
                "long",
                "long long",
                "unsigned",
                "std::int32_t",
                "std::int64_t",
                "std::size_t",
            ],
            &["double", "float"],
            &["bool"],
        ),
        _ => return None,
    };
    if texts.contains(&written) {
        return Some("string");
    }
    if integers.contains(&written) {
        return Some("integer");
    }
    if numbers.contains(&written) {
        let whole = initializer.is_some_and(|text| is_whole_number(language, text));
        if matches!(language, "ts" | "js") && whole {
            return Some("integer");
        }
        return Some("number");
    }
    booleans.contains(&written).then_some("boolean")
}

fn without_nullable(type_source: &str) -> &str {
    let mut text = type_source.trim();
    for prefix in ["Option<", "Optional[", "?", "*"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.trim_end_matches(['>', ']']);
        }
    }
    for suffix in ["?", "| None", "| null", "| undefined", ", nil"] {
        text = text.strip_suffix(suffix).unwrap_or(text).trim();
    }
    text
}

fn literal_type(language: &str, text: &str) -> Option<&'static str> {
    match literal_default(language, text).ok()?? {
        Value::String(_) => Some("string"),
        Value::Bool(_) => Some("boolean"),
        Value::Number(number) if number.is_i64() || number.is_u64() => Some("integer"),
        Value::Number(_) => Some("number"),
        _ => None,
    }
}

fn is_whole_number(language: &str, text: &str) -> bool {
    matches!(literal_default(language, text), Ok(Some(Value::Number(number))) if number.is_i64() || number.is_u64())
}

fn literal_default(language: &str, text: &str) -> Result<Option<Value>, String> {
    let text = text.trim().trim_end_matches([';', ',']).trim();
    if matches!(text, "null" | "nil" | "None" | "undefined") {
        return Ok(None);
    }
    if matches!(text, "true" | "True") {
        return Ok(Some(Value::Bool(true)));
    }
    if matches!(text, "false" | "False") {
        return Ok(Some(Value::Bool(false)));
    }
    if let Some(inner) = quoted(language, text) {
        return Ok(Some(Value::String(inner.to_owned())));
    }
    let number = text.trim_end_matches(['L', 'l', 'f', 'F', 'd', 'D', 'm', 'M']);
    if let Ok(whole) = number.parse::<i64>() {
        return Ok(Some(Value::from(whole)));
    }
    let decimal = number.parse::<f64>().ok().filter(|value| value.is_finite());
    if number.contains(['.', 'e', 'E'])
        && let Some(value) = decimal.and_then(serde_json::Number::from_f64)
    {
        return Ok(Some(Value::Number(value)));
    }
    Err(
        "its default must be a plain literal: text in quotes, a number, true, false or null"
            .to_owned(),
    )
}

fn quoted<'text>(language: &str, text: &'text str) -> Option<&'text str> {
    let single_quotes = matches!(language, "js" | "ts" | "py" | "php" | "dart");
    for quote in ['"', '\''] {
        if quote == '\'' && !single_quotes {
            continue;
        }
        let inner = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote));
        if let Some(inner) = inner
            && !inner.contains([quote, '\\', '$'])
        {
            return Some(inner);
        }
    }
    None
}

fn validate_fields(
    input: &Input<'_>,
    at: usize,
    prefix: &str,
    fields: &Value,
) -> Result<(), Failure> {
    let fields = match fields.as_object() {
        Some(fields) if !fields.is_empty() && fields.len() <= 64 => fields,
        _ => {
            return Err(input.failure(at, "Settings needs 1–64 instance fields with a type"));
        }
    };
    let mut names = BTreeSet::new();
    for (field, descriptor) in fields {
        let name = descriptor["name"].as_str().unwrap_or_default();
        if !super::identifier(field) || !exact_environment_name(name) {
            return Err(input.failure(
                at,
                format!("The Settings field '{field}' needs a plain name; its environment name '{name}' must use capitals, digits and underscores"),
            ));
        }
        let Some(descriptor) = descriptor.as_object() else {
            return Err(input.failure(at, "Invalid Settings field"));
        };
        if !typed_descriptor_is_valid(descriptor, "name") {
            return Err(input.failure(
                at,
                format!("The Settings field '{field}': its default must match its type and sit inside its @Min/@Max range"),
            ));
        }
        if prefix.len() + name.len() > 256 || !names.insert(name) {
            return Err(input.failure(at, "Settings property names must be unique and bounded"));
        }
    }
    Ok(())
}

fn valid_property_prefix(prefix: &str) -> bool {
    let known_framework = prefix.starts_with("TAC_") || prefix.starts_with("YON_");
    environment_name(prefix) && prefix.len() <= 128 && prefix.ends_with('_') && known_framework
}

fn exact_environment_name(name: &str) -> bool {
    environment_name(name) && !name.ends_with('_')
}

fn validate_typed(
    input: &Input<'_>,
    marker: &Marker,
    descriptor: &Map<String, Value>,
    key: &str,
) -> Result<(), Failure> {
    if typed_descriptor_is_valid(descriptor, key) {
        return Ok(());
    }
    Err(input.failure(marker.span.start, "Settings descriptor requires name/type and optional valid default/min/max; names and values are literal and bounded"))
}

fn typed_descriptor_is_valid(descriptor: &Map<String, Value>, key: &str) -> bool {
    let allowed_keys = [key, "type", "default", "min", "max"];
    for member in descriptor.keys() {
        if !allowed_keys.contains(&member.as_str()) {
            return false;
        }
    }
    let value_type = descriptor
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(value_type, "string" | "boolean" | "integer" | "number") {
        return false;
    }
    if !bounds_are_valid(descriptor, value_type) {
        return false;
    }
    let Some(default) = descriptor.get("default") else {
        return true;
    };
    default_is_valid(descriptor, default, value_type)
}

fn bounds_are_valid(descriptor: &Map<String, Value>, value_type: &str) -> bool {
    let numeric = matches!(value_type, "integer" | "number");
    for key in ["min", "max"] {
        let Some(bound) = descriptor.get(key) else {
            continue;
        };
        if !numeric || !valid_number(bound, value_type == "integer") {
            return false;
        }
    }
    let min = descriptor.get("min").and_then(Value::as_f64);
    let max = descriptor.get("max").and_then(Value::as_f64);
    if let (Some(min), Some(max)) = (min, max) {
        return min <= max;
    }
    true
}

fn default_is_valid(descriptor: &Map<String, Value>, default: &Value, value_type: &str) -> bool {
    let valid = match value_type {
        "string" => default.as_str().is_some_and(|value| value.len() <= 4096),
        "boolean" => default.is_boolean(),
        _ => valid_number(default, value_type == "integer"),
    };
    if !valid {
        return false;
    }
    let Some(value) = default.as_f64() else {
        return true;
    };
    let below_min = descriptor
        .get("min")
        .and_then(Value::as_f64)
        .is_some_and(|min| value < min);
    let above_max = descriptor
        .get("max")
        .and_then(Value::as_f64)
        .is_some_and(|max| value > max);
    !below_min && !above_max
}

fn valid_number(value: &Value, integer: bool) -> bool {
    let Some(number) = value.as_f64() else {
        return false;
    };
    let whole = !integer || number.fract() == 0.0;
    number.is_finite() && number.abs() <= 9_007_199_254_740_991.0 && whole
}

pub(super) fn profiles(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let names = super::string_list(input, marker)?;
    if names.iter().any(|name| name.len() > 64) {
        return Err(input.failure(
            marker.span.start,
            "WhenProfile names must be at most 64 ASCII identifier characters",
        ));
    }
    Ok(serde_json::json!(names))
}

pub(super) fn conditional(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let object = super::object(input, marker, &["name", "havingValue", "matchIfMissing"])?;
    let valid_name = object["name"].as_str().is_some_and(exact_environment_name);
    let valid_match = object["havingValue"]
        .as_str()
        .is_some_and(|value| value.len() <= 4096);
    let valid_missing = object["matchIfMissing"].is_boolean();
    if !valid_name || !valid_match || !valid_missing {
        return Err(input.failure(marker.span.start, "WhenSetting requires a bounded environment name/string match and boolean matchIfMissing"));
    }
    Ok(Value::Object(object))
}

pub(super) fn validate(
    input: &Input<'_>,
    at: usize,
    policies: &Map<String, Value>,
    configuration: bool,
    environment: &str,
) -> Result<(), Failure> {
    if (policies.contains_key("profiles") || policies.contains_key("conditionalOnProperty"))
        && !configuration
    {
        return Err(input.failure(
            at,
            "WhenProfile and WhenSetting require Setup on the same type",
        ));
    }
    check_value_paths(input, at, policies, environment)?;
    let prefix = if environment == "tac" { "TAC_" } else { "YON_" };
    for (policy, key) in [
        ("configurationProperties", "prefix"),
        ("conditionalOnProperty", "name"),
    ] {
        let Some(value) = policies.get(policy) else {
            continue;
        };
        let uses_prefix = value[key]
            .as_str()
            .is_some_and(|name| name.starts_with(prefix));
        if !uses_prefix {
            return Err(input.failure(
                at,
                format!("{policy} environment names must use {prefix} for this framework"),
            ));
        }
    }
    Ok(())
}

fn check_value_paths(
    input: &Input<'_>,
    at: usize,
    policies: &Map<String, Value>,
    environment: &str,
) -> Result<(), Failure> {
    let Some(values) = policies.get("values").and_then(Value::as_object) else {
        return Ok(());
    };
    let namespace = format!("{environment}.");
    for descriptor in values.values() {
        let path = descriptor["path"].as_str().unwrap_or_default();
        if value_path(path).is_none() || !path.starts_with(&namespace) {
            return Err(input.failure(
                at,
                format!("Setting paths must use the {environment} namespace for this framework"),
            ));
        }
    }
    Ok(())
}

enum ValuePath {
    ServerPort,
    Scalar,
}

fn value_path(path: &str) -> Option<ValuePath> {
    let mut segments = path.split('.');
    if path.len() > 256 || !matches!(segments.next(), Some("tac" | "yon")) {
        return None;
    }
    let rest: Vec<&str> = segments.collect();
    if rest.is_empty() {
        return None;
    }
    for segment in rest {
        if !valid_path_segment(segment) {
            return None;
        }
    }
    if path == "yon.server.port" {
        Some(ValuePath::ServerPort)
    } else {
        Some(ValuePath::Scalar)
    }
}

fn valid_path_segment(segment: &str) -> bool {
    if segment.is_empty() || !segment.as_bytes()[0].is_ascii_lowercase() {
        return false;
    }
    for byte in segment.bytes() {
        if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() {
            return false;
        }
    }
    !matches!(segment, "constructor" | "prototype")
}

pub(super) fn value(input: &Input<'_>, marker: &Marker) -> Result<Value, Failure> {
    let arguments = marker.arguments.trim();
    let shorthand = arguments.starts_with(['"', '\'']);
    let mut descriptor = if shorthand {
        shorthand_descriptor(input, marker, arguments)?
    } else {
        literal_descriptor(input, marker)?
    };
    let path = descriptor
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| input.failure(marker.span.start, "Setting requires a string path"))?;
    let Some(kind) = value_path(&path) else {
        return Err(input.failure(marker.span.start, "Setting path must start with tac. or yon. and contain lowercase ASCII segments without underscores"));
    };
    let fixed = match kind {
        ValuePath::ServerPort => Some("integer"),
        ValuePath::Scalar => None,
    };
    set_value_type(input, marker, &mut descriptor, &path, fixed)?;
    if shorthand && fixed == Some("integer") {
        parse_integer_default(input, marker, &mut descriptor, &path)?;
    }
    validate_typed(input, marker, &descriptor, "path")?;
    Ok(Value::Object(descriptor))
}

fn shorthand_descriptor(
    input: &Input<'_>,
    marker: &Marker,
    arguments: &str,
) -> Result<Map<String, Value>, Failure> {
    let placeholder = quoted_placeholder(arguments).ok_or_else(|| {
        input.failure(
            marker.span.start,
            "Setting string form is one quoted \"${path}\" or \"${path:default}\" placeholder",
        )
    })?;
    let inner = placeholder
        .strip_prefix("${")
        .and_then(|rest| rest.strip_suffix('}'))
        .ok_or_else(|| {
            input.failure(
                marker.span.start,
                "Setting string form is \"${path}\" or \"${path:default}\"",
            )
        })?;
    let (path, fallback) = match inner.split_once(':') {
        Some((path, fallback)) => (path, Some(fallback)),
        None => (inner, None),
    };
    let mut object = Map::new();
    object.insert("path".into(), Value::String(path.to_owned()));
    if let Some(fallback) = fallback {
        object.insert("default".into(), Value::String(fallback.to_owned()));
    }
    Ok(object)
}

fn quoted_placeholder(arguments: &str) -> Option<&str> {
    let double_quoted = arguments
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'));
    let inner = match double_quoted {
        Some(inner) => inner,
        None => arguments
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))?,
    };
    if inner.len() > 512 || inner.contains(['"', '\'', '\\', '\n', '\r']) {
        return None;
    }
    Some(inner)
}

fn literal_descriptor(input: &Input<'_>, marker: &Marker) -> Result<Map<String, Value>, Failure> {
    let literal: Value = super::literal::parse(&marker.arguments).map_err(|_| {
        input.failure(
            marker.span.start,
            "Setting configuration must be a strict JSON object or one quoted placeholder",
        )
    })?;
    literal.as_object().cloned().ok_or_else(|| {
        input.failure(
            marker.span.start,
            "Setting configuration must be a JSON object",
        )
    })
}

fn set_value_type(
    input: &Input<'_>,
    marker: &Marker,
    descriptor: &mut Map<String, Value>,
    path: &str,
    fixed: Option<&str>,
) -> Result<(), Failure> {
    match descriptor.get("type") {
        None => {
            descriptor.insert(
                "type".into(),
                Value::String(fixed.unwrap_or("string").into()),
            );
        }
        Some(Value::String(declared)) if fixed.is_some_and(|fixed| declared != fixed) => {
            return Err(input.failure(
                marker.span.start,
                format!(
                    "Setting path {path} is a {}; declare that type or omit it",
                    fixed.unwrap_or_default()
                ),
            ));
        }
        Some(_) => {}
    }
    Ok(())
}

fn parse_integer_default(
    input: &Input<'_>,
    marker: &Marker,
    descriptor: &mut Map<String, Value>,
    path: &str,
) -> Result<(), Failure> {
    let Some(Value::String(text)) = descriptor.get("default") else {
        return Ok(());
    };
    let number = text.parse::<i64>().map_err(|_| {
        input.failure(
            marker.span.start,
            format!("Setting default for {path} must be an integer"),
        )
    })?;
    descriptor.insert("default".into(), Value::from(number));
    Ok(())
}

pub(super) fn bind_field(
    input: &Input<'_>,
    marker: &Marker,
    policies: &mut Map<String, Value>,
    field: &str,
    descriptor: Value,
) -> Result<(), Failure> {
    let values = policies
        .entry("values".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(values) = values.as_object_mut() else {
        return Err(input.failure(marker.span.start, "Setting policy is malformed"));
    };
    if values.len() >= 64 {
        return Err(input.failure(marker.span.start, "A type binds at most 64 values"));
    }
    if values.insert(field.to_owned(), descriptor).is_some() {
        return Err(input.failure(marker.span.start, "Duplicate Setting on one field"));
    }
    Ok(())
}
