//! Generates a companion's member table from the class it declares.
//!
//! A JavaScript companion is a class, and every other language's was a
//! hand-written `tac` dictionary mapping names to closures. That asymmetry was
//! not a language limitation but a missing build step: a compiled companion
//! has no reflection the host can enumerate a class with, so *something* has
//! to write the table down. This writes it.
//!
//! Tachyon does it rather than each language's macro system — Swift macros,
//! Kotlin KSP, Roslyn source generators — because every
//! one of those drags a project file and a build tool into a framework whose
//! stance is that neither should be needed to compile one source file.
//!
//! An author who writes the table by hand keeps it: an explicit `tac`
//! declaration is left exactly as it was. The generator only fills a gap.

use crate::Failure;
use crate::failure::{diagnostic, source_span};

/// Generates the member table a native host's prelude reads.
///
/// One generator for every language: the table is the same shape whichever
/// compiler will read it, and writing it four times is how three of them
/// silently drift.
pub(crate) fn swift_member_table(source: &str, source_path: &str) -> Result<String, Failure> {
    table(CompanionLanguage::Swift, source, source_path)
}

pub(crate) fn kotlin_member_table(source: &str, source_path: &str) -> Result<String, Failure> {
    table(CompanionLanguage::Kotlin, source, source_path)
}

pub(crate) fn csharp_member_table(source: &str, source_path: &str) -> Result<String, Failure> {
    table(CompanionLanguage::CSharp, source, source_path)
}

pub(crate) fn rust_member_table(source: &str, source_path: &str) -> Result<String, Failure> {
    table(CompanionLanguage::Rust, source, source_path)
}

fn table(language: CompanionLanguage, source: &str, source_path: &str) -> Result<String, Failure> {
    member_table(language, source, source_path).map(Option::unwrap_or_default)
}

/// One member found on an authored class.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Member {
    name: String,
    /// The declared type, when the declaration carries one.
    ///
    /// A read-only field need not: `let runtime = "x"` is how Swift and Kotlin
    /// write one, and inferring the type would mean implementing the language.
    /// Only a *writable* field must declare it, because the setter has to
    /// convert an incoming JSON value into something.
    declared_type: Option<String>,
    /// Whether this is a method rather than a field.
    ///
    /// This used to be read off `declared_type.is_none()`, which made an
    /// untyped `let` indistinguishable from a `func` — the table then called
    /// a string, and the companion did not compile.
    method: bool,
    /// Whether a field may be assigned as well as read.
    mutable: bool,
}

/// Languages whose companion can be written as a class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompanionLanguage {
    Swift,
    Kotlin,
    CSharp,
    /// Rust declares its state as a `struct` and its behaviour in an `impl`,
    /// so the two are read together rather than from one brace-delimited body.
    Rust,
}

/// Every one of these languages spells the declaration `class`.
const CLASS_KEYWORD: &str = "class";

/// Returns the member table to append, or `None` when one is already declared.
///
/// # Errors
///
/// Returns a diagnostic when a class declares a mutable field without a type,
/// which is the one thing that cannot be generated: the setter has to convert
/// an incoming JSON value into something, and only the declaration says what.
fn member_table(
    language: CompanionLanguage,
    source: &str,
    source_path: &str,
) -> Result<Option<String>, Failure> {
    let source = crate::without_bom(source);
    if declares_table(language, source) {
        return Ok(None);
    }
    if language == CompanionLanguage::Rust {
        return rust_table(source, source_path);
    }
    let Some((class_name, body)) = class_body(source) else {
        return Ok(None);
    };
    let members = members(language, body, source_path)?;
    if members.is_empty() {
        return Ok(None);
    }
    Ok(Some(render(language, &class_name, &members)))
}

/// Whether the author wrote the table themselves.
fn declares_table(language: CompanionLanguage, source: &str) -> bool {
    let needles: &[&str] = match language {
        CompanionLanguage::Swift => &["let tac", "var tac"],
        CompanionLanguage::Kotlin => &["val tac", "var tac"],
        CompanionLanguage::CSharp => &["class TacBridge"],
        CompanionLanguage::Rust => &["fn tac()"],
    };
    source
        .lines()
        .map(str::trim_start)
        .any(|line| needles.iter().any(|needle| line.starts_with(needle)))
}

