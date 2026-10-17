use super::super::mapper;
use super::super::testing::{Unit, joined, polyglot_unit};
use super::{Edit, Method, Module, Parameter, Type, block, concrete_edit, declared_fields, render};
use serde_json::{Map, Value};

pub(super) fn lower(module: &Module) -> Result<String, String> {
    let mut edits = Vec::new();
    let mut registrations = Map::new();
    let mut configurations = String::new();
    let mut entities = String::new();
    let mut policies = String::new();
    for type_declaration in &module.types {
        edits.extend(concrete_edit(module, type_declaration));
        policies.push_str(&policy_line(type_declaration));
        if type_declaration.configuration {
            configurations.push_str(&configuration_line(type_declaration));
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
                replacement: format!(
                    "{}{}",
                    test_members(&unit, &module.environment),
                    super::typed_test_construction::TypedTestConstruction::members(
                        module,
                        type_declaration,
                        &unit,
                        "cs",
                    ),
                ),
            });
        }
        let methods = lower_methods(module, type_declaration, &mut edits)?;
        registrations.insert(
            type_declaration.name.clone(),
            serde_json::json!({"cache": type_declaration.cache, "methods": methods}),
        );
    }
    let registry = Value::Object(registrations).to_string();
    let mut generated = runtime(
        &module.module_id,
        &module.environment,
        &registry,
        &configurations,
        &entities,
        &policies,
    );
    generated.push_str(super::super::testing::unit_helper(
        &module.types,
        super::super::TestAssert::CSHARP,
    ));
    render(module, edits, &generated)
}

pub(super) fn runtime(
    module_id: &str,
    environment: &str,
    registry: &str,
    configurations: &str,
    entities: &str,
    policies: &str,
) -> String {
    concat!(
        include_str!("csharp_runtime.cs"),
        include_str!("csharp_browser_http/BehaviorBrowserHttp.cs"),
        include_str!("csharp_browser_http/BehaviorBrowserHttpAdapter.cs"),
        include_str!("csharp_browser_http/BehaviorHttpRevalidation.cs")
    )
    .replace("__TAC_BEHAVIOR_MODULE__", &quote(module_id))
    .replace("__TAC_BEHAVIOR_ENVIRONMENT__", &quote(environment))
    .replace("__TAC_BEHAVIOR_REGISTRY__", &quote(registry))
    .replace("__TAC_BEHAVIOR_CONFIGURATIONS__", configurations)
    .replace("__TAC_BEHAVIOR_ENTITIES__", entities)
    .replace("__TAC_BEHAVIOR_TYPES__", policies)
}

fn policy_line(type_declaration: &Type) -> String {
    let policies = Value::Object(type_declaration.policies.clone()).to_string();
    format!(
        "if (type == typeof({})) return BehaviorJSON.Parse({});\n",
        type_declaration.name,
        quote(&policies)
    )
}

pub(super) fn configuration_line(type_declaration: &Type) -> String {
    format!(
        "if (configuration == typeof({})) return {}.configure(execution);\n",
        type_declaration.name, type_declaration.name
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
        "if (entity == typeof({})) return BehaviorJSON.Parse({});\n",
        type_declaration.name,
        quote(&mapping.to_string())
    )
}

fn constructor_edit(type_declaration: &Type, fields: &[String]) -> Result<Edit, String> {
    let parameters = constructor_parameters(type_declaration, fields)?;
    let mut assignments = Vec::new();
    for name in fields {
        assignments.push(format!("this.{name} = {name};"));
    }
    let assignments = assignments.join(" ");
    let declarations = declared_fields(type_declaration, |field| {
        format!("\nreadonly {} {};", field.type_source, field.name)
    });
    Ok(Edit {
        range: type_declaration.body.start..type_declaration.body.start,
        replacement: format!(
            "{declarations}\npublic {}({parameters}) {{ {assignments} }}\n",
            type_declaration.name
        ),
    })
}

fn constructor_parameters(type_declaration: &Type, fields: &[String]) -> Result<String, String> {
    let mut parameters = Vec::new();
    for name in fields {
        let Some(field) = type_declaration
            .fields
            .iter()
            .find(|field| field.name == *name)
        else {
            return Err(format!(
                "Required constructor field '{name}' has no explicit C# type"
            ));
        };
        parameters.push(format!("{} {name}", field.type_source));
    }
    Ok(parameters.join(", "))
}

