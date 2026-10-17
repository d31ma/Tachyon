use super::{
    CppAttribute, CppClass, CppFailure, CppFile, CppMember, CppMemberKind, CppSource, CppSpelling,
};
use crate::Failure;
use crate::annotation_rules::{AnnotationRules, PlacedMarker, Realm, Target};
use std::path::Path;

pub(crate) struct CppMarkerCheck<'a> {
    path: &'a Path,
    realm: Realm,
    class_markers: Vec<String>,
}

impl<'a> CppMarkerCheck<'a> {
    const NEAR_MISS: usize = 2;
    const DESCRIPTION: &'static str = "Description";
    const DESCRIPTION_FORM: &'static str = "[[Description]] takes one quoted line of at most 256 characters, without quotes, escapes or control characters,, for example `class [[Service, Description(\"Keeps the score\")]] TallyService`";

    pub(crate) fn check_source(path: &'a Path, source: &str) -> Result<(), Failure> {
        let file = CppFile::read(path, source)?;
        Self::check(path, &file.class)
    }

    pub(crate) fn either(
        path: &'a Path,
        source: &str,
        other: impl Fn(&Path, &str) -> Result<(), Failure>,
    ) -> Result<(), Failure> {
        Self::routed(CppSource::is_cpp(path), path, source, other)
    }

    fn routed(
        cpp: bool,
        path: &'a Path,
        source: &str,
        other: impl Fn(&Path, &str) -> Result<(), Failure>,
    ) -> Result<(), Failure> {
        if cpp {
            return Self::check_source(path, source);
        }
        other(path, source)
    }

    pub(crate) fn check(path: &'a Path, class: &CppClass) -> Result<(), Failure> {
        let check = Self {
            path,
            realm: Realm::of(path).unwrap_or(Realm::Tac),
            class_markers: class
                .markers
                .iter()
                .map(|marker| marker.name.clone())
                .collect(),
        };
        check.check_class(class)
    }

    pub(crate) fn is_standard(name: &str) -> bool {
        CppSpelling::is_standard(name)
    }

    pub(crate) fn description(class: &CppClass) -> Option<String> {
        let marker = class
            .markers
            .iter()
            .find(|marker| marker.name == Self::DESCRIPTION)?;
        serde_json::from_str::<String>(&marker.arguments).ok()
    }

    fn check_description(&self, class: &CppClass) -> Result<(), Failure> {
        let Some(marker) = class
            .markers
            .iter()
            .find(|marker| marker.name == Self::DESCRIPTION)
        else {
            return Ok(());
        };
        let plain = Self::description(class).is_some_and(|text| {
            let bounded = [!text.trim().is_empty(), text.chars().count() <= 256] == [true, true];
            let clean = !text
                .chars()
                .any(|character| character.is_control() || matches!(character, '"' | '\'' | '\\'));
            [bounded, clean] == [true, true]
        });
        plain
            .then_some(())
            .ok_or_else(|| self.refused(marker, Self::DESCRIPTION_FORM))
    }

    pub(crate) fn described_alone(class: &CppClass) -> Option<usize> {
        let marker = class
            .markers
            .iter()
            .find(|marker| marker.name == Self::DESCRIPTION)?;
        let accompanied = class.markers.iter().any(|other| {
            [
                other.name != Self::DESCRIPTION,
                CppSpelling::of(&other.name) == CppSpelling::Exact,
            ] == [true, true]
        });
        (!accompanied).then_some(marker.at)
    }

    fn check_class(&self, class: &CppClass) -> Result<(), Failure> {
        self.declaration(&class.markers, Target::Type)?;
        self.check_description(class)?;
        for member in &class.members {
            self.check_member(member)?;
        }
        Ok(())
    }

    fn check_member(&self, member: &CppMember) -> Result<(), Failure> {
        self.declaration(&member.markers, Self::member_target(member.kind))?;
        for parameter in &member.parameters {
            self.declaration(&parameter.markers, Target::Parameter)?;
        }
        Ok(())
    }

    fn member_target(kind: CppMemberKind) -> Target {
        match kind {
            CppMemberKind::Field => Target::Field,
            _ => Target::Method,
        }
    }

    fn declaration(&self, markers: &[CppAttribute], target: Target) -> Result<(), Failure> {
        for index in 0..markers.len() {
            self.one_marker(markers, index, target)?;
        }
        Ok(())
    }

    fn one_marker(
        &self,
        markers: &[CppAttribute],
        index: usize,
        target: Target,
    ) -> Result<(), Failure> {
        let marker = &markers[index];
        let tachyon = self.is_tachyon_marker(marker)?;
        let neighbours: Vec<String> = markers
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, other)| other.name.clone())
            .collect();
        let placed = PlacedMarker {
            name: &marker.name,
            target,
            has_arguments: marker.has_arguments(),
            neighbours: &neighbours,
            class_markers: &self.class_markers,
        };
        let problem = tachyon
            .then(|| placed.problem(CppSource::EXTENSION, self.realm))
            .flatten();
        problem.map_or(Ok(()), |(code, message)| {
            Err(CppFailure::coded(code, self.path, marker.at, &message))
        })
    }

    fn is_tachyon_marker(&self, marker: &CppAttribute) -> Result<bool, Failure> {
        match CppSpelling::of(&marker.name) {
            CppSpelling::Exact => Ok(true),
            CppSpelling::Vendor | CppSpelling::Standard => Ok(false),
            CppSpelling::Unknown => Err(self.unknown(marker)),
            CppSpelling::OwnNamespace => Err(self.refused(
                marker,
                &format!(
                    "Tachyon markers have no namespace in C++: write [[{}]], not [[{}]]",
                    marker.name.rsplit("::").next().unwrap_or_default(),
                    marker.name
                ),
            )),
            CppSpelling::Cased(rule) => Err(self.refused(
                marker,
                &format!(
                    "C++ markers are PascalCase standard attributes: write [[{rule}]], not [[{}]]",
                    marker.name
                ),
            )),
        }
    }

    fn refused(&self, marker: &CppAttribute, message: &str) -> Failure {
        CppFailure::unreadable(self.path, marker.at, message)
    }

    fn unknown(&self, marker: &CppAttribute) -> Failure {
        let closest = AnnotationRules::rules()
            .filter(|rule| !rule.is_retired())
            .map(|rule| (Self::distance(rule.name, &marker.name), rule.name))
            .min();
        let suggestion = closest
            .filter(|(distance, _)| *distance <= Self::NEAR_MISS)
            .map(|(_, name)| format!("; did you mean [[{name}]]?"))
            .unwrap_or_default();
        let message = format!(
            "[[{}]] is not a Tachyon annotation or a standard C++ attribute{suggestion}",
            marker.name
        );
        self.refused(marker, &message)
    }

    fn distance(left: &str, right: &str) -> usize {
        let right: Vec<char> = right.chars().collect();
        let mut previous: Vec<usize> = (0..=right.len()).collect();
        for (row, left_character) in left.chars().enumerate() {
            previous = Self::next_row(&previous, row, left_character, &right);
        }
        previous[right.len()]
    }

    fn next_row(
        previous: &[usize],
        row: usize,
        left_character: char,
        right: &[char],
    ) -> Vec<usize> {
        let mut current = vec![row + 1];
        for (column, right_character) in right.iter().enumerate() {
            let replaced = previous[column] + usize::from(left_character != *right_character);
            let best = replaced
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
            current.push(best);
        }
        current
    }
}
