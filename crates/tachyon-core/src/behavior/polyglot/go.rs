use super::super::call_style::CallStyle;
use super::go_test_construction::GoTestConstruction;
use super::go_typed_exchange::GoTypedExchange;
use super::{Edit, Method, Module, Type, render};
use crate::go_source::GoFile;
use serde_json::Value;
use std::fmt::Write as _;

struct GoNames;

impl GoNames {
    const RECEIVER: &str = "__tachyon_receiver";
    const CONTEXT_TYPE: &str = "*BehaviorContext";
    const RESULTS: &str = "(BehaviorValue,error)";
    const TEST_RESULTS: [&str; 2] = ["", "error"];
    const ASSERT_FILE: &str = "assert";
    const NO_BUILTINS: &str = "\nfunc behaviorBuiltinCache(name string) (BehaviorCacheAdapter, bool) {\n\treturn nil, false\n}\n\nfunc behaviorBuiltinEntities(name string) (BehaviorEntityAdapter, bool) {\n\treturn nil, false\n}\n\nfunc behaviorDefaultExchange(request *BehaviorHttpRequest, operation *behaviorOperation, publicRead bool) (BehaviorValue, error) {\n\treturn nil, behaviorInvalid()\n}\n";
}

pub(super) fn lower(module: &Module) -> Result<String, String> {
    if module.language != "go" {
        return Err(String::from(
            "Go behavior lowering requires a Go source module",
        ));
    }
    let mut edits = Vec::new();
    let mut generated = String::new();
    for declaration in &module.types {
        lower_type(module, declaration, &mut edits, &mut generated)?;
    }
    let mut suffix = runtime(&module.module_id, &module.environment);
    suffix.push_str(&generated);
    suffix.push_str(&assert_file(&module.types));
    render(module, edits, &suffix)
}

pub(super) fn runtime(module_id: &str, environment: &str) -> String {
    let mut text = format!("{}behavior\n", GoFile::MARKER);
    text.push_str(include_str!("go_runtime.go"));
    if environment == "tac" {
        text.push_str(
            include_str!("go_storage.go")
                .strip_prefix("package main\n")
                .unwrap_or_default(),
        );
        text.push_str(
            include_str!("go_browser_http.go")
                .strip_prefix("package main\n")
                .unwrap_or_default(),
        );
    } else {
        text.push_str(GoNames::NO_BUILTINS);
    }
    let _ = writeln!(text, "\nconst behaviorModuleID = {}", quoted(module_id));
    let _ = writeln!(text, "const behaviorEnvironment = {}", quoted(environment));
    text
}

fn lower_type(
    module: &Module,
    declaration: &Type,
    edits: &mut Vec<Edit>,
    generated: &mut String,
) -> Result<(), String> {
    if declaration.declared {
        edits.push(declared_struct(declaration));
    }
    if let Some(unit) = super::super::testing::polyglot_unit(declaration) {
        let silent = silent_test_cases(module, declaration, &unit)?;
        generated.push_str(&test_members(&unit, &silent));
        if [module.environment == "tac", !unit.mocks.is_empty()] == [true, true] {
            generated.push_str(&GoTestConstruction::members(module, declaration, &unit));
        }
    }
    configuration(declaration, generated)?;
    properties(declaration, generated)?;
    values(declaration, generated)?;
    validation(declaration, generated)?;
    entity(declaration, generated)?;
    service(module, declaration, edits, generated)
}

fn declared_struct(declaration: &Type) -> Edit {
    let fields = if super::needs_no_context(declaration) {
        String::from("struct{}")
    } else {
        format!("struct {{ behaviors {} }}", GoNames::CONTEXT_TYPE)
    };
    Edit {
        range: declaration.keyword.clone(),
        replacement: format!(
            "{fields}; type behaviorDeclared{} interface",
            declaration.name
        ),
    }
}

