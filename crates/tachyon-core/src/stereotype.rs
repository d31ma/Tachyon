//! Mandatory layer stereotypes for the server.
//!
//! Spring and `NestJS` use these to register a class with a dependency-injection
//! container. Yon has no container to register with — a handler is one bounded
//! process per request, so nothing an injector built would outlive the response
//! it was built for. Reproducing the annotations without saying so would be the
//! worst of both: a spelling every backend developer recognises, doing none of
//! what they would reasonably expect it to do.
//!
//! So they are kept, and what they mean is narrowed to something this
//! architecture can actually honour. A stereotype declares which layer a class
//! belongs to, and Tachyon checks the declaration against where the file sits
//! and against what it is allowed to reach. That is most of what the Spring
//! stereotypes buy a codebase in practice — architectural intent, written down
//! and enforced — and it is the part that does not need a container.
//!
//! Each language spells an annotation its own way, and none is invented here:
//! Java and Kotlin write `@Controller`, C# writes `[Controller]`, PHP and Rust
//! write `#[controller]`, Python and TypeScript write `@controller`. A language
//! with no annotation syntax at all is not given one — see
//! `ANNOTATED_LANGUAGES`.

use crate::Failure;
use crate::failure::{diagnostic, source_span};
use std::collections::BTreeSet;
use std::path::Path;

/// The layers a server class may declare itself part of.
///
/// Five, not Spring's three, because the scaffold already lays a project out in
/// five and has since before this existed: "A route receives a request and
/// returns a response. A service owns a decision. A repository owns storage. A
/// client owns an outbound call. A delegate owns work handed to something
/// else." Naming only three of them would have left `server/clients` and
/// `server/delegates` outside a vocabulary that claims to describe the layers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stereotype {
    /// Answers a request. Lives under `server/routes`.
    Controller,
    /// Owns a decision. Lives under `server/services`.
    Service,
    /// Owns storage. Lives under `server/repositories`.
    Repository,
    /// Owns one outbound dependency. Lives under `server/clients`.
    Client,
    /// Owns work handed to something else. Lives under `server/delegates`.
    Delegate,
}

impl Stereotype {
    /// Every stereotype, outermost layer first.
    pub const ALL: [Self; 5] = [
        Self::Controller,
        Self::Service,
        Self::Repository,
        Self::Client,
        Self::Delegate,
    ];

    /// The name as it is written in every language that has one.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Controller => "Controller",
            Self::Service => "Service",
            Self::Repository => "Repository",
            Self::Client => "Client",
            Self::Delegate => "Delegate",
        }
    }

    /// The directory a class carrying this stereotype has to live under.
    #[must_use]
    pub const fn root(self) -> &'static str {
        match self {
            Self::Controller => "server/routes",
            Self::Service => "server/services",
            Self::Repository => "server/repositories",
            Self::Client => "server/clients",
            Self::Delegate => "server/delegates",
        }
    }

    /// How far down the stack this layer sits. A layer may reach a deeper one
    /// and never a shallower one, which is the whole of the rule.
    ///
    /// The bottom three share a depth because they are peers, not a stack: a
    /// repository, a client and a delegate each own one edge of the system and
    /// none of them is beneath another. Sharing a depth is what stops a
    /// repository calling a client, which is the arrangement that turns a
    /// storage class into a second service.
    const fn depth(self) -> u8 {
        match self {
            Self::Controller => 0,
            Self::Service => 1,
            Self::Repository | Self::Client | Self::Delegate => 2,
        }
    }

    /// Whether a class in this layer may reference one in `other`.
    #[must_use]
    pub const fn may_reach(self, other: Self) -> bool {
        self.depth() < other.depth()
    }

    /// The stereotype a path implies, from the directory it sits in.
    #[must_use]
    pub fn of_path(source: &Path) -> Option<Self> {
        let portable = source.to_string_lossy().replace('\\', "/");
        // Longest root first: `server/routes` is not a prefix of any other, but
        // matching in declaration order is what keeps that true if one is added.
        Self::ALL
            .into_iter()
            .find(|layer| portable.starts_with(&format!("{}/", layer.root())))
    }
}

/// File extensions whose language has annotation syntax Tachyon can read.
///
/// JavaScript is here with a condition attached. No engine implements
/// decorators — V8 has not shipped them, and Node's own documentation says it
/// will not polyfill a Stage 3 proposal, which is why Node 26 removed the
/// transform flag rather than extending it. Every runtime that accepts one
/// transpiles it, and two do: `ty --javascript-runtime $(command -v bun)` runs
/// the same `yon.js` that Node rejects with a parse error, as does Deno.
///
/// Rust is here, and the reason it was once absent was wrong. A custom
/// attribute is a procedural macro, and a procedural macro needs a *crate* —
/// not a Cargo manifest. `rustc --crate-type=proc-macro` builds one from a
/// single file in under a tenth of a second, and `--extern` puts it in reach of
/// the handler, which is two invocations of a compiler Tachyon already drives.
///
/// The absentees are absent for reasons no toolchain trick fixes. Go has no
/// annotation syntax: a struct tag is metadata on a field rather than on a
/// type, and a `//go:` directive is a comment. Ruby has none either.
pub const ANNOTATED_LANGUAGES: [&str; 8] = ["js", "ts", "py", "java", "cs", "kt", "php", "rs"];

/// How one language opens an annotation, and what closes it.
const SYNTAX: [(&str, &str, &str); 8] = [
    ("js", "@", ""),
    ("ts", "@", ""),
    ("py", "@", ""),
    ("java", "@", ""),
    ("kt", "@", ""),
    ("cs", "[", "]"),
    ("php", "#[", "]"),
    // Rust spells an attribute the way PHP does, and it sits on the item it
    // describes: a struct, an impl or a method.
    ("rs", "#[", "]"),
];

