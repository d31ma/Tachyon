use super::kotlin_stream_clients::KotlinStreamClients;
use crate::{
    Failure,
    companion::CompanionLanguage,
    events::{EventListener, EventMethod, EventModule, EventType},
    project::NativeCompanion,
    unicode_name::UnicodeName,
};
use std::collections::BTreeSet;
use std::path::Path;

pub(super) struct Prepared {
    pub source: String,
    pub excluded: Vec<String>,
    pub entries: String,
}

struct PublisherField {
    name: String,
    replacement: Option<String>,
}

impl PublisherField {
    const TYPE_NAME: &str = "ApplicationEventPublisher";
    const SWIFT_OR_KOTLIN_MODIFIERS: [&str; 6] = [
        "public",
        "internal",
        "private",
        "fileprivate",
        "final",
        "lazy",
    ];
    const CSHARP_MODIFIERS: [&str; 6] = [
        "public",
        "internal",
        "private",
        "protected",
        "readonly",
        "static",
    ];

    fn first_unmodified_word<'a>(line: &'a str, modifiers: &[&str]) -> Option<(&'a str, String)> {
        let mut rest = line;
        loop {
            let word = UnicodeName::leading(rest);
            let after = &rest[word.len()..];
            let spaced = after.starts_with(char::is_whitespace);
            if [modifiers.contains(&word.as_str()), spaced] != [true, true] {
                return Some((rest, word)).filter(|(_, word)| !word.is_empty());
            }
            rest = after.trim_start();
        }
    }
}

struct ListenerParts {
    events: String,
    replay: bool,
    order: i64,
    condition: String,
    models: String,
    returns: String,
    asynchronous: bool,
}

enum Member<'a> {
    Listener(&'a EventListener),
    Hook { destroy: bool },
}

fn failure(path: &Path, message: &str) -> Failure {
    super::host::native_tool_failure(1405, &format!("{}: {message}", path.display()))
}

pub(super) fn companion_language(language: NativeCompanion) -> CompanionLanguage {
    match language {
        NativeCompanion::Swift => CompanionLanguage::Swift,
        NativeCompanion::Kotlin => CompanionLanguage::Kotlin,
        NativeCompanion::CSharp => CompanionLanguage::CSharp,
        NativeCompanion::Rust => CompanionLanguage::Rust,
    }
}

pub(super) fn prepare(
    language: NativeCompanion,
    source: &str,
    module: Option<&EventModule>,
    path: &Path,
) -> Result<Prepared, Failure> {
    let Some(module) = module else {
        return Ok(Prepared {
            source: source.into(),
            excluded: Vec::new(),
            entries: String::new(),
        });
    };
    let owner = validate_module(language, module, path)?;
    let mut output = Prepared {
        source: module.source.clone(),
        excluded: Vec::new(),
        entries: String::new(),
    };
    if let Some(owner) = owner.as_deref() {
        output.entries = KotlinStreamClients::entries(module, owner, path)?;
    }
    let metadata = owner.as_deref().and_then(|owner| {
        module
            .types
            .iter()
            .find(|event_type| event_type.name == owner)
    });
    if let Some(metadata) = metadata {
        add_members(language, module, metadata, path, &mut output)?;
    }
    let (rewritten, publishers) = publisher_members(language, &output.source);
    output.source = rewritten;
    output.excluded.extend(publishers);
    Ok(output)
}

fn add_members(
    language: NativeCompanion,
    module: &EventModule,
    metadata: &EventType,
    path: &Path,
    output: &mut Prepared,
) -> Result<(), Failure> {
    let mut seen = BTreeSet::new();
    for method in &module.methods {
        let of_a_client = module.types.iter().any(|event_type| {
            [
                event_type.name == method.owner,
                KotlinStreamClients::is_client_of(event_type, &metadata.name),
            ] == [true, true]
        });
        if of_a_client || !seen.insert((method.owner.as_str(), method.name.as_str())) {
            continue;
        }
        if is_asynchronous(method) {
            return Err(failure(
                path,
                "Native event and lifecycle methods must be synchronous; async native owner scheduling is not supported.",
            ));
        }
        output.excluded.push(method.name.clone());
        let member = member_for(metadata, method);
        let signature = &module.source[method.signature.clone()];
        output
            .entries
            .push_str(&entry(language, method, &member, path, signature)?);
    }
    Ok(())
}

fn is_asynchronous(method: &EventMethod) -> bool {
    method.asynchronous
        || method.return_type.contains("Task")
        || method.return_type.contains("Future")
}

fn member_for<'a>(metadata: &'a EventType, method: &EventMethod) -> Member<'a> {
    for listener in &metadata.listeners {
        if listener.method == method.name {
            return Member::Listener(listener);
        }
    }
    Member::Hook {
        destroy: metadata.pre_destroy.as_ref() == Some(&method.name),
    }
}

fn validate_module(
    language: NativeCompanion,
    module: &EventModule,
    path: &Path,
) -> Result<Option<String>, Failure> {
    let owner = crate::companion::owner_name(
        companion_language(language),
        &module.source,
        &path.to_string_lossy(),
    );
    let page_types: Vec<&EventType> = module
        .types
        .iter()
        .filter(|event_type| {
            let name = owner.as_deref().unwrap_or("");
            !KotlinStreamClients::is_client_of(event_type, name)
        })
        .collect();
    if page_types.len() > 1 {
        return Err(failure(
            path,
            "Native events belong to the page companion type only.",
        ));
    }
    let Some(metadata) = page_types.first() else {
        return Ok(owner);
    };
    if owner.as_deref() != Some(&metadata.name) {
        return Err(failure(
            path,
            "Native event annotations must belong to the page companion, not an unowned helper type.",
        ));
    }
    let mut topics = BTreeSet::new();
    for listener in &metadata.listeners {
        for event in &listener.events {
            topics.insert(event.as_str());
        }
    }
    if topics.len() > 32 || metadata.listeners.len() > 128 {
        return Err(failure(
            path,
            "Native event metadata exceeds its 32-topic/128-listener bounds.",
        ));
    }
    Ok(owner)
}

