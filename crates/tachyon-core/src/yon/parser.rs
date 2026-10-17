use super::plan::{Injection, Plan, Provider, Role, Route, Scope};
use crate::Failure;
use crate::failure::{diagnostic, source_span};
use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;

mod access_marker;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod access_tests;
mod errors;
mod helper_providers;
mod helper_sources;
pub(super) use helper_sources::HelperSources;
mod identity_marker;
mod parameter_markers;
mod routes;
mod ruby;
mod sockets;

pub(crate) struct Parsed {
    pub plan: Plan,
    pub source: String,
}

struct Input<'a> {
    path: &'a Path,
    source: &'a str,
    code: String,
    extension: &'a str,
    manual_helpers: BTreeSet<String>,
}

impl Input<'_> {
    const MAX_SOURCE: usize = 1024 * 1024;
    const MAX_PROVIDERS: usize = 128;
    const MAX_DEPENDENCIES: usize = 32;
    const MAX_ROUTES: usize = 64;
    const VERBS: [&'static str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"];

    fn failure(&self, at: usize, message: impl Into<String>) -> Failure {
        Failure::one(diagnostic(
            2015,
            message,
            Some("A controller is one @RequestRoute(\"/path\") class in server/controllers/. Mark each route method with @GetRoute, @PostRoute, @PutRoute, @PatchRoute or @DeleteRoute, and start its name with that HTTP method (ADR 0048). NeedsAll generates initialization and infers provider dependencies from fields; explicit @Needs remains available for handwritten constructors.".to_owned()),
            source_span(&self.path.to_string_lossy(), at, (at + 1).min(self.source.len())),
        ))
    }

    fn missing_provider(&self, name: &str) -> String {
        if self.manual_helpers.contains(name) {
            return format!(
                "Helper {name} uses explicit caller construction; select class Needs or NeedsAll with declared source-local providers before requesting it through DI or a gateway"
            );
        }
        format!("Gateway target {name} is not a managed Yon type")
    }
}

#[derive(Debug)]
struct Annotation {
    name: String,
    arguments: String,
    span: Range<usize>,
}

struct Declaration {
    annotations: Vec<Annotation>,
    start: usize,
}

struct ManagedType {
    provider: Provider,
    body: Range<usize>,
    declaration: usize,
}

impl ManagedType {
    fn injected_field_at(&self, input: &Input<'_>, field: &str) -> usize {
        let body = &input.code[self.body.clone()];
        body.match_indices(field)
            .map(|(at, _)| at)
            .find(|at| Self::stands_alone(body, *at, field.len()))
            .map_or(self.declaration, |at| self.body.start + at)
    }

    fn stands_alone(body: &str, at: usize, length: usize) -> bool {
        let before = body[..at].chars().next_back();
        let after = body[at + length..].chars().next();
        [Self::is_name(before), Self::is_name(after)] == [false, false]
    }

    fn is_name(character: Option<char>) -> bool {
        character.is_some_and(|character| {
            [character.is_alphanumeric(), character == '_'].contains(&true)
        })
    }
}

pub(crate) fn parse(path: &Path, source: &str) -> Result<Option<Parsed>, Failure> {
    let manual = super::manual_helpers::ManualHelpers::read(path, source)?;
    parse_prepared(path, source, None, &manual)
}

pub(crate) fn parse_prepared(
    path: &Path,
    source: &str,
    target: Option<&str>,
    manual: &BTreeSet<String>,
) -> Result<Option<Parsed>, Failure> {
    parse_with_root(path, source, target, manual)
}

pub(crate) fn parse_test(path: &Path, source: &str) -> Result<Option<Parsed>, Failure> {
    let manual = super::manual_helpers::ManualHelpers::read(path, source)?;
    parse_test_prepared(path, source, &manual)
}

pub(crate) fn parse_test_prepared(
    path: &Path,
    source: &str,
    manual: &BTreeSet<String>,
) -> Result<Option<Parsed>, Failure> {
    let names = path
        .extension()
        .is_some_and(|extension| extension == "rs")
        .then(|| crate::annotation_layout::declaration_names(path, source))
        .transpose()?
        .unwrap_or_default();
    let name = names
        .iter()
        .find(|name| crate::annotation_layout::ClassFileName::matches(path, name))
        .map_or_else(
            || {
                path.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
            },
            String::as_str,
        );
    let Some(mut parsed) = parse_with_root(path, source, Some(name), manual)? else {
        return Ok(None);
    };
    parsed.plan.routes.push(Route {
        verb: String::from("POST"),
        method: String::from("POST"),
        takes_request: path.extension().is_some_and(|value| value == "php"),
        ..Route::default()
    });
    Ok(Some(parsed))
}

fn parse_with_root(
    path: &Path,
    source: &str,
    gateway_target: Option<&str>,
    manual: &BTreeSet<String>,
) -> Result<Option<Parsed>, Failure> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if source.len() > Input::MAX_SOURCE {
        return Err(Failure::one(diagnostic(
            2015,
            "Yon annotation source exceeds 1 MiB",
            None,
            None,
        )));
    }
    let mask = crate::lexical::code_mask(extension, source);
    let input = Input {
        path,
        source,
        code: masked_code(source, mask.code),
        extension: crate::groovy_source::GroovySource::syntax(extension),
        manual_helpers: manual.clone(),
    };
    let declarations = declarations(&input)?;
    if declarations.is_empty() {
        return Ok(None);
    }
    if let Some(error) = mask.error {
        return Err(input.failure(0, error));
    }
    let mut markers = Vec::new();
    let plan = match gateway_target {
        Some(target) => {
            markers.extend(parameter_markers::ParameterMarkers::spans(&input));
            build_gateway_plan(&input, &declarations, target)?
        }
        None => build_plan(&input, &declarations, &mut markers)?,
    };
    Ok(Some(Parsed {
        plan,
        source: strip_annotations(source, &declarations, &markers),
    }))
}

fn masked_code(source: &str, visibility: Vec<bool>) -> String {
    let mut bytes = source.as_bytes().to_vec();
    if bytes.starts_with("\u{feff}".as_bytes()) {
        bytes[..3].fill(b' ');
    }
    for (byte, visible) in bytes.iter_mut().zip(visibility) {
        if !visible && !matches!(*byte, b'\n' | b'\r') {
            *byte = b' ';
        }
    }
    String::from_utf8(bytes).unwrap_or_default()
}

fn build_plan(
    input: &Input<'_>,
    declarations: &[Declaration],
    markers: &mut Vec<Range<usize>>,
) -> Result<Plan, Failure> {
    reject_reserved_declarations(input)?;
    let mut types = managed_types(input, declarations)?;
    let controller = controller(input, &types)?;
    if identity_marker::IdentityMarker::uses(declarations) {
        return build_identity_plan(input, declarations, types, markers);
    }
    if sockets::uses_socket_route(declarations) {
        return build_socket_plan(input, declarations, types, controller);
    }
    if !routes::uses_request_route(declarations) {
        return Err(input.failure(
            0,
            "A controller class needs @RequestRoute(\"/path\"): it marks a controller and gives the path its routes start with",
        ));
    }
    let mapped = routes::mapped(input, declarations, &types, &controller)?;
    let errors = errors::read(input, declarations, &types)?;
    markers.extend(mapped.markers);
    let mut routes = mapped.routes;
    let class_access = access_marker::AccessMarker::class(input, declarations, &controller)?;
    for route in &mut routes {
        route.error_responses = errors.iter().map(Into::into).collect();
        route.access = std::mem::take(&mut route.access).or_class(&class_access);
    }
    add_resources(input, &mut types)?;
    let root = types
        .iter()
        .position(|value| value.provider.role == Role::Controller);
    validate_graph(input, &types, root)?;
    Ok(Plan {
        providers: types.into_iter().map(|value| value.provider).collect(),
        controller,
        routes,
        errors,
    })
}

fn build_socket_plan(
    input: &Input<'_>,
    declarations: &[Declaration],
    mut types: Vec<ManagedType>,
    controller: String,
) -> Result<Plan, Failure> {
    let mut socket = sockets::mapped(input, declarations, &types, &controller)?;
    socket.access = access_marker::AccessMarker::class(input, declarations, &controller)?;
    add_resources(input, &mut types)?;
    let root = types
        .iter()
        .position(|value| value.provider.role == Role::Controller);
    validate_graph(input, &types, root)?;
    Ok(Plan {
        providers: types.into_iter().map(|value| value.provider).collect(),
        controller,
        routes: vec![socket],
        errors: Vec::new(),
    })
}

fn build_identity_plan(
    input: &Input<'_>,
    declarations: &[Declaration],
    mut types: Vec<ManagedType>,
    markers: &mut Vec<Range<usize>>,
) -> Result<Plan, Failure> {
    let root = types
        .iter()
        .position(|value| value.provider.role == Role::Controller);
    let mut found = routes::MappedRoutes {
        routes: Vec::new(),
        markers: Vec::new(),
    };
    let managed = root.map(|index| &types[index]).ok_or_else(|| {
        input.failure(
            0,
            "An identity provider file holds one @IdentityProvider class",
        )
    })?;
    let route = identity_marker::IdentityMarker::route(input, declarations, managed, &mut found)?;
    let controller = managed.provider.name.clone();
    markers.extend(found.markers);
    add_resources(input, &mut types)?;
    validate_graph(input, &types, root)?;
    Ok(Plan {
        providers: types.into_iter().map(|value| value.provider).collect(),
        controller,
        routes: vec![route],
        errors: Vec::new(),
    })
}

fn build_gateway_plan(
    input: &Input<'_>,
    declarations: &[Declaration],
    target: &str,
) -> Result<Plan, Failure> {
    reject_reserved_declarations(input)?;
    let mut types = managed_types(input, declarations)?;
    add_resources(input, &mut types)?;
    let Some(root) = types.iter().position(|value| value.provider.name == target) else {
        return Err(input.failure(0, input.missing_provider(target)));
    };
    validate_graph(input, &types, Some(root))?;
    Ok(Plan {
        providers: types.into_iter().map(|value| value.provider).collect(),
        controller: target.to_owned(),
        routes: Vec::new(),
        errors: Vec::new(),
    })
}

fn strip_annotations(
    source: &str,
    declarations: &[Declaration],
    markers: &[Range<usize>],
) -> String {
    let mut stripped = source.as_bytes().to_vec();
    for declaration in declarations {
        for annotation in &declaration.annotations {
            stripped[annotation.span.clone()].fill(b' ');
        }
    }
    for marker in markers {
        stripped[marker.clone()].fill(b' ');
    }
    String::from_utf8(stripped).unwrap_or_default()
}

