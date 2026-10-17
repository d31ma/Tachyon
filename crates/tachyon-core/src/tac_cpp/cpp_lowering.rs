use super::{
    CppAttribute, CppClass, CppCoverage, CppFile, CppLifetime, CppMarkerCheck, CppMember,
    CppRemote, CppService,
};
use std::fmt::Write as _;
use std::ops::Range;

pub(crate) struct CppLowering {
    text: String,
    insertions: Vec<(usize, String)>,
}

impl CppLowering {
    const REMOVED_DIRECTIVES: [&'static str; 3] =
        ["pragma once", "include \"", "include <tachyon/"];

    pub(crate) fn model(file: &CppFile, source: &str) -> String {
        Self::blanked(file, source).finished()
    }

    pub(crate) fn test_unit(file: &CppFile, source: &str, fakes: &[&CppMember]) -> String {
        let mut lowering = Self::blanked(file, source);
        let provided = Self::provided_fakes(fakes);
        let construct = format!(
            " public: template <typename Made> Made construct() {{{provided} return Made(); }}"
        );
        lowering
            .insertions
            .push((file.class.body.end.saturating_sub(1), construct));
        lowering.finished()
    }

    fn provided_fakes(fakes: &[&CppMember]) -> String {
        let mut provided = String::new();
        for fake in fakes {
            let _ = write!(
                provided,
                " tachyon::TacFake<{}> {}Fake({});",
                fake.plain_type(),
                fake.name,
                fake.name
            );
        }
        provided
    }

    pub(crate) fn service(service: &CppService, prototypes: &[String]) -> String {
        Self::service_lowering(service, prototypes).finished_service(service)
    }

    pub(crate) fn recorded_service(service: &CppService, prototypes: &[String]) -> String {
        let mut lowering = Self::service_lowering(service, prototypes);
        lowering.record_calls(service);
        lowering.finished_service(service)
    }

    fn service_lowering(service: &CppService, prototypes: &[String]) -> Self {
        let mut lowering = Self::blanked(&service.file, &service.source);
        let class = service.class();
        let friend = format!(" friend class {};", service.binding_name());
        lowering.insertions.push((class.body.start + 1, friend));
        lowering.fill_needs(class, prototypes);
        lowering
    }

    fn finished_service(self, service: &CppService) -> String {
        let remote = Self::remote_definitions(service);
        format!("{}{remote}", self.finished())
    }

    fn record_calls(&mut self, service: &CppService) {
        let class = service.class();
        for member in CppCoverage::covered_members(&service.source, &class.members) {
            let key = format!("{}.{}", class.name, member.name);
            let hook =
                crate::method_coverage::CallHooks::statement(super::CppSource::EXTENSION, &key);
            self.insertions.push((member.terminator + 1, hook));
        }
    }

    fn remote_definitions(service: &CppService) -> String {
        if !service.is_remote() {
            return String::new();
        }
        format!("\n{}", CppRemote::definitions(service.class()))
    }

    fn fill_needs(&mut self, class: &CppClass, prototypes: &[String]) {
        for injection in CppLifetime::injections(class) {
            let provider = &injection.provider;
            if !prototypes.contains(provider) {
                let shared = format!(" = ::tachyon::TacNeeds::of<{provider}>()");
                self.insertions.push((injection.terminator, shared));
                continue;
            }
            let owned = format!("tachyonOwned{}", Self::capitalized(&injection.field));
            let holder = format!(
                " std::shared_ptr<{provider}> {owned} = ::tachyon::TacNeeds::fresh<{provider}>();"
            );
            self.insertions.push((class.body.start + 1, holder));
            self.insertions
                .push((injection.terminator, format!(" = *{owned}")));
        }
    }

    fn capitalized(name: &str) -> String {
        let mut characters = name.chars();
        characters.next().map_or_else(String::new, |first| {
            format!("{}{}", first.to_ascii_uppercase(), characters.as_str())
        })
    }

    fn blanked(file: &CppFile, source: &str) -> Self {
        let mut text = source.to_owned();
        Self::blank_directives(&mut text, file, source);
        Self::blank_markers(&mut text, &file.class);
        Self {
            text,
            insertions: Vec::new(),
        }
    }

    fn blank_directives(text: &mut String, file: &CppFile, source: &str) {
        let removed = file
            .directives
            .iter()
            .filter(|range| Self::is_removed(&source[(*range).clone()]));
        for range in removed {
            Self::blank(text, range);
        }
    }

    fn blank_markers(text: &mut String, class: &CppClass) {
        let markers = class
            .every_marker()
            .into_iter()
            .filter(|marker| Self::is_tachyon(marker));
        for marker in markers {
            Self::blank(text, &marker.specifier);
        }
    }

    fn is_removed(directive: &str) -> bool {
        let squeezed = directive
            .trim_start_matches('#')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        Self::REMOVED_DIRECTIVES
            .iter()
            .any(|removed| squeezed.contains(removed))
    }

    fn is_tachyon(marker: &CppAttribute) -> bool {
        let standard = CppMarkerCheck::is_standard(&marker.name);
        let known = crate::annotation_rules::AnnotationRules::find(&marker.name).is_some();
        [standard, known] == [false, true]
    }

    fn blank(text: &mut String, range: &Range<usize>) {
        let spaces: String = text[range.clone()]
            .chars()
            .map(Self::blank_character)
            .collect();
        text.replace_range(range.clone(), &spaces);
    }

    fn blank_character(character: char) -> String {
        if character == '\n' {
            return String::from("\n");
        }
        " ".repeat(character.len_utf8())
    }

    fn finished(mut self) -> String {
        self.insertions
            .sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        for (at, inserted) in &self.insertions {
            self.text.insert_str(*at, inserted);
        }
        self.text
    }
}