/// Modifiers a declaration may carry before its keyword, across the languages.
const CLASS_MODIFIERS: [&str; 11] = [
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

/// Extracts the name and brace-balanced body of the first declared class.
///
/// The keyword has to open a line, after any modifiers. Scanning for it
/// anywhere would find the word in a comment or a string — as it did, turning
/// "the same class structure" in a doc comment into a class named
/// `structure`.
fn class_body(source: &str) -> Option<(String, &str)> {
    let keyword = CLASS_KEYWORD;
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let mut rest = trimmed;
        loop {
            let stripped = CLASS_MODIFIERS.iter().find_map(|modifier| {
                rest.strip_prefix(modifier)
                    .filter(|value| value.starts_with(char::is_whitespace))
                    .map(str::trim_start)
            });
            match stripped {
                Some(value) => rest = value,
                None => break,
            }
        }
        let Some(after) = rest
            .strip_prefix(keyword)
            .filter(|value| value.starts_with(char::is_whitespace))
        else {
            offset += line.len();
            continue;
        };
        let name = identifier(after.trim_start());
        if name.is_empty() {
            offset += line.len();
            continue;
        }
        // The body starts at the first brace at or after this declaration.
        let from = source[offset..].find('{')? + offset;
        let mut depth = 0usize;
        for (position, character) in source[from..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((name, &source[from + 1..from + position]));
                    }
                }
                _ => {}
            }
        }
        return None;
    }
    None
}

/// Reads the fields and methods declared directly on a class body.
fn members(
    language: CompanionLanguage,
    body: &str,
    source_path: &str,
) -> Result<Vec<Member>, Failure> {
    let mut members = Vec::new();
    let mut depth = 0i32;
    for line in body.lines() {
        let trimmed = line.trim();
        // Only the class's own declarations: anything inside a method body is
        // that method's business.
        let entering = depth;
        depth += i32::try_from(trimmed.matches('{').count()).unwrap_or(0)
            - i32::try_from(trimmed.matches('}').count()).unwrap_or(0);
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

/// Builds the table for a Rust companion.
///
/// Rust keeps state in a `struct` and behaviour in an `impl`, so both are read
/// and joined. A Rust struct has no inline field defaults, so the value a field
/// starts at lives in a constructor: `fn new() -> Self` in the inherent impl,
/// which keeps a companion to one struct and one impl — as close to the class
/// every other language writes as Rust gets. `Default` is accepted too, for a
/// companion that has one already.
fn rust_table(source: &str, source_path: &str) -> Result<Option<String>, Failure> {
    let Some((name, body)) = block(source, "struct") else {
        return Ok(None);
    };
    let mut members: Vec<Member> = body
        .lines()
        .filter_map(|line| rust_field(line.trim()))
        .collect();
    // Every inherent impl, and only those: `impl Default for Companion` is
    // where the starting values live, not where the methods do, and taking the
    // first `impl` found silently picked it.
    let mut constructs = false;
    for (header, body) in blocks(source, "impl") {
        if header.trim() == name {
            members.extend(rust_methods(body));
            constructs |= body.contains("fn new(") && body.contains("-> Self");
        }
    }
    if members.is_empty() {
        return Ok(None);
    }
    // `new()` first: a companion that has both means the one it wrote by hand.
    let instance = if constructs {
        format!("{name}::new()")
    } else if source.contains("Default") {
        format!("<{name} as Default>::default()")
    } else {
        return Err(Failure::one(diagnostic(
            1406,
            format!("Companion struct '{name}' needs a constructor."),
            Some(String::from(
                "A Rust struct has no inline field defaults, so the value a field \
                 starts at lives in a constructor. Add `pub fn new() -> Self` to the \
                 impl, which is the shape every other companion language writes; \
                 #[derive(Default)] or an impl Default is accepted too.",
            )),
            source_span(source_path, 0, source_path.len()),
        )));
    };
    members.sort_by(|left, right| left.name.cmp(&right.name));
    members.dedup_by(|left, right| left.name == right.name);
    Ok(Some(render_rust(&name, &instance, &members)))
}

/// Reads one `name: Type,` struct field.
fn rust_field(line: &str) -> Option<Member> {
    let line = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ");
    if line.starts_with("//") || line.starts_with('#') {
        return None;
    }
    let name = identifier(line);
    if name.is_empty() {
        return None;
    }
    let rest = line[name.len()..].trim_start();
    let declared = rest.strip_prefix(':')?.trim().trim_end_matches(',').trim();
    // `&'static str` is how Rust writes a constant string field, so it is read
    // as one. Any other borrowed or generic type is not something this
    // generator converts to.
    let declared = if matches!(declared, "&'static str" | "&str") {
        "&str"
    } else if declared.is_empty() || declared.contains(['&', '<', '\'']) {
        return None;
    } else {
        declared
    };
    Some(Member {
        name,
        declared_type: Some(String::from(declared)),
        method: false,
        mutable: true,
    })
}

/// Reads the `fn name(&self …)` methods of an impl block.
fn rust_methods(body: &str) -> Vec<Member> {
    body.lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches("pub ");
            let rest = line.strip_prefix("fn").and_then(after_keyword)?;
            let name = identifier(rest);
            let arguments = rest[name.len()..].trim_start();
            // Only a method reachable through the instance: an associated
            // function has no receiver to call it on.
            if name.is_empty() || !arguments.starts_with("(&self") {
                return None;
            }
            let returns = arguments
                .split("->")
                .nth(1)
                .map(|value| value.trim().trim_end_matches('{').trim().to_owned())
                .filter(|value| !value.is_empty());
            Some(Member {
                name,
                // A method's declared type is what it returns; a field's is
                // what it holds. Both decide how the value crosses.
                declared_type: returns,
                method: true,
                mutable: false,
            })
        })
        .collect()
}