fn configuration(declaration: &Type, output: &mut String) -> Result<(), String> {
    if !declaration.configuration {
        return Ok(());
    }
    let name = &declaration.name;
    let profiles = configuration_profiles(declaration)?;
    let property = property_condition(declaration)?;
    let _ = writeln!(
        output,
        "\nfunc ({name}) behaviorConfigure(execution *BehaviorExecution) (*BehaviorConfiguration, error) {{\n\treturn (&{name}{{}}).Configure(execution)\n}}"
    );
    let _ = writeln!(
        output,
        "\nfunc ({name}) behaviorConditions() BehaviorConfigurationConditions {{\n\treturn BehaviorConfigurationConditions{{Profiles: []string{{{profiles}}}, Property: {property}}}\n}}"
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
    quoted_strings(profiles, "Invalid Go configuration profile")
}

fn property_condition(declaration: &Type) -> Result<String, String> {
    let Some(condition) = declaration.policies.get("conditionalOnProperty") else {
        return Ok(String::from("nil"));
    };
    let match_if_missing = condition["matchIfMissing"]
        .as_bool()
        .ok_or("Invalid Go configuration matchIfMissing")?;
    Ok(format!(
        "&BehaviorPropertyCondition{{Name: {}, HavingValue: {}, MatchIfMissing: {match_if_missing}}}",
        quoted(string(condition, "name")?),
        quoted(string(condition, "havingValue")?)
    ))
}

fn properties(declaration: &Type, output: &mut String) -> Result<(), String> {
    let Some(policy) = declaration.policies.get("configurationProperties") else {
        return Ok(());
    };
    let fields = policy["fields"]
        .as_object()
        .ok_or("Missing Go configuration fields")?;
    let fields = property_fields(fields, "name")?;
    let _ = writeln!(
        output,
        "\nfunc ({}) behaviorProperties() BehaviorPropertiesPolicy {{\n\treturn BehaviorPropertiesPolicy{{Prefix: {}, Fields: []BehaviorPropertyField{{{fields}}}}}\n}}",
        declaration.name,
        quoted(string(policy, "prefix")?)
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
        "\nfunc ({}) behaviorValues() BehaviorValuesPolicy {{\n\treturn BehaviorValuesPolicy{{Fields: []BehaviorPropertyField{{{fields}}}}}\n}}",
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
    let mut text = format!(
        "{{Field: {}, Name: {}, Kind: {}",
        quoted(field),
        quoted(string(descriptor, name_key)?),
        quoted(string(descriptor, "type")?)
    );
    if let Some(default) = descriptor.get("default") {
        let _ = write!(text, ", Default: {}", property_default(default)?);
    }
    text.push_str(&bounds(descriptor.get("min"), descriptor.get("max"))?);
    text.push('}');
    Ok(text)
}

fn bounds(min: Option<&Value>, max: Option<&Value>) -> Result<String, String> {
    let mut text = String::new();
    if let Some(min) = min {
        let _ = write!(text, ", Min: {}, HasMin: true", bound(min)?);
    }
    if let Some(max) = max {
        let _ = write!(text, ", Max: {}, HasMax: true", bound(max)?);
    }
    Ok(text)
}

fn bound(value: &Value) -> Result<String, String> {
    let Some(bound) = value.as_f64().filter(|bound| bound.is_finite()) else {
        return Err(String::from("Invalid Go configuration numeric bound"));
    };
    Ok(format!("{bound:?}"))
}

fn property_default(value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(quoted(text)),
        Value::Bool(flag) => Ok(flag.to_string()),
        Value::Number(number) => {
            let number = number
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or("Invalid Go configuration numeric default")?;
            Ok(format!("float64({number:?})"))
        }
        _ => Err(String::from(
            "Go configuration defaults require declared scalar values",
        )),
    }
}

fn entity(declaration: &Type, output: &mut String) -> Result<(), String> {
    let Some(entity) = &declaration.entity else {
        return Ok(());
    };
    let table = declaration
        .table
        .as_deref()
        .ok_or_else(|| String::from("Go Entity requires Table"))?;
    let _ = writeln!(
        output,
        "\nfunc ({}) behaviorEntity() BehaviorEntityMetadata {{\n\treturn {}\n}}",
        declaration.name,
        entity_metadata(
            &declaration.name,
            string(entity, "adapter")?,
            table,
            string(entity, "id")?
        )
    );
    Ok(())
}

fn entity_metadata(name: &str, adapter: &str, table: &str, id: &str) -> String {
    format!(
        "BehaviorEntityMetadata{{Name: {}, Adapter: {}, Table: {}, Id: {}}}",
        quoted(name),
        quoted(adapter),
        quoted(table),
        quoted(id)
    )
}

fn validation(declaration: &Type, output: &mut String) -> Result<(), String> {
    let Some(schema) = declaration.policies.get("validated") else {
        return Ok(());
    };
    let fields = schema["fields"]
        .as_object()
        .ok_or("Missing Go validation fields")?;
    let mut rendered = Vec::new();
    for (name, rules) in fields {
        rendered.push(validation_field(name, rules)?);
    }
    let _ = writeln!(
        output,
        "\nfunc ({}) behaviorValidation() BehaviorValidationSchema {{\n\treturn BehaviorValidationSchema{{Fields: []BehaviorValidationField{{{}}}}}\n}}",
        declaration.name,
        rendered.join(", ")
    );
    Ok(())
}

fn validation_field(name: &str, rules: &Value) -> Result<String, String> {
    let mut text = format!("{{Name: {}", quoted(name));
    if rules.get("notNull").and_then(Value::as_bool) == Some(true) {
        text.push_str(", NotNull: true");
    }
    if rules.get("notBlank").and_then(Value::as_bool) == Some(true) {
        text.push_str(", NotBlank: true");
    }
    if let Some(size) = rules.get("size") {
        let _ = write!(
            text,
            ", HasSize: true, SizeMin: {}, SizeMax: {}",
            number(size, "min")?,
            number(size, "max")?
        );
    }
    text.push_str(&bounds(rules.get("min"), rules.get("max"))?);
    text.push('}');
    Ok(text)
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
            "Annotated Go service {} requires a behaviors *BehaviorContext field",
            declaration.name
        ));
    }
    let mut cases = String::new();
    for method in methods {
        if GoTypedExchange::applies(method, &module.environment) {
            GoTypedExchange::validate(method)?;
        } else {
            validate_method(method)?;
        }
        let receiver = receiver_name(module, method);
        let body = if let Some(policy) = method.metadata.get("gateway") {
            gateway_body(method, policy)
        } else {
            let policy = method_policy(declaration, method)?;
            let _ = write!(
                cases,
                "\n\tcase {}:\n\t\treturn &{policy}",
                quoted(&method.name)
            );
            wrapper(module, method, &policy, &receiver)?
        };
        place_body(module, declaration, method, body, edits, output);
    }
    if needs_context {
        service_members(declaration, &cases, output);
    }
    Ok(())
}

