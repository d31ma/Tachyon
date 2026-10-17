use std::collections::BTreeSet;
use std::path::Path;

struct DeclaredField {
    name: String,
    type_name: String,
    marked: bool,
}

pub(crate) struct RequiredFields;

impl RequiredFields {
    const PUBLISHER: &'static str = "ApplicationEventPublisher";
    const FILLED_FIELD_MARKERS: [&'static str; 6] = [
        "Needs",
        "Named",
        "Setting",
        "LocalState",
        "SessionState",
        "Fake",
    ];
    const LANGUAGES: [&'static str; 13] = [
        "js", "ts", "dart", "cs", "kt", "swift", "rs", "go", "rb", "lua", "java", "fs", "py",
    ];
    const NOT_FIELD_WORDS: [&'static str; 19] = [
        "static",
        "const",
        "class",
        "struct",
        "enum",
        "interface",
        "object",
        "companion",
        "fun",
        "func",
        "fn",
        "get",
        "set",
        "init",
        "deinit",
        "case",
        "typealias",
        "import",
        "return",
    ];

    pub(crate) fn props(
        path: &Path,
        source: &str,
        class: &str,
        injectable: &BTreeSet<String>,
    ) -> Vec<String> {
        let (_, props) = Self::injected_and_props(path, source, class, injectable);
        props
    }

    pub(crate) fn injected_fields(
        path: &Path,
        source: &str,
        class: &str,
        injectable: &BTreeSet<String>,
    ) -> Vec<String> {
        let (injected, _) = Self::injected_and_props(path, source, class, injectable);
        injected
    }

    pub(crate) fn service_dependencies(
        path: &Path,
        source: &str,
        class: &str,
        known: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        Self::fields(path, source, class)
            .into_iter()
            .map(|field| field.name)
            .filter(|name| known.contains(name))
            .collect()
    }

    fn injected_and_props(
        path: &Path,
        source: &str,
        class: &str,
        injectable: &BTreeSet<String>,
    ) -> (Vec<String>, Vec<String>) {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if extension == crate::tac_cpp::CppSource::EXTENSION {
            return (Vec::new(), crate::tac_cpp::CppRoles::props(path, source));
        }
        let local_classes = crate::annotation_layout::declaration_names(path, source)
            .unwrap_or_default()
            .into_iter()
            .filter(|name| name != class)
            .collect::<BTreeSet<_>>();
        let fields = Self::fields(path, source, class);
        let (injected, props): (Vec<_>, Vec<_>) = fields
            .into_iter()
            .partition(|field| Self::injected(field, injectable, &local_classes));
        (
            Self::prop_names(extension, injected),
            Self::prop_names(extension, props),
        )
    }

    fn fields(path: &Path, source: &str, class: &str) -> Vec<DeclaredField> {
        let Ok(code) = crate::annotation_layout::masked_source(path, source) else {
            return Vec::new();
        };
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !Self::LANGUAGES.contains(&extension) {
            return Vec::new();
        }
        match extension {
            "py" => Self::python_fields(path, source, class),
            "rb" => Self::ruby_fields(&code),
            "lua" => Self::lua_fields(source, class),
            "go" => Self::go_fields(source, class),
            "rs" => Self::rust_fields(&code, class),
            "fs" => Self::fsharp_fields(path, source, class),
            _ => Self::declared_fields(extension, &code, class),
        }
    }

    fn python_fields(path: &Path, source: &str, class: &str) -> Vec<DeclaredField> {
        let members = crate::behavior::PythonMembers::read(path, source, class).unwrap_or_default();
        members["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|field| field["uninitialized"] == true)
            .map(|field| DeclaredField {
                name: field["page"].as_str().unwrap_or_default().to_owned(),
                type_name: field["type"].as_str().unwrap_or_default().to_owned(),
                marked: false,
            })
            .collect()
    }

    fn prop_names(extension: &str, fields: Vec<DeclaredField>) -> Vec<String> {
        let names = fields.into_iter().map(|field| field.name);
        if extension != "cs" {
            return names.collect();
        }
        names
            .map(|name| crate::companion::lower_first(&name))
            .collect()
    }

    pub(crate) fn constructed(path: &Path, source: &str, class: &str) -> Vec<(String, String)> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !matches!(extension, "swift" | "kt" | "cs") {
            return Vec::new();
        }
        let Ok(code) = crate::annotation_layout::masked_source(path, source) else {
            return Vec::new();
        };
        Self::declared_fields(extension, &code, class)
            .into_iter()
            .map(|field| (field.name, field.type_name))
            .collect()
    }

