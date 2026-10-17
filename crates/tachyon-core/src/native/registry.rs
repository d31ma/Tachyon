use super::csharp_browser_factory::CsharpBrowserFactory;
use crate::Failure;
use crate::annotation_layout::NativeWebApis;
use crate::project::NativeCompanion;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct NativeCompanionInput {
    pub(super) snapshot_root: PathBuf,
    pub(super) language: NativeCompanion,
    pub(super) source: PathBuf,
    pub(super) service: String,
    pub(super) class: String,
    pub(super) web_assembly: bool,
    pub(super) application_provider: bool,
}

impl NativeCompanionInput {
    pub(super) fn web_services(
        snapshot_root: &Path,
        language: NativeCompanion,
        services: &[(String, String, PathBuf)],
    ) -> Vec<Self> {
        services
            .iter()
            .map(|(service, class, source)| Self {
                snapshot_root: snapshot_root.to_path_buf(),
                language,
                source: source.clone(),
                service: service.clone(),
                class: class.clone(),
                web_assembly: true,
                application_provider: false,
            })
            .collect()
    }
    fn lifetime_declaration(
        language: NativeCompanion,
        prototype: bool,
        web_assembly: bool,
    ) -> String {
        match language {
            NativeCompanion::Swift => format!("static let tacPrototype = {prototype}\n"),
            NativeCompanion::Kotlin => format!("val tacPrototype: Boolean = {prototype}\n"),
            NativeCompanion::CSharp => format!(
                "internal static class TacServiceLifetime {{ internal const bool Prototype = {}; }}\n",
                [prototype, web_assembly].contains(&true)
            ),
            NativeCompanion::Rust => String::new(),
        }
    }

    fn refuse_preferences(&self, captured: &str) -> Result<(), Failure> {
        if self.web_assembly {
            Ok(())
        } else {
            let relative = self
                .source
                .strip_prefix(&self.snapshot_root)
                .unwrap_or(&self.source);
            crate::annotation_layout::NativeWebApis::refuse_preferences(relative, captured)
        }
    }
}

pub(super) fn source(
    companions: &[NativeCompanionInput],
    language: NativeCompanion,
) -> Result<Option<String>, Failure> {
    let mut selected = Vec::new();
    for item in companions {
        if item.language == language {
            selected.push(item);
        }
    }
    if selected.is_empty() {
        return Ok(None);
    }
    if language == NativeCompanion::Rust && selected.iter().all(|input| !input.web_assembly) {
        return super::rust_page_graph::RustPageGraph::source(&selected).map(Some);
    }
    let mut imports = BTreeSet::new();
    let mut namespaces = String::new();
    let mut services = Vec::new();
    let mut fsharp = String::new();
    let mut runtimes = String::new();
    for (index, item) in selected.iter().enumerate() {
        let body = service_body(language, item, &mut imports)?;
        let namespace = format!("TachyonService{index}");
        let (body, fsharp_body) = crate::fsharp_source::FsharpSource::web_split(&body);
        let (fsharp_body, runtime) = crate::fsharp_source::FsharpSource::runtime_split(fsharp_body);
        namespaces.push_str(&namespace_source(language, &namespace, body));
        fsharp.push_str(&crate::fsharp_source::FsharpSource::web_namespace(
            &namespace,
            fsharp_body,
        ));
        runtimes.push_str(&crate::fsharp_source::FsharpSource::runtime_namespace(
            &namespace, runtime,
        ));
        services.push((item.service.clone(), namespace));
    }
    let models = models_source(language, &selected[0].snapshot_root, &mut imports)?;
    let mut result = imports.into_iter().collect::<Vec<_>>().join("\n");
    result.push('\n');
    result.push_str(&namespaces);
    result.push_str(&models);
    result.push_str(&dispatch(language, &services));
    let result = super::fsharp_web_project::FsharpWebProject::with_model_context(
        &result,
        &selected[0].snapshot_root,
        &fsharp,
    )?;
    Ok(Some(crate::fsharp_source::FsharpSource::web_body(
        &result,
        &crate::fsharp_source::FsharpSource::with_runtime(&fsharp, &runtimes),
    )))
}

pub(super) fn models_source(
    language: NativeCompanion,
    snapshot_root: &Path,
    imports: &mut BTreeSet<String>,
) -> Result<String, Failure> {
    let extension = language_extension(language);
    let models = crate::model_sources::ModelSources::read(snapshot_root, "client", extension)?;
    let conversions = crate::route_types::Converters::tac_text(extension, snapshot_root)
        .map_err(|message| super::host::native_tool_failure(1605, &message))?;
    let source = match language {
        NativeCompanion::Rust => models.rust_module(),
        NativeCompanion::CSharp => models.csharp_namespaces(),
        NativeCompanion::Swift => body_without_imports(language, &models.joined_sources(), imports),
        NativeCompanion::Kotlin => String::new(),
    };
    Ok(format!("{source}{conversions}"))
}

