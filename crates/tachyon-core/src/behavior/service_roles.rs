use super::{Input, Marker};
use crate::Failure;
use std::path::Path;

pub(crate) struct ServiceRoles {
    pub(crate) source: String,
    pub(crate) prototype: bool,
    pub(crate) application: Option<usize>,
    pub(crate) required: Vec<String>,
    explicit_prototype: Option<usize>,
}

impl ServiceRoles {
    const NAMES: [&str; 6] = [
        "Service", "Lifetime", "NeedsAll", "Helper", "Store", "Client",
    ];

    pub(crate) fn read(path: &Path, source: &str) -> Result<Self, Failure> {
        crate::tac_cpp::CppSource::either(
            path,
            source,
            crate::tac_cpp::CppRoles::service_roles,
            |path, source| {
                crate::lua_source::LuaView::either(path, source, |path, source| {
                    Self::scan(path, source, false)
                })
            },
        )
    }

    pub(crate) fn declared(source: &str, prototype: bool, required: Vec<String>) -> Self {
        Self {
            source: source.to_owned(),
            prototype,
            application: None,
            required,
            explicit_prototype: None,
        }
    }

    pub(crate) fn without_roles(path: &Path, source: &str) -> Result<String, Failure> {
        Ok(Self::scan(path, source, true)?.source)
    }

    pub(crate) fn refuse_native_peer_lifetime(path: &Path, source: &str) -> Result<(), Failure> {
        Self::read(path, source)?.require_supported_peer_lifetime(path)
    }

    fn require_supported_peer_lifetime(&self, path: &Path) -> Result<(), Failure> {
        if let Some(at) = self.explicit_prototype {
            return Err(Failure::one(crate::failure::diagnostic(
                1406,
                "Explicit prototype lifetime on a native Rust dependency is not supported yet.",
                Some(String::from(
                    "The current typed native graph shares Arc providers within one owner. Prototype overrides need separate provider instances and lifecycle registration; remove the override only if owner-local sharing is the intended contract.",
                )),
                crate::failure::source_span(&path.to_string_lossy(), at, at + "Lifetime".len()),
            )));
        }
        Ok(())
    }

    fn scan(path: &Path, source: &str, keeps_needs_all: bool) -> Result<Self, Failure> {
        let has_http_client = super::HttpClientSource::rewrite(path, source)?.is_some();
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("");
        let mask = crate::lexical::code_mask(extension, source);
        let input = Input {
            path,
            source,
            code: String::new(),
        };
        let mut roles = Self {
            source: source.to_owned(),
            prototype: false,
            application: None,
            required: Vec::new(),
            explicit_prototype: None,
        };
        let mut lifetime_is_prototype = None;
        let mut needs_all = false;
        let mut offset = 0;
        for line in source.split_inclusive('\n') {
            let mut start = offset + line.len() - line.trim_start().len();
            offset += line.len();
            let mut text = line.trim();
            if extension == "go" && text.starts_with("// @") {
                start += 3;
                text = &text[3..];
            }
            if mask.code.get(start) != Some(&true) {
                continue;
            }
            let Some(marker) = Self::marker(extension, text, start) else {
                continue;
            };
            match marker.name.as_str() {
                "Lifetime" => {
                    let scope = super::scope(&input, &marker)?;
                    lifetime_is_prototype = Some(scope == "prototype");
                    roles.explicit_prototype = (scope == "prototype").then_some(marker.span.start);
                    roles.application = (scope == "application").then_some(marker.span.start);
                }
                "NeedsAll" => {
                    needs_all = true;
                    roles.required = Self::required_names(&input, &marker)?;
                    if keeps_needs_all {
                        continue;
                    }
                }
                "Store" if !marker.arguments.trim().is_empty() => continue,
                "Client" => {}
                "Helper" => {
                    let refusal = crate::annotation_rules::AnnotationRules::unavailable(
                        "Helper", extension, "tac",
                    );
                    if let Some(message) = refusal {
                        return Err(input.failure(marker.span.start, message));
                    }
                    super::flag_without_arguments(&input, &marker)?;
                    if super::HelperRole::keeps_tac_marker(path) {
                        super::HelperRole::refuse_in_tac(path, source)?;
                        continue;
                    }
                }
                _ => {
                    super::flag_without_arguments(&input, &marker)?;
                }
            }
            if marker.name == "Service" && has_http_client {
                continue;
            }
            let blank = " ".repeat(marker.span.len());
            roles.source.replace_range(marker.span, &blank);
        }
        roles.prototype = lifetime_is_prototype.unwrap_or(needs_all);
        Ok(roles)
    }