fn extension(source: &Path) -> String {
    source
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Preserves executable source and line structure while blanking lexical
/// constructs that a language compiler ignores for annotation discovery.
fn executable_source(source: &Path, contents: &str) -> String {
    let mask = crate::lexical::code_mask(&extension(source), contents);
    let mut visible = contents.as_bytes().to_vec();
    for (byte, is_code) in visible.iter_mut().zip(mask.code) {
        if !is_code && *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
    String::from_utf8(visible).unwrap_or_default()
}

/// Whether this file's language can carry a stereotype at all.
#[must_use]
pub fn is_annotated_language(source: &Path) -> bool {
    ANNOTATED_LANGUAGES.contains(&extension(source).as_str())
}

/// Finds the stereotype a source declares, if it declares one.
///
/// Textual over the shared code-only lexical mask: parsing eight languages to
/// find one word at the start of a line would be eight parsers to maintain,
/// while scanning raw text would let comments and strings impersonate source.
#[must_use]
pub fn declared(source: &Path, contents: &str) -> Option<Stereotype> {
    let contents = executable_source(source, contents);
    let extension = extension(source);
    let (_, open, close) = SYNTAX
        .into_iter()
        .find(|(language, _, _)| *language == extension)?;
    for line in contents.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(open) else {
            continue;
        };
        for layer in Stereotype::ALL {
            // Python names a decorator with an ordinary function, which its
            // own style guide lowercases; the other five capitalise an
            // annotation. It is one stereotype either way, so the name is
            // matched without case rather than each language being given its
            // own spelling to remember.
            let Some(after) = rest
                .get(..layer.name().len())
                .filter(|head| head.eq_ignore_ascii_case(layer.name()))
                .map(|head| &rest[head.len()..])
            else {
                continue;
            };
            // `@Controller`, `@Controller()` and `@Controller("/x")` all
            // declare the same thing; `@ControllerAdvice` declares something
            // else entirely and must not match.
            let boundary = after
                .chars()
                .next()
                .is_none_or(|character| !character.is_alphanumeric() && character != '_');
            if boundary && (close.is_empty() || after.contains(close)) {
                return Some(layer);
            }
        }
    }
    None
}

/// The type a stereotype is attached to, and which stereotype it is.
///
/// Found textually over the same code-only mask as the stereotype itself. The
/// declaration always follows the annotation, so that is where it is looked
/// for without allowing a comment or string to supply either half.
///
/// This is what lets a handler be found by what it declares rather than by
/// being called `Handler`. A name is a convention every language has to be
/// told; an annotation is the thing the author already wrote to say what the
/// class is.
#[must_use]
pub fn declared_class(source: &Path, contents: &str) -> Option<(Stereotype, String)> {
    let contents = executable_source(source, contents);
    let extension = extension(source);
    let (_, open, close) = SYNTAX
        .into_iter()
        .find(|(language, _, _)| *language == extension)?;
    let mut found: Option<Stereotype> = None;
    for line in contents.lines() {
        let line = line.trim();
        if let Some(layer) = found {
            // The declaration may be several lines below the annotation, with
            // other attributes between: `#[Controller]` then `#[derive(...)]`
            // then the struct is ordinary Rust.
            if line.is_empty() || line.starts_with(open) || line.starts_with("//") {
                continue;
            }
            if let Some(name) = type_name(line) {
                return Some((layer, name));
            }
            // Anything else means the annotation was not on a declaration.
            return None;
        }
        if let Some(rest) = line.strip_prefix(open) {
            found = Stereotype::ALL.into_iter().find(|layer| {
                let Some(after) = rest
                    .get(..layer.name().len())
                    .filter(|head| head.eq_ignore_ascii_case(layer.name()))
                    .map(|head| &rest[head.len()..])
                else {
                    return false;
                };
                let boundary = after
                    .chars()
                    .next()
                    .is_none_or(|character| !character.is_alphanumeric() && character != '_');
                boundary && (close.is_empty() || after.contains(close))
            });
        }
    }
    None
}

/// The name a declaration gives its type, whichever keyword introduces it.
fn type_name(line: &str) -> Option<String> {
    const KEYWORDS: [&str; 5] = ["class", "struct", "object", "interface", "record"];
    let mut rest = line;
    // Modifiers first: `public`, `final`, `pub`, `export`, `sealed`, `data`.
    loop {
        let stripped = [
            "public",
            "private",
            "internal",
            "protected",
            "final",
            "open",
            "abstract",
            "sealed",
            "static",
            "pub",
            "pub(crate)",
            "export",
            "default",
            "data",
            "partial",
        ]
        .into_iter()
        .find_map(|modifier| {
            rest.strip_prefix(modifier)
                .filter(|value| value.starts_with(char::is_whitespace))
                .map(str::trim_start)
        });
        match stripped {
            Some(value) => rest = value,
            None => break,
        }
    }
    let after = KEYWORDS.into_iter().find_map(|keyword| {
        rest.strip_prefix(keyword)
            .filter(|value| value.starts_with(char::is_whitespace))
            .map(str::trim_start)
    })?;
    let name: String = after
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The methods the protocol dispatches. Anything else a controller declares is
/// never called through a route.
const HTTP_METHODS: [&str; 8] = [
    "GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD", "TRACE",
];

/// Selects the entry point's body, not methods in colocated compiled helpers.
/// Rust declares methods in separate inherent `impl` blocks, all of which
/// belong to the same controller. The input has already had literals and
/// comments masked, so their braces cannot extend a controller's scope.
fn compiled_controller_source(source: &Path, executable: &str) -> String {
    let language = extension(source);
    if !matches!(language.as_str(), "java" | "cs" | "kt" | "rs") {
        return executable.to_owned();
    }
    let Some((Stereotype::Controller, name)) = declared_class(source, executable) else {
        return executable.to_owned();
    };
    if language == "rs" {
        return rust_controller_source(executable, &name);
    }
    let mut result = String::new();
    let mut offset = 0;
    let mut consumed_until = 0;
    for line in executable.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        if start < consumed_until {
            continue;
        }
        let declaration = line.trim();
        if type_name(declaration).as_deref() == Some(name.as_str()) {
            let Some((body, consumed)) = braced_body(&executable[start..]) else {
                break;
            };
            result.push_str(body);
            result.push('\n');
            consumed_until = start + consumed;
        }
    }
    result
}

fn rust_controller_source(executable: &str, name: &str) -> String {
    let mut result = String::new();
    let mut consumed_until = 0;
    for (start, _) in executable.match_indices("impl") {
        if start < consumed_until
            || executable[..start]
                .chars()
                .next_back()
                .is_some_and(identifier_character)
            || executable[start + 4..]
                .chars()
                .next()
                .is_some_and(identifier_character)
        {
            continue;
        }
        let declaration = &executable[start + 4..];
        let Some(open) = rust_impl_body_start(declaration) else {
            // Malformed headers must not erase the remaining methods.
            result.push_str(declaration);
            break;
        };
        let Some((body, consumed)) = braced_body(&declaration[open..]) else {
            break;
        };
        if inherent_impl(&declaration[..open], name) {
            result.push_str(body);
            result.push('\n');
        }
        consumed_until = start + 4 + open + consumed;
    }
    result
}

fn identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Find the body after generics and where bounds, not a const-generic block.
fn rust_impl_body_start(declaration: &str) -> Option<usize> {
    let mut angles = 0usize;
    let mut braces = 0usize;
    let bytes = declaration.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'{' if angles == 0 && braces == 0 => return Some(at),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if braces == 0 => angles += 1,
            b'>' if braces == 0 && at.checked_sub(1).and_then(|i| bytes.get(i)) != Some(&b'-') => {
                angles = angles.saturating_sub(1);
            }
            b';' if angles == 0 && braces == 0 => return None,
            _ => {}
        }
    }
    None
}