    fn injected(
        field: &DeclaredField,
        injectable: &BTreeSet<String>,
        local_classes: &BTreeSet<String>,
    ) -> bool {
        field.marked
            || injectable.contains(&field.name)
            || field.name == crate::companion::lower_first(Self::PUBLISHER)
            || local_classes
                .iter()
                .any(|class| crate::companion::lower_first(class) == field.name)
            || field
                .type_name
                .split(|character: char| !Self::is_word_character(character))
                .any(|word| {
                    word == Self::PUBLISHER
                        || injectable.contains(word)
                        || local_classes.contains(word)
                })
    }

    fn ruby_fields(code: &str) -> Vec<DeclaredField> {
        crate::ruby_source::RubyMembers::read(code)
            .fields
            .iter()
            .map(|name| DeclaredField {
                name: crate::companion::camel_case(name),
                type_name: String::new(),
                marked: false,
            })
            .collect()
    }

    fn fsharp_fields(path: &Path, source: &str, class: &str) -> Vec<DeclaredField> {
        crate::fsharp_source::FsharpConstructorProps::read(path, source, class)
            .into_iter()
            .map(|prop| DeclaredField {
                name: prop.name,
                type_name: prop.type_name,
                marked: false,
            })
            .collect()
    }

    fn lua_fields(source: &str, class: &str) -> Vec<DeclaredField> {
        crate::lua_source::LuaFields::without_values(source, class)
            .into_iter()
            .map(|(name, type_name, marked)| DeclaredField {
                name: crate::companion::camel_case(&name),
                type_name,
                marked,
            })
            .collect()
    }

    fn go_fields(source: &str, class: &str) -> Vec<DeclaredField> {
        let Some((_, body)) = Self::class_body(source, class) else {
            return Vec::new();
        };
        let mut marked = false;
        let mut fields = Vec::new();
        for line in body.lines().map(str::trim) {
            let comment = line.trim_start_matches('/').trim();
            if matches!(comment, "@Required" | "@Required()") {
                marked = true;
                continue;
            }
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let mut words = line.split_whitespace();
            if let (true, Some(name), Some(type_name)) = (marked, words.next(), words.next()) {
                fields.push(DeclaredField {
                    name: crate::companion::lower_first(name),
                    type_name: type_name.to_owned(),
                    marked: false,
                });
            }
            marked = false;
        }
        fields
    }

    fn rust_fields(code: &str, class: &str) -> Vec<DeclaredField> {
        let Some((declared_at, body)) = Self::class_body(code, class) else {
            return Vec::new();
        };
        if Self::rust_has_default(code, class, declared_at) {
            return Vec::new();
        }
        let mut fields = Vec::new();
        let mut pending_marker = false;
        for piece in Self::pieces(body, true) {
            let (declaration, marked) = Self::strip_markers(piece);
            if declaration.is_empty() {
                pending_marker |= marked;
                continue;
            }
            let field = declaration.split_once(':').and_then(|(prefix, type_name)| {
                let name = prefix.split_whitespace().last()?;
                Self::is_name(name).then(|| DeclaredField {
                    name: crate::companion::camel_case(name),
                    type_name: type_name.trim().to_owned(),
                    marked: marked || pending_marker,
                })
            });
            fields.extend(field);
            pending_marker = false;
        }
        fields
    }

    pub(crate) fn rust_has_default(code: &str, class: &str, declared_at: usize) -> bool {
        let attributes = code[..declared_at]
            .lines()
            .rev()
            .skip(1)
            .take_while(|line| {
                let line = line.trim();
                line.is_empty() || line.starts_with("#[")
            })
            .collect::<String>();
        let derives_default = attributes.contains("derive(") && attributes.contains("Default");
        let spaced = code.split_whitespace().collect::<Vec<_>>().join(" ");
        derives_default || spaced.contains(&format!("impl Default for {class} "))
    }

