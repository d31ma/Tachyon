use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::route_types::{ModelTable, RouteTypes, Shape, TypeText};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;

mod browser_streams;
pub(crate) mod condition;
pub(crate) mod publications;
pub(crate) mod yon;
pub(crate) use browser_streams::BrowserStreams;

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct EventType {
    pub name: String,
    pub post_construct: Option<String>,
    pub pre_destroy: Option<String>,
    pub listeners: Vec<EventListener>,
    pub resources: Vec<EventResource>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub streams: Vec<EventStream>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EventStream {
    pub method: String,
    pub path: String,
    pub event: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub socket: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub topic: bool,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EventListener {
    pub method: String,
    pub events: Vec<String>,
    pub replay: bool,
    pub condition: Option<Value>,
    pub id: Option<String>,
    pub order: i64,
    pub asynchronous: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct EventResource {
    pub field: String,
    pub name: String,
}
#[derive(Clone, Debug)]
pub(crate) struct EventParameter {
    pub name: String,
    pub type_source: String,
}
#[derive(Clone, Debug)]
pub(crate) struct EventMethod {
    pub owner: String,
    pub name: String,
    pub parameters: Vec<EventParameter>,
    pub signature: Range<usize>,
    pub body: Range<usize>,
    pub return_type: String,
    pub asynchronous: bool,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct TopicUse {
    pub topics: Vec<String>,
    pub listeners: Vec<(String, String)>,
}
#[derive(Clone, Debug)]
pub(crate) struct EventModule {
    pub source: String,
    pub types: Vec<EventType>,
    pub methods: Vec<EventMethod>,
    pub topics: Vec<String>,
}

impl EventModule {
    const MAX_SOURCE_BYTES: usize = 1024 * 1024;
    const MAX_MARKERS: usize = 512;
    const MAX_EVENT_NAMES: usize = 128;

    pub(crate) fn first_marker(source: &str, language: &str, names: &[&str]) -> Option<usize> {
        scan(source, language)
            .markers
            .into_iter()
            .find(|marker| names.contains(&marker.name.as_str()))
            .map(|marker| marker.span.start)
    }

    fn add_topic_streams(&mut self) {
        for event_type in &mut self.types {
            for listener in &event_type.listeners {
                let Some(model) = listener.events.first() else {
                    continue;
                };
                let Some(slug) = crate::topics::Topics::slug(model) else {
                    continue;
                };
                let path = format!("/.tachyon/topics/{slug}");
                let known = event_type.streams.iter().any(|stream| stream.path == path);
                if !self.topics.contains(model) || known {
                    continue;
                }
                event_type.streams.push(EventStream {
                    method: String::new(),
                    path,
                    event: model.clone(),
                    socket: false,
                    models: listener.models.clone(),
                    topic: true,
                });
            }
        }
    }

    pub(crate) fn topic_use(&self) -> TopicUse {
        TopicUse {
            topics: self.topics.clone(),
            listeners: self.topic_listeners(),
        }
    }

    pub(crate) fn topic_listeners(&self) -> Vec<(String, String)> {
        let mut found = Vec::new();
        for event_type in &self.types {
            for listener in &event_type.listeners {
                let Some(model) = listener.events.first() else {
                    continue;
                };
                if self.topics.contains(model) {
                    found.push((
                        model.clone(),
                        format!("{}.{}", event_type.name, listener.method),
                    ));
                }
            }
        }
        found
    }

    pub(crate) fn describe_models(&mut self, models: &ModelTable) -> Result<(), String> {
        for event_type in &mut self.types {
            for listener in &mut event_type.listeners {
                let Some(model) = listener.events.first() else {
                    continue;
                };
                let described = models.event_models(model).map_err(|message| {
                    format!("The OnEvent method '{}': {message}", listener.method)
                })?;
                listener.models = Some(described);
                let returns_a_model = listener
                    .returns
                    .as_deref()
                    .is_some_and(|name| models.is_record(name));
                if !returns_a_model {
                    listener.returns = None;
                }
            }
            for stream in &mut event_type.streams {
                let written = if stream.socket {
                    "SocketRequest"
                } else {
                    "StreamRequest"
                };
                let described = models.event_models(&stream.event).map_err(|message| {
                    format!("The {written} method '{}': {message}", stream.method)
                })?;
                stream.models = Some(described);
            }
        }
        Ok(())
    }

    fn has_listeners(&self) -> bool {
        self.types
            .iter()
            .any(|event_type| !event_type.listeners.is_empty() || !event_type.streams.is_empty())
    }
}

pub(crate) fn describe(
    module: &mut EventModule,
    path: &Path,
    root: &Path,
    realm: &str,
) -> Result<(), Failure> {
    let language = language_of(path);
    let mut models = match ModelTable::read_realm(root, realm, language) {
        Ok(models) => models,
        Err(message) if module.has_listeners() => {
            return Err(failure(&Path::new(realm).join("models"), 0, message));
        }
        Err(_) => ModelTable::default(),
    };
    if language == "js" {
        models.add_documented(&module.source);
    }
    if realm == "client" {
        models.add_web_events();
    }
    module.topics = models.topic_names();
    if !module.has_listeners() {
        return Ok(());
    }
    module
        .describe_models(&models)
        .map_err(|message| failure(path, 0, message))?;
    if realm == "client" {
        module.add_topic_streams();
    }
    Ok(())
}

impl EventListener {
    const MAX_ID_BYTES: usize = 64;
    const MAX_ORDER: i64 = 1_000_000;
    const OPTION_KEYS: [&str; 3] = ["replay", "condition", "id"];
    const NAMED_EVENT_REFUSAL: &str = "OnEvent takes no event names: the model type of the payload parameter is the event (ADR 0055). Write @OnEvent and give the parameter a model type, for example placed(OrderPlaced order). `ty migrate fix --write` converts a listener whose event name matches a model";
}

impl EventResource {
    const TAC_NAMES: [&str; 18] = [
        "view",
        "lifecycle",
        "resourceLoader",
        "taskScheduler",
        "applicationEventPublisher",
        "clock",
        "secureRandom",
        "connectivity",
        "pageVisibility",
        "preferences",
        "preferenceStrings",
        "documentTitle",
        "clipboard",
        "share",
        "notifications",
        "geolocation",
        "windowControls",
        "filePicker",
    ];
    const RENAMED_NAMES: [(&str, &str); 4] = [
        ("element", "view"),
        ("signal", "lifecycle"),
        ("renderScheduler", "taskScheduler"),
        ("redis", "redisTemplate"),
    ];
    const FIELD_MODIFIERS: [&str; 6] = [
        "private ",
        "protected ",
        "public ",
        "readonly ",
        "declare ",
        "override ",
    ];
    const PUBLISHER: &str = "applicationEventPublisher";
    const REMOVED_CACHE_MANAGER: &str = "cacheManager";
    const CACHE_MANAGER_REFUSAL: &str = "The cacheManager resource was removed with the IndexedDB fetch cache (ADR 0087). Mark the HTTPClient method that changes the data @ClearCache or @UpdateCache, like Spring's @CacheEvict and @CachePut";

    fn new_name(old_name: &str) -> Option<&'static str> {
        for (old, new) in Self::RENAMED_NAMES {
            if old == old_name {
                return Some(new);
            }
        }
        None
    }
}

struct Marker {
    span: Range<usize>,
    name: String,
    arguments: String,
}

struct ListenerModifiers {
    order: Option<i64>,
    asynchronous: bool,
    at: usize,
}

type ModifiersByMethod = BTreeMap<(String, String), ListenerModifiers>;

struct EventScan {
    code: Vec<u8>,
    markers: Vec<Marker>,
    error: Option<&'static str>,
}

struct SourceContext<'a> {
    path: &'a Path,
    source: &'a str,
    code: &'a str,
    environment: &'a str,
    language: &'a str,
}

fn failure(path: &Path, at: usize, message: impl Into<String>) -> Failure {
    let help = "Use own-line OnEvent, OnStart and OnStop annotations on named instance members, and @Needs on a field named after the resource it takes.";
    let span = source_span(&path.to_string_lossy(), at, at.saturating_add(1));
    Failure::one(diagnostic(1308, message, Some(help.into()), span))
}

fn language_of(path: &Path) -> &str {
    let Some(extension) = path.extension() else {
        return "";
    };
    extension.to_str().unwrap_or("")
}

fn is_tac_script(environment: &str, language: &str) -> bool {
    environment == "tac" && matches!(language, "js" | "ts")
}

fn is_line_break(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r')
}

fn trimmed_lines(text: &str) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset + line.len() - line.trim_start().len();
        let end = offset + line.trim_end().len();
        offset += line.len();
        if start < end {
            lines.push(start..end);
        }
    }
    lines
}

