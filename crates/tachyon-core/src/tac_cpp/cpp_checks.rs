use super::{CppAttribute, CppClass, CppFailure, CppMember, CppMemberKind, CppTypeText};
use crate::Failure;
use serde_json::Value;
use std::path::Path;

pub(crate) struct CppChecks;

impl CppChecks {
    const CHECKED: &'static str = "Checked";
    const CONSTRAINTS: [&'static str; 5] = ["Required", "NotEmpty", "Length", "Min", "Max"];
    const OPTIONAL: &'static str = "std::optional<";
    const TEXT: [&'static str; 2] = ["std::string", "string"];
    const SIZED: [&'static str; 5] = [
        "std::string",
        "string",
        "std::vector<",
        "std::map<",
        "std::set<",
    ];
    pub(crate) const NUMBERS: [&'static str; 23] = [
        "int",
        "unsigned",
        "unsigned int",
        "short",
        "unsigned short",
        "long",
        "unsigned long",
        "long long",
        "unsigned long long",
        "float",
        "double",
        "std::size_t",
        "size_t",
        "std::int8_t",
        "std::int16_t",
        "std::int32_t",
        "std::int64_t",
        "std::uint8_t",
        "std::uint16_t",
        "std::uint32_t",
        "std::uint64_t",
        "int64_t",
        "int32_t",
    ];
    const NEEDS_CHECKED: &'static str = "A C++ field constraint ([[Required]], [[NotEmpty]], [[Length]], [[Min]] or [[Max]]) needs [[Checked]] on its model, for example `struct [[Checked]] SignUp { ... };`";
    const MODELS_ONLY: &'static str = "[[Checked]] and the field constraints go on a C++ model in client/models; a service checks an argument with [[Check]], for example `void submit([[Check]] const SignUp& form)`";
    const MAX_FIELDS: usize = 64;
    const TOO_MANY: &'static str = "A [[Checked]] C++ model has at most 64 constrained fields";
    const REQUIRED_FORM: &'static str = "A plain C++ field always holds a value, so [[Required]] checks a std::optional or pointer field, for example `[[Required]] std::optional<std::string> email;`";
    const NOT_EMPTY_FORM: &'static str = "[[NotEmpty]] refuses blank text, so it goes on a std::string field (or a std::optional<std::string>); limit a list with [[Length({\"min\":1})]]";
    const MAX_LENGTH: u64 = 1_048_576;
    const LENGTH_FORM: &'static str = "[[Length]] takes one JSON object with whole numbers min and max from 0 to 1048576, for example [[Length({\"min\":1,\"max\":80})]], and goes on text, a std::vector, a std::map or a std::set";
    const MAX_NUMBER: f64 = 9_007_199_254_740_991.0;
    const LIMIT_FORM: &'static str = "[[Min]] and [[Max]] take one finite JSON number, for example [[Min(0)]], and go on a number field; Min cannot exceed Max";
    const DUPLICATE: &'static str = "Each field constraint appears once on a C++ field";
    const CHECK_FORM: &'static str = "[[Check]] takes no arguments and goes on a parameter whose type is a [[Checked]] model in client/models, for example `void submit([[Check]] const SignUp& form)`";

    pub(crate) fn is_checked(class: &CppClass) -> bool {
        class
            .markers
            .iter()
            .any(|marker| marker.name == Self::CHECKED)
    }

    pub(crate) fn check_model(path: &Path, class: &CppClass) -> Result<(), Failure> {
        let constrained: Vec<&CppMember> = class
            .members
            .iter()
            .filter(|member| Self::constraints(member).next().is_some())
            .collect();
        let unchecked = [constrained.is_empty(), Self::is_checked(class)] == [false, false];
        constrained
            .first()
            .filter(|_| unchecked)
            .map_or(Ok(()), |first| {
                Err(CppFailure::unreadable(
                    path,
                    first.name_at,
                    Self::NEEDS_CHECKED,
                ))
            })?;
        (constrained.len() <= Self::MAX_FIELDS)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, class.name_at, Self::TOO_MANY))?;
        constrained
            .iter()
            .try_for_each(|member| Self::check_field(path, member))
    }