fn place_body(
    module: &Module,
    declaration: &Type,
    method: &Method,
    body: String,
    edits: &mut Vec<Edit>,
    output: &mut String,
) {
    if !method.declared {
        edits.push(Edit {
            range: method.body.clone(),
            replacement: body,
        });
        return;
    }
    let signature = module.source[method.signature.clone()].trim();
    let _ = writeln!(
        output,
        "\nfunc ({} *{}) {signature} {{{body}}}",
        GoNames::RECEIVER,
        declaration.name
    );
}

fn service_members(declaration: &Type, cases: &str, output: &mut String) {
    let name = &declaration.name;
    let receiver = GoNames::RECEIVER;
    let _ = writeln!(
        output,
        "\nfunc ({receiver} {name}) behaviorContext() *BehaviorContext {{\n\treturn {receiver}.behaviors\n}}"
    );
    let _ = writeln!(
        output,
        "\nfunc ({name}) behaviorMethod(name string) *BehaviorMethodPolicy {{\n\tswitch name {{{cases}\n\t}}\n\treturn nil\n}}"
    );
}

fn receiver_name(module: &Module, method: &Method) -> String {
    if method.declared {
        return String::from(GoNames::RECEIVER);
    }
    let signature = &module.source[method.signature.clone()];
    crate::go_source::receiver_name(signature)
}

fn has_context_field(declaration: &Type) -> bool {
    declaration
        .fields
        .iter()
        .any(|field| field.name == "behaviors" && field.type_source.trim() == GoNames::CONTEXT_TYPE)
}