/// Extracts the name and brace-balanced body of the first matching block.
fn block<'a>(source: &'a str, keyword: &str) -> Option<(String, &'a str)> {
    blocks(source, keyword)
        .into_iter()
        .next()
        .map(|(header, body)| (identifier(header.trim()), body))
}

/// Extracts every `keyword …  { … }` block, with the text between the keyword
/// and the brace so a caller can tell `impl X` from `impl Trait for X`.
fn blocks<'a>(source: &'a str, keyword: &str) -> Vec<(&'a str, &'a str)> {
    let mut found = Vec::new();
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let mut rest = line.trim_start();
        loop {
            let stripped = CLASS_MODIFIERS.iter().find_map(|modifier| {
                rest.strip_prefix(modifier)
                    .filter(|value| value.starts_with(char::is_whitespace))
                    .map(str::trim_start)
            });
            match stripped {
                Some(value) => rest = value,
                None => break,
            }
        }
        let Some(after) = rest
            .strip_prefix(keyword)
            .filter(|value| value.starts_with(char::is_whitespace))
        else {
            offset += line.len();
            continue;
        };
        let header = after.trim_start();
        let header = &header[..header.find(['{', ';', '\n']).unwrap_or(header.len())];
        let tail = &source[offset..];
        let Some(brace) = tail.find('{') else {
            break;
        };
        // A unit or tuple struct ends at a semicolon: it is a real declaration
        // with no body of its own. Reading on to the next brace made the
        // following `impl` its body and every method body inside it a list of
        // fields — which is how `std::thread::spawn` became a member `std`.
        if tail[..brace].contains(';') {
            found.push((header, ""));
            offset += line.len();
            continue;
        }
        let brace = brace + offset;
        let mut depth = 0usize;
        for (position, character) in source[brace..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        found.push((header, &source[brace + 1..brace + position]));
                        break;
                    }
                }
                _ => {}
            }
        }
        offset += line.len();
    }
    found
}

/// Renders the table, reaching the instance through a thread-local cell.
///
/// A `static mut` would need `unsafe` at every access and warns in this
/// edition; the cell costs nothing in a single-threaded module.
fn render_rust(struct_name: &str, instance: &str, members: &[Member]) -> String {
    let mut out = format!(
        "\n// Generated from `struct {struct_name}`. Declaring `fn tac()` yourself \
         replaces this.\nthread_local! {{\n    static TAC_INSTANCE: \
         core::cell::RefCell<{struct_name}> =\n        \
         core::cell::RefCell::new({instance});\n}}\n\n\
         pub fn tac() -> Vec<(&'static str, TacMember)> {{\n    vec![\n"
    );
    for member in members {
        let name = &member.name;
        // A view names members as the template writes them, which is camel
        // case; Rust declares them snake_cased by convention. The same
        // reconciliation C# gets for its Pascal case.
        let key = camel_case(name);
        let line = if member.method {
            let kind = member.declared_type.as_deref().unwrap_or("()");
            let call = rust_read(kind, &format!("it.borrow().{name}()"));
            format!("        (\"{key}\", TacMember::Method(|_| TAC_INSTANCE.with(|it| {call}))),\n")
        } else {
            let kind = member.declared_type.as_deref().unwrap_or("i64");
            // A borrowed string is Copy, so cloning it is a no-op rustc warns
            // about — and a project building with warnings denied would not
            // compile at all.
            let access = if kind == "&str" {
                format!("it.borrow().{name}")
            } else {
                format!("it.borrow().{name}.clone()")
            };
            let read = rust_read(kind, &access);
            // A borrowed string cannot be assigned an owned one, so it is
            // read-only — the same answer Swift's `let` and C#'s `=>` get.
            let write = rust_write(kind).map_or_else(
                || String::from("None"),
                |conversion| {
                    format!(
                        "Some(|value| TAC_INSTANCE.with(|it| it.borrow_mut().{name} = {conversion}))"
                    )
                },
            );
            format!(
                "        (\"{key}\", TacMember::Field {{\n            \
                 read: || TAC_INSTANCE.with(|it| {read}),\n            \
                 write: {write},\n        \
                 }}),\n"
            )
        };
        out.push_str(&line);
    }
    out.push_str("    ]\n}\n");
    out
}

