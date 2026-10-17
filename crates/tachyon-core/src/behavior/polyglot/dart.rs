use super::super::call_style::CallStyle;
use super::super::mapper;
use super::super::testing::{Unit, joined, polyglot_unit};
use super::string_literal::StringLiteral;
use super::{Edit, Method, Module, Parameter, Type, block, concrete_edit, declared_fields, render};
use serde_json::{Map, Value};
use std::fmt::Write as _;

pub(super) fn lower(module: &Module) -> Result<String, String> {
    let (runtime_head, runtime_body) = include_str!("dart_runtime.dart")
        .split_once("//__BEHAVIOR_BODY__\n")
        .ok_or("The Dart behavior runtime has no body marker")?;
    let mut edits = vec![Edit {
        range: 0..0,
        replacement: runtime_head.to_owned(),
    }];
    let mut registrations = Map::new();
    let mut configurations = String::new();
    let mut entities = String::new();
    let mut policies = String::new();
    for type_declaration in &module.types {
        edits.extend(concrete_edit(module, type_declaration));
        policies.push_str(&policy_line(type_declaration));
        if type_declaration.configuration {
            let _ = writeln!(
                configurations,
                "{}",
                configuration_arm(module, type_declaration)?
            );
        }
        if let Some(entity) = &type_declaration.entity {
            entities.push_str(&entity_line(type_declaration, entity));
        }
        if let Some(fields) = &type_declaration.constructor {
            edits.push(constructor_edit(type_declaration, fields)?);
        }
        if let Some(unit) = polyglot_unit(type_declaration) {
            edits.push(Edit {
                range: type_declaration.body.start..type_declaration.body.start,
                replacement: test_members(module, type_declaration, &unit)?,
            });
        }
        let methods = lower_methods(module, type_declaration, &mut edits)?;
        registrations.insert(
            type_declaration.name.clone(),
            serde_json::json!({"cache": type_declaration.cache, "methods": methods}),
        );
    }
    let registry = Value::Object(registrations).to_string();
    let generated = runtime_body
        .replace("__TAC_BEHAVIOR_MODULE__", &quote(&module.module_id))
        .replace("__TAC_BEHAVIOR_ENVIRONMENT__", &quote(&module.environment))
        .replace("__TAC_BEHAVIOR_REGISTRY__", &quote(&registry))
        .replace("__TAC_BEHAVIOR_CONFIGURATIONS__", &configurations)
        .replace("__TAC_BEHAVIOR_ENTITIES__", &entities)
        .replace("__TAC_BEHAVIOR_TYPES__", &policies);
    let helper = super::super::testing::unit_helper(&module.types, super::super::TestAssert::DART);
    render(module, edits, &format!("{generated}{helper}"))
}

fn policy_line(type_declaration: &Type) -> String {
    let policies = Value::Object(type_declaration.policies.clone()).to_string();
    format!(
        "    if (type == {}) return BehaviorJson.parse({});\n",
        type_declaration.name,
        quote(&policies)
    )
}

fn entity_line(type_declaration: &Type, entity: &Value) -> String {
    let mapping = serde_json::json!({
        "entity": type_declaration.name,
        "adapter": entity["adapter"],
        "idField": entity["id"],
        "table": type_declaration.table
    });
    format!(
        "    if (entity == {}) return BehaviorJson.parse({});\n",
        type_declaration.name,
        quote(&mapping.to_string())
    )
}

fn constructor_edit(type_declaration: &Type, fields: &[String]) -> Result<Edit, String> {
    for name in fields {
        let is_declared = type_declaration
            .fields
            .iter()
            .any(|field| field.name == *name);
        if !is_declared {
            return Err(format!(
                "Required constructor field '{name}' has no explicit Dart declaration"
            ));
        }
    }
    let mut parameters = Vec::new();
    for name in fields {
        parameters.push(format!("this.{name}"));
    }
    let parameters = parameters.join(", ");
    let declarations = declared_fields(type_declaration, |field| {
        format!("\n  final {} {};", field.type_source, field.name)
    });
    Ok(Edit {
        range: type_declaration.body.start..type_declaration.body.start,
        replacement: format!(
            "{declarations}\n  {}({parameters});\n",
            type_declaration.name
        ),
    })
}