fn inherent_impl(header: &str, name: &str) -> bool {
    // The target follows the optional generic parameter list. Retain unknown
    // headers conservatively rather than silently exempting their methods.
    let Some(target) = after_rust_generics(header.trim()) else {
        return true;
    };
    let target = target.trim_start();
    let path_end = target
        .find(|character: char| !identifier_character(character) && character != ':')
        .unwrap_or(target.len());
    if target[..path_end].rsplit("::").next() != Some(name) {
        return false;
    }
    let Some(after) = after_rust_generics(target[path_end..].trim_start()) else {
        return true;
    };
    let after = after.trim_start();
    after.is_empty()
        || after
            .strip_prefix("where")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn after_rust_generics(source: &str) -> Option<&str> {
    if !source.starts_with('<') {
        return Some(source);
    }
    let mut depth = 0usize;
    let mut braces = 0usize;
    let bytes = source.as_bytes();
    for (at, byte) in bytes.iter().enumerate() {
        match byte {
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'<' if braces == 0 => depth += 1,
            b'>' if braces == 0 && at.checked_sub(1).and_then(|i| bytes.get(i)) != Some(&b'-') => {
                depth -= 1;
                if depth == 0 {
                    return Some(&source[at + 1..]);
                }
            }
            _ => {}
        }
    }
    None
}

fn braced_body(declaration: &str) -> Option<(&str, usize)> {
    let open = declaration.find(['{', ';'])?;
    if declaration.as_bytes()[open] != b'{' {
        return None;
    }
    let mut depth = 1usize;
    for (offset, byte) in declaration.as_bytes()[open + 1..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Some((&declaration[open + 1..open + 1 + offset], open + 2 + offset));
        }
    }
    // Keep malformed bodies visible to validation; the compiler diagnoses the
    // missing delimiter. Do not silently accept a truncated empty controller.
    Some((&declaration[open + 1..], declaration.len()))
}

/// The index of the parenthesis closing the one opened at `open`.
///
/// Counted rather than searched, because a parameter list holds parentheses of
/// its own — a default value, a generic bound, a nested type.
fn closing_parenthesis(line: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, character) in line.char_indices().skip(open) {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The names of the methods declared in one source file.
///
/// Textual, because this runs over eight languages and a parser for each is
/// eight parsers to keep. Two shapes cover all of them: a keyword before the
/// name, and — for the languages that put a return type there instead — a
/// signature that opens a block.
fn declared_methods(contents: &str) -> Vec<String> {
    const KEYWORDS: [&str; 4] = ["fn", "def", "fun", "function"];
    // Words that take a parenthesis and open a block without declaring
    // anything, which is the one thing the second shape can be confused with.
    const CONTROL: [&str; 12] = [
        "if", "for", "while", "switch", "catch", "else", "do", "try", "using", "lock", "foreach",
        "match",
    ];
    let mut names = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        let Some(open) = line.find('(') else { continue };
        let before = &line[..open];
        let name: String = before
            .chars()
            .rev()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if name.is_empty() || CONTROL.contains(&name.as_str()) {
            continue;
        }
        let head = before[..before.len() - name.len()].trim_end();
        let keyword = KEYWORDS
            .into_iter()
            .any(|keyword| head.split_whitespace().next_back() == Some(keyword));
        // Java, C# and TypeScript put a return type where the others put a
        // keyword, so the block is what marks the declaration. `{` anywhere
        // after the parameters rather than at the end of the line, because a
        // one-line body puts the `}` there.
        // A name reached through an accessor is a call, not a declaration:
        // `Yon.Relay(new[] { … })` is an argument list with a brace in it,
        // which is otherwise indistinguishable from a one-line method body.
        // An annotation is not a declaration either — `@Relay({"ruby", "x"})`
        // carries braces for the same reason and reads the same way.
        let called = ["::", "->", "?.", ".", "@", "[", "#["]
            .into_iter()
            .any(|prefix| head.ends_with(prefix));
        // What follows the parameter list is what tells a declaration from a
        // call. A signature closes its parentheses and then either stops — the
        // brace is on the next line, which is how C# and Java are usually
        // written — or opens a block or a body. A call closes and keeps going:
        // `String(value).trim()` continues into a method, and a statement ends
        // in a semicolon.
        //
        // Reading only "contains a brace" missed every brace-on-the-next-line
        // declaration; reading "ends with a parenthesis" instead called
        // `String(…).toLowerCase()` a declaration. This reads the boundary.
        let after = closing_parenthesis(line, open).map(|at| line[at + 1..].trim_start());
        // Not a colon. Every language whose signature ends in one puts a
        // keyword in front of it — Python's `def`, Kotlin's `fun` — so the
        // keyword rule already has them, and accepting a colon here instead
        // read `for tick in range(1, 4):` as a declaration of `range`.
        let declares = after.is_some_and(|rest| {
            rest.is_empty()
                || rest.starts_with('{')
                || rest.starts_with("=>")
                || rest.starts_with("->")
                || rest.starts_with("where")
        });
        let statement = ["new", "raise", "throw", "return", "await", "yield"]
            .contains(&head.split_whitespace().next_back().unwrap_or_default());
        let signature = !head.is_empty() && !called && !statement && declares;
        if keyword || signature {
            names.push(name);
        }
    }
    names
}

/// The languages whose handlers can answer more than once.
///
/// A stream is a handler that yields. The two built-in adapters hold a
/// generator open across frames, and PHP now does the same: `yield` makes a
/// method return a `Generator`, and the prelude writes one length-prefixed
/// frame per value. The reader never asked which language wrote them.
///
/// Six of the eight. Java and Rust are the two left out, and deliberately:
/// neither has a generator, so `yield` is not what an author would write, and
/// a route that must stream can be one of the six. ADR 0017 has the whole of
/// it.
pub const STREAMING_LANGUAGES: [&str; 6] = ["js", "ts", "py", "php", "kt", "cs"];

/// The HTTP methods a source declares `@Stream` on.
///
/// Textual, like every other scan here: this runs before any compiler does, so
/// there is nothing but the source to read.
#[must_use]
pub fn streaming_methods(source: &Path, contents: &str) -> BTreeSet<String> {
    let contents = executable_source(source, contents);
    let contents = compiled_controller_source(source, &contents);
    let extension = extension(source);
    let mut declared = BTreeSet::new();
    let Some((_, open, close)) = SYNTAX
        .into_iter()
        .find(|(language, _, _)| *language == extension)
    else {
        return declared;
    };
    let mut pending = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let marks_stream = line.strip_prefix(open).is_some_and(|rest| {
            rest.strip_prefix("Stream").is_some_and(|tail| {
                let tail = tail.strip_suffix(close).unwrap_or(tail);
                tail.is_empty() || tail == "()"
            })
        });
        if marks_stream {
            pending = true;
            continue;
        }
        if pending {
            if let Some(name) = declared_methods(line)
                .into_iter()
                .find(|name| HTTP_METHODS.contains(&name.to_ascii_uppercase().as_str()))
            {
                declared.insert(name.to_ascii_uppercase());
                pending = false;
                continue;
            }
            // Another annotation between `@Stream` and the method is ordinary,
            // and it may carry the declaration with it: `@JvmStatic fun GET(…)`
            // on one line is what Kotlin is usually written as, so the method
            // is looked for before the line is dismissed as an annotation.
            if line.starts_with(open) {
                continue;
            }
            pending = false;
        }
    }
    declared
}