fn add_resources(input: &Input<'_>, types: &mut Vec<ManagedType>) -> Result<(), Failure> {
    for name in ["DataSource", "RedisTemplate", "ApplicationEventPublisher"] {
        if types.iter().any(|ty| ty.provider.name == name) {
            return Err(input.failure(
                0,
                format!("{name} is a framework-owned managed resource token"),
            ));
        }
        if types.iter().any(|ty| {
            ty.provider
                .dependencies
                .iter()
                .any(|dependency| dependency == name)
        }) {
            types.push(ManagedType {
                provider: Provider {
                    name: name.into(),
                    role: Role::Service,
                    scope: Scope::Singleton,
                    dependencies: Vec::new(),
                    injections: Vec::new(),
                },
                body: 0..0,
                declaration: 0,
            });
        }
    }
    if types.len() > Input::MAX_PROVIDERS {
        return Err(input.failure(0, "Yon provider limit includes managed resources"));
    }
    Ok(())
}

fn declarations(input: &Input<'_>) -> Result<Vec<Declaration>, Failure> {
    let mut declarations = Vec::new();
    let mut pending = Vec::new();
    let mut offset = 0;
    for line in input.code.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            offset += line.len();
            continue;
        }
        if input.extension == "rb" && matches!(trimmed, "#[NeedsAll]" | "#[NeedsAll()]") {
            offset += line.len();
            continue;
        }
        let start = offset + line.len() - line.trim_start().len();
        if let Some(annotation) = annotation(
            input,
            start,
            offset + line.trim_end_matches(['\r', '\n']).len(),
        )? {
            pending.push(annotation);
        } else if !pending.is_empty() {
            if trimmed.starts_with('@') || trimmed.starts_with("#[") || trimmed.starts_with('[') {
                return Err(input.failure(
                    start,
                    "Do not mix unknown decorators with Yon annotations on one declaration",
                ));
            }
            declarations.push(Declaration {
                annotations: std::mem::take(&mut pending),
                start,
            });
        }
        offset += line.len();
    }
    if let Some(annotation) = pending.first() {
        return Err(input.failure(
            annotation.span.start,
            "Yon annotation has no following declaration",
        ));
    }
    if declarations.len() > Input::MAX_PROVIDERS + Input::MAX_ROUTES {
        return Err(input.failure(0, "Yon module exceeds its bounded declaration limit"));
    }
    Ok(declarations)
}

pub(crate) fn is_yon_annotation(written: &str) -> bool {
    framework_name(&written.replace('_', "").to_ascii_lowercase())
}

fn framework_name(name: &str) -> bool {
    matches!(
        name,
        "service"
            | "store"
            | "client"
            | "helper"
            | "converter"
            | "lifetime"
            | "needs"
            | "stream"
            | "trace"
            | "useguards"
            | "useinterceptors"
            | "usefilters"
            | "middleware"
            | "guard"
            | "interceptor"
            | "exceptionfilter"
            | "module"
            | "injectable"
            | "errorhandler"
            | "handleserror"
            | "controlleradvice"
            | "restcontrolleradvice"
            | "exceptionhandler"
            | identity_marker::IdentityMarker::CLASS
    ) || routes::RouteMarker::is_name(name)
        || access_marker::AccessMarker::is_name(name)
        || sockets::SocketMarker::is_name(name)
        || Input::VERBS
            .iter()
            .any(|verb| verb.eq_ignore_ascii_case(name))
}

fn annotation(input: &Input<'_>, start: usize, end: usize) -> Result<Option<Annotation>, Failure> {
    let line = &input.code[start..end];
    let prefixes: &[&str] = match input.extension {
        "rs" | "php" | "rb" => &["#["],
        "cs" => &["["],
        _ => &["@"],
    };
    let Some(prefix) = prefixes.iter().find(|prefix| line.starts_with(**prefix)) else {
        return reject_inline(input, start, end);
    };
    let rest = &line[prefix.len()..];
    let length = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
        .count();
    let qualified = &rest[..length];
    if input.extension == "rs" && qualified == "allow" {
        return Ok(None);
    }
    let name = qualified
        .strip_prefix("yon::")
        .unwrap_or(qualified)
        .replace('_', "")
        .to_ascii_lowercase();
    if !framework_name(&name) {
        return Ok(None);
    }
    if name == "needs" && !annotates_type(input, end) {
        return Ok(None);
    }
    validate_annotation_name(input, start, qualified, &name)?;
    let arguments = annotation_arguments(input, start, end, prefix, length)?;
    Ok(Some(Annotation {
        name,
        arguments,
        span: start..end,
    }))
}

fn annotates_type(input: &Input<'_>, end: usize) -> bool {
    for line in input.code[end..].lines() {
        let trimmed = line.trim();
        let is_annotation = trimmed.starts_with(['@', '[', '#']);
        if trimmed.is_empty() || is_annotation {
            continue;
        }
        return crate::annotation_layout::declares_type(input.extension, trimmed);
    }
    false
}

fn validate_annotation_name(
    input: &Input<'_>,
    start: usize,
    qualified: &str,
    name: &str,
) -> Result<(), Failure> {
    if matches!(
        name,
        "controlleradvice"
            | "restcontrolleradvice"
            | "exceptionhandler"
            | "exceptionfilter"
            | "usefilters"
    ) {
        return Err(input.failure(start, format!("Yon uses @ErrorHandler on the class and @HandlesError on its typed error method instead of {qualified}")));
    }
    if Input::VERBS
        .iter()
        .any(|verb| verb.eq_ignore_ascii_case(name))
        || name == "trace"
    {
        return Err(input.failure(start, format!("{qualified} is not a Tachyon annotation. Use @GetRoute, @PostRoute, @PutRoute, @PatchRoute or @DeleteRoute: @GetRoute means the method answers GET requests")));
    }
    if matches!(
        name,
        "useguards"
            | "useinterceptors"
            | "usefilters"
            | "middleware"
            | "guard"
            | "interceptor"
            | "exceptionfilter"
            | "module"
            | "injectable"
    ) {
        return Err(input.failure(start, format!("Yon does not implement the {qualified} annotation; use existing middleware and explicit application calls")));
    }
    if qualified.contains(':') && !qualified.starts_with("yon::") {
        return Err(input.failure(start, "Yon annotations accept only the yon:: namespace"));
    }
    Ok(())
}

fn annotation_arguments(
    input: &Input<'_>,
    start: usize,
    end: usize,
    prefix: &str,
    length: usize,
) -> Result<String, Failure> {
    let rest = &input.code[start + prefix.len()..end];
    let mut tail = rest[length..].trim();
    if prefix != "@" {
        tail = tail
            .strip_suffix(']')
            .ok_or_else(|| input.failure(start, "Close the Yon annotation on the same line"))?
            .trim_end();
    }
    if tail.is_empty() {
        return Ok(String::new());
    }
    if !tail.starts_with('(') || !tail.ends_with(')') {
        return Err(input.failure(start, "Place each Yon annotation on its own complete line"));
    }
    let opening = start + prefix.len() + length + rest[length..].find('(').unwrap_or(0);
    let closing = matching(&input.code, opening, b'(', b')')
        .ok_or_else(|| input.failure(start, "Unbalanced Yon annotation arguments"))?;
    if closing >= end
        || !input.code[closing + 1..end]
            .trim()
            .trim_end_matches(']')
            .trim()
            .is_empty()
    {
        return Err(input.failure(start, "Place each Yon annotation on its own complete line"));
    }
    Ok(input.source[opening + 1..closing].trim().to_owned())
}

fn reject_inline(
    input: &Input<'_>,
    start: usize,
    end: usize,
) -> Result<Option<Annotation>, Failure> {
    let line = &input.code[start..end];
    let prefix = match input.extension {
        "php" | "rs" | "rb" => "#[",
        "cs" => "[",
        _ => "@",
    };
    for (offset, _) in line.match_indices(prefix) {
        if input.extension == "cs"
            && line[..offset]
                .trim_end()
                .chars()
                .next_back()
                .is_some_and(|character| !matches!(character, '{' | '}' | ';'))
        {
            continue;
        }
        let rest = &line[offset + prefix.len()..];
        let name = rest
            .split(|character: char| {
                !character.is_ascii_alphanumeric() && character != '_' && character != ':'
            })
            .next()
            .unwrap_or("");
        if !inline_declaration_follows(
            input,
            start + offset + prefix.len() + name.len(),
            end,
            prefix,
        ) {
            continue;
        }
        reject_inline_marker(input, start + offset, name)?;
    }
    Ok(None)
}

fn reject_inline_marker(input: &Input<'_>, start: usize, name: &str) -> Result<(), Failure> {
    let marker = name.strip_prefix("yon::").unwrap_or(name);
    if !framework_name(&marker.to_ascii_lowercase()) {
        return Ok(());
    }
    if Input::VERBS
        .iter()
        .any(|verb| verb.eq_ignore_ascii_case(marker))
        || marker.eq_ignore_ascii_case("trace")
    {
        return Err(input.failure(start, format!("{marker} is not a Tachyon annotation. Use @GetRoute, @PostRoute, @PutRoute, @PatchRoute or @DeleteRoute: @GetRoute means the method answers GET requests")));
    }
    Err(input.failure(
        start,
        "Place each Yon annotation on its own line before its declaration",
    ))
}

fn inline_declaration_follows(
    input: &Input<'_>,
    name_end: usize,
    line_end: usize,
    prefix: &str,
) -> bool {
    let rest = &input.code[name_end..line_end];
    let whitespace = rest.len() - rest.trim_start().len();
    let mut next = name_end + whitespace;
    if input.code.as_bytes().get(next) == Some(&b'(') {
        let Some(closing) =
            matching(&input.code, next, b'(', b')').filter(|closing| *closing < line_end)
        else {
            return false;
        };
        next = closing + 1;
    }
    if prefix != "@" {
        let suffix = input.code[next..line_end].trim_start();
        if !suffix.starts_with(']') {
            return false;
        }
        next = line_end - suffix.len() + 1;
    }
    input.code[next..line_end]
        .trim_start()
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'@' | b'#' | b'['))
}

fn words(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .filter(|value| !value.is_empty())
}

fn identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && value.len() <= 128
        && value != "Handler"
        && !value.to_ascii_lowercase().starts_with("__yon")
}