    fn declared_fields(extension: &str, code: &str, class: &str) -> Vec<DeclaredField> {
        let Some((_, body)) = Self::class_body(code, class) else {
            return Vec::new();
        };
        let mut fields = Vec::new();
        let mut pending_marker = false;
        let mut pending_needs = false;
        for piece in Self::pieces(body, false) {
            let (declaration, marked) = Self::strip_markers(piece);
            let prefix = &piece[..piece.len() - declaration.len()];
            pending_needs |= Self::java_needs_marker(extension, prefix);
            if declaration.is_empty() {
                pending_marker |= marked;
                continue;
            }
            if let Some(mut field) = Self::required_field(extension, declaration, pending_needs) {
                field.marked = marked || pending_marker;
                fields.push(field);
            }
            pending_marker = false;
            pending_needs = false;
        }
        fields
    }

    fn java_needs_marker(extension: &str, prefix: &str) -> bool {
        extension == "java" && prefix.split('@').skip(1).any(Self::is_needs_marker)
    }

    fn is_needs_marker(marker: &str) -> bool {
        let name = marker
            .split(Self::ends_java_marker)
            .next()
            .unwrap_or_default();
        name.rsplit('.').next() == Some("Needs")
    }

    const fn ends_java_marker(character: char) -> bool {
        !matches!(character, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '.')
    }

    fn required_field(extension: &str, piece: &str, needs: bool) -> Option<DeclaredField> {
        let assignment = Self::assignment(piece);
        let initialized = assignment.is_some() || piece.split_whitespace().any(|word| word == "by");
        let mut declaration = piece[..assignment.unwrap_or(piece.len())].trim();
        if extension == "cs"
            && !initialized
            && let Some(open) = declaration.find('{')
        {
            declaration = declaration[..open].trim();
        }
        if declaration.contains(['(', '{', '}']) || declaration.contains("=>") {
            return None;
        }
        let words = declaration.split_whitespace().collect::<Vec<_>>();
        let kotlin_object = |word: &&str| *word == "object" && extension != "kt";
        if words
            .iter()
            .any(|word| Self::NOT_FIELD_WORDS.contains(word) && !kotlin_object(word))
        {
            return None;
        }
        let has = |modifier: &str| words.contains(&modifier);
        let (name, type_name) = match extension {
            "js" => match words.as_slice() {
                [name] => ((*name).to_owned(), String::new()),
                _ => return None,
            },
            "ts" | "kt" | "swift" => {
                let (prefix, type_name) = declaration.split_once(':')?;
                let name = prefix.split_whitespace().last()?;
                if name.ends_with('?') {
                    return None;
                }
                (
                    name.trim_end_matches('!').to_owned(),
                    type_name.trim().to_owned(),
                )
            }
            "dart" | "cs" | "java" => {
                let (name, before) = words.split_last()?;
                ((*name).to_owned(), before.join(" "))
            }
            _ => return None,
        };
        let required = match extension {
            "js" | "ts" => !initialized,
            "dart" => !initialized && (has("final") || has("late")),
            "cs" => has("required") || (!initialized && has("readonly")),
            "kt" => !initialized && (has("val") || has("lateinit")),
            "java" => !initialized && (needs || !has("private")) && words.len() > 1,
            "swift" => {
                !initialized
                    && !has("lazy")
                    && (has("let") || (has("var") && !type_name.ends_with(['?', '!'])))
            }
            _ => false,
        };
        (required && Self::is_name(&name)).then_some(DeclaredField {
            name,
            type_name,
            marked: false,
        })
    }