fn configuration_arm(module: &Module, type_declaration: &Type) -> Result<String, String> {
    let configure = type_declaration
        .methods
        .iter()
        .find(|method| method.name == "configure")
        .ok_or(
            "Dart Setup requires static BehaviorAdapters configure(BehaviorExecution execution)",
        )?;
    let signature = &module.source[configure.signature.clone()];
    let is_valid = configure.is_static
        && !configure.asynchronous
        && signature.contains("BehaviorAdapters")
        && configure.parameters.len() == 1;
    if !is_valid {
        return Err(
            "Dart configure must be static BehaviorAdapters configure(BehaviorExecution execution), synchronous"
                .to_owned(),
        );
    }
    Ok(format!(
        "    if (configuration == {}) return {}.configure(execution);",
        type_declaration.name, type_declaration.name
    ))
}

fn future_value_type(return_type: &str) -> &str {
    let trimmed = return_type.trim();
    let Some(inner) = trimmed.strip_prefix("Future<") else {
        return trimmed;
    };
    let Some(value_type) = inner.strip_suffix('>') else {
        return trimmed;
    };
    value_type.trim()
}

fn cast_suffix(type_name: &str) -> String {
    if matches!(type_name, "Object?" | "dynamic" | "") {
        String::new()
    } else {
        format!(" as {type_name}")
    }
}

fn json_arguments(method: &Method) -> String {
    let mut arguments = Vec::new();
    for parameter in &method.parameters {
        arguments.push(format!("BehaviorJson.from({})", parameter.name));
    }
    arguments.join(", ")
}

fn lower_methods(
    module: &Module,
    type_declaration: &Type,
    edits: &mut Vec<Edit>,
) -> Result<Map<String, Value>, String> {
    let mut methods = Map::new();
    for method in &type_declaration.methods {
        if method.metadata.is_empty() {
            continue;
        }
        let replacement = match written_body(method)? {
            Some(replacement) => replacement,
            None => generated_body(module, type_declaration, method)?,
        };
        edits.push(Edit {
            range: method.body.clone(),
            replacement: block(method, &replacement),
        });
        methods.insert(method.name.clone(), Value::Object(method.metadata.clone()));
    }
    Ok(methods)
}

fn generated_body(
    module: &Module,
    type_declaration: &Type,
    method: &Method,
) -> Result<String, String> {
    let mut prefix = validation_prefix(method)?;
    if let Some(policy) = method.metadata.get("pageable") {
        prefix.push_str(&pageable_line(method, policy)?);
    }
    let body = &module.source[method.body.clone()];
    if is_validation_only(method) {
        return Ok(format!("\n{prefix}{body}\n"));
    }
    if uses_invocation(method) {
        return invocation_body(type_declaration, method, &prefix, body);
    }
    Ok(format!(
        "\n{prefix}    return behaviors.paginate(() {{ {body} }});\n"
    ))
}

fn is_validation_only(method: &Method) -> bool {
    method.metadata.len() == 1 && method.metadata.contains_key("valid")
}

fn uses_invocation(method: &Method) -> bool {
    method.metadata.contains_key("cacheable")
        || method.metadata.contains_key("httpExchange")
        || method.asynchronous
        || method.metadata.contains_key("cacheEvict")
        || method.metadata.contains_key("cachePut")
}

fn is_declared_exchange(method: &Method) -> bool {
    let Some(exchange) = method.metadata.get("httpExchange") else {
        return false;
    };
    exchange.get("generated").is_some()
}

fn returns_future(method: &Method) -> bool {
    method.asynchronous && method.return_type.trim().starts_with("Future<")
}

