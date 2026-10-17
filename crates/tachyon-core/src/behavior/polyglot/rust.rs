use super::rust_async_client::RustAsyncClient;
use super::rust_test_construction::RustTestConstruction;
use super::{Edit, Method, Module, Type, block, context_type, render};
use serde_json::Value;
use std::fmt::Write as _;

struct RustNames;

impl RustNames {
    const NO_BUILTINS: &str = "fn behavior_builtin_cache(_name: &str) -> Option<std::sync::Arc<dyn BehaviorCacheAdapter>> { None }\nfn behavior_builtin_entities(_name: &str) -> Option<std::sync::Arc<dyn BehaviorEntityAdapter>> { None }\n";
    const TAC_ASYNC_RUNTIME: &str = concat!(
        include_str!("behavior_exchange.rs"),
        include_str!("behavior_browser_http.rs"),
        include_str!("behavior_tasks.rs")
    );
    const TYPED_ANSWER: &str = include_str!("behavior_typed_answer.rs");
}

pub(super) fn lower(module: &Module) -> Result<String, String> {
    if module.language != "rs" {
        return Err(String::from(
            "Rust behavior lowering requires a Rust source module",
        ));
    }
    let mut edits = Vec::new();
    let mut generated = String::new();
    for declaration in &module.types {
        lower_type(module, declaration, &mut edits, &mut generated)?;
    }
    generated.push_str(&runtime(&module.module_id, &module.environment));
    if module.types.iter().any(RustAsyncClient::reads_models) {
        generated.push_str(RustNames::TYPED_ANSWER);
    }
    render(module, edits, &generated)
}

pub(super) fn runtime(module_id: &str, environment: &str) -> String {
    let mut generated = String::new();
    let _ = writeln!(
        generated,
        "const __TACHYON_BEHAVIOR_MODULE_ID: &str = {module_id:?};"
    );
    let _ = writeln!(
        generated,
        "const __TACHYON_BEHAVIOR_ENVIRONMENT: &str = {environment:?};"
    );
    generated.push_str(include_str!("rust_runtime.rs"));
    if environment == "tac" {
        generated.push_str(include_str!("rust_storage.rs"));
        generated.push_str(RustNames::TAC_ASYNC_RUNTIME);
    } else {
        generated.push_str(RustNames::NO_BUILTINS);
    }
    if environment == "yon" {
        generated.push_str("impl From<Json> for BehaviorValue { fn from(value: Json) -> Self { match value { Json::Null => Self::Null, Json::Bool(value) => Self::Bool(value), Json::Number(value) => Self::Number(value), Json::Integer(value) => Self::Number(value as f64), Json::Unsigned(value) => Self::Number(value as f64), Json::Text(value) => Self::String(value), Json::List(values) => Self::Array(values.into_iter().map(Self::from).collect()), Json::Map(entries) => Self::Object(entries.into_iter().map(|(key, value)| (key, Self::from(value))).collect()) } } }\n");
    }
    generated
}

fn lower_type(
    module: &Module,
    declaration: &Type,
    edits: &mut Vec<Edit>,
    generated: &mut String,
) -> Result<(), String> {
    if declaration.declared {
        edits.push(Edit {
            range: declaration.keyword.clone(),
            replacement: declared_struct(declaration),
        });
    }
    let mut construction = String::new();
    constructor(declaration, module.environment == "tac", &mut construction)?;
    if !construction.is_empty() {
        let insertion = declaration.body.end + 1;
        edits.push(Edit {
            range: insertion..insertion,
            replacement: format!("\n{construction}"),
        });
    }
    if let Some(unit) = super::super::testing::polyglot_unit(declaration) {
        generated.push_str(&test_members(&unit, module, declaration));
        if module.environment == "tac" {
            generated.push_str(&RustTestConstruction::members(module, declaration, &unit));
        }
    }
    configuration(declaration, generated)?;
    properties(declaration, generated)?;
    values(declaration, generated)?;
    validation(declaration, generated)?;
    entity(declaration, generated)?;
    service(module, declaration, edits, generated)
}

fn declared_struct(declaration: &Type) -> String {
    let name = &declaration.name;
    if super::needs_no_context(declaration) {
        return format!(
            "struct {name} {{}}\nimpl {name} {{ pub fn new() -> Self {{ Self {{}} }} }}\nimpl"
        );
    }
    format!(
        "struct {name} {{ behaviors: {} }}\nimpl",
        context_type("rs")
    )
}

fn constructor(declaration: &Type, tac: bool, output: &mut String) -> Result<(), String> {
    let Some(fields) = &declaration.constructor else {
        return Ok(());
    };
    if declaration
        .methods
        .iter()
        .any(|method| method.name == "new")
    {
        return Err(format!(
            "{} ({})",
            crate::annotation_rules::AnnotationRules::needs_all_constructor("rs"),
            declaration.name
        ));
    }
    if fields.len() != declaration.fields.len() && !tac {
        return Err(format!(
            "Rust NeedsAll on {} must list every declared field; Rust does not implicitly initialize omitted fields",
            declaration.name
        ));
    }
    let mut arguments = Vec::new();
    for name in fields {
        let Some(field) = declaration.fields.iter().find(|field| field.name == *name) else {
            return Err(format!(
                "Required Rust field {}.{name} is not declared",
                declaration.name
            ));
        };
        if field.type_source.trim().is_empty() {
            return Err(format!(
                "Required Rust field {}.{name} needs an explicit type",
                declaration.name
            ));
        }
        arguments.push(format!("{name}: {}", field.type_source));
    }
    let mut filled = fields.clone();
    filled.extend(
        declaration
            .fields
            .iter()
            .filter(|field| !field.is_static && !fields.contains(&field.name))
            .map(|field| format!("{}: ::core::default::Default::default()", field.name)),
    );
    let _ = writeln!(
        output,
        "impl {} {{ pub fn new({}) -> Self {{ Self {{ {} }} }} }}",
        declaration.name,
        arguments.join(", "),
        filled.join(", ")
    );
    Ok(())
}