    fn class_body<'a>(code: &'a str, class: &str) -> Option<(usize, &'a str)> {
        let mut search = 0;
        while let Some(found) = code[search..].find(class) {
            let at = search + found;
            search = at + class.len();
            let keyword = code[..at]
                .trim_end()
                .rsplit(|character: char| !Self::is_word_character(character))
                .next()
                .unwrap_or("");
            let whole_word = code[search..]
                .chars()
                .next()
                .is_none_or(|character| !Self::is_word_character(character));
            if !whole_word || !matches!(keyword, "class" | "struct" | "type") {
                continue;
            }
            let open = search + code[search..].find(['{', ';'])?;
            if code.as_bytes()[open] == b';' {
                return None;
            }
            let close = Self::closing_brace(code, open)?;
            return Some((at, &code[open + 1..close]));
        }
        None
    }

    fn closing_brace(code: &str, open: usize) -> Option<usize> {
        let mut depth = 0_usize;
        for (at, byte) in code.bytes().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
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

    fn pieces(body: &str, rust: bool) -> Vec<&str> {
        let bytes = body.as_bytes();
        let mut pieces = Vec::new();
        let mut depth = 0_usize;
        let mut start = 0;
        for (at, byte) in bytes.iter().enumerate() {
            let arrow = at > 0 && bytes[at - 1] == b'-';
            match byte {
                b'{' | b'(' | b'[' => depth += 1,
                b'<' if rust => depth += 1,
                b'>' if rust && !arrow => depth = depth.saturating_sub(1),
                b'}' | b')' | b']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && *byte == b'}' {
                        pieces.push(&body[start..=at]);
                        start = at + 1;
                    }
                }
                b',' if rust && depth == 0 => {
                    pieces.push(&body[start..at]);
                    start = at + 1;
                }
                b';' | b'\n' if depth == 0 => {
                    pieces.push(&body[start..at]);
                    start = at + 1;
                }
                _ => {}
            }
        }
        pieces.push(&body[start..]);
        pieces
            .into_iter()
            .map(str::trim)
            .filter(|piece| !piece.is_empty())
            .collect()
    }

    fn strip_markers(piece: &str) -> (&str, bool) {
        let mut rest = piece.trim_start();
        let mut marked = false;
        loop {
            if let Some(after) = rest
                .strip_prefix('#')
                .filter(|after| after.starts_with('['))
            {
                let Some(close) = Self::closing(after, b'[', b']') else {
                    return (rest, marked);
                };
                marked |= Self::fills_field(&after[1..close]);
                rest = after[close + 1..].trim_start();
            } else if rest.starts_with('[') {
                let Some(close) = Self::closing(rest, b'[', b']') else {
                    return (rest, marked);
                };
                marked |= Self::fills_field(&rest[1..close]);
                rest = rest[close + 1..].trim_start();
            } else if let Some(after) = rest.strip_prefix('@') {
                let name_end = after
                    .find(|character: char| !Self::is_word_character(character) && character != '.')
                    .unwrap_or(after.len());
                marked |= Self::fills_field(&after[..name_end]);
                rest = &after[name_end..];
                if rest.starts_with('(') {
                    let Some(close) = Self::closing(rest, b'(', b')') else {
                        return (rest, marked);
                    };
                    rest = &rest[close + 1..];
                }
                rest = rest.trim_start();
            } else {
                return (rest, marked);
            }
        }
    }

    fn fills_field(markers: &str) -> bool {
        markers.split(',').any(|marker| {
            let name = marker
                .trim_start()
                .split(Self::ends_java_marker)
                .next()
                .unwrap_or_default();
            let last = name.rsplit('.').next().unwrap_or_default();
            Self::FILLED_FIELD_MARKERS.contains(&last)
        })
    }

    fn closing(text: &str, open: u8, close: u8) -> Option<usize> {
        let mut depth = 0_usize;
        for (at, byte) in text.bytes().enumerate() {
            if byte == open {
                depth += 1;
            } else if byte == close {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(at);
                }
            }
        }
        None
    }

    fn assignment(code: &str) -> Option<usize> {
        let bytes = code.as_bytes();
        bytes
            .iter()
            .enumerate()
            .find_map(|(at, _)| Self::assignment_at(bytes, at))
    }

    fn assignment_at(bytes: &[u8], at: usize) -> Option<usize> {
        let previous = at.checked_sub(1).and_then(|before| bytes.get(before));
        match (bytes[at], bytes.get(at + 1), previous) {
            (b'=', Some(b'>' | b'='), _) | (b'=', _, Some(b'=' | b'!' | b'<' | b'>')) => None,
            (b'=', _, _) => Some(at),
            _ => None,
        }
    }

    fn is_name(name: &str) -> bool {
        Self::starts_name(name) && name.chars().all(Self::is_word_character)
    }

    fn starts_name(name: &str) -> bool {
        name.chars().next().is_some_and(Self::starts_name_character)
    }

    fn starts_name_character(character: char) -> bool {
        character.is_alphabetic() || character == '_'
    }

    const fn is_word_character(character: char) -> bool {
        character.is_ascii_alphanumeric() || character == '_'
    }
}

