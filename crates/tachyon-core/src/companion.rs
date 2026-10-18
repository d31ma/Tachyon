use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::unicode_name::UnicodeName;
use std::collections::HashMap;
use std::fmt::Write as _;

mod constructor_field_policy;
mod csharp_browser_members;
#[cfg(test)]
mod csharp_browser_members_tests;
mod csharp_model_type;
mod csharp_task_members;
#[cfg(test)]
mod csharp_task_model_tests;
mod csharp_task_result;
mod csharp_task_shape;
mod parameter_text;
use constructor_field_policy::ConstructorFieldPolicy;
use csharp_browser_members::CsharpBrowserMembers;
pub(crate) use csharp_task_members::CsharpTaskMembers;
pub(crate) use csharp_task_result::CsharpTaskResult;
use parameter_text::ParameterText;
mod kotlin_header;
mod kotlin_member;
mod kotlin_members;
#[cfg(test)]
mod kotlin_members_tests;
pub(crate) use kotlin_header::KotlinHeader;
pub(crate) use kotlin_member::KotlinMember;
pub(crate) use kotlin_members::KotlinMembers;
pub(crate) mod go;
mod rust_collaborators;
mod rust_construction;
mod rust_model_type;
mod rust_task_member;

pub(crate) use csharp_model_type::CsharpModelType;
use rust_collaborators::RustCollaborators;
mod rust_graph;
use rust_construction::RustConstruction;
pub(crate) use rust_graph::RustGraph;
pub(crate) use rust_model_type::RustModelType;
use rust_task_member::RustTaskMember;

pub(crate) fn owner_name(
    language: CompanionLanguage,
    source: &str,
    source_path: &str,
) -> Option<String> {
    let wanted = declared_service(source_path, source);
    if language == CompanionLanguage::Rust {
        block_named(source, "struct", wanted.as_deref()).map(|(name, _)| name)
    } else {
        class_body_named(source, wanted.as_deref()).map(|(name, _)| name)
    }
}

fn exposes_nothing(source_path: &str, name: &str) -> Failure {
    Failure::one(diagnostic(
        1406,
        format!("Native service type '{name}' exposes no members."),
        Some(String::from(
            "Give the type a stored property or a method, or remove the service. A native \
             service reaches the page through its members; one with none has nothing to bind.",
        )),
        source_span(source_path, 0, source_path.len()),
    ))
}

fn unnamed_service(source_path: &str, wanted: Option<&str>, keyword: &str) -> Failure {
    let Some(wanted) = wanted else {
        return missing_type("Companion", source_path, "native");
    };
    Failure::one(diagnostic(
        1406,
        format!("Native service '{source_path}' declares no {keyword} named '{wanted}'."),
        Some(format!(
            "A service file owns the type named after it (ADR 0030). Name the {keyword} \
             '{wanted}', and keep helpers, entities and configuration types beside it."
        )),
        source_span(source_path, 0, source_path.len()),
    ))
}

fn authored_table(source_path: &str) -> Failure {
    Failure::one(diagnostic(
        1405,
        "Native companions require an owned class or struct, not an authored member table.",
        None,
        source_span(source_path, 0, source_path.len()),
    ))
}

fn missing_constructor(source_path: &str, name: &str) -> Failure {
    Failure::one(diagnostic(
        1406,
        format!("Companion struct '{name}' needs a constructor."),
        Some("Add fn new() -> Self or implement Default.".into()),
        source_span(source_path, 0, source_path.len()),
    ))
}

fn declared_service(source_path: &str, source: &str) -> Option<String> {
    let path = source_path.replace('\\', "/");
    let (parent, file) = path.rsplit_once('/')?;
    let role_parent = parent == "client/services"
        || parent.starts_with("client/services/")
        || parent.ends_with("/client/services")
        || parent.contains("/client/services/");
    if !role_parent {
        return None;
    }
    let stem = match file.rsplit_once('.') {
        Some((stem, _)) => stem,
        None => file,
    };
    if ![stem.ends_with("Service"), file.ends_with("_service.rs")].contains(&true) {
        return None;
    }
    Some(
        crate::annotation_layout::ClassFileName::matching_declaration(
            std::path::Path::new(&path),
            source,
        )
        .filter(|name| name.ends_with("Service"))
        .unwrap_or_else(|| stem.to_owned()),
    )
}

pub(crate) fn owner_table(
    language: CompanionLanguage,
    source: &str,
    source_path: &str,
    excluded: &[String],
    extra: &str,
    needs_all: bool,
) -> Result<String, Failure> {
    let source = crate::without_bom(source);
    if declares_table(language, source) {
        return Err(authored_table(source_path));
    }
    if language == CompanionLanguage::Rust {
        return rust_owner_table(source, source_path, excluded, extra, needs_all);
    }
    class_owner_table(language, source, source_path, excluded, extra, needs_all)
}

pub(crate) fn with_required_constructor(
    language: CompanionLanguage,
    source: &str,
    source_path: &str,
    needs_all: bool,
) -> String {
    let source = crate::without_bom(source);
    let wanted = declared_service(source_path, source);
    let Some((name, body)) = class_body_named(source, wanted.as_deref()) else {
        return source.to_owned();
    };
    let fields = constructed_fields(source, source_path, &name, needs_all);
    if fields.is_empty() {
        return source.to_owned();
    }
    let constructor = match language {
        CompanionLanguage::Swift => swift_constructor(&fields),
        CompanionLanguage::Kotlin => kotlin_constructor(&fields),
        CompanionLanguage::CSharp | CompanionLanguage::CSharpBrowser | CompanionLanguage::Rust => {
            return source.to_owned();
        }
    };
    let body_end = body.as_ptr() as usize - source.as_ptr() as usize + body.len();
    format!(
        "{}{constructor}{}",
        &source[..body_end],
        &source[body_end..]
    )
}

fn constructed_fields(
    source: &str,
    source_path: &str,
    class: &str,
    needs_all: bool,
) -> Vec<(String, String)> {
    let fields = crate::template::required_fields::RequiredFields::constructed(
        std::path::Path::new(source_path),
        source,
        class,
    );
    fields
        .into_iter()
        .filter(|(_, kind)| ConstructorFieldPolicy::accepts(source_path, kind, needs_all))
        .collect()
}

fn swift_constructor(fields: &[(String, String)]) -> String {
    let mut constructor = String::from("\ninit?(tacProps: [String: Any]) {\n");
    for (name, kind) in fields {
        let _ = writeln!(
            constructor,
            "guard let {name} = tachyonSwiftJSONValue(tacProps[\"{name}\"]) as? {kind} else {{ return nil }}\nself.{name} = {name}"
        );
    }
    constructor.push_str("}\n");
    constructor
}

fn kotlin_constructor(fields: &[(String, String)]) -> String {
    let mut constructor =
        String::from("\nconstructor(tacProps: Map<String, Any?>?, tacPage: TacPageScope) {\n");
    for (name, kind) in fields {
        let value = format!("tacRequiredValue(tacProps, \"{name}\")");
        let converted = match kind.as_str() {
            "DocumentTitle" => String::from("tacPage.documentTitle"),
            "Int" | "Long" | "Double" | "Float" | "Boolean" | "String" => {
                format!("{value}.let {{ {} }}", kotlin_value(kind, "it"))
            }
            _ => kotlin_value(kind, &value),
        };
        let _ = writeln!(constructor, "this.{name} = {converted}");
    }
    constructor.push_str("}\n");
    constructor
}

fn class_owner_table(
    language: CompanionLanguage,
    source: &str,
    source_path: &str,
    excluded: &[String],
    extra: &str,
    needs_all: bool,
) -> Result<String, Failure> {
    let wanted = declared_service(source_path, source);
    let Some((name, body)) = class_body_named(source, wanted.as_deref()) else {
        return Err(unnamed_service(source_path, wanted.as_deref(), "class"));
    };
    let constructs = !constructed_fields(source, source_path, &name, needs_all).is_empty();
    let members = members(language, body, source_path)?;
    let mut entries = String::new();
    for member in &members {
        if excluded.contains(&member.name) {
            continue;
        }
        entries.push_str(&entry(language, member));
    }
    entries.push_str(extra);
    if entries.trim().is_empty() {
        return Err(exposes_nothing(source_path, &name));
    }
    let required = if matches!(
        language,
        CompanionLanguage::CSharp | CompanionLanguage::CSharpBrowser
    ) {
        csharp_required_members(body)
    } else {
        Vec::new()
    };
    Ok(class_table(
        language, &name, &entries, &required, constructs,
    ))
}