fn configuration(declaration: &Type, output: &mut String) -> Result<(), String> {
    if !declaration.configuration {
        return Ok(());
    }
    let profiles = configuration_profiles(declaration)?;
    let property = property_condition(declaration)?;
    let _ = writeln!(
        output,
        "impl BehaviorConfigurationSource for {} {{ fn behavior_configure(execution: &BehaviorExecution) -> BehaviorResult<BehaviorConfiguration> {{ Self::configure(execution) }} fn behavior_conditions() -> BehaviorConfigurationConditions {{ BehaviorConfigurationConditions {{ profiles: vec![{profiles}], property: {property} }} }} }}",
        declaration.name
    );
    Ok(())
}

fn configuration_profiles(declaration: &Type) -> Result<String, String> {
    let Some(profiles) = declaration
        .policies
        .get("profiles")
        .and_then(Value::as_array)
    else {
        return Ok(String::new());
    };
    quoted_strings(profiles, "Invalid Rust configuration profile")
}

fn property_condition(declaration: &Type) -> Result<String, String> {
    let Some(condition) = declaration.policies.get("conditionalOnProperty") else {
        return Ok(String::from("None"));
    };
    let match_if_missing = condition["matchIfMissing"]
        .as_bool()
        .ok_or("Invalid Rust configuration matchIfMissing")?;
    Ok(format!(
        "Some(BehaviorPropertyCondition {{ name: {:?}, having_value: {:?}, match_if_missing: {match_if_missing} }})",
        string(condition, "name")?,
        string(condition, "havingValue")?
    ))
}

fn properties(declaration: &Type, output: &mut String) -> Result<(), String> {
    let Some(policy) = declaration.policies.get("configurationProperties") else {
        return Ok(());
    };
    let fields = policy["fields"]
        .as_object()
        .ok_or("Missing Rust configuration fields")?;
    let fields = property_fields(fields, "name")?;
    let _ = writeln!(
        output,
        "impl BehaviorPropertiesSource for {} {{ fn behavior_properties() -> BehaviorPropertiesPolicy {{ BehaviorPropertiesPolicy {{ prefix: {:?}, fields: vec![{fields}] }} }} }}",
        declaration.name,
        string(policy, "prefix")?
    );
    Ok(())
}

fn values(declaration: &Type, output: &mut String) -> Result<(), String> {
    let Some(policy) = declaration
        .policies
        .get("values")
        .and_then(Value::as_object)
    else {
        return Ok(());
    };
    let fields = property_fields(policy, "path")?;
    let _ = writeln!(
        output,
        "impl BehaviorValuesSource for {} {{ fn behavior_values() -> BehaviorValuesPolicy {{ BehaviorValuesPolicy {{ fields: vec![{fields}] }} }} }}",
        declaration.name
    );
    Ok(())
}

fn property_fields(
    fields: &serde_json::Map<String, Value>,
    name_key: &str,
) -> Result<String, String> {
    let mut rendered = Vec::new();
    for (field, descriptor) in fields {
        rendered.push(property_field(field, descriptor, name_key)?);
    }
    Ok(rendered.join(", "))
}

fn property_field(field: &str, descriptor: &Value, name_key: &str) -> Result<String, String> {
    let default = match descriptor.get("default") {
        Some(value) => property_default(value)?,
        None => String::from("None"),
    };
    Ok(format!(
        "BehaviorPropertyField {{ field: {field:?}, name: {:?}, kind: {:?}, default: {default}, min: {}, max: {} }}",
        string(descriptor, name_key)?,
        string(descriptor, "type")?,
        property_bound(descriptor.get("min"))?,
        property_bound(descriptor.get("max"))?
    ))
}

fn property_bound(value: Option<&Value>) -> Result<String, String> {
    let Some(value) = value else {
        return Ok(String::from("None"));
    };
    let Some(bound) = value.as_f64().filter(|bound| bound.is_finite()) else {
        return Err(String::from("Invalid Rust configuration numeric bound"));
    };
    Ok(format!("Some({bound:?})"))
}

fn property_default(value: &Value) -> Result<String, String> {
    let value = match value {
        Value::String(value) => format!("BehaviorValue::from({value:?})"),
        Value::Bool(value) => format!("BehaviorValue::Bool({value})"),
        Value::Number(value) => format!(
            "BehaviorValue::Number({:?})",
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("Invalid Rust configuration numeric default")?
        ),
        _ => {
            return Err(String::from(
                "Rust configuration defaults require declared scalar values",
            ));
        }
    };
    Ok(format!("Some({value})"))
}

fn entity(declaration: &Type, output: &mut String) -> Result<(), String> {
    let Some(entity) = &declaration.entity else {
        return Ok(());
    };
    let adapter = string(entity, "adapter")?;
    let id = string(entity, "id")?;
    let table = declaration
        .table
        .as_deref()
        .ok_or_else(|| String::from("Rust Entity requires Table"))?;
    let _ = writeln!(
        output,
        "impl BehaviorEntity for {} {{ fn behavior_entity() -> BehaviorEntityMetadata {{ BehaviorEntityMetadata {{ name: {:?}, adapter: {adapter:?}, table: {table:?}, id: {id:?} }} }} }}",
        declaration.name, declaration.name
    );
    Ok(())
}