fn validate_method(method: &Method) -> Result<(), String> {
    let results = compact(&method.return_type);
    let model_result = method
        .metadata
        .get("mapping")
        .and_then(|policy| policy.get("result"))
        .and_then(Value::as_str);
    let typed = model_result.is_some_and(|model| results == format!("({model},error)"));
    if results != GoNames::RESULTS && !typed {
        return Err(format!(
            "Go behavior method {} must return (BehaviorValue, error); convert explicit JSON data at the application response boundary",
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
            "Go behavior method {} arguments must use the bounded BehaviorValue type",
            method.name
        ));
    }
    Ok(())
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn argument_list(method: &Method) -> String {
    let mut names = Vec::new();
    for parameter in &method.parameters {
        names.push(parameter.name.as_str());
    }
    format!("[]BehaviorValue{{{}}}", names.join(", "))
}

fn gateway_body(method: &Method, policy: &Value) -> String {
    let key = policy["key"].as_str().unwrap_or_default();
    format!(
        " __tachyon_arguments, __tachyon_error := BehaviorsGatewayArguments({}); if __tachyon_error != nil {{ return nil, __tachyon_error }}; return BehaviorsGatewayResult(YonGateway.Call({}, __tachyon_arguments)) ",
        argument_list(method),
        quoted(key)
    )
}

fn wrapper(
    module: &Module,
    method: &Method,
    policy: &str,
    receiver: &str,
) -> Result<String, String> {
    let original = module
        .source
        .get(method.body.clone())
        .ok_or_else(|| String::from("Invalid Go behavior method body range"))?;
    let valid = validation_prefix(method)?;
    if method.metadata.len() == 1 && method.metadata.contains_key("valid") {
        return Ok(format!("{valid}{original}"));
    }
    if let Some(mapping) = method.metadata.get("mapping") {
        return Ok(format!(" return {} ", mapping_call(method, mapping)?));
    }
    require_receiver(method, receiver)?;
    let arguments = argument_list(method);
    if let Some(query) = method.metadata.get("query") {
        return Ok(format!(
            " return {receiver}.behaviors.Query({}, {arguments}) ",
            query_policy(query)?
        ));
    }
    if GoTypedExchange::applies(method, &module.environment) {
        return Ok(format!(
            "{valid}{}",
            GoTypedExchange::body(method, policy, receiver)
        ));
    }
    if is_generated_exchange(method) {
        return Ok(format!(
            "{valid} return {receiver}.behaviors.Invoke({policy}, {arguments}, func([]BehaviorValue) (BehaviorValue, error) {{ return map[string]any{{}}, nil }}) "
        ));
    }
    Ok(format!(
        "{valid} return {receiver}.behaviors.Invoke({policy}, {arguments}, func(__tachyon_arguments []BehaviorValue) (BehaviorValue, error) {{ {}{original}}})",
        argument_bindings(method)
    ))
}

fn require_receiver(method: &Method, receiver: &str) -> Result<(), String> {
    if !receiver.is_empty() && receiver != "_" {
        return Ok(());
    }
    Err(format!(
        "Go behavior method {} needs a named receiver, for example func (service *Catalog) {}(...)",
        method.name, method.name
    ))
}

fn argument_bindings(method: &Method) -> String {
    let mut bindings = String::new();
    for (index, parameter) in method.parameters.iter().enumerate() {
        if parameter.name == "_" {
            continue;
        }
        let _ = write!(
            bindings,
            "{} := __tachyon_arguments[{index}]; _ = {}; ",
            parameter.name, parameter.name
        );
    }
    bindings
}

fn is_generated_exchange(method: &Method) -> bool {
    method
        .metadata
        .get("httpExchange")
        .and_then(|exchange| exchange.get("generated"))
        .is_some()
}

fn mapping_call(method: &Method, policy: &Value) -> Result<String, String> {
    let (argument, fields) = super::super::mapper::plan(policy)?;
    let value = method
        .parameters
        .get(argument)
        .ok_or("Convert reads an argument the method does not declare")?;
    let plan = mapping_fields(&fields)?;
    Ok(match policy.get("result").and_then(Value::as_str) {
        Some(result) => format!("BehaviorMapModel[{result}]({}, {plan})", value.name),
        None => format!("BehaviorMapValue({}, {plan})", value.name),
    })
}

fn mapping_fields(fields: &[super::super::mapper::Plan]) -> Result<String, String> {
    let mut entries = Vec::new();
    for field in fields {
        let read = match &field.source {
            Some(path) => format!("Source: []string{{{}}}", quoted_names(path)),
            None => format!("Constant: {}", value_literal(&field.constant)),
        };
        let nested = field
            .fields
            .as_ref()
            .map(|fields| mapping_fields(fields))
            .transpose()?
            .map(|plan| format!(", Fields: {plan}"))
            .unwrap_or_default();
        entries.push(format!(
            "{{Target: {}, {read}{nested}}}",
            quoted(&field.target)
        ));
    }
    Ok(format!("[]BehaviorMappingField{{{}}}", entries.join(", ")))
}

fn value_literal(constant: &Value) -> String {
    match constant {
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => format!("float64({:?})", number.as_f64().unwrap_or_default()),
        Value::String(text) => quoted(text),
        _ => String::from("nil"),
    }
}

fn validation_prefix(method: &Method) -> Result<String, String> {
    let Some(valid) = method.metadata.get("valid") else {
        return Ok(String::new());
    };
    let index = usize::try_from(number(valid, "argument")?)
        .map_err(|_| String::from("Invalid Go Check argument index"))?;
    let parameter = method
        .parameters
        .get(index)
        .ok_or("Go Check argument does not exist")?;
    Ok(format!(
        " {}, __tachyon_error := BehaviorsValidate[{}]({}); if __tachyon_error != nil {{ return nil, __tachyon_error }};",
        parameter.name,
        string(valid, "type")?,
        parameter.name
    ))
}

fn method_policy(declaration: &Type, method: &Method) -> Result<String, String> {
    let metadata = &method.metadata;
    let mut fields = vec![
        format!("Owner: {}", quoted(&declaration.name)),
        format!("Name: {}", quoted(&method.name)),
    ];
    if let Some(cache) = &declaration.cache {
        fields.push(format!("Cache: &{}", cache_policy(cache)?));
    }
    if let Some(cacheable) = metadata.get("cacheable") {
        fields.push(format!("KeyArgs: {}", key_arguments(cacheable)?));
    }
    if let Some(page) = metadata.get("pageable") {
        fields.push(format!("Pageable: &{}", page_policy(page)?));
    }
    if let Some(exchange) = metadata.get("httpExchange") {
        fields.push(format!("Http: &{}", http_policy(exchange)?));
    }
    if let Some(mutation) = mutation_policy(method)? {
        fields.push(format!("Mutation: &{mutation}"));
    }
    if let Some(retryable) = metadata.get("retryable") {
        fields.push(format!("Retry: &{}", retry_policy(retryable)?));
    }
    if let Some(limit) = metadata.get("concurrencyLimit") {
        fields.push(format!("Limit: &{}", limit_policy(limit)?));
    }
    if let Some(transactional) = metadata.get("transactional") {
        fields.push(format!(
            "Transactional: &BehaviorTransactionPolicy{{Adapter: {}}}",
            quoted(transactional["adapter"].as_str().unwrap_or_default())
        ));
    }
    Ok(format!("BehaviorMethodPolicy{{{}}}", fields.join(", ")))
}

fn key_arguments(cacheable: &Value) -> Result<String, String> {
    let indices = index_list(
        &cacheable["keyArgs"],
        "Invalid Go Cached keyArgs",
        "Invalid Go Cached argument index",
    )?;
    Ok(format!("[]int{{{indices}}}"))
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
    let mut text = format!(
        "BehaviorHttpPolicy{{Client: {}, Method: {}, Path: {}",
        quoted(string(exchange, "client")?),
        quoted(string(exchange, "method")?),
        quoted(string(exchange, "path")?)
    );
    let query = argument_names(exchange, "query");
    if !query.is_empty() {
        let _ = write!(text, ", Query: []BehaviorArgumentName{{{query}}}");
    }
    let headers = argument_names(exchange, "headers");
    if !headers.is_empty() {
        let _ = write!(text, ", Headers: []BehaviorArgumentName{{{headers}}}");
    }
    if let Some(index) = exchange.get("body").and_then(Value::as_u64) {
        let _ = write!(text, ", Body: {index}, HasBody: true");
    }
    let path_variables = argument_names(exchange, "pathVariables");
    if !path_variables.is_empty() {
        let _ = write!(
            text,
            ", PathVariables: []BehaviorArgumentName{{{path_variables}}}"
        );
    }
    text.push('}');
    Ok(text)
}

fn argument_names(exchange: &Value, key: &str) -> String {
    let Some(entries) = exchange.get(key).and_then(Value::as_object) else {
        return String::new();
    };
    let mut pairs = Vec::new();
    for (name, index) in entries {
        pairs.push(format!(
            "{{Name: {}, Argument: {}}}",
            quoted(name),
            index.as_u64().unwrap_or_default()
        ));
    }
    pairs.join(", ")
}

fn retry_policy(retryable: &Value) -> Result<String, String> {
    let multiplier = retryable
        .get("multiplier")
        .and_then(Value::as_f64)
        .ok_or("Retry needs multiplier")?;
    Ok(format!(
        "BehaviorRetryPolicy{{MaxRetries: {}, DelayMs: {}, Multiplier: {multiplier:?}, MaxDelayMs: {}, JitterMs: {}}}",
        number(retryable, "maxRetries")?,
        number(retryable, "delayMs")?,
        number(retryable, "maxDelayMs")?,
        number(retryable, "jitterMs")?
    ))
}

fn limit_policy(concurrency_limit: &Value) -> Result<String, String> {
    let blocking = string(concurrency_limit, "policy")? == "block";
    Ok(format!(
        "BehaviorLimitPolicy{{Limit: {}, Blocking: {blocking}}}",
        number(concurrency_limit, "limit")?
    ))
}

fn mutation_policy(method: &Method) -> Result<Option<String>, String> {
    let (operation, mutation) = if let Some(mutation) = method.metadata.get("cachePut") {
        ("cache:put", mutation)
    } else if let Some(mutation) = method.metadata.get("cacheEvict") {
        ("cache:evict", mutation)
    } else {
        return Ok(None);
    };
    let target = mutation
        .get("target")
        .ok_or("Missing Go cache mutation target")?;
    let arguments = index_list(
        &mutation["arguments"],
        "Invalid Go cache mutation indices",
        "Invalid Go cache mutation argument",
    )?;
    let key_args = index_list(
        &target["cacheable"]["keyArgs"],
        "Invalid Go cache mutation indices",
        "Invalid Go cache mutation argument",
    )?;
    let mut text = format!(
        "BehaviorCacheMutationPolicy{{Operation: {}, Method: {}, Arguments: []int{{{arguments}}}, KeyArgs: []int{{{key_args}}}",
        quoted(operation),
        quoted(string(target, "method")?)
    );
    if let Some(page) = target.get("pageable") {
        let _ = write!(text, ", Pageable: &{}", page_policy(page)?);
    }
    text.push('}');
    Ok(Some(text))
}

fn cache_policy(value: &Value) -> Result<String, String> {
    Ok(format!(
        "BehaviorCachePolicy{{Name: {}, TtlMs: {}, MaxBytes: {}, MaxEntries: {}}}",
        quoted(string(value, "name")?),
        number(value, "ttlMs")?,
        number(value, "maxBytes")?,
        number(value, "maxEntries")?
    ))
}

fn page_policy(value: &Value) -> Result<String, String> {
    let sort = value
        .get("sort")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Invalid Go Pageable sort"))?;
    Ok(format!(
        "BehaviorPagePolicy{{Argument: {}, DefaultSize: {}, MaxSize: {}, Sort: []string{{{}}}}}",
        number(value, "argument")?,
        number(value, "defaultSize")?,
        number(value, "maxSize")?,
        quoted_strings(sort, "Invalid Go Pageable sort field")?
    ))
}

fn query_policy(value: &Value) -> Result<String, String> {
    let entity = value.get("entity").ok_or("Missing Go query entity")?;
    let fields = value["fields"]
        .as_array()
        .ok_or("Missing Go query fields")?;
    let predicates = value["where"]
        .as_array()
        .ok_or("Missing Go query predicates")?;
    let order = value["orderBy"]
        .as_array()
        .ok_or("Missing Go query ordering")?;
    let mut text = format!(
        "BehaviorQueryPolicy{{Entity: {}, Fields: []string{{{}}}, DefaultSize: {}, MaxSize: {}, Operation: {}, Predicates: []BehaviorQueryPredicate{{{}}}, OrderBy: []BehaviorQuerySort{{{}}}",
        entity_metadata(
            string(entity, "name")?,
            string(entity, "adapter")?,
            string(entity, "table")?,
            string(entity, "id")?
        ),
        quoted_strings(fields, "Invalid Go query field")?,
        number(value, "defaultSize")?,
        number(value, "maxSize")?,
        quoted(string(value, "operation")?),
        query_predicates(predicates)?,
        query_order(order)?
    );
    if let Some(index) = optional_index(value.get("pageArgument"))? {
        let _ = write!(text, ", PageArgument: {index}, HasPageArgument: true");
    }
    text.push('}');
    Ok(text)
}

fn query_predicates(predicates: &[Value]) -> Result<String, String> {
    let mut rendered = Vec::new();
    for predicate in predicates {
        let mut text = format!(
            "{{Field: {}, Operator: {}",
            quoted(string(predicate, "field")?),
            quoted(string(predicate, "operator")?)
        );
        if let Some(index) = optional_index(predicate.get("argument"))? {
            let _ = write!(text, ", Argument: {index}, HasArgument: true");
        }
        text.push('}');
        rendered.push(text);
    }
    Ok(rendered.join(", "))
}

fn query_order(sorts: &[Value]) -> Result<String, String> {
    let mut rendered = Vec::new();
    for sort in sorts {
        rendered.push(format!(
            "{{Field: {}, Direction: {}}}",
            quoted(string(sort, "field")?),
            quoted(string(sort, "direction")?)
        ));
    }
    Ok(rendered.join(", "))
}

fn optional_index(value: Option<&Value>) -> Result<Option<u64>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| String::from("Invalid Go behavior positional index")),
    }
}