/// Wraps a Rust expression of the declared type as a boundary value.
fn rust_read(kind: &str, expression: &str) -> String {
    match kind {
        "f32" | "f64" => format!("TacValue::Float(f64::from({expression}))"),
        "bool" => format!("TacValue::Flag({expression})"),
        "String" | "&str" => format!("TacValue::Text(({expression}).to_string())"),
        "()" => format!("{{ {expression}; TacValue::Null }}"),
        // A cast rather than `i64::from`: `usize` and `u64` have no such
        // conversion, because their width is the platform's to decide. A
        // companion counting cores is an ordinary thing to write.
        _ => format!("TacValue::Int(({expression}) as i64)"),
    }
}

/// Converts an incoming boundary value into the declared Rust type.
fn rust_write(kind: &str) -> Option<String> {
    Some(match kind {
        "f32" => String::from("value.as_float() as f32"),
        "f64" => String::from("value.as_float()"),
        "bool" => String::from("value.as_flag()"),
        "String" => String::from("value.as_text()"),
        // Nothing owned can be assigned to a borrowed string.
        "&str" => return None,
        _ => format!("value.as_int() as {kind}"),
    })
}

/// Reads one declaration, if the line is one.
fn declaration(
    language: CompanionLanguage,
    line: &str,
    source_path: &str,
) -> Result<Option<Member>, Failure> {
    match language {
        CompanionLanguage::Swift => {
            name_first(line, source_path, "var", "let", "func", "tac.swift")
        }
        CompanionLanguage::Kotlin => name_first(line, source_path, "var", "val", "fun", "tac.kt"),
        CompanionLanguage::CSharp => Ok(type_first(line)),
        // Rust is read from its struct and impl by `rust_table`.
        CompanionLanguage::Rust => Ok(None),
    }
}

/// Reads a `var name: Type` / `func name()` declaration.
fn name_first(
    line: &str,
    source_path: &str,
    mutable: &str,
    immutable: &str,
    function: &str,
    language: &str,
) -> Result<Option<Member>, Failure> {
    // A property wrapper or an annotation sits in front of the declaration it
    // decorates — `@Stored("visits") var visits: Int = 0`. It says how the
    // property stores itself, not whether it is one, so it is stepped over.
    let line = strip_attribute(line);
    // A private member is not part of the class's surface, and the generated
    // table lives outside the class: naming one produced code that did not
    // compile at all.
    if line.starts_with("private ") || line.starts_with("fileprivate ") {
        return Ok(None);
    }
    let line = line.trim_start_matches("public ");
    if let Some(rest) = line.strip_prefix(function).and_then(after_keyword) {
        let name: String = identifier(rest);
        return Ok((!name.is_empty()).then_some(Member {
            name,
            declared_type: None,
            method: true,
            mutable: false,
        }));
    }
    for (keyword, writable) in [(mutable, true), (immutable, false)] {
        let Some(rest) = line.strip_prefix(keyword).and_then(after_keyword) else {
            continue;
        };
        let name = identifier(rest);
        if name.is_empty() {
            continue;
        }
        let annotation = rest[name.len()..].trim_start();
        let declared_type = annotation.strip_prefix(':').map(|value| {
            value
                .split('=')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        });
        if writable && declared_type.is_none() {
            // The setter has to convert an incoming JSON value into something,
            // and only the declaration says what. Inferring it would mean
            // implementing the language's type checker.
            return Err(missing_type(&name, source_path, language));
        }
        return Ok(Some(Member {
            name,
            declared_type,
            method: false,
            mutable: writable,
        }));
    }
    Ok(None)
}

/// Modifiers a type-first declaration may carry, and whether each one fixes
/// the value. A modifier that does means the table gets no setter.
const TYPE_FIRST_MODIFIERS: [(&str, bool); 6] = [
    ("public ", false),
    ("internal ", false),
    ("static ", false),
    ("final ", true),
    ("readonly ", true),
    ("const ", true),
];

/// Reads a `Type name;` / `Type name()` declaration.
fn type_first(line: &str) -> Option<Member> {
    let mut line = line.trim();
    // As above: the table cannot reach a private member.
    if line.starts_with("private ") || line.starts_with("protected ") {
        return None;
    }
    let mut fixed = false;
    while let Some((rest, immutable)) = TYPE_FIRST_MODIFIERS
        .iter()
        .find_map(|(modifier, immutable)| Some((line.strip_prefix(modifier)?, *immutable)))
    {
        fixed |= immutable;
        line = rest.trim_start();
    }
    if line.starts_with("//") || line.starts_with('@') {
        return None;
    }
    let declared_type = identifier(line);
    if declared_type.is_empty() {
        return None;
    }
    let rest = line[declared_type.len()..].trim_start();
    // A generic or nullable type carries punctuation the identifier scan
    // stops at, which this generator deliberately does not try to parse.
    if rest.starts_with('<') || rest.starts_with('?') {
        return None;
    }
    let name = identifier(rest);
    if name.is_empty() {
        return None;
    }
    let after = rest[name.len()..].trim_start();
    let method = after.starts_with('(');
    // An expression body or an accessor list without a setter is a getter, and
    // generating `Instance.Label = …` for one does not compile. That is a
    // build failure rather than a wrong answer, but only after the author has
    // waited for a publish to tell them.
    let getter = after.starts_with("=>")
        || (after.starts_with('{')
            && !after
                .split_once('}')
                .map_or(after, |(accessors, _)| accessors)
                .split(|value: char| !value.is_alphanumeric())
                .any(|word| word == "set" || word == "init"));
    Some(Member {
        name,
        declared_type: Some(declared_type),
        method,
        mutable: !method && !fixed && !getter,
    })
}