fn reject_reserved_declarations(input: &Input<'_>) -> Result<(), Failure> {
    if words(&input.code).any(|word| word.to_ascii_lowercase().starts_with("__yon")) {
        return Err(input.failure(
            0,
            "Annotated modules cannot use reserved __Yon-prefixed identifiers",
        ));
    }
    if input
        .code
        .lines()
        .any(|line| declares_namespace(input.extension, line))
    {
        return Err(input.failure(0, "Annotated Yon sources use the default package or namespace; import application dependencies without wrapping the controller module"));
    }
    let mut tokens = words(&input.code);
    while let Some(token) = tokens.next() {
        if !names_a_type(input.extension, token) {
            continue;
        }
        if let Some(name) = tokens.next() {
            if ["DataSource", "RedisTemplate", "ApplicationEventPublisher"]
                .iter()
                .any(|reserved| {
                    name == *reserved
                        || (input.extension == "php" && name.eq_ignore_ascii_case(reserved))
                })
            {
                return Err(input.failure(
                    0,
                    "Annotated modules cannot shadow framework-owned DataSource, RedisTemplate or ApplicationEventPublisher types",
                ));
            }
            let reserved = name == "Handler"
                || name.to_ascii_lowercase().starts_with("__yon")
                || (input.extension == "php" && name.eq_ignore_ascii_case("Handler"));
            if reserved {
                return Err(input.failure(
                    0,
                    "Annotated modules cannot declare reserved Handler or __Yon-prefixed types",
                ));
            }
        }
    }
    Ok(())
}

fn declares_namespace(extension: &str, line: &str) -> bool {
    let line = line.trim_start();
    if extension == "rb" {
        return line.starts_with("module ");
    }
    if extension == "go" {
        return line.starts_with("package ") && line.trim_end() != "package main";
    }
    line.starts_with("namespace ") || line.starts_with("package ")
}

fn names_a_type(extension: &str, token: &str) -> bool {
    match (extension, token) {
        ("go", _) => token == "type",
        ("rb" | "py", _) => token == "class",
        (_, "record") => matches!(extension, "java" | "cs" | "groovy"),
        (_, "object") => extension == "kt",
        _ => matches!(token, "class" | "struct" | "interface" | "enum"),
    }
}

fn declared_type(input: &Input<'_>, declaration: &Declaration) -> Option<(String, usize)> {
    let line = input.code[declaration.start..].lines().next()?;
    if input.extension == "go" {
        let declared = crate::go_source::type_at(&input.code, declaration.start)?;
        return Some((declared.name, declaration.start));
    }
    for token in line.split_whitespace() {
        if matches!(token, "class" | "struct") {
            let keyword_at = line.find(token)?;
            let name = line[keyword_at + token.len()..]
                .trim_start()
                .split(|character: char| {
                    character.is_whitespace() || matches!(character, '{' | '(' | ':' | ';' | '<')
                })
                .next()?;
            return Some((name.to_owned(), declaration.start));
        }
        if matches!(token, "fn" | "def" | "function" | "fun") {
            return None;
        }
    }
    None
}

fn role(name: &str) -> Option<Role> {
    match name {
        "requestroute" | "socketroute" | identity_marker::IdentityMarker::CLASS => {
            Some(Role::Controller)
        }
        "service" => Some(Role::Service),
        "store" => Some(Role::Repository),
        "client" => Some(Role::Client),
        "helper" | "converter" => Some(Role::Component),
        "errorhandler" => Some(Role::ErrorHandler),
        _ => None,
    }
}

fn managed_types(
    input: &Input<'_>,
    declarations: &[Declaration],
) -> Result<Vec<ManagedType>, Failure> {
    let mut types = Vec::new();
    let mut names = BTreeSet::new();
    for declaration in declarations {
        let Some((name, start)) = declared_type(input, declaration) else {
            continue;
        };
        validate_managed_declaration(input, start, &name)?;
        let key = if input.extension == "php" {
            name.to_ascii_lowercase()
        } else {
            name.clone()
        };
        if !names.insert(key) {
            return Err(input.failure(start, format!("Duplicate Yon provider {name}")));
        }
        let provider = provider(input, declaration, name)?;
        let body = type_body(input, declaration.start)?;
        types.push(ManagedType {
            provider,
            body,
            declaration: start,
        });
    }
    if names.len() > Input::MAX_PROVIDERS {
        return Err(input.failure(0, "Yon permits at most 128 managed providers per source"));
    }
    types = helper_providers::HelperProviders::select(input, types)?;
    if input.extension == "go" {
        add_go_injections(input, &mut types)?;
    } else if input.extension == "rb" {
        ruby::RubyMethod::add_field_injections(input, &mut types)?;
        ruby::RubyMethod::validate_constructors(input, &types)?;
    } else {
        validate_constructors(input, &types)?;
    }
    Ok(types)
}

fn validate_managed_declaration(
    input: &Input<'_>,
    start: usize,
    name: &str,
) -> Result<(), Failure> {
    let nested = if matches!(input.extension, "py" | "rb") {
        start != input.code[..start].rfind('\n').map_or(0, |at| at + 1)
    } else {
        brace_depth(&input.code[..start]) != 0
    };
    if nested {
        return Err(input.failure(start, "Managed Yon types must be declared at module top level, not nested in a function, class, or namespace"));
    }
    if !identifier(name) || (input.extension == "php" && name.eq_ignore_ascii_case("Handler")) {
        return Err(input.failure(start, "Managed type identifiers must be ASCII identifiers and cannot use Handler or the __yon_ prefix"));
    }
    Ok(())
}

fn provider(
    input: &Input<'_>,
    declaration: &Declaration,
    name: String,
) -> Result<Provider, Failure> {
    let roles = declaration
        .annotations
        .iter()
        .filter_map(|annotation| role(&annotation.name))
        .collect::<Vec<_>>();
    if roles.len() != 1 {
        return Err(input.failure(
            declaration.start,
            "Every managed type requires exactly one Yon role annotation",
        ));
    }
    let role = roles[0];
    let mut scope = if matches!(role, Role::Controller | Role::ErrorHandler) {
        Scope::Request
    } else {
        Scope::Singleton
    };
    let mut dependencies = Vec::new();
    let mut seen = BTreeSet::new();
    for annotation in &declaration.annotations {
        if !seen.insert(&annotation.name) {
            return Err(input.failure(annotation.span.start, "Duplicate Yon annotation"));
        }
        match annotation.name.as_str() {
            "lifetime" => scope = scope_argument(input, annotation)?,
            "needs" => dependencies = injection_arguments(input, annotation)?,
            name if self::role(name).is_some() && annotation.arguments.is_empty() => {},
            routes::RouteMarker::CLASS | sockets::SocketMarker::CLASS => {},
            name if access_marker::AccessMarker::is_name(name) && role == Role::Controller => {},
            _ => return Err(input.failure(annotation.span.start, "Type annotations accept one role, @Lifetime and @Needs only; paths belong to filesystem routing")),
        }
    }
    Ok(Provider {
        name,
        role,
        scope,
        dependencies,
        injections: Vec::new(),
    })
}

fn scope_argument(input: &Input<'_>, annotation: &Annotation) -> Result<Scope, Failure> {
    match annotation.arguments.as_str() {
        "\"singleton\"" | "'singleton'" => Ok(Scope::Singleton),
        "\"request\"" | "'request'" => Ok(Scope::Request),
        "\"transient\"" | "'transient'" => Ok(Scope::Transient),
        _ => Err(input.failure(
            annotation.span.start,
            "@Lifetime requires the literal string singleton, request, or transient",
        )),
    }
}

fn injection_arguments(input: &Input<'_>, annotation: &Annotation) -> Result<Vec<String>, Failure> {
    if annotation.arguments.is_empty() {
        return Ok(Vec::new());
    }
    let arguments = if input.extension == "java" {
        annotation
            .arguments
            .strip_prefix(['{', '['])
            .and_then(|value| value.strip_suffix(['}', ']']))
            .unwrap_or(&annotation.arguments)
    } else {
        &annotation.arguments
    };
    let tokens = arguments
        .split(',')
        .map(str::trim)
        .map(|token| injection_token(input.extension, token))
        .collect::<Vec<_>>();
    if tokens.len() > Input::MAX_DEPENDENCIES || tokens.iter().any(|token| !identifier(token)) {
        return Err(input.failure(annotation.span.start, "@Needs accepts at most 32 source-local provider identifiers, not expressions or strings"));
    }
    Ok(tokens.into_iter().map(str::to_owned).collect())
}

fn injection_token<'a>(extension: &str, token: &'a str) -> &'a str {
    crate::behavior::literal::class_reference(extension, token).unwrap_or(token)
}

fn type_body(input: &Input<'_>, start: usize) -> Result<Range<usize>, Failure> {
    if matches!(input.extension, "py" | "rb") {
        return python_body(input, start);
    }
    let suffix = &input.code[start..];
    if input.extension == "kt" {
        let line_end = suffix.find('\n').map_or(input.code.len(), |at| start + at);
        if !input.code[start..line_end].contains('{') {
            return Ok(line_end..line_end);
        }
    }
    let opening = suffix
        .find(['{', ';'])
        .ok_or_else(|| input.failure(start, "Managed type requires a complete body"))?
        + start;
    if input.code.as_bytes()[opening] == b';' && input.extension == "rs" {
        return Ok(opening..opening);
    }
    let closing = matching(&input.code, opening, b'{', b'}')
        .ok_or_else(|| input.failure(start, "Unbalanced managed type body"))?;
    Ok(opening + 1..closing)
}

fn python_body(input: &Input<'_>, start: usize) -> Result<Range<usize>, Failure> {
    let line_start = input.code[..start].rfind('\n').map_or(0, |at| at + 1);
    let indent = start - line_start;
    let opening = input.code[start..]
        .find('\n')
        .map(|at| start + at + 1)
        .ok_or_else(|| {
            input.failure(
                start,
                "Use a multiline body for decorated Python declarations",
            )
        })?;
    let mut cursor = opening;
    for line in input.code[opening..].split_inclusive('\n') {
        if !line.trim().is_empty() && line.len() - line.trim_start().len() <= indent {
            return Ok(opening..cursor);
        }
        cursor += line.len();
    }
    Ok(opening..input.code.len())
}

fn matching(source: &str, opening: usize, left: u8, right: u8) -> Option<usize> {
    let mut depth = 0_u32;
    for (offset, byte) in source.bytes().enumerate().skip(opening) {
        if byte == left {
            depth = depth.checked_add(1)?;
        }
        if byte == right {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(offset);
            }
        }
    }
    None
}

fn controller(input: &Input<'_>, types: &[ManagedType]) -> Result<String, Failure> {
    let controllers = types
        .iter()
        .filter(|value| value.provider.role == Role::Controller)
        .collect::<Vec<_>>();
    if controllers.len() != 1 {
        return Err(input.failure(
            0,
            "A controller file holds exactly one controller class, marked @RequestRoute or @SocketRoute",
        ));
    }
    Ok(controllers[0].provider.name.clone())
}

fn controller_methods(input: &Input<'_>, managed: &ManagedType) -> Result<Vec<usize>, Failure> {
    named_methods(input, managed, &Input::VERBS)
}

