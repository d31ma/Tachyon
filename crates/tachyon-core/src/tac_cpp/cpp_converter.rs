use super::{
    CppAttribute, CppChecks, CppClass, CppFailure, CppMember, CppMemberKind, CppModel, CppTypeText,
};
use crate::Failure;
use serde_json::{Map, Value};
use std::fmt::Write as _;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
enum CppFill {
    Path(Vec<String>),
    Constant(Value),
}

struct CppConversion {
    typed: bool,
    fills: Vec<(String, CppFill)>,
}

pub(crate) struct CppConverter<'a> {
    path: &'a Path,
    models: &'a [CppModel],
}

impl<'a> CppConverter<'a> {
    const ROLE: &'static str = "Converter";
    const MARKER: &'static str = "Convert";
    const OPTIONAL: &'static str = "std::optional<";
    const LIST: &'static str = "std::vector<";
    const MAP: &'static str = "std::map<";
    const JSON: [&'static str; 4] = [
        "JsonNode",
        "json::JsonNode",
        "tachyon::json::JsonNode",
        "::tachyon::json::JsonNode",
    ];
    const TEXT: [&'static str; 2] = ["std::string", "string"];
    const FRACTIONS: [&'static str; 2] = ["float", "double"];
    const ENTRY_KEYS: [&'static str; 3] = ["target", "source", "constant"];
    const MAX_FIELDS: usize = 32;
    const MAX_DEPTH: usize = 8;
    const MAX_NAME: usize = 256;
    const SOURCE: &'static str = "tachyonSource";
    const NO_IMPLEMENTATION: &'static str = "A Converter has no implementation: Tachyon generates each mapping. A C++ [[Converter]] class declares only methods without a body, for example `Summary toSummary(const Product& product);`";
    const ONE_PARAMETER: &'static str = "A C++ [[Convert]] method takes exactly one named parameter, the value it converts, and returns the new value, not a Task";
    const PLACE: &'static str =
        "[[Convert]] goes on a method of a C++ [[Converter]] class in client/converters";
    const FORM: &'static str = "[[Convert]] takes nothing, or one JSON object with \"fields\": 1 through 32 entries, each with a \"target\" field name and exactly one of a dotted \"source\" path of at most 8 names or a JSON scalar \"constant\"; each target appears once";
    const TYPES: &'static str = "A C++ [[Convert]] method turns one model in client/models into another, or a JsonNode into a JsonNode; one method cannot mix model types and JSON values";
    const UNTYPED: &'static str = "A C++ [[Convert]] method that takes and returns a JsonNode needs a field plan, for example [[Convert({\"fields\":[{\"source\":\"detail.id\",\"target\":\"productId\"}]})]]";
    const DEEP: &'static str =
        "models nest more than 8 levels deep; map the deeper levels with their own method";
    const MAYBE_EMPTY: &'static str =
        "its source may be empty, so the target field needs std::optional";

    pub(crate) fn new(path: &'a Path, models: &'a [CppModel]) -> Self {
        Self { path, models }
    }

    pub(crate) fn is_converter(class: &CppClass) -> bool {
        class.markers.iter().any(|marker| marker.name == Self::ROLE)
    }

    pub(crate) fn check(path: &Path, source: &str, class: &CppClass) -> Result<(), Failure> {
        if !Self::is_converter(class) {
            let misplaced = class
                .members
                .iter()
                .find(|member| member.marked(Self::MARKER));
            return misplaced.map_or(Ok(()), |member| {
                Err(CppFailure::unreadable(path, member.name_at, Self::PLACE))
            });
        }
        for member in &class.members {
            Self::check_member(path, source, member)?;
        }
        Ok(())
    }

    pub(crate) fn check_methods(&self, class: &CppClass) -> Result<(), Failure> {
        for member in &class.members {
            self.conversion(member)?;
        }
        Ok(())
    }

    pub(crate) fn definitions(&self, class: &CppClass) -> String {
        let mut text = String::new();
        for member in &class.members {
            if let Ok(conversion) = self.conversion(member) {
                text.push_str(&Self::definition(&class.name, member, &conversion));
            }
        }
        text
    }

    fn check_member(path: &Path, source: &str, member: &CppMember) -> Result<(), Failure> {
        let bodyless = source.as_bytes().get(member.terminator) == Some(&b';');
        let method = [member.is_instance(CppMemberKind::Method), bodyless] == [true, true];
        method
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, member.name_at, Self::NO_IMPLEMENTATION))?;
        let named = member
            .parameters
            .first()
            .is_some_and(|parameter| !parameter.name.is_empty());
        let shaped = [member.parameters.len() == 1, named, !member.is_task()] == [true, true, true];
        shaped
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, member.name_at, Self::ONE_PARAMETER))?;
        let markers: Vec<&CppAttribute> = member
            .markers
            .iter()
            .filter(|marker| marker.name == Self::MARKER)
            .collect();
        if let Some(repeated) = markers.get(1) {
            return Err(CppFailure::unreadable(path, repeated.at, Self::FORM));
        }
        markers.first().map_or(Ok(()), |marker| {
            Self::plan(marker)
                .map(drop)
                .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::FORM))
        })
    }

    fn plan(marker: &CppAttribute) -> Option<Vec<(String, CppFill)>> {
        if !marker.has_arguments() {
            return Some(Vec::new());
        }
        let options: Value = serde_json::from_str(&marker.arguments).ok()?;
        let object = options.as_object().filter(|object| object.len() == 1)?;
        let entries = object.get("fields")?.as_array()?;
        if !(1..=Self::MAX_FIELDS).contains(&entries.len()) {
            return None;
        }
        let mut plan: Vec<(String, CppFill)> = Vec::new();
        for entry in entries {
            let (target, fill) = Self::entry(entry.as_object()?)?;
            if plan.iter().any(|(written, _)| *written == target) {
                return None;
            }
            plan.push((target, fill));
        }
        Some(plan)
    }

    fn entry(entry: &Map<String, Value>) -> Option<(String, CppFill)> {
        if entry
            .keys()
            .any(|key| !Self::ENTRY_KEYS.contains(&key.as_str()))
        {
            return None;
        }
        let target = entry
            .get("target")?
            .as_str()
            .filter(|target| Self::bounded(target))?;
        let fill = match (entry.get("source"), entry.get("constant")) {
            (Some(source), None) => CppFill::Path(Self::source_path(source.as_str()?)?),
            (None, Some(constant)) => CppFill::Constant(Self::constant(constant)?),
            _ => return None,
        };
        Some((target.to_owned(), fill))
    }

    fn bounded(name: &str) -> bool {
        [name.is_empty(), name.len() > Self::MAX_NAME] == [false, false]
    }

    fn source_path(path: &str) -> Option<Vec<String>> {
        let names: Vec<String> = path.split('.').map(str::to_owned).collect();
        let fits = [
            names.len() <= Self::MAX_DEPTH,
            names.iter().all(|name| Self::bounded(name)),
        ] == [true, true];
        fits.then_some(names)
    }

    fn constant(value: &Value) -> Option<Value> {
        let fits = match value {
            Value::String(text) => text.len() <= Self::MAX_NAME,
            Value::Null | Value::Bool(_) => true,
            Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
            _ => false,
        };
        fits.then(|| value.clone())
    }

    fn fail(&self, at: usize, message: &str) -> Failure {
        CppFailure::unreadable(self.path, at, message)
    }

    fn conversion(&self, member: &CppMember) -> Result<CppConversion, Failure> {
        let at = member.name_at;
        let parameter = member
            .parameters
            .first()
            .ok_or_else(|| self.fail(at, Self::ONE_PARAMETER))?;
        let source = CppTypeText::plain(&parameter.type_text);
        let target = member.plain_type();
        let plan = member
            .markers
            .iter()
            .find(|marker| marker.name == Self::MARKER)
            .and_then(Self::plan)
            .unwrap_or_default();
        match [Self::is_json(&source), Self::is_json(&target)] {
            [true, true] if plan.is_empty() => Err(self.fail(at, Self::UNTYPED)),
            [true, true] => Ok(CppConversion {
                typed: false,
                fills: plan,
            }),
            [false, false] => self.typed(at, &source, &target, &plan),
            _ => Err(self.fail(at, Self::TYPES)),
        }
    }

    fn typed(
        &self,
        at: usize,
        source: &str,
        target: &str,
        plan: &[(String, CppFill)],
    ) -> Result<CppConversion, Failure> {
        let (Some(source_model), Some(target_model)) =
            (self.structure(source), self.structure(target))
        else {
            return Err(self.fail(at, Self::TYPES));
        };
        let target_fields = target_model.fields();
        let source_fields = source_model.fields();
        let unknown = plan
            .iter()
            .find(|(name, _)| !target_fields.iter().any(|(field, _)| field == name));
        if let Some((name, _)) = unknown {
            return Err(self.fail(
                at,
                &format!("The C++ [[Convert]] plan sets {name}, and {target} has no field {name}"),
            ));
        }
        let mut fills = Vec::new();
        let mut missing = Vec::new();
        for (field, field_type) in &target_fields {
            let written = plan
                .iter()
                .find(|(name, _)| name == field)
                .map(|(_, fill)| fill.clone());
            let same = source_fields
                .iter()
                .any(|(name, _)| name == field)
                .then(|| CppFill::Path(vec![field.clone()]));
            let Some(fill) = written.or(same) else {
                missing.push(field.clone());
                continue;
            };
            self.fits_fill(source, &fill, field_type)
                .map_err(|reason| {
                    self.fail(
                        at,
                        &format!(
                            "The C++ [[Convert]] method cannot fill {target}.{field}: {reason}"
                        ),
                    )
                })?;
            fills.push((field.clone(), fill));
        }
        if !missing.is_empty() {
            return Err(self.fail(
                at,
                &format!(
                    "The C++ [[Convert]] method has no source for {target} fields {}: give {source} a field with the same name, or add a [[Convert]] plan entry",
                    missing.join(", ")
                ),
            ));
        }
        Ok(CppConversion { typed: true, fills })
    }

    fn fits_fill(&self, source: &str, fill: &CppFill, target: &str) -> Result<(), String> {
        match fill {
            CppFill::Path(names) => {
                let (found, maybe_empty) = self.resolve(source, names)?;
                found.map_or(Ok(()), |found| self.fits(&found, target, maybe_empty, 0))
            }
            CppFill::Constant(constant) => self.fits_constant(constant, target),
        }
    }

    fn resolve(&self, start: &str, names: &[String]) -> Result<(Option<String>, bool), String> {
        let mut current = start.to_owned();
        let mut maybe_empty = false;
        for name in names {
            let (held, optional) = Self::unwrapped(&current);
            maybe_empty = [maybe_empty, optional].contains(&true);
            if Self::is_json(&held) {
                return Ok((None, true));
            }
            let model = self
                .structure(&held)
                .ok_or_else(|| format!("{held} is not a model, so it has no field {name}"))?;
            let (_, field_type) = model
                .fields()
                .into_iter()
                .find(|(field, _)| field == name)
                .ok_or_else(|| format!("{held} has no field {name}"))?;
            current = CppTypeText::plain(&field_type);
        }
        Ok((Some(current), maybe_empty))
    }

    fn fits(
        &self,
        source: &str,
        target: &str,
        maybe_empty: bool,
        depth: usize,
    ) -> Result<(), String> {
        if depth > Self::MAX_DEPTH {
            return Err(Self::DEEP.to_owned());
        }
        let (source_held, source_optional) = Self::unwrapped(source);
        let (target_held, target_optional) = Self::unwrapped(target);
        if Self::is_json(&target_held) {
            return Ok(());
        }
        let empty = [source_optional, maybe_empty].contains(&true);
        if [empty, !target_optional] == [true, true] {
            return Err(Self::MAYBE_EMPTY.to_owned());
        }
        if [Self::is_json(&source_held), source_held == target_held].contains(&true) {
            return Ok(());
        }
        let wrong = format!("{source_held} cannot fill {target_held}");
        if let (Some(from), Some(to)) = (self.kind(&source_held), self.kind(&target_held)) {
            let widened = [from, to] == ["whole", "number"];
            return [from == to, widened]
                .contains(&true)
                .then_some(())
                .ok_or(wrong);
        }
        if let (Some(from), Some(to)) = (
            Self::inside(&source_held, Self::LIST),
            Self::inside(&target_held, Self::LIST),
        ) {
            return self.fits(&from, &to, false, depth + 1);
        }
        if let (Some(from), Some(to)) =
            (Self::map_value(&source_held), Self::map_value(&target_held))
        {
            return self.fits(&from, &to, false, depth + 1);
        }
        match (self.structure(&source_held), self.structure(&target_held)) {
            (Some(from), Some(to)) => self.fits_model(from, to, depth),
            _ => Err(wrong),
        }
    }

    fn fits_model(&self, source: &CppModel, target: &CppModel, depth: usize) -> Result<(), String> {
        let source_fields = source.fields();
        for (field, field_type) in target.fields() {
            let (_, found) = source_fields
                .iter()
                .find(|(name, _)| *name == field)
                .ok_or_else(|| {
                    format!(
                        "{} has no field {field} for {}",
                        source.name(),
                        target.name()
                    )
                })?;
            self.fits(
                &CppTypeText::plain(found),
                &CppTypeText::plain(&field_type),
                false,
                depth + 1,
            )
            .map_err(|reason| format!("{field}: {reason}"))?;
        }
        Ok(())
    }

    fn fits_constant(&self, constant: &Value, target: &str) -> Result<(), String> {
        let (held, optional) = Self::unwrapped(target);
        if Self::is_json(&held) {
            return Ok(());
        }
        let kind = match constant {
            Value::Null => {
                return optional
                    .then_some(())
                    .ok_or_else(|| String::from("null needs a std::optional target field"));
            }
            Value::Bool(_) => "truth",
            Value::Number(number) if number.is_i64() || number.is_u64() => "whole",
            Value::Number(_) => "number",
            _ => "text",
        };
        let to = self.kind(&held);
        let fits = [
            to == Some(kind),
            [Some(kind), to] == [Some("whole"), Some("number")],
        ];
        fits.contains(&true)
            .then_some(())
            .ok_or_else(|| format!("the constant {constant} cannot fill {held}"))
    }

    fn kind(&self, held: &str) -> Option<&'static str> {
        if Self::TEXT.contains(&held) {
            return Some("text");
        }
        if held == "bool" {
            return Some("truth");
        }
        if Self::FRACTIONS.contains(&held) {
            return Some("number");
        }
        if CppChecks::NUMBERS.contains(&held) {
            return Some("whole");
        }
        self.model(held)
            .filter(|model| model.is_enum())
            .map(|_| "text")
    }

    fn model(&self, name: &str) -> Option<&CppModel> {
        let name = name.strip_prefix("::").unwrap_or(name);
        self.models.iter().find(|model| model.name() == name)
    }

    fn structure(&self, name: &str) -> Option<&CppModel> {
        self.model(name)
            .filter(|model| [model.is_enum(), model.is_abstract()] == [false, false])
    }

    fn is_json(type_name: &str) -> bool {
        Self::JSON.contains(&type_name)
    }

    fn unwrapped(type_name: &str) -> (String, bool) {
        Self::inside(type_name, Self::OPTIONAL)
            .map_or_else(|| (type_name.to_owned(), false), |held| (held, true))
    }

    fn inside(type_name: &str, prefix: &str) -> Option<String> {
        type_name
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix('>'))
            .map(|held| held.trim().to_owned())
    }

    fn map_value(type_name: &str) -> Option<String> {
        let inner = Self::inside(type_name, Self::MAP)?;
        let (key, value) = inner.split_once(',')?;
        Self::TEXT
            .contains(&key.trim())
            .then(|| value.trim().to_owned())
    }

    fn definition(class: &str, member: &CppMember, conversion: &CppConversion) -> String {
        let Some(parameter) = member.parameters.first() else {
            return String::new();
        };
        let constant = [" ", " const "][usize::from(member.is_const)];
        let body = if conversion.typed {
            Self::typed_body(
                member,
                &parameter.name,
                &CppTypeText::plain(&parameter.type_text),
                conversion,
            )
        } else {
            Self::json_body(&parameter.name, conversion)
        };
        format!(
            "inline {} {class}::{}({} {}){constant}{{\n{body}    return result;\n}}\n",
            member.type_text, member.name, parameter.type_text, parameter.name
        )
    }

    fn typed_body(
        member: &CppMember,
        value: &str,
        source: &str,
        conversion: &CppConversion,
    ) -> String {
        let mut text = format!("    {} result{{}};\n", member.plain_type());
        let nested = conversion
            .fills
            .iter()
            .any(|(_, fill)| matches!(fill, CppFill::Path(names) if names.len() > 1));
        if nested {
            let _ = writeln!(
                text,
                "    const ::tachyon::json::JsonNode {} = ::tachyon::json::JsonCodec<{source}>::toJson({value});",
                Self::SOURCE
            );
        }
        for (field, fill) in &conversion.fills {
            let filled = match fill {
                CppFill::Path(names) if names.len() == 1 => {
                    format!("copy<decltype(result.{field})>({value}.{})", names[0])
                }
                CppFill::Path(names) => format!(
                    "read<decltype(result.{field})>({}, {})",
                    Self::SOURCE,
                    Self::names(names)
                ),
                CppFill::Constant(constant) => format!(
                    "constant<decltype(result.{field})>({})",
                    Self::literal(&constant.to_string())
                ),
            };
            let _ = writeln!(
                text,
                "    result.{field} = ::tachyon::TacConvert::{filled};"
            );
        }
        text
    }

    fn json_body(value: &str, conversion: &CppConversion) -> String {
        let mut text = String::from("    ::tachyon::json::JsonObject result;\n");
        for (field, fill) in &conversion.fills {
            let filled = match fill {
                CppFill::Path(names) => {
                    format!("::tachyon::TacConvert::at({value}, {})", Self::names(names))
                }
                CppFill::Constant(constant) => format!(
                    "::tachyon::json::JsonNode::parse({})",
                    Self::literal(&constant.to_string())
                ),
            };
            let _ = writeln!(text, "    result[{}] = {filled};", Self::literal(field));
        }
        text
    }

    fn names(names: &[String]) -> String {
        let written: Vec<String> = names.iter().map(|name| Self::literal(name)).collect();
        format!("{{{}}}", written.join(", "))
    }

    fn literal(text: &str) -> String {
        Value::String(text.to_owned()).to_string()
    }
}