fn validation(declaration: &Type, output: &mut String) -> Result<(), String> {
    let Some(schema) = declaration.policies.get("validated") else {
        return Ok(());
    };
    let fields = schema["fields"]
        .as_object()
        .ok_or("Missing Rust validation fields")?;
    let mut rendered = Vec::new();
    for (name, rules) in fields {
        rendered.push(validation_field(name, rules)?);
    }
    let fields = rendered.join(", ");
    let _ = writeln!(
        output,
        "impl BehaviorValidated for {} {{ fn behavior_validation() -> BehaviorValidationSchema {{ BehaviorValidationSchema {{ fields: vec![{fields}] }} }} }}",
        declaration.name
    );
    Ok(())
}

fn validation_field(name: &str, rules: &Value) -> Result<String, String> {
    let size = match rules.get("size") {
        Some(size) => format!("Some(({}, {}))", number(size, "min")?, number(size, "max")?),
        None => String::from("None"),
    };
    let not_null = rules
        .get("notNull")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let not_blank = rules
        .get("notBlank")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(format!(
        "BehaviorValidationField {{ name: {name:?}, not_null: {not_null}, not_blank: {not_blank}, size: {size}, min: {}, max: {} }}",
        property_bound(rules.get("min"))?,
        property_bound(rules.get("max"))?
    ))
}

fn service(
    module: &Module,
    declaration: &Type,
    edits: &mut Vec<Edit>,
    output: &mut String,
) -> Result<(), String> {
    let methods = declaration
        .methods
        .iter()
        .filter(|method| !method.metadata.is_empty())
        .collect::<Vec<_>>();
    if methods.is_empty() {
        return Ok(());
    }
    let needs_context = methods
        .iter()
        .any(|method| super::method_needs_context(method));
    if needs_context && !has_context_field(declaration) {
        return Err(format!(
            "Annotated Rust service {} requires a behaviors: BehaviorContext field",
            declaration.name
        ));
    }
    if needs_context {
        let _ = writeln!(
            output,
            "impl BehaviorService for {} {{ fn behavior_context(&self) -> &BehaviorContext {{ &self.behaviors }} fn behavior_method(name: &str) -> Option<BehaviorMethodPolicy> {{ match name {{",
            declaration.name
        );
    }
    for method in methods {
        if RustAsyncClient::applies(method) {
            RustAsyncClient::validate(method)?;
        } else {
            validate_method(method)?;
        }
        if let Some(policy) = method.metadata.get("gateway") {
            edits.push(Edit {
                range: method.body.clone(),
                replacement: block(method, &gateway_body(method, policy)),
            });
            continue;
        }
        let policy = method_policy(declaration, method)?;
        if needs_context {
            let _ = writeln!(output, "{:?} => Some({policy}),", method.name);
        }
        let mut edit = wrapper(module, method, &policy)?;
        edit.replacement = block(method, &edit.replacement);
        edits.push(edit);
    }
    if needs_context {
        output.push_str("_ => None, } } }\n");
    }
    Ok(())
}

fn gateway_body(method: &Method, policy: &Value) -> String {
    let key = policy["key"].as_str().unwrap_or_default();
    let mut names = Vec::new();
    for parameter in &method.parameters {
        names.push(parameter.name.as_str());
    }
    format!(
        "\nlet __tachyon_arguments = Behaviors::gateway_arguments(vec![{}])?;\nBehaviors::gateway_result(BehaviorValue::from(YonGateway::call({key:?}, &__tachyon_arguments)))\n",
        names.join(", ")
    )
}

fn has_context_field(declaration: &Type) -> bool {
    declaration
        .fields
        .iter()
        .any(|field| field.name == "behaviors" && field.type_source.trim() == "BehaviorContext")
}

fn validate_method(method: &Method) -> Result<(), String> {
    if method.asynchronous || method.is_static {
        return Err(format!(
            "Rust behavior method {} must be synchronous and take &self or &mut self; adapters receive cooperative deadlines, and Tachyon does not install an async executor",
            method.name
        ));
    }
    let return_type = method
        .return_type
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let model_result = method
        .metadata
        .get("mapping")
        .and_then(|policy| policy.get("result"))
        .and_then(Value::as_str);
    let typed = model_result.is_some_and(|model| {
        [
            format!("BehaviorResult<{model}>"),
            format!("Result<{model},BehaviorError>"),
        ]
        .contains(&return_type)
    });
    if !typed
        && !matches!(
            return_type.as_str(),
            "BehaviorResult<BehaviorValue>" | "Result<BehaviorValue,BehaviorError>"
        )
    {
        return Err(format!(
            "Rust behavior method {} must return BehaviorResult<BehaviorValue>; convert explicit JSON data at the application response boundary",
            method.name
        ));
    }
    if !typed
        && method
            .parameters
            .iter()
            .any(|parameter| parameter.type_source.trim() != "BehaviorValue")
    {
        return Err(format!(
            "Rust behavior method {} arguments must use the bounded BehaviorValue type",
            method.name
        ));
    }
    Ok(())
}