fn invocation_body(
    type_declaration: &Type,
    method: &Method,
    prefix: &str,
    body: &str,
) -> Result<String, String> {
    if !returns_future(method) {
        return Err("Dart asynchronous behaviors must be async and return a Future".to_owned());
    }
    if is_declared_exchange(method) {
        let value_type = future_value_type(&method.return_type);
        let shape = crate::route_types::TypeText::shape("dart", value_type)?;
        let decoded = crate::route_types::DartValues::read(&shape, "value");
        return Ok(format!(
            "\n{prefix}    return BehaviorBrowserHttp.decode(await behaviors.invoke({}, {}, [{}], () async {{ return null; }}), (value) => {decoded});\n",
            quote(&type_declaration.name),
            quote(&method.name),
            json_arguments(method),
        ));
    }
    Ok(format!(
        "\n{prefix}    return (await behaviors.invoke({}, {}, [{}], () async {{ {body} }})){};\n",
        quote(&type_declaration.name),
        quote(&method.name),
        json_arguments(method),
        cast_suffix(future_value_type(&method.return_type))
    ))
}

fn pageable_line(method: &Method, policy: &Value) -> Result<String, String> {
    let parameter = parameter_at(method, policy, "Pageable")?;
    Ok(format!(
        "    {} = BehaviorPage.normalize({}, BehaviorJson.parse({})){};\n",
        parameter.name,
        parameter.name,
        quote(&policy.to_string()),
        cast_suffix(parameter.type_source.trim())
    ))
}

fn parameter_at<'a>(
    method: &'a Method,
    policy: &Value,
    annotation_name: &str,
) -> Result<&'a Parameter, String> {
    let Some(position) = policy["argument"].as_u64() else {
        return Err(format!("Missing {annotation_name} argument"));
    };
    let out_of_range = format!("{annotation_name} argument is out of range");
    let Ok(index) = usize::try_from(position) else {
        return Err(out_of_range);
    };
    method.parameters.get(index).ok_or(out_of_range)
}

fn validation_prefix(method: &Method) -> Result<String, String> {
    let Some(policy) = method.metadata.get("valid") else {
        return Ok(String::new());
    };
    let parameter = parameter_at(method, policy, "Check")?;
    Ok(format!(
        "    {} = Behaviors.validateSchema(BehaviorJson.parse({}), {}){};\n",
        parameter.name,
        quote(&policy["schema"].to_string()),
        parameter.name,
        cast_suffix(parameter.type_source.trim())
    ))
}

fn written_body(method: &Method) -> Result<Option<String>, String> {
    if let Some(policy) = method.metadata.get("gateway") {
        return Ok(Some(gateway_body(method, policy)));
    }
    if let Some(query) = method.metadata.get("query") {
        return query_body(method, query).map(Some);
    }
    if let Some(policy) = method.metadata.get("mapping") {
        return mapping_body(method, policy).map(Some);
    }
    Ok(None)
}

fn gateway_body(method: &Method, policy: &Value) -> String {
    let key = policy["key"].as_str().unwrap_or_default();
    let mut names = Vec::new();
    for parameter in &method.parameters {
        names.push(parameter.name.as_str());
    }
    format!(
        "\n    return Behaviors.gatewayCall({}, [{}], YonGateway.call);\n",
        quote(key),
        names.join(", ")
    )
}

fn query_body(method: &Method, query: &Value) -> Result<String, String> {
    if !returns_future(method) {
        return Err("Dart query methods must be async and return a Future".to_owned());
    }
    Ok(format!(
        "\n    return (await behaviors.query(BehaviorJson.parse({}), [{}])){};\n",
        quote(&query.to_string()),
        json_arguments(method),
        cast_suffix(future_value_type(&method.return_type))
    ))
}