fn language_extension(language: NativeCompanion) -> &'static str {
    match language {
        NativeCompanion::Swift => "swift",
        NativeCompanion::Kotlin => "kt",
        NativeCompanion::CSharp => "cs",
        NativeCompanion::Rust => "rs",
    }
}

fn service_body(
    language: NativeCompanion,
    item: &NativeCompanionInput,
    imports: &mut BTreeSet<String>,
) -> Result<String, Failure> {
    let captured = read_captured_source(&item.source)?;
    let captured = crate::without_bom(&captured);
    item.refuse_preferences(captured)?;
    super::fsharp_service::FsharpService::body_or(item, captured, || {
        compiled_service_body(language, item, captured, imports)
    })
}

fn compiled_service_body(
    language: NativeCompanion,
    item: &NativeCompanionInput,
    captured: &str,
    imports: &mut BTreeSet<String>,
) -> Result<String, Failure> {
    let identity = service_identity(language, &item.class);
    let relative = item
        .source
        .strip_prefix(&item.snapshot_root)
        .unwrap_or(&identity);
    let mut roles = crate::behavior::ServiceRoles::read(relative, captured)?;
    let (_, native_fields) = native_service(language, item, captured, &identity);
    let native_fields = native_fields
        .iter()
        .map(|field| crate::companion::lower_first(field))
        .collect::<Vec<_>>();
    roles.required.retain(|name| !native_fields.contains(name));
    let mut body = lowered_body(
        language,
        item,
        &roles.source,
        imports,
        !roles.required.is_empty(),
    )?;
    body.push_str(&required_declaration(language, &roles.required));
    body.push_str(&NativeCompanionInput::lifetime_declaration(
        language,
        roles.prototype,
        item.web_assembly,
    ));
    Ok(body)
}

fn read_captured_source(path: &Path) -> Result<String, Failure> {
    std::fs::read_to_string(path).map_err(|error| {
        super::host::native_tool_failure(
            1605,
            &format!("Cannot read captured native companion: {error}"),
        )
    })
}

fn lowered_body(
    language: NativeCompanion,
    item: &NativeCompanionInput,
    authored: &str,
    imports: &mut BTreeSet<String>,
    needs_all: bool,
) -> Result<String, Failure> {
    let identity = service_identity(language, &item.class);
    let relative = item
        .source
        .strip_prefix(&item.snapshot_root)
        .unwrap_or(&identity);
    let native_rust = language == NativeCompanion::Rust && !item.web_assembly;
    let assembled = if native_rust {
        crate::annotation_modules::Module::native_source(&item.snapshot_root, relative, authored)?
    } else {
        crate::annotation_modules::assemble(&item.snapshot_root, relative, authored, "tac")?
    };
    let authored = assembled.as_deref().unwrap_or(authored);
    let roles_consumed = native_rust
        .then(|| crate::behavior::ServiceRoles::without_roles(relative, authored))
        .transpose()?;
    let authored = roles_consumed.as_deref().unwrap_or(authored);
    let models = client_models(item, language)?;
    let mut events = crate::events::extract(&identity, authored, "tac")?;
    if let Some(module) = events.as_mut() {
        crate::events::describe(module, &identity, &item.snapshot_root, "client")?;
    }
    let held_off_the_page = [
        item.web_assembly,
        events
            .as_ref()
            .is_some_and(super::kotlin_stream_clients::KotlinStreamClients::has_streams),
    ] == [false, true];
    if held_off_the_page {
        return Err(super::host::native_tool_failure(
            1405,
            &format!(
                "{}: The page's JavaScript holds @StreamRequest and @SocketRequest connections, so a Kotlin service holds those clients only in its browser build. In a native build, write the client in JavaScript or TypeScript and receive its messages with @OnEvent",
                identity.display()
            ),
        ));
    }
    crate::route_types::Converters::require_rust_program(
        language,
        &item.snapshot_root,
        (relative, &models.assembled(relative, authored)?),
        events.as_ref(),
    )?;
    let prepared = super::events::prepare(language, authored, events.as_ref(), &identity)?;
    let (native_source, native_fields) =
        native_service(language, item, &prepared.source, &identity);
    let authored = native_source.as_str();
    let csharp_factory =
        CsharpBrowserFactory::prepare(item, &models.assembled(relative, authored)?, relative)?;
    if !item.web_assembly {
        crate::behavior::NativeStorage::refuse_builtins(&identity, authored)?;
        NativeWebApis::refuse_with_models(&identity, authored, &item.snapshot_root)?;
    }
    let lowered = task_lowered(item, &models, &identity, authored)?;
    let authored = lowered.as_deref().unwrap_or(authored);
    let portable = item.source.to_string_lossy();
    let mut excluded = prepared.excluded.clone();
    excluded.extend(native_fields);
    if item.web_assembly && language == NativeCompanion::Kotlin {
        excluded.extend(
            crate::template::required_fields::RequiredFields::constructed(
                relative,
                authored,
                &item.class,
            )
            .into_iter()
            .filter(|(_, kind)| kind == "DocumentTitle")
            .map(|(name, _)| name),
        );
    }
    let entries = super::csharp_task_inventory::CsharpTaskInventory::entries(
        item,
        authored,
        (relative, &excluded),
        &prepared.entries,
    )?;
    crate::route_types::Converters::require_rust_selected_owner(
        language,
        &item.snapshot_root,
        (relative, authored),
        &excluded,
    )?;
    let table = crate::companion::owner_table(
        super::events::companion_language(language).for_web(item.web_assembly),
        authored,
        &portable,
        &excluded,
        &entries,
        needs_all,
    )?;
    let table =
        CsharpBrowserFactory::table(&table, csharp_factory.as_deref(), &item.class, relative)?;
    let authored = crate::companion::with_required_constructor(
        super::events::companion_language(language),
        authored,
        &portable,
        needs_all,
    );
    let mut body = body_without_imports(language, &authored, imports);
    body.push_str(&table);
    Ok(body)
}