fn wrapper(module: &Module, method: &Method, policy: &str) -> Result<Edit, String> {
    let original = module
        .source
        .get(method.body.clone())
        .ok_or_else(|| String::from("Invalid Rust behavior method body range"))?;
    let signature = module
        .source
        .get(method.signature.clone())
        .ok_or_else(|| String::from("Invalid Rust behavior method signature range"))?;
    let tokens = signature
        .split(|character: char| character.is_whitespace() || character == ':')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let replacement = wrapper_body(method, policy, original, &tokens)?;
    Ok(Edit {
        range: method.body.clone(),
        replacement,
    })
}

fn wrapper_body(
    method: &Method,
    policy: &str,
    original: &str,
    tokens: &[&str],
) -> Result<String, String> {
    let arguments = method
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let valid = validation_prefix(method, tokens)?;
    if method.metadata.len() == 1 && method.metadata.contains_key("valid") {
        return Ok(format!("\n{valid}{original}\n"));
    }
    if let Some(query) = method.metadata.get("query") {
        return Ok(format!(
            "\nself.behaviors.clone().query({}, vec![{arguments}])\n",
            query_policy(query)?
        ));
    }
    if let Some(mapping) = method.metadata.get("mapping") {
        return Ok(format!("\n{}\n", mapping_call(method, mapping)?));
    }
    if RustAsyncClient::applies(method) {
        return Ok(RustAsyncClient::body(method, policy));
    }
    if is_generated_exchange(method) {
        return Ok(format!(
            "\n{valid}self.behaviors.clone().invoke({policy}, vec![{arguments}], |_| Ok(BehaviorValue::object([] as [(&str, BehaviorValue); 0])))\n"
        ));
    }
    Ok(invoke_body(
        method, policy, original, tokens, &valid, &arguments,
    ))
}

fn is_generated_exchange(method: &Method) -> bool {
    method
        .metadata
        .get("httpExchange")
        .and_then(|exchange| exchange.get("generated"))
        .is_some()
}

fn invoke_body(
    method: &Method,
    policy: &str,
    original: &str,
    tokens: &[&str],
    valid: &str,
    arguments: &str,
) -> String {
    let mut body = format!(
        "\n{valid}let __tachyon_context = self.behaviors.clone();\n__tachyon_context.invoke({policy}, vec![{arguments}], |__tachyon_arguments| {{\nlet mut __tachyon_arguments = __tachyon_arguments.into_iter();\n"
    );
    for parameter in &method.parameters {
        let mutable = mutable_prefix(tokens, &parameter.name);
        let _ = writeln!(
            body,
            "let {mutable}{} = __tachyon_arguments.next().ok_or_else(behavior_invalid)?;",
            parameter.name
        );
    }
    body.push_str(original);
    body.push_str("\n})\n");
    body
}

fn mutable_prefix(tokens: &[&str], name: &str) -> &'static str {
    if tokens.windows(2).any(|pair| pair == ["mut", name]) {
        "mut "
    } else {
        ""
    }
}

fn mapping_call(method: &Method, policy: &Value) -> Result<String, String> {
    let (argument, fields) = super::super::mapper::plan(policy)?;
    let value = method
        .parameters
        .get(argument)
        .ok_or("Convert reads an argument the method does not declare")?;
    let mut entries = Vec::new();
    for field in &fields {
        let read = match &field.source {
            Some(path) => format!("Some(&[{}][..])", quoted_names(path)),
            None => String::from("None"),
        };
        entries.push(format!(
            "({:?}, {read}, {})",
            field.target,
            value_literal(&field.constant)
        ));
    }
    let Some(result) = policy.get("result").and_then(Value::as_str) else {
        return Ok(format!(
            "{}.map_value(&[{}])",
            value.name,
            entries.join(", ")
        ));
    };
    let tac = policy["realm"] == "tac";
    let (kind, reader, source, object) = if tac {
        (
            "TacValue",
            "TacModel",
            format!("TacModel::write({})", value.name),
            "Object",
        )
    } else {
        (
            "Json",
            "YonRead",
            format!("Json::from({})", value.name),
            "Map",
        )
    };
    let mapped = typed_fields(&fields, tac);
    Ok(format!(
        "fn __tachyon_shape(value: {kind}, map: fn({kind}) -> {kind}) -> {kind} {{ match value {{ {kind}::List(items) => {kind}::List(items.into_iter().map(|item| __tachyon_shape(item, map)).collect()), {kind}::{object}(_) => map(value), value => value }} }}\nlet __tachyon_source = {source};\nOk(<{result} as {reader}>::read(&{mapped}))"
    ))
}

fn typed_fields(fields: &[super::super::mapper::Plan], tac: bool) -> String {
    let entries: Vec<String> = fields.iter().map(|field| typed_entry(field, tac)).collect();
    if tac {
        format!("TacValue::Object(vec![{}])", entries.join(", "))
    } else {
        format!(
            "Json::map([{}] as [(&str, Json); {}])",
            entries.join(", "),
            entries.len()
        )
    }
}