fn csharp_required_members(body: &str) -> Vec<String> {
    body.lines()
        .filter(|line| !line.contains('('))
        .filter_map(|line| {
            let words = line.split_whitespace().collect::<Vec<_>>();
            let at = words.iter().position(|word| *word == "required")?;
            let name = UnicodeName::leading(words.get(at + 2)?);
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

fn class_table(
    language: CompanionLanguage,
    name: &str,
    entries: &str,
    required: &[String],
    constructs: bool,
) -> String {
    match language {
        CompanionLanguage::Swift => {
            let instance = if constructs {
                format!("guard let tacInstance = {name}(tacProps: tacProps) else {{ return nil }}")
            } else {
                format!("let tacInstance = {name}()")
            };
            format!(
                "\nstatic func tacCreate(_ tacProps: [String: Any]) -> [String: TacMember]? {{\n{instance}\nreturn [\n{entries}]\n}}\n"
            )
        }
        CompanionLanguage::Kotlin => {
            let arguments = if constructs { "tacProps, tacPage" } else { "" };
            format!(
                "\nfun tacCreate(tacProps: Map<String, Any?>?, tacPage: TacPageScope): Map<String, Any> {{\nval tacInstance = {name}({arguments})\nreturn linkedMapOf(\n{entries})\n}}\n"
            )
        }
        CompanionLanguage::CSharp | CompanionLanguage::CSharpBrowser => {
            let filled = required
                .iter()
                .map(|member| format!("{member} = default!"))
                .collect::<Vec<_>>();
            let initializer = if filled.is_empty() {
                String::new()
            } else {
                format!(" {{ {} }}", filled.join(", "))
            };
            format!(
                "\npublic static class TacBridge {{\npublic static Dictionary<string, TacMember> Create() {{\nvar Instance = new {name}(){initializer};\nreturn new() {{\n{entries}}};\n}}\n}}\n"
            )
        }
        CompanionLanguage::Rust => String::new(),
    }
}

fn rust_owner_table(
    source: &str,
    source_path: &str,
    excluded: &[String],
    extra: &str,
    needs_all: bool,
) -> Result<String, Failure> {
    let wanted = declared_service(source_path, source);
    let Some((name, body)) = block_named(source, "struct", wanted.as_deref()) else {
        return Err(unnamed_service(source_path, wanted.as_deref(), "struct"));
    };
    let mut members = rust_struct_fields(body);

    let collaborators = RustCollaborators::declared_beside(&name, source);
    members.retain(|member| !collaborators.holds(member.declared_type.as_deref()));
    let mut parameters = HashMap::new();
    for (header, impl_body) in blocks(source, "impl") {
        if header.trim() != name {
            continue;
        }
        for (member, declared) in rust_methods(impl_body) {
            parameters.insert(member.name.clone(), declared);
            members.push(member);
        }
    }
    let instance = RustConstruction::instance(source, source_path, &name, needs_all)?;
    let mut output = format!(
        "pub fn tac_create() -> Vec<(&'static str, TacMember)> {{\nlet instance = std::rc::Rc::new(core::cell::RefCell::new({instance}));\nvec![\n"
    );
    members.sort_by(|left, right| left.name.cmp(&right.name));
    members.dedup_by(|left, right| left.name == right.name);
    write_rust_members(&mut output, &members, excluded, &parameters);
    if extra.trim().is_empty() && members.iter().all(|member| excluded.contains(&member.name)) {
        return Err(exposes_nothing(source_path, &name));
    }
    output.push_str(extra);
    output.push_str("]\n}\n");
    Ok(output)
}

fn write_rust_members(
    output: &mut String,
    members: &[Member],
    excluded: &[String],
    parameters: &HashMap<String, Vec<String>>,
) {
    for member in members {
        if excluded.contains(&member.name) {
            continue;
        }
        if member.asynchronous {
            let declared = parameters.get(&member.name).map_or(&[][..], Vec::as_slice);
            let returns = member.declared_type.as_deref();
            let entry =
                RustTaskMember::entry(&camel_case(&member.name), &member.name, returns, declared);
            let _ = writeln!(output, "{entry}");
        } else if member.method {
            let declared = parameters.get(&member.name).map_or(&[][..], Vec::as_slice);
            write_rust_method(output, member, declared);
        } else {
            write_rust_field(output, member);
        }
    }
}

fn write_rust_method(output: &mut String, member: &Member, parameters: &[String]) {
    let name = &member.name;
    let key = camel_case(name);
    let kind = member.declared_type.as_deref().unwrap_or("()");
    let (bindings, passed) = rust_arguments(parameters);
    let value = rust_read(kind, &format!("it.borrow_mut().{name}({passed})"));
    let _ = writeln!(
        output,
        "(\"{key}\", TacMember::Method({{let it = instance.clone(); Box::new(move |arguments: Vec<TacValue>| {{ #[allow(unused_mut, unused_variables)] let mut arguments = arguments.into_iter(); {bindings}{value} }})}})),"
    );
}

fn write_rust_field(output: &mut String, member: &Member) {
    let name = &member.name;
    let key = camel_case(name);
    let kind = member.declared_type.as_deref().unwrap_or("i64");
    let access = if kind == "&str" {
        format!("it.borrow().{name}")
    } else {
        format!("it.borrow().{name}.clone()")
    };
    let value = rust_read(kind, &access);
    let write = match rust_write(kind) {
        Some(conversion) => format!(
            "Some({{let it = instance.clone(); Box::new(move |value| it.borrow_mut().{name} = {conversion})}})"
        ),
        None => String::from("None"),
    };
    let _ = writeln!(
        output,
        "(\"{key}\", TacMember::Field {{read: {{let it = instance.clone(); Box::new(move || {value})}}, write: {write}}}),"
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Member {
    name: String,
    declared_type: Option<String>,
    method: bool,
    mutable: bool,
    parameters: Option<Vec<String>>,
    asynchronous: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompanionLanguage {
    Swift,
    Kotlin,
    CSharp,
    CSharpBrowser,
    Rust,
}

impl CompanionLanguage {
    pub(crate) fn for_web(self, web: bool) -> Self {
        self.browser_selection(web)
    }

    fn browser_selection(self, web: bool) -> Self {
        match (self, web) {
            (Self::CSharp, true) => Self::CSharpBrowser,
            _ => self,
        }
    }
}

impl Member {
    fn is_runtime_resource(annotation: &str) -> bool {
        let declared = annotated_type(annotation).unwrap_or_default();
        let initializer = annotation.split_once('=').map_or("", |(_, value)| value);
        let constructed = initializer.split('(').next().unwrap_or_default();
        [declared.trim_end_matches(['!', '?']), constructed.trim()]
            .iter()
            .any(|name| Self::is_resource_type(name))
    }

    fn is_resource_type(name: &str) -> bool {
        name == "BehaviorContext" || crate::web_api_clients::WebApiClients::is_client_type(name)
    }

    const BEHAVIOR_JSON: &'static str = "BehaviorJSON";
    const SWIFT_KINDS: [&'static str; 2] = ["method", "task"];

    fn method_effects(signature: &str) -> &str {
        let after = closing_parenthesis(signature).map_or("", |close| &signature[close + 1..]);
        after.split('{').next().unwrap_or_default()
    }

    fn require_name(name: &str) -> Option<()> {
        (!name.is_empty()).then_some(())
    }

    fn method_returned_type(signature: &str, language: &str) -> Option<String> {
        let effects = Self::method_effects(signature);
        Self::returned_type(effects, language)
            .or_else(|| Self::block_returned_type(signature, language))
    }

    fn block_returned_type(signature: &str, language: &str) -> Option<String> {
        let close = closing_parenthesis(signature)?;
        let after = signature[close + 1..].trim_start();
        Self::block_unit(after, language)
    }

    fn block_unit(after: &str, language: &str) -> Option<String> {
        match (language, after.as_bytes().first()) {
            ("tac.kt", Some(b'{')) => Some(String::from("Unit")),
            _ => None,
        }
    }

    fn returned_type(effects: &str, language: &str) -> Option<String> {
        match language {
            "tac.kt" => Self::kotlin_returned_type(effects),
            _ => Self::swift_returned_type(effects),
        }
    }

    fn swift_returned_type(effects: &str) -> Option<String> {
        let (_, returned) = effects.split_once("->")?;
        Some(returned.trim().to_owned())
    }

    fn kotlin_returned_type(effects: &str) -> Option<String> {
        let returned = effects.trim_start().strip_prefix(':')?;
        let declared = returned.split('=').next().unwrap_or_default().trim();
        (!declared.is_empty()).then(|| declared.to_owned())
    }

    fn property_type(annotation: &str, language: &str) -> Option<String> {
        match language {
            "tac.kt" => Self::kotlin_property_type(annotation),
            _ => annotated_type(annotation),
        }
    }

    fn kotlin_property_type(annotation: &str) -> Option<String> {
        let declared = annotated_type(annotation)?;
        let end = Self::getter_start(&declared).unwrap_or(declared.len());
        Some(declared[..end].trim().to_owned())
    }

    fn getter_start(declared: &str) -> Option<usize> {
        declared
            .match_indices("get")
            .find_map(|(at, _)| Self::getter_word(declared, at).then_some(at))
    }

    fn getter_word(declared: &str, at: usize) -> bool {
        let before = declared.as_bytes()[..at]
            .last()
            .is_some_and(u8::is_ascii_whitespace);
        let call = declared[at + 3..].trim_start().starts_with('(');
        before && call
    }

    fn is_asynchronous(effects: &str) -> bool {
        effects.split_whitespace().any(|word| word == "async")
    }

    fn swift_behavior_entry(&self, parameters: &[String]) -> String {
        let name = &self.name;
        let call = format!("tacInstance.{name}({})", swift_arguments(parameters));
        let answer = self.swift_answer(&self.swift_finished(call));
        let kind = Self::SWIFT_KINDS[usize::from(self.asynchronous)];
        format!("    \"{name}\": .{kind}({{ tacArguments in {answer} }}),\n")
    }

    fn is_swift_behavior_member(&self) -> bool {
        self.asynchronous || self.returns_behavior_json()
    }

    fn returns_behavior_json(&self) -> bool {
        self.declared_type.as_deref() == Some(Self::BEHAVIOR_JSON)
    }

    fn swift_finished(&self, call: String) -> String {
        if self.asynchronous {
            return format!("await {call}");
        }
        call
    }

    fn swift_answer(&self, call: &str) -> String {
        if self.returns_behavior_json() {
            return format!("tacParseJSON(try {call}.encoded())");
        }
        format!("try {call}")
    }

    fn swift_argument(kind: &str, index: usize) -> String {
        if kind == Self::BEHAVIOR_JSON {
            return format!("BehaviorJSON.parse(tacArgumentText(tacArguments, {index}))");
        }
        format!("tacArgument(tacArguments, {index})")
    }
}

struct Modifiers;

impl Modifiers {
    const CLASS: [&str; 11] = [
        "public",
        "private",
        "internal",
        "final",
        "open",
        "abstract",
        "sealed",
        "static",
        "pub",
        "pub(crate)",
        "data",
    ];

    const CSHARP_READABLE: [&str; 2] = ["public ", "internal "];

    const TYPE_FIRST: [(&str, bool); 8] = [
        ("public ", false),
        ("async ", false),
        ("required ", false),
        ("internal ", false),
        ("static ", false),
        ("final ", true),
        ("readonly ", true),
        ("const ", true),
    ];

    const CSHARP_TASKS: [(&str, &str); 4] = [
        (
            "System.Threading.Tasks.Task<BehaviorJSON>",
            "Task<BehaviorJSON>",
        ),
        ("Task<BehaviorJSON>", "Task<BehaviorJSON>"),
        ("System.Threading.Tasks.Task", "Task"),
        ("Task", "Task"),
    ];
}

fn declares_table(language: CompanionLanguage, source: &str) -> bool {
    let prefixes: &[&str] = match language {
        CompanionLanguage::Swift => &["let tac", "var tac"],
        CompanionLanguage::Kotlin => &["val tac", "var tac"],
        CompanionLanguage::CSharp | CompanionLanguage::CSharpBrowser => &["class TacBridge"],
        CompanionLanguage::Rust => &["fn tac()"],
    };
    for line in source.lines() {
        let line = line.trim_start();
        if prefixes.iter().any(|prefix| line.starts_with(prefix)) {
            return true;
        }
    }
    false
}

fn class_body_named<'a>(source: &'a str, wanted: Option<&str>) -> Option<(String, &'a str)> {
    let Some(wanted) = wanted else {
        return class_body_at(source);
    };
    let mut rest = source;
    while let Some((name, body)) = class_body_at(rest) {
        if name == wanted {
            return Some((name, body));
        }
        let body_start = body.as_ptr() as usize - rest.as_ptr() as usize;
        rest = rest.get(body_start + body.len()..)?;
    }
    None
}

fn class_body_at(source: &str) -> Option<(String, &str)> {
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let Some(after) = after_declaration_keyword(line, "class") else {
            continue;
        };
        let name = UnicodeName::leading(after);
        if name.is_empty() {
            continue;
        }
        let open = source[line_start..].find('{')? + line_start;
        let body = braced_body(source, open)?;
        return Some((name, body));
    }
    None
}

fn after_declaration_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let mut rest = line.trim_start();
    while let Some(after) = without_class_modifier(rest) {
        rest = after;
    }
    rest.strip_prefix(keyword).and_then(after_keyword)
}

fn without_class_modifier(text: &str) -> Option<&str> {
    for modifier in Modifiers::CLASS {
        let Some(after) = text.strip_prefix(modifier) else {
            continue;
        };
        if after.starts_with(char::is_whitespace) {
            return Some(after.trim_start());
        }
    }
    None
}

fn braced_body(source: &str, open: usize) -> Option<&str> {
    let mut depth = 0usize;
    for (position, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&source[open + 1..open + position]);
                }
            }
            _ => {}
        }
    }
    None
}