fn named_methods(
    input: &Input<'_>,
    managed: &ManagedType,
    names: &[&str],
) -> Result<Vec<usize>, Failure> {
    if matches!(input.extension, "py" | "rb") {
        return Ok(python_http_methods(input, &managed.body, names));
    }
    if input.extension == "go" {
        return Ok(go_http_methods(input, &managed.provider.name, names));
    }
    let bodies = if input.extension == "rs" {
        rust_type_bodies(input, managed)?
    } else {
        vec![managed.body.clone()]
    };
    let mut methods = Vec::new();
    for body in bodies {
        methods.extend(braced_http_methods(input, &body, names));
    }
    Ok(methods)
}

fn go_http_methods(input: &Input<'_>, owner: &str, names: &[&str]) -> Vec<usize> {
    let mut methods = Vec::new();
    for method in crate::go_source::methods(&input.code) {
        if method.receiver_type == owner && names.contains(&method.name.as_str()) {
            methods.push(method.start);
        }
    }
    methods
}

fn python_http_methods(input: &Input<'_>, body: &Range<usize>, names: &[&str]) -> Vec<usize> {
    let mut methods = Vec::new();
    let expected_indent = input.code[body.clone()]
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len());
    let mut offset = body.start;
    for line in input.code[body.clone()].split_inclusive('\n') {
        let trimmed = line.trim_start();
        let start = offset + line.len() - trimmed.len();
        if !trimmed.is_empty() && expected_indent == Some(line.len() - trimmed.len()) {
            let mut tokens = words(trimmed);
            let first = tokens.next();
            let keyword = if first == Some("async") {
                tokens.next()
            } else {
                first
            };
            if keyword == Some("def") && tokens.next().is_some_and(|name| names.contains(&name)) {
                methods.push(start);
                if methods.len() > Input::VERBS.len() {
                    break;
                }
            }
        }
        offset += line.len();
    }
    methods
}

fn braced_http_methods(input: &Input<'_>, body: &Range<usize>, names: &[&str]) -> Vec<usize> {
    let bytes = input.code.as_bytes();
    let mut methods = Vec::new();
    let mut state = MemberScanner {
        brace: 0,
        paren: 0,
        member: body.start,
        expression: false,
    };
    let mut at = body.start;
    while at < body.end {
        let byte = bytes[at];
        if state.at_member() && (byte.is_ascii_alphabetic() || byte == b'_') {
            let end = identifier_end(bytes, at, body.end);
            if state.is_http_candidate(&input.code[at..end], &input.code[end..body.end], names) {
                methods.push(member_start(input, state.member, at));
                if methods.len() > Input::VERBS.len() {
                    break;
                }
            }
            at = end;
            continue;
        }
        state.advance(byte, at);
        at += 1;
    }
    methods
}

struct MemberScanner {
    brace: i32,
    paren: i32,
    member: usize,
    expression: bool,
}

impl MemberScanner {
    fn at_member(&self) -> bool {
        self.brace == 0 && self.paren == 0
    }

    fn is_http_candidate(&self, name: &str, suffix: &str, names: &[&str]) -> bool {
        !self.expression
            && names.contains(&name)
            && suffix.trim_start().starts_with(['(', '<', ':', '=', ';'])
    }

    fn reset_member(&mut self, at: usize) {
        self.member = at + 1;
        self.expression = false;
    }

    fn advance(&mut self, byte: u8, at: usize) {
        if byte == b'{' {
            self.brace += 1;
            return;
        }
        if byte == b'}' {
            self.brace -= 1;
            if self.brace == 0 {
                self.reset_member(at);
            }
            return;
        }
        if self.brace != 0 {
            return;
        }
        match byte {
            b'(' => self.paren += 1,
            b')' => self.paren -= 1,
            _ => self.advance_member(byte, at),
        }
    }

    fn advance_member(&mut self, byte: u8, at: usize) {
        if self.paren != 0 {
            return;
        }
        match byte {
            b';' => self.reset_member(at),
            b'=' => self.expression = true,
            b'\n' if self.expression => self.reset_member(at),
            _ => {}
        }
    }
}

fn identifier_end(bytes: &[u8], start: usize, limit: usize) -> usize {
    let mut end = start + 1;
    while end < limit && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    end
}

fn member_start(input: &Input<'_>, member: usize, name: usize) -> usize {
    let prefix = &input.code[member..name];
    let line_start = prefix.rfind('\n').map_or(member, |at| member + at + 1);
    let mut start = line_start;
    while start > member {
        let prior_end = start - 1;
        let prior_start = input.code[member..prior_end]
            .rfind('\n')
            .map_or(member, |at| member + at + 1);
        let prior = input.code[prior_start..prior_end].trim();
        if prior.is_empty() {
            start = prior_start;
            continue;
        }
        if words(prior).all(|word| {
            matches!(
                word,
                "static"
                    | "public"
                    | "private"
                    | "protected"
                    | "internal"
                    | "async"
                    | "suspend"
                    | "pub"
                    | "crate"
            )
        }) {
            start = prior_start;
            continue;
        }
        break;
    }
    start + input.code[start..name].len() - input.code[start..name].trim_start().len()
}

fn method_owner<'a>(
    input: &Input<'_>,
    start: usize,
    types: &'a [ManagedType],
) -> Result<&'a str, Failure> {
    if input.extension == "go" {
        return go_method_owner(input, start, types);
    }
    for managed in types {
        let body = if input.extension == "rs" {
            rust_impl(input, &managed.provider.name, start)?
        } else {
            Some(managed.body.clone())
        };
        if let Some(body) = body.filter(|body| body.contains(&start)) {
            let indented = matches!(input.extension, "py" | "rb");
            if !indented && brace_depth(&input.code[body.start..start]) != 0 {
                continue;
            }
            if indented && !python_direct_member(input, &body, start) {
                continue;
            }
            return Ok(&managed.provider.name);
        }
    }
    Err(input.failure(
        start,
        "Place annotated routes directly inside the controller class or its inherent Rust impl",
    ))
}

fn go_method_owner<'a>(
    input: &Input<'_>,
    start: usize,
    types: &'a [ManagedType],
) -> Result<&'a str, Failure> {
    let method = crate::go_source::method_at(&input.code, start);
    let owner = method.and_then(|method| {
        types
            .iter()
            .find(|managed| managed.provider.name == method.receiver_type)
    });
    match owner {
        Some(managed) => Ok(&managed.provider.name),
        None => Err(input.failure(
            start,
            "Place annotated routes on a method of the controller type, for example func (controller *OrdersController) GET(request *http.Request) any",
        )),
    }
}

fn brace_depth(code: &str) -> i32 {
    let mut depth = 0;
    for byte in code.bytes() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn python_direct_member(input: &Input<'_>, body: &Range<usize>, start: usize) -> bool {
    let indent = |at: usize| at - input.code[..at].rfind('\n').map_or(0, |value| value + 1);
    let first = input.code[body.clone()]
        .char_indices()
        .find(|(_, value)| !value.is_whitespace())
        .map(|(at, _)| body.start + at);
    first.is_some_and(|first| indent(first) == indent(start))
}

fn rust_impl(
    input: &Input<'_>,
    name: &str,
    method: usize,
) -> Result<Option<Range<usize>>, Failure> {
    let mut offset = 0;
    for line in input.code.split_inclusive('\n') {
        let tokens = words(line).collect::<Vec<_>>();
        if tokens.first() == Some(&"impl") && tokens.get(1) == Some(&name) {
            let body = type_body(input, offset)?;
            if body.contains(&method) && !tokens.contains(&"for") {
                return Ok(Some(body));
            }
        }
        offset += line.len();
    }
    Ok(None)
}

fn validate_streaming_method(
    input: &Input<'_>,
    start: usize,
    signature_end: usize,
    streaming: bool,
) -> Result<(), Failure> {
    if streaming
        && input.extension == "py"
        && words(&input.code[start..signature_end]).any(|word| word == "async")
    {
        return Err(input.failure(
            start,
            "Python async generators are not supported; use a synchronous generator for streaming",
        ));
    }
    if streaming && input.extension == "java" {
        return Err(input.failure(
            start,
            "@StreamRoute needs JavaScript, TypeScript, Python, PHP, Kotlin, C#, F#, Dart, Ruby, Lua, Go or Rust: Java and Groovy have no generator to produce the stream",
        ));
    }
    Ok(())
}

fn reject_python_static(input: &Input<'_>, start: usize) -> Result<(), Failure> {
    if input.extension != "py" {
        return Ok(());
    }
    let line_start = input.code[..start].rfind('\n').map_or(0, |at| at + 1);
    for line in input.code[..line_start].lines().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('@') {
            break;
        }
        if line.starts_with("@staticmethod") || line.starts_with("@classmethod") {
            return Err(input.failure(
                start,
                "Controller HTTP methods must be instance methods, not staticmethod or classmethod",
            ));
        }
    }
    Ok(())
}

struct Signature {
    name: String,
    end: usize,
    generator: bool,
}

fn method_signature(input: &Input<'_>, start: usize) -> Result<Signature, Failure> {
    if input.extension == "rb" {
        return ruby::RubyMethod::signature(input, start);
    }
    let suffix = &input.code[start..];
    let signature_start = match input.extension {
        "rs" => suffix
            .find("fn ")
            .map_or(start, |offset| start + offset + 3),
        "go" => go_receiver_end(input, start),
        _ => start,
    };
    let opening = input.code[signature_start..]
        .find('(')
        .map(|at| signature_start + at)
        .ok_or_else(|| input.failure(start, "Annotated methods require a callable declaration"))?;
    let prefix = &input.code[start..opening];
    validate_method_prefix(input, start, prefix)?;
    let name = prefix
        .trim_end()
        .rsplit(|character: char| character.is_whitespace() || character == '*')
        .next()
        .unwrap_or("");
    if !identifier(name) {
        return Err(input.failure(start, "Annotated methods require non-reserved ASCII identifiers, without generics or computed names"));
    }
    let closing = matching(&input.code, opening, b'(', b')')
        .ok_or_else(|| input.failure(start, "Unbalanced annotated method parameters"))?;
    Ok(Signature {
        name: name.to_owned(),
        end: closing + 1,
        generator: input.extension != "go" && prefix.contains('*'),
    })
}

fn go_receiver_end(input: &Input<'_>, start: usize) -> usize {
    let Some(opening) = input.code[start..].find('(').map(|at| start + at) else {
        return start;
    };
    matching(&input.code, opening, b'(', b')').map_or(start, |closing| closing + 1)
}