fn masked_code(source: &str, visible: &[bool]) -> Vec<u8> {
    let mut code = Vec::new();
    for (byte, is_visible) in source.bytes().zip(visible) {
        if *is_visible || is_line_break(byte) {
            code.push(byte);
        } else {
            code.push(b' ');
        }
    }
    code
}

fn blank_line(code: &mut [u8]) {
    for byte in code {
        if !is_line_break(*byte) {
            *byte = b' ';
        }
    }
}

fn autowired_resources(path: &Path, source: &str, code: &[u8]) -> Result<Vec<Marker>, Failure> {
    let mut found = Vec::new();
    for span in trimmed_lines(source) {
        if !matches!(&source[span.clone()], "@Needs" | "@Needs()") {
            continue;
        }
        let following_code = String::from_utf8_lossy(&code[span.end..]);
        let Some(declaration) = first_non_empty_line(&following_code) else {
            continue;
        };
        let field = field_name(declaration);
        if field == EventResource::REMOVED_CACHE_MANAGER {
            return Err(failure(
                path,
                span.start,
                EventResource::CACHE_MANAGER_REFUSAL,
            ));
        }
        if let Some(new_name) = EventResource::new_name(field) {
            return Err(failure(
                path,
                span.start,
                format!(
                    "The '{field}' resource is now '{new_name}', Spring's name for it: name the field {new_name}"
                ),
            ));
        }
        if EventResource::TAC_NAMES.contains(&field) {
            found.push(Marker {
                span,
                name: String::from("AutowiredResource"),
                arguments: format!("{{\"name\":\"{field}\"}}"),
            });
        }
    }
    Ok(found)
}

fn first_non_empty_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

fn field_name(declaration: &str) -> &str {
    let mut declaration = declaration;
    while let Some(rest) = strip_field_modifier(declaration) {
        declaration = rest.trim_start();
    }
    let is_name_character =
        |character: char| character.is_ascii_alphanumeric() || character == '_' || character == '$';
    declaration
        .split(|character: char| !is_name_character(character))
        .next()
        .unwrap_or("")
}

fn strip_field_modifier(declaration: &str) -> Option<&str> {
    for modifier in EventResource::FIELD_MODIFIERS {
        if let Some(rest) = declaration.strip_prefix(modifier) {
            return Some(rest);
        }
    }
    None
}

fn scan(source: &str, language: &str) -> EventScan {
    let mask = crate::lexical::code_mask(language, source);
    let mut code = masked_code(source, &mask.code);
    let mut markers = Vec::new();
    for mut span in trimmed_lines(source) {
        if language == "go" && source[span.clone()].starts_with("// @") {
            span.start += 3;
        }
        if !mask.code.get(span.start).copied().unwrap_or(false) {
            continue;
        }
        let line = &source[span.clone()];
        if language == "rb" && !line.starts_with("#[") {
            continue;
        }
        if let Some((name, arguments)) = annotation(line) {
            markers.push(Marker {
                span: span.clone(),
                name: name.to_owned(),
                arguments: arguments.to_owned(),
            });
        }
        if is_annotation_line(line, language) {
            blank_line(&mut code[span]);
        }
    }
    EventScan {
        code,
        markers,
        error: mask.error,
    }
}

fn is_annotation_line(line: &str, language: &str) -> bool {
    line.starts_with('@') || line.starts_with("#[") || (language == "cs" && line.starts_with('['))
}

fn is_modifier(name: &str) -> bool {
    matches!(name, "Order" | "InBackground")
}

fn record_modifier(
    path: &Path,
    marker: &Marker,
    modifiers: &mut ListenerModifiers,
) -> Result<(), Failure> {
    let at = marker.span.start;
    if marker.name == "Order" {
        return record_order(path, &marker.arguments, modifiers, at);
    }
    if !marker.arguments.trim().is_empty() {
        return Err(failure(
            path,
            at,
            crate::annotation_rules::AnnotationRules::no_arguments("InBackground"),
        ));
    }
    if modifiers.asynchronous {
        return Err(failure(path, at, "One InBackground per listener"));
    }
    modifiers.asynchronous = true;
    Ok(())
}

fn record_order(
    path: &Path,
    arguments: &str,
    modifiers: &mut ListenerModifiers,
    at: usize,
) -> Result<(), Failure> {
    let Some(order) = parse_order(arguments) else {
        return Err(failure(path, at, "Order takes one integer literal"));
    };
    if modifiers.order.is_some() {
        return Err(failure(path, at, "One Order per listener"));
    }
    modifiers.order = Some(order);
    Ok(())
}

fn parse_order(arguments: &str) -> Option<i64> {
    let order = arguments.trim().parse::<i64>().ok()?;
    let limit = EventListener::MAX_ORDER;
    if !(-limit..=limit).contains(&order) {
        return None;
    }
    Some(order)
}

fn read_markers(
    context: &SourceContext,
    markers: Vec<Marker>,
    output: &mut EventModule,
) -> Result<ModifiersByMethod, Failure> {
    let path = context.path;
    let mut annotated_members = BTreeSet::new();
    let mut modifiers = ModifiersByMethod::new();
    for marker in markers {
        let member = member_start(path, context.code, &marker)?;
        reject_retired_annotation(path, &marker)?;
        let is_field = marker.name == "AutowiredResource";
        let method =
            crate::behavior::event_target(path, context.source, context.code, member, is_field)?;
        validate_method(path, context.code, &method, marker.span.start)?;
        let key = (method.owner.clone(), method.name.clone());
        if is_modifier(&marker.name) {
            let entry = modifiers.entry(key).or_insert(ListenerModifiers {
                order: None,
                asynchronous: false,
                at: marker.span.start,
            });
            record_modifier(path, &marker, entry)?;
        } else {
            if !annotated_members.insert(key) {
                return Err(failure(
                    path,
                    marker.span.start,
                    "Duplicate or conflicting event annotations on one member",
                ));
            }
            let event_type = event_type_mut(&mut output.types, &method.owner);
            apply(context, event_type, &method, &marker)?;
        }
        let stays_for_the_client =
            matches!(marker.name.as_str(), "StreamRequest" | "SocketRequest");
        remember_method(output, method);
        if !stays_for_the_client {
            blank_marker(output, &marker.span);
        }
    }
    Ok(modifiers)
}