fn members(
    language: CompanionLanguage,
    body: &str,
    source_path: &str,
) -> Result<Vec<Member>, Failure> {
    let mut members = Vec::new();
    let mut depth = 0i32;
    for line in body.lines() {
        let trimmed = line.trim();
        let entering = depth;
        let opened = i32::try_from(trimmed.matches('{').count()).unwrap_or(0);
        let closed = i32::try_from(trimmed.matches('}').count()).unwrap_or(0);
        depth += opened - closed;
        if entering != 0 || trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if let Some(member) = declaration(language, trimmed, source_path)? {
            members.push(member);
        }
    }
    members.sort_by(|left, right| left.name.cmp(&right.name));
    members.dedup_by(|left, right| left.name == right.name);
    Ok(members)
}

fn rust_struct_fields(body: &str) -> Vec<Member> {
    let mut fields = Vec::new();
    for line in body.lines() {
        for declaration in rust_field_declarations(line) {
            if let Some(field) = rust_field(declaration.trim()) {
                fields.push(field);
            }
        }
    }
    fields
}

fn rust_field_declarations(line: &str) -> Vec<String> {
    let mut declarations = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for character in line.chars() {
        match character {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth <= 0 => {
                declarations.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    declarations.push(current);
    declarations
}

fn rust_field(line: &str) -> Option<Member> {
    let line = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ");
    if line.starts_with("//") || line.starts_with('#') {
        return None;
    }
    let name = UnicodeName::leading(line);
    if name.is_empty() {
        return None;
    }
    let rest = line[name.len()..].trim_start();
    let declared = rest.strip_prefix(':')?.trim().trim_end_matches(',').trim();
    let declared = if matches!(declared, "&'static str" | "&str") {
        "&str"
    } else if declared.is_empty()
        || declared.contains(['&', '\''])
        || (declared.contains('<') && !RustModelType::is_model(declared))
        || RustModelType::is_runtime(declared)
        || crate::web_api_clients::WebApiClients::is_client_type(declared)
        || !RustModelType::crosses(declared)
    {
        return None;
    } else {
        declared
    };
    Some(Member {
        name,
        declared_type: Some(String::from(declared)),
        method: false,
        mutable: true,
        parameters: Some(Vec::new()),
        asynchronous: false,
    })
}

fn rust_methods(body: &str) -> Vec<(Member, Vec<String>)> {
    let mut methods = Vec::new();
    for line in body.lines() {
        if let Some(method) = rust_method(line) {
            methods.push(method);
        }
    }
    methods
}

fn rust_method(line: &str) -> Option<(Member, Vec<String>)> {
    let line = line.trim().trim_start_matches("pub ");
    let asynchronous = line.starts_with("async ");
    let line = line.trim_start_matches("async ").trim_start();
    let rest = line.strip_prefix("fn").and_then(after_keyword)?;
    let name = UnicodeName::leading(rest);
    let arguments = rest[name.len()..].trim_start();
    let takes_self = arguments.starts_with("(&self") || arguments.starts_with("(&mut self");
    if name.is_empty() || !takes_self {
        return None;
    }
    let returns = rust_return_type(arguments);
    let crosses = |kind: &str| kind == "()" || RustModelType::crosses(kind);
    let shown = if asynchronous {
        RustTaskMember::crosses(returns.as_deref())
    } else {
        returns.as_deref().is_none_or(crosses)
    };
    if !shown {
        return None;
    }
    let parameters = rust_parameters(arguments)?;
    let member = Member {
        name,
        declared_type: returns,
        method: true,
        mutable: false,
        parameters: Some(Vec::new()),
        asynchronous,
    };
    Some((member, parameters))
}

fn rust_return_type(arguments: &str) -> Option<String> {
    let returned = arguments.split("->").nth(1)?;
    let returned = returned.split('{').next().unwrap_or_default().trim();
    if returned.is_empty() {
        return None;
    }
    Some(returned.to_owned())
}

fn rust_parameters(arguments: &str) -> Option<Vec<String>> {
    let close = closing_parenthesis(arguments).unwrap_or(arguments.len());
    let inside = &arguments[1..close];
    let mut kinds = Vec::new();
    for parameter in inside.split(',').skip(1) {
        let parameter = parameter.trim();
        if parameter.is_empty() {
            continue;
        }
        let kind = parameter.split_once(':')?.1.trim();
        if !RustModelType::crosses(kind) {
            return None;
        }
        kinds.push(kind.to_owned());
    }
    Some(kinds)
}

fn closing_parenthesis(arguments: &str) -> Option<usize> {
    let mut depth = 0_usize;
    for (position, character) in arguments.char_indices() {
        match character {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && character == ')' {
                    return Some(position);
                }
            }
            _ => {}
        }
    }
    None
}

fn rust_arguments(parameters: &[String]) -> (String, String) {
    let mut bindings = String::new();
    let mut passed = Vec::new();
    for (index, kind) in parameters.iter().enumerate() {
        let conversion = rust_write(kind).unwrap_or_else(|| String::from("value.as_text()"));
        let _ = write!(
            bindings,
            "let a{index} = {{ let value = arguments.next().unwrap_or(TacValue::Null); {conversion} }}; "
        );
        if kind == "&str" {
            passed.push(format!("&a{index}"));
        } else {
            passed.push(format!("a{index}"));
        }
    }
    (bindings, passed.join(", "))
}

fn block_named<'a>(
    source: &'a str,
    keyword: &str,
    wanted: Option<&str>,
) -> Option<(String, &'a str)> {
    for (header, body) in blocks(source, keyword) {
        let name = UnicodeName::leading(header.trim());
        let Some(wanted) = wanted else {
            return Some((name, body));
        };
        if name == wanted {
            return Some((name, body));
        }
    }
    None
}

fn blocks<'a>(source: &'a str, keyword: &str) -> Vec<(&'a str, &'a str)> {
    let mut found = Vec::new();
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let Some(after) = after_declaration_keyword(line, keyword) else {
            continue;
        };
        let header_end = after.find(['{', ';', '\n']).unwrap_or(after.len());
        let header = &after[..header_end];
        let tail = &source[line_start..];
        let Some(brace) = tail.find('{') else {
            break;
        };
        if tail[..brace].contains(';') {
            found.push((header, ""));
            continue;
        }
        if let Some(body) = braced_body(source, line_start + brace) {
            found.push((header, body));
        }
    }
    found
}