fn mapping_body(method: &Method, policy: &Value) -> Result<String, String> {
    let (argument, _) = mapper::plan(policy)?;
    let value = method
        .parameters
        .get(argument)
        .ok_or("Convert reads an argument the method does not declare")?;
    if let Some(result) = policy.get("result").and_then(Value::as_str) {
        return Ok(format!(
            "\n    return tachyonReadModel{result}(BehaviorContext.mapValue(BehaviorJson.parse({}), BehaviorJson.from(tachyonPlainModel({}))));\n",
            quote(&policy.to_string()),
            value.name
        ));
    }
    let mapped = format!(
        "BehaviorContext.mapValue(BehaviorJson.parse({}), BehaviorJson.from({}))",
        quote(&policy.to_string()),
        value.name
    );
    Ok(format!(
        "\n    return {mapped}{};\n",
        cast_suffix(future_value_type(&method.return_type))
    ))
}

fn quote(value: &str) -> String {
    StringLiteral::DART.of(value)
}

fn test_members(module: &Module, owner: &Type, unit: &Unit) -> Result<String, String> {
    let calls = super::super::ValueSource::calls(&unit.cases, &unit.inputs, &CallStyle::DART);
    let names = joined(&calls, |(name, _)| format!("{}, ", quote(name)));
    let dispatch = joined(&calls, |(name, call)| {
        format!("      case {}:\n        await {call};\n", quote(name))
    });
    let profiles = profile_list(unit);
    let properties = property_list(unit);
    let construction =
        super::dart_test_construction::DartTestConstruction::members(module, owner, unit);
    let cleared_fakes = super::typed_fake_reset::TypedFakeReset::DART.statements(owner, unit)?;
    Ok(format!(
        "\n  BehaviorContext? behaviors;\n{construction}\
         \n  void startBehaviors() {{\n    stopBehaviors();\n    \
         behaviors = Behaviors.create({}, partition: {}, signal: BehaviorSignal(), profiles: [{profiles}], properties: {{{properties}}});\n  }}\n\
         \n  void stopBehaviors() {{\n    behaviors?.dispose();\n    behaviors = null;\n  }}\n\
         \n  Future<void> runCase(String name) async {{\n    switch (name) {{\n{dispatch}    }}\n  }}\n\
         \n  Future<Object?> POST() async {{\n    final cases = <Object?>[];\n    \
         for (final name in [{names}]) {{\n      try {{\n{cleared_fakes}        startBehaviors();\n        BehaviorCalls.take();\n        await runCase(name);\n        \
         cases.add({{'name': name, 'passed': true, 'called': BehaviorCalls.take()}});\n      }} on BehaviorError catch (error) {{\n        \
         cases.add({{'name': name, 'passed': false, 'error': error.code, 'called': BehaviorCalls.take()}});\n      }} catch (error) {{\n        \
         cases.add({{'name': name, 'passed': false, 'error': '$error', 'called': BehaviorCalls.take()}});\n      }}\n      stopBehaviors();\n    }}\n    \
         return {{'unit': {}, 'cases': cases}};\n  }}\n",
        unit.configuration,
        quote(&unit.partition),
        quote(&unit.name),
    ))
}

fn profile_list(unit: &Unit) -> String {
    let mut profiles = String::new();
    let Some(values) = unit.profiles.as_array() else {
        return profiles;
    };
    for value in values {
        if let Some(profile) = value.as_str() {
            let _ = write!(profiles, "{}, ", quote(profile));
        }
    }
    profiles
}