#[cfg(test)]
mod tests {
    use super::RequiredFields;
    use std::collections::BTreeSet;
    use std::path::Path;

    fn props(file: &str, source: &str, class: &str) -> Vec<String> {
        let injectable = BTreeSet::from([String::from("TallyService"), String::from("tally")]);
        RequiredFields::props(
            &Path::new("client/services").join(file),
            source,
            class,
            &injectable,
        )
    }

    #[test]
    fn assignment_keeps_operator_boundaries_and_first_initializer_offset() {
        for source in ["", "plain", "==", "===", "!=", "<=", ">=", "=>"] {
            assert_eq!(RequiredFields::assignment(source), None, "{source}");
        }
        for (source, at) in [("=", 0), ("value = 1", 6), ("=> = 1", 3), ("é = 1", 3)] {
            assert_eq!(RequiredFields::assignment(source), Some(at), "{source}");
        }
        for operator in [
            "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "??=", "&&=", "||=",
        ] {
            let source = format!("value {operator} 1");
            assert_eq!(
                RequiredFields::assignment(&source),
                source.find('='),
                "{source}"
            );
        }
        for operator in ["=>", "==", "===", "!=", "<=", ">="] {
            let source = format!("{operator} next = 1");
            assert_eq!(
                RequiredFields::assignment(&source),
                Some(operator.len() + 6),
                "{source}"
            );
        }
    }

    #[test]
    fn assignment_refactor_preserves_required_props_and_injected_fields() {
        let javascript = "@Service\n@NeedsAll\nexport default class BoundaryService {\n route\n tally\n equal = left === right\n unequal = left != right\n lower = left <= right\n greater = left >= right\n arrow = value => value\n quoted = 'left = right => next'\n}\n";
        let typescript = "@Service\n@NeedsAll\nexport default class BoundaryService {\n route: string\n tally: TallyService\n equal: boolean = left === right\n unequal: boolean = left != right\n lower: boolean = left <= right\n greater: boolean = left >= right\n arrow = value => value\n quoted: string = 'left = right => next'\n}\n";
        let injectable = BTreeSet::from([String::from("TallyService"), String::from("tally")]);
        for (extension, source) in [("js", javascript), ("ts", typescript)] {
            let path = Path::new("client/services").join(format!("BoundaryService.{extension}"));
            assert_eq!(
                RequiredFields::props(&path, source, "BoundaryService", &injectable),
                ["route"],
                "{extension}"
            );
            assert_eq!(
                RequiredFields::injected_fields(&path, source, "BoundaryService", &injectable),
                ["tally"],
                "{extension}"
            );
        }
    }

    #[test]
    fn javascript_and_typescript_fields_without_a_value_are_required() {
        let source = "@Service\n@NeedsAll\nexport default class SnippetService {\n  files\n  axis\n  tally\n  applicationEventPublisher\n  helper\n  platform = storedPlatform()\n  companion = ''\n  static shared\n  spans() {\n    return this.axis !== 'language'\n  }\n}\nclass Helper {}\n";
        assert_eq!(
            props("SnippetService.js", source, "SnippetService"),
            ["files", "axis"]
        );
        let injectable = BTreeSet::from([String::from("TallyService"), String::from("tally")]);
        let injected = RequiredFields::injected_fields(
            Path::new("client/services/SnippetService.js"),
            source,
            "SnippetService",
            &injectable,
        );
        assert_eq!(injected, ["tally", "applicationEventPublisher", "helper"]);
        let typed = "@Service\n@NeedsAll\nexport default class DrawerService {\n  open!: boolean\n  label?: string\n  readonly title: string\n  count: number = 0\n  tally: TallyService\n}\n";
        assert_eq!(
            props("DrawerService.ts", typed, "DrawerService"),
            ["open", "title"]
        );
    }