pub(crate) fn rust_read(kind: &str, expression: &str) -> String {
    match kind {
        "TacValue" => format!("({expression})"),
        "f32" | "f64" => format!("TacValue::Float(f64::from({expression}))"),
        "bool" => format!("TacValue::Flag({expression})"),
        "String" | "&str" => format!("TacValue::Text(({expression}).to_string())"),
        "()" => format!("{{ {expression}; TacValue::Null }}"),
        _ if RustModelType::is_model(kind) => RustModelType::read(expression),
        _ => format!("TacValue::Int(({expression}) as i64)"),
    }
}

pub(crate) fn rust_write(kind: &str) -> Option<String> {
    let conversion = match kind {
        "TacValue" => String::from("value"),
        "f32" => String::from("value.as_float() as f32"),
        "f64" => String::from("value.as_float()"),
        "bool" => String::from("value.as_flag()"),
        "String" => String::from("value.as_text()"),
        "&str" => return None,
        _ if RustModelType::is_model(kind) => RustModelType::write(kind),
        _ => format!("value.as_int() as {kind}"),
    };
    Some(conversion)
}

fn declaration(
    language: CompanionLanguage,
    line: &str,
    source_path: &str,
) -> Result<Option<Member>, Failure> {
    match language {
        CompanionLanguage::Swift => {
            name_first(line, source_path, "var", "let", "func", "tac.swift")
        }
        CompanionLanguage::Kotlin => KotlinMembers::public_declaration(strip_attribute(line))
            .map_or(Ok(None), |line| {
                let (asynchronous, line) = KotlinMembers::suspended(line);
                let found = name_first(line, source_path, "var", "val", "fun", "tac.kt")?;
                Ok(found.map(|member| Member {
                    asynchronous,
                    ..member
                }))
            }),
        CompanionLanguage::CSharp | CompanionLanguage::CSharpBrowser => Ok(type_first(line)),
        CompanionLanguage::Rust => Ok(None),
    }
}

fn name_first(
    line: &str,
    source_path: &str,
    mutable: &str,
    immutable: &str,
    function: &str,
    language: &str,
) -> Result<Option<Member>, Failure> {
    let line = strip_attribute(line);
    if line.starts_with("private ") || line.starts_with("fileprivate ") {
        return Ok(None);
    }
    let line = line.trim_start_matches("public ");
    if let Some(rest) = line.strip_prefix(function).and_then(after_keyword) {
        return Ok(name_first_method(rest, language));
    }
    for (keyword, writable) in [(mutable, true), (immutable, false)] {
        let Some(rest) = line.strip_prefix(keyword).and_then(after_keyword) else {
            continue;
        };
        let name = UnicodeName::leading(rest);
        if name.is_empty() {
            continue;
        }
        let annotation = rest[name.len()..].trim_start();
        if Member::is_runtime_resource(annotation) {
            return Ok(None);
        }
        let declared_type = Member::property_type(annotation, language);
        if writable && declared_type.is_none() {
            return Err(missing_type(&name, source_path, language));
        }
        return Ok(Some(Member {
            name,
            declared_type,
            method: false,
            mutable: writable,
            parameters: Some(Vec::new()),
            asynchronous: false,
        }));
    }
    Ok(None)
}

fn name_first_method(rest: &str, language: &str) -> Option<Member> {
    let name = UnicodeName::leading(rest);
    Member::require_name(&name)?;
    let signature = rest[name.len()..].trim_start();
    let parameters = signature
        .strip_prefix('(')
        .map_or_else(Vec::new, typed_parameters);
    let effects = Member::method_effects(signature);
    Some(Member {
        name,
        declared_type: Member::method_returned_type(signature, language),
        method: true,
        mutable: false,
        parameters: Some(parameters),
        asynchronous: Member::is_asynchronous(effects),
    })
}

fn typed_parameters(list: &str) -> Vec<String> {
    let mut types = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for character in list.chars() {
        match character {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' if depth == 0 => break,
            ')' | '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                types.extend(typed_parameter(&current));
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    types.extend(typed_parameter(&current));
    types
}

fn typed_parameter(parameter: &str) -> Option<String> {
    let (names, declared) = ParameterText::without_annotations(parameter).split_once(':')?;
    let kind = declared.split('=').next().unwrap_or_default().trim();
    (!kind.is_empty()).then(|| format!("{}: {kind}", names.trim()))
}

fn parameter_type(declaration: &str) -> &str {
    declaration
        .split_once(':')
        .map_or(declaration, |(_, kind)| kind.trim())
}

fn swift_label(declaration: &str) -> String {
    let names = declaration.split_once(':').map_or("", |(names, _)| names);
    match names.split_whitespace().next() {
        Some("_") | None => String::new(),
        Some(label) => format!("{label}: "),
    }
}

fn annotated_type(annotation: &str) -> Option<String> {
    let after_colon = annotation.strip_prefix(':')?;
    let before_value = after_colon.split('=').next().unwrap_or_default();
    Some(before_value.trim().to_owned())
}

fn type_first(line: &str) -> Option<Member> {
    let line = line.trim();
    if !Modifiers::CSHARP_READABLE
        .iter()
        .any(|access| line.starts_with(access))
    {
        return None;
    }
    let (line, fixed) = without_type_first_modifiers(line);
    if line.starts_with("//") || line.starts_with('@') {
        return None;
    }
    let (declared_type, rest) = csharp_declared_type(line)?;
    let name = UnicodeName::leading(rest);
    if name.is_empty() {
        return None;
    }
    let after = rest[name.len()..].trim_start();
    let method = after.starts_with('(');
    let hidden_type = CsharpModelType::is_runtime(&declared_type)
        || crate::web_api_clients::WebApiClients::is_client_type(&declared_type);
    if !method && hidden_type {
        return None;
    }
    let getter = is_getter_only(after);
    let parameters = if method {
        csharp_parameters(after)
    } else {
        Some(Vec::new())
    };
    Some(Member {
        name,
        declared_type: Some(declared_type),
        method,
        mutable: !method && !fixed && !getter,
        parameters,
        asynchronous: false,
    })
}

fn csharp_declared_type(line: &str) -> Option<(String, &str)> {
    for (written, declared) in Modifiers::CSHARP_TASKS {
        if let Some(rest) = line.strip_prefix(written).and_then(after_keyword) {
            return Some((String::from(declared), rest));
        }
    }
    let name = UnicodeName::leading(line);
    if name.is_empty() {
        return None;
    }
    let (declared_type, rest) = CsharpModelType::declared(line, name.len())?;
    if rest.starts_with('<') || rest.starts_with('?') || CsharpModelType::is_task(&declared_type) {
        return None;
    }
    Some((declared_type, rest))
}

fn without_type_first_modifiers(line: &str) -> (&str, bool) {
    let mut line = line;
    let mut fixed = false;
    'strip: loop {
        for (modifier, immutable) in Modifiers::TYPE_FIRST {
            if let Some(rest) = line.strip_prefix(modifier) {
                fixed |= immutable;
                line = rest.trim_start();
                continue 'strip;
            }
        }
        return (line, fixed);
    }
}

fn is_getter_only(after_name: &str) -> bool {
    if after_name.starts_with("=>") {
        return true;
    }
    if !after_name.starts_with('{') {
        return false;
    }
    let accessors = match after_name.split_once('}') {
        Some((accessors, _)) => accessors,
        None => after_name,
    };
    let mut words = accessors.split(|character: char| !character.is_alphanumeric());
    !words.any(|word| word == "set" || word == "init")
}

fn strip_attribute(line: &str) -> &str {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('@') {
        let name_length = UnicodeName::leading(after).len();
        if name_length == 0 {
            return rest;
        }
        let tail = &after[name_length..];
        rest = match tail.strip_prefix('(') {
            Some(arguments) => after_closing_parenthesis(arguments),
            None => tail,
        };
        rest = rest.trim_start();
    }
    rest
}

fn after_closing_parenthesis(arguments: &str) -> &str {
    let mut depth = 1i32;
    for (index, character) in arguments.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &arguments[index + 1..];
                }
            }
            _ => {}
        }
    }
    arguments
}