fn typed_entry(field: &super::super::mapper::Plan, tac: bool) -> String {
    let kind = if tac { "TacValue" } else { "Json" };
    let mut value = match &field.source {
        Some(path) => {
            let mut read = String::from("__tachyon_source");
            for name in path {
                if tac {
                    read = format!("tac_read_field::<TacValue>(&{read}, {name:?})");
                } else {
                    let _ = write!(read, ".get({name:?})");
                }
            }
            format!("{read}.clone()")
        }
        None => match &field.constant {
            Value::Bool(flag) if tac => format!("TacValue::Flag({flag})"),
            Value::Bool(flag) => format!("Json::from({flag})"),
            Value::Number(number) if tac => format!(
                "TacValue::Float({:?}_f64)",
                number.as_f64().unwrap_or_default()
            ),
            Value::Number(number) => {
                format!("Json::from({:?}_f64)", number.as_f64().unwrap_or_default())
            }
            Value::String(text) if tac => format!("TacValue::Text(String::from({text:?}))"),
            Value::String(text) => format!("Json::from({text:?})"),
            _ => format!("{kind}::Null"),
        },
    };
    if let Some(fields) = &field.fields {
        value = format!(
            "__tachyon_shape({value}, |__tachyon_source| {})",
            typed_fields(fields, tac)
        );
    }
    if tac {
        format!("(String::from({:?}), {value})", field.target)
    } else {
        format!("({:?}, {value})", field.target)
    }
}