    #[test]
    fn required_prop_names_keep_the_existing_ascii_boundary_and_dependency_selection() {
        let javascript = "@Service\n@NeedsAll\nexport default class NamesService {\n  _port\n  value7\n  port_name\n  tally\n  $cache\n  café\n  initialized = 0\n}\n";
        let typescript = "@Service\n@NeedsAll\nexport default class NamesService {\n  _port: string\n  value7: number\n  port_name: string\n  tally: TallyService\n  $cache: string\n  café: string\n  initialized = 0\n}\n";
        for (extension, source) in [("js", javascript), ("ts", typescript)] {
            assert_eq!(
                props(&format!("NamesService.{extension}"), source, "NamesService"),
                ["_port", "value7", "port_name"],
                "{extension}"
            );
        }
    }

    #[test]
    fn typed_languages_follow_required_args_constructor_rules() {
        let dart = "@Service\n@NeedsAll\nclass PagerService {\n  final String? path;\n  final Map<String, Object?>? docs;\n  late String label;\n  String? note;\n  final int size = 2;\n  final TallyService tally;\n  final Helper helper;\n  int count() => 1;\n}\n\nclass Helper {}\n";
        assert_eq!(
            props("PagerService.dart", dart, "PagerService"),
            ["path", "docs", "label"]
        );
        let csharp = "[Service]\n[NeedsAll]\npublic class LinksService\n{\n    public readonly string Path;\n    public required string Title { get; set; }\n    public object Docs;\n    public readonly int Size = 2;\n    public object Topics()\n    {\n        return Docs;\n    }\n}\n";
        assert_eq!(
            props("LinksService.cs", csharp, "LinksService"),
            ["path", "title"]
        );
        let kotlin = "@Service\n@NeedsAll\nclass TallyViewService {\n    val path: String\n    lateinit var label: String\n    var count: Int = 0\n    val size by lazy { 2 }\n    fun doubled(): Int = count * 2\n}\n";
        assert_eq!(
            props("TallyViewService.kt", kotlin, "TallyViewService"),
            ["path", "label"]
        );
        let java = "@Service\n@NeedsAll\nclass ShelfService {\n    List<String> files;\n    String owner;\n    int count = 0;\n    private String hidden;\n    static String shared;\n    TallyService tally;\n    int doubled() {\n        return count * 2;\n    }\n}\n";
        assert_eq!(
            props("ShelfService.java", java, "ShelfService"),
            ["files", "owner"]
        );
        let swift = "@Service\n@NeedsAll\nclass BadgeService {\n    let path: String\n    var label: String\n    var note: String?\n    var count: Int = 0\n    var doubled: Int { count * 2 }\n    func reset() {}\n}\n";
        assert_eq!(
            props("BadgeService.swift", swift, "BadgeService"),
            ["path", "label"]
        );
    }

    #[test]
    fn java_private_needs_fields_are_injected_while_other_private_fields_stay_hidden() {
        let source = "@Service\n@NeedsAll\nclass ShelfService {\n @Needs\n private final TallyService tally;\n @tachyon.Needs private final TallyService second;\n @Deprecated\n private String hidden;\n private TallyService unmarked;\n @Needs private TallyService initialized = null;\n String title;\n}\n";
        let path = Path::new("client/services/ShelfService.java");
        let injectable = BTreeSet::from([String::from("TallyService")]);
        assert_eq!(
            RequiredFields::props(path, source, "ShelfService", &injectable),
            ["title"]
        );
        assert_eq!(
            RequiredFields::injected_fields(path, source, "ShelfService", &injectable),
            ["tally", "second"]
        );
    }