fn native_service(
    language: NativeCompanion,
    item: &NativeCompanionInput,
    source: &str,
    identity: &Path,
) -> (String, Vec<String>) {
    let class = &item.class;
    match (language, item.web_assembly) {
        (NativeCompanion::Swift, false) => {
            super::native_capabilities::NativeCapabilities::swift_service(source, identity, class)
        }
        (NativeCompanion::Swift, true) => {
            super::native_capabilities::NativeCapabilities::swift_page_service(
                source, identity, class,
            )
        }
        (_, false) => super::managed_native_capabilities::ManagedNativeCapabilities::service(
            language, identity, source, class,
        ),
        (_, true) => super::managed_native_capabilities::ManagedNativeCapabilities::page_service(
            language, identity, source, class,
        ),
    }
}

fn client_models(
    item: &NativeCompanionInput,
    language: NativeCompanion,
) -> Result<crate::model_sources::ModelMetadata, Failure> {
    crate::model_sources::ModelSources::client_metadata(
        &item.snapshot_root,
        language_extension(language),
    )
}

fn task_lowered(
    item: &NativeCompanionInput,
    models: &crate::model_sources::ModelMetadata,
    identity: &Path,
    authored: &str,
) -> Result<Option<String>, Failure> {
    models.lowered(identity, authored, |text| {
        super::csharp_task_inventory::CsharpTaskInventory::lower(item, identity, text)
    })
}

fn service_identity(language: NativeCompanion, class: &str) -> PathBuf {
    let extension = language_extension(language);
    PathBuf::from(format!("client/services/{class}.{extension}"))
}