    pub(crate) fn check_service(path: &Path, class: &CppClass) -> Result<(), Failure> {
        let field_marker = class.members.iter().flat_map(Self::constraints);
        class
            .markers
            .iter()
            .filter(|marker| marker.name == Self::CHECKED)
            .chain(field_marker)
            .next()
            .map_or(Ok(()), |marker| {
                Err(CppFailure::unreadable(path, marker.at, Self::MODELS_ONLY))
            })?;
        class
            .members
            .iter()
            .flat_map(|member| member.parameters.iter())
            .flat_map(|parameter| parameter.markers.iter())
            .filter(|marker| marker.name == "Check")
            .find(|marker| marker.has_arguments())
            .map_or(Ok(()), |marker| {
                Err(CppFailure::unreadable(path, marker.at, Self::CHECK_FORM))
            })
    }

    pub(crate) fn checked_types(class: &CppClass) -> Vec<(String, usize)> {
        class
            .members
            .iter()
            .flat_map(|member| member.parameters.iter())
            .filter_map(|parameter| {
                parameter
                    .markers
                    .iter()
                    .find(|marker| marker.name == "Check")
                    .map(|marker| (CppTypeText::plain(&parameter.type_text), marker.at))
            })
            .collect()
    }

    pub(crate) fn require_checked(
        path: &Path,
        at: usize,
        type_name: &str,
        checked: &[String],
    ) -> Result<(), Failure> {
        checked
            .iter()
            .any(|name| name == type_name)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, at, Self::CHECK_FORM))
    }

    pub(crate) fn guards(member: &CppMember) -> String {
        member
            .parameters
            .iter()
            .filter(|parameter| {
                parameter
                    .markers
                    .iter()
                    .any(|marker| marker.name == "Check")
            })
            .map(|parameter| {
                format!(
                    " ::tachyon::TacChecks<{}>::check({});",
                    CppTypeText::plain(&parameter.type_text),
                    parameter.name
                )
            })
            .collect::<Vec<_>>()
            .concat()
    }

    pub(crate) fn specialization(class: &CppClass) -> String {
        Self::is_checked(class)
            .then(|| Self::checks_text(class))
            .into_iter()
            .collect()
    }

    fn checks_text(class: &CppClass) -> String {
        let name = &class.name;
        let statements: String = class.members.iter().map(Self::field_statements).collect();
        format!(
            "namespace tachyon {{\ntemplate <>\nclass TacChecks<::{name}> {{\npublic:\n    static void check(const ::{name}& value) {{\n{statements}    }}\n}};\n}}\n"
        )
    }

    fn constraints(member: &CppMember) -> impl Iterator<Item = &CppAttribute> {
        member
            .markers
            .iter()
            .filter(|marker| Self::CONSTRAINTS.contains(&marker.name.as_str()))
    }

    fn field_statements(member: &CppMember) -> String {
        Self::CONSTRAINTS
            .iter()
            .filter_map(|rule| member.markers.iter().find(|marker| marker.name == *rule))
            .map(|marker| format!("        TacCheck::{};\n", Self::call(&member.name, marker)))
            .collect::<Vec<_>>()
            .concat()
    }

    fn call(field: &str, marker: &CppAttribute) -> String {
        match marker.name.as_str() {
            "Required" => format!("required(value.{field}, \"{field}\")"),
            "NotEmpty" => format!("notBlank(value.{field}, \"{field}\")"),
            "Length" => {
                let (minimum, maximum) = Self::length(marker).unwrap_or((0, Self::MAX_LENGTH));
                format!("size(value.{field}, {minimum}, {maximum}, \"{field}\")")
            }
            rule => {
                let bound = Self::limit(marker).unwrap_or_default();
                format!(
                    "{}(value.{field}, {bound:?}, \"{field}\")",
                    rule.to_ascii_lowercase()
                )
            }
        }
    }

    fn check_field(path: &Path, member: &CppMember) -> Result<(), Failure> {
        let mut seen: Vec<&str> = Vec::new();
        for marker in Self::constraints(member) {
            Self::seen_once(&mut seen, path, marker)?;
            Self::check_rule(path, member, marker)?;
        }
        Self::check_limits(path, member)
    }

    fn seen_once<'m>(
        seen: &mut Vec<&'m str>,
        path: &Path,
        marker: &'m CppAttribute,
    ) -> Result<(), Failure> {
        let repeated = seen.contains(&marker.name.as_str());
        seen.push(&marker.name);
        (!repeated)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::DUPLICATE))
    }

    fn check_limits(path: &Path, member: &CppMember) -> Result<(), Failure> {
        let minimum = member.markers.iter().find(|marker| marker.name == "Min");
        let maximum = member.markers.iter().find(|marker| marker.name == "Max");
        minimum
            .zip(maximum)
            .filter(|(minimum, maximum)| Self::limit(minimum) > Self::limit(maximum))
            .map_or(Ok(()), |(minimum, _)| {
                Err(CppFailure::unreadable(path, minimum.at, Self::LIMIT_FORM))
            })
    }

    fn check_rule(path: &Path, member: &CppMember, marker: &CppAttribute) -> Result<(), Failure> {
        let field = member.is_instance(CppMemberKind::Field);
        let written = member.plain_type();
        let held = Self::held(&written);
        let fits = match marker.name.as_str() {
            "Required" => (
                [
                    written.starts_with(Self::OPTIONAL),
                    member.type_text.contains('*'),
                ]
                .contains(&true),
                Self::REQUIRED_FORM,
            ),
            "NotEmpty" => (Self::TEXT.contains(&held), Self::NOT_EMPTY_FORM),
            "Length" => (
                [Self::length(marker).is_some(), Self::is_sized(held)] == [true, true],
                Self::LENGTH_FORM,
            ),
            _ => (
                [Self::limit(marker).is_some(), Self::NUMBERS.contains(&held)] == [true, true],
                Self::LIMIT_FORM,
            ),
        };
        let (shaped, message) = fits;
        [field, shaped]
            .iter()
            .all(|fact| *fact)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, message))
    }

    fn held(written: &str) -> &str {
        written
            .strip_prefix(Self::OPTIONAL)
            .and_then(|inner| inner.strip_suffix('>'))
            .map_or(written, str::trim)
    }

    fn is_sized(held: &str) -> bool {
        Self::SIZED.iter().any(|sized| {
            let prefix = [sized.ends_with('<'), held.starts_with(sized)] == [true, true];
            [held == *sized, prefix].contains(&true)
        })
    }

    fn length(marker: &CppAttribute) -> Option<(u64, u64)> {
        let options: Value = serde_json::from_str(&marker.arguments).ok()?;
        let object = options.as_object()?;
        object
            .keys()
            .all(|key| ["min", "max"].contains(&key.as_str()))
            .then_some(())?;
        let read = |key: &str, fallback: u64| -> Option<u64> {
            object.get(key).map_or(Some(fallback), Value::as_u64)
        };
        let minimum = read("min", 0)?;
        let maximum = read("max", Self::MAX_LENGTH)?;
        [minimum <= maximum, maximum <= Self::MAX_LENGTH]
            .iter()
            .all(|fact| *fact)
            .then_some((minimum, maximum))
    }

    fn limit(marker: &CppAttribute) -> Option<f64> {
        let value: Value = serde_json::from_str(&marker.arguments).ok()?;
        value
            .as_f64()
            .filter(|number| [number.is_finite(), number.abs() <= Self::MAX_NUMBER] == [true, true])
    }
}