fn member_start(path: &Path, code: &str, marker: &Marker) -> Result<usize, Failure> {
    let after_marker = &code[marker.span.end..];
    let Some(distance) = after_marker.find(|character: char| !character.is_whitespace()) else {
        return Err(failure(
            path,
            marker.span.start,
            "Event annotation has no member",
        ));
    };
    Ok(marker.span.end + distance)
}

fn reject_retired_annotation(path: &Path, marker: &Marker) -> Result<(), Failure> {
    let at = marker.span.start;
    if marker.name == "EventPublisher" {
        return Err(failure(
            path,
            at,
            "EventPublisher is removed (ADR 0031): declare an applicationEventPublisher field and call publishEvent(event, value)",
        ));
    }
    if marker.name == "Resource" {
        let name = retired_resource_name(&marker.arguments);
        return Err(failure(
            path,
            at,
            format!("Resource is retired (ADR 0038): write @Needs on a field named {name}"),
        ));
    }
    Ok(())
}

fn retired_resource_name(arguments: &str) -> String {
    let parsed = crate::behavior::literal::parse(arguments).ok();
    let name_value = parsed.as_ref().and_then(|value| value.get("name"));
    let name = name_value.and_then(Value::as_str).unwrap_or("<resource>");
    let current_name = EventResource::new_name(name).unwrap_or(name);
    current_name.to_owned()
}

fn event_type_mut<'a>(types: &'a mut Vec<EventType>, owner: &str) -> &'a mut EventType {
    let position = types.iter().position(|event_type| event_type.name == owner);
    if let Some(index) = position {
        return &mut types[index];
    }
    types.push(EventType {
        name: owner.to_owned(),
        ..EventType::default()
    });
    let last = types.len() - 1;
    &mut types[last]
}

fn remember_method(output: &mut EventModule, method: EventMethod) {
    let is_known = output
        .methods
        .iter()
        .any(|item| item.owner == method.owner && item.name == method.name);
    if !is_known {
        output.methods.push(method);
    }
}

fn blank_marker(output: &mut EventModule, span: &Range<usize>) {
    let spaces = " ".repeat(span.len());
    output.source.replace_range(span.clone(), &spaces);
}

fn finish(
    context: &SourceContext,
    modifiers: ModifiersByMethod,
    output: &mut EventModule,
) -> Result<(), Failure> {
    let path = context.path;
    for ((owner, method), modifier) in modifiers {
        let Some(listener) = find_listener(output, &owner, &method) else {
            return Err(failure(
                path,
                modifier.at,
                "Order and InBackground apply to OnEvent methods",
            ));
        };
        listener.order = modifier.order.unwrap_or(0);
        listener.asynchronous = modifier.asynchronous;
    }
    if is_tac_script(context.environment, context.language) {
        implicit_publisher_resources(context, output);
    }
    if distinct_event_names(output).len() > EventModule::MAX_EVENT_NAMES {
        return Err(failure(
            path,
            0,
            "Local event modules permit at most 128 names",
        ));
    }
    Ok(())
}

fn name_publications(path: &Path, language: &str, output: &mut EventModule) -> Result<(), Failure> {
    use publications::Publications;
    if !matches!(language, "js" | "ts") {
        return Publications::refuse_names(language, &output.source)
            .map_err(|(at, message)| failure(path, at, message));
    }
    output.source = Publications::name_events(language, &output.source)
        .map_err(|(at, message)| failure(path, at, message))?;
    Ok(())
}

fn find_listener<'a>(
    output: &'a mut EventModule,
    owner: &str,
    method: &str,
) -> Option<&'a mut EventListener> {
    let event_type = output
        .types
        .iter_mut()
        .find(|event_type| event_type.name == owner)?;
    event_type
        .listeners
        .iter_mut()
        .find(|listener| listener.method == method)
}

fn distinct_event_names(module: &EventModule) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    for event_type in &module.types {
        for listener in &event_type.listeners {
            for event in &listener.events {
                names.insert(event.as_str());
            }
        }
    }
    names
}

pub(crate) fn extract(
    path: &Path,
    source: &str,
    environment: &str,
) -> Result<Option<EventModule>, Failure> {
    let language = language_of(path);
    let EventScan {
        code,
        mut markers,
        error,
    } = scan(source, language);
    if is_tac_script(environment, language) {
        markers.extend(autowired_resources(path, source, &code)?);
        markers.sort_by_key(|marker| marker.span.start);
    }
    let publishes = mentions_publisher(&String::from_utf8_lossy(&code));
    if markers.is_empty() && !publishes {
        return Ok(None);
    }
    let is_within_bounds =
        source.len() <= EventModule::MAX_SOURCE_BYTES && markers.len() <= EventModule::MAX_MARKERS;
    let is_known_realm = matches!(environment, "tac" | "yon");
    if !is_within_bounds || !is_known_realm {
        return Err(failure(
            path,
            0,
            "Application event source exceeds its bounds or has an invalid realm",
        ));
    }
    if let Some(error) = error {
        return Err(failure(path, 0, error));
    }
    let code = String::from_utf8(code).map_err(|_| failure(path, 0, "Invalid event source"))?;
    let context = SourceContext {
        path,
        source,
        code: &code,
        environment,
        language,
    };
    let mut output = EventModule {
        source: source.to_owned(),
        types: Vec::new(),
        methods: Vec::new(),
        topics: Vec::new(),
    };
    let modifiers = read_markers(&context, markers, &mut output)?;
    finish(&context, modifiers, &mut output)?;
    name_publications(path, language, &mut output)?;
    Ok(Some(output))
}

fn validate_method(
    path: &Path,
    code: &str,
    method: &EventMethod,
    at: usize,
) -> Result<(), Failure> {
    if is_constructor(method) {
        return Err(failure(
            path,
            at,
            "Use a separate instance lifecycle method; event annotations cannot decorate constructors",
        ));
    }
    if uses_yield(&code[method.body.clone()]) {
        return Err(failure(
            path,
            at,
            "Event and lifecycle methods cannot be generators",
        ));
    }
    Ok(())
}

fn is_constructor(method: &EventMethod) -> bool {
    let constructor_names = ["constructor", "__init__", "__construct", "init"];
    constructor_names.contains(&method.name.as_str()) || method.name == method.owner
}

fn uses_yield(body: &str) -> bool {
    body.split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|word| word == "yield")
}

fn annotation(line: &str) -> Option<(&'static str, &str)> {
    let without_opening = line
        .strip_prefix("#[")
        .or_else(|| line.strip_prefix('['))
        .or_else(|| line.strip_prefix('@'))?;
    let text = without_opening.strip_suffix(']').unwrap_or(without_opening);
    let (written_name, arguments) = text.split_once('(').unwrap_or((text, ""));
    let last_segment = written_name.trim().rsplit(['.', ':']).next()?;
    let normalized = last_segment.replace('_', "").to_ascii_lowercase();
    let name = match normalized.as_str() {
        "eventpublisher" => "EventPublisher",
        "onevent" => "OnEvent",
        "streamrequest" => "StreamRequest",
        "socketrequest" => "SocketRequest",
        "onstart" => "OnStart",
        "onstop" => "OnStop",
        "resource" => "Resource",
        "order" => "Order",
        "inbackground" => "InBackground",
        _ => return None,
    };
    if arguments.is_empty() && !text.contains('(') {
        return Some((name, ""));
    }
    let arguments = arguments
        .strip_suffix(')')
        .unwrap_or("malformed annotation arguments");
    Some((name, arguments))
}

