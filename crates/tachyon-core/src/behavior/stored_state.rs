use super::{Input, Marker};
use crate::Failure;
use crate::failure::{diagnostic, source_span};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) struct StoredState;

pub(crate) struct SigilField {
    pub(crate) line_start: usize,
    pub(crate) indent: usize,
    pub(crate) at: usize,
    pub(crate) written: String,
}

impl StoredState {
    const LOCAL: &'static str = "LocalState";
    const SESSION: &'static str = "SessionState";
    const SPELLINGS: [&'static str; 4] =
        ["LocalState", "SessionState", "local_state", "session_state"];
    const SIGIL_LANGUAGES: [&'static str; 3] = ["js", "ts", "dart"];

    pub(crate) fn marks(name: &str) -> bool {
        name == Self::LOCAL || name == Self::SESSION
    }

    pub(crate) fn fields(path: &Path, source: &str) -> Result<BTreeMap<String, String>, Failure> {
        crate::fsharp_source::FsharpView::either(path, source, Self::written_fields)
    }

    fn written_fields(path: &Path, source: &str) -> Result<BTreeMap<String, String>, Failure> {
        let language = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !Self::SPELLINGS
            .iter()
            .any(|spelling| source.contains(spelling))
        {
            return Ok(BTreeMap::new());
        }
        let mask = crate::lexical::code_mask(language, source);
        let input = Input {
            path,
            source,
            code: super::visible_code(source, &mask.code),
        };
        let markers = if matches!(language, "js" | "ts") {
            super::markers(&input)?
        } else {
            super::polyglot::annotations(&input, language, "tac")?
        };
        let mut fields = BTreeMap::new();
        for marker in markers.iter().filter(|marker| Self::marks(&marker.name)) {
            let name = super::validation::field_name(&input, marker, language)?;
            fields.insert(Self::visible_name(language, &name), Self::area(marker));
        }
        Ok(fields)
    }

    fn area(marker: &Marker) -> String {
        if marker.name == Self::LOCAL {
            String::from("local")
        } else {
            String::from("session")
        }
    }

    fn visible_name(language: &str, name: &str) -> String {
        match language {
            "cs" | "go" => crate::companion::lower_first(name),
            "rs" | "rb" | "py" => crate::companion::camel_case(name),
            _ => name.to_owned(),
        }
    }

    pub(crate) fn refuse_sigils(path: &Path, source: &str) -> Result<(), Failure> {
        match Self::sigil_fields(path, source).first() {
            Some(field) => Err(Self::sigil_failure(path, source, field.at, &field.written)),
            None => Ok(()),
        }
    }

    pub(crate) fn sigil_fields(path: &Path, source: &str) -> Vec<SigilField> {
        let language = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let mut fields = Vec::new();
        if !Self::SIGIL_LANGUAGES.contains(&language) || !source.contains('$') {
            return fields;
        }
        let mask = crate::lexical::code_mask(language, source);
        let code = super::visible_code(source, &mask.code);
        let mut depth = 0_usize;
        let mut in_class = false;
        let mut offset = 0;
        for line in code.split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            if depth == 1
                && in_class
                && let Some((at, written)) = Self::sigil_field(line)
            {
                fields.push(SigilField {
                    line_start: start,
                    indent: line.len() - line.trim_start().len(),
                    at: start + at,
                    written: written.to_owned(),
                });
            }
            for byte in line.bytes() {
                match byte {
                    b'{' => {
                        if depth == 0 {
                            in_class = line.split_whitespace().any(|word| word == "class");
                        }
                        depth += 1;
                    }
                    b'}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }
        fields
    }

    pub(crate) fn marker_for(written: &str) -> &'static str {
        if written.starts_with("$$") {
            Self::LOCAL
        } else {
            Self::SESSION
        }
    }

    fn sigil_field(line: &str) -> Option<(usize, &str)> {
        let declaration = line.split(['=', ';', ':']).next().unwrap_or_default();
        if declaration.contains(['(', '{', '}', '.']) {
            return None;
        }
        let word = declaration
            .split_whitespace()
            .last()?
            .trim_end_matches(['!', '?']);
        let bare = word.trim_start_matches('$');
        let sigils = word.len() - bare.len();
        let named = bare
            .chars()
            .next()
            .is_some_and(|first| first.is_alphabetic() || first == '_')
            && bare
                .chars()
                .all(|character| character.is_alphanumeric() || character == '_');
        if !(1..=2).contains(&sigils) || !named {
            return None;
        }
        Some((line.find(word)?, word))
    }

    fn sigil_failure(path: &Path, source: &str, at: usize, written: &str) -> Failure {
        let bare = written.trim_start_matches('$');
        let marker = Self::marker_for(written);
        let portable = path.to_string_lossy().replace('\\', "/");
        let line = source[..at.min(source.len())].matches('\n').count() + 1;
        Failure::one(diagnostic(
            1306,
            format!(
                "The field `{written}` uses a `$` prefix to keep its value, and Tac no longer reads that prefix. Write `@{marker}` on the line above and name the field `{bare}` ({portable}:{line})"
            ),
            Some(String::from(
                "@SessionState keeps a field in sessionStorage (the old `$name`). @LocalState keeps it in localStorage (the old `$$name`). `ty migrate fix --write` converts the fields and the views that read them (ADR 0059).",
            )),
            source_span(&portable, at, (at + written.len()).min(source.len())),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::StoredState;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn fields(file: &str, source: &str) -> BTreeMap<String, String> {
        StoredState::fields(&Path::new("client/services").join(file), source).expect("fields")
    }

    fn stored(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(name, area)| ((*name).to_owned(), (*area).to_owned()))
            .collect()
    }

    #[test]
    fn every_tac_behavior_language_marks_local_and_session_fields() {
        let wanted = stored(&[("draft", "session"), ("theme", "local")]);
        let sources = [
            (
                "EditorService.js",
                "@Service\nexport default class EditorService {\n  @SessionState\n  draft = ''\n  @LocalState\n  theme = 'light'\n}\n",
            ),
            (
                "EditorService.ts",
                "@Service\nexport default class EditorService {\n  @SessionState\n  draft: string = ''\n  @LocalState\n  theme = 'light'\n}\n",
            ),
            (
                "EditorService.dart",
                "@Service\nclass EditorService {\n  @SessionState\n  String draft = '';\n  @LocalState\n  String theme = 'light';\n}\n",
            ),
            (
                "EditorService.kt",
                "class EditorService {\n  @SessionState\n  var draft = \"\"\n  @LocalState\n  var theme: String = \"light\"\n}\n",
            ),
            (
                "EditorService.swift",
                "final class EditorService {\n  @SessionState\n  var draft = \"\"\n  @LocalState\n  var theme: String = \"light\"\n}\n",
            ),
            (
                "EditorService.cs",
                "public sealed class EditorService {\n    [SessionState]\n    public string Draft = \"\";\n    [LocalState]\n    public string theme = \"light\";\n}\n",
            ),
            (
                "EditorService.rs",
                "struct EditorService {\n    #[SessionState]\n    draft: String,\n    #[LocalState]\n    theme: String,\n}\n",
            ),
            (
                "EditorService.go",
                "type EditorService struct {\n\t// @SessionState\n\tDraft string\n\t// @LocalState\n\tTheme string\n}\n",
            ),
            (
                "EditorService.rb",
                "#[Service]\nclass EditorService\n  #[SessionState]\n  attr_accessor :draft\n  #[LocalState]\n  attr_accessor :theme\nend\n",
            ),
            (
                "EditorService.fs",
                "[<Service>]\ntype EditorService() =\n    [<SessionState>]\n    member val Draft = \"\" with get, set\n\n    [<LocalState>]\n    member val theme : string = \"light\" with get, set\n",
            ),
        ];
        for (file, source) in sources {
            assert_eq!(fields(file, source), wanted, "{file}");
        }
    }

    #[test]
    fn python_fields_use_their_lower_camel_page_names() {
        assert_eq!(
            fields(
                "EditorService.py",
                "from typing import Annotated\n\n@service\nclass EditorService:\n    draft: Annotated[str, session_state] = ''\n    dark_mode: Annotated[bool, local_state] = False\n",
            ),
            stored(&[("darkMode", "local"), ("draft", "session")])
        );
    }

    #[test]
    fn ruby_fields_use_their_lower_camel_page_names() {
        assert_eq!(
            fields(
                "EditorService.rb",
                "#[Service]\nclass EditorService\n  #[LocalState]\n  attr_accessor :dark_mode\nend\n",
            ),
            stored(&[("darkMode", "local")])
        );
    }

    #[test]
    fn a_service_without_markers_keeps_nothing() {
        assert!(
            fields(
                "PlainService.js",
                "export default class PlainService { theme = 'x' }\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn dollar_prefixed_fields_are_refused_with_the_marker_to_write() {
        let cases = [
            (
                "EditorService.js",
                "export default class EditorService {\n  $$theme = 'light'\n}\n",
                "@LocalState",
                "`theme`",
            ),
            (
                "EditorService.ts",
                "export default class EditorService {\n  $draft: string = ''\n}\n",
                "@SessionState",
                "`draft`",
            ),
            (
                "EditorService.dart",
                "class EditorService {\n  String $$theme = 'light';\n}\n",
                "@LocalState",
                "`theme`",
            ),
        ];
        for (file, source, marker, name) in cases {
            let failure =
                StoredState::refuse_sigils(&Path::new("client/services").join(file), source)
                    .expect_err("refused");
            let message = &failure.diagnostics()[0].message;
            assert!(
                message.contains(marker) && message.contains(name),
                "{message}"
            );
            assert!(message.contains(":2)"), "{message}");
        }
    }

    #[test]
    fn dollar_names_inside_methods_and_strings_are_not_fields() {
        let source = "export default class EditorService {\n  label = '$$theme'\n  load() {\n    const $el = 1\n    this.$$theme = $el\n  }\n}\n";
        StoredState::refuse_sigils(Path::new("client/services/EditorService.js"), source)
            .expect("plain source");
    }
}