fn after_keyword(rest: &str) -> Option<&str> {
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(rest.trim_start())
}

fn missing_type(name: &str, source_path: &str, language: &str) -> Failure {
    Failure::one(diagnostic(
        1405,
        format!("Companion field '{name}' needs a declared type."),
        Some(format!(
            "Write it as a typed declaration, so the generated setter knows what \
             to convert an incoming value into. A read-only field needs no type. \
             This applies to a class-shaped {language} companion.",
        )),
        source_span(source_path, 0, source_path.len()),
    ))
}

fn entry(language: CompanionLanguage, member: &Member) -> String {
    match language {
        CompanionLanguage::Swift => swift_entry(member),
        CompanionLanguage::Kotlin => kotlin_entry(member),
        CompanionLanguage::CSharp => csharp_entry(member),
        CompanionLanguage::CSharpBrowser => CsharpBrowserMembers::entry(member),
        CompanionLanguage::Rust => String::new(),
    }
}

fn swift_entry(member: &Member) -> String {
    let name = &member.name;
    let parameters = member.parameters.as_deref().unwrap_or_default();
    if member.is_swift_behavior_member() {
        return member.swift_behavior_entry(parameters);
    }
    if member.method && parameters.is_empty() {
        return format!("    \"{name}\": .method({{ _ in try tacInstance.{name}() }}),\n");
    }
    if member.method {
        let arguments = swift_arguments(parameters);
        return format!(
            "    \"{name}\": .method({{ tacArguments in try tacInstance.{name}({arguments}) }}),\n"
        );
    }
    match (&member.declared_type, member.mutable) {
        (Some(kind), true) => format!(
            "    \"{name}\": .field({{ tacInstance.{name} }}, \
             {{ tacInstance.{name} = tacField({kind}.self, $0) ?? tacInstance.{name} }}),\n"
        ),
        _ => format!("    \"{name}\": .field({{ tacInstance.{name} }}),\n"),
    }
}

