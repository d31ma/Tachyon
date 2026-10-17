use super::injected_field::InjectedField;
use std::path::Path;

pub(crate) struct InjectedContract<'a> {
    language: &'static str,
    marker: &'static str,
    filled: &'static str,
    contracts: &'a [String],
    remotes: &'a [String],
}

impl<'a> InjectedContract<'a> {
    pub(crate) const GO_START: &'static str = "tachyonInject";
    const FORMS: [(&'static str, &'static str, &'static str); 7] = [
        (
            "kt",
            "@Needs",
            "{indent}private val {name}: {type} = {type}()",
        ),
        (
            "swift",
            "@Needs",
            "{indent}private let {name}: {type} = {type}()",
        ),
        (
            "java",
            "@Needs",
            "{indent}private final {type} {name} = new {type}();",
        ),
        (
            "cs",
            "[Needs]",
            "{indent}private readonly {type} {name} = new {type}();",
        ),
        ("rs", "#[Needs]", "{indent}{name}: std::sync::Arc<{type}>,"),
        ("go", "// @Needs", "{indent}{name} *{type}"),
        (
            "rb",
            "#[Needs]",
            "{indent}private def {name}\n{indent}  @{name} ||= {type}.new\n{indent}end",
        ),
    ];
    pub(super) const MODIFIERS: [&'static str; 9] = [
        "private",
        "public",
        "protected",
        "internal",
        "fileprivate",
        "final",
        "readonly",
        "lateinit",
        "pub",
    ];
    pub(super) const KEYWORDS: [&'static str; 3] = ["val", "var", "let"];
    const EMPTY_ARGUMENTS: &'static str = "()";
    const INJECTED_FOLDERS: [&'static str; 2] = ["client/stores", "client/converters"];
    const REMOTES: &'static str = "client/remotes";
    const HELPERS: &'static str = "client/helpers";
    const KOTLIN: &'static str = "kt";
    const GO: &'static str = "go";
    const REMOTE_LANGUAGES: [&'static str; 5] = [Self::GO, "kt", "swift", "java", Self::RUBY];
    const RUBY: &'static str = "rb";
    const RUBY_REMOTE: &'static str = "{indent}private def {name}\n{indent}  @{name} ||= {type}.new(@tachyon_test_context)\n{indent}end";
    const RUBY_COMMENT: &'static str = "# ";
    const GO_METHOD: &'static str =
        "\nfunc (service *{owner}) {start}() error {\n{assignments}\treturn nil\n}\n";
    const GO_ASSIGNMENT: &'static str = "\t{name}, err := New{type}()\n\tif err != nil {\n\t\treturn err\n\t}\n\tservice.{name} = {name}\n";

    pub(crate) fn injected_names<'p>(paths: impl Iterator<Item = &'p Path>) -> Vec<String> {
        paths
            .filter(|path| Self::injected(path))
            .filter_map(|path| path.file_stem()?.to_str().map(str::to_owned))
            .collect()
    }

    pub(crate) fn remote_names<'p>(paths: impl Iterator<Item = &'p Path>) -> Vec<String> {
        paths
            .filter(|path| Self::injected_remote(path))
            .filter_map(|path| path.file_stem()?.to_str().map(str::to_owned))
            .collect()
    }

    pub(crate) fn filled(
        path: &Path,
        source: &str,
        contracts: &'a [String],
        remotes: &'a [String],
    ) -> Option<String> {
        let language = path.extension()?.to_str()?;
        let owner = path.file_stem()?.to_str()?;
        let (form, marker, filled) = Self::FORMS.iter().find(|(form, _, _)| *form == language)?;
        let injected = Self {
            language: form,
            marker,
            filled,
            contracts,
            remotes,
        };
        injected.rewritten(source, owner)
    }

    fn injected(path: &Path) -> bool {
        let in_folder = Self::INJECTED_FOLDERS
            .iter()
            .any(|folder| path.parent() == Some(Path::new(folder)));
        [
            in_folder,
            Self::injected_remote(path),
            Self::kotlin_helper(path),
        ]
        .contains(&true)
    }

    pub(crate) fn kotlin_helper(path: &Path) -> bool {
        [
            path.starts_with(Self::HELPERS),
            path.extension().and_then(|extension| extension.to_str()) == Some(Self::KOTLIN),
        ] == [true, true]
    }

    fn injected_remote(path: &Path) -> bool {
        [
            path.parent() == Some(Path::new(Self::REMOTES)),
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| Self::REMOTE_LANGUAGES.contains(&extension)),
        ] == [true, true]
    }

    fn rewritten(&self, source: &str, owner: &str) -> Option<String> {
        let lines: Vec<&str> = source.split('\n').collect();
        let edits: Vec<(usize, usize, InjectedField)> = (0..lines.len())
            .filter(|at| self.is_marker(lines[*at]))
            .filter_map(|at| self.edit(&lines, at))
            .collect();
        let written: Vec<String> = lines
            .iter()
            .enumerate()
            .map(|(at, line)| self.written_line(at, line, &edits))
            .collect();
        let fields: Vec<&InjectedField> = edits.iter().map(|(_, _, field)| field).collect();
        let start = self.go_start(owner, &fields);
        (!edits.is_empty()).then(|| format!("{}{start}", written.join("\n")))
    }

    fn written_line(
        &self,
        at: usize,
        line: &str,
        edits: &[(usize, usize, InjectedField)],
    ) -> String {
        let marker = edits
            .iter()
            .any(|(marker_at, _, _)| *marker_at == at)
            .then(String::new);
        let field = edits
            .iter()
            .find(|(_, field_at, _)| *field_at == at)
            .map(|(_, _, field)| field.filled(self.form_of(field)));
        field.or(marker).unwrap_or_else(|| line.to_owned())
    }

    fn form_of(&self, field: &InjectedField) -> &'static str {
        let ruby_remote = [
            self.language == Self::RUBY,
            self.remotes.contains(&field.type_name),
        ] == [true, true];
        [self.filled, Self::RUBY_REMOTE][usize::from(ruby_remote)]
    }

    fn is_marker(&self, line: &str) -> bool {
        let trimmed = line.trim();
        let arguments = trimmed.strip_prefix(self.marker);
        [Some(""), Some(Self::EMPTY_ARGUMENTS)].contains(&arguments)
    }

    fn edit(&self, lines: &[&str], marker_at: usize) -> Option<(usize, usize, InjectedField)> {
        let field_at = (marker_at + 1..lines.len()).find(|at| self.is_field(lines[*at]))?;
        let field = if self.language == Self::RUBY {
            InjectedField::read_ruby(&lines[marker_at + 1..field_at], lines[field_at])?
        } else {
            InjectedField::read(lines[field_at], self.language == Self::GO)?
        };
        self.contracts
            .contains(&field.type_name)
            .then_some((marker_at, field_at, field))
    }

    fn is_field(&self, line: &str) -> bool {
        let trimmed = line.trim();
        let documentation = [
            self.language == Self::RUBY,
            trimmed.starts_with(Self::RUBY_COMMENT),
        ] == [true, true];
        [trimmed.is_empty(), documentation] == [false, false]
    }

    fn go_start(&self, owner: &str, fields: &[&InjectedField]) -> String {
        let assignments: String = fields
            .iter()
            .map(|field| field.filled(Self::GO_ASSIGNMENT))
            .collect();
        let wanted = [self.language == Self::GO, !fields.is_empty()] == [true, true];
        if !wanted {
            return String::new();
        }
        Self::GO_METHOD
            .replace("{owner}", owner)
            .replace("{start}", Self::GO_START)
            .replace("{assignments}", &assignments)
    }
}