    fn marker(extension: &str, line: &str, start: usize) -> Option<Marker> {
        let text = Self::marker_text(extension, line)?;
        for name in Self::NAMES {
            let Some(rest) = Self::role_rest(extension, text, name) else {
                continue;
            };
            let arguments = if rest.is_empty() {
                ""
            } else {
                rest.strip_prefix('(')?.strip_suffix(')')?
            };
            return Some(Marker {
                name: name.to_owned(),
                arguments: arguments.to_owned(),
                span: start..start + line.len(),
                target: start + line.len(),
                parameter: false,
            });
        }
        None
    }

    fn role_rest<'a>(extension: &str, text: &'a str, name: &str) -> Option<&'a str> {
        if extension != "py" {
            return text.strip_prefix(name);
        }
        let written = text.split('(').next().unwrap_or(text);
        let normalized = written.replace('_', "");
        normalized
            .eq_ignore_ascii_case(name)
            .then(|| &text[written.len()..])
    }

    fn marker_text<'a>(extension: &str, line: &'a str) -> Option<&'a str> {
        match extension {
            "rs" | "rb" => line.strip_prefix("#[")?.strip_suffix(']'),
            "cs" => line.strip_prefix('[')?.strip_suffix(']'),
            _ => line.strip_prefix('@'),
        }
    }

    fn required_names(input: &Input<'_>, marker: &Marker) -> Result<Vec<String>, Failure> {
        if !marker.arguments.trim().is_empty() {
            return Err(input.failure(
                marker.span.start,
                crate::annotation_rules::AnnotationRules::no_arguments("NeedsAll"),
            ));
        }
        Ok(vec![String::from("*")])
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::ServiceRoles;
    use std::path::Path;

    fn read(file: &str, source: &str) -> ServiceRoles {
        ServiceRoles::read(Path::new(file), source).expect("service roles")
    }

    fn refusal(file: &str, source: &str) -> (String, u64) {
        let Err(failure) = ServiceRoles::read(Path::new(file), source) else {
            panic!("{file} should be refused");
        };
        let diagnostic = &failure.diagnostics()[0];
        (diagnostic.message.clone(), diagnostic.spans[0].start)
    }

    #[test]
    fn ruby_markers_are_read_and_blanked_in_place() {
        let source = "#[Service]\n#[NeedsAll]\nclass TocService\n  attr_reader :entries\nend\n";
        let roles = read("TocService.rb", source);
        assert!(roles.prototype);
        assert_eq!(roles.required, ["*"]);
        assert_eq!(roles.source.len(), source.len());
        assert!(roles.source.trim_start().starts_with("class TocService"));
    }

    #[test]
    fn rust_markers_are_read_and_blanked_in_place() {
        let source = "#[Service]\n#[Lifetime({\"value\":\"prototype\"})]\n#[NeedsAll()]\nstruct TocService {}\n";
        let roles = read("TocService.rs", source);
        assert!(roles.prototype);
        assert_eq!(roles.required, ["*"]);
        assert_eq!(roles.source.len(), source.len());
        assert_eq!(roles.source.trim_start(), "struct TocService {}\n");
    }

    #[test]
    fn needs_all_alone_is_prototype_and_accepts_every_property() {
        let roles = read(
            "TocService.rs",
            "#[Service]\n#[NeedsAll]\nstruct TocService {}\n",
        );
        assert!(roles.prototype);
        assert_eq!(roles.required, ["*"]);
        assert!(!read("TocService.rs", "struct TocService {}\n").prototype);
    }

    #[test]
    fn a_singleton_lifetime_wins_over_needs_all() {
        let source = "[NeedsAll]\n[Lifetime({\"value\":\"singleton\"})]\npublic class A {}\n";
        assert!(!read("AService.cs", source).prototype);
    }

    #[test]
    fn csharp_markers_use_square_brackets() {
        let source = "[Service()]\n[NeedsAll]\npublic class LinksService {}\n";
        let roles = read("LinksService.cs", source);
        assert_eq!(roles.required, ["*"]);
        assert_eq!(roles.source.trim_start(), "public class LinksService {}\n");
    }

    #[test]
    fn swift_kotlin_and_dart_markers_use_the_at_sign() {
        for file in ["TallyService.swift", "TallyService.kt", "TallyService.dart"] {
            let source = "@Service\n@Lifetime({\"value\":\"prototype\"})\n@NeedsAll\nclass TallyService {}\n";
            let roles = read(file, source);
            assert!(roles.prototype, "{file}");
            assert_eq!(
                roles.source.trim_start(),
                "class TallyService {}\n",
                "{file}"
            );
        }
    }

    #[test]
    fn a_marker_in_another_language_spelling_is_left_alone() {
        let source = "@Service\nstruct TallyService {}\n";
        assert_eq!(read("TallyService.rs", source).source, source);
        let source = "#[Service]\nclass TallyService {}\n";
        assert_eq!(read("TallyService.kt", source).source, source);
    }

    #[test]
    fn a_service_that_is_also_an_http_client_is_refused() {
        for (extension, service, client, declaration) in [
            (
                "rs",
                "#[Service]",
                "#[HTTPClient]",
                "struct CatalogService {}",
            ),
            (
                "go",
                "// @Service",
                "// @HTTPClient",
                "type CatalogService struct {}",
            ),
            ("cs", "[Service]", "[HTTPClient]", "class CatalogService {}"),
            (
                "swift",
                "@Service",
                "@HTTPClient",
                "class CatalogService {}",
            ),
            ("kt", "@Service", "@HTTPClient", "class CatalogService {}"),
            ("dart", "@Service", "@HTTPClient", "class CatalogService {}"),
        ] {
            let source = format!("{service}\n{client}\n{declaration}\n");
            let relative = format!("client/services/CatalogService.{extension}");
            let roles = read(&relative, &source);
            let failure = crate::annotation_layout::validate(Path::new(&relative), &roles.source)
                .expect_err("a service is not also an HTTPClient");
            assert!(
                failure.to_string().contains("is its own role"),
                "{extension}: {failure}"
            );
        }
    }

    #[test]
    fn a_marker_inside_a_comment_is_left_alone() {
        let source = "/*\n@Lifetime(\"often\")\n*/\nclass TallyService {}\n";
        assert_eq!(read("TallyService.swift", source).source, source);
    }

    #[test]
    fn a_bad_lifetime_is_refused_at_its_marker_in_every_spelling() {
        for (file, service, lifetime) in [
            (
                "TallyService.rs",
                "#[Service]",
                "#[Lifetime({\"value\":\"often\"})]",
            ),
            (
                "TallyService.go",
                "// @Service",
                "// @Lifetime({\"value\":\"often\"})",
            ),
            (
                "TallyService.cs",
                "[Service]",
                "[Lifetime({\"value\":\"often\"})]",
            ),
            (
                "TallyService.swift",
                "@Service",
                "@Lifetime({\"value\":\"often\"})",
            ),
            (
                "TallyService.kt",
                "@Service",
                "@Lifetime({\"value\":\"often\"})",
            ),
            (
                "TallyService.dart",
                "@Service()",
                "@Lifetime({\"value\":\"often\"})",
            ),
        ] {
            let source = format!("\n{service}\n  {lifetime}\nclass TallyService {{}}\n");
            let expected = source.find(lifetime).expect("marker") as u64
                + u64::from(
                    std::path::Path::new(file)
                        .extension()
                        .is_some_and(|value| value == "go"),
                ) * 3;
            let (message, start) = refusal(file, &source);
            assert_eq!(
                message, "Lifetime.value must be singleton or prototype",
                "{file}"
            );
            assert_eq!(start, expected, "{file}");
        }
    }

    #[test]
    fn lifetime_needs_a_json_object_with_one_value() {
        let (message, start) = refusal("TallyService.kt", "@Lifetime(\"prototype\")\nclass A\n");
        assert!(message.contains("JSON object"), "{message}");
        assert_eq!(start, 0);
        let (message, _) = refusal(
            "TallyService.rs",
            "#[Lifetime({\"value\":\"prototype\",\"extra\":1})]\nstruct A {}\n",
        );
        assert_eq!(message, "Lifetime requires exactly these keys: value");
    }

    #[test]
    fn service_and_needs_all_take_no_arguments() {
        let (message, start) = refusal(
            "TallyService.go",
            "\n// @Service(\"tally\")\ntype A struct{}\n",
        );
        assert!(
            message.starts_with("Service accepts no arguments"),
            "{message}"
        );
        assert_eq!(start, 4);
        let (message, _) = refusal("TallyService.swift", "@NeedsAll(\"count\")\nclass A {}\n");
        assert!(
            message.starts_with("NeedsAll takes no field list; every field can be filled"),
            "{message}"
        );
    }
}