fn property_list(unit: &Unit) -> String {
    let mut properties = String::new();
    let Some(entries) = unit.properties.as_object() else {
        return properties;
    };
    for (key, value) in entries {
        let _ = write!(
            properties,
            "{}: {}, ",
            quote(key),
            quote(&value.to_string())
        );
    }
    properties
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    fn emit(fixture: &str) -> Option<String> {
        if std::process::Command::new("dart")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping: dart is unavailable");
            return None;
        }
        let emitted = super::super::lower(
            std::path::Path::new("server/services/AnnotatedService.dart"),
            fixture,
            "yon",
        )
        .expect("Dart annotation lowering")
        .expect("annotated source");
        Some(emitted)
    }

    async fn run_emitted(emitted: &str, success: &str) {
        let directory = tempfile::tempdir().expect("emitted source directory");
        let path = directory.path().join("emitted.dart");
        std::fs::write(&path, emitted).expect("emitted Dart source");
        let mut command = tokio::process::Command::new("dart");
        command.arg("run").arg(&path);
        let result = crate::external_command::run(
            &mut command,
            std::time::Duration::from_mins(4),
            1024 * 1024,
        )
        .await
        .expect("Dart run");
        assert!(
            result.status.success() && String::from_utf8_lossy(&result.stdout).contains(success),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[tokio::test]
    async fn dart_configuration_and_validation_compile_and_execute() {
        let Some(emitted) = emit(include_str!(
            "../../../tests/fixtures/behavior/dart-configuration.dart"
        )) else {
            return;
        };
        run_emitted(
            &emitted,
            "Dart emitted configuration binding and validation executed",
        )
        .await;
    }

    #[tokio::test]
    async fn dart_all_annotations_compile_and_execute() {
        let Some(emitted) = emit(include_str!(
            "../../../tests/fixtures/behavior/dart-annotations.dart"
        )) else {
            return;
        };
        assert!(
            emitted.contains("AnnotatedCatalog(this.behaviors);"),
            "{emitted}"
        );
        run_emitted(&emitted, "Dart emitted annotations").await;
    }

    fn lower_client(source: &str) -> String {
        super::super::lower(
            std::path::Path::new("server/remotes/ReportClient.dart"),
            source,
            "yon",
        )
        .expect("Dart gateway lowering")
        .expect("lowered source")
    }

    #[test]
    fn browser_http_models_use_checked_decoding() {
        let path = std::path::Path::new("client/remotes/GreetingClient.dart");
        let concrete = crate::declared_contract::DeclaredContract::concrete(
            path,
            "@HTTPClient\nabstract interface class GreetingClient {\n  @GetRequest(\"/api/greeting\")\n  Future<Greeting> read();\n}\n",
            "tac",
        ).expect("concrete contract");
        let rewritten = crate::behavior::HttpClientSource::rewrite(path, &concrete)
            .expect("HTTP rewrite")
            .expect("rewritten source");
        let lowered = lower_client(&rewritten);
        assert!(
            lowered.contains("BehaviorBrowserHttp.decode(await behaviors.invoke("),
            "{lowered}"
        );
        assert!(lowered.contains("__yon_read_Greeting(value)"), "{lowered}");
        assert!(!lowered.contains("as Greeting"), "{lowered}");
    }

    #[tokio::test]
    async fn browser_http_runtime_preserves_typed_errors_and_abort_listeners() {
        let source = include_str!("dart_runtime.dart")
            .replace("__TAC_BEHAVIOR_MODULE__", "'test'")
            .replace("__TAC_BEHAVIOR_ENVIRONMENT__", "'tac'")
            .replace("__TAC_BEHAVIOR_REGISTRY__", "'{}'")
            .replace("__TAC_BEHAVIOR_ENTITIES__", "")
            .replace("__TAC_BEHAVIOR_TYPES__", "")
            .replace("__TAC_BEHAVIOR_CONFIGURATIONS__", "");
        let main = r#"
Future<void> main() async {
  final owner = BehaviorSignal();
  final signal = BehaviorSignal._child(owner);
  final execution = BehaviorExecution('tac', 'test', signal, owner, Behaviors.now + 1000);
  BehaviorBrowserHttp.host = (request, signal) async => '{"value":{"message":"typed"}}';
  final answer = await BehaviorBrowserHttp.adapter!.exchange({}, execution);
  if (BehaviorJson.get(answer, 'message') != 'typed') throw StateError('answer');
  BehaviorBrowserHttp.host = (request, signal) async => '{"error":{"code":"HTTP_STATUS"}}';
  try { await BehaviorBrowserHttp.adapter!.exchange({}, execution); throw StateError('accepted'); }
  on BehaviorError catch (error) { if (error.code != 'HTTP_STATUS') rethrow; }
  try { BehaviorBrowserHttp.decode(42, (value) => value as String); throw StateError('accepted'); }
  on BehaviorError catch (error) { if (error.code != 'HTTP_RESPONSE') rethrow; }
  var stopped = 0;
  final remove = signal.watch(() => stopped += 1);
  remove();
  owner.abort();
  if (stopped != 0 || !signal.aborted) throw StateError('removed listener');
  signal.watch(() => stopped += 1);
  if (stopped != 1) throw StateError('already aborted');
  final scope = BehaviorSignal();
  final context = await BehaviorBrowserHttp.withOwner(scope, () async {
    await Future<void>.delayed(Duration.zero);
    return BehaviorContext._(BehaviorAdapters(), 'test', BehaviorSignal(), 1000, 'test');
  });
  if (BehaviorBrowserHttp.owner != null) throw StateError('scope leaked');
  scope.abort();
  if (!context._signal.aborted || !context._released) throw StateError('scope disposal');
  print('Dart browser HTTP runtime passed');
}
"#;
        run_emitted(&(source + main), "Dart browser HTTP runtime passed").await;
    }

    #[test]
    fn a_gateway_method_calls_the_transport_without_a_behavior_context() {
        let lowered = lower_client(
            "class ReportClient {\n  @Gateway({\"class\":\"server/services/ReportService.py\"})\n  Future<Object?> summarize(Object? mode, Object? detail) async {}\n}\n",
        );
        assert!(lowered.contains(
            "Future<Object?> summarize(Object? mode, Object? detail) async {\n    return Behaviors.gatewayCall('ReportClient.summarize', [mode, detail], YonGateway.call);\n}"
        ));
        assert!(!lowered.contains("BehaviorContext behaviors"));
    }

    #[test]
    fn a_gateway_abstract_class_becomes_a_class_without_a_behavior_context() {
        let lowered = lower_client(
            "abstract class ReportClient {\n  @Gateway({\"command\":[\"python3\"]})\n  Future<Object?> ping();\n}\n",
        );
        assert!(lowered.contains("class ReportClient {"));
        assert!(!lowered.contains("abstract class ReportClient"));
        assert!(lowered.contains(
            "Future<Object?> ping() { \n    return Behaviors.gatewayCall('ReportClient.ping', [], YonGateway.call);\n }"
        ));
        assert!(!lowered.contains("final BehaviorContext behaviors"));
    }

    #[test]
    fn an_abstract_interface_class_converter_becomes_a_class_without_a_context() {
        let lowered = super::super::lower(
            std::path::Path::new("server/converters/ProductConverter.dart"),
            "@Converter\nabstract interface class ProductConverter {\n  @Convert({\"fields\":[{\"source\":\"name\",\"target\":\"title\"}]})\n  Object? toSummary(Object? product);\n}\n",
            "yon",
        )
        .expect("Dart converter lowering")
        .expect("lowered source");
        assert!(lowered.contains("class ProductConverter {"), "{lowered}");
        assert!(!lowered.contains("interface class"), "{lowered}");
        assert!(!lowered.contains("behaviors"), "{lowered}");
        assert!(
            lowered.contains("BehaviorContext.mapValue(BehaviorJson.parse("),
            "{lowered}"
        );
    }

    #[test]
    fn a_gateway_result_must_be_a_future_object() {
        let failure = super::super::lower(
            std::path::Path::new("server/remotes/ReportClient.dart"),
            "class ReportClient {\n  @Gateway({\"command\":[\"python3\"]})\n  Future<String> ping(Object? mode) async {}\n}\n",
            "yon",
        )
        .expect_err("typed result");
        let message = failure.to_string();
        assert!(message.contains("TY1312"), "{message}");
        assert!(message.contains("return Future<Object?>"), "{message}");
    }
}