/// Drops a leading `@Attribute` or `@Attribute(argument)` from a declaration.
///
/// Swift spells a property wrapper this way and Kotlin an annotation; both
/// decorate the declaration that follows rather than replacing it. Skipping
/// the line instead lost the member, and a companion that stored a field
/// ended up with no table at all.
fn strip_attribute(line: &str) -> &str {
    let mut rest = line.trim_start();
    while let Some(after) = rest.strip_prefix('@') {
        let name_len = after
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .map(char::len_utf8)
            .sum::<usize>();
        if name_len == 0 {
            return rest;
        }
        let tail = &after[name_len..];
        // An argument list belongs to the attribute, not to the declaration.
        rest = tail.strip_prefix('(').map_or(tail, |arguments| {
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
        });
        rest = rest.trim_start();
    }
    rest
}

/// Returns the leading identifier of a fragment.
fn identifier(value: &str) -> String {
    value
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect()
}

/// Returns what follows a keyword, when the keyword is a whole word.
fn after_keyword(rest: &str) -> Option<&str> {
    rest.starts_with(char::is_whitespace)
        .then(|| rest.trim_start())
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

/// Renders the member table the prelude reads.
fn render(language: CompanionLanguage, class_name: &str, members: &[Member]) -> String {
    let (open, close) = match language {
        CompanionLanguage::Swift => (
            format!("private let tacInstance = {class_name}()\nlet tac: [String: TacMember] = [\n"),
            String::from("]\n"),
        ),
        CompanionLanguage::Kotlin => (
            format!("private val tacInstance = {class_name}()\nval tac = mapOf(\n"),
            String::from(")\n"),
        ),
        CompanionLanguage::Rust => (String::new(), String::new()),
        CompanionLanguage::CSharp => (
            format!(
                "public static class TacBridge\n{{\n    private static readonly {class_name} \
                 Instance = new {class_name}();\n    public static readonly \
                 Dictionary<string, TacMember> Tac = new()\n    {{\n"
            ),
            String::from("    };\n}\n"),
        ),
    };

    let mut out = format!(
        "\n// Generated from `class {class_name}`. Declaring the table yourself \
         replaces this.\n{open}"
    );
    for member in members {
        out.push_str(&entry(language, member));
    }
    out.push_str(&close);
    out
}

/// Renders one member as its language spells a table entry.
fn entry(language: CompanionLanguage, member: &Member) -> String {
    let name = &member.name;
    match language {
        CompanionLanguage::Swift => match (member.method, &member.declared_type, member.mutable) {
            (true, _, _) => format!("    \"{name}\": .method({{ _ in tacInstance.{name}() }}),\n"),
            (false, Some(kind), true) => format!(
                "    \"{name}\": .field({{ tacInstance.{name} }}, \
                 {{ tacInstance.{name} = $0 as? {kind} ?? tacInstance.{name} }}),\n"
            ),
            (false, _, _) => format!("    \"{name}\": .field({{ tacInstance.{name} }}),\n"),
        },
        CompanionLanguage::Kotlin => match (member.method, &member.declared_type, member.mutable) {
            (true, _, _) => format!("    \"{name}\" to TacMethod {{ tacInstance.{name}() }},\n"),
            (false, Some(kind), true) => {
                let converter = kotlin_converter(kind);
                format!(
                    "    \"{name}\" to TacField({{ tacInstance.{name} }}, \
                     {{ tacInstance.{name} = {converter} }}),\n"
                )
            }
            (false, _, _) => format!("    \"{name}\" to TacField({{ tacInstance.{name} }}),\n"),
        },
        // Rust renders its whole table in `render_rust`, entry included.
        CompanionLanguage::Rust => String::new(),
        CompanionLanguage::CSharp => {
            // A view names members as the template writes them, which is camel
            // case; C# declares them Pascal-cased by convention.
            let key = lower_first(name);
            match (member.method, &member.declared_type, member.mutable) {
                (true, Some(kind), _) if kind == "void" => format!(
                    "        [\"{key}\"] = TacMember.Method(arguments => {{ Instance.{name}(); return null; }}),\n"
                ),
                (true, _, _) => format!(
                    "        [\"{key}\"] = TacMember.Method(arguments => Instance.{name}()),\n"
                ),
                (false, Some(kind), true) => {
                    let converter = csharp_converter(kind);
                    format!(
                        "        [\"{key}\"] = TacMember.Field(() => Instance.{name}, \
                         value => Instance.{name} = {converter}),\n"
                    )
                }
                (false, _, _) => {
                    format!("        [\"{key}\"] = TacMember.Field(() => Instance.{name}),\n")
                }
            }
        }
    }
}

/// The prelude helper that turns an incoming value into a Kotlin type.
fn kotlin_converter(kind: &str) -> &'static str {
    match kind {
        "Int" => "tacInt(it)",
        "Long" => "tacInt(it).toLong()",
        "Double" | "Float" => "tacDouble(it)",
        "Boolean" => "tacBoolean(it)",
        _ => "tacString(it)",
    }
}