fn assert_file(types: &[Type]) -> String {
    let helper = super::super::testing::unit_helper(types, super::super::TestAssert::GO);
    if helper.is_empty() {
        return String::new();
    }
    format!("\n{}{}\n{helper}", GoFile::MARKER, GoNames::ASSERT_FILE)
}

fn silent_test_cases(
    module: &Module,
    declaration: &Type,
    unit: &super::super::testing::Unit,
) -> Result<Vec<String>, String> {
    let code =
        crate::annotation_layout::masked_source(std::path::Path::new("unit.go"), &module.source)
            .map_err(|_| String::from("Cannot read the Go test unit"))?;
    let methods = crate::go_source::methods(&code);
    let mut silent = Vec::new();
    for method in crate::go_source::methods_of(&methods, &declaration.name) {
        if !unit.cases.contains(&method.name) {
            continue;
        }
        let results = compact(&code[method.results.clone()]);
        if !GoNames::TEST_RESULTS.contains(&results.as_str()) {
            return Err(format!(
                "Go test case {} must return nothing or error; check it with Assert, or return nil when it passes",
                method.name
            ));
        }
        if results.is_empty() {
            silent.push(method.name.clone());
        }
    }
    Ok(silent)
}

fn test_members(unit: &super::super::testing::Unit, silent: &[String]) -> String {
    let receiver = GoNames::RECEIVER;
    let name = &unit.name;
    let calls = super::super::ValueSource::calls(&unit.cases, &unit.inputs, &CallStyle::JSON);
    let mut dispatch = String::new();
    for (case, call) in &calls {
        let statement = case_statement(call, silent);
        let _ = write!(dispatch, "\n\tcase {}:\n\t\t{statement}", quoted(case));
    }
    let names: Vec<String> = calls.into_iter().map(|(case, _)| case).collect();
    let cleared_fakes = super::super::testing::joined(&unit.mocks, |fake| {
        format!("\t\tbehaviorClearFake(&{receiver}.{fake})\n")
    });
    format!(
        "\nfunc ({receiver} *{name}) startBehaviors() error {{\n\t{receiver}.stopBehaviors()\n\toptions := NewBehaviorOptions({}, NewBehaviorSignal())\n{}{}\tcontext, err := BehaviorsCreate[{}](options)\n\tif err != nil {{\n\t\treturn err\n\t}}\n\t{receiver}.behaviors = context\n\treturn nil\n}}\n\nfunc ({receiver} *{name}) stopBehaviors() {{\n\tif {receiver}.behaviors != nil {{\n\t\t{receiver}.behaviors.Dispose()\n\t\t{receiver}.behaviors = nil\n\t}}\n}}\n\nfunc ({receiver} *{name}) runCase(name string) (err error) {{\n\tdefer func() {{\n\t\tif recovered := recover(); recovered != nil {{\n\t\t\terr = behaviorAssertRecovered(recovered)\n\t\t}}\n\t}}()\n\tswitch name {{{dispatch}\n\t}}\n\treturn nil\n}}\n\nfunc ({receiver} *{name}) POST() any {{\n\tcases := []any{{}}\n\tfor _, name := range []string{{{}}} {{\n{cleared_fakes}\t\terr := {receiver}.startBehaviors()\n\t\tbehaviorCallsTake()\n\t\tif err == nil {{\n\t\t\terr = {receiver}.runCase(name)\n\t\t}}\n\t\tif err == nil {{\n\t\t\tcases = append(cases, map[string]any{{\"name\": name, \"passed\": true, \"called\": behaviorCallsTake()}})\n\t\t}} else {{\n\t\t\tcases = append(cases, map[string]any{{\"name\": name, \"passed\": false, \"error\": behaviorErrorMessage(err), \"called\": behaviorCallsTake()}})\n\t\t}}\n\t\t{receiver}.stopBehaviors()\n\t}}\n\treturn map[string]any{{\"unit\": {}, \"cases\": cases}}\n}}\n",
        quoted(&unit.partition),
        profile_statements(unit),
        property_statements(unit),
        unit.configuration,
        quoted_names(&names),
        quoted(name),
    )
}