fn lower_methods(
    module: &Module,
    type_declaration: &Type,
    edits: &mut Vec<Edit>,
) -> Result<Map<String, Value>, String> {
    let mut methods = Map::new();
    edits.extend(public_members(type_declaration));
    for method in &type_declaration.methods {
        if method.metadata.is_empty() {
            continue;
        }
        if let Some(replacement) = written_body(method)? {
            edits.push(Edit {
                range: method.body.clone(),
                replacement: block(method, &replacement),
            });
            continue;
        }
        let mut prefix = validation_prefix(method)?;
        if let Some(policy) = method.metadata.get("pageable") {
            prefix.push_str(&pageable_line(method, policy)?);
        }
        let body = &module.source[method.body.clone()];
        let replacement = method_body(type_declaration, method, &prefix, body)?;
        edits.push(Edit {
            range: method.body.clone(),
            replacement: block(method, &replacement),
        });
        methods.insert(method.name.clone(), Value::Object(method.metadata.clone()));
    }
    Ok(methods)
}

fn method_body(
    type_declaration: &Type,
    method: &Method,
    prefix: &str,
    body: &str,
) -> Result<String, String> {
    if let Some(body) =
        super::csharp_model_exchange::CsharpModelExchange::body(type_declaration, method, prefix)
    {
        return Ok(body);
    }
    if is_validation_only(method) {
        return validation_only_body(method, prefix, body);
    }
    if is_declared_exchange(method) {
        return declared_exchange(type_declaration, method, prefix);
    }
    if uses_invocation(method) {
        return invocation_body(type_declaration, method, prefix, body);
    }
    if method.return_type.trim() == "void" {
        return Err("C# Pageable methods must return a value".to_owned());
    }
    Ok(format!(
        "\n{prefix}return this.behaviors.Paginate(() => {{ {body} }});\n"
    ))
}

fn is_validation_only(method: &Method) -> bool {
    method.metadata.len() == 1 && method.metadata.contains_key("valid")
}

fn is_declared_exchange(method: &Method) -> bool {
    let Some(exchange) = method.metadata.get("httpExchange") else {
        return false;
    };
    exchange.get("generated").is_some()
}

fn uses_invocation(method: &Method) -> bool {
    method.metadata.contains_key("cacheable")
        || method.metadata.contains_key("httpExchange")
        || method.asynchronous
        || method.metadata.contains_key("cacheEvict")
        || method.metadata.contains_key("cachePut")
}

fn validation_only_body(method: &Method, prefix: &str, body: &str) -> Result<String, String> {
    if !returns_json_or_task_json(method) {
        return Err("C# Check methods require BehaviorJSON or async Task<BehaviorJSON>".to_owned());
    }
    Ok(format!("\n{prefix}{body}\n"))
}

fn invocation_body(
    type_declaration: &Type,
    method: &Method,
    prefix: &str,
    body: &str,
) -> Result<String, String> {
    if !method.asynchronous || !is_task_json(&method.return_type) {
        return Err("C# asynchronous behavior methods require async Task<BehaviorJSON>".to_owned());
    }
    let arguments = json_arguments(method);
    Ok(format!(
        "\n{prefix}return await this.behaviors.Invoke({}, {}, new BehaviorJSON[] {{ {arguments} }}, async () => {{ {body} }});\n",
        quote(&type_declaration.name),
        quote(&method.name)
    ))
}

fn pageable_line(method: &Method, policy: &Value) -> Result<String, String> {
    let parameter = parameter_at(method, policy, "Pageable")?;
    if !is_json(&parameter.type_source) {
        return Err("C# Pageable parameters must have type BehaviorJSON".to_owned());
    }
    Ok(format!(
        "{} = BehaviorPage.Normalize({}, BehaviorJSON.Parse({}));\n",
        parameter.name,
        parameter.name,
        quote(&policy.to_string())
    ))
}