fn quoted_names(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("{name:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn value_literal(constant: &Value) -> String {
    match constant {
        Value::Bool(value) => format!("BehaviorValue::Bool({value})"),
        Value::Number(value) => format!(
            "BehaviorValue::Number({:?}_f64)",
            value.as_f64().unwrap_or_default()
        ),
        Value::String(value) => format!("BehaviorValue::String({value:?}.to_owned())"),
        _ => String::from("BehaviorValue::Null"),
    }
}

fn validation_prefix(method: &Method, tokens: &[&str]) -> Result<String, String> {
    let Some(valid) = method.metadata.get("valid") else {
        return Ok(String::new());
    };
    let index = usize::try_from(number(valid, "argument")?)
        .map_err(|_| String::from("Invalid Rust Check argument index"))?;
    let parameter = method
        .parameters
        .get(index)
        .ok_or("Rust Check argument does not exist")?;
    let mutable = if method.metadata.len() == 1 {
        mutable_prefix(tokens, &parameter.name)
    } else {
        ""
    };
    Ok(format!(
        "let {mutable}{} = Behaviors::validate::<{}>({})?;\n",
        parameter.name,
        string(valid, "type")?,
        parameter.name
    ))
}

fn method_policy(declaration: &Type, method: &Method) -> Result<String, String> {
    let metadata = &method.metadata;
    let cache = policy_or_none(declaration.cache.as_ref(), cache_policy)?;
    let key_args = policy_or_none(metadata.get("cacheable"), key_arguments_policy)?;
    let page = policy_or_none(metadata.get("pageable"), page_policy)?;
    let http = policy_or_none(metadata.get("httpExchange"), http_policy)?;
    let mutation = mutation_policy(method)?;
    let retry = policy_or_none(metadata.get("retryable"), retry_policy)?;
    let transactional = policy_or_none(metadata.get("transactional"), transaction_policy)?;
    let limit = policy_or_none(metadata.get("concurrencyLimit"), limit_policy)?;
    Ok(format!(
        "BehaviorMethodPolicy {{ owner: {:?}, name: {:?}, cache: {cache}, key_args: {key_args}, pageable: {page}, http: {http}, mutation: {mutation}, retry: {retry}, limit: {limit}, transactional: {transactional} }}",
        declaration.name, method.name
    ))
}

fn policy_or_none(
    value: Option<&Value>,
    render_policy: impl Fn(&Value) -> Result<String, String>,
) -> Result<String, String> {
    match value {
        Some(value) => render_policy(value),
        None => Ok(String::from("None")),
    }
}

fn key_arguments_policy(cacheable: &Value) -> Result<String, String> {
    let indices = index_list(
        &cacheable["keyArgs"],
        "Invalid Rust Cached keyArgs",
        "Invalid Rust Cached argument index",
    )?;
    Ok(format!("Some(vec![{indices}])"))
}

fn index_list(value: &Value, list_error: &str, index_error: &str) -> Result<String, String> {
    let Some(values) = value.as_array() else {
        return Err(String::from(list_error));
    };
    let mut indices = Vec::new();
    for value in values {
        let Some(index) = value.as_u64() else {
            return Err(String::from(index_error));
        };
        indices.push(index.to_string());
    }
    Ok(indices.join(", "))
}

fn http_policy(exchange: &Value) -> Result<String, String> {
    let body = match exchange.get("body").and_then(Value::as_u64) {
        Some(index) => format!("Some({index})"),
        None => String::from("None"),
    };
    Ok(format!(
        "Some(BehaviorHttpPolicy {{ client: {:?}, method: {:?}, path: {:?}, query: &[{}], headers: &[{}], body: {body}, path_variables: &[{}] }})",
        string(exchange, "client")?,
        string(exchange, "method")?,
        string(exchange, "path")?,
        argument_pairs(exchange, "query"),
        argument_pairs(exchange, "headers"),
        argument_pairs(exchange, "pathVariables")
    ))
}

fn retry_policy(retryable: &Value) -> Result<String, String> {
    let max_retries = number(retryable, "maxRetries")?;
    let delay = number(retryable, "delayMs")?;
    let multiplier = retryable
        .get("multiplier")
        .and_then(Value::as_f64)
        .ok_or("Retry needs multiplier")?;
    let max_delay = number(retryable, "maxDelayMs")?;
    let jitter = number(retryable, "jitterMs")?;
    Ok(format!(
        "Some(BehaviorRetryPolicy {{ max_retries: {max_retries}, delay_ms: {delay}, multiplier: {multiplier:?}, max_delay_ms: {max_delay}, jitter_ms: {jitter} }})"
    ))
}

#[allow(clippy::unnecessary_wraps)]
fn transaction_policy(transactional: &Value) -> Result<String, String> {
    Ok(format!(
        "Some(BehaviorTransactionPolicy {{ adapter: {:?} }})",
        transactional["adapter"].as_str().unwrap_or_default()
    ))
}

fn limit_policy(concurrency_limit: &Value) -> Result<String, String> {
    let limit = number(concurrency_limit, "limit")?;
    let blocking = string(concurrency_limit, "policy")? == "block";
    Ok(format!(
        "Some(BehaviorLimitPolicy {{ limit: {limit}, blocking: {blocking} }})"
    ))
}

fn argument_pairs(exchange: &Value, key: &str) -> String {
    let Some(entries) = exchange.get(key).and_then(Value::as_object) else {
        return String::new();
    };
    let mut pairs = Vec::new();
    for (name, index) in entries {
        pairs.push(format!(
            "({name:?}, {})",
            index.as_u64().unwrap_or_default()
        ));
    }
    pairs.join(", ")
}

fn mutation_policy(method: &Method) -> Result<String, String> {
    let (operation, mutation) = if let Some(mutation) = method.metadata.get("cachePut") {
        ("cache:put", mutation)
    } else if let Some(mutation) = method.metadata.get("cacheEvict") {
        ("cache:evict", mutation)
    } else {
        return Ok(String::from("None"));
    };
    let target = mutation
        .get("target")
        .ok_or("Missing Rust cache mutation target")?;
    let arguments = index_list(
        &mutation["arguments"],
        "Invalid Rust cache mutation indices",
        "Invalid Rust cache mutation argument",
    )?;
    let key_args = index_list(
        &target["cacheable"]["keyArgs"],
        "Invalid Rust cache mutation indices",
        "Invalid Rust cache mutation argument",
    )?;
    let page = policy_or_none(target.get("pageable"), page_policy)?;
    Ok(format!(
        "Some(BehaviorCacheMutationPolicy {{ operation: {operation:?}, method: {:?}, arguments: vec![{arguments}], key_args: vec![{key_args}], pageable: {page} }})",
        string(target, "method")?
    ))
}

fn cache_policy(value: &Value) -> Result<String, String> {
    Ok(format!(
        "Some(BehaviorCachePolicy {{ name: {:?}, ttl_ms: {}, max_bytes: {}, max_entries: {} }})",
        string(value, "name")?,
        number(value, "ttlMs")?,
        number(value, "maxBytes")?,
        number(value, "maxEntries")?
    ))
}

fn query_policy(value: &Value) -> Result<String, String> {
    let entity = value.get("entity").ok_or("Missing Rust query entity")?;
    let fields = value["fields"]
        .as_array()
        .ok_or("Missing Rust query fields")?;
    let fields = quoted_strings(fields, "Invalid Rust query field")?;
    let predicates = value["where"]
        .as_array()
        .ok_or("Missing Rust query predicates")?;
    let predicates = query_predicates(predicates)?;
    let order = value["orderBy"]
        .as_array()
        .ok_or("Missing Rust query ordering")?;
    let order = query_order(order)?;
    Ok(format!(
        "BehaviorQueryPolicy {{ entity: BehaviorEntityMetadata {{ name: {:?}, adapter: {:?}, id: {:?}, table: {:?} }}, fields: vec![{fields}], default_size: {}, max_size: {}, operation: {:?}, predicates: vec![{predicates}], order_by: vec![{order}], page_argument: {} }}",
        string(entity, "name")?,
        string(entity, "adapter")?,
        string(entity, "id")?,
        string(entity, "table")?,
        number(value, "defaultSize")?,
        number(value, "maxSize")?,
        string(value, "operation")?,
        optional_index(value.get("pageArgument"))?
    ))
}

fn query_predicates(predicates: &[Value]) -> Result<String, String> {
    let mut rendered = Vec::new();
    for predicate in predicates {
        let argument = optional_index(predicate.get("argument"))?;
        rendered.push(format!(
            "BehaviorQueryPredicate {{ field: {:?}, operator: {:?}, argument: {argument} }}",
            string(predicate, "field")?,
            string(predicate, "operator")?
        ));
    }
    Ok(rendered.join(", "))
}

fn query_order(sorts: &[Value]) -> Result<String, String> {
    let mut rendered = Vec::new();
    for sort in sorts {
        rendered.push(format!(
            "BehaviorQuerySort {{ field: {:?}, direction: {:?} }}",
            string(sort, "field")?,
            string(sort, "direction")?
        ));
    }
    Ok(rendered.join(", "))
}

fn quoted_strings(values: &[Value], error: &str) -> Result<String, String> {
    let mut quoted = Vec::new();
    for value in values {
        let Some(text) = value.as_str() else {
            return Err(String::from(error));
        };
        quoted.push(format!("{text:?}"));
    }
    Ok(quoted.join(", "))
}

fn optional_index(value: Option<&Value>) -> Result<String, String> {
    match value {
        None | Some(Value::Null) => Ok(String::from("None")),
        Some(value) => value
            .as_u64()
            .map(|index| format!("Some({index})"))
            .ok_or_else(|| String::from("Invalid Rust behavior positional index")),
    }
}

fn page_policy(value: &Value) -> Result<String, String> {
    let sort = value
        .get("sort")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Invalid Rust Pageable sort"))?;
    let sort = quoted_strings(sort, "Invalid Rust Pageable sort field")?;
    Ok(format!(
        "Some(BehaviorPagePolicy {{ argument: {}, default_size: {}, max_size: {}, sort: vec![{sort}] }})",
        number(value, "argument")?,
        number(value, "defaultSize")?,
        number(value, "maxSize")?
    ))
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Invalid Rust behavior string policy {field}"))
}

fn number(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Invalid Rust behavior integer policy {field}"))
}

fn test_members(unit: &super::super::testing::Unit, module: &Module, owner: &Type) -> String {
    let environment = module.environment.as_str();
    let cleared_fakes = RustTestConstruction::cleared_fakes(owner, unit);
    let (report, answer) = if environment == "tac" {
        ("BehaviorResult<String>", "body.to_json(1048576)")
    } else {
        (
            "Result<Json, YonResourceError>",
            "Json::parse(&body.to_json(1048576).map_err(|_| YonResourceError)?).map_err(|_| YonResourceError)",
        )
    };
    let calls = super::super::ValueSource::rust_calls(&unit.cases, &unit.inputs);
    let case_names: Vec<String> = calls.iter().map(|(name, _)| name.clone()).collect();
    let names = quoted_names(&case_names);
    let mut dispatch = String::new();
    for (name, call) in &calls {
        let call = if environment == "tac" {
            RustTestConstruction::wrapped_call(module, name, call)
        } else {
            call.clone()
        };
        let _ = write!(dispatch, "{name:?} => CaseOutcome::from({call}).failure(),");
    }
    let profiles = profile_statements(unit);
    let properties = property_statements(unit);
    format!(
        "impl {} {{\n\
         fn start_behaviors(&self) -> BehaviorResult<()> {{\n\
         self.stop_behaviors();\n\
         let mut options = BehaviorOptions::new({:?}, BehaviorSignal::new());\n\
         {profiles}{properties}\n\
         *self.behaviors.borrow_mut() = Some(Behaviors::create::<{}>(options)?);\n\
         Ok(())\n\
         }}\n\
         fn stop_behaviors(&self) {{\n\
         if let Some(context) = self.behaviors.borrow_mut().take() {{ context.dispose(); }}\n\
         }}\n\
         fn run_case(&self, name: &str) -> Option<String> {{\n\
         let hook = std::panic::take_hook();\n\
         std::panic::set_hook(Box::new(|_| {{}}));\n\
         let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match name {{ {dispatch} _ => None }}));\n\
         std::panic::set_hook(hook);\n\
         outcome.unwrap_or_else(|payload| Some(Assert::panic_message(payload.as_ref())))\n\
         }}\n\
         #[allow(non_snake_case)]\n\
         fn POST(&self) -> {report} {{\n\
         let mut cases = Vec::new();\n\
         for name in [{names}] {{\n\
         {cleared_fakes}\
         let failure = match self.start_behaviors() {{ Ok(()) => {{ BehaviorCalls::take(); self.run_case(name) }} Err(error) => Some(error.message.to_owned()) }};\n\
         cases.push(match failure {{\n\
         None => BehaviorValue::object([(\"name\", name.into()), (\"passed\", BehaviorValue::Bool(true)), (\"called\", BehaviorCalls::take())]),\n\
         Some(message) => BehaviorValue::object([(\"name\", name.into()), (\"passed\", BehaviorValue::Bool(false)), (\"error\", BehaviorValue::String(message)), (\"called\", BehaviorCalls::take())]),\n\
         }});\n\
         self.stop_behaviors();\n\
         }}\n\
         let body = BehaviorValue::object([(\"unit\", {:?}.into()), (\"cases\", BehaviorValue::Array(cases))]);\n\
         {answer}\n\
         }}\n\
         }}\n{}",
        unit.name,
        unit.partition,
        unit.configuration,
        unit.name,
        super::super::TestAssert::RUST,
    )
}

fn profile_statements(unit: &super::super::testing::Unit) -> String {
    let Some(profiles) = unit.profiles.as_array() else {
        return String::new();
    };
    super::super::testing::joined(profiles.iter().filter_map(Value::as_str), |profile| {
        format!("options.profiles.push({profile:?}.to_owned());")
    })
}

fn property_statements(unit: &super::super::testing::Unit) -> String {
    let Some(properties) = unit.properties.as_object() else {
        return String::new();
    };
    super::super::testing::joined(properties, |(key, value)| {
        format!(
            "options.properties.insert({key:?}.to_owned(), {:?}.to_owned());",
            value.to_string()
        )
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::external_command::ToolOutput;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    fn executable_path(directory: &Path, name: &str) -> PathBuf {
        if cfg!(windows) {
            directory.join(format!("{name}.exe"))
        } else {
            directory.join(name)
        }
    }

    async fn run_successfully(
        command: &mut tokio::process::Command,
        description: &str,
    ) -> ToolOutput {
        let output = crate::external_command::run(command, Duration::from_mins(2), 65_536)
            .await
            .expect(description);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    async fn compile_and_run(
        source: &Path,
        executable: &Path,
        compiler_description: &str,
        execution_description: &str,
    ) -> String {
        let mut compiler = tokio::process::Command::new("rustc");
        compiler
            .args(["--edition", "2024"])
            .arg(source)
            .arg("-o")
            .arg(executable);
        run_successfully(&mut compiler, compiler_description).await;
        let mut command = tokio::process::Command::new(executable);
        let output = run_successfully(&mut command, execution_description).await;
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn contract_service_source() -> String {
        let fixture = include_str!("../../../tests/fixtures/behavior/rust_annotations.yon.rs");
        let (providers, _) = fixture
            .split_once("\nstruct Handler;")
            .expect("Yon entry boundary");
        let providers = providers
            .replace("YON_", "TAC_")
            .replace("yon.", "tac.")
            .replace("\"yon\"", "\"tac\"");
        format!(
            r#"
struct ContractService {{
    ready: bool,
}}
impl ContractService {{
    pub fn new() -> Self {{ Self {{ ready: true }} }}
    pub fn run(&self) -> String {{
        let value = exercise().expect("all Rust native behavior adapters");
        assert_eq!(value.get("fresh").and_then(|value| value.get("calls")), Some(&BehaviorValue::Number(2.0)));
        assert_eq!(value.get("remote"), Some(&BehaviorValue::from("/products")));
        String::from("native behaviors passed")
    }}
}}
{providers}
"#
        )
    }

    fn native_bridge_program(lowered: &str, table: &str) -> String {
        let prelude = crate::native::companion_prelude();
        format!(
            r###"{lowered}
{table}
const TAC_APPLICATION_ID: &str = "org.tachyon.behavior.contract";
fn tac_service_members(service: &str) -> Option<Vec<(&'static str, TacMember)>> {{ (service == "contract").then(tac_create) }}
fn tac_service_required(_service: &str) -> &'static [&'static str] {{ &[] }}
{prelude}
fn main() {{
    let _ = tac_invoke_owned(r##"{{"route":"/","service":"contract","owner":"00000000-0000-4000-8000-000000000001","op":"init"}}"##);
    let answer = tac_invoke_owned(r##"{{"route":"/","service":"contract","owner":"00000000-0000-4000-8000-000000000001","op":"call","name":"run","args":[]}}"##);
    assert_eq!(answer, r##"{{"value":"native behaviors passed"}}"##);
    println!("native behavior bridge passed");
}}
"###
        )
    }

    #[tokio::test]
    async fn rust_tac_behaviors_execute_through_the_native_companion_bridge() {
        let lowered = super::super::lower(
            Path::new("client/services/ContractService.rs"),
            &contract_service_source(),
            "tac",
        )
        .expect("native lowering")
        .expect("annotated native source");
        let table = crate::companion::owner_table(
            crate::companion::CompanionLanguage::Rust,
            &lowered,
            "client/services/ContractService.rs",
            &[],
            "",
            false,
        )
        .expect("real native member table");
        let root = tempfile::tempdir().expect("native runtime directory");
        let source = root.path().join("companion.rs");
        let executable = executable_path(root.path(), "companion");
        std::fs::write(&source, native_bridge_program(&lowered, &table)).expect("native program");
        let output = compile_and_run(
            &source,
            &executable,
            "native Rust compiler",
            "native Rust execution",
        )
        .await;
        assert!(output.contains("native behavior bridge passed"));
    }

    #[tokio::test]
    async fn rust_behavior_runtime_contracts_execute_with_owned_processes() {
        let root = tempfile::tempdir().expect("temporary executable directory");
        let executable = executable_path(root.path(), "behaviors");
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/behavior/rust_runtime_contract.rs");
        let output = compile_and_run(
            &source,
            &executable,
            "Rust compiler",
            "Rust runtime execution",
        )
        .await;
        assert!(output.contains("rust behavior runtime contracts passed"));
    }

    fn lower_client(source: &str) -> String {
        super::super::lower(Path::new("server/remotes/ReportClient.rs"), source, "yon")
            .expect("Rust gateway lowering")
            .expect("lowered source")
    }

    #[test]
    fn a_gateway_method_calls_the_transport_without_a_behavior_context() {
        let lowered = lower_client(
            "struct ReportClient;\nimpl ReportClient {\n    #[Gateway({\"class\":\"server/services/ReportService.py\"})]\n    fn summarize(&self, mode: BehaviorValue, detail: BehaviorValue) -> BehaviorResult<BehaviorValue> {}\n}\n",
        );
        assert!(lowered.contains(
            "fn summarize(&self, mode: BehaviorValue, detail: BehaviorValue) -> BehaviorResult<BehaviorValue> {\nlet __tachyon_arguments = Behaviors::gateway_arguments(vec![mode, detail])?;\nBehaviors::gateway_result(BehaviorValue::from(YonGateway::call(\"ReportClient.summarize\", &__tachyon_arguments)))\n}"
        ));
        assert!(!lowered.contains("impl BehaviorService for ReportClient"));
    }

    #[test]
    fn a_gateway_trait_becomes_a_struct_without_a_behavior_context() {
        let lowered = lower_client(
            "trait ReportClient {\n    #[Gateway({\"command\":[\"python3\"]})]\n    fn ping(&self) -> BehaviorResult<BehaviorValue>;\n}\n",
        );
        assert!(lowered.contains(
            "struct ReportClient {}\nimpl ReportClient { pub fn new() -> Self { Self {} } }\nimpl ReportClient {"
        ));
        assert!(lowered.contains("YonGateway::call(\"ReportClient.ping\", &__tachyon_arguments)"));
        assert!(!lowered.contains("behaviors: BehaviorContext"));
    }

    #[test]
    fn a_gateway_parameter_must_be_a_behavior_value() {
        let failure = super::super::lower(
            Path::new("server/remotes/ReportClient.rs"),
            "struct ReportClient;\nimpl ReportClient {\n    #[Gateway({\"command\":[\"python3\"]})]\n    fn ping(&self, mode: String) -> BehaviorResult<BehaviorValue> {}\n}\n",
            "yon",
        )
        .expect_err("typed parameter");
        let message = failure.to_string();
        assert!(message.contains("TY1312"), "{message}");
        assert!(
            message.contains("must take BehaviorValue parameters"),
            "{message}"
        );
    }
}