fn case_statement(call: &str, silent: &[String]) -> String {
    let receiver = GoNames::RECEIVER;
    let returns_nothing = silent
        .iter()
        .any(|method| call.starts_with(&format!("{method}(")));
    if returns_nothing {
        return format!("{receiver}.{call}\n\t\treturn nil");
    }
    format!("return {receiver}.{call}")
}

fn profile_statements(unit: &super::super::testing::Unit) -> String {
    let Some(profiles) = unit.profiles.as_array() else {
        return String::new();
    };
    super::super::testing::joined(profiles.iter().filter_map(Value::as_str), |profile| {
        format!(
            "\toptions.Profiles = append(options.Profiles, {})\n",
            quoted(profile)
        )
    })
}

fn property_statements(unit: &super::super::testing::Unit) -> String {
    let Some(properties) = unit.properties.as_object() else {
        return String::new();
    };
    super::super::testing::joined(properties, |(key, value)| {
        format!(
            "\toptions.Properties[{}] = {}\n",
            quoted(key),
            quoted(&value.to_string())
        )
    })
}

fn quoted(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

fn quoted_names(names: &[String]) -> String {
    let mut quoted_names = Vec::new();
    for name in names {
        quoted_names.push(quoted(name));
    }
    quoted_names.join(", ")
}

fn quoted_strings(values: &[Value], error: &str) -> Result<String, String> {
    let mut quoted_values = Vec::new();
    for value in values {
        let Some(text) = value.as_str() else {
            return Err(String::from(error));
        };
        quoted_values.push(quoted(text));
    }
    Ok(quoted_values.join(", "))
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Invalid Go behavior string policy {field}"))
}

fn number(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("Invalid Go behavior integer policy {field}"))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::Path;

    fn lower_client(source: &str) -> String {
        super::super::lower(Path::new("server/remotes/ReportClient.go"), source, "yon")
            .expect("Go gateway lowering")
            .expect("lowered source")
    }

    #[test]
    fn a_gateway_method_calls_the_transport_without_a_behavior_context() {
        let lowered = lower_client(
            "type ReportClient struct{}\n\n// @Gateway({\"class\":\"server/services/ReportService.py\"})\nfunc (client *ReportClient) Summarize(mode, detail BehaviorValue) (BehaviorValue, error) {}\n",
        );
        assert!(
            lowered.contains("(BehaviorValue, error) { __tachyon_arguments, __tachyon_error := BehaviorsGatewayArguments([]BehaviorValue{mode, detail}); if __tachyon_error != nil { return nil, __tachyon_error }; return BehaviorsGatewayResult(YonGateway.Call(\"ReportClient.Summarize\", __tachyon_arguments)) }"),
            "{lowered}"
        );
        assert!(
            !lowered.contains("ReportClient) behaviorContext()"),
            "{lowered}"
        );
        assert!(
            lowered.contains("\n//tachyon:file behavior\npackage main\n"),
            "{lowered}"
        );
    }

    #[test]
    fn a_gateway_interface_becomes_a_struct_without_a_behavior_context() {
        let lowered = lower_client(
            "type ReportClient interface {\n\t// @Gateway({\"command\":[\"python3\"]})\n\tPing() (BehaviorValue, error)\n}\n",
        );
        assert!(
            lowered.contains(
                "type ReportClient struct{}; type behaviorDeclaredReportClient interface {"
            ),
            "{lowered}"
        );
        assert!(
            lowered.contains(
                "func (__tachyon_receiver *ReportClient) Ping() (BehaviorValue, error) {"
            ),
            "{lowered}"
        );
    }

    #[test]
    fn a_gateway_parameter_must_be_a_behavior_value() {
        let failure = super::super::lower(
            Path::new("server/remotes/ReportClient.go"),
            "type ReportClient struct{}\n\n// @Gateway({\"command\":[\"python3\"]})\nfunc (client *ReportClient) Ping(mode string) (BehaviorValue, error) {}\n",
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

    #[test]
    fn a_wrapped_method_keeps_its_line_numbers() {
        let source = "// @CacheSettings({\"name\":\"products\",\"ttlMs\":1000,\"maxBytes\":4096,\"maxEntries\":10})\ntype Catalog struct {\n\tbehaviors *BehaviorContext\n}\n\n// @Cached\nfunc (catalog *Catalog) List(page BehaviorValue) (BehaviorValue, error) {\n\treturn page, nil\n}\n";
        let lowered = super::super::lower(Path::new("server/services/Catalog.go"), source, "yon")
            .expect("Go lowering")
            .expect("lowered source");
        let (user_file, _) = lowered
            .split_once("//tachyon:file behavior")
            .expect("behavior file");
        assert_eq!(user_file.lines().count(), source.lines().count() + 1);
        assert!(user_file.contains("return catalog.behaviors.Invoke(BehaviorMethodPolicy{Owner: \"Catalog\", Name: \"List\""), "{user_file}");
        assert!(
            user_file
                .contains("page := __tachyon_arguments[0]; _ = page; \n\treturn page, nil\n})}"),
            "{user_file}"
        );
    }

    #[test]
    fn behavior_methods_need_go_results_and_a_named_receiver() {
        let service = "// @CacheSettings({\"name\":\"products\",\"ttlMs\":1000,\"maxBytes\":4096,\"maxEntries\":10})\ntype Catalog struct {\n\tbehaviors *BehaviorContext\n}\n\n// @Cached\nfunc (catalog *Catalog) List(page BehaviorValue) (BehaviorValue, error) { return page, nil }\n";
        for (broken, message) in [
            (
                service.replace("(BehaviorValue, error) {", "BehaviorValue {"),
                "must return (BehaviorValue, error)",
            ),
            (
                service.replace("(catalog *Catalog)", "(*Catalog)"),
                "needs a named receiver",
            ),
            (
                service.replace("\tbehaviors *BehaviorContext\n", ""),
                "requires a behaviors *BehaviorContext field",
            ),
        ] {
            let failure =
                super::super::lower(Path::new("server/services/Catalog.go"), &broken, "yon")
                    .expect_err(message);
            assert!(failure.to_string().contains(message), "{failure}");
        }
    }
}