pub(crate) fn relocate(
    path: &Path,
    source: &str,
    module: &EventModule,
) -> Result<EventModule, Failure> {
    let mask = crate::lexical::code_mask(language_of(path), source);
    let code: String = masked_code(source, &mask.code)
        .into_iter()
        .map(char::from)
        .collect();
    let mut relocated = module.clone();
    source.clone_into(&mut relocated.source);
    relocated.methods.clear();
    for original in &module.methods {
        let is_field = is_resource_field(module, original);
        let found = find_relocated_method(path, source, &code, original, is_field)?;
        let Some(method) = found else {
            return Err(failure(
                path,
                original.signature.start,
                "Event method was not retained by source lowering",
            ));
        };
        relocated.methods.push(method);
    }
    Ok(relocated)
}

fn is_resource_field(module: &EventModule, method: &EventMethod) -> bool {
    module.types.iter().any(|event_type| {
        event_type.name == method.owner
            && event_type
                .resources
                .iter()
                .any(|resource| resource.field == method.name)
    })
}

fn find_relocated_method(
    path: &Path,
    source: &str,
    code: &str,
    original: &EventMethod,
    is_field: bool,
) -> Result<Option<EventMethod>, Failure> {
    let mut found = None;
    let mut offset = 0;
    for line in code.split_inclusive('\n') {
        let start = offset + line.len() - line.trim_start().len();
        offset += line.len();
        if !line.contains(&original.name) {
            continue;
        }
        let has_parenthesis = line.contains('(') || line.trim_start().starts_with("def ");
        if is_field == has_parenthesis {
            continue;
        }
        let Ok(method) = crate::behavior::event_target(path, source, code, start, is_field) else {
            continue;
        };
        if method.name != original.name || method.owner != original.owner {
            continue;
        }
        if found.is_some() {
            return Err(failure(
                path,
                start,
                "Overloaded event methods are unsupported",
            ));
        }
        found = Some(method);
    }
    Ok(found)
}