/// Whether the body of one method yields, in the language's own word for it.
///
/// The point of `@Stream` is that it and the body cannot disagree, so one of
/// them has to be read. This reads the body, from the declaration to the next
/// method declaration.
fn yields(contents: &str, method: &str) -> bool {
    let mut inside = false;
    for line in contents.lines() {
        let declares = declared_methods(line);
        if declares
            .iter()
            .any(|name| name.eq_ignore_ascii_case(method))
        {
            inside = true;
            // A one-line body puts the yield on the declaration itself.
            if line.contains("yield") {
                return true;
            }
            continue;
        }
        if !inside {
            continue;
        }
        // Another HTTP method ends this one.
        if declares
            .iter()
            .any(|name| HTTP_METHODS.contains(&name.to_ascii_uppercase().as_str()))
        {
            return false;
        }
        if line.contains("yield") {
            return true;
        }
    }
    false
}

/// Checks `@Stream` against the body it sits on, in both directions.
///
/// The server decides which path a route takes from the annotation and the
/// handler decides what it returns from the body, so the two cannot be allowed
/// to disagree. A route declared streaming whose method returns one value
/// would hold a connection open for nothing; a method that yields without the
/// annotation would have its generator serialised as a value, which in PHP is
/// an empty object.
fn check_streaming(source: &Path, contents: &str, portable: &str) -> Result<(), Failure> {
    let executable = executable_source(source, contents);
    let executable = compiled_controller_source(source, &executable);
    let streams = streaming_methods(source, &executable);
    if let Some(method) = streams.iter().next()
        && !STREAMING_LANGUAGES.contains(&extension(source).as_str())
    {
        return Err(Failure::one(diagnostic(
            2014,
            format!("'{portable}' declares @Stream on '{method}()'."),
            Some(format!(
                "A stream is a handler that answers more than once, and only {} can do that \
                 — the direct protocol writes one envelope and exits. Move the route to one \
                 of those, or return a single response.",
                STREAMING_LANGUAGES
                    .iter()
                    .map(|extension| format!("yon.{extension}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            source_span(portable, 0, contents.len()),
        )));
    }
    for method in &streams {
        if !yields(&executable, method) {
            return Err(Failure::one(diagnostic(
                2013,
                format!("'{portable}' declares @Stream on '{method}()', which does not yield."),
                Some(String::from(
                    "A stream is a handler that yields, and @Stream is how the route says so \
                     before it is called. Yield the events, or remove the annotation.",
                )),
                source_span(portable, 0, contents.len()),
            )));
        }
    }
    if let Some((Stereotype::Controller, _)) = declared_class(source, contents) {
        for method in declared_methods(&executable) {
            let upper = method.to_ascii_uppercase();
            if HTTP_METHODS.contains(&upper.as_str())
                && yields(&executable, &method)
                && !streams.contains(&upper)
            {
                return Err(Failure::one(diagnostic(
                    2013,
                    format!("'{portable}' yields from '{method}()', which is not @Stream."),
                    Some(String::from(
                        "The server decides whether a route streams before it calls the \
                         handler, so yielding is not enough on its own. Declare @Stream on \
                         the method.",
                    )),
                    source_span(portable, 0, contents.len()),
                )));
            }
        }
    }

    Ok(())
}

/// Checks one server source against the layering contract.
///
/// # Errors
///
/// Returns TY2008 when a class declares a stereotype that does not match the
/// directory it is in, and TY2009 when it reaches a layer above its own.
pub fn check(source: &Path, contents: &str) -> Result<(), Failure> {
    let portable = source.to_string_lossy().replace('\\', "/");
    // Interpreted middleware and scheduled workers are invoked through the
    // same class-and-method adapter as routes. They are therefore controller
    // entry points even though their orchestration homes are deliberately
    // outside `server/routes`. Direct-protocol middleware (for example PHP)
    // remains a script: imposing a class contract on it would reject an
    // already-supported protocol form that has no class dispatch step.
    let interpreted_controller = matches!(extension(source).as_str(), "js" | "ts" | "py");
    let worker_controller = interpreted_controller && portable.starts_with("server/workers/");
    let middleware_controller = interpreted_controller
        && !portable.contains('/')
        && Path::new(&portable)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("middleware."));
    let sits_in = Stereotype::of_path(source)
        .or_else(|| (worker_controller || middleware_controller).then_some(Stereotype::Controller));
    let placement_root = if worker_controller {
        "server/workers"
    } else if middleware_controller {
        "the project root"
    } else {
        sits_in.map_or("server", Stereotype::root)
    };
    let executable = executable_source(source, contents);

    if let Some(expected) = sits_in
        && declared_class(source, contents).is_none()
    {
        return Err(Failure::one(diagnostic(
            2015,
            format!(
                "'{portable}' does not declare a class with @{}.",
                expected.name()
            ),
            Some(format!(
                "Every source under {}/ must attach @{} to its layer class. The legacy \
                 class-name fallback was removed; add the annotation and name the class with \
                 the {} suffix.",
                placement_root,
                expected.name(),
                expected.name()
            )),
            source_span(&portable, 0, contents.len()),
        )));
    }

    if let Some(found) = declared(source, contents)
        && sits_in != Some(found)
    {
        return Err(Failure::one(diagnostic(
            2008,
            format!(
                "'{portable}' declares @{} but is not under {}/.",
                found.name(),
                found.root()
            ),
            Some(format!(
                "A stereotype names the layer a class is in, so it has to agree with where the \
                 file is. Move the file under {}/, or declare the stereotype for the directory \
                 it is in.",
                found.root()
            )),
            source_span(&portable, 0, contents.len()),
        )));
    }

    // A stereotype names what a class is, so the name should say it too. The
    // suffix is the one part of a naming convention worth enforcing: it is
    // visible at every call site, where the annotation is not.
    if let Some((layer, name)) = declared_class(source, contents)
        && !name.ends_with(layer.name())
    {
        return Err(Failure::one(diagnostic(
            2011,
            format!(
                "'{portable}' declares @{} on '{name}', which does not end in {}.",
                layer.name(),
                layer.name()
            ),
            Some(format!(
                "A layer is visible at every call site through the name, and only at the \
                 declaration through the annotation. Rename it to something ending in {} — \
                 `Orders{}`, say.",
                layer.name(),
                layer.name()
            )),
            source_span(&portable, 0, contents.len()),
        )));
    }

    // A controller's methods are dispatched by name, so a method that is not
    // an HTTP method is never reached through a route.
    if let Some((Stereotype::Controller, name)) = declared_class(source, contents)
        && let Some(extra) = declared_methods(&compiled_controller_source(source, &executable))
            .into_iter()
            .find(|method| !HTTP_METHODS.contains(&method.to_ascii_uppercase().as_str()))
    {
        return Err(Failure::one(diagnostic(
            2012,
            format!("'{portable}' declares '{extra}()' on the controller '{name}'."),
            Some(String::from(
                "A route dispatches on the HTTP method, so a controller answers GET, POST, \
                 PUT, PATCH, DELETE, OPTIONS, HEAD or TRACE and nothing else. Move the rest \
                 into a service under server/services.",
            )),
            source_span(&portable, 0, contents.len()),
        )));
    }

    check_streaming(source, contents, &portable)?;

    sits_in.map_or(Ok(()), |layer| {
        check_references(layer, &portable, &executable)
    })
}

fn check_references(layer: Stereotype, portable: &str, contents: &str) -> Result<(), Failure> {
    // A reference is looked for by name in executable source rather than by
    // resolving each language's import syntax. Comments and strings were
    // already masked, so architectural examples and payload values cannot
    // impersonate a dependency edge.
    //
    // Both separators are looked for, because four of the six languages spell
    // a module path with dots: `from server.services.billing import charge` is
    // the same reference as `require('../services/billing')` and checking only
    // for a slash would have missed it in Python, Java, Kotlin and C#.
    for other in Stereotype::ALL {
        if other == layer || layer.may_reach(other) {
            continue;
        }
        let dotted = other.root().replace('/', ".");
        if contents.contains(other.root()) || contents.contains(&dotted) {
            return Err(Failure::one(diagnostic(
                2009,
                format!(
                    "'{portable}' is a @{} and reaches {}/.",
                    layer.name(),
                    other.root()
                ),
                Some(if layer.depth() == other.depth() {
                    // Peers, not a stack. "Move it down" is meaningless advice
                    // when there is nothing below either of them.
                    format!(
                        "@{} and @{} are peers: each owns one edge of the system, so neither \
                         calls the other. Move the work that needs both up into a @Service, \
                         which is the layer allowed to reach each of them.",
                        layer.name(),
                        other.name()
                    )
                } else {
                    format!(
                        "A layer may reach a deeper one and never a shallower one: a controller \
                         may use a service, and a service may use a repository, a client or a \
                         delegate. Invert the call so the deeper layer returns what the \
                         shallower one needs, or move the shared code into {}/.",
                        layer.root()
                    )
                }),
                source_span(portable, 0, contents.len()),
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{Stereotype, check, declared, declared_class, is_annotated_language};
    use std::path::Path;

    #[test]
    fn every_language_spells_the_annotation_its_own_way() {
        // None of these is invented: they are what the language already uses
        // for an annotation, which is the whole reason a stereotype can be
        // written at all in six of them and in none of the rest.
        let cases = [
            (
                "server/routes/x/yon.java",
                "@Controller\npublic class Handler {}",
            ),
            ("server/routes/x/yon.kt", "@Controller\nclass Handler"),
            ("server/routes/x/yon.cs", "[Controller]\nclass Handler {}"),
            ("server/routes/x/yon.php", "#[Controller]\nclass Handler {}"),
            (
                "server/routes/x/yon.py",
                "@controller\nclass Handler:\n    pass",
            ),
            (
                "server/routes/x/yon.ts",
                "@Controller\nexport class Handler {}",
            ),
        ];
        for (path, code) in cases {
            assert_eq!(
                declared(Path::new(path), code),
                Some(Stereotype::Controller),
                "{path} declares a controller"
            );
        }
    }

    #[test]
    fn a_language_without_annotations_is_not_given_one() {
        // Go has no annotation syntax — a struct tag is metadata on a field,
        // and a `//go:` directive is a comment — and neither has Ruby. Rust is
        // not here: it has attributes, and a proc macro to define one needs a
        // crate rather than a manifest.
        for path in ["yon.go", "yon.rb"] {
            assert!(!is_annotated_language(Path::new(path)), "{path}");
            assert_eq!(
                declared(Path::new(path), "@Controller\nclass Handler {}"),
                None
            );
        }
    }

    #[test]
    fn a_longer_name_starting_with_a_stereotype_is_a_different_annotation() {
        // Spring's own `@ControllerAdvice` is the case that makes this matter.
        let source = Path::new("server/routes/x/yon.java");
        assert_eq!(
            declared(source, "@ControllerAdvice\nclass Handler {}"),
            None
        );
        assert_eq!(
            declared(source, "@Controller(\"/products\")\nclass Handler {}"),
            Some(Stereotype::Controller)
        );
    }

    #[test]
    fn comments_and_every_string_form_cannot_declare_a_stereotype_or_class() {
        let cases = [
            (
                "yon.js",
                "const decoy = `@Repository\nclass WrongRepository {}`\n\
                 /* @Service\nclass WrongService {} */\n\
                 @Controller\nexport class OrdersController {}",
            ),
            (
                "yon.ts",
                "const decoy: string = `@Repository\nclass WrongRepository {}`\n\
                 // @Service\n// class WrongService {}\n\
                 @Controller\nexport class OrdersController {}",
            ),
            (
                "yon.py",
                "decoy = '''@repository\nclass WrongRepository:\n    pass'''\n\
                 # @service\n# class WrongService: pass\n\
                 @controller\nclass OrdersController:\n    pass",
            ),
            (
                "yon.java",
                "class Text { String value = \"\"\"@Repository\nclass WrongRepository {}\"\"\"; }\n\
                 /* @Service\nclass WrongService {} */\n\
                 @Controller\npublic class OrdersController {}",
            ),
            (
                "yon.cs",
                "class Text { string Value = @\"@Repository\nclass WrongRepository { \"\"x\"\" }\"; }\n\
                 // [Service]\n// class WrongService {}\n\
                 [Controller]\npublic class OrdersController {}",
            ),
            (
                "yon.kt",
                "val decoy = \"\"\"@Repository\nclass WrongRepository {}\"\"\"\n\
                 /* @Service\nclass WrongService {} */\n\
                 @Controller\nclass OrdersController",
            ),
            (
                "yon.php",
                "<?php\n$decoy = '#[Repository] class WrongRepository {}';\n\
                 /* #[Service]\nclass WrongService {} */\n\
                 #[Controller]\nclass OrdersController {}",
            ),
            (
                "yon.rs",
                "const DECOY: &str = r##\"#[Repository]\nstruct WrongRepository;\"##;\n\
                 /* outer /* #[Service] */ struct WrongService; */\n\
                 #[Controller]\npub struct OrdersController;",
            ),
        ];
        for (name, code) in cases {
            let source = Path::new(name);
            assert_eq!(
                declared(source, code),
                Some(Stereotype::Controller),
                "{name} selected a lexical decoy"
            );
            assert_eq!(
                declared_class(source, code),
                Some((Stereotype::Controller, String::from("OrdersController"))),
                "{name} attached a lexical decoy"
            );
        }
    }

    #[test]
    fn a_stereotype_has_to_agree_with_the_directory_it_is_in() {
        let failure = check(
            Path::new("server/routes/x/yon.java"),
            "@Repository\npublic class Handler {}",
        )
        .expect_err("a repository in a route directory");
        assert!(failure.to_string().contains("TY2008"), "{failure}");
        check(
            Path::new("server/repositories/orders.java"),
            "@Repository\npublic class OrdersRepository {}",
        )
        .expect("a repository where repositories live");
    }

    #[test]
    fn a_scheduled_worker_is_a_controller_shaped_protocol_entry_point() {
        check(
            Path::new("server/workers/heartbeat.py"),
            "@Controller\nclass HeartbeatController:\n    @staticmethod\n    def POST(request):\n        return {\"status\": 204}\n",
        )
        .expect("scheduled worker controller");

        let failure = check(
            Path::new("server/workers/heartbeat.py"),
            "class Heartbeat:\n    pass\n",
        )
        .expect_err("worker without protocol controller");
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2015"), "{rendered}");
        assert!(rendered.contains("server/workers"), "{rendered}");
    }

    #[test]
    fn root_middleware_is_a_controller_shaped_protocol_entry_point() {
        check(
            Path::new("middleware.py"),
            "@Controller\nclass AccessController:\n    @staticmethod\n    def GET(request):\n        return {\"status\": 204}\n",
        )
        .expect("root middleware controller");
    }

    #[test]
    fn the_five_layers_are_the_five_the_scaffold_already_writes() {
        // The vocabulary is not invented here: `ty init` has laid a project out
        // in these five since before this module existed, and naming only
        // Spring's three would have left two of the scaffold's own directories
        // outside the rule that claims to describe them.
        let roots: Vec<_> = Stereotype::ALL.into_iter().map(Stereotype::root).collect();
        assert_eq!(
            roots,
            [
                "server/routes",
                "server/services",
                "server/repositories",
                "server/clients",
                "server/delegates"
            ]
        );
    }

    #[test]
    fn the_bottom_three_are_peers_rather_than_a_stack() {
        // A repository, a client and a delegate each own one edge of the
        // system. None is beneath another, so none may call another: that
        // arrangement is what turns a storage class into a second service.
        for layer in [
            Stereotype::Repository,
            Stereotype::Client,
            Stereotype::Delegate,
        ] {
            for other in [
                Stereotype::Repository,
                Stereotype::Client,
                Stereotype::Delegate,
            ] {
                assert!(
                    !layer.may_reach(other),
                    "{layer:?} must not reach {other:?}"
                );
            }
            assert!(Stereotype::Service.may_reach(layer));
            assert!(!layer.may_reach(Stereotype::Service));
        }
    }

    #[test]
    fn a_stereotype_names_the_class_it_is_on() {
        // The suffix is the one part of a naming convention worth enforcing: a
        // layer is visible at every call site through the name, and only at
        // the declaration through the annotation.
        let failure = check(
            Path::new("server/routes/orders/yon.java"),
            "@Controller\nclass Handler {}",
        )
        .expect_err("a controller not named as one");
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2011"), "{rendered}");
        assert!(
            rendered.contains("does not end in Controller"),
            "{rendered}"
        );

        check(
            Path::new("server/routes/orders/yon.java"),
            "@Controller\nclass OrdersController {}",
        )
        .expect("a controller named as one");
        check(
            Path::new("server/repositories/orders.java"),
            "@Repository\nclass OrderRepository {}",
        )
        .expect("a repository named as one");
    }

    #[test]
    fn the_class_a_stereotype_sits_on_is_found_in_every_language() {
        // The handler is found by what it declares rather than by being called
        // `Handler`, so reading the name off the declaration has to work
        // wherever the annotation does.
        let cases = [
            ("yon.java", "@Controller\npublic class OrdersController {}"),
            ("yon.kt", "@Controller\nobject OrdersController"),
            ("yon.cs", "[Controller]\nsealed class OrdersController {}"),
            ("yon.php", "#[Controller]\nfinal class OrdersController {}"),
            ("yon.rs", "#[Controller]\npub struct OrdersController;"),
            ("yon.ts", "@Controller\nexport class OrdersController {}"),
            ("yon.py", "@Controller\nclass OrdersController:"),
        ];
        for (name, code) in cases {
            assert_eq!(
                declared_class(Path::new(name), code),
                Some((Stereotype::Controller, String::from("OrdersController"))),
                "{name}"
            );
        }
    }

    #[test]
    fn colocated_compiled_helpers_are_not_controller_methods() {
        for (extension, code) in [
            (
                "java",
                "@Controller\nclass OrdersController {\nstatic Object GET(Object request) { return null; }\n}\n@Service\nclass OrderService {\nstatic Object list() { return null; }\n}",
            ),
            (
                "cs",
                "[Controller]\nclass OrdersController\n{\nstatic object GET(object request) => null;\n}\n[Service]\nclass OrderService\n{\nstatic object List() => null;\n}",
            ),
            (
                "kt",
                "@Controller\nobject OrdersController {\nfun GET(request: Any) = request\n}\n@Service\nobject OrderService {\nfun list() = listOf(1)\n}",
            ),
            (
                "rs",
                "#[Controller]\nstruct OrdersController;\nimpl OrdersController {\nfn GET() {}\n}\n#[Service]\nstruct OrderService;\nimpl OrderService {\nfn list() {}\n}",
            ),
        ] {
            let path = format!("server/routes/orders/yon.{extension}");
            check(Path::new(&path), code)
                .unwrap_or_else(|failure| panic!("{extension}: {failure}"));
        }
    }

    #[test]
    fn helper_streams_do_not_change_the_controller_protocol() {
        let path = Path::new("server/routes/orders/yon.kt");
        let code = "@Controller\nobject OrdersController {\nfun GET(request: Any) = request\n}\n@Service\nobject OrderService {\n@Stream\nfun GET() = sequence { yield(1) }\n}";
        check(path, code).expect("only the controller determines a route's streaming protocol");
        assert!(super::streaming_methods(path, code).is_empty());
    }

    #[test]
    fn controller_scope_preserves_literals_nested_blocks_and_all_inherent_impls() {
        let path = Path::new("server/routes/orders/yon.rs");
        let code = "#[Controller]\nstruct OrdersController;\nimpl\nOrdersController {\nfn GET() { if true { let text = \"} fn hidden() {\"; } }\n}\nimpl OrdersControllerExtra {\nfn helper() {}\n}\nimpl OrdersController {\nfn secret() {}\n}";
        let failure = check(path, code).expect_err("methods in later controller impls are checked");
        assert!(failure.to_string().contains("secret()"), "{failure}");
        assert!(!failure.to_string().contains("helper()"), "{failure}");
        check(path, &code.replace("fn secret()", "fn POST()"))
            .expect("nested blocks, literal braces and another type do not change scope");
    }

    #[test]
    fn compiled_controller_body_boundaries_fail_closed() {
        assert_eq!(super::braced_body("class X; class Y {}"), None);
        assert_eq!(super::braced_body("class X"), None);
        assert_eq!(
            super::braced_body("class X { fn secret() {}").map(|(body, _)| body),
            Some(" fn secret() {}")
        );
        let path = Path::new("server/routes/orders/yon.java");
        let failure = check(
            path,
            "@Controller\nclass OrdersController {\nstatic Object hidden() {}\n",
        )
        .expect_err("a malformed body does not erase controller methods");
        assert!(failure.to_string().contains("TY2012"), "{failure}");
    }

    #[test]
    fn blank_lines_do_not_duplicate_rust_controller_bodies() {
        let path = Path::new("server/routes/orders/yon.rs");
        let code = "#[Controller]\nstruct OrdersController;\n\n\n\nimpl\nOrdersController {\nfn GET() {}\n}";
        let body = super::compiled_controller_source(path, code);
        assert_eq!(super::declared_methods(&body), ["GET"]);
    }

    #[test]
    fn rust_controller_impls_are_independent_of_line_layout_and_generics() {
        let path = Path::new("server/routes/orders/yon.rs");
        for declaration in [
            "impl<'a> OrdersController",
            "impl <'a> OrdersController",
            "impl<T: Into<Vec<u8>>> OrdersController<T>",
            "impl<T: Fn() -> Vec<u8>> OrdersController<T> where T: Send",
            "impl OrdersController<{ 1 > 0 }>",
            "impl self::OrdersController",
            "impl OrdersController",
        ] {
            let code = format!(
                "#[Controller]\nstruct OrdersController; {declaration} {{\nfn secret() {{}}\n}} impl OrdersController {{\nfn POST() {{}}\n}}"
            );
            let failure = check(path, &code).expect_err("every inherent impl is checked");
            assert!(
                failure.to_string().contains("secret()"),
                "{declaration}: {failure}"
            );
            check(path, &code.replace("secret()", "GET()"))
                .expect("valid controller methods in generic and same-line impls");
        }
    }

    #[test]
    fn rust_trait_impls_and_prefix_matches_do_not_become_controller_methods() {
        let path = Path::new("server/routes/orders/yon.rs");
        let code = "#[Controller]\nstruct OrdersController;\nimpl OrdersController for Other {\nfn helper() {}\n}\nimpl<'a> Trait<'a> for OrdersController {\nfn helper() {}\n}\nimpl OrdersControllerExtra {\nfn helper() {}\n}\nimpl OrdersController {\nfn GET() {}\n}";
        check(path, code).expect("only inherent methods belong to the controller protocol");
    }

    #[test]
    fn malformed_nested_declarations_do_not_amplify_the_source() {
        let path = Path::new("server/routes/orders/yon.rs");
        let code = format!(
            "#[Controller]\nstruct OrdersController;\n{}fn GET() {{}}\n{}",
            "impl OrdersController {\n".repeat(1_000),
            "}\n".repeat(1_000)
        );
        let body = super::compiled_controller_source(path, &code);
        assert!(body.len() <= code.len());
        assert_eq!(super::declared_methods(&body), ["GET"]);
    }

    #[test]
    fn a_controller_answers_http_methods_and_nothing_else() {
        // The dispatch finds the method by name, so a method that is not an
        // HTTP method is never reached through a route — it reads as working
        // and never runs.
        let failure = check(
            Path::new("server/routes/orders/yon.java"),
            "@Controller\nclass OrdersController {\n\
             static YonResponse GET(YonRequest request) { return null; }\n\
             static String loadOrders() { return \"\"; }\n}",
        )
        .expect_err("a helper on a controller");
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2012"), "{rendered}");
        assert!(rendered.contains("loadOrders()"), "{rendered}");

        // Every language it runs, and only the HTTP methods in each.
        for (name, code) in [
            (
                "server/routes/orders/yon.rs",
                "#[Controller]\nstruct OrdersController;\nimpl OrdersController {\n\
                 fn GET(_request: &YonRequest) -> YonResponse { todo!() }\n}",
            ),
            (
                "server/routes/orders/yon.py",
                "@Controller\nclass OrdersController:\n    def POST(request):\n        pass",
            ),
            (
                "server/routes/orders/yon.kt",
                "@Controller\nobject OrdersController {\n\
                 fun DELETE(request: YonRequest): YonResponse = TODO()\n}",
            ),
            (
                "server/routes/orders/yon.php",
                "#[Controller]\nfinal class OrdersController {\n\
                 public static function PATCH($request) { return null; }\n}",
            ),
            (
                "server/routes/orders/yon.ts",
                "@Controller\nexport class OrdersController {\n\
                 static OPTIONS(request: YonRequest) {\n    return null\n  }\n}",
            ),
        ] {
            check(Path::new(name), code)
                .unwrap_or_else(|failure| panic!("{name} declares only an HTTP method: {failure}"));
        }

        // A call is not a declaration. `Yon.Delegate(new[] { … })` reads as a
        // one-line method body to a scanner that does not look at what comes
        // before the name — which is how this was found.
        check(
            Path::new("server/routes/orders/yon.cs"),
            "[Controller]\nclass OrdersController {\n\
             static YonResponse GET(YonRequest request) =>\n\
             Yon.Delegate(new[] { \"ruby\", \"server/delegates/orders.rb\" }, request);\n}",
        )
        .expect("a controller delegating to another language");

        // A loop calling a function is not a declaration either. `range(1, 4):`
        // closes its parentheses and is followed by a colon, which is what a
        // Python signature ends with — the difference is the `def`, and the
        // framework's own streaming test was refused before this.
        check(
            Path::new("server/routes/ticks/yon.py"),
            "@Controller\nclass TicksController:\n    @staticmethod\n    @Stream\n    \
             def GET(request):\n        for tick in range(1, 4):\n            \
             yield {\"tick\": tick}",
        )
        .expect("a call in a loop header is not a declaration");

        // A call that closes its parentheses and keeps going is not a
        // declaration. `String(value).trim().toLowerCase()` reads as one to a
        // scanner that only asks how the line ends — and the framework's own
        // website was refused for it.
        check(
            Path::new("server/routes/search/yon.js"),
            "@Controller\nexport class SearchController {\n  static GET(request) {\n    \
             const query = String(request?.parameters?.query ?? '').trim().toLowerCase()\n    \
             return { query }\n  }\n}",
        )
        .expect("a call inside a handler is not a declaration");

        // A signature whose brace is on the next line is still a declaration.
        // C# and Java are usually written that way, and TY2012 saw none of
        // them until it was.
        let failure = check(
            Path::new("server/routes/orders/yon.cs"),
            "[Controller]\nclass OrdersController\n{\n\
             static YonResponse GET(YonRequest request)\n    {\n        return null;\n    }\n\
             static string Score(YonRequest request)\n    {\n        return \"\";\n    }\n}",
        )
        .expect_err("a helper whose brace is on the next line");
        assert!(failure.to_string().contains("TY2012"), "{failure}");
        assert!(failure.to_string().contains("Score()"), "{failure}");

        // An annotation carrying an array is not a method declaration, even
        // though it has a name, parentheses and braces in the same order.
        for (name, code) in [
            (
                "server/routes/orders/yon.java",
                "@Controller\nclass OrdersController {\n\
                 @Relay({\"ruby\", \"server/delegates/orders.rb\"})\n\
                 static YonResponse GET(YonRequest request) { return null; }\n}",
            ),
            (
                "server/routes/orders/yon.cs",
                "[Controller]\nclass OrdersController {\n\
                 [Relay(\"ruby\", \"server/delegates/orders.rb\")]\n\
                 static YonResponse GET(YonRequest request) => null;\n}",
            ),
            (
                "server/routes/orders/yon.rs",
                "#[Controller]\nstruct OrdersController;\nimpl OrdersController {\n\
                 #[Relay(\"ruby\", \"server/delegates/orders.rb\")]\n\
                 fn GET(request: &YonRequest) -> YonResponse {}\n}",
            ),
        ] {
            check(Path::new(name), code)
                .unwrap_or_else(|failure| panic!("{name} relays its only method: {failure}"));
        }

        // A rule that fires on a service too would ban every service method.
        check(
            Path::new("server/services/orders.py"),
            "@Service\nclass OrderService:\n    def load(self):\n        pass",
        )
        .expect("a service may name its methods anything");
    }

    #[test]
    fn a_stream_declares_itself_and_the_body_has_to_agree() {
        // The server decides which path a route takes before it calls the
        // handler, so the annotation and the body cannot be allowed to
        // disagree — each is checked against the other.
        check(
            Path::new("server/routes/ticks/yon.py"),
            "@Controller\nclass TicksController:\n    @staticmethod\n    @Stream\n\
             def GET(request):\n        yield {\"tick\": 1}",
        )
        .expect("a declared stream that yields");

        let silent = check(
            Path::new("server/routes/ticks/yon.py"),
            "@Controller\nclass TicksController:\n    @staticmethod\n    @Stream\n\
             def GET(request):\n        return {\"tick\": 1}",
        )
        .expect_err("a declared stream that returns one value");
        assert!(silent.to_string().contains("TY2013"), "{silent}");
        assert!(silent.to_string().contains("does not yield"), "{silent}");

        let undeclared = check(
            Path::new("server/routes/ticks/yon.py"),
            "@Controller\nclass TicksController:\n    @staticmethod\n\
             def GET(request):\n        yield {\"tick\": 1}",
        )
        .expect_err("a generator nothing declared");
        assert!(undeclared.to_string().contains("TY2013"), "{undeclared}");
        assert!(
            undeclared.to_string().contains("not @Stream"),
            "{undeclared}"
        );
    }

    #[test]
    fn only_a_language_that_can_stream_may_declare_one() {
        // Refusing the annotation is how a language that cannot stream stays a
        // diagnostic rather than a handler that holds a connection open and
        // sends nothing. C# has a native generator; ADR 0017 scopes it into
        // the supported set. Rust and Java have none at all, so they are excluded
        // rather than deferred — the relay is what covers them.
        for (name, code) in [
            (
                "server/routes/ticks/yon.rs",
                "#[Controller]\nstruct TicksController;\nimpl TicksController {\n\
                 #[Stream]\n    fn GET(request: &YonRequest) -> YonResponse {}\n}",
            ),
            (
                "server/routes/ticks/yon.java",
                "@Controller\nclass TicksController {\n  @Stream\n  \
                 static YonResponse GET(YonRequest request) { return null; }\n}",
            ),
        ] {
            let refused = check(Path::new(name), code).expect_err(name);
            assert!(refused.to_string().contains("TY2014"), "{refused}");
            assert!(refused.to_string().contains("yon.js"), "{refused}");
        }

        // And the four that can, in each of their own spellings.
        for (name, code) in [
            (
                "server/routes/ticks/yon.js",
                "@Controller\nexport class TicksController {\n  @Stream\n  \
                 static async *GET(request) { yield 1 }\n}",
            ),
            (
                "server/routes/ticks/yon.ts",
                "@Controller\nexport class TicksController {\n  @Stream\n  \
                 static async *GET(request: unknown) { yield 1 }\n}",
            ),
            (
                "server/routes/ticks/yon.py",
                "@Controller\nclass TicksController:\n    @staticmethod\n    @Stream\n\
                 def GET(request):\n        yield 1",
            ),
            (
                "server/routes/ticks/yon.php",
                "<?php\n#[Controller]\nclass TicksController\n{\n\
                 #[Stream]\n    public static function GET($request) { yield 1; }\n}",
            ),
            (
                "server/routes/ticks/yon.kt",
                "@Controller\nobject TicksController {\n    @Stream\n\
                 @JvmStatic fun GET(request: YonRequest) = sequence { yield(1) }\n}",
            ),
            (
                "server/routes/ticks/yon.cs",
                "[Controller]\nclass TicksController\n{\n    [Stream]\n\
                 static IEnumerable<object> GET(YonRequest request)\n    {\n\
                 yield return 1;\n    }\n}",
            ),
        ] {
            check(Path::new(name), code).unwrap_or_else(|failure| panic!("{name}: {failure}"));
        }
    }

    #[test]
    fn a_layer_may_reach_a_deeper_one_and_never_a_shallower_one() {
        assert!(Stereotype::Controller.may_reach(Stereotype::Service));
        assert!(Stereotype::Service.may_reach(Stereotype::Repository));
        assert!(!Stereotype::Repository.may_reach(Stereotype::Service));
        assert!(!Stereotype::Service.may_reach(Stereotype::Controller));

        check(
            Path::new("server/routes/x/yon.py"),
            "@Controller\nclass XController:\n    def GET(request):\n        from server.services.orders import place\n",
        )
        .expect("a controller reaching a service");

        let failure = check(
            Path::new("server/repositories/orders.py"),
            "@Repository\nclass OrdersRepository:\n    def load(self):\n        from server.services.billing import charge\n",
        )
        .expect_err("a repository reaching a service");
        assert!(failure.to_string().contains("TY2009"), "{failure}");
    }

    #[test]
    fn a_call_in_a_statement_is_not_a_method_declaration() {
        check(
            Path::new("server/routes/ticks/yon.py"),
            "@Controller\nclass TicksController:\n    @Stream\n    def GET(request):\n        \
             yield {\"tick\": 1}\n        raise RuntimeError(\"stopped\")\n",
        )
        .expect("a raised exception is not a controller method");
    }

    #[test]
    fn every_layer_source_requires_an_attached_stereotype() {
        for (path, source, expected) in [
            (
                "server/routes/orders/yon.py",
                "class Handler:\n    def GET(request): pass\n",
                "@Controller",
            ),
            (
                "server/services/orders.py",
                "class OrdersService: pass\n",
                "@Service",
            ),
            (
                "server/repositories/orders.rs",
                "struct OrdersRepository;\n",
                "@Repository",
            ),
            (
                "server/clients/payments.java",
                "class PaymentsClient {}\n",
                "@Client",
            ),
            (
                "server/delegates/report.php",
                "class ReportDelegate {}\n",
                "@Delegate",
            ),
        ] {
            let failure = check(Path::new(path), source).expect_err(path);
            let rendered = failure.to_string();
            assert!(rendered.contains("TY2015"), "{rendered}");
            assert!(rendered.contains(expected), "{rendered}");
        }
    }
}