fn validate_method_prefix(input: &Input<'_>, start: usize, prefix: &str) -> Result<(), Failure> {
    if prefix.contains(['{', '}', ';', '='])
        || words(prefix).any(|word| {
            matches!(
                word,
                "static" | "abstract" | "private" | "protected" | "suspend"
            )
        })
    {
        return Err(input.failure(
            start,
            "Annotated routes must be concrete accessible instance methods",
        ));
    }
    if input.extension == "rs" && words(prefix).any(|word| word == "async") {
        return Err(input.failure(start, "Rust async controller methods are not supported by the synchronous Yon protocol adapter"));
    }
    Ok(())
}

fn parameters(input: &Input<'_>, value: &str, at: usize) -> Result<Vec<String>, Failure> {
    let mut parameters = Vec::new();
    let mut depth = 0_i32;
    let mut start = 0;
    for (index, byte) in value.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' | b'<' => depth += 1,
            b')' | b']' | b'}' | b'>' => depth -= 1,
            b',' if depth == 0 => {
                parameters.push(value[start..index].trim().to_owned());
                start = index + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return Err(input.failure(at, "Unsupported method parameter syntax"));
        }
    }
    if !value[start..].trim().is_empty() {
        parameters.push(value[start..].trim().to_owned());
    }
    if depth != 0
        || parameters.iter().any(|value| {
            value.is_empty()
                || value.contains('=')
                || value.starts_with("...")
                || value.starts_with('*')
        })
    {
        return Err(input.failure(
            at,
            "Use explicit positional parameters without defaults, rest arguments, or destructuring",
        ));
    }
    Ok(parameters)
}

fn instance_parameters(
    input: &Input<'_>,
    parameters: &[String],
    at: usize,
) -> Result<usize, Failure> {
    match input.extension {
        "py" if parameters.first().is_some_and(|value| value == "self") => Ok(parameters.len() - 1),
        "rs" if parameters.first().is_some_and(|value| value == "&self") => {
            Ok(parameters.len() - 1)
        }
        "py" | "rs" => Err(input.failure(
            at,
            "Annotated routes require an instance receiver (self or &self)",
        )),
        _ => Ok(parameters.len()),
    }
}

fn method_body(input: &Input<'_>, start: usize, end: usize) -> Result<Range<usize>, Failure> {
    if input.extension == "rb" {
        return Ok(ruby::RubyMethod::body(input, start, end));
    }
    if input.extension == "py" {
        return python_body(input, start);
    }
    if input.extension == "go" {
        return crate::go_source::method_at(&input.code, start)
            .map(|method| method.body)
            .ok_or_else(|| input.failure(start, "Annotated method requires a concrete body"));
    }
    let suffix = &input.code[end..];
    if matches!(input.extension, "kt" | "cs") {
        let header_end = suffix.find(['{', '\n']).unwrap_or(suffix.len());
        if suffix[..header_end].contains('=') {
            if input.extension == "kt"
                && words(&suffix[..header_end]).any(|word| word == "sequence")
                && suffix.as_bytes().get(header_end) == Some(&b'{')
            {
                let opening = end + header_end;
                let closing = matching(&input.code, opening, b'{', b'}')
                    .ok_or_else(|| input.failure(start, "Unbalanced Kotlin sequence body"))?;
                return Ok(opening + 1..closing);
            }
            return Ok(end..end);
        }
    }
    if input.extension == "dart" {
        let header_end = suffix.find(['{', ';']).unwrap_or(suffix.len());
        if suffix[..header_end].contains("=>") {
            return Ok(end..end);
        }
    }
    let opening = suffix
        .find('{')
        .map(|at| end + at)
        .ok_or_else(|| input.failure(start, "Annotated method requires a concrete body"))?;
    let closing = matching(&input.code, opening, b'{', b'}')
        .ok_or_else(|| input.failure(start, "Unbalanced annotated method body"))?;
    Ok(opening + 1..closing)
}