fn swift_arguments(parameters: &[String]) -> String {
    parameters
        .iter()
        .enumerate()
        .map(|(index, declaration)| {
            format!(
                "{}{}",
                swift_label(declaration),
                Member::swift_argument(parameter_type(declaration), index)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn kotlin_entry(member: &Member) -> String {
    let name = &member.name;
    if member.method {
        let arguments = member
            .parameters
            .iter()
            .flatten()
            .enumerate()
            .map(|(index, declaration)| {
                kotlin_value(
                    parameter_type(declaration),
                    &format!("it.getOrNull({index})"),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        if member.asynchronous {
            return format!(
                "    \"{name}\" to TacTask({{ tacThrownCode(it) }}) {{ tacInstance.{name}({arguments}) }},\n"
            );
        }
        return format!("    \"{name}\" to TacMethod {{ tacInstance.{name}({arguments}) }},\n");
    }
    match (&member.declared_type, member.mutable) {
        (Some(kind), true) => {
            let converter = kotlin_value(kind, "it");
            format!(
                "    \"{name}\" to TacField({{ tacInstance.{name} }}, \
                 {{ tacInstance.{name} = {converter} }}),\n"
            )
        }
        _ => format!("    \"{name}\" to TacField({{ tacInstance.{name} }}),\n"),
    }
}

fn csharp_entry(member: &Member) -> String {
    let name = &member.name;
    let key = lower_first(name);
    let Some(parameters) = &member.parameters else {
        return String::new();
    };
    let passed = csharp_arguments(parameters);
    match (member.method, &member.declared_type, member.mutable) {
        (true, Some(kind), _) if kind == "void" => format!(
            "        [\"{key}\"] = TacMember.Method(arguments => {{ Instance.{name}({passed}); return null; }}),\n"
        ),
        (true, Some(kind), _) if kind == "Task" => format!(
            "        [\"{key}\"] = TacMember.Method(arguments => {{ Instance.{name}({passed}).GetAwaiter().GetResult(); return null; }}),\n"
        ),
        (true, Some(kind), _) if kind == "Task<BehaviorJSON>" => format!(
            "        [\"{key}\"] = TacMember.Method(arguments => Tac.PlainJson(Instance.{name}({passed}).GetAwaiter().GetResult())),\n"
        ),
        (true, Some(kind), _) if kind == "BehaviorJSON" => format!(
            "        [\"{key}\"] = TacMember.Method(arguments => Tac.PlainJson(Instance.{name}({passed}))),\n"
        ),
        (true, Some(kind), _) if kind == "object" => format!(
            "        [\"{key}\"] = TacMember.Method(arguments => TacMember.Result(() => Instance.{name}({passed}))),\n"
        ),
        (true, kind, _) => {
            let call =
                CsharpModelType::value(kind.as_deref(), &format!("Instance.{name}({passed})"));
            format!("        [\"{key}\"] = TacMember.Method(arguments => {call}),\n")
        }
        (false, kind, mutable) => {
            let read = CsharpModelType::value(kind.as_deref(), &format!("Instance.{name}"));
            let write = kind
                .as_deref()
                .filter(|_| mutable)
                .and_then(|kind| CsharpModelType::reading(kind, "value"))
                .map(|converter| format!(", value => Instance.{name} = {converter}"))
                .unwrap_or_default();
            format!("        [\"{key}\"] = TacMember.Field(() => {read}{write}),\n")
        }
    }
}

fn kotlin_value(kind: &str, value: &str) -> String {
    match kind {
        "Int" => format!("tacInt({value})"),
        "Long" => format!("tacLong({value})"),
        "Double" => format!("tacDouble({value})"),
        "Boolean" => format!("tacBoolean({value})"),
        "String" => format!("tacString({value})"),
        _ => format!("tacRead<{kind}>({value})"),
    }
}

pub(crate) fn camel_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut capitalise = false;
    for character in value.chars() {
        if character == '_' {
            capitalise = true;
        } else if capitalise {
            output.extend(character.to_uppercase());
            capitalise = false;
        } else {
            output.push(character);
        }
    }
    output
}

pub(crate) fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_lowercase().collect::<String>() + characters.as_str()
}

fn csharp_converter(kind: &str) -> &'static str {
    match kind {
        "int" => "Convert.ToInt32(value)",
        "long" => "Convert.ToInt64(value)",
        "double" | "float" => "Convert.ToDouble(value)",
        "decimal" => "Convert.ToDecimal(value)",
        "bool" => "Convert.ToBoolean(value)",
        "object" => "value",
        _ => "Convert.ToString(value)",
    }
}

fn csharp_parameters(after_name: &str) -> Option<Vec<String>> {
    let mut kinds = Vec::new();
    for parameter in ParameterText::csharp_parameters(after_name)? {
        let kind = parameter.split_whitespace().next()?;
        let readable_model =
            CsharpModelType::is_model(kind) && CsharpModelType::read(kind, kind).is_some();
        let supported = readable_model
            || matches!(
                kind,
                "string" | "int" | "long" | "double" | "float" | "bool" | "object" | "BehaviorJSON"
            );
        if !supported {
            return None;
        }
        kinds.push(kind.to_owned());
    }
    Some(kinds)
}

fn csharp_arguments(parameters: &[String]) -> String {
    let mut arguments = Vec::new();
    for (index, kind) in parameters.iter().enumerate() {
        let value = format!("(arguments.Length > {index} ? arguments[{index}] : null)");
        let argument = match kind.as_str() {
            "object" => value,
            "float" => format!("Convert.ToSingle({value})"),
            "BehaviorJSON" => format!("BehaviorJSON.Parse(Tac.EncodeValue({value}))"),
            other => CsharpModelType::reading(other, &value).unwrap_or(value),
        };
        arguments.push(argument);
    }
    arguments.join(", ")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{CompanionLanguage, RustGraph, owner_name, owner_table, typed_parameters};
    use crate::Failure;

    struct Sources;

    impl Sources {
        const UNIT_STRUCT: &str = "pub struct Probe;\n\n\
             impl Probe {\n\
             \x20   pub fn new() -> Self { Self }\n\
             \x20   pub fn start(&self) -> String {\n\
             \x20       std::thread::spawn(|| {\n\
             \x20           1 + 1\n\
             \x20       });\n\
             \x20       String::from(\"started\")\n\
             \x20   }\n\
             }\n";

        const SWIFT_COMPANION: &str = "final class CompanionService {\n  var count: Int = 6\n  \
                 let runtime = \"Swift\"\n\n  func doubled() -> Int { count * 2 }\n}\n";

        const KOTLIN_COMPANION: &str = "class CompanionService {\n  var count: Int = 6\n  \
                 val runtime: String = \"Kotlin\"\n\n  fun doubled(): Int = count * 2\n}\n";

        const CSHARP_COMPANION: &str = "public class Companion\n{\n  public int Count = 6;\n  \
                 public string Runtime => \"C#\";\n\n  public int Doubled() => Count * 2;\n}\n";

        const RUST_COMPANION: &str = "pub struct CompanionService {\n  pub count: i32,\n  pub runtime: &'static str,\n}\n\n\
                 impl CompanionService {\n  pub fn new() -> Self {\n    \
                 Self { count: 6, runtime: \"Rust\" }\n  }\n\n  \
                 pub fn doubled(&self) -> i32 {\n    self.count * 2\n  }\n}\n";

        const PLATFORM_WIDTH_FIELD: &str = "pub struct CompanionService {\n    pub cores: usize,\n}\n\n\
             impl CompanionService {\n    pub fn new() -> Self {\n        \
             Self { cores: 1 }\n    }\n}\n";

        const SWIFT_PRIVATE_MEMBER: &str = "final class CompanionService {\n  private let secret = \"x\"\n  let runtime = \"Swift\"\n}\n";

        const KOTLIN_PRIVATE_MEMBER: &str = "class CompanionService {\n  private val secret = \"x\"\n  val runtime = \"Kotlin\"\n}\n";

        const CSHARP_PRIVATE_MEMBER: &str = "public class Companion\n{\n  private string Secret => \"x\";\n  \
                 public string Runtime => \"C#\";\n}\n";

        const SWIFT_CLASS: &str = "final class CompanionService {\n    var count: Int = 6\n    \
             let label: String = \"a\"\n    var behaviors: BehaviorContext!\n    let clock = Clock()\n    \
             func doubled() -> Int { count * 2 }\n}\n";

        const RUST_ARGUMENTS: &str = "#[derive(Default)]\nstruct DrawerService {\n    open: bool,\n}\n\n\
             impl DrawerService {\n    fn dismiss(&mut self, key: &str, times: u8) {}\n    \
             fn keep(&self, value: TacValue) -> TacValue { value }\n    \
             fn hidden(&self, inner: std::path::PathBuf) {}\n}\n";

        const RUST_BINDINGS: &str = "let a0 = { let value = arguments.next().unwrap_or(TacValue::Null); value.as_text() }; let a1 = { let value = arguments.next().unwrap_or(TacValue::Null); value.as_int() as u8 }; ";

        const CSHARP_ARGUMENTS: &str = "public class DrawerService\n{\n    public object Entries;\n    \
             public string Dismiss(string key, int times) => key;\n    \
             public void Hidden(System.IO.Stream inner) { }\n}\n";

        const CSHARP_BEHAVIOR_SERVICE: &str = "public sealed class NotesService {\n    \
             public BehaviorContext behaviors;\n    public int reads = 0;\n    \
             public async System.Threading.Tasks.Task<BehaviorJSON> find(BehaviorJSON key) { return key; }\n    \
             public Task forget(BehaviorJSON key) { return Task.CompletedTask; }\n    \
             public BehaviorJSON label() { return null; }\n}\n";

        const RUST_BEHAVIOR_AND_MODEL_SERVICE: &str = "struct NotesService {\n    behaviors: BehaviorContext,\n    \
             last: BehaviorValue,\n    product: Product,\n}\n\n\
             impl NotesService {\n    fn new() -> Self { todo!() }\n\n    \
             #[Cached]\n    fn find(&self, key: BehaviorValue) -> BehaviorResult<BehaviorValue> { Ok(key) }\n\n    \
             fn look(&self, key: String) -> String { key }\n\n    \
             fn choose(&mut self, product: Product) {}\n}\n";

        const CSHARP_BEHAVIOR_AND_MODEL_SERVICE: &str = "public sealed class NotesService {\n    \
             public BehaviorSignal signal = new BehaviorSignal();\n    \
             public Product Product = new Product(\"Anvil\");\n    \
             public BehaviorJSON Keep(BehaviorSignal given) { return null; }\n    \
             public void Choose(Product product) { }\n}\n";

        const CSHARP_DISMISS_CALL: &str = "Instance.Dismiss(Convert.ToString((arguments.Length > 0 ? arguments[0] : null)), Convert.ToInt32((arguments.Length > 1 ? arguments[1] : null)))";

        const RUST_STRUCT_WITH_DEFAULT: &str = "pub struct CompanionService {\n    count: i32,\n    label: String,\n}\n\n\
             impl Default for CompanionService {\n    fn default() -> Self {\n        \
             Self { count: 6, label: String::from(\"Rust\") }\n    }\n}\n\n\
             impl CompanionService {\n    fn doubled(&self) -> i32 {\n        self.count * 2\n    }\n}\n";

        const RUST_BORROWED_STRING: &str = "pub struct CompanionService {\n    pub runtime: &'static str,\n}\n\n\
             impl Default for CompanionService {\n    fn default() -> Self {\n        \
             Self { runtime: \"Rust\" }\n    }\n}\n";

        const RUST_CONSTRUCTOR: &str = "pub struct CompanionService {\n    pub count: i32,\n}\n\n\
             impl CompanionService {\n    pub fn new() -> Self {\n        \
             Self { count: 6 }\n    }\n}\n";

        const KOTLIN_METHOD_BODY: &str = "class CompanionService {\n    var count: Int = 1\n    \
             fun doubled(): Int {\n        val scratch = count\n        return scratch * 2\n    }\n}\n";

        const RUST_HTTP_CLIENT_SERVICE: &str = "struct CatalogService { catalog: CatalogClient, count: i64 }\n\
             impl CatalogService {\n    fn new() -> Self {\n        \
             Self { catalog: CatalogClient::new(), count: 0 }\n    }\n\n    \
             fn all(&self) -> BehaviorResult<BehaviorValue> {\n        self.catalog.all()\n    }\n\n    \
             fn load(&mut self) {\n        self.count = 1;\n    }\n}\n\
             struct CatalogClient { behaviors: BehaviorContext }\n";

        const CSHARP_HTTP_CLIENT_SERVICE: &str = "public class CatalogService {\n    \
             readonly CatalogClient _catalog = new CatalogClient();\n    public int count = 0;\n\n    \
             public async Task Load() { count = 1; }\n    int Helper() => 1;\n}\n";

        const CSHARP_READ_ONLY_FIELDS: &str = "public class Companion\n{\n    public int Count { get; set; } = 6;\n    \
             public string Label => \"Native\";\n    public readonly int Fixed = 1;\n    \
             public int Reading { get; }\n}\n";
    }

    fn member_table(
        language: CompanionLanguage,
        source: &str,
        path: &str,
    ) -> Result<String, Failure> {
        owner_table(language, source, path, &[], "", false)
    }

    fn rust_member_table(source: &str, path: &str) -> Result<String, Failure> {
        member_table(CompanionLanguage::Rust, source, path)
    }

    fn page_table(language: CompanionLanguage, file_name: &str, source: &str) -> String {
        member_table(language, source, &format!("client/pages/{file_name}")).expect("generated")
    }

    #[test]
    fn a_parameter_annotation_with_json_arguments_leaves_the_parameter_type() {
        let parameters = typed_parameters(
            "@PageDefaults({\"defaultSize\":20,\"sort\":[\"id\"]}) input: Map<String, Any?>): Any?",
        );
        assert_eq!(parameters, ["input: Map<String, Any?>"]);
    }

    #[test]
    fn a_unit_struct_takes_its_members_from_its_impl_alone() {
        let generated =
            rust_member_table(Sources::UNIT_STRUCT, "tac.rs").expect("a unit struct is a struct");
        assert!(generated.contains("\"start\""), "{generated}");
        assert!(
            !generated.contains("\"std\""),
            "a method body was read as fields: {generated}"
        );
    }

    #[test]
    fn every_language_reads_the_same_class_the_same_way() {
        let authored = [
            (
                CompanionLanguage::Swift,
                "tac.swift",
                Sources::SWIFT_COMPANION,
            ),
            (
                CompanionLanguage::Kotlin,
                "tac.kt",
                Sources::KOTLIN_COMPANION,
            ),
            (
                CompanionLanguage::CSharp,
                "tac.cs",
                Sources::CSHARP_COMPANION,
            ),
            (CompanionLanguage::Rust, "tac.rs", Sources::RUST_COMPANION),
        ];
        for (language, file_name, source) in authored {
            let generated = page_table(language, file_name, source);
            for member in ["count", "runtime", "doubled"] {
                assert!(
                    generated.contains(&format!("\"{member}\"")),
                    "{file_name} lost {member}:\n{generated}"
                );
            }
            assert!(!generated.contains("\"new\""), "{file_name}:\n{generated}");
            assert!(
                !generated.contains("\"default\""),
                "{file_name}:\n{generated}"
            );
        }
    }

    #[test]
    fn an_untyped_read_only_field_is_a_field_and_not_a_method() {
        let cases = [
            (
                CompanionLanguage::Swift,
                "tac.swift",
                "final class CompanionService {\n  let runtime = \"Swift\"\n}\n",
                "\"runtime\": .field({ tacInstance.runtime }),",
            ),
            (
                CompanionLanguage::Kotlin,
                "tac.kt",
                "class CompanionService {\n  val runtime = \"Kotlin\"\n}\n",
                "\"runtime\" to TacField({ tacInstance.runtime }),",
            ),
        ];
        for (language, file_name, source, expected) in cases {
            let generated = page_table(language, file_name, source);
            assert!(generated.contains(expected), "{file_name}:\n{generated}");
            assert!(
                !generated.contains("runtime()"),
                "{file_name}:\n{generated}"
            );
        }
    }

    #[test]
    fn a_platform_width_integer_field_crosses_as_a_number() {
        let table =
            rust_member_table(Sources::PLATFORM_WIDTH_FIELD, "client/pages/tac.rs").expect("table");
        assert!(table.contains("as i64"), "{table}");
        assert!(!table.contains("i64::from"), "{table}");
    }

    #[test]
    fn a_private_member_is_not_in_the_table() {
        let cases = [
            (
                CompanionLanguage::Swift,
                "tac.swift",
                Sources::SWIFT_PRIVATE_MEMBER,
            ),
            (
                CompanionLanguage::Kotlin,
                "tac.kt",
                Sources::KOTLIN_PRIVATE_MEMBER,
            ),
            (
                CompanionLanguage::CSharp,
                "tac.cs",
                Sources::CSHARP_PRIVATE_MEMBER,
            ),
        ];
        for (language, file_name, source) in cases {
            let generated = page_table(language, file_name, source);
            assert!(generated.contains("untime"), "{file_name}:\n{generated}");
            assert!(!generated.contains("ecret"), "{file_name}:\n{generated}");
        }
    }

    #[test]
    fn a_swift_class_becomes_a_member_table() {
        let generated = member_table(
            CompanionLanguage::Swift,
            Sources::SWIFT_CLASS,
            "client/services/CompanionService.swift",
        )
        .expect("a table");
        for expected in [
            "let tacInstance = CompanionService()",
            "tacField(Int.self, $0) ?? tacInstance.count",
            "\"label\": .field({ tacInstance.label }),",
            ".method({ _ in try tacInstance.doubled() })",
        ] {
            assert!(generated.contains(expected), "{generated}");
        }
        for hidden in ["\"behaviors\"", "\"clock\""] {
            assert!(!generated.contains(hidden), "{generated}");
        }
    }

    #[test]
    fn a_swift_async_method_is_a_task_that_answers_json() {
        let source = "final class NotesService {\n  \
                      func find(_ key: BehaviorJSON, limit: Int) async throws -> BehaviorJSON { key }\n  \
                      func save(_ id: String) async throws {}\n}\n";
        let generated = member_table(
            CompanionLanguage::Swift,
            source,
            "client/services/NotesService.swift",
        )
        .expect("a table");
        assert!(
            generated.contains(
                "\"find\": .task({ tacArguments in tacParseJSON(try await tacInstance.find(\
                 BehaviorJSON.parse(tacArgumentText(tacArguments, 0)), limit: tacArgument(tacArguments, 1)).encoded()) }),"
            ),
            "{generated}"
        );
        assert!(
            generated.contains(
                "\"save\": .task({ tacArguments in try await tacInstance.save(tacArgument(tacArguments, 0)) }),"
            ),
            "{generated}"
        );
    }

    #[test]
    fn a_swift_method_passes_its_arguments_with_their_labels() {
        let source = "final class CounterService {\n  var count: Int = 0\n\n  \
                      func add(_ by: Int, why: String, with note: String = \"\") -> String { why }\n}\n";
        let generated = member_table(
            CompanionLanguage::Swift,
            source,
            "client/services/CounterService.swift",
        )
        .expect("a table");
        assert!(
            generated.contains(
                "\"add\": .method({ tacArguments in try tacInstance.add(tacArgument(tacArguments, 0), \
                 why: tacArgument(tacArguments, 1), with: tacArgument(tacArguments, 2)) }),"
            ),
            "{generated}"
        );
    }

    #[test]
    fn rust_methods_take_json_arguments_and_keep_the_rest_inside() {
        let generated =
            rust_member_table(Sources::RUST_ARGUMENTS, "client/services/DrawerService.rs")
                .expect("a table");
        assert!(generated.contains(Sources::RUST_BINDINGS), "{generated}");
        assert!(
            generated.contains("it.borrow_mut().dismiss(&a0, a1)"),
            "{generated}"
        );
        assert!(
            generated.contains("it.borrow_mut().keep(a0)"),
            "{generated}"
        );
        assert!(!generated.contains("\"hidden\""), "{generated}");
    }

    #[test]
    fn csharp_behavior_methods_wait_for_their_task_and_pass_json() {
        let generated = member_table(
            CompanionLanguage::CSharp,
            Sources::CSHARP_BEHAVIOR_SERVICE,
            "client/services/NotesService.cs",
        )
        .expect("a table");
        for expected in [
            "[\"find\"] = TacMember.Method(arguments => Tac.PlainJson(Instance.find(BehaviorJSON.Parse(Tac.EncodeValue((arguments.Length > 0 ? arguments[0] : null)))).GetAwaiter().GetResult()))",
            "[\"forget\"] = TacMember.Method(arguments => { Instance.forget(BehaviorJSON.Parse(Tac.EncodeValue((arguments.Length > 0 ? arguments[0] : null)))).GetAwaiter().GetResult(); return null; })",
            "[\"label\"] = TacMember.Method(arguments => Tac.PlainJson(Instance.label()))",
            "[\"reads\"] = TacMember.Field(",
        ] {
            assert!(generated.contains(expected), "{generated}");
        }
        assert!(!generated.contains("\"system\""), "{generated}");
        assert!(!generated.contains("\"behaviors\""), "{generated}");
    }

    #[test]
    fn rust_behavior_values_stay_inside_while_models_still_bind() {
        let generated = rust_member_table(
            Sources::RUST_BEHAVIOR_AND_MODEL_SERVICE,
            "client/services/NotesService.rs",
        )
        .expect("a table");
        for hidden in [
            "\"find\"",
            "\"last\"",
            "\"behaviors\"",
            "BehaviorValue as TacModel",
        ] {
            assert!(!generated.contains(hidden), "{hidden}: {generated}");
        }
        for expected in [
            "\"look\"",
            "TacModel::write(it.borrow().product.clone())",
            "<Product as TacModel>::read(&value)",
        ] {
            assert!(generated.contains(expected), "{expected}: {generated}");
        }
    }

    #[test]
    fn csharp_behavior_runtime_types_stay_inside_while_models_still_bind() {
        let generated = member_table(
            CompanionLanguage::CSharp,
            Sources::CSHARP_BEHAVIOR_AND_MODEL_SERVICE,
            "client/services/NotesService.cs",
        )
        .expect("a table");
        for hidden in [
            "Instance.signal",
            "Instance.Keep(",
            "TacModels.Read<BehaviorSignal>",
        ] {
            assert!(!generated.contains(hidden), "{hidden}: {generated}");
        }
        for expected in [
            "TacModels.Plain(Instance.Product)",
            "Instance.Choose(TacModels.Read<Product>(",
        ] {
            assert!(generated.contains(expected), "{expected}: {generated}");
        }
    }

    #[test]
    fn csharp_methods_take_json_arguments_and_object_fields_pass_through() {
        let generated = member_table(
            CompanionLanguage::CSharp,
            Sources::CSHARP_ARGUMENTS,
            "client/services/DrawerService.cs",
        )
        .expect("a table");
        assert!(
            generated.contains("value => Instance.Entries = value"),
            "{generated}"
        );
        assert!(
            generated.contains(Sources::CSHARP_DISMISS_CALL),
            "{generated}"
        );
        assert!(!generated.contains("\"hidden\""), "{generated}");
    }

    #[test]
    fn a_rust_struct_and_its_inherent_impl_become_a_table() {
        let generated = rust_member_table(
            Sources::RUST_STRUCT_WITH_DEFAULT,
            "client/services/CompanionService.rs",
        )
        .expect("a table");
        assert!(!generated.contains("\"default\""), "{generated}");
        for expected in [
            "\"doubled\", TacMember::Method",
            "TacValue::Int((it.borrow().count.clone()) as i64)",
            "TacValue::Text((it.borrow().label.clone()).to_string())",
            "value.as_text()",
            "value.as_int() as i32",
        ] {
            assert!(generated.contains(expected), "{generated}");
        }
    }

    #[test]
    fn a_borrowed_string_field_is_read_only_rather_than_dropped() {
        let table =
            rust_member_table(Sources::RUST_BORROWED_STRING, "client/pages/tac.rs").expect("table");
        assert!(table.contains("\"runtime\""), "{table}");
        assert!(table.contains("TacValue::Text"), "{table}");
        assert!(table.contains("write: None"), "{table}");
        assert!(!table.contains("runtime.clone()"), "{table}");
    }

    #[test]
    fn a_rust_service_hides_the_http_client_it_holds() {
        let table = rust_member_table(
            Sources::RUST_HTTP_CLIENT_SERVICE,
            "client/services/CatalogService.rs",
        )
        .expect("table");
        assert!(table.contains("\"count\""), "{table}");
        assert!(table.contains("\"load\""), "{table}");
        for hidden in [
            "\"catalog\"",
            "\"all\"",
            "as CatalogClient",
            "catalog.clone()",
        ] {
            assert!(!table.contains(hidden), "{hidden}:\n{table}");
        }
    }

    #[test]
    fn a_csharp_member_without_an_access_modifier_is_private() {
        let table = member_table(
            CompanionLanguage::CSharp,
            Sources::CSHARP_HTTP_CLIENT_SERVICE,
            "client/services/CatalogService.cs",
        )
        .expect("table");
        assert!(table.contains("[\"count\"]"), "{table}");
        assert!(table.contains("[\"load\"]"), "{table}");
        assert!(!table.contains("_catalog"), "{table}");
        assert!(!table.contains("Helper"), "{table}");
    }

    #[test]
    fn a_needs_all_service_is_built_without_a_constructor() {
        let rust = owner_table(
            CompanionLanguage::Rust,
            "struct TocService {\n    page_path: String,\n    entries: TacValue,\n}\n",
            "client/services/TocService.rs",
            &[],
            "",
            true,
        )
        .expect("NeedsAll struct table");
        assert!(
            rust.contains("TocService { page_path: ::core::default::Default::default(), entries: ::core::default::Default::default() }"),
            "{rust}"
        );
        let csharp = owner_table(
            CompanionLanguage::CSharp,
            "public class DocumentationService\n{\n    public string Source = \"page.json\";\n    public required object Docs;\n    public required object Features;\n}\n",
            "client/services/DocumentationService.cs",
            &[],
            "",
            true,
        )
        .expect("required members table");
        assert!(
            csharp.contains("new DocumentationService() { Docs = default!, Features = default! }"),
            "{csharp}"
        );
        assert!(csharp.contains("value => Instance.Docs ="), "{csharp}");
    }

    #[test]
    fn a_rust_struct_without_a_constructor_is_a_diagnostic() {
        let failure = rust_member_table(
            "pub struct CompanionService {\n    count: i32,\n}\n",
            "client/services/CompanionService.rs",
        )
        .expect_err("no constructor");
        let message = failure.to_string();
        assert!(message.contains("needs a constructor"), "{failure}");
        assert!(message.contains("fn new() -> Self"), "{failure}");
    }

    #[test]
    fn a_rust_constructor_is_used_over_default() {
        let table =
            rust_member_table(Sources::RUST_CONSTRUCTOR, "client/pages/tac.rs").expect("table");
        assert!(
            table.contains("RefCell::new(CompanionService::new())"),
            "{table}"
        );
        assert!(!table.contains("\"new\""), "{table}");
    }

    #[test]
    fn a_hand_written_table_is_rejected_without_owner_lifecycle() {
        let source = "class CompanionService { var count: Int = 1 }\n\
                      let tac: [String: TacMember] = [:]\n";
        assert!(member_table(CompanionLanguage::Swift, source, "x/tac.swift").is_err());
    }

    #[test]
    fn a_service_file_owns_the_type_it_names() {
        let swift = "final class CatalogProperties {}\n\n\
                     final class SettingsService { var title: String = \"Catalog\" }\n";
        let table = member_table(
            CompanionLanguage::Swift,
            swift,
            "client/services/SettingsService.swift",
        )
        .expect("named service");
        assert!(table.contains("SettingsService()"), "{table}");
        assert!(!table.contains("CatalogProperties()"), "{table}");
        assert!(table.contains("\"title\""), "{table}");

        let rust = "#[derive(Default)]\nstruct CatalogProperties {}\n\
                    #[derive(Default)]\nstruct SettingsService { title: i64 }\n";
        let table = rust_member_table(rust, "client/services/SettingsService.rs").expect("named");
        assert!(
            table.contains("<SettingsService as ::core::default::Default>::default()"),
            "{table}"
        );
        assert!(table.contains("\"title\""), "{table}");

        let table = member_table(
            CompanionLanguage::Swift,
            "final class CompanionService { var count: Int = 1 }\n",
            "client/pages/tac.swift",
        )
        .expect("unnamed");
        assert!(table.contains("CompanionService()"), "{table}");
    }

    #[test]
    fn rust_snake_case_native_members_select_the_matching_declared_service() {
        let source = "use crate::utils::TitleUtils as Titles;\n\
                      #[derive(Default)]\nstruct EarlierService {}\n\
                      #[derive(Default)]\nstruct LabelService;\n\
                      impl LabelService {\n    pub fn read(&self) -> String { Titles::text() }\n}\n";
        for path in [
            "client/services/feature/native/label_service.rs",
            "client/services/feature/native/LabelService.rs",
        ] {
            assert_eq!(
                owner_name(CompanionLanguage::Rust, source, path).as_deref(),
                Some("LabelService")
            );
            assert!(RustGraph::supports_arc(source, path, true).expect("immutable owner"));
            let table = RustGraph::table(source, path, "scope.label()", (&[], ""))
                .expect("named native graph owner");
            assert!(table.contains("fn tac_members_LabelService("), "{table}");
            assert!(table.contains("\"read\""), "{table}");
            assert!(!table.contains("tac_members_EarlierService"), "{table}");
        }
    }

    #[test]
    fn rust_snake_case_native_members_preserve_declared_type_mismatch_errors() {
        let failure = rust_member_table(
            "#[derive(Default)]\nstruct OtherService { count: i64 }\n",
            "client/services/feature/native/label_service.rs",
        )
        .expect_err("the filename must identify its actual service");
        assert!(
            failure
                .to_string()
                .contains("declares no struct named 'label_service'"),
            "{failure}"
        );
    }

    #[test]
    fn a_service_file_without_its_named_type_is_refused() {
        let failure = member_table(
            CompanionLanguage::Swift,
            "final class Helper { var count: Int = 1 }\n",
            "client/services/PanelService.swift",
        )
        .expect_err("absent");
        let message = failure.to_string();
        assert!(
            message.contains("declares no class named 'PanelService'"),
            "{message}"
        );
        assert!(message.contains("ADR 0030"), "{message}");
    }

    #[test]
    fn a_service_exposing_no_members_is_refused_in_every_language() {
        let empty_services = [
            (CompanionLanguage::Swift, "final class PanelService {\n}\n"),
            (CompanionLanguage::Kotlin, "class PanelService {\n}\n"),
            (
                CompanionLanguage::CSharp,
                "public class PanelService {\n}\n",
            ),
        ];
        for (language, source) in empty_services {
            let failure = member_table(language, source, "client/services/PanelService.swift")
                .expect_err("empty");
            assert!(
                failure.to_string().contains("exposes no members"),
                "{failure}"
            );
        }
        let failure = rust_member_table(
            "#[derive(Default)]\nstruct PanelService {}\n",
            "client/services/PanelService.rs",
        )
        .expect_err("empty");
        assert!(
            failure.to_string().contains("exposes no members"),
            "{failure}"
        );
    }

    #[test]
    fn a_lifecycle_only_service_still_stages() {
        let table = owner_table(
            CompanionLanguage::Swift,
            "final class PanelService {\n    func start() {}\n}\n",
            "client/services/PanelService.swift",
            &[String::from("start")],
            "\"start\": .hook(false, { tacInstance.start() }),\n",
            false,
        )
        .expect("hook entry keeps the table populated");
        assert!(table.contains("\"start\": .hook"), "{table}");
        assert!(!table.contains("[:]"), "{table}");
    }

    #[test]
    fn a_mutable_field_without_a_type_is_a_diagnostic() {
        let failure = member_table(
            CompanionLanguage::Swift,
            "class CompanionService {\n    var count = 6\n}\n",
            "client/services/CompanionService.swift",
        )
        .expect_err("untyped");
        assert!(
            failure.to_string().contains("needs a declared type"),
            "{failure}"
        );
    }

    #[test]
    fn a_method_body_does_not_contribute_members() {
        let generated = member_table(
            CompanionLanguage::Kotlin,
            Sources::KOTLIN_METHOD_BODY,
            "x/tac.kt",
        )
        .expect("a table");
        assert!(!generated.contains("scratch"), "{generated}");
        assert!(
            generated.contains("\"doubled\" to TacMethod"),
            "{generated}"
        );
        assert!(generated.contains("tacInt(it)"), "{generated}");
    }

    #[test]
    fn a_csharp_class_reads_types_first() {
        let csharp = member_table(
            CompanionLanguage::CSharp,
            "public class Companion\n{\n    public int Count = 6;\n    \
             public int Doubled() => Count * 2;\n}\n",
            "x/tac.cs",
        )
        .expect("a table");
        assert!(csharp.contains("[\"count\"]"), "{csharp}");
        assert!(csharp.contains("Convert.ToInt32(value)"), "{csharp}");
    }

    #[test]
    fn a_csharp_decimal_field_is_written_as_a_decimal() {
        let csharp = member_table(
            CompanionLanguage::CSharp,
            "public class Companion\n{\n    public decimal Price = 6m;\n}\n",
            "x/tac.cs",
        )
        .expect("a table");
        assert!(
            csharp.contains("Instance.Price = Convert.ToDecimal(value)"),
            "{csharp}"
        );
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_class_behind_it() {
        let generated = member_table(
            CompanionLanguage::CSharp,
            "\u{feff}public class Companion\n{\n    public int Count { get; set; } = 6;\n}\n",
            "x/tac.cs",
        )
        .expect("a table");
        assert!(generated.contains("TacBridge"), "{generated}");
        assert!(generated.contains("[\"count\"]"), "{generated}");
    }

    #[test]
    fn a_field_the_author_cannot_assign_gets_no_setter() {
        let csharp = member_table(
            CompanionLanguage::CSharp,
            Sources::CSHARP_READ_ONLY_FIELDS,
            "x/tac.cs",
        )
        .expect("a table");
        assert!(
            csharp.contains("Field(() => Instance.Count, value =>"),
            "{csharp}"
        );
        for read_only in ["Instance.Label)", "Instance.Fixed)", "Instance.Reading)"] {
            assert!(csharp.contains(read_only), "{read_only} in {csharp}");
        }
        assert!(!csharp.contains("Instance.Label ="), "{csharp}");
    }
}