fn publisher_members(language: NativeCompanion, source: &str) -> (String, Vec<String>) {
    let mut rewritten = String::with_capacity(source.len() + 64);
    let mut names = Vec::new();
    for line in source.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let newline = &line[body.len()..];
        let trimmed = body.trim();
        let indent = &body[..body.len() - body.trim_start().len()];
        match publisher_declaration(language, trimmed) {
            Some(PublisherField { name, replacement }) => {
                names.push(name);
                rewritten.push_str(indent);
                rewritten.push_str(replacement.as_deref().unwrap_or(trimmed));
            }
            None => rewritten.push_str(body),
        }
        rewritten.push_str(newline);
    }
    (rewritten, names)
}

fn publisher_declaration(language: NativeCompanion, line: &str) -> Option<PublisherField> {
    if line.starts_with("//") || !line.contains(PublisherField::TYPE_NAME) {
        return None;
    }
    match language {
        NativeCompanion::Swift => swift_or_kotlin_publisher(line, &["let", "var"]),
        NativeCompanion::Kotlin => swift_or_kotlin_publisher(line, &["val", "var"]),
        NativeCompanion::CSharp => csharp_publisher(line),
        NativeCompanion::Rust => rust_publisher(line),
    }
}

fn swift_or_kotlin_publisher(line: &str, keywords: &[&str]) -> Option<PublisherField> {
    let (rest, word) =
        PublisherField::first_unmodified_word(line, &PublisherField::SWIFT_OR_KOTLIN_MODIFIERS)?;
    let after = &rest[word.len()..];
    let spaced = after.starts_with(char::is_whitespace);
    ([keywords.contains(&word.as_str()), spaced] == [true, true]).then_some(())?;
    let keyword = &line[..line.len() - rest.len() + word.len()];
    swift_or_kotlin_property(keyword, after.trim_start())
}

fn swift_or_kotlin_property(keyword: &str, declaration: &str) -> Option<PublisherField> {
    let name = UnicodeName::leading(declaration);
    if name.is_empty() {
        return None;
    }
    let tail = declaration[name.len()..].trim_start();
    let declared = tail.strip_prefix(':')?.trim_start();
    let annotation = declared
        .strip_prefix(PublisherField::TYPE_NAME)?
        .trim_start();
    if annotation.is_empty() {
        let replacement = format!("{keyword} {name} = {}()", PublisherField::TYPE_NAME);
        return Some(PublisherField {
            name,
            replacement: Some(replacement),
        });
    }
    if !annotation.starts_with('=') {
        return None;
    }
    Some(PublisherField {
        name,
        replacement: None,
    })
}

fn csharp_publisher(line: &str) -> Option<PublisherField> {
    let (rest, word) =
        PublisherField::first_unmodified_word(line, &PublisherField::CSHARP_MODIFIERS)?;
    let after = &rest[word.len()..];
    let spaced = after.starts_with(char::is_whitespace);
    ([word == PublisherField::TYPE_NAME, spaced] == [true, true]).then_some(())?;
    let modifiers = &line[..line.len() - rest.len()];
    csharp_field(modifiers, after.trim_start())
}

fn csharp_field(modifiers: &str, declaration: &str) -> Option<PublisherField> {
    let name = UnicodeName::leading(declaration);
    if name.is_empty() {
        return None;
    }
    let tail = declaration[name.len()..].trim_start();
    if tail == ";" {
        let type_name = PublisherField::TYPE_NAME;
        let replacement = format!("{modifiers}{type_name} {name} = new {type_name}();");
        return Some(PublisherField {
            name,
            replacement: Some(replacement),
        });
    }
    if !tail.starts_with('=') {
        return None;
    }
    Some(PublisherField {
        name,
        replacement: None,
    })
}

fn rust_publisher(line: &str) -> Option<PublisherField> {
    let rest = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);
    let name = UnicodeName::leading(rest);
    let declared = rest[name.len()..].trim_start().strip_prefix(':')?.trim();
    let declared_type = declared.trim_end_matches(',').trim();
    if name.is_empty() || declared_type != PublisherField::TYPE_NAME {
        return None;
    }
    Some(PublisherField {
        name,
        replacement: None,
    })
}