fn validation_prefix(method: &Method) -> Result<String, String> {
    let Some(policy) = method.metadata.get("valid") else {
        return Ok(String::new());
    };
    let parameter = parameter_at(method, policy, "Check")?;
    if !is_json(&parameter.type_source) {
        return Err("C# Check parameters require BehaviorJSON".to_owned());
    }
    Ok(format!(
        "{} = Behaviors.ValidateSchema(BehaviorJSON.Parse({}), {});\n",
        parameter.name,
        quote(&policy["schema"].to_string()),
        parameter.name
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

fn declared_exchange(
    type_declaration: &Type,
    method: &Method,
    prefix: &str,
) -> Result<String, String> {
    if !method.asynchronous || !is_task_json(&method.return_type) {
        return Err("C# declared WebCall methods require async Task<BehaviorJSON>".to_owned());
    }
    let arguments = json_arguments(method);
    Ok(format!(
        "\n{prefix}return await this.behaviors.Invoke({}, {}, new BehaviorJSON[] {{ {arguments} }}, () => System.Threading.Tasks.Task.FromResult(BehaviorJSON.Parse(\"null\")));\n",
        quote(&type_declaration.name),
        quote(&method.name)
    ))
}

fn public_members(type_declaration: &Type) -> impl Iterator<Item = Edit> + '_ {
    type_declaration
        .methods
        .iter()
        .filter(|method| method.declared)
        .map(|method| Edit {
            range: method.signature.start..method.signature.start,
            replacement: "public ".to_owned(),
        })
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
        "\nreturn Behaviors.GatewayResult(YonGateway.Call({}, Behaviors.GatewayArguments(new BehaviorJSON[] {{ {} }})));\n",
        quote(key),
        names.join(", ")
    )
}

fn mapping_body(method: &Method, policy: &Value) -> Result<String, String> {
    let result = policy.get("result").and_then(Value::as_str);
    if result.is_none() && !returns_json_or_task_json(method) {
        return Err(
            "C# mapping methods require BehaviorJSON or async Task<BehaviorJSON>".to_owned(),
        );
    }
    let (argument, _) = mapper::plan(policy)?;
    let value = method
        .parameters
        .get(argument)
        .ok_or("Convert reads an argument the method does not declare")?;
    let plan = quote(&policy.to_string());
    let Some(result) = result else {
        return Ok(format!(
            "\nreturn BehaviorContext.MapValue(BehaviorJSON.Parse({plan}), BehaviorJSON.From({}));\n",
            value.name
        ));
    };
    if policy["realm"] == "tac" {
        return Ok(format!(
            "\nreturn TacModels.Read<{result}>(Tac.ParseValue(BehaviorContext.MapValue(BehaviorJSON.Parse({plan}), BehaviorJSON.Parse(Tac.EncodeValue(TacModels.Plain({})))).ToString()));\n",
            value.name
        ));
    }
    Ok(format!(
        "\nreturn System.Text.Json.JsonSerializer.Deserialize<{result}>(BehaviorContext.MapValue(BehaviorJSON.Parse({plan}), BehaviorJSON.Parse(System.Text.Json.JsonSerializer.Serialize({}))).ToString(), YonModels.Options)!;\n",
        value.name
    ))
}

fn query_body(method: &Method, query: &Value) -> Result<String, String> {
    if !method.asynchronous || !is_task_json(&method.return_type) {
        return Err("C# query methods require async Task<BehaviorJSON>".to_owned());
    }
    let arguments = json_arguments(method);
    Ok(format!(
        "\nreturn await this.behaviors.Query(BehaviorJSON.Parse({}), new BehaviorJSON[] {{ {arguments} }});\n",
        quote(&query.to_string())
    ))
}

pub(super) fn json_arguments(method: &Method) -> String {
    let mut arguments = Vec::new();
    for parameter in &method.parameters {
        arguments.push(format!("BehaviorJSON.From({})", parameter.name));
    }
    arguments.join(", ")
}

fn quote(value: &str) -> String {
    Value::String(value.to_owned()).to_string()
}

fn is_json(value: &str) -> bool {
    value.trim() == "BehaviorJSON"
}

fn is_task_json(value: &str) -> bool {
    let without_spaces = value.split_whitespace().collect::<String>();
    matches!(
        without_spaces.as_str(),
        "Task<BehaviorJSON>" | "System.Threading.Tasks.Task<BehaviorJSON>"
    )
}

fn returns_json_or_task_json(method: &Method) -> bool {
    is_json(&method.return_type) || (method.asynchronous && is_task_json(&method.return_type))
}

fn test_members(unit: &Unit, environment: &str) -> String {
    let (report, answer) = if environment == "tac" {
        ("string", "report.ToString()")
    } else {
        (
            "object",
            "System.Text.Json.Nodes.JsonNode.Parse(report.ToString())",
        )
    };
    let calls = super::super::ValueSource::csharp_calls(&unit.cases, &unit.inputs);
    let names = case_names(&calls);
    let dispatch = joined(&calls, |(name, call)| {
        format!("case {}: {call}; return;", quote(name))
    });
    let context = test_context(unit);
    let cleared_fakes = joined(&unit.mocks, |name| format!("this.{name} = default;\n"));
    format!(
        "\npublic BehaviorContext behaviors;\n\
         public void startBehaviors() {{ stopBehaviors(); this.behaviors = {context}; }}\n\
         public void stopBehaviors() {{ if (this.behaviors != null) {{ this.behaviors.Dispose(); this.behaviors = null; }} }}\n\
         void runCase(string name) {{ switch (name) {{ {dispatch} default: return; }} }}\n\
         public {report} POST() {{\n\
         var cases = new System.Collections.Generic.List<BehaviorJSON>();\n\
         foreach (var name in new string[] {{{names}}}) {{\n\
         {cleared_fakes}\
         startBehaviors();\n\
         BehaviorCalls.TakeJson();\n\
         try {{ runCase(name); cases.Add(BehaviorJSON.Parse(\"{{\\\"name\\\":\" + Quoted(name) + \",\\\"passed\\\":true,\\\"called\\\":\" + BehaviorCalls.TakeJson() + \"}}\")); }}\n\
         catch (System.Exception error) {{ cases.Add(BehaviorJSON.Parse(\"{{\\\"name\\\":\" + Quoted(name) + \",\\\"passed\\\":false,\\\"error\\\":\" + Quoted(error.Message ?? \"failed\") + \",\\\"called\\\":\" + BehaviorCalls.TakeJson() + \"}}\")); }}\n\
         finally {{ stopBehaviors(); }}\n\
         }}\n\
         var report = BehaviorJSON.Object(new System.Collections.Generic.Dictionary<string, BehaviorJSON> {{\n\
         [\"unit\"] = BehaviorJSON.From({}), [\"cases\"] = BehaviorJSON.Array(cases.ToArray()),\n\
         }});\n\
         return {answer};\n\
         }}\n\
         static string Quoted(string value) => BehaviorJSON.Quote(value);\n",
        quote(&unit.name),
    )
}

pub(super) fn test_context(unit: &Unit) -> String {
    format!(
        "Behaviors.create(typeof({}), {}, new BehaviorSignal(), 10000, new string[] {{{}}}, \
         new System.Collections.Generic.Dictionary<string, string> {{ {} }})",
        unit.configuration,
        quote(&unit.partition),
        profile_list(unit),
        property_entries(unit),
    )
}

fn case_names(calls: &[(String, String)]) -> String {
    let mut names = Vec::new();
    for (name, _) in calls {
        names.push(quote(name));
    }
    names.join(", ")
}

fn profile_list(unit: &Unit) -> String {
    let Some(values) = unit.profiles.as_array() else {
        return String::new();
    };
    let mut profiles = Vec::new();
    for value in values {
        if let Some(profile) = value.as_str() {
            profiles.push(quote(profile));
        }
    }
    profiles.join(", ")
}

fn property_entries(unit: &Unit) -> String {
    let Some(entries) = unit.properties.as_object() else {
        return String::new();
    };
    joined(entries, |(key, value)| {
        format!("[{}] = {}, ", quote(key), quote(&value.to_string()))
    })
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    fn dotnet_is_available() -> bool {
        std::process::Command::new("dotnet")
            .arg("--version")
            .output()
            .is_ok()
    }

    fn annotated_source() -> String {
        let annotations = include_str!("../../../tests/fixtures/behavior/csharp-annotations.cs")
            .replace(
                "using var signal =",
                "ConfigurationAnnotationTests.Run(); await ValidationAnnotationTests.Run(); using var signal =",
            );
        format!(
            "{}\n{}\n{}",
            annotations,
            include_str!("../../../tests/fixtures/behavior/csharp-configuration.cs"),
            include_str!("../../../tests/fixtures/behavior/csharp-validation.cs")
        )
    }

    async fn compile_and_execute(emitted: String) {
        let directory = tempfile::tempdir().expect("emitted source directory");
        let path = directory.path().join("Emitted.cs");
        std::fs::write(&path, emitted).expect("emitted C# source");
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/behavior-native-runtime-test.mjs");
        let mut command = tokio::process::Command::new("node");
        command.arg(script).arg("cs").arg("--emitted").arg(path);
        let result = crate::external_command::run(
            &mut command,
            std::time::Duration::from_mins(4),
            1024 * 1024,
        )
        .await
        .expect("C# compile and execution tool");
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    fn lower_client(source: &str) -> String {
        super::super::lower(
            std::path::Path::new("server/remotes/ReportClient.cs"),
            source,
            "yon",
        )
        .expect("C# gateway lowering")
        .expect("lowered source")
    }

    #[test]
    fn a_gateway_method_calls_the_transport_without_a_behavior_context() {
        let lowered = lower_client(
            "class ReportClient {\n    [Gateway({\"class\":\"server/services/ReportService.py\"})]\n    public BehaviorJSON Summarize(BehaviorJSON mode, BehaviorJSON detail) {}\n}\n",
        );
        assert!(lowered.contains(
            "public BehaviorJSON Summarize(BehaviorJSON mode, BehaviorJSON detail) {\nreturn Behaviors.GatewayResult(YonGateway.Call(\"ReportClient.Summarize\", Behaviors.GatewayArguments(new BehaviorJSON[] { mode, detail })));\n}"
        ));
        assert!(!lowered.contains("BehaviorContext behaviors"));
    }

    #[test]
    fn a_gateway_interface_becomes_a_class_without_a_behavior_context() {
        let lowered = lower_client(
            "interface ReportClient {\n    [Gateway({\"command\":[\"python3\"]})]\n    BehaviorJSON Ping();\n}\n",
        );
        assert!(lowered.contains("class ReportClient {"));
        assert!(lowered.contains(
            "public BehaviorJSON Ping() { \nreturn Behaviors.GatewayResult(YonGateway.Call(\"ReportClient.Ping\", Behaviors.GatewayArguments(new BehaviorJSON[] {  })));\n }"
        ));
        assert!(!lowered.contains("BehaviorContext behaviors"));
    }

    #[test]
    fn a_gateway_parameter_must_be_behavior_json() {
        let failure = super::super::lower(
            std::path::Path::new("server/remotes/ReportClient.cs"),
            "class ReportClient {\n    [Gateway({\"command\":[\"python3\"]})]\n    BehaviorJSON Ping(string mode) {}\n}\n",
            "yon",
        )
        .expect_err("typed parameter");
        let message = failure.to_string();
        assert!(message.contains("TY1312"), "{message}");
        assert!(
            message.contains("must take BehaviorJSON parameters"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn csharp_json_serializes_without_reflection() {
        if !dotnet_is_available() {
            return;
        }
        let runtime = super::runtime("json-test", "tac", "{}", "", "", "");
        let checks = r#"
public static class EmittedBehaviorTests {
    public static void Main() {
        if (System.Text.Json.JsonSerializer.IsReflectionEnabledByDefault)
            throw new System.Exception("reflection must be disabled");
        var text = "Catalog \"title\"\n<\u00e9>\\";
        var key = "a\"\n";
        var value = BehaviorJSON.Object(new System.Collections.Generic.Dictionary<string, BehaviorJSON> {
            ["z"] = BehaviorJSON.Array(BehaviorJSON.From(true), BehaviorJSON.From(25), BehaviorJSON.From(null)),
            [key] = BehaviorJSON.From(text),
        });
        var encoded = value.ToString();
        if (!encoded.StartsWith("{\"a") || encoded.Contains("\n"))
            throw new System.Exception("canonical order or escaping changed");
        var decoded = BehaviorJSON.Parse(encoded);
        if (decoded.Get(key).Text != text || !decoded.Get("z").Items[0].Boolean
            || decoded.Get("z").Items[1].Integer != 25 || !decoded.Get("z").Items[2].IsNull)
            throw new System.Exception("JSON round trip changed");
    }
}
"#;
        compile_and_execute(format!("{runtime}\n{checks}")).await;
    }

    #[tokio::test]
    async fn a_tac_service_without_adapters_uses_the_page_storage_host() {
        if !dotnet_is_available() {
            return;
        }
        let source = r#"[Setup]
public sealed class StorageSetup {
    public static BehaviorAdapters configure(BehaviorExecution execution) { return new BehaviorAdapters(); }
}
[Entity({"adapter":"indexeddb"})]
[Table("notes")]
public sealed class Note {
    [Id]
    public string id;
}
[CacheSettings({"name":"notes","ttlMs":60000,"maxBytes":4096,"maxEntries":8})]
public sealed class NotesCatalog {
    [Needs]
    public BehaviorContext behaviors;
    public int reads = 0;
    [Cached]
    public async System.Threading.Tasks.Task<BehaviorJSON> find(BehaviorJSON key) {
        await System.Threading.Tasks.Task.CompletedTask;
        reads++;
        return BehaviorJSON.From(reads);
    }
    [ClearCache({"method":"find"})]
    public async System.Threading.Tasks.Task<BehaviorJSON> forget(BehaviorJSON key) {
        await System.Threading.Tasks.Task.CompletedTask;
        return key;
    }
    [AllOrNothing({"adapter":"indexeddb"})]
    public async System.Threading.Tasks.Task<BehaviorJSON> refuse(BehaviorJSON id) {
        await behaviors.repository(typeof(Note)).save(BehaviorJSON.Object(new System.Collections.Generic.Dictionary<string, BehaviorJSON> { ["id"] = id }));
        throw new BehaviorError("REFUSED");
    }
}
public static class PageStorage {
    public static readonly System.Collections.Generic.Dictionary<string, string> Cache = new System.Collections.Generic.Dictionary<string, string>();
    public static readonly System.Collections.Generic.SortedDictionary<string, string> Rows = new System.Collections.Generic.SortedDictionary<string, string>();
    public static readonly System.Collections.Generic.Dictionary<string, string> Staged = new System.Collections.Generic.Dictionary<string, string>();
    public static string Answer(string text) {
        var request = BehaviorJSON.Parse(text);
        var operation = request.Get("op").Text;
        if (operation == "cache.get") return Cache.TryGetValue(request.Get("key").Text, out var found) ? "{\"value\":" + found + "}" : "{\"value\":{\"hit\":false}}";
        if (operation == "cache.set") { Cache[request.Get("key").Text] = "{\"hit\":true,\"value\":" + request.Get("value") + ",\"expiresAt\":" + request.Get("expiresAt") + "}"; return "{\"value\":null}"; }
        if (operation == "cache.delete") { Cache.Remove(request.Get("key").Text); return "{\"value\":null}"; }
        var command = request.Get("command");
        var action = command.Get("operation").Text;
        if (action == "transaction") {
            if (command.Get("action").Text == "begin") return "{\"value\":{\"transaction\":\"memory-1\"}}";
            if (command.Get("action").Text == "commit") foreach (var pair in Staged) Rows[pair.Key] = pair.Value;
            Staged.Clear();
            return "{\"value\":null}";
        }
        if (action == "save") { (command.Has("transaction") ? (System.Collections.Generic.IDictionary<string, string>)Staged : Rows)[command.Get("value").Get("id").Text] = command.Get("value").ToString(); return "{\"value\":" + command.Get("value") + "}"; }
        if (action == "find") return "{\"value\":[" + string.Join(",", Rows.Values) + "]}";
        return "{\"error\":\"The IndexedDB adapter does not know the operation '" + action + "'.\"}";
    }
}
public static class EmittedBehaviorTests {
    public static async System.Threading.Tasks.Task Main() {
        BehaviorStorage.Host = PageStorage.Answer;
        using var signal = new BehaviorSignal();
        using var context = Behaviors.create(typeof(StorageSetup), "notes", signal);
        var catalog = new NotesCatalog(context);
        var key = BehaviorJSON.From("a");
        var first = await catalog.find(key);
        var second = await catalog.find(key);
        await catalog.forget(key);
        var third = await catalog.find(key);
        if (first.Integer != 1 || second.Integer != 1 || third.Integer != 2) throw new System.Exception("the page cache was not used: " + first + second + third);
        await context.repository(typeof(Note)).save(BehaviorJSON.Object(new System.Collections.Generic.Dictionary<string, BehaviorJSON> { ["id"] = BehaviorJSON.From("n1") }));
        try { await catalog.refuse(BehaviorJSON.From("n2")); throw new System.Exception("refuse must fail"); }
        catch (BehaviorError error) when (error.Code == "REFUSED") { }
        var listed = await context.repository(typeof(Note)).find(BehaviorJSON.Parse("{}"));
        if (listed.ToString() != "[{\"id\":\"n1\"}]") throw new System.Exception("the refused note was saved: " + listed);
        System.Console.WriteLine("C# storage host: cache and store");
    }
}
"#;
        let emitted =
            super::super::lower(std::path::Path::new("client/pages/tac.cs"), source, "tac")
                .expect("C# storage lowering")
                .expect("annotated source");
        compile_and_execute(emitted).await;
    }

    #[tokio::test]
    async fn csharp_all_annotations_compile_and_execute() {
        if !dotnet_is_available() {
            return;
        }
        let path = std::path::Path::new("client/pages/tac.cs");
        let emitted = super::super::lower(path, &annotated_source(), "tac")
            .expect("C# annotation lowering")
            .expect("annotated source");
        assert!(emitted.contains("public AnnotatedCatalog(BehaviorContext behaviors)"));
        compile_and_execute(emitted).await;
    }
}