fn generator_body(input: &Input<'_>, body: &Range<usize>, _declaration: usize) -> bool {
    if matches!(input.extension, "js" | "ts") {
        return false;
    }
    if matches!(input.extension, "py" | "rb") {
        let mut nested_indent = None;
        for line in input.code[body.clone()].lines() {
            if line.trim().is_empty() {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if nested_indent.is_some_and(|depth| indent <= depth) {
                nested_indent = None;
            }
            if nested_indent.is_some() {
                continue;
            }
            if words(line).any(|word| matches!(word, "def" | "class")) {
                nested_indent = Some(indent);
                continue;
            }
            if words(line).any(|word| word == "yield") {
                return true;
            }
        }
        return false;
    }
    let code = &input.code[body.clone()];
    if words(code).any(|word| matches!(word, "function" | "fun" | "class")) {
        return false;
    }
    words(code).any(|word| word == "yield")
}

fn add_go_injections(input: &Input<'_>, types: &mut [ManagedType]) -> Result<(), Failure> {
    for managed in types {
        let fields = crate::go_source::fields(&input.code, managed.body.clone());
        let mut injections = Vec::new();
        for dependency in &managed.provider.dependencies {
            let mut found = Vec::new();
            for field in &fields {
                if field.type_source.trim_start_matches('*') == dependency {
                    found.push(field);
                }
            }
            let [field] = found.as_slice() else {
                return Err(input.failure(
                    managed.declaration,
                    format!(
                        "{} needs exactly one field of type *{dependency} or {dependency} to receive its injected dependency",
                        managed.provider.name
                    ),
                ));
            };
            injections.push(Injection {
                field: field.name.clone(),
                pointer: field.type_source.starts_with('*'),
            });
        }
        managed.provider.injections = injections;
    }
    Ok(())
}

fn validate_constructors(input: &Input<'_>, types: &[ManagedType]) -> Result<(), Failure> {
    for managed in types {
        let count = helper_providers::HelperProviders::count(input, managed)?;
        if count != managed.provider.dependencies.len() {
            return Err(input.failure(
                managed.declaration,
                format!(
                    "{} constructor has {count} arguments but @Needs declares {} dependencies",
                    managed.provider.name,
                    managed.provider.dependencies.len()
                ),
            ));
        }
    }
    Ok(())
}

fn constructor_probe(input: &Input<'_>, managed: &ManagedType) -> Result<Option<usize>, Failure> {
    let primary = if input.extension == "kt" {
        kotlin_constructor(input, managed)?
    } else {
        None
    };
    let bodies = if input.extension == "rs" {
        rust_type_bodies(input, managed)?
    } else {
        vec![managed.body.clone()]
    };
    let target = match input.extension {
        "js" | "ts" | "kt" => "constructor",
        "py" => "__init__",
        "rb" => "initialize",
        "php" => "__construct",
        "rs" => "new",
        _ => &managed.provider.name,
    };
    let mut count = primary;
    for body in bodies {
        let code = &input.code[body.clone()];
        for (at, _) in code.match_indices(target) {
            let absolute = body.start + at;
            let Some(found) = constructor_parameters(input, target, body.start, absolute)? else {
                continue;
            };
            if count.replace(found).is_some() {
                return Err(input.failure(
                    absolute,
                    "Managed types require one unambiguous constructor; overloads are not supported",
                ));
            }
        }
    }
    Ok(count)
}

fn constructor_parameters(
    input: &Input<'_>,
    target: &str,
    body_start: usize,
    start: usize,
) -> Result<Option<usize>, Failure> {
    let before = &input.code[body_start..start];
    if before
        .chars()
        .next_back()
        .is_some_and(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        return Ok(None);
    }
    if !matches!(input.extension, "py" | "rb") && brace_depth(before) != 0 {
        return Ok(None);
    }
    if !constructor_declaration(input, target, body_start, start) {
        return Ok(None);
    }
    let after = &input.code[start + target.len()..];
    if !after.trim_start().starts_with('(') {
        return Ok(None);
    }
    let opening = start + target.len() + after.find('(').unwrap_or(0);
    let closing = matching(&input.code, opening, b'(', b')')
        .ok_or_else(|| input.failure(start, "Unbalanced constructor parameters"))?;
    let parameters = parameters(input, &input.code[opening + 1..closing], start)?;
    let count = if input.extension == "py" {
        instance_parameters(input, &parameters, start)?
    } else {
        parameters.len()
    };
    Ok(Some(count))
}

fn constructor_declaration(input: &Input<'_>, target: &str, body_start: usize, at: usize) -> bool {
    let prefix = &input.code[body_start..at];
    let boundary = prefix
        .rfind(['\n', ';', '}'])
        .map_or(0, |position| position + 1);
    let words = words(&prefix[boundary..]).collect::<Vec<_>>();
    if words.iter().any(|word| matches!(*word, "new" | "return")) {
        return false;
    }
    match input.extension {
        "py" | "rb" => words.last() == Some(&"def"),
        "rs" => words.last() == Some(&"fn") && target == "new",
        "php" => words.last() == Some(&"function"),
        _ => !prefix[boundary..].contains(['=', '(', '.']),
    }
}

fn rust_type_bodies(
    input: &Input<'_>,
    managed: &ManagedType,
) -> Result<Vec<Range<usize>>, Failure> {
    let mut offset = 0;
    let mut bodies = Vec::new();
    for line in input.code.split_inclusive('\n') {
        let tokens = words(line).collect::<Vec<_>>();
        if tokens.first() == Some(&"impl")
            && tokens.get(1) == Some(&managed.provider.name.as_str())
            && !tokens.contains(&"for")
        {
            bodies.push(type_body(input, offset)?);
        }
        offset += line.len();
    }
    Ok(bodies)
}

fn kotlin_constructor(input: &Input<'_>, managed: &ManagedType) -> Result<Option<usize>, Failure> {
    let end = if managed.body.is_empty() {
        managed.body.start
    } else {
        managed.body.start.saturating_sub(1)
    };
    let header = &input.code[managed.declaration..end];
    let Some(opening) = header.find('(') else {
        return Ok(None);
    };
    let closing = matching(header, opening, b'(', b')').ok_or_else(|| {
        input.failure(managed.declaration, "Unbalanced Kotlin primary constructor")
    })?;
    Ok(Some(
        parameters(input, &header[opening + 1..closing], managed.declaration)?.len(),
    ))
}

fn validate_graph(
    input: &Input<'_>,
    types: &[ManagedType],
    root: Option<usize>,
) -> Result<(), Failure> {
    for managed in types {
        for (position, dependency) in managed.provider.dependencies.iter().enumerate() {
            if !types
                .iter()
                .any(|candidate| candidate.provider.name == *dependency)
            {
                let injection = managed.provider.injections.get(position);
                let needed = injection.map_or_else(
                    || dependency.clone(),
                    |injection| format!("field '{}' of type {dependency}", injection.field),
                );
                if input.manual_helpers.contains(dependency) {
                    return Err(
                        input.failure(managed.declaration, input.missing_provider(dependency))
                    );
                }
                let at = injection.map_or(managed.declaration, |injection| {
                    managed.injected_field_at(input, &injection.field)
                });
                return Err(input.failure(at, format!("Unresolved dependency {dependency}: {} needs {needed}, but no provider {dependency} is declared; annotate and explicitly select its provider in this compilation unit", managed.provider.name)));
            }
        }
    }
    for (index, managed) in types.iter().enumerate() {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        visit(
            input,
            types,
            index,
            managed.provider.scope == Scope::Singleton,
            &mut visiting,
            &mut visited,
        )?;
    }
    if let Some(root) = root {
        instance_budget(input, types, root, &mut BTreeSet::new(), &mut 0)?;
    }
    Ok(())
}

fn instance_budget(
    input: &Input<'_>,
    types: &[ManagedType],
    index: usize,
    cached: &mut BTreeSet<usize>,
    count: &mut usize,
) -> Result<(), Failure> {
    if cached.contains(&index) {
        return Ok(());
    }
    *count += 1;
    let managed = &types[index];
    if *count > 4096 {
        return Err(input.failure(managed.declaration, "Yon dependency resolution would construct more than 4096 instances; reduce transient fan-out or share providers"));
    }
    if managed.provider.scope != Scope::Transient {
        cached.insert(index);
    }
    for dependency in &managed.provider.dependencies {
        if let Some(next) = types
            .iter()
            .position(|value| value.provider.name == *dependency)
        {
            instance_budget(input, types, next, cached, count)?;
        }
    }
    Ok(())
}

fn visit(
    input: &Input<'_>,
    types: &[ManagedType],
    index: usize,
    singleton: bool,
    visiting: &mut BTreeSet<usize>,
    visited: &mut BTreeSet<usize>,
) -> Result<(), Failure> {
    let managed = &types[index];
    if visiting.contains(&index) {
        return Err(input.failure(
            managed.declaration,
            format!("Dependency cycle reaches {}", managed.provider.name),
        ));
    }
    if singleton && managed.provider.scope == Scope::Request {
        return Err(input.failure(
            managed.declaration,
            format!(
                "Singleton dependency graph captures request-scoped {}",
                managed.provider.name
            ),
        ));
    }
    if visited.contains(&index) {
        return Ok(());
    }
    visiting.insert(index);
    for dependency in &managed.provider.dependencies {
        if let Some(next) = types
            .iter()
            .position(|candidate| candidate.provider.name == *dependency)
        {
            visit(input, types, next, singleton, visiting, visited)?;
        }
    }
    visiting.remove(&index);
    visited.insert(index);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn parsed(extension: &str, source: &str) -> Parsed {
        parse(
            Path::new(&format!("server/controllers/C.{extension}")),
            source,
        )
        .expect("valid source")
        .expect("annotated source")
    }

    fn rejects(extension: &str, source: &str, message: &str) {
        let failure = parse(
            Path::new(&format!("server/controllers/C.{extension}")),
            source,
        )
        .err()
        .expect("must reject");
        assert!(failure.to_string().contains(message), "{failure}");
        assert!(failure.to_string().contains("TY2015"));
    }

    fn first_route_streams(extension: &str, source: &str) -> bool {
        parsed(extension, source).plan.routes[0].streaming
    }

    fn controller(body: &str) -> String {
        format!("@RequestRoute(\"/c\")\nclass C {{\n{body}\n}}")
    }

    fn diamond_graph(scope: &str) -> String {
        let mut source = format!("@Service\n@Lifetime('{scope}')\nclass P0 {{}}\n");
        for index in 1..14 {
            writeln!(source, "@Service\n@Lifetime('{scope}')\n@Needs(P{}, P{})\nclass P{index} {{ constructor(a, b) {{}} }}", index - 1, index - 1).expect("fixture");
        }
        source.push_str("@RequestRoute(\"/c\")\n@Needs(P13)\nclass C {\n constructor(p) {}\n @GetRoute\n get() {}\n}\n");
        source
    }

    #[test]
    fn rust_explicit_helper_forms_do_not_emit_managed_factories() {
        let source = "#[Helper]\nstruct LiteralHelper {}\nimpl LiteralHelper { fn check(&self) {} }\n#[Helper]\nstruct StaticHelper {}\nimpl StaticHelper { fn answer() -> usize { 1 } }\n#[derive(Default)]\n#[Helper]\nstruct DefaultHelper {}\n#[RequestRoute(\"/c\")]\nstruct C;\nimpl C { fn new() -> Self { Self }\n #[GetRoute]\n fn get(&self) { LiteralHelper {}.check(); let _ = StaticHelper::answer(); let _ = DefaultHelper::default(); } }";
        assert_eq!(
            parsed("rs", source)
                .plan
                .providers
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["C"]
        );
    }

    #[test]
    fn rust_explicit_helper_forms_still_reject_incoming_di_and_lifecycle() {
        let source = "#[Helper]\nstruct StaticHelper {}\nimpl StaticHelper { fn answer() -> usize { 1 } }\n#[RequestRoute(\"/c\")]\n#[Needs(StaticHelper)]\nstruct C { helper: StaticHelper }\nimpl C { fn new(helper: StaticHelper) -> Self { Self { helper } }\n #[GetRoute]\n fn get(&self) { let _ = StaticHelper::answer(); } }";
        rejects("rs", source, "uses explicit caller construction");
        let hooked = "#[Helper]\nstruct StaticHelper {}\nimpl StaticHelper {\n #[OnStart]\n fn start(&self) {}\n fn answer() -> usize { 1 } }\n#[RequestRoute(\"/c\")]\nstruct C;\nimpl C { fn new() -> Self { Self }\n #[GetRoute]\n fn get(&self) { let _ = StaticHelper::answer(); } }";
        rejects("rs", hooked, "lifecycle and event hooks require");
    }

    #[test]
    fn caller_constructed_helpers_are_not_provider_factories() {
        let source = "@Helper\nclass ReaderHelper { constructor(context) {} }\n@Service\nclass ReaderService { helper = new ReaderHelper(1); }\n@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n get() { return 1; } }";
        let plan = parsed("js", source).plan;
        assert_eq!(
            plan.providers
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["ReaderService", "C"]
        );
    }

    #[test]
    fn caller_constructed_helpers_require_managed_opt_in_for_incoming_di() {
        let source = "@Helper\nclass ReaderHelper { constructor(context) {} }\n@Service\n@Needs(ReaderHelper)\nclass ReaderService { constructor(helper) {} make() { return new ReaderHelper(1); } }\n@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n get() { return 1; } }";
        rejects("js", source, "uses explicit caller construction");
    }

    #[test]
    fn caller_construction_does_not_disable_managed_helper_validation() {
        let source = "@Helper\n@Needs()\nclass ReaderHelper { constructor(context) {} }\n@Service\nclass ReaderService { helper = new ReaderHelper(1); }\n@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n get() { return 1; } }";
        rejects("js", source, "constructor has 1 arguments");
    }

    #[test]
    fn caller_construction_does_not_disable_service_or_converter_validation() {
        for marker in ["Service", "Converter"] {
            let source = format!(
                "@{marker}\nclass Reader {{ constructor(context) {{}} }}\n@Service\nclass Caller {{ reader = new Reader(1); }}\n@RequestRoute(\"/c\")\nclass C {{\n @GetRoute\n get() {{ return 1; }} }}"
            );
            rejects("js", &source, "constructor has 1 arguments");
        }
    }

    #[test]
    fn zero_argument_helpers_retain_existing_provider_identity() {
        let source = "@Helper\nclass ReaderHelper {}\n@Service\nclass ReaderService { helper = new ReaderHelper(); }\n@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n get() { return 1; } }";
        assert!(
            parsed("js", source)
                .plan
                .providers
                .iter()
                .any(|item| item.name == "ReaderHelper")
        );
    }

    #[test]
    fn caller_constructed_helpers_cannot_be_selected_as_gateway_targets() {
        let source = "@Helper\nclass ReaderHelper { constructor(context) {} }\n@Service\nclass ReaderService { helper = new ReaderHelper(1); }";
        let path = Path::new("server/helpers/ReaderHelper.js");
        let manual = super::super::manual_helpers::ManualHelpers::read(path, source)
            .expect("helper ownership");
        let failure = parse_prepared(path, source, Some("ReaderHelper"), &manual)
            .err()
            .expect("explicit helper is not gateway provider");
        assert!(
            failure
                .to_string()
                .contains("uses explicit caller construction"),
            "{failure}"
        );
    }

    #[test]
    fn routes_come_from_their_annotations() {
        let result = parsed(
            "js",
            &controller(
                " @GetRoute\n get() { return 1; }\n @PostRoute\n post(request) { return request; }\n helper() {}",
            ),
        );
        let methods: Vec<&str> = result
            .plan
            .routes
            .iter()
            .map(|route| route.method.as_str())
            .collect();
        assert_eq!(methods, ["get", "post"]);
        assert!(!result.plan.routes[0].takes_request);
        assert!(result.plan.routes[1].takes_request);
    }

    #[test]
    fn dart_arrow_bodies_are_concrete_bodies() {
        let source = "@RequestRoute(\"/c\")\nclass C {\n  @GetRoute\n  Map<String, int> getItem() => {'id': 1};\n\n  @PostRoute\n  Future<Response> postItem(Request request) =>\n      relay(['ruby', 'r.rb'], request);\n\n  @PutRoute\n  String putItem(@FromBody String item) => item;\n}\n";
        let methods: Vec<String> = parsed("dart", source)
            .plan
            .routes
            .iter()
            .map(|route| route.method.clone())
            .collect();
        assert_eq!(methods, ["getItem", "postItem", "putItem"]);
    }

    #[test]
    fn every_route_annotation_sets_its_http_method() {
        let mut body = String::new();
        for (marker, name) in [
            ("GetRoute", "getItems"),
            ("PostRoute", "postItem"),
            ("PutRoute", "putItem"),
            ("PatchRoute", "patchItem"),
            ("DeleteRoute", "deleteItem"),
        ] {
            let path = name.to_ascii_lowercase();
            writeln!(body, " @{marker}(\"/{path}\")\n {name}() {{ return 1; }}").expect("fixture");
        }
        let plan = parsed("js", &controller(&body)).plan;
        let verbs: Vec<&str> = plan
            .routes
            .iter()
            .map(|route| route.verb.as_str())
            .collect();
        assert_eq!(verbs, ["GET", "POST", "PUT", "PATCH", "DELETE"]);
        rejects(
            "js",
            &controller(" helper() {}"),
            "needs at least one route method",
        );
        rejects(
            "js",
            &controller(" GET() { return 1; }"),
            "no longer answers requests by its name",
        );
    }

    #[test]
    fn redundant_http_annotations_fail_with_rename_guidance() {
        rejects(
            "js",
            &controller(" @GET() get() {}"),
            "is not a Tachyon annotation. Use @GetRoute",
        );
        for annotation in ["@GET", "@GET()", "@get", "@POST", "@TRACE"] {
            rejects(
                "js",
                &controller(&format!(" {annotation}\n get() {{}}")),
                "is not a Tachyon annotation. Use @GetRoute",
            );
        }
        for (extension, source) in [
            (
                "py",
                "@request_route(\"/c\")\nclass C:\n @get\n def get(self):\n  return 1\n",
            ),
            (
                "php",
                "<?php\n#[RequestRoute(\"/c\")]\nclass C {\n #[GET]\n function get() {}\n}",
            ),
            (
                "cs",
                "[RequestRoute(\"/c\")]\nclass C {\n [POST]\n public object Get() { return null; }\n}",
            ),
            (
                "rs",
                "#[RequestRoute(\"/c\")]\nstruct C;\nimpl C { fn new() -> Self { Self }\n #[yon::get]\n fn get(&self) {}\n}",
            ),
        ] {
            rejects(
                extension,
                source,
                "is not a Tachyon annotation. Use @GetRoute",
            );
        }
    }

    #[test]
    fn helpers_nested_functions_and_text_are_not_routes() {
        let source = "@Service\nclass S { static *get() { yield 1; } }\n@RequestRoute(\"/c\")\nclass C {\n value = Factory.GET();\n helper() { function POST() {} return '@GetRoute'; }\n @GetRoute\n get() { const object = { POST() {} }; return 1; }\n}";
        let plan = parsed("js", source).plan;
        assert_eq!(plan.routes.len(), 1);
        assert!(!plan.routes[0].streaming);
        rejects(
            "js",
            "@Service\nclass S {\n @GetRoute\n get() {}\n}\n@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n get() {}\n}",
            "belongs on a method of the controller class, not on S",
        );
    }

    #[test]
    fn a_signature_may_span_lines() {
        let plan = parsed(
            "ts",
            &controller(" @GetRoute\n get(\n request: { method: string }\n ) { return request; }\n @PostRoute\n post() { return 1; }"),
        )
        .plan;
        assert_eq!(plan.routes.len(), 2);
        assert!(plan.routes[0].takes_request);
    }

    #[test]
    fn a_route_is_one_accessible_instance_method() {
        for body in [
            " @GetRoute\n static get() {}",
            " @GetRoute\n private get() {}",
        ] {
            rejects("js", &controller(body), "instance methods");
        }
        rejects(
            "py",
            "@request_route(\"/c\")\nclass C:\n @staticmethod\n @get_route\n def get():\n  return 1\n",
            "instance methods",
        );
        rejects(
            "js",
            &controller(" @GetRoute\n get() {}\n @GetRoute\n getAgain() {}"),
            "Two methods answer GET /c",
        );
        rejects(
            "js",
            &controller(" @GetRoute\n get(a, b) {}"),
            "Only one parameter may be left unmarked",
        );
        rejects(
            "rs",
            "#[RequestRoute(\"/c\")]\nstruct C;\nimpl C { fn new() -> Self { Self }\n #[GetRoute]\n fn get(&mut self) {}\n}",
            "instance receiver",
        );
    }

    #[test]
    fn stream_route_marks_a_get_route_that_streams() {
        assert!(first_route_streams(
            "js",
            &controller(" @StreamRoute\n async *streamTicks() { yield 1; }")
        ));
        assert!(first_route_streams(
            "js",
            &controller(" @StreamRoute(\"/ticks\")\n streamTicks() { return events(); }")
        ));
        assert!(first_route_streams(
            "kt",
            "@RequestRoute(\"/c\")\nclass C {\n @StreamRoute\n fun streamTicks() = sequence { yield(1) }\n}"
        ));
        assert!(first_route_streams(
            "py",
            "@request_route(\"/c\")\nclass C:\n @stream_route\n def stream_ticks(self):\n  yield 1\n"
        ));
        assert!(first_route_streams(
            "rs",
            "#[RequestRoute(\"/c\")]\nstruct C;\nimpl C { fn new() -> Self { Self }\n #[StreamRoute]\n fn stream_ticks(&self) -> impl Iterator<Item = u32> { 1..3 }\n}"
        ));
        rejects(
            "js",
            &controller(" @GetRoute\n async *getTicks() { yield 1; }"),
            "@StreamRoute replaces @GetRoute and @Stream",
        );
        rejects(
            "js",
            &controller(" @Stream\n @GetRoute\n getTicks() { return events(); }"),
            "@StreamRoute replaces @GetRoute and @Stream",
        );
        rejects(
            "py",
            "@request_route(\"/c\")\nclass C:\n @post_route\n def post_ticks(self):\n  yield 1\n",
            "start the method name with 'stream'",
        );
        rejects(
            "js",
            &controller(" @StreamRoute\n async *getTicks() { yield 1; }"),
            "its name must start with 'stream'",
        );
        rejects(
            "java",
            "@RequestRoute(\"/c\")\nclass C {\n @StreamRoute\n public Object streamTicks() { return null; }\n}",
            "@StreamRoute needs JavaScript",
        );
        rejects(
            "py",
            "@request_route(\"/c\")\nclass C:\n @stream_route\n async def stream_ticks(self):\n  yield 1\n",
            "async generators",
        );
    }

    fn socket(body: &str) -> String {
        format!("@SocketRoute(\"/chat\")\nclass ChatController {{\n{body}\n}}")
    }

    #[test]
    fn socket_route_maps_its_event_methods() {
        let chat = parsed(
            "ts",
            &socket(
                " @OnOpen\n onOpen(session) {}\n @OnMessage\n onMessage(session, message: ChatMessage) {}\n @OnClose\n onClose(session) {}",
            ),
        );
        let methods = chat.plan.routes[0].socket.clone().expect("socket methods");
        assert_eq!(methods.message, "onMessage");
        let open = methods.open.expect("open method");
        assert_eq!((open.name.as_str(), open.parameters), ("onOpen", 1));
        let close = methods.close.expect("close method");
        assert_eq!((close.name.as_str(), close.parameters), ("onClose", 1));
        assert_eq!(methods.error, None);
        let route = &chat.plan.routes[0];
        assert_eq!(route.verb, "GET");
        assert_eq!(route.pattern.as_deref(), Some("/chat"));
        assert_eq!(route.parameters[0].type_text, "ChatMessage");
        assert!(!chat.source.contains("@OnMessage"));
        let documented = parsed(
            "js",
            &socket(
                " /** @param {ChatMessage} message */\n @OnMessage\n receive(session, message) {}",
            ),
        );
        assert_eq!(documented.plan.routes[0].method, "receive");
        let python = parsed(
            "py",
            "@socket_route(\"/chat\")\nclass ChatController:\n    @on_open\n    def on_open(self):\n        pass\n\n    @on_message\n    def on_message(self, session, message: ChatMessage):\n        pass\n",
        );
        let methods = python.plan.routes[0]
            .socket
            .clone()
            .expect("socket methods");
        assert_eq!(methods.message, "on_message");
        let open = methods.open.expect("open method");
        assert_eq!((open.name.as_str(), open.parameters), ("on_open", 0));
        assert_eq!(python.plan.routes[0].parameters[0].type_text, "ChatMessage");
        let rust = parsed(
            "rs",
            "#[SocketRoute(\"/chat\")]\npub struct ChatController {}\n\nimpl ChatController {\n    fn new() -> Self {\n        Self {}\n    }\n\n    #[OnMessage]\n    pub fn on_message(&self, session: &SocketSession, message: ChatMessage) {}\n}\n",
        );
        assert_eq!(rust.plan.routes[0].parameters[0].type_text, "ChatMessage");
    }

    #[test]
    fn socket_route_refuses_what_it_cannot_run() {
        let message = " @OnMessage\n onMessage(session, message: ChatMessage) {}";
        rejects(
            "ts",
            &socket(" @OnOpen\n onOpen(session) {}"),
            "needs one @OnMessage method",
        );
        rejects(
            "ts",
            &socket(&format!(
                " @OnOpen\n onOpen() {{}}\n @OnOpen\n opened() {{}}\n{message}"
            )),
            "at most one @OnOpen",
        );
        rejects(
            "ts",
            &socket(" @OnMessage\n onMessage(session) {}"),
            "takes (session, message)",
        );
        rejects(
            "ts",
            &socket(" @OnMessage\n onMessage(session, message) {}"),
            "needs a model type",
        );
        rejects(
            "ts",
            &socket(&format!(" @GetRoute\n getChat() {{}}\n{message}")),
            "answers socket events only",
        );
        rejects(
            "ts",
            &format!("@SocketRoute(\"/chat/{{room}}\")\nclass ChatController {{\n{message}\n}}"),
            "no {variables}",
        );
    }

    #[test]
    fn nested_generator_yields_do_not_stream() {
        assert!(!first_route_streams(
            "js",
            &controller(" @GetRoute\n get() { function* local() { yield 1; } return 1; }")
        ));
        assert!(!first_route_streams(
            "py",
            "@request_route(\"/c\")\nclass C:\n @get_route\n def get(self):\n  def local():\n   yield 1\n  return 1\n"
        ));
    }

    #[test]
    fn preserves_plain_handlers_array_accesses_comments_and_regular_expressions() {
        for (extension, source) in [
            (
                "js",
                "export class Handler { static GET() { return values[GET]; } }",
            ),
            (
                "js",
                "const pattern = /@RequestRoute()/; class Handler { static GET() { return 1; } }",
            ),
            ("js", "// @RequestRoute()\nconst text = '@GetRoute';"),
            ("js", "const text = '\n@RequestRoute()\nclass Fake {}\n';"),
            (
                "py",
                "class Handler:\n @staticmethod\n def GET(request):\n  return [service]\n",
            ),
            (
                "cs",
                "class Handler { public static object GET() { return values[GET]; } }",
            ),
            (
                "php",
                "<?php class Handler { static function GET() { return $values[GET]; } }",
            ),
        ] {
            assert!(
                parse(Path::new(&format!("server/workers/w.{extension}")), source)
                    .expect("ordinary source")
                    .is_none()
            );
        }
    }

    #[test]
    fn extracts_constructor_plan_and_preserves_line_positions() {
        let source = "@Store()\nclass Products { list() { return []; } }\n@Service()\n@Needs(Products)\nclass Catalog { constructor(private products: Products) {} }\n@RequestRoute(\"/products\")\n@Needs(Catalog)\nexport class ProductsController {\n constructor(private catalog: Catalog) {}\n @GetRoute\n async getProducts(request: Request) { return this.catalog; }\n @PostRoute\n postProduct(@FromBody product: object) { return product; }\n}\n";
        let result = parsed("ts", source);
        assert_eq!(result.plan.providers[1].dependencies, ["Products"]);
        assert_eq!(result.plan.routes.len(), 2);
        assert_eq!(result.source.lines().count(), source.lines().count());
        for marker in ["@RequestRoute", "@GetRoute", "@PostRoute", "@FromBody"] {
            assert!(!result.source.contains(marker), "{}", result.source);
        }
        assert!(result.source.contains("async getProducts"));
    }

    #[test]
    fn a_converter_is_a_provider_that_a_controller_needs() {
        let source = "@Converter\nclass ProductConverter {}\n@RequestRoute(\"/catalog\")\n@Needs(ProductConverter)\nexport class CatalogController {\n constructor(private converter: ProductConverter) {}\n @GetRoute\n async getCatalog() { return {}; }\n}\n";
        let result = parsed("ts", source);
        let converter = &result.plan.providers[0];
        assert_eq!(converter.name, "ProductConverter");
        assert_eq!(result.plan.providers[1].dependencies, ["ProductConverter"]);
    }

    #[test]
    fn a_test_unit_module_drops_the_route_parameter_markers_of_its_controllers() {
        let source = "@Service\nclass NewsCaller {\n call() { return 1; }\n}\n@RequestRoute(\"/news\")\nclass NewsController {\n @PostRoute\n postNews(@FromBody item, @FromHeader('x-id') id) { return item; }\n}\n";
        let result = parse_test(Path::new("server/tests/NewsCaller.js"), source)
            .expect("test module")
            .expect("managed types");
        assert!(!result.source.contains('@'), "{}", result.source);
        assert_eq!(result.source.len(), source.len());
    }

    #[test]
    fn rust_snake_case_test_targets_choose_the_actual_type_from_composed_sources() {
        let source = "#[Service]\nstruct PeerService;\nimpl PeerService { fn new() -> Self { Self } }\n#[Service]\nstruct NewsCaller;\nimpl NewsCaller { fn new() -> Self { Self } fn call(&self) -> i64 { 1 } }\n";
        let parsed = parse_test(Path::new("server/tests/nested/news_caller.rs"), source)
            .expect("typed test target")
            .expect("managed types");
        assert_eq!(parsed.plan.controller, "NewsCaller");
    }

    #[test]
    fn native_injection_literals_remain_supported() {
        for (extension, source) in [
            (
                "java",
                "@Service\nclass S {}\n@RequestRoute(\"/c\")\n@Needs({S.class})\nclass C {\n C(S s) {}\n @GetRoute\n Object get() { return null; }\n}",
            ),
            (
                "cs",
                "[Service]\nclass S {}\n[RequestRoute(\"/c\")]\n[Needs(typeof(S))]\nclass C {\n public C(S s) {}\n [GetRoute]\n public object Get() => null;\n}",
            ),
            (
                "php",
                "<?php\n#[Service]\nclass S {}\n#[RequestRoute(\"/c\")]\n#[Needs(S::class)]\nclass C {\n public function __construct(S $s) {}\n #[GetRoute]\n public function get() { return null; }\n}",
            ),
            (
                "kt",
                "@Service\nclass S\n@RequestRoute(\"/c\")\n@Needs(S::class)\nclass C(val s: S) {\n @GetRoute\n fun get(): Int = 1\n}",
            ),
        ] {
            assert_eq!(
                parsed(extension, source).plan.providers[1].dependencies,
                ["S"],
                "{extension}"
            );
        }
    }

    #[test]
    fn qualified_return_types_keep_their_streaming() {
        for source in [
            "[RequestRoute(\"/c\")]\nclass C {\n [StreamRoute]\n public System.Collections.Generic.IEnumerable<object> StreamItems() { yield return 1; }\n}",
            "[RequestRoute(\"/c\")]\nclass C {\n [StreamRoute]\n public async System.Collections.Generic.IAsyncEnumerable<object> StreamItems() { yield return 1; }\n}",
        ] {
            let plan = parsed("cs", source).plan;
            assert_eq!(plan.routes[0].verb, "GET");
            assert!(plan.routes[0].streaming);
        }
        let plan = parsed(
            "java",
            "@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n public java.util.Map<String, Object> getItems() { return null; }\n}",
        )
        .plan;
        assert_eq!(plan.routes[0].verb, "GET");
        assert!(!plan.routes[0].streaming);
        assert_eq!(plan.routes[0].returns, "java.util.Map<String, Object>");
    }

    #[test]
    fn go_injections_name_the_one_field_of_each_dependency() {
        let source = "// @Service\ntype Store struct {\n\tcount int\n}\n// @RequestRoute(\"/orders\")\n// @Needs(Store)\ntype Orders struct {\n\tstore *Store\n\tlimit int\n}\n// @GetRoute\nfunc (orders Orders) GetOrders(request *http.Request) any { return nil }\n";
        let plan = parsed("go", source).plan;
        assert_eq!(
            plan.providers[1].injections,
            [Injection {
                field: "store".into(),
                pointer: true
            }]
        );
        assert!(plan.routes[0].takes_request);
        assert!(!plan.routes[0].streaming);
        rejects(
            "go",
            &source.replace("\tlimit int", "\tbackup Store"),
            "exactly one field of type *Store or Store",
        );
        rejects(
            "go",
            "package orders\n// @RequestRoute(\"/orders\")\ntype Orders struct{}\n// @GetRoute\nfunc (Orders) GetOrders() any { return nil }\n",
            "default package or namespace",
        );
    }

    #[test]
    fn graph_errors_still_fail_before_execution() {
        rejects(
            "js",
            "@RequestRoute(\"/c\")\n@Needs(Missing)\nclass C {\n constructor(x) {}\n @GetRoute\n get() {}\n}",
            "Unresolved dependency Missing",
        );
        rejects(
            "js",
            &controller(" constructor(x) {}\n @GetRoute\n get() {}"),
            "constructor has 1 arguments",
        );
        rejects(
            "js",
            &format!(
                "@Service\n@Needs(A)\nclass A {{ constructor(a) {{}} }}\n{}",
                controller(" @GetRoute\n get() {}")
            ),
            "Dependency cycle",
        );
        rejects(
            "js",
            &format!(
                "@Service\n@Lifetime('request')\nclass A {{}}\n@Service\n@Needs(A)\nclass B {{ constructor(a) {{}} }}\n{}",
                controller(" @GetRoute\n get() {}")
            ),
            "captures request-scoped",
        );
    }

    #[test]
    fn bounds_transient_expansion_but_shares_cached_diamonds() {
        rejects("js", &diamond_graph("transient"), "more than 4096");
        assert_eq!(
            parsed("js", &diamond_graph("singleton"))
                .plan
                .providers
                .len(),
            15
        );
    }

    #[test]
    fn rejects_nested_types_namespaces_reserved_names_and_unknown_hooks() {
        let route = " @GetRoute\n get() {}";
        rejects(
            "js",
            &format!("function wrap() {{\n {}\n}}", controller(route)),
            "module top level",
        );
        rejects(
            "py",
            "def wrap():\n @request_route(\"/c\")\n class C:\n  @get_route\n  def get(self):\n   return 1\n",
            "module top level",
        );
        rejects(
            "cs",
            "namespace App;\n[RequestRoute(\"/c\")]\nclass C {\n [GetRoute]\n public object Get() { return null; }\n}",
            "default package or namespace",
        );
        rejects(
            "js",
            &format!("{}\nclass Handler {{}}", controller(route)),
            "reserved Handler",
        );
        for name in ["DataSource", "RedisTemplate", "ApplicationEventPublisher"] {
            rejects(
                "js",
                &format!("{}\nclass {name} {{}}", controller(route)),
                "framework-owned DataSource, RedisTemplate or ApplicationEventPublisher",
            );
        }
        rejects(
            "js",
            &format!("{}\nclass __YonContainer {{}}", controller(route)),
            "reserved __Yon",
        );
        rejects(
            "java",
            "@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n public Object get() { return null; }\n}\nrecord ApplicationEventPublisher() {}",
            "framework-owned DataSource, RedisTemplate or ApplicationEventPublisher",
        );
        rejects(
            "js",
            &controller(" @UseGuards(Auth)\n @GetRoute\n get() {}"),
            "does not implement",
        );
    }

    #[test]
    fn a_parameter_named_like_another_language_type_keyword_is_not_a_declaration() {
        for (extension, source) in [
            (
                "rb",
                "#[RequestRoute(\"/c\")]\nclass C\n  #[PostRoute]\n  def post(record)\n    ApplicationEventPublisher.new.publish_event(record)\n  end\nend\n",
            ),
            (
                "py",
                "@request_route(\"/c\")\nclass C:\n    @post_route\n    def post(self, object):\n        return object or ApplicationEventPublisher\n",
            ),
            (
                "js",
                "@RequestRoute(\"/c\")\nclass C {\n @PostRoute\n post(record) { return record || ApplicationEventPublisher; }\n}",
            ),
        ] {
            assert_eq!(parsed(extension, source).plan.routes.len(), 1);
        }
    }

    #[test]
    fn rejects_duplicate_or_malformed_provider_annotations() {
        for value in ["x", "é"] {
            rejects(
                "js",
                &format!("@RequestRoute({value})\nclass C {{\n @GetRoute\n get() {{}}\n}}"),
                "path in quotes",
            );
            rejects(
                "js",
                &format!(
                    "@RequestRoute\nclass C {{\n @GetRoute\n get(@FromQuery({value}) value) {{}}\n}}"
                ),
                "name in quotes",
            );
        }
        rejects(
            "js",
            "@RequestRoute(\"/a\") class A {}",
            "own complete line",
        );
        rejects(
            "js",
            "@RequestRoute(\"/c\")\n@RequestRoute(\"/c\")\nclass C {\n @GetRoute\n get() {}\n}",
            "exactly one Yon role annotation",
        );
        rejects("js", "@Service\nclass A {}", "exactly one controller class");
        rejects(
            "js",
            "@RequestRoute(\"/a\")\nclass A {}\n@RequestRoute(\"/b\")\nclass B {}",
            "exactly one controller class",
        );
        rejects(
            "js",
            "@RequestRoute(\"/c\")\n@Lifetime('global')\nclass C {}",
            "literal string",
        );
        rejects(
            "js",
            "@RequestRoute(\"/c\")\n@Needs('S')\nclass C {}",
            "provider identifiers",
        );
        rejects(
            "js",
            "@RequestRoute(\"/c\")\nclass $C {\n @GetRoute\n get() {}\n}",
            "ASCII identifiers",
        );
        rejects(
            "js",
            "@RequestRoute(/c)\nclass C {\n @GetRoute\n get() {}\n}",
            "takes one path in quotes",
        );
    }
}