/// Rust members are `snake_case` by convention; a view names them as written in
/// the template, which is camel case.
fn camel_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut capitalise = false;
    for character in value.chars() {
        if character == '_' {
            capitalise = true;
        } else if capitalise {
            out.extend(character.to_uppercase());
            capitalise = false;
        } else {
            out.push(character);
        }
    }
    out
}

/// C# members are Pascal-cased by convention; a view names them as written in
/// the template, which is camel case.
fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().collect::<String>() + characters.as_str()
    })
}

fn csharp_converter(kind: &str) -> &'static str {
    match kind {
        "int" => "Convert.ToInt32(value)",
        "long" => "Convert.ToInt64(value)",
        "double" | "float" => "Convert.ToDouble(value)",
        "bool" => "Convert.ToBoolean(value)",
        _ => "Convert.ToString(value)",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{CompanionLanguage, member_table};

    /// A companion whose behaviour is all in its `impl` still reads correctly.
    ///
    /// `struct Probe;` is a real declaration with no body. Reading on to the
    /// next brace made the following `impl` its body, so every line of every
    /// method body was scanned as a field list — which is how
    /// `std::thread::spawn(|| {` became a member named `std` with a declared
    /// type of `:thread::spawn(|| {`, and the generated table did not compile.
    #[test]
    fn a_unit_struct_takes_its_members_from_its_impl_alone() {
        let generated = member_table(
            CompanionLanguage::Rust,
            "pub struct Probe;\n\n\
             impl Probe {\n\
             \x20   pub fn new() -> Self { Self }\n\
             \x20   pub fn start(&self) -> String {\n\
             \x20       std::thread::spawn(|| {\n\
             \x20           tac_publish(\"tick\", TacValue::Int(1));\n\
             \x20       });\n\
             \x20       String::from(\"started\")\n\
             \x20   }\n\
             }\n",
            "tac.rs",
        )
        .expect("a unit struct is a struct")
        .expect("its impl declares members");
        assert!(generated.contains("\"start\""), "{generated}");
        assert!(
            !generated.contains("\"std\""),
            "a method body was read as fields: {generated}"
        );
    }

    /// The same companion, authored idiomatically in every language, has to
    /// name the same members with the same mutability.
    ///
    /// A developer writes one companion and then writes it again for another
    /// target; if the two disagree about what the page can reach, the page
    /// renders differently on two platforms and nothing says why. So the shape
    /// is fixed — a class, fields with values, methods — and each language
    /// spells it its own way.
    #[test]
    fn every_language_reads_the_same_class_the_same_way() {
        let authored = [
            (
                CompanionLanguage::Swift,
                "tac.swift",
                "final class Companion {\n  var count: Int = 6\n  \
                 let runtime = \"Swift\"\n\n  func doubled() -> Int { count * 2 }\n}\n",
            ),
            (
                CompanionLanguage::Kotlin,
                "tac.kt",
                "class Companion {\n  var count: Int = 6\n  \
                 val runtime: String = \"Kotlin\"\n\n  fun doubled(): Int = count * 2\n}\n",
            ),
            (
                CompanionLanguage::CSharp,
                "tac.cs",
                "public class Companion\n{\n  public int Count = 6;\n  \
                 public string Runtime => \"C#\";\n\n  public int Doubled() => Count * 2;\n}\n",
            ),
            (
                CompanionLanguage::Rust,
                "tac.rs",
                "pub struct Companion {\n  pub count: i32,\n  pub runtime: &'static str,\n}\n\n\
                 impl Companion {\n  pub fn new() -> Self {\n    \
                 Self { count: 6, runtime: \"Rust\" }\n  }\n\n  \
                 pub fn doubled(&self) -> i32 {\n    self.count * 2\n  }\n}\n",
            ),
        ];
        for (language, name, source) in authored {
            let generated = member_table(language, source, &format!("client/pages/{name}"))
                .expect("generated")
                .unwrap_or_else(|| panic!("{name} produced no table"));
            for member in ["count", "runtime", "doubled"] {
                assert!(
                    generated.contains(&format!("\"{member}\"")),
                    "{name} lost {member}:\n{generated}"
                );
            }
            // `new` and `default` are how an instance is made, not something
            // the page can call.
            assert!(!generated.contains("\"new\""), "{name}:\n{generated}");
            assert!(!generated.contains("\"default\""), "{name}:\n{generated}");
        }
    }

    /// A read-only field need not declare its type, and is still a field.
    ///
    /// `let runtime = "x"` is how Swift and Kotlin write one, and it is what
    /// JavaScript's `runtime = 'x'` looks like in those languages. It had been
    /// read as a *method*, because the table decided that from the absence of
    /// a type — so the generated code called a string, and the companion did
    /// not compile.
    #[test]
    fn an_untyped_read_only_field_is_a_field_and_not_a_method() {
        for (language, name, source, expected) in [
            (
                CompanionLanguage::Swift,
                "tac.swift",
                "final class Companion {\n  let runtime = \"Swift\"\n}\n",
                "\"runtime\": .field({ tacInstance.runtime }),",
            ),
            (
                CompanionLanguage::Kotlin,
                "tac.kt",
                "class Companion {\n  val runtime = \"Kotlin\"\n}\n",
                "\"runtime\" to TacField({ tacInstance.runtime }),",
            ),
        ] {
            let generated = member_table(language, source, &format!("client/pages/{name}"))
                .expect("generated")
                .unwrap_or_else(|| panic!("{name} produced no table"));
            assert!(generated.contains(expected), "{name}:\n{generated}");
            assert!(!generated.contains("runtime()"), "{name}:\n{generated}");
        }
    }

    #[test]
    fn a_platform_width_integer_field_crosses_as_a_number() {
        // `i64::from(usize)` does not exist, so the generated table did not
        // compile for a companion that counted anything.
        let table = super::rust_member_table(
            "pub struct Companion {\n    pub cores: usize,\n}\n\n\
             impl Companion {\n    pub fn new() -> Self {\n        \
             Self { cores: 1 }\n    }\n}\n",
            "client/pages/tac.rs",
        )
        .expect("table");
        assert!(table.contains("as i64"), "{table}");
        assert!(!table.contains("i64::from"), "{table}");
    }

    #[test]
    fn a_private_member_is_not_in_the_table() {
        // It is not part of the class's surface, and the generated table lives
        // outside the class — naming one produced code that did not compile.
        for (language, name, source) in [
            (
                CompanionLanguage::Swift,
                "tac.swift",
                "final class Companion {\n  private let secret = \"x\"\n  let runtime = \"Swift\"\n}\n",
            ),
            (
                CompanionLanguage::Kotlin,
                "tac.kt",
                "class Companion {\n  private val secret = \"x\"\n  val runtime = \"Kotlin\"\n}\n",
            ),
            (
                CompanionLanguage::CSharp,
                "tac.cs",
                "public class Companion\n{\n  private string Secret => \"x\";\n  \
                 public string Runtime => \"C#\";\n}\n",
            ),
        ] {
            let generated = member_table(language, source, &format!("client/pages/{name}"))
                .expect("generated")
                .unwrap_or_else(|| panic!("{name} produced no table"));
            assert!(generated.contains("untime"), "{name}:\n{generated}");
            assert!(!generated.contains("ecret"), "{name}:\n{generated}");
        }
    }

    #[test]
    fn a_swift_class_becomes_a_member_table() {
        let generated = member_table(
            CompanionLanguage::Swift,
            "final class Companion {\n    var count: Int = 6\n    \
             let label: String = \"a\"\n    func doubled() -> Int { count * 2 }\n}\n",
            "client/components/x/tac.swift",
        )
        .expect("generated")
        .expect("a table");
        assert!(
            generated.contains("private let tacInstance = Companion()"),
            "{generated}"
        );
        // A mutable field gets a setter typed by its declaration.
        assert!(
            generated.contains("$0 as? Int ?? tacInstance.count"),
            "{generated}"
        );
        // An immutable one is read-only rather than silently writable.
        assert!(
            generated.contains("\"label\": .field({ tacInstance.label }),"),
            "{generated}"
        );
        assert!(
            generated.contains(".method({ _ in tacInstance.doubled() })"),
            "{generated}"
        );
    }

    #[test]
    fn a_rust_struct_and_its_inherent_impl_become_a_table() {
        let generated = member_table(
            CompanionLanguage::Rust,
            "pub struct Companion {\n    count: i32,\n    label: String,\n}\n\n\
             impl Default for Companion {\n    fn default() -> Self {\n        \
             Self { count: 6, label: String::from(\"Rust\") }\n    }\n}\n\n\
             impl Companion {\n    fn doubled(&self) -> i32 {\n        self.count * 2\n    }\n}\n",
            "client/components/x/tac.rs",
        )
        .expect("generated")
        .expect("a table");
        // Only the inherent impl contributes methods: `impl Default for
        // Companion` is where the starting values live, and taking the first
        // impl found silently picked it.
        assert!(
            generated.contains("\"doubled\", TacMember::Method"),
            "{generated}"
        );
        assert!(!generated.contains("\"default\""), "{generated}");
        // Each type crosses as itself rather than as an assumed integer.
        assert!(
            generated.contains("TacValue::Int((it.borrow().count.clone()) as i64)"),
            "{generated}"
        );
        assert!(
            generated.contains("TacValue::Text((it.borrow().label.clone()).to_string())"),
            "{generated}"
        );
        assert!(generated.contains("value.as_text()"), "{generated}");
        assert!(generated.contains("value.as_int() as i32"), "{generated}");
    }

    #[test]
    fn a_borrowed_string_field_is_read_only_rather_than_dropped() {
        // It had been dropped in silence: `&'static str` is how a constant
        // string field is written, and the page rendered nothing for it.
        let table = super::rust_member_table(
            "pub struct Companion {\n    pub runtime: &'static str,\n}\n\n\
             impl Default for Companion {\n    fn default() -> Self {\n        \
             Self { runtime: \"Rust\" }\n    }\n}\n",
            "client/pages/tac.rs",
        )
        .expect("table");
        assert!(table.contains("\"runtime\""), "{table}");
        assert!(table.contains("TacValue::Text"), "{table}");
        assert!(table.contains("write: None"), "{table}");
        // `&str` is Copy, so cloning it is a no-op rustc warns about.
        assert!(!table.contains("runtime.clone()"), "{table}");
    }

    #[test]
    fn a_rust_struct_without_a_constructor_is_a_diagnostic() {
        // A Rust struct has no inline field defaults, so the value a field
        // starts at lives in a constructor and there is nothing to build
        // without one.
        let failure = member_table(
            CompanionLanguage::Rust,
            "pub struct Companion {\n    count: i32,\n}\n",
            "client/components/x/tac.rs",
        )
        .expect_err("no constructor");
        assert!(
            failure.to_string().contains("needs a constructor"),
            "{failure}"
        );
        assert!(
            failure.to_string().contains("fn new() -> Self"),
            "{failure}"
        );
    }

    #[test]
    fn a_rust_constructor_is_used_over_default() {
        let table = super::rust_member_table(
            "pub struct Companion {\n    pub count: i32,\n}\n\n\
             impl Companion {\n    pub fn new() -> Self {\n        \
             Self { count: 6 }\n    }\n}\n",
            "client/pages/tac.rs",
        )
        .expect("table");
        assert!(table.contains("RefCell::new(Companion::new())"), "{table}");
        assert!(!table.contains("\"new\""), "{table}");
    }

    #[test]
    fn a_hand_written_table_is_left_alone() {
        // The generator fills a gap; it does not overrule an author.
        let source = "class Companion { var count: Int = 1 }\n\
                      let tac: [String: TacMember] = [:]\n";
        assert_eq!(
            member_table(CompanionLanguage::Swift, source, "x/tac.swift").expect("read"),
            None
        );
    }

    #[test]
    fn a_mutable_field_without_a_type_is_a_diagnostic() {
        // The setter has to convert an incoming JSON value into something, and
        // only the declaration says what.
        let failure = member_table(
            CompanionLanguage::Swift,
            "class Companion {\n    var count = 6\n}\n",
            "client/components/x/tac.swift",
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
            "class Companion {\n    var count: Int = 1\n    \
             fun doubled(): Int {\n        val scratch = count\n        return scratch * 2\n    }\n}\n",
            "x/tac.kt",
        )
        .expect("generated")
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
        .expect("generated")
        .expect("a table");
        // A view names members as the template writes them.
        assert!(csharp.contains("[\"count\"]"), "{csharp}");
        assert!(csharp.contains("Convert.ToInt32(value)"), "{csharp}");
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_class_behind_it() {
        // Notepad, Visual Studio and PowerShell 5.1's `Set-Content -Encoding
        // utf8` all write one. U+FEFF is not whitespace, so the declaration
        // behind it was not found, the table came out empty, and the generated
        // bridge referred to a TacBridge nobody had written.
        let generated = member_table(
            CompanionLanguage::CSharp,
            "\u{feff}public class Companion\n{\n    public int Count { get; set; } = 6;\n}\n",
            "x/tac.cs",
        )
        .expect("generated")
        .expect("a table");
        assert!(generated.contains("TacBridge"), "{generated}");
        assert!(generated.contains("[\"count\"]"), "{generated}");
    }

    #[test]
    fn a_field_the_author_cannot_assign_gets_no_setter() {
        // Generating one does not compile, and the author only finds out when
        // the publish fails several minutes later.
        let csharp = member_table(
            CompanionLanguage::CSharp,
            "public class Companion\n{\n    public int Count { get; set; } = 6;\n    \
             public string Label => \"Native\";\n    public readonly int Fixed = 1;\n    \
             public int Reading { get; }\n}\n",
            "x/tac.cs",
        )
        .expect("generated")
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