    #[test]
    fn only_markers_that_fill_a_field_make_it_injected() {
        let injectable = BTreeSet::new();
        let java = "@Service\n@NeedsAll\nclass NoteService {\n    @JsonProperty(\"note\")\n    String note;\n    @Deprecated String title;\n    @Needs\n    String helper;\n    @tachyon.Named(\"main\") String named;\n}\n";
        let path = Path::new("client/services/NoteService.java");
        assert_eq!(
            RequiredFields::props(path, java, "NoteService", &injectable),
            ["note", "title"]
        );
        assert_eq!(
            RequiredFields::injected_fields(path, java, "NoteService", &injectable),
            ["helper", "named"]
        );
        let csharp = "[Service]\n[NeedsAll]\npublic class NoteService\n{\n    [JsonPropertyName(\"note\")]\n    public readonly string Note;\n    [Needs]\n    public readonly string Helper;\n}\n";
        let path = Path::new("client/services/NoteService.cs");
        assert_eq!(
            RequiredFields::props(path, csharp, "NoteService", &injectable),
            ["note"]
        );
        let swift = "@Service\n@NeedsAll\nclass NoteService {\n    @Published var note: String\n    @LocalState var count: Int\n}\n";
        let path = Path::new("client/services/NoteService.swift");
        assert_eq!(
            RequiredFields::props(path, swift, "NoteService", &injectable),
            ["note"]
        );
    }

    #[test]
    fn csharp_required_members_are_lower_camel_props() {
        let csharp = "[Service]\n[NeedsAll]\npublic class DocumentationService\n{\n    public string Source = \"website/content/docs/page.json\";\n    public required object Docs;\n    public required object Features;\n    public object Contents = new List<object>\n    {\n        new Dictionary<string, object> { [\"heading\"] = \"Guides\", [\"anchor\"] = \"#guides\" },\n    };\n    public object Topics()\n    {\n        var docs = Docs as IDictionary<string, object>;\n        return docs;\n    }\n}\n";
        assert_eq!(
            props("DocumentationService.cs", csharp, "DocumentationService"),
            ["docs", "features"]
        );
    }

    #[test]
    fn every_tac_language_of_needs_all_has_a_required_field_rule() {
        let rule = crate::annotation_rules::AnnotationRules::find("NeedsAll");
        let tac = rule.map(|rule| rule.tac).unwrap_or_default();
        assert!(!tac.is_empty());
        for language in tac {
            assert!(RequiredFields::LANGUAGES.contains(language), "{language}");
        }
        assert!(rule.is_some_and(|rule| rule.explained().contains("no initial value")));
    }

    #[test]
    fn rust_go_and_ruby_follow_their_own_initial_values() {
        let defaulted = "#[Service]\n#[NeedsAll]\n#[derive(Default)]\nstruct TocService {\n    entries: TacValue,\n}\n";
        assert!(props("TocService.rs", defaulted, "TocService").is_empty());
        let plain = "#[Service]\n#[NeedsAll]\nstruct TocService {\n    page_path: String,\n    entries: HashMap<String, TacValue>,\n    #[Needs]\n    helper: Helper,\n}\n";
        assert_eq!(
            props("TocService.rs", plain, "TocService"),
            ["pagePath", "entries"]
        );
        let go = "// @Service\n// @NeedsAll\ntype CatalogService struct {\n\tSource string\n}\n";
        assert!(props("CatalogService.go", go, "CatalogService").is_empty());
        let marked = "// @Service\n// @NeedsAll\ntype CatalogService struct {\n\tSource string\n\t// @Required\n\tFeatures Catalogue\n}\n";
        assert_eq!(
            props("CatalogService.go", marked, "CatalogService"),
            ["features"]
        );
        let fsharp = "[<Service>]\n[<NeedsAll>]\ntype ShelfService(files: string list, owner: string, tally: TallyService) =\n    member this.Owner : string = owner\n";
        assert_eq!(
            props("ShelfService.fs", fsharp, "ShelfService"),
            ["files", "owner"]
        );
        let ruby = "#[Service]\n#[NeedsAll]\nclass CounterService\n  attr_reader :start_count, :label\n\n  def increment\n    @start_count += 1\n  end\nend\n";
        assert_eq!(
            props("CounterService.rb", ruby, "CounterService"),
            ["startCount", "label"]
        );
    }
}