fn body_without_imports(
    language: NativeCompanion,
    authored: &str,
    imports: &mut BTreeSet<String>,
) -> String {
    let collects_imports = matches!(language, NativeCompanion::Swift | NativeCompanion::Kotlin);
    let mut body = String::new();
    for line in authored.lines() {
        if collects_imports && line.trim_start().starts_with("import ") {
            imports.insert(line.to_owned());
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    body
}

fn required_declaration(language: NativeCompanion, required: &[String]) -> String {
    let mut quoted = Vec::new();
    for name in required {
        quoted.push(format!("{name:?}"));
    }
    let required = quoted.join(", ");
    match language {
        NativeCompanion::Rust => format!("pub const TAC_REQUIRED: &[&str] = &[{required}];\n"),
        NativeCompanion::CSharp => format!(
            "internal static class TacServiceInfo {{ internal static readonly string[] Required = {{ {required} }}; }}\n"
        ),
        NativeCompanion::Swift => format!("static let tacRequired: [String] = [{required}]\n"),
        NativeCompanion::Kotlin => format!("val tacRequired: List<String> = listOf({required})\n"),
    }
}

fn namespace_source(language: NativeCompanion, namespace: &str, source: &str) -> String {
    match language {
        NativeCompanion::Swift => format!("private enum {namespace} {{\n{source}\n}}\n"),
        NativeCompanion::Kotlin => format!("private object {namespace} {{\n{source}\n}}\n"),
        NativeCompanion::CSharp => format!("namespace {namespace} {{\n{source}\n}}\n"),
        NativeCompanion::Rust => {
            format!("#[allow(non_snake_case)]\nmod {namespace} {{\nuse super::*;\n{source}\n}}\n")
        }
    }
}

fn quoted_service(service: &str) -> String {
    serde_json::to_string(service).unwrap_or_else(|_| String::from("\"\""))
}

fn member_entry(language: NativeCompanion, service: &str, namespace: &str) -> String {
    let service = quoted_service(service);
    match language {
        NativeCompanion::Swift => {
            format!("case {service}: return {namespace}.tacCreate(tacProps)\n")
        }
        NativeCompanion::Kotlin => {
            format!("{service} -> {namespace}.tacCreate(tacProps, tacPage)\n")
        }
        NativeCompanion::CSharp => format!("{service} => {namespace}.TacBridge.Create(),\n"),
        NativeCompanion::Rust => format!("{service} => Some({namespace}::tac_create()),\n"),
    }
}

fn dispatch(language: NativeCompanion, services: &[(String, String)]) -> String {
    let mut entries = String::new();
    for (service, namespace) in services {
        entries.push_str(&member_entry(language, service, namespace));
    }
    match language {
        NativeCompanion::Swift => swift_dispatch(services, &entries),
        NativeCompanion::Kotlin => kotlin_dispatch(services, &entries),
        NativeCompanion::CSharp => csharp_dispatch(services, &entries),
        NativeCompanion::Rust => rust_dispatch(services, &entries),
    }
}

fn swift_dispatch(services: &[(String, String)], entries: &str) -> String {
    let mut known = Vec::new();
    let mut required = String::new();
    let mut prototypes = String::new();
    for (service, namespace) in services {
        known.push(serde_json::to_string(service).unwrap_or_default());
        let service = quoted_service(service);
        let _ = writeln!(required, "case {service}: return {namespace}.tacRequired");
        let _ = writeln!(
            prototypes,
            "case {service}: return {namespace}.tacPrototype"
        );
    }
    let known = known.join(",");
    format!(
        "\nfunc tacServiceKnown(_ service: String) -> Bool {{ [{known}].contains(service) }}\nfunc tacServiceMembers(_ service: String, _ tacProps: [String: Any]) -> [String: TacMember]? {{\nswitch service {{\n{entries}default: return nil\n}}\n}}\n\
         func tacServiceRequired(_ service: String) -> [String] {{\nswitch service {{\n{required}default: return []\n}}\n}}\n\
         func tacServicePrototype(_ service: String) -> Bool {{\nswitch service {{\n{prototypes}default: return true\n}}\n}}\n"
    )
}

fn kotlin_dispatch(services: &[(String, String)], entries: &str) -> String {
    let mut required = String::new();
    let mut prototype = String::new();
    for (service, namespace) in services {
        let service = quoted_service(service);
        let _ = writeln!(required, "{service} -> {namespace}.tacRequired");
        let _ = writeln!(prototype, "{service} -> {namespace}.tacPrototype");
    }
    format!(
        "\nprivate fun tacServiceMembers(service: String, tacProps: Map<String, Any?>?, tacPage: TacPageScope): Map<String, Any>? = when (service) {{\n{entries}else -> null\n}}\n\
         private fun tacServiceRequired(service: String): List<String> = when (service) {{\n{required}else -> emptyList()\n}}\n\
         private fun tacServicePrototype(service: String): Boolean = when (service) {{\n{prototype}else -> true\n}}\n"
    )
}

fn csharp_dispatch(services: &[(String, String)], entries: &str) -> String {
    let mut required = String::new();
    let mut prototypes = String::new();
    for (service, namespace) in services {
        let service = quoted_service(service);
        let _ = writeln!(
            required,
            "{service} => {namespace}.TacServiceInfo.Required,"
        );
        let _ = writeln!(
            prototypes,
            "{service} => {namespace}.TacServiceLifetime.Prototype,"
        );
    }
    format!(
        "\ninternal static class TacServices {{\ninternal static Dictionary<string, TacMember> Members(string service) => service switch {{\n{entries}_ => null\n}};\n\
         internal static string[] Required(string service) => service switch {{\n{required}_ => System.Array.Empty<string>()\n}};\n\
         internal static bool Prototype(string service) => service switch {{\n{prototypes}_ => true\n}};\n}}\n"
    )
}

fn rust_dispatch(services: &[(String, String)], entries: &str) -> String {
    let mut required = String::new();
    for (service, namespace) in services {
        let service = quoted_service(service);
        let _ = writeln!(required, "{service} => {namespace}::TAC_REQUIRED,");
    }
    format!(
        "\nfn tac_service_members(service: &str) -> Option<Vec<(&'static str, TacMember)>> {{\nmatch service {{\n{entries}_ => None\n}}\n}}\n\
         fn tac_service_required(service: &str) -> &'static [&'static str] {{\nmatch service {{\n{required}_ => &[]\n}}\n}}\n"
    )
}