fn is_listener_id(id: &str) -> bool {
    if id.is_empty() || id.len() > EventListener::MAX_ID_BYTES {
        return false;
    }
    id.bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn listener_replay(path: &Path, object: &Map<String, Value>, at: usize) -> Result<bool, Failure> {
    match object.get("replay") {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(failure(path, at, "OnEvent.replay must be a boolean")),
    }
}

fn listener_condition(
    path: &Path,
    object: &Map<String, Value>,
    at: usize,
) -> Result<Option<Value>, Failure> {
    match object.get("condition") {
        None => Ok(None),
        Some(Value::String(source)) => {
            let compiled = condition::compile(source)
                .map_err(|message| failure(path, at, format!("OnEvent.condition: {message}")))?;
            Ok(Some(compiled))
        }
        _ => Err(failure(path, at, "OnEvent.condition must be a string")),
    }
}

fn listener_id(
    path: &Path,
    object: &Map<String, Value>,
    at: usize,
) -> Result<Option<String>, Failure> {
    match object.get("id") {
        None => Ok(None),
        Some(Value::String(id)) if is_listener_id(id) => Ok(Some(id.clone())),
        _ => Err(failure(
            path,
            at,
            "OnEvent.id must be a short literal identifier",
        )),
    }
}

fn apply(
    context: &SourceContext,
    event_type: &mut EventType,
    method: &EventMethod,
    marker: &Marker,
) -> Result<(), Failure> {
    let path = context.path;
    let name = marker.name.as_str();
    let at = marker.span.start;
    if matches!(name, "OnStart" | "OnStop") {
        return lifecycle(path, event_type, method, marker);
    }
    if name == "AutowiredResource" {
        let object = metadata_object(path, &marker.arguments, at)?;
        return add_autowired_resource(context, event_type, method, &object, at);
    }
    if matches!(name, "StreamRequest" | "SocketRequest") {
        let stream = stream_request(context, method, marker)?;
        if stream.socket && event_type.streams.iter().any(|other| other.socket) {
            return Err(failure(
                path,
                at,
                "An @HTTPClient class has at most one @SocketRequest method, because its send(message) method writes to that one socket",
            ));
        }
        event_type.streams.push(stream);
        return Ok(());
    }
    let object = listener_object(path, &marker.arguments, at)?;
    check_one_parameter(path, method, at)?;
    let listener = typed_listener(context, method, &object, at)?;
    event_type.listeners.push(listener);
    Ok(())
}

pub(crate) struct ListenerPayload {
    pub name: String,
    pub type_text: String,
    pub signature: Range<usize>,
}

pub(crate) fn listener_payload(
    path: &Path,
    source: &str,
    after_marker: usize,
) -> Option<ListenerPayload> {
    let language = language_of(path);
    let code = String::from_utf8(scan(source, language).code).ok()?;
    let mut at = after_marker;
    loop {
        let rest = code.get(at..)?;
        at += rest.find(|character: char| !character.is_whitespace())?;
        if language != "rb" || !code[at..].starts_with("#[") {
            break;
        }
        at += code[at..].find('\n')?;
    }
    let method = crate::behavior::event_target(path, source, &code, at, false).ok()?;
    let parameter = method.parameters.first()?;
    let type_text = if language == "js" {
        let documentation = documentation_before(source, method.signature.start);
        RouteTypes::documented_type(documentation, "@param", &parameter.name)
    } else {
        parameter.type_source.clone()
    };
    Some(ListenerPayload {
        name: parameter.name.clone(),
        type_text,
        signature: method.signature.clone(),
    })
}

fn check_one_parameter(path: &Path, method: &EventMethod, at: usize) -> Result<(), Failure> {
    if method.parameters.len() == 1 {
        return Ok(());
    }
    Err(failure(
        path,
        at,
        "OnEvent methods take exactly one payload parameter",
    ))
}

fn typed_listener(
    context: &SourceContext,
    method: &EventMethod,
    object: &Map<String, Value>,
    at: usize,
) -> Result<EventListener, Failure> {
    let path = context.path;
    if object.contains_key("event") || object.contains_key("events") {
        return Err(failure(path, at, EventListener::NAMED_EVENT_REFUSAL));
    }
    let has_known_keys = object
        .keys()
        .all(|key| EventListener::OPTION_KEYS.contains(&key.as_str()));
    if !has_known_keys {
        return Err(failure(
            path,
            at,
            "OnEvent takes only replay, condition and id",
        ));
    }
    let model = payload_model(context, method, at, "OnEvent listens by")?;
    Ok(EventListener {
        method: method.name.clone(),
        events: vec![model],
        replay: listener_replay(path, object, at)?,
        condition: listener_condition(path, object, at)?,
        id: listener_id(path, object, at)?,
        order: 0,
        asynchronous: false,
        models: None,
        returns: returned_model(context, method),
    })
}

fn stream_request(
    context: &SourceContext,
    method: &EventMethod,
    marker: &Marker,
) -> Result<EventStream, Failure> {
    let path = context.path;
    let at = marker.span.start;
    let written = marker.name.as_str();
    let socket = written == "SocketRequest";
    let (holds, example) = if socket {
        ("a WebSocket", "receive(message: ChatMessage)")
    } else {
        ("a stream of server-sent events", "streamTicks(tick: Tick)")
    };
    if !BrowserStreams::held_by(context.language, context.environment) {
        let refusal = BrowserStreams::refusal(written, context.language, context.environment);
        return Err(failure(path, at, refusal));
    }
    if method.parameters.len() != 1 {
        return Err(failure(
            path,
            at,
            format!(
                "A @{written} method takes exactly one parameter: the message. Its model type says what each message holds, for example {example}"
            ),
        ));
    }
    let Some(base) =
        crate::behavior::HttpClientSource::class_path(path, context.source, &method.owner)?
    else {
        return Err(failure(
            path,
            at,
            format!(
                "@{written} keeps {holds} open, so its class needs @HTTPClient(\"/path\") to say where it is"
            ),
        ));
    };
    let text = marker.arguments.trim();
    let quoted = text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')));
    if !text.is_empty() && !quoted {
        return Err(failure(
            path,
            at,
            format!(
                "@{written} takes an optional path in quotes, like @{written}(\"/ticks\"). It is added to the @HTTPClient path"
            ),
        ));
    }
    let own = if quoted { &text[1..text.len() - 1] } else { "" };
    let mut full = format!("{base}{own}");
    if full.is_empty() {
        full.push('/');
    }
    Ok(EventStream {
        method: method.name.clone(),
        path: full,
        event: payload_model(
            context,
            method,
            at,
            &format!("Each @{written} message is checked against"),
        )?,
        socket,
        models: None,
        topic: false,
    })
}

fn payload_model(
    context: &SourceContext,
    method: &EventMethod,
    at: usize,
    reads: &str,
) -> Result<String, Failure> {
    let parameter = &method.parameters[0];
    let type_text = if context.language == "js" {
        let documentation = documentation_before(context.source, method.signature.start);
        RouteTypes::documented_type(documentation, "@param", &parameter.name)
    } else {
        parameter.type_source.clone()
    };
    if let Some(model) = model_name(context.language, &type_text) {
        return Ok(model);
    }
    let hint = if context.language == "js" {
        "add a JSDoc line such as `@param {OrderPlaced} order` above the method"
    } else {
        "give the parameter a model type, for example placed(OrderPlaced order)"
    };
    Err(failure(
        context.path,
        at,
        format!(
            "{reads} the model type of its one parameter, and '{}' is not a model: {hint}",
            type_text.trim()
        ),
    ))
}

fn returned_model(context: &SourceContext, method: &EventMethod) -> Option<String> {
    let type_text = match context.language {
        "js" => {
            let documentation = documentation_before(context.source, method.signature.start);
            RouteTypes::documented_type(documentation, "@returns", "")
        }
        "py" => python_return_type(context.source, method),
        _ => method.return_type.clone(),
    };
    let shape = TypeText::shape(context.language, type_text.trim().trim_start_matches('&')).ok()?;
    model_inside(&shape)
}

fn python_return_type(source: &str, method: &EventMethod) -> String {
    let signature = source.get(method.signature.clone()).unwrap_or("");
    let Some((_, declared)) = signature.rsplit_once("->") else {
        return String::new();
    };
    declared.trim().trim_end_matches(':').trim().to_owned()
}

fn model_inside(shape: &Shape) -> Option<String> {
    match shape {
        Shape::Model(name) => Some(name.clone()),
        Shape::List(inner) | Shape::Optional(inner) => model_inside(inner),
        _ => None,
    }
}

fn model_name(language: &str, type_text: &str) -> Option<String> {
    let text = type_text.trim().trim_start_matches('&');
    match TypeText::shape(language, text) {
        Ok(Shape::Model(name)) => Some(name),
        _ => None,
    }
}

fn documentation_before(source: &str, at: usize) -> &str {
    let mut end = at;
    loop {
        let before = source[..end].trim_end();
        if let Some(without_end) = before.strip_suffix("*/") {
            let Some(start) = without_end.rfind("/**") else {
                return "";
            };
            return &without_end[start + 3..];
        }
        let line_start = before.rfind('\n').map_or(0, |position| position + 1);
        if !before[line_start..].trim_start().starts_with('@') {
            return "";
        }
        end = line_start;
    }
}

fn listener_object(path: &Path, arguments: &str, at: usize) -> Result<Map<String, Value>, Failure> {
    if arguments.trim().is_empty() {
        return Ok(Map::new());
    }
    metadata_object(path, arguments, at)
}

fn metadata_object(path: &Path, arguments: &str, at: usize) -> Result<Map<String, Value>, Failure> {
    let Ok(value) = crate::behavior::literal::parse(arguments) else {
        return Err(failure(
            path,
            at,
            "Event metadata requires a literal JSON object with unique keys",
        ));
    };
    let Value::Object(object) = value else {
        return Err(failure(path, at, "Event metadata requires an object"));
    };
    Ok(object)
}

fn add_autowired_resource(
    context: &SourceContext,
    event_type: &mut EventType,
    method: &EventMethod,
    object: &Map<String, Value>,
    at: usize,
) -> Result<(), Failure> {
    let resource = object.get("name").and_then(Value::as_str).unwrap_or("");
    let is_supplied = is_tac_script(context.environment, context.language)
        && EventResource::TAC_NAMES.contains(&resource);
    if !is_supplied {
        return Err(failure(
            context.path,
            at,
            "Needs names a resource this realm does not supply",
        ));
    }
    event_type.resources.push(EventResource {
        field: method.name.clone(),
        name: resource.to_owned(),
    });
    Ok(())
}

fn mentions_publisher(code: &str) -> bool {
    [
        "applicationEventPublisher",
        "ApplicationEventPublisher",
        "application_event_publisher",
    ]
    .iter()
    .any(|name| code.contains(name))
}

fn declares_publisher_field(line: &str) -> bool {
    let Some(rest) = line.trim().strip_prefix(EventResource::PUBLISHER) else {
        return false;
    };
    if rest.contains('=') || rest.contains('(') {
        return false;
    }
    let after_name = rest.trim_start();
    after_name.is_empty() || after_name.starts_with([';', '!', ':'])
}

fn implicit_publisher_resources(context: &SourceContext, output: &mut EventModule) {
    let mut offset = 0;
    for line in context.code.split_inclusive('\n') {
        let start = offset + line.len() - line.trim_start().len();
        offset += line.len();
        if !declares_publisher_field(line) {
            continue;
        }
        let target =
            crate::behavior::event_target(context.path, context.source, context.code, start, true);
        let Ok(field) = target else {
            continue;
        };
        if field.name != EventResource::PUBLISHER {
            continue;
        }
        let event_type = event_type_mut(&mut output.types, &field.owner);
        let is_known = event_type
            .resources
            .iter()
            .any(|resource| resource.field == field.name);
        if !is_known {
            event_type.resources.push(EventResource {
                field: field.name.clone(),
                name: String::from(EventResource::PUBLISHER),
            });
        }
    }
}

fn lifecycle(
    path: &Path,
    event_type: &mut EventType,
    method: &EventMethod,
    marker: &Marker,
) -> Result<(), Failure> {
    let at = marker.span.start;
    if !marker.arguments.trim().is_empty() || !method.parameters.is_empty() {
        return Err(failure(
            path,
            at,
            "Lifecycle annotations and methods take no arguments",
        ));
    }
    let hook = if marker.name == "OnStart" {
        &mut event_type.post_construct
    } else {
        &mut event_type.pre_destroy
    };
    if hook.is_some() {
        return Err(failure(
            path,
            at,
            "Each type has at most one method per lifecycle hook",
        ));
    }
    *hook = Some(method.name.clone());
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tac(source: &str) -> Result<Option<EventModule>, Failure> {
        extract(Path::new("tac.ts"), source, "tac")
    }

    fn yon(source: &str) -> Result<Option<EventModule>, Failure> {
        extract(Path::new("yon.js"), source, "yon")
    }

    fn listener_source(options: &str) -> String {
        format!("class Page {{\n@OnEvent({options})\nreceive(value: Probe) {{}}\n}}")
    }

    fn tac_module(source: &str) -> EventModule {
        tac(source).expect("parse").expect("events")
    }

    fn first_listener(module: &EventModule) -> &EventListener {
        &module.types[0].listeners[0]
    }

    fn listener_summary(listener: &EventListener) -> (&str, i64, bool) {
        (
            listener.method.as_str(),
            listener.order,
            listener.asynchronous,
        )
    }

    #[test]
    fn typescript_owned_resources_accept_existing_instance_field_modifiers() {
        for declaration in [
            "documentTitle!: DocumentTitle;",
            "private documentTitle!: DocumentTitle;",
            "protected readonly documentTitle?: DocumentTitle;",
            "public documentTitle: DocumentTitle;",
            "declare documentTitle: DocumentTitle;",
        ] {
            let source = format!("class Page {{\n@Needs\n{declaration}\n}}");
            let module = tac_module(&source);
            assert_eq!(module.types[0].resources.len(), 1);
            assert_eq!(module.types[0].resources[0].field, "documentTitle");
            assert_eq!(module.types[0].resources[0].name, "documentTitle");
            assert!(module.source.contains(declaration), "{declaration}");
            assert!(!module.source.contains("@Needs"), "{declaration}");
        }
    }

    #[test]
    fn extracts_lifecycle_event_and_resource_metadata_without_shifting_source() {
        let source = "class Page {\n@Needs\nview;\n@OnStart\nstart() {}\n@OnEvent({\"replay\":true})\nreceive(value: ProbeReady) {}\n@OnStop\nstop() {}\n}";
        let module = tac_module(source);
        assert_eq!(module.source.len(), source.len());
        assert_eq!(module.types[0].name, "Page");
        assert_eq!(module.types[0].post_construct.as_deref(), Some("start"));
        assert_eq!(module.types[0].pre_destroy.as_deref(), Some("stop"));
        assert_eq!(module.types[0].resources[0].field, "view");
        assert_eq!(module.types[0].resources[0].name, "view");
        assert!(!module.source.contains("@Needs"));
        let listener = first_listener(&module);
        assert_eq!(listener.method, "receive");
        assert_eq!(listener.events, vec!["ProbeReady".to_owned()]);
        assert!(listener.replay);
        assert!(listener.condition.is_none());
        assert!(listener.id.is_none());
        assert_eq!(listener.order, 0);
        assert!(!listener.asynchronous);
    }

    #[test]
    fn event_publisher_is_removed() {
        let source = "class Page {\n@EventPublisher({\"event\":\"probe.ready\"})\nemit(value) { return value; }\n}";
        let error = tac(source).expect_err("removed").to_string();
        assert!(
            error.contains("EventPublisher is removed (ADR 0031): declare an applicationEventPublisher field and call publishEvent(event, value)"),
            "{error}"
        );
        assert!(yon(source).is_err());
    }

    #[test]
    fn listeners_listen_by_the_model_type_of_their_payload() {
        let module =
            tac_module("class Page {\n@OnEvent\nreceive(value: CartAdded): CartSaved {}\n}");
        let listener = first_listener(&module);
        assert_eq!(listener.events, vec!["CartAdded".to_owned()]);
        assert_eq!(listener.returns.as_deref(), Some("CartSaved"));
        let documented = extract(
            Path::new("tac.js"),
            "class Page {\n/**\n * @param {CartAdded} value\n * @returns {CartSaved[]}\n */\n@OnEvent()\nreceive(value) {}\n}",
            "tac",
        )
        .expect("parse")
        .expect("events");
        assert_eq!(
            first_listener(&documented).events,
            vec!["CartAdded".to_owned()]
        );
        assert_eq!(
            first_listener(&documented).returns.as_deref(),
            Some("CartSaved")
        );
        for options in ["{\"event\":\"x\"}", "{\"events\":[\"x\",\"y\"]}"] {
            let error = tac(&listener_source(options))
                .expect_err(options)
                .to_string();
            assert!(error.contains("OnEvent takes no event names"), "{error}");
        }
        let error = tac(&listener_source("{\"classes\":[\"y\"]}"))
            .expect_err("unknown")
            .to_string();
        assert!(
            error.contains("OnEvent takes only replay, condition and id"),
            "{error}"
        );
        for member in [
            "receive(value) {}",
            "receive(value: string) {}",
            "receive(value: CartAdded[]) {}",
        ] {
            let source = format!("class Page {{\n@OnEvent\n{member}\n}}");
            let error = tac(&source).expect_err(member).to_string();
            assert!(error.contains("is not a model"), "{member}: {error}");
        }
        let untyped = extract(
            Path::new("tac.js"),
            "class Page {\n@OnEvent\nreceive(value) {}\n}",
            "tac",
        )
        .expect_err("untyped")
        .to_string();
        assert!(untyped.contains("@param {OrderPlaced} order"), "{untyped}");
    }

    #[test]
    fn a_ruby_listener_reads_its_event_from_the_yard_parameter_type() {
        let module = extract(
            Path::new("tac.rb"),
            "class Page\n  # @param order [OrderPlaced]\n  # @return [OrderSynced]\n  #[OnEvent]\n  def receive(order)\n  end\nend\n",
            "tac",
        )
        .expect("parse")
        .expect("events");
        let listener = first_listener(&module);
        assert_eq!(listener.events, vec!["OrderPlaced".to_owned()]);
        assert_eq!(listener.returns.as_deref(), Some("OrderSynced"));
        let named = extract(
            Path::new("tac.rb"),
            "class Page\n  #[OnEvent({\"event\":\"probe.ready\"})]\n  def receive(value)\n  end\nend\n",
            "tac",
        )
        .expect_err("string event names are refused")
        .to_string();
        assert!(named.contains("OnEvent takes no event names"), "{named}");
    }

    #[test]
    fn listener_models_come_from_the_model_table() {
        let root = tempfile::tempdir().expect("root");
        let models = root.path().join("client/models");
        std::fs::create_dir_all(&models).expect("models");
        std::fs::write(
            models.join("Orders.ts"),
            "export interface OrderPlaced {\n  id: string\n  total: number\n  note?: string\n  lines: Line[]\n}\nexport interface Line {\n  sku: string\n  count: integer\n}\n",
        )
        .expect("write");
        let path = Path::new("client/services/Orders.ts");
        let source = "class Orders {\n@OnEvent\nplaced(order: OrderPlaced): Receipt {}\n}";
        let mut module = extract(path, source, "tac")
            .expect("parse")
            .expect("events");
        describe(&mut module, path, root.path(), "client").expect("described");
        let listener = first_listener(&module);
        assert_eq!(
            listener.models,
            Some(json!({
                "OrderPlaced": {"open": false, "fields": {"id": "!string", "total": "!number", "note": "string", "lines": "![]Line"}},
                "Line": {"open": false, "fields": {"sku": "!string", "count": "!integer"}},
            }))
        );
        assert!(listener.returns.is_none());
        let missing = "class Orders {\n@OnEvent\nplaced(order: Missing) {}\n}";
        let mut module = extract(path, missing, "tac")
            .expect("parse")
            .expect("events");
        let error = describe(&mut module, path, root.path(), "client")
            .expect_err("missing")
            .to_string();
        assert!(error.contains("'Missing' is not a model"), "{error}");
    }

    #[test]
    fn a_javascript_listener_reads_models_from_the_models_folder_in_any_realm_language() {
        let root = tempfile::tempdir().expect("root");
        let models = root.path().join("client/models");
        std::fs::create_dir_all(&models).expect("models");
        std::fs::write(
            models.join("PlatformChanged.ts"),
            "export interface PlatformChanged {\n  platform: string\n}\n",
        )
        .expect("write");
        std::fs::write(
            models.join("CartTotal.js"),
            "/**\n * @typedef {Object} CartTotal\n * @property {number} total\n */\nexport {}\n",
        )
        .expect("write");
        let path = Path::new("client/services/SnippetService.js");
        for (source, model, fields) in [
            (
                "/** @import { PlatformChanged } from '/models/PlatformChanged.ts' */\nclass SnippetService {\n/** @param {PlatformChanged} value */\n@OnEvent\nchanged(value) {}\n}",
                "PlatformChanged",
                json!({"platform": "!string"}),
            ),
            (
                "class SnippetService {\n/** @param {CartTotal} value */\n@OnEvent\nchanged(value) {}\n}",
                "CartTotal",
                json!({"total": "!number"}),
            ),
        ] {
            let mut module = extract(path, source, "tac")
                .expect("parse")
                .expect("events");
            describe(&mut module, path, root.path(), "client").expect(model);
            assert_eq!(
                first_listener(&module).models,
                Some(json!({model: {"open": false, "fields": fields}}))
            );
        }
    }

    #[test]
    fn listener_condition_compiles_into_a_matcher() {
        let module = tac_module(&listener_source(
            "{\"condition\":\"value.status == 'paid' && value.total >= 10\"}",
        ));
        let condition = first_listener(&module)
            .condition
            .clone()
            .expect("condition");
        assert_eq!(
            condition,
            json!({
                "k": "and",
                "l": {"k": "cmp", "op": "eq", "l": {"k": "value", "path": ["status"]}, "r": {"k": "lit", "v": "paid"}},
                "r": {"k": "cmp", "op": "ge", "l": {"k": "value", "path": ["total"]}, "r": {"k": "lit", "v": 10}},
            })
        );
        let module = tac_module(&listener_source("{\"condition\":\"event == 'Probe'\"}"));
        let condition = first_listener(&module)
            .condition
            .clone()
            .expect("condition");
        assert_eq!(condition["l"], json!({"k": "event"}));
        for condition in [
            "value.total()",
            "other.total > 2",
            "value.total + 1 > 2",
            "",
        ] {
            let options = format!("{{\"condition\":\"{condition}\"}}");
            let error = tac(&listener_source(&options))
                .expect_err(condition)
                .to_string();
            assert!(error.contains("OnEvent.condition"), "{condition}: {error}");
        }
        assert!(tac(&listener_source("{\"condition\":true}")).is_err());
    }

    #[test]
    fn listener_id_is_a_short_identifier() {
        let module = tac_module(&listener_source("{\"id\":\"cart-listener.1\"}"));
        assert_eq!(
            first_listener(&module).id.as_deref(),
            Some("cart-listener.1")
        );
        for id in [
            "\"\"",
            "\"has space\"",
            "7",
            &format!("\"{}\"", "a".repeat(65)),
        ] {
            let options = format!("{{\"id\":{id}}}");
            assert!(tac(&listener_source(&options)).is_err(), "{id}");
        }
    }

    #[test]
    fn order_and_async_modify_listeners() {
        let source = "class Page {\n@OnEvent\n@Order(-5)\nfirst(value: Probe) {}\n@Order(2)\n@InBackground()\n@OnEvent\nasync later(value: Probe) {}\n@OnEvent\nplain(value: Probe) {}\n}";
        let module = tac_module(source);
        assert_eq!(module.source.len(), source.len());
        assert!(!module.source.contains("@Order") && !module.source.contains("@InBackground"));
        let listeners = &module.types[0].listeners;
        assert_eq!(listeners.len(), 3);
        assert_eq!(listener_summary(&listeners[0]), ("first", -5, false));
        assert_eq!(listener_summary(&listeners[1]), ("later", 2, true));
        assert_eq!(listener_summary(&listeners[2]), ("plain", 0, false));
        assert_eq!(module.methods.len(), 3);
        let bare = tac_module("class Page {\n@InBackground\n@OnEvent\nreceive(value: Probe) {}\n}");
        assert!(first_listener(&bare).asynchronous);
    }

    #[test]
    fn order_and_async_require_a_listener_and_literal_arguments() {
        for (member, message) in [
            (
                "@Order(1)\nreceive(value) {}",
                "Order and InBackground apply to OnEvent methods",
            ),
            (
                "@InBackground()\nreceive(value) {}",
                "Order and InBackground apply to OnEvent methods",
            ),
            (
                "@Order(1)\n@OnStart()\nstart() {}",
                "Order and InBackground apply to OnEvent methods",
            ),
            (
                "@Order(\"1\")\n@OnEvent\nreceive(value: Probe) {}",
                "Order takes one integer literal",
            ),
            (
                "@Order()\n@OnEvent\nreceive(value: Probe) {}",
                "Order takes one integer literal",
            ),
            (
                "@Order(1000001)\n@OnEvent\nreceive(value: Probe) {}",
                "Order takes one integer literal",
            ),
            (
                "@Order(1)\n@Order(2)\n@OnEvent\nreceive(value: Probe) {}",
                "One Order per listener",
            ),
            (
                "@InBackground(true)\n@OnEvent\nreceive(value: Probe) {}",
                "InBackground accepts no arguments",
            ),
            (
                "@InBackground()\n@InBackground()\n@OnEvent\nreceive(value: Probe) {}",
                "One InBackground per listener",
            ),
        ] {
            let source = format!("class Page {{\n{member}\n}}");
            let error = tac(&source).expect_err(member).to_string();
            assert!(error.contains(message), "{member}: {error}");
        }
    }

    #[test]
    fn a_stream_client_outside_browser_script_names_its_language() {
        let cases = [
            (
                "client/services/TicksService.java",
                "tac",
                "@HTTPClient(\"/ticks\")\ninterface TicksClient {\n    @StreamRequest\n    void receive(Tick tick);\n}\n",
                "@StreamRequest is not available in Java Tac code; it works in JavaScript, TypeScript",
            ),
            (
                "client/services/ChatService.swift",
                "tac",
                "@HTTPClient(\"/chat\")\nprotocol ChatClient {\n    @SocketRequest\n    func receive(message: ChatMessage)\n}\n",
                "@SocketRequest is not available in Swift Tac code",
            ),
            (
                "server/controllers/TicksController.js",
                "yon",
                "class TicksClient {\n  @StreamRequest\n  receive(tick) {}\n}\n",
                "@StreamRequest is not used in Yon code",
            ),
        ];
        for (path, environment, source, expected) in cases {
            let refused = extract(Path::new(path), source, environment).expect_err("refused");
            assert!(refused.to_string().contains(expected), "{refused}");
        }
    }

    #[test]
    fn tac_detects_the_implicit_publisher_resource() {
        let source = "class Basket {\napplicationEventPublisher;\ncount = 0;\nadd(item: BasketItem) { this.applicationEventPublisher.publishEvent(item); }\n}";
        let module = tac(source).expect("parse").expect("module");
        assert_eq!(module.types.len(), 1);
        assert_eq!(module.types[0].name, "Basket");
        assert_eq!(module.types[0].resources.len(), 1);
        assert_eq!(
            module.types[0].resources[0].field,
            "applicationEventPublisher"
        );
        assert_eq!(
            module.types[0].resources[0].name,
            "applicationEventPublisher"
        );
        assert!(module.types[0].listeners.is_empty());
        let typed = extract(
            Path::new("tac.ts"),
            "class Basket {\napplicationEventPublisher!: ApplicationEventPublisher;\n}",
            "tac",
        )
        .expect("parse")
        .expect("module");
        assert_eq!(
            typed.types[0].resources[0].name,
            "applicationEventPublisher"
        );
        let initialised = tac("class Basket {\napplicationEventPublisher = null;\n}")
            .expect("parse")
            .expect("module");
        assert!(initialised.types.is_empty());
        let explicit = tac("class Basket {\n@Needs\napplicationEventPublisher;\n}")
            .expect("parse")
            .expect("module");
        assert_eq!(explicit.types[0].resources.len(), 1);
    }

    #[test]
    fn a_publisher_only_class_yields_an_empty_yon_module() {
        let module = yon("class Orders {\napplicationEventPublisher;\n/** @param {OrderPlaced} order */\nplace(order) { this.applicationEventPublisher.publishEvent(order); }\n}")
            .expect("parse")
            .expect("module");
        assert!(module.types.is_empty());
        assert!(module.methods.is_empty());
        assert!(
            module
                .source
                .contains("publishEvent(\"OrderPlaced\", order)")
        );
        let typed = extract(
            Path::new("yon.py"),
            "class Orders:\n    application_event_publisher: ApplicationEventPublisher\n    def place(self, order):\n        self.application_event_publisher.publish_event(order)\n",
            "yon",
        )
        .expect("parse")
        .expect("module");
        assert!(typed.types.is_empty());
        assert!(
            yon("class Orders {\nplace(order) { return order; }\n}")
                .expect("parse")
                .is_none()
        );
    }

    #[test]
    fn a_lua_class_that_only_publishes_keeps_its_event_runtime() {
        let source = "---@class Orders\nOrders = {\n  ---@type ApplicationEventPublisher\n  application_event_publisher = nil,\n}\n";
        let module =
            extract(Path::new("server/controllers/Orders.lua"), source, "yon").expect("parse");
        assert!(module.is_some());
    }

    #[test]
    fn autowired_fields_named_after_a_resource_receive_it() {
        for (extension, field) in [
            ("js", "lifecycle;"),
            ("js", "taskScheduler;"),
            ("ts", "windowControls?: TacWindowControls"),
            ("ts", "resourceLoader!: TacResourceLoader"),
        ] {
            let name = field
                .trim_start_matches("private readonly ")
                .split(|character: char| !character.is_ascii_alphanumeric())
                .next()
                .expect("name");
            let source = format!("class Orders {{\n@Needs()\n{field}\n}}");
            let module = extract(Path::new(&format!("tac.{extension}")), &source, "tac")
                .expect(name)
                .expect("module");
            assert_eq!(module.types[0].resources[0].name, name, "{extension}");
        }
        let collaborator = tac("class Orders {\n@Needs\nbasket;\n}").expect("parse");
        assert!(collaborator.is_none_or(|module| module.types.is_empty()));
    }

    #[test]
    fn the_removed_cache_manager_points_at_clear_cache() {
        let error = tac("class Orders {\n@Needs\ncacheManager;\n}")
            .expect_err("removed resource")
            .to_string();
        assert!(error.contains("@ClearCache"), "{error}");
    }

    #[test]
    fn resource_is_retired_and_old_names_point_at_spring_ones() {
        for (source, expected) in [
            (
                "class Page {\n@Resource({\"name\":\"element\"})\nroot;\n}",
                "write @Needs on a field named view",
            ),
            (
                "class Page {\n@Resource({\"name\":\"resourceLoader\"})\nresourceLoader;\n}",
                "field named resourceLoader",
            ),
        ] {
            let error = tac(source).expect_err(source).to_string();
            assert!(error.contains(expected), "{error}");
        }
        let error = yon("class Orders {\n@Resource({\"name\":\"redis\"})\nredis;\n}")
            .expect_err("retired")
            .to_string();
        assert!(error.contains("field named redisTemplate"), "{error}");
        for (old, renamed) in [
            ("signal", "lifecycle"),
            ("element", "view"),
            ("renderScheduler", "taskScheduler"),
        ] {
            let error = tac(&format!("class Page {{\n@Needs\n{old};\n}}"))
                .expect_err(old)
                .to_string();
            assert!(error.contains(&format!("now '{renamed}'")), "{error}");
        }
    }

    #[test]
    fn ignores_comment_and_string_examples() {
        let source = "const example = '@OnEvent({})';\n// @OnStart()\nclass Page {}";
        assert!(tac(source).expect("parse").is_none());
        let source = "// this.applicationEventPublisher\nconst hint = 'ApplicationEventPublisher';\nclass Page {}";
        assert!(tac(source).expect("parse").is_none());
    }

    #[test]
    fn bare_lifecycle_markers_and_duplicate_policy_keys() {
        let source = "class Page {\n@OnStart\nstart() {}\n@OnStop\nstop() {}\n}";
        assert!(tac(source).expect("bare hooks").is_some());
        let source =
            "class Page {\n@OnEvent({\"id\":\"x\",\"id\":\"y\"})\nreceive(value: Probe) {}\n}";
        assert!(tac(source).is_err());
    }

    #[test]
    fn invalid_targets_and_options_fail_closed() {
        for member in [
            "@OnStart()\nstart(value) {}",
            "@OnStart(1)\nstart() {}",
            "@OnStart()\nstatic start() {}",
            "@OnEvent\nlisten() {}",
            "@OnEvent({\"remote\":true})\nlisten(value: Probe) {}",
            "@OnEvent({\"replay\":1})\nlisten(value: Probe) {}",
            "@OnEvent\n*listen(value: Probe) {}",
            "@OnEvent\n@OnEvent\nlisten(value: Probe) {}",
            "@OnEvent\n@OnStart()\nlisten(value: Probe) {}",
            "@Needs\nview = null;",
        ] {
            let source = format!("class Page {{\n{member}\n}}");
            assert!(tac(&source).is_err(), "{member}");
        }
        assert!(
            extract(
                Path::new("tac.js"),
                "class Page {\n@OnStart()\nstart() {}\n}",
                "browser"
            )
            .is_err()
        );
    }

    #[test]
    fn native_frontends_preserve_explicit_parameter_types() {
        for (extension, source, ty) in [
            (
                "swift",
                "class Page {\n@OnEvent\nfunc receive(_ value: Probe) {}\n}",
                "Probe",
            ),
            (
                "kt",
                "class Page {\n@OnEvent\nfun receive(value: Probe) {}\n}",
                "Probe",
            ),
            (
                "cs",
                "class Page {\n[OnEvent]\nvoid Receive(Probe value) {}\n}",
                "Probe",
            ),
            (
                "rs",
                "struct Page {}\nimpl Page {\n#[tac::on_event]\nfn receive(&self, value: Probe) {}\n}",
                "Probe",
            ),
            (
                "go",
                "type Page struct{}\n\n// @OnEvent\nfunc (page *Page) Receive(value Probe) {}\n",
                "Probe",
            ),
        ] {
            let module = extract(Path::new(&format!("tac.{extension}")), source, "tac")
                .expect(extension)
                .expect("events");
            assert_eq!(
                module.methods[0].parameters[0].type_source, ty,
                "{extension}"
            );
        }
    }
}