pub(super) fn string_literal(language: NativeCompanion, text: &str) -> String {
    let mut literal = String::with_capacity(text.len() + 2);
    literal.push('"');
    for character in text.chars() {
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '$' if language == NativeCompanion::Kotlin => literal.push_str("\\$"),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

fn events_literal(language: NativeCompanion, events: &[String]) -> String {
    let mut quoted = Vec::new();
    for event in events {
        quoted.push(serde_json::to_string(event).unwrap_or_default());
    }
    let names = quoted.join(", ");
    match language {
        NativeCompanion::Swift => format!("[{names}]"),
        NativeCompanion::Kotlin => format!("listOf({names})"),
        NativeCompanion::CSharp => format!("new[] {{ {names} }}"),
        NativeCompanion::Rust => format!("&[{names}]"),
    }
}

fn condition_literal(
    language: NativeCompanion,
    condition: Option<&serde_json::Value>,
    path: &Path,
) -> Result<String, Failure> {
    let Some(condition) = condition else {
        return Ok(optional_text(language, None));
    };
    let json = serde_json::to_string(condition)
        .map_err(|_| failure(path, "Invalid listener condition."))?;
    Ok(optional_text(language, Some(&json)))
}

fn optional_text(language: NativeCompanion, text: Option<&str>) -> String {
    let Some(text) = text else {
        return String::from(match language {
            NativeCompanion::Rust => "None",
            NativeCompanion::Swift => "nil",
            NativeCompanion::Kotlin | NativeCompanion::CSharp => "null",
        });
    };
    let literal = string_literal(language, text);
    if language == NativeCompanion::Rust {
        return format!("Some({literal})");
    }
    literal
}

fn listener_parts(
    language: NativeCompanion,
    listener: &EventListener,
    path: &Path,
) -> Result<ListenerParts, Failure> {
    let models = listener.models.as_ref().map(ToString::to_string);
    Ok(ListenerParts {
        events: events_literal(language, &listener.events),
        replay: listener.replay,
        order: listener.order,
        condition: condition_literal(language, listener.condition.as_ref(), path)?,
        models: optional_text(language, models.as_deref()),
        returns: optional_text(language, listener.returns.as_deref()),
        asynchronous: listener.asynchronous,
    })
}

fn entry(
    language: NativeCompanion,
    method: &EventMethod,
    member: &Member<'_>,
    path: &Path,
    signature: &str,
) -> Result<String, Failure> {
    let arguments = call_arguments(language, method, path, signature)?;
    let key = format!("__event_{:08}_{}", method.signature.start, method.name);
    let listener = match member {
        Member::Listener(listener) => Some(listener_parts(language, listener, path)?),
        Member::Hook { .. } => None,
    };
    let destroy = matches!(member, Member::Hook { destroy: true });
    let result = match language {
        NativeCompanion::Swift => {
            let throwing = signature.contains("throws");
            swift_entry(&method.name, &arguments, &key, listener, destroy, throwing)
        }
        NativeCompanion::Kotlin => kotlin_entry(&method.name, &arguments, &key, listener, destroy),
        NativeCompanion::CSharp => csharp_entry(method, &arguments, &key, listener, destroy),
        NativeCompanion::Rust => rust_entry(method, &arguments, &key, listener, destroy),
    };
    Ok(result)
}

fn call_arguments(
    language: NativeCompanion,
    method: &EventMethod,
    path: &Path,
    signature: &str,
) -> Result<String, Failure> {
    let mut arguments = Vec::new();
    for (index, parameter) in method.parameters.iter().enumerate() {
        let converted = convert(language, &parameter.type_source, "value", path)?;
        let label = if language == NativeCompanion::Swift {
            swift_argument_label(signature, index)
        } else {
            None
        };
        match label {
            Some(label) => arguments.push(format!("{label}: {converted}")),
            None => arguments.push(converted),
        }
    }
    Ok(arguments.join(", "))
}

fn swift_argument_label(signature: &str, index: usize) -> Option<&str> {
    let (_, after_open) = signature.split_once('(')?;
    let (parameters, _) = after_open.split_once(')')?;
    let parameter = parameters.split(',').nth(index)?;
    let before_colon = parameter.split(':').next()?;
    let label = before_colon.split_whitespace().next()?;
    if label == "_" {
        return None;
    }
    Some(label)
}

fn swift_entry(
    name: &str,
    arguments: &str,
    key: &str,
    listener: Option<ListenerParts>,
    destroy: bool,
    throwing: bool,
) -> String {
    let attempt = if throwing { "try " } else { "" };
    let call = format!("{attempt}tacInstance.{name}({arguments})");
    match listener {
        Some(ListenerParts {
            events,
            replay,
            order,
            condition,
            models,
            returns,
            asynchronous,
        }) => format!(
            "\"{key}\": .listener(TacListener({events}, {replay}, {order}, {condition}, {models}, {returns}, {asynchronous}) {{ value in {call} }}),\n"
        ),
        None => format!("\"{key}\": .hook({destroy}, {{ _ = {call} }}),\n"),
    }
}

fn kotlin_entry(
    name: &str,
    arguments: &str,
    key: &str,
    listener: Option<ListenerParts>,
    destroy: bool,
) -> String {
    match listener {
        Some(ListenerParts {
            events,
            replay,
            order,
            condition,
            models,
            returns,
            asynchronous,
        }) => format!(
            "\"{key}\" to TacListener({events}, {replay}, {order}, {condition}, {models}, {returns}, {asynchronous}) {{ value -> tacInstance.{name}({arguments}) }},\n"
        ),
        None => format!("\"{key}\" to TacHook({destroy}) {{ tacInstance.{name}(); Unit }},\n"),
    }
}

fn csharp_entry(
    method: &EventMethod,
    arguments: &str,
    key: &str,
    listener: Option<ListenerParts>,
    destroy: bool,
) -> String {
    let call = format!("Instance.{}({arguments})", method.name);
    let Some(ListenerParts {
        events,
        replay,
        order,
        condition,
        models,
        returns,
        asynchronous,
    }) = listener
    else {
        return format!("[\"{key}\"] = TacMember.Hook({destroy}, () => {{ {call}; }}),\n");
    };
    let returns_nothing = matches!(
        method.return_type.trim(),
        "" | "Void" | "()" | "Unit" | "void"
    );
    let invoke = if returns_nothing {
        format!("value => {{ {call}; return null; }}")
    } else {
        format!("value => {call}")
    };
    format!(
        "[\"{key}\"] = TacMember.Listener({events}, {replay}, {order}, {condition}, {models}, {returns}, {asynchronous}, {invoke}),\n"
    )
}

fn rust_entry(
    method: &EventMethod,
    arguments: &str,
    key: &str,
    listener: Option<ListenerParts>,
    destroy: bool,
) -> String {
    let call = format!("it.borrow_mut().{}({arguments})", method.name);
    let success_type = method
        .return_type
        .strip_prefix("Result<")
        .and_then(|value| value.split(',').next());
    let member = if let Some(listener) = listener {
        rust_listener_member(method, &call, success_type, listener)
    } else {
        rust_hook_member(&call, success_type.is_some(), destroy)
    };
    format!("(\"{key}\", {member}),\n")
}

fn rust_listener_member(
    method: &EventMethod,
    call: &str,
    success_type: Option<&str>,
    listener: ListenerParts,
) -> String {
    let ListenerParts {
        events,
        replay,
        order,
        condition,
        models,
        returns,
        asynchronous,
    } = listener;
    let answer = rust_listener_answer(method, call, success_type, returns != "None");
    format!(
        "TacMember::Listener(TacListener {{ events: {events}, replay: {replay}, order: {order}, condition: {condition}, models: {models}, returns: {returns}, asynchronous: {asynchronous}, invoke: {{let it = instance.clone(); Box::new(move |value| {answer})}} }})"
    )
}

fn rust_hook_member(call: &str, returns_result: bool, destroy: bool) -> String {
    let hook = if returns_result {
        format!("{call}.map(|_| ()).map_err(|_| \"Native listener failed\")")
    } else {
        format!("{{let _ = {call}; Ok(())}}")
    };
    format!("TacMember::Hook({destroy}, {{let it = instance.clone(); Box::new(move || {hook})}})")
}

fn rust_listener_answer(
    method: &EventMethod,
    call: &str,
    success_type: Option<&str>,
    returns_model: bool,
) -> String {
    if let Some(success_type) = success_type {
        let converted = if returns_model {
            String::from("TacModel::write(value)")
        } else {
            crate::companion::rust_read(success_type, "value")
        };
        return format!(
            "match {call} {{ Ok(value) => Ok({converted}), Err(_) => Err(\"Native listener failed\") }}"
        );
    }
    if returns_model {
        return format!("Ok(TacModel::write({call}))");
    }
    let return_type = if method.return_type.is_empty() {
        "()"
    } else {
        &method.return_type
    };
    let converted = crate::companion::rust_read(return_type, call);
    format!("Ok({converted})")
}

fn convert(
    language: NativeCompanion,
    kind: &str,
    value: &str,
    path: &Path,
) -> Result<String, Failure> {
    let converted = match language {
        NativeCompanion::Swift => swift_conversion(kind, value),
        NativeCompanion::Kotlin => kotlin_conversion(kind, value),
        NativeCompanion::CSharp => csharp_conversion(kind, value),
        NativeCompanion::Rust => rust_conversion(kind, value),
    };
    converted.ok_or_else(|| {
        failure(
            path,
            "Native listener parameters require portable JSON scalar types.",
        )
    })
}

fn swift_conversion(kind: &str, value: &str) -> Option<String> {
    match kind {
        "Any" | "Any?" => Some(value.into()),
        "String" => Some(format!("({value} as? String ?? \"\")")),
        "Int" => Some(format!("({value} as? Int ?? 0)")),
        "Double" => Some(format!("({value} as? Double ?? 0)")),
        "Bool" => Some(format!("({value} as? Bool ?? false)")),
        _ if is_model_type("swift", kind) => Some(format!("try tacModel({kind}.self, {value})")),
        _ => None,
    }
}

fn kotlin_conversion(kind: &str, value: &str) -> Option<String> {
    match kind {
        "Any" | "Any?" => Some(value.into()),
        "String" => Some(format!("tacString({value})")),
        "Int" => Some(format!("tacInt({value})")),
        "Long" => Some(format!("tacLong({value})")),
        "Double" => Some(format!("tacDouble({value})")),
        "Boolean" => Some(format!("tacBoolean({value})")),
        _ if is_model_type("kt", kind) => {
            Some(format!("tacModel({kind}::class, {value}) as {kind}"))
        }
        _ => None,
    }
}

fn csharp_conversion(kind: &str, value: &str) -> Option<String> {
    match kind {
        "object" => Some(value.into()),
        "string" => Some(format!("Convert.ToString({value})")),
        "int" => Some(format!("Convert.ToInt32({value})")),
        "long" => Some(format!("Convert.ToInt64({value})")),
        "double" => Some(format!("Convert.ToDouble({value})")),
        "bool" => Some(format!("Convert.ToBoolean({value})")),
        _ if is_model_type("cs", kind) => Some(format!("TacModels.Read<{kind}>({value})")),
        _ => None,
    }
}

fn rust_conversion(kind: &str, value: &str) -> Option<String> {
    match kind {
        "TacValue" => Some(format!("{value}.clone()")),
        "String" => Some(format!("{value}.as_text()")),
        "i64" => Some(format!("{value}.as_int()")),
        "f64" => Some(format!("{value}.as_float()")),
        "bool" => Some(format!("{value}.as_flag()")),
        _ if is_model_type("rs", kind) => Some(format!("<{kind} as TacModel>::read({value})")),
        _ => None,
    }
}

fn is_model_type(language: &str, kind: &str) -> bool {
    matches!(
        crate::route_types::TypeText::shape(language, kind),
        Ok(crate::route_types::Shape::Model(_))
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn publisher_parts(field: Option<PublisherField>) -> Option<(String, Option<String>)> {
        field.map(|field| (field.name, field.replacement))
    }

    #[test]
    fn publisher_fields_skip_only_their_language_modifiers() {
        let swift = [
            (
                "public let events: ApplicationEventPublisher",
                Some((
                    "events",
                    Some("public let events = ApplicationEventPublisher()"),
                )),
            ),
            (
                "public  lazy var bus: ApplicationEventPublisher",
                Some((
                    "bus",
                    Some("public  lazy var bus = ApplicationEventPublisher()"),
                )),
            ),
            (
                "private var events: ApplicationEventPublisher = ApplicationEventPublisher()",
                Some(("events", None)),
            ),
            ("static let events: ApplicationEventPublisher", None),
            ("public", None),
            ("public let", None),
            ("", None),
        ];
        for (line, expected) in swift {
            let expected = expected
                .map(|(name, replacement)| (name.to_owned(), replacement.map(str::to_owned)));
            assert_eq!(
                publisher_parts(swift_or_kotlin_publisher(line, &["let", "var"])),
                expected,
                "{line:?}"
            );
        }
        assert_eq!(
            publisher_parts(swift_or_kotlin_publisher(
                "val events: ApplicationEventPublisher",
                &["val", "var"]
            )),
            Some((
                String::from("events"),
                Some(String::from("val events = ApplicationEventPublisher()"))
            ))
        );
        let csharp = [
            (
                "private readonly ApplicationEventPublisher events;",
                Some((
                    "events",
                    Some(
                        "private readonly ApplicationEventPublisher events = new ApplicationEventPublisher();",
                    ),
                )),
            ),
            (
                "ApplicationEventPublisher events = new();",
                Some(("events", None)),
            ),
            ("public lazy ApplicationEventPublisher events;", None),
            ("public static", None),
            ("ApplicationEventPublisherevents;", None),
        ];
        for (line, expected) in csharp {
            let expected = expected
                .map(|(name, replacement)| (name.to_owned(), replacement.map(str::to_owned)));
            assert_eq!(
                publisher_parts(csharp_publisher(line)),
                expected,
                "{line:?}"
            );
        }
    }

    fn probe_path(extension: &str) -> PathBuf {
        PathBuf::from(format!("client/services/ProbeService.{extension}"))
    }

    fn prepared(extension: &str, language: NativeCompanion, authored: &str) -> Prepared {
        let path = probe_path(extension);
        let metadata = crate::events::extract(&path, authored, "tac").expect("parse events");
        prepare(language, authored, metadata.as_ref(), &path).expect("prepare native events")
    }

    fn assert_contains(entries: &str, expected: &str) {
        assert!(entries.contains(expected), "{entries}");
    }

    fn assert_excluded(output: &Prepared, name: &str) {
        assert_eq!(output.excluded, vec![String::from(name)]);
    }

    fn asynchronous_listener_sources() -> [(&'static str, NativeCompanion, &'static str); 4] {
        [
            (
                "swift",
                NativeCompanion::Swift,
                "class ProbeService {\n @OnEvent\n func receive(_ value: Probe) async {}\n}\n",
            ),
            (
                "kt",
                NativeCompanion::Kotlin,
                "class ProbeService {\n @OnEvent\n suspend fun receive(value: Probe) {}\n}\n",
            ),
            (
                "cs",
                NativeCompanion::CSharp,
                "public class ProbeService {\n [OnEvent]\n public async Task Receive(Probe value) {}\n}\n",
            ),
            (
                "rs",
                NativeCompanion::Rust,
                "#[derive(Default)]\nstruct ProbeService {}\nimpl ProbeService {\n #[OnEvent]\n async fn receive(&self, value: Probe) {}\n}\n",
            ),
        ]
    }

    #[test]
    fn asynchronous_native_listeners_fail_before_the_toolchain() {
        for (extension, language, authored) in asynchronous_listener_sources() {
            let path = probe_path(extension);
            let metadata = crate::events::extract(&path, authored, "tac").expect("parse event");
            let error = prepare(language, authored, metadata.as_ref(), &path)
                .err()
                .expect("asynchronous listener rejected");
            assert!(format!("{error:?}").contains("must be synchronous"));
        }
    }

    fn event_publisher_source(extension: &str) -> &'static str {
        match extension {
            "cs" => {
                "public class ProbeService {\n [EventPublisher({\"event\":\"value\"})]\n public int Send() { return 1; }\n}\n"
            }
            "rs" => {
                "#[derive(Default)]\nstruct ProbeService {}\nimpl ProbeService {\n #[EventPublisher({\"event\":\"value\"})]\n fn send(&self) -> i64 { 1 }\n}\n"
            }
            _ => {
                "class ProbeService {\n @EventPublisher({\"event\":\"value\"})\n fun send(): Int { return 1 }\n}\n"
            }
        }
    }

    #[test]
    fn event_publisher_annotations_are_removed() {
        for extension in ["swift", "kt", "cs", "rs"] {
            let path = probe_path(extension);
            let error = crate::events::extract(&path, event_publisher_source(extension), "tac")
                .expect_err("EventPublisher rejected");
            assert!(
                format!("{error:?}").contains("EventPublisher is removed (ADR 0031)"),
                "{error:?}"
            );
        }
    }

    fn swift_publisher_is_constructed() {
        let swift = prepared(
            "swift",
            NativeCompanion::Swift,
            "final class ProbeService {\n    var count: Int = 0\n    private let applicationEventPublisher: ApplicationEventPublisher\n    func send() { applicationEventPublisher.publishEvent(Pulse(amount: count)) }\n}\n",
        );
        assert!(
            swift.source.contains(
                "    private let applicationEventPublisher = ApplicationEventPublisher()\n"
            ),
            "{}",
            swift.source
        );
        assert_excluded(&swift, "applicationEventPublisher");
        assert!(swift.entries.is_empty());
    }

    fn kotlin_publisher_is_constructed() {
        let kotlin = prepared(
            "kt",
            NativeCompanion::Kotlin,
            "class ProbeService {\n  val applicationEventPublisher: ApplicationEventPublisher\n  fun send() { applicationEventPublisher.publishEvent(Pulse(1)) }\n}\n",
        );
        assert!(
            kotlin
                .source
                .contains("  val applicationEventPublisher = ApplicationEventPublisher()\n"),
            "{}",
            kotlin.source
        );
        assert_excluded(&kotlin, "applicationEventPublisher");
    }

    fn csharp_publisher_is_constructed() {
        let csharp = prepared(
            "cs",
            NativeCompanion::CSharp,
            "public class ProbeService {\n    private readonly ApplicationEventPublisher applicationEventPublisher;\n    public void Send() { applicationEventPublisher.PublishEvent(new Pulse(1)); }\n}\n",
        );
        assert!(
            csharp.source.contains(
                "    private readonly ApplicationEventPublisher applicationEventPublisher = new ApplicationEventPublisher();\n"
            ),
            "{}",
            csharp.source
        );
        assert_excluded(&csharp, "applicationEventPublisher");
    }

    fn rust_publisher_is_kept() {
        let rust = prepared(
            "rs",
            NativeCompanion::Rust,
            "#[derive(Default)]\nstruct ProbeService {\n    count: i64,\n    application_event_publisher: ApplicationEventPublisher,\n}\nimpl ProbeService {\n    fn send(&self) { self.application_event_publisher.publish_event(Pulse { amount: self.count }, false); }\n}\n",
        );
        assert!(
            rust.source
                .contains("    application_event_publisher: ApplicationEventPublisher,\n")
        );
        assert_excluded(&rust, "application_event_publisher");
    }

    fn initialised_swift_publisher_is_kept() {
        let initialised = prepared(
            "swift",
            NativeCompanion::Swift,
            "final class ProbeService {\n    let applicationEventPublisher: ApplicationEventPublisher = ApplicationEventPublisher()\n}\n",
        );
        assert!(initialised.source.contains(
            "let applicationEventPublisher: ApplicationEventPublisher = ApplicationEventPublisher()\n"
        ));
        assert_excluded(&initialised, "applicationEventPublisher");
    }

    #[test]
    fn publisher_members_are_constructed_by_the_prelude_and_leave_the_table() {
        swift_publisher_is_constructed();
        kotlin_publisher_is_constructed();
        csharp_publisher_is_constructed();
        rust_publisher_is_kept();
        initialised_swift_publisher_is_kept();
    }

    fn listener_option_sources() -> [(&'static str, NativeCompanion, &'static str); 4] {
        [
            (
                "swift",
                NativeCompanion::Swift,
                "final class ProbeService {\n    var log = \"\"\n    @Order(-2)\n    @OnEvent({\"condition\":\"value.total >= 10 && event == 'Probe'\"})\n    func first(_ value: Probe) -> Echo { return Echo(amount: 1) }\n    @InBackground()\n    @OnEvent({\"replay\":true})\n    func later(_ value: Probe) { log += \"x\" }\n}\n",
            ),
            (
                "kt",
                NativeCompanion::Kotlin,
                "class ProbeService {\n    var log: String = \"\"\n    @Order(-2)\n    @OnEvent({\"condition\":\"value.total >= 10 && event == 'Probe'\"})\n    fun first(value: Probe): Echo { return Echo(1) }\n    @InBackground()\n    @OnEvent({\"replay\":true})\n    fun later(value: Probe) { log += \"x\" }\n}\n",
            ),
            (
                "cs",
                NativeCompanion::CSharp,
                "public class ProbeService {\n    public string Log = \"\";\n    [Order(-2)]\n    [OnEvent({\"condition\":\"value.total >= 10 && event == 'Probe'\"})]\n    public Echo First(Probe value) { return new Echo(1); }\n    [InBackground()]\n    [OnEvent({\"replay\":true})]\n    public void Later(Probe value) { Log += \"x\"; }\n}\n",
            ),
            (
                "rs",
                NativeCompanion::Rust,
                "#[derive(Default)]\nstruct ProbeService {\n    log: String,\n}\nimpl ProbeService {\n    #[Order(-2)]\n    #[OnEvent({\"condition\":\"value.total >= 10 && event == 'Probe'\"})]\n    fn first(&mut self, value: Probe) -> Echo { Echo { amount: 1 } }\n    #[InBackground()]\n    #[OnEvent({\"replay\":true})]\n    fn later(&mut self, value: Probe) { self.log.push('x'); }\n}\n",
            ),
        ]
    }

    fn assert_swift_listeners(entries: &str) {
        assert_contains(entries, "TacListener([\"Probe\"], false, -2, \"");
        assert_contains(entries, condition_matcher());
        assert_contains(
            entries,
            "\", nil, \"Echo\", false) { value in tacInstance.first(try tacModel(Probe.self, value)) }",
        );
        assert_contains(
            entries,
            "TacListener([\"Probe\"], true, 0, nil, nil, nil, true) { value in tacInstance.later(try tacModel(Probe.self, value)) }",
        );
    }

    fn assert_kotlin_listeners(entries: &str) {
        assert_contains(entries, "TacListener(listOf(\"Probe\"), false, -2, \"");
        assert_contains(entries, condition_matcher());
        assert_contains(
            entries,
            "TacListener(listOf(\"Probe\"), true, 0, null, null, null, true) { value -> tacInstance.later(tacModel(Probe::class, value) as Probe) }",
        );
    }

    fn assert_csharp_listeners(entries: &str) {
        assert_contains(
            entries,
            "TacMember.Listener(new[] { \"Probe\" }, false, -2, \"",
        );
        assert_contains(entries, condition_matcher());
        assert_contains(
            entries,
            "null, \"Echo\", false, value => Instance.First(TacModels.Read<Probe>(value)))",
        );
        assert_contains(
            entries,
            "TacMember.Listener(new[] { \"Probe\" }, true, 0, null, null, null, true, value => { Instance.Later(TacModels.Read<Probe>(value)); return null; })",
        );
    }

    fn assert_rust_listeners(entries: &str) {
        assert_contains(
            entries,
            "TacListener { events: &[\"Probe\"], replay: false, order: -2, condition: Some(\"",
        );
        assert_contains(entries, condition_matcher());
        assert_contains(
            entries,
            "models: None, returns: Some(\"Echo\"), asynchronous: false, invoke: {let it = instance.clone(); Box::new(move |value| Ok(TacModel::write(it.borrow_mut().first(<Probe as TacModel>::read(value)))))} }",
        );
        assert_contains(
            entries,
            "TacListener { events: &[\"Probe\"], replay: true, order: 0, condition: None, models: None, returns: None, asynchronous: true, invoke: {let it = instance.clone(); Box::new(move |value| Ok({ it.borrow_mut().later(<Probe as TacModel>::read(value)); TacValue::Null }))} }",
        );
    }

    fn assert_listener_order(language: NativeCompanion, output: &Prepared) {
        let entries = &output.entries;
        let (earlier, later) = if language == NativeCompanion::CSharp {
            ("_First", "_Later")
        } else {
            ("_first", "_later")
        };
        assert!(
            entries.find(earlier) < entries.find(later) && entries.contains(earlier),
            "{entries}"
        );
        assert_eq!(
            output.excluded,
            vec![String::from(&earlier[1..]), String::from(&later[1..])]
        );
    }

    #[test]
    fn listener_options_lower_into_every_language() {
        for (extension, language, authored) in listener_option_sources() {
            let output = prepared(extension, language, authored);
            match language {
                NativeCompanion::Swift => assert_swift_listeners(&output.entries),
                NativeCompanion::Kotlin => assert_kotlin_listeners(&output.entries),
                NativeCompanion::CSharp => assert_csharp_listeners(&output.entries),
                NativeCompanion::Rust => assert_rust_listeners(&output.entries),
            }
            assert_listener_order(language, &output);
        }
    }

    fn condition_matcher() -> &'static str {
        "{\\\"k\\\":\\\"and\\\""
    }

    #[test]
    fn matcher_literals_escape_each_language() {
        let condition = serde_json::json!({"k":"cmp","op":"eq","l":{"k":"value","path":["a$b"]},"r":{"k":"lit","v":"say \"hi\"\\"}});
        let path = Path::new("client/services/ProbeService.kt");
        let kotlin =
            condition_literal(NativeCompanion::Kotlin, Some(&condition), path).expect("literal");
        assert!(kotlin.contains("a\\$b"), "{kotlin}");
        assert!(
            kotlin.contains("say \\\\\\\"hi\\\\\\\"\\\\\\\\"),
            "{kotlin}"
        );
        let swift =
            condition_literal(NativeCompanion::Swift, Some(&condition), path).expect("literal");
        assert!(swift.contains("a$b"), "{swift}");
        assert!(swift.starts_with('"') && swift.ends_with('"'));
        let rust =
            condition_literal(NativeCompanion::Rust, Some(&condition), path).expect("literal");
        assert!(rust.starts_with("Some(\"") && rust.ends_with("\")"));
        assert_eq!(
            condition_literal(NativeCompanion::Rust, None, path).expect("none"),
            "None"
        );
        assert_eq!(
            condition_literal(NativeCompanion::Swift, None, path).expect("nil"),
            "nil"
        );
        assert_eq!(
            condition_literal(NativeCompanion::CSharp, None, path).expect("null"),
            "null"
        );
    }

    fn swift_service_with_topics(count: usize) -> String {
        use std::fmt::Write as _;

        let mut authored = String::from("final class ProbeService {\n");
        for index in 0..count {
            let _ = write!(
                authored,
                "    @OnEvent\n    func receive{index}(_ value: Topic{index}) {{}}\n"
            );
        }
        authored.push_str("}\n");
        authored
    }

    #[test]
    fn listener_bounds_hold_for_native_services() {
        let authored = swift_service_with_topics(33);
        let path = Path::new("client/services/ProbeService.swift");
        let metadata = crate::events::extract(path, &authored, "tac").expect("parse events");
        let Err(error) = prepare(NativeCompanion::Swift, &authored, metadata.as_ref(), path) else {
            unreachable!("33 topics rejected")
        };
        assert!(format!("{error:?}").contains("32-topic/128-listener"));
    }

    /// Each fixture carries the companion prelude exactly as native staging appends it
    /// after the authored unit, so `scripts/native/events-test.mjs` compiles what ships.
    fn owner_fixtures() -> [(&'static str, NativeCompanion, &'static str, String); 4] {
        [
            (
                "swift",
                NativeCompanion::Swift,
                include_str!("../../tests/fixtures/native-events/tac.swift"),
                super::super::macos::companion_prelude().to_owned(),
            ),
            (
                "kt",
                NativeCompanion::Kotlin,
                include_str!("../../tests/fixtures/native-events/tac.kt"),
                super::super::android::companion_prelude(),
            ),
            (
                "cs",
                NativeCompanion::CSharp,
                include_str!("../../tests/fixtures/native-events/tac.cs"),
                super::super::windows::companion_prelude().to_owned()
                    + &super::super::csharp_web_apis::CsharpWebApis::native(),
            ),
            (
                "rs",
                NativeCompanion::Rust,
                include_str!("../../tests/fixtures/native-events/tac.rs"),
                super::super::rust::companion_prelude().to_owned()
                    + &crate::route_types::WebEventSources::rust(),
            ),
        ]
    }

    fn owner_models(extension: &str) -> &'static str {
        match extension {
            "swift" => include_str!("../../tests/fixtures/native-events/models/Events.swift"),
            "kt" => include_str!("../../tests/fixtures/native-events/models/Events.kt"),
            "cs" => include_str!("../../tests/fixtures/native-events/models/Events.cs"),
            _ => include_str!("../../tests/fixtures/native-events/models/Events.rs"),
        }
    }

    fn write_owner_models(root: &Path, extension: &str) {
        let folder = root.join("client/models");
        std::fs::create_dir_all(&folder).expect("models folder");
        for model in owner_models(extension).split("\n\n") {
            let names = crate::route_types::ModelTable::declared_names(extension, model);
            let file = folder.join(format!("{}.{extension}", names[0]));
            std::fs::write(file, model).expect("write model");
        }
    }

    fn emit_owner_unit(
        root: &Path,
        extension: &str,
        language: NativeCompanion,
        authored: &str,
    ) -> String {
        let source = root
            .join("client/services")
            .join(format!("OwnerProbeService.{extension}"));
        std::fs::create_dir_all(source.parent().expect("parent")).expect("fixture tree");
        let marker = match extension {
            "rs" => "#[Service]",
            "cs" => "[Service]",
            _ => "@Service",
        };
        let role_source = if extension == "rs" {
            authored.replacen("#[derive(Default)]", "#[Service]\n#[derive(Default)]", 1)
        } else {
            format!("{marker}\n{authored}")
        };
        std::fs::write(&source, role_source).expect("write fixture");
        write_owner_models(root, extension);
        let companion = super::super::registry::NativeCompanionInput {
            snapshot_root: root.to_path_buf(),
            language,
            source,
            service: "ownerProbe".into(),
            class: "OwnerProbeService".into(),
            web_assembly: false,
            application_provider: false,
        };
        super::super::registry::source(&[companion], language)
            .expect("emit native event owner")
            .expect("selected owner")
    }

    fn assert_owner_unit(unit: &str) {
        assert!(!unit.contains("@EventPublisher"));
        assert!(unit.contains("Listener") || unit.contains(".listener"));
        assert!(unit.contains("ApplicationEventPublisher"), "{unit}");
        assert!(!unit.contains("\"applicationEventPublisher\""), "{unit}");
        assert!(
            !unit.contains("\"applicationEventPublisher\": .field"),
            "{unit}"
        );
    }

    fn assert_owner_models(extension: &str, unit: &str) {
        let expected = match extension {
            "swift" => "extension Pulse: Codable {}",
            "cs" => "[System.Text.Json.Serialization.JsonSerializable(typeof(Models.Pulse))]",
            "rs" => "impl TacModel for crate::TachyonRustServices::models::Pulse {",
            _ => "tacModel(models.Pulse::class, value) as models.Pulse",
        };
        assert!(unit.contains(expected), "{extension}: {unit}");
        assert!(
            unit.contains("{\\\"Pulse\\\":{"),
            "{extension}: listener models are missing: {unit}"
        );
    }

    #[test]
    fn native_event_owner_units_are_emitted() {
        let captured = tempfile::tempdir().expect("fixture directory");
        let output = std::env::var_os("TAC_NATIVE_EVENT_FIXTURES").map(PathBuf::from);
        for (extension, language, authored, prelude) in owner_fixtures() {
            let unit = emit_owner_unit(captured.path(), extension, language, authored);
            assert_owner_unit(&unit);
            assert_owner_models(extension, &unit);
            if let Some(output) = &output {
                std::fs::create_dir_all(output).expect("create fixture output");
                std::fs::write(output.join(format!("owner.{extension}")), unit)
                    .expect("write emitted owner");
                std::fs::write(output.join(format!("prelude.{extension}")), prelude)
                    .expect("write emitted prelude");
                if extension == "kt" {
                    let models = format!("package models\n\n{}", owner_models("kt"));
                    std::fs::write(output.join("owner-models.kt"), models)
                        .expect("write emitted models");
                }
            }
        }
    }
}
