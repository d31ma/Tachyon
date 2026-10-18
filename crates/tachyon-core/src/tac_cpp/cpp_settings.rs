use super::{CppAttribute, CppClass, CppFailure, CppMember, CppMemberKind};
use crate::Failure;
use crate::behavior::{WrittenField, WrittenMarker, WrittenPolicies};
use serde_json::{Map, Value, json};
use std::path::Path;

pub(crate) struct CppSettings;

impl CppSettings {
    const SETTINGS: &'static str = "Settings";
    const SETTING: &'static str = "Setting";
    const BOUNDS: [&'static str; 2] = ["Min", "Max"];
    const OTHER_CHECKS: [&'static str; 3] = ["Required", "NotEmpty", "Length"];
    const MAX_VALUES: usize = 64;
    const BOTH: &'static str = "A C++ model is either [[Settings]] or [[Checked]], not both";
    const BOUND_FORM: &'static str = "A C++ [[Settings]] field takes only [[Min]] and [[Max]], each with one finite JSON number, for example [[Min(1)]]";
    const SETTING_FIELD: &'static str = "[[Setting]] goes on a public instance field of a C++ model, for example `[[Setting(\"${tac.app.title:Shop}\")]] std::string title;`";
    const TOO_MANY: &'static str = "A C++ model binds at most 64 [[Setting]] fields";

    pub(crate) fn is_settings(class: &CppClass) -> bool {
        class
            .markers
            .iter()
            .any(|marker| marker.name == Self::SETTINGS)
    }

    pub(crate) fn plans(path: &Path, source: &str, class: &CppClass) -> Result<String, Failure> {
        let written = WrittenPolicies::new(path, source, super::CppSource::EXTENSION);
        let settings = class
            .markers
            .iter()
            .find(|marker| marker.name == Self::SETTINGS)
            .map(|marker| Self::settings_plan(&written, path, source, class, marker))
            .transpose()?
            .unwrap_or_default();
        let values = Self::values_plan(&written, path, class)?;
        Ok(format!("{settings}{values}"))
    }

    fn settings_plan(
        written: &WrittenPolicies<'_>,
        path: &Path,
        source: &str,
        class: &CppClass,
        marker: &CppAttribute,
    ) -> Result<String, Failure> {
        let checked = class.markers.iter().any(|other| other.name == "Checked");
        (!checked)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::BOTH))?;
        let plan = Self::settings(written, path, source, class, marker)?;
        Ok(Self::specialization("TacSettingsPlan", &class.name, &plan))
    }

    fn values_plan(
        written: &WrittenPolicies<'_>,
        path: &Path,
        class: &CppClass,
    ) -> Result<String, Failure> {
        let marked = class.members.iter().filter_map(|member| {
            member
                .markers
                .iter()
                .find(|marker| marker.name == Self::SETTING)
                .map(|marker| (member, marker))
        });
        let mut values = Map::new();
        for (member, marker) in marked {
            Self::add_value(written, path, &mut values, member, marker)?;
        }
        Ok(Self::values_text(&class.name, &values))
    }

    fn add_value(
        written: &WrittenPolicies<'_>,
        path: &Path,
        values: &mut Map<String, Value>,
        member: &CppMember,
        marker: &CppAttribute,
    ) -> Result<(), Failure> {
        member
            .is_public_instance(CppMemberKind::Field)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::SETTING_FIELD))?;
        (values.len() < Self::MAX_VALUES)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::TOO_MANY))?;
        values.insert(
            member.name.clone(),
            written.setting(&Self::written(marker))?,
        );
        Ok(())
    }

    fn values_text(name: &str, values: &Map<String, Value>) -> String {
        (!values.is_empty())
            .then(|| Self::specialization("TacValuesPlan", name, &json!({ "fields": values })))
            .into_iter()
            .collect()
    }

    fn settings(
        written: &WrittenPolicies<'_>,
        path: &Path,
        source: &str,
        class: &CppClass,
        marker: &CppAttribute,
    ) -> Result<Value, Failure> {
        let members: Vec<&CppMember> = class
            .members
            .iter()
            .filter(|member| member.is_public_instance(CppMemberKind::Field))
            .collect();
        let types: Vec<String> = members.iter().map(|member| member.plain_type()).collect();
        let mut fields = Vec::new();
        let mut bounds = Map::new();
        for (member, type_source) in members.iter().zip(&types) {
            fields.push(WrittenField {
                name: &member.name,
                type_source,
                initializer: Self::initializer(source, member),
            });
            bounds.insert(
                member.name.clone(),
                Value::Object(Self::bounds(path, member)?),
            );
        }
        written.settings(&Self::written(marker), &fields, bounds)
    }

    fn bounds(path: &Path, member: &CppMember) -> Result<Map<String, Value>, Failure> {
        let mut limits = Map::new();
        for marker in &member.markers {
            Self::add_bound(path, &mut limits, marker)?;
        }
        Ok(limits)
    }

    fn add_bound(
        path: &Path,
        limits: &mut Map<String, Value>,
        marker: &CppAttribute,
    ) -> Result<(), Failure> {
        let refused = Self::OTHER_CHECKS.contains(&marker.name.as_str());
        (!refused)
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::BOUND_FORM))?;
        Self::BOUNDS
            .contains(&marker.name.as_str())
            .then(|| Self::insert_bound(path, limits, marker))
            .transpose()
            .map(drop)
    }

    fn insert_bound(
        path: &Path,
        limits: &mut Map<String, Value>,
        marker: &CppAttribute,
    ) -> Result<(), Failure> {
        let number = serde_json::from_str::<Value>(&marker.arguments)
            .ok()
            .filter(Value::is_number)
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::BOUND_FORM))?;
        let fresh = limits
            .insert(marker.name.to_ascii_lowercase(), number)
            .is_none();
        fresh
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::BOUND_FORM))
    }

    fn initializer<'s>(source: &'s str, member: &CppMember) -> Option<&'s str> {
        let start = member.name_at + member.name.len();
        let written = source.get(start..member.terminator)?.trim();
        if let Some(value) = written.strip_prefix('=') {
            return Some(value.trim());
        }
        written
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn specialization(plan_type: &str, name: &str, plan: &Value) -> String {
        let literal = Value::String(plan.to_string()).to_string();
        format!(
            "namespace tachyon {{\ntemplate <>\nstruct {plan_type}<::{name}> {{\n    static const json::JsonNode& plan() {{\n        static const json::JsonNode written = json::JsonNode::parse({literal});\n        return written;\n    }}\n}};\n}}\n"
        )
    }

    fn written(marker: &CppAttribute) -> WrittenMarker<'_> {
        WrittenMarker {
            name: &marker.name,
            arguments: &marker.arguments,
            at: marker.at,
        }
    }
}
