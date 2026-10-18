use super::{
    CppAttribute, CppChecks, CppClass, CppConversion, CppFailure, CppFill, CppMember,
    CppMemberKind, CppModel, CppParameter, CppTypeText,
};
use crate::Failure;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

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
    const KINDS: [(&'static [&'static str], &'static str); 4] = [
        (&Self::TEXT, "text"),
        (&["bool"], "truth"),
        (&Self::FRACTIONS, "number"),
        (&CppChecks::NUMBERS, "whole"),
    ];
    const ENTRY_KEYS: [&'static str; 3] = ["target", "source", "constant"];
    const SOURCE: &'static str = "tachyonSource";
    const NO_IMPLEMENTATION: &'static str = "A Converter has no implementation: Tachyon generates each mapping. A C++ [[Converter]] class declares only methods without a body, for example `Summary toSummary(const Product& product);`";
    const ONE_PARAMETER: &'static str = "A C++ [[Convert]] method takes exactly one named parameter, the value it converts, and returns the new value, not a Task";
    const PLACE: &'static str =
        "[[Convert]] goes on a method of a C++ [[Converter]] class in client/converters";
    const MAX_FIELDS: usize = 32;
    const FORM: &'static str = "[[Convert]] takes nothing, or one JSON object with \"fields\": 1 through 32 entries, each with a \"target\" field name and exactly one of a dotted \"source\" path of at most 8 names or a JSON scalar \"constant\"; each target appears once";
    const TYPES: &'static str = "A C++ [[Convert]] method turns one model in client/models into another, or a JsonNode into a JsonNode; one method cannot mix model types and JSON values";
    const MAX_DEPTH: usize = 8;
    const UNTYPED: &'static str = "A C++ [[Convert]] method that takes and returns a JsonNode needs a field plan, for example [[Convert({\"fields\":[{\"source\":\"detail.id\",\"target\":\"productId\"}]})]]";
    const DEEP: &'static str =
        "models nest more than 8 levels deep; map the deeper levels with their own method";
    const MAX_NAME: usize = 256;
    const MAYBE_EMPTY: &'static str =
        "its source may be empty, so the target field needs std::optional";

    pub(crate) fn new(path: &'a Path, models: &'a [CppModel]) -> Self {
        Self { path, models }
    }

    pub(crate) fn is_converter(class: &CppClass) -> bool {
        class.markers.iter().any(|marker| marker.name == Self::ROLE)
    }

    pub(crate) fn check(path: &Path, source: &str, class: &CppClass) -> Result<(), Failure> {
        let checks = [Self::check_misplaced, Self::check_members];
        checks[usize::from(Self::is_converter(class))](path, source, class)
    }

    pub(crate) fn check_methods(&self, class: &CppClass) -> Result<(), Failure> {
        class
            .members
            .iter()
            .try_for_each(|member| self.conversion(member).map(drop))
    }

    pub(crate) fn definitions(&self, class: &CppClass) -> String {
        class
            .members
            .iter()
            .filter_map(|member| {
                self.conversion(member)
                    .ok()
                    .map(|conversion| Self::definition(&class.name, member, &conversion))
            })
            .collect()
    }

    fn check_misplaced(path: &Path, _source: &str, class: &CppClass) -> Result<(), Failure> {
        class
            .members
            .iter()
            .find(|member| member.marked(Self::MARKER))
            .map_or(Ok(()), |member| {
                Err(CppFailure::unreadable(path, member.name_at, Self::PLACE))
            })
    }

    fn check_members(path: &Path, source: &str, class: &CppClass) -> Result<(), Failure> {
        class
            .members
            .iter()
            .try_for_each(|member| Self::check_member(path, source, member))
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
        markers.get(1).map_or(Ok(()), |repeated| {
            Err(CppFailure::unreadable(path, repeated.at, Self::FORM))
        })?;
        markers.first().map_or(Ok(()), |marker| {
            Self::plan(marker)
                .map(drop)
                .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::FORM))
        })
    }

    fn plan(marker: &CppAttribute) -> Option<Vec<(String, CppFill)>> {
        marker
            .has_arguments()
            .then_some(marker.arguments.as_str())
            .map_or_else(|| Some(Vec::new()), Self::written_plan)
    }

    fn written_plan(arguments: &str) -> Option<Vec<(String, CppFill)>> {
        let options: Value = serde_json::from_str(arguments).ok()?;
        let object = options.as_object().filter(|object| object.len() == 1)?;
        let entries = object
            .get("fields")?
            .as_array()
            .filter(|entries| (1..=Self::MAX_FIELDS).contains(&entries.len()))?;
        let plan: Vec<(String, CppFill)> = entries
            .iter()
            .map(|entry| entry.as_object().and_then(Self::entry))
            .collect::<Option<_>>()?;
        let targets: BTreeSet<&str> = plan.iter().map(|(target, _)| target.as_str()).collect();
        (targets.len() == plan.len()).then_some(plan)
    }

    fn entry(entry: &Map<String, Value>) -> Option<(String, CppFill)> {
        entry
            .keys()
            .all(|key| Self::ENTRY_KEYS.contains(&key.as_str()))
            .then_some(())?;
        let target = entry
            .get("target")?
            .as_str()
            .filter(|target| Self::bounded(target))?;
        let fill = Self::fill(entry.get("source"), entry.get("constant"))?;
        Some((target.to_owned(), fill))
    }

    fn fill(source: Option<&Value>, constant: Option<&Value>) -> Option<CppFill> {
        match (source, constant) {
            (Some(source), None) => source
                .as_str()
                .and_then(Self::source_path)
                .map(CppFill::from_path),
            (None, Some(constant)) => Self::constant(constant).map(CppFill::from_constant),
            _ => None,
        }
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
        let fits = [
            value.is_null(),
            value.is_boolean(),
            value.as_f64().is_some_and(f64::is_finite),
            value
                .as_str()
                .is_some_and(|text| text.len() <= Self::MAX_NAME),
        ];
        fits.contains(&true).then(|| value.clone())
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
        let json = [Self::is_json(&source), Self::is_json(&target)];
        let kinds = [Self::typed, Self::mixed, Self::untyped];
        let chosen = json.iter().filter(|json| **json).count();
        kinds[chosen](self, at, &source, &target, &plan)
    }

    fn mixed(
        &self,
        at: usize,
        _source: &str,
        _target: &str,
        _plan: &[(String, CppFill)],
    ) -> Result<CppConversion, Failure> {
        Err(self.fail(at, Self::TYPES))
    }

    fn untyped(
        &self,
        at: usize,
        _source: &str,
        _target: &str,
        plan: &[(String, CppFill)],
    ) -> Result<CppConversion, Failure> {
        let planned = !plan.is_empty();
        planned
            .then(|| CppConversion {
                typed: false,
                fills: plan.to_vec(),
            })
            .ok_or_else(|| self.fail(at, Self::UNTYPED))
    }

    fn typed(
        &self,
        at: usize,
        source: &str,
        target: &str,
        plan: &[(String, CppFill)],
    ) -> Result<CppConversion, Failure> {
        let (source_model, target_model) = self
            .structure(source)
            .zip(self.structure(target))
            .ok_or_else(|| self.fail(at, Self::TYPES))?;
        let target_fields = target_model.fields();
        let source_fields = source_model.fields();
        self.check_targets(at, target, &target_fields, plan)?;
        let chosen: Vec<(&String, &String, Option<CppFill>)> = target_fields
            .iter()
            .map(|(field, field_type)| {
                (field, field_type, Self::chosen(plan, &source_fields, field))
            })
            .collect();
        chosen
            .iter()
            .filter_map(|(field, field_type, fill)| {
                fill.as_ref().map(|fill| (field, field_type, fill))
            })
            .try_for_each(|(field, field_type, fill)| {
                self.fits_fill(source, fill, field_type).map_err(|reason| {
                    self.fail(
                        at,
                        &format!(
                            "The C++ [[Convert]] method cannot fill {target}.{field}: {reason}"
                        ),
                    )
                })
            })?;
        self.check_missing(at, source, target, &chosen)?;
        let fills = chosen
            .into_iter()
            .filter_map(|(field, _, fill)| fill.map(|fill| (field.clone(), fill)))
            .collect();
        Ok(CppConversion { typed: true, fills })
    }

    fn check_targets(
        &self,
        at: usize,
        target: &str,
        target_fields: &[(String, String)],
        plan: &[(String, CppFill)],
    ) -> Result<(), Failure> {
        plan.iter()
            .find(|(name, _)| !target_fields.iter().any(|(field, _)| field == name))
            .map_or(Ok(()), |(name, _)| {
                Err(self.fail(
                    at,
                    &format!(
                        "The C++ [[Convert]] plan sets {name}, and {target} has no field {name}"
                    ),
                ))
            })
    }

    fn check_missing(
        &self,
        at: usize,
        source: &str,
        target: &str,
        chosen: &[(&String, &String, Option<CppFill>)],
    ) -> Result<(), Failure> {
        let missing: Vec<&str> = chosen
            .iter()
            .filter(|(_, _, fill)| fill.is_none())
            .map(|(field, _, _)| field.as_str())
            .collect();
        missing.is_empty().then_some(()).ok_or_else(|| {
            self.fail(
                at,
                &format!(
                    "The C++ [[Convert]] method has no source for {target} fields {}: give {source} a field with the same name, or add a [[Convert]] plan entry",
                    missing.join(", ")
                ),
            )
        })
    }

    fn chosen(
        plan: &[(String, CppFill)],
        source_fields: &[(String, String)],
        field: &String,
    ) -> Option<CppFill> {
        let written = plan
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, fill)| fill.clone());
        let same = source_fields
            .iter()
            .any(|(name, _)| name == field)
            .then(|| CppFill::from_path(vec![field.clone()]));
        written.or(same)
    }

    fn fits_fill(&self, source: &str, fill: &CppFill, target: &str) -> Result<(), String> {
        fill.constant.as_ref().map_or_else(
            || self.fits_path(source, &fill.path, target),
            |constant| self.fits_constant(constant, target),
        )
    }

    fn fits_path(&self, source: &str, names: &[String], target: &str) -> Result<(), String> {
        let (found, maybe_empty) = self.resolve(source, names)?;
        found.map_or(Ok(()), |found| self.fits(&found, target, maybe_empty, 0))
    }

    fn resolve(&self, start: &str, names: &[String]) -> Result<(Option<String>, bool), String> {
        names.iter().try_fold(
            (Some(start.to_owned()), false),
            |(current, maybe_empty), name| {
                current.map_or(Ok((None, true)), |current| {
                    self.step(&current, maybe_empty, name)
                })
            },
        )
    }

    fn step(
        &self,
        current: &str,
        maybe_empty: bool,
        name: &str,
    ) -> Result<(Option<String>, bool), String> {
        let (held, optional) = Self::unwrapped(current);
        let empty = [maybe_empty, optional].contains(&true);
        let found = (!Self::is_json(&held))
            .then(|| self.field_type(&held, name))
            .transpose()?;
        Ok(found.map_or((None, true), |found| (Some(found), empty)))
    }

    fn field_type(&self, held: &str, name: &str) -> Result<String, String> {
        let model = self
            .structure(held)
            .ok_or_else(|| format!("{held} is not a model, so it has no field {name}"))?;
        model
            .fields()
            .into_iter()
            .find(|(field, _)| field == name)
            .map(|(_, field_type)| CppTypeText::plain(&field_type))
            .ok_or_else(|| format!("{held} has no field {name}"))
    }

    fn fits(
        &self,
        source: &str,
        target: &str,
        maybe_empty: bool,
        depth: usize,
    ) -> Result<(), String> {
        let (source_held, source_optional) = Self::unwrapped(source);
        let (target_held, target_optional) = Self::unwrapped(target);
        let empty = [source_optional, maybe_empty].contains(&true);
        let required = [empty, !target_optional] == [true, true];
        let same = [Self::is_json(&source_held), source_held == target_held].contains(&true);
        (depth > Self::MAX_DEPTH)
            .then(|| Err(Self::DEEP.to_owned()))
            .or_else(|| Self::is_json(&target_held).then_some(Ok(())))
            .or_else(|| required.then(|| Err(Self::MAYBE_EMPTY.to_owned())))
            .or_else(|| same.then_some(Ok(())))
            .or_else(|| self.fits_kind(&source_held, &target_held))
            .or_else(|| self.fits_inside(&source_held, &target_held, depth))
            .unwrap_or_else(|| self.fits_structure(&source_held, &target_held, depth))
    }

    fn fits_kind(&self, source: &str, target: &str) -> Option<Result<(), String>> {
        self.kind(source).zip(self.kind(target)).map(|(from, to)| {
            Self::same_kind(from, to)
                .then_some(())
                .ok_or_else(|| format!("{source} cannot fill {target}"))
        })
    }

    fn fits_inside(&self, source: &str, target: &str, depth: usize) -> Option<Result<(), String>> {
        let lists = Self::inside(source, Self::LIST).zip(Self::inside(target, Self::LIST));
        let maps = Self::map_value(source).zip(Self::map_value(target));
        lists
            .or(maps)
            .map(|(from, to)| self.fits(&from, &to, false, depth + 1))
    }

    fn fits_structure(&self, source: &str, target: &str, depth: usize) -> Result<(), String> {
        self.structure(source)
            .zip(self.structure(target))
            .map_or_else(
                || Err(format!("{source} cannot fill {target}")),
                |(from, to)| self.fits_model(from, to, depth),
            )
    }

    fn fits_model(&self, source: &CppModel, target: &CppModel, depth: usize) -> Result<(), String> {
        let source_fields = source.fields();
        target
            .fields()
            .into_iter()
            .try_for_each(|(field, field_type)| {
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
                .map_err(|reason| format!("{field}: {reason}"))
            })
    }

    fn fits_constant(&self, constant: &Value, target: &str) -> Result<(), String> {
        let (held, optional) = Self::unwrapped(target);
        let from = Self::constant_kind(constant);
        let to = self.kind(&held);
        let filled = to.is_some_and(|to| Self::same_kind(from, to));
        Self::is_json(&held)
            .then_some(Ok(()))
            .or_else(|| {
                constant.is_null().then(|| {
                    optional
                        .then_some(())
                        .ok_or_else(|| String::from("null needs a std::optional target field"))
                })
            })
            .unwrap_or_else(|| {
                filled
                    .then_some(())
                    .ok_or_else(|| format!("the constant {constant} cannot fill {held}"))
            })
    }

    fn constant_kind(constant: &Value) -> &'static str {
        match constant {
            Value::Bool(_) => "truth",
            Value::Number(number) if number.is_i64() || number.is_u64() => "whole",
            Value::Number(_) => "number",
            _ => "text",
        }
    }

    fn same_kind(from: &str, to: &str) -> bool {
        [from == to, [from, to] == ["whole", "number"]].contains(&true)
    }

    fn kind(&self, held: &str) -> Option<&'static str> {
        Self::KINDS
            .iter()
            .find(|(names, _)| names.contains(&held))
            .map(|(_, kind)| *kind)
            .or_else(|| {
                self.model(held)
                    .filter(|model| model.is_enum())
                    .map(|_| "text")
            })
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
        member
            .parameters
            .first()
            .map(|parameter| Self::defined(class, member, parameter, conversion))
            .unwrap_or_default()
    }

    fn defined(
        class: &str,
        member: &CppMember,
        parameter: &CppParameter,
        conversion: &CppConversion,
    ) -> String {
        let constant = [" ", " const "][usize::from(member.is_const)];
        let bodies = [Self::json_body, Self::typed_body];
        let body = bodies[usize::from(conversion.typed)](member, parameter, conversion);
        format!(
            "inline {} {class}::{}({} {}){constant}{{\n{body}    return result;\n}}\n",
            member.type_text, member.name, parameter.type_text, parameter.name
        )
    }

    fn typed_body(
        member: &CppMember,
        parameter: &CppParameter,
        conversion: &CppConversion,
    ) -> String {
        let value = &parameter.name;
        let source = CppTypeText::plain(&parameter.type_text);
        let declared = format!("    {} result{{}};\n", member.plain_type());
        let nested = conversion.fills.iter().any(|(_, fill)| fill.path.len() > 1);
        let read: String = nested
            .then(|| {
                format!(
                    "    const ::tachyon::json::JsonNode {} = ::tachyon::json::JsonCodec<{source}>::toJson({value});\n",
                    Self::SOURCE
                )
            })
            .into_iter()
            .collect();
        let fields: String = conversion
            .fills
            .iter()
            .map(|(field, fill)| {
                format!(
                    "    result.{field} = ::tachyon::TacConvert::{};\n",
                    Self::typed_fill(field, value, fill)
                )
            })
            .collect::<Vec<_>>()
            .concat();
        format!("{declared}{read}{fields}")
    }

    fn typed_fill(field: &str, value: &str, fill: &CppFill) -> String {
        fill.constant.as_ref().map_or_else(
            || Self::typed_path(field, value, &fill.path),
            |constant| {
                format!(
                    "constant<decltype(result.{field})>({})",
                    Self::literal(&constant.to_string())
                )
            },
        )
    }

    fn typed_path(field: &str, value: &str, names: &[String]) -> String {
        match names {
            [name] => format!("copy<decltype(result.{field})>({value}.{name})"),
            _ => format!(
                "read<decltype(result.{field})>({}, {})",
                Self::SOURCE,
                Self::names(names)
            ),
        }
    }

    fn json_body(
        _member: &CppMember,
        parameter: &CppParameter,
        conversion: &CppConversion,
    ) -> String {
        let value = &parameter.name;
        let fields: String = conversion
            .fills
            .iter()
            .map(|(field, fill)| {
                format!(
                    "    result[{}] = {};\n",
                    Self::literal(field),
                    Self::json_fill(value, fill)
                )
            })
            .collect::<Vec<_>>()
            .concat();
        format!("    ::tachyon::json::JsonObject result;\n{fields}")
    }

    fn json_fill(value: &str, fill: &CppFill) -> String {
        fill.constant.as_ref().map_or_else(
            || {
                format!(
                    "::tachyon::TacConvert::at({value}, {})",
                    Self::names(&fill.path)
                )
            },
            |constant| {
                format!(
                    "::tachyon::json::JsonNode::parse({})",
                    Self::literal(&constant.to_string())
                )
            },
        )
    }

    fn names(names: &[String]) -> String {
        let written: Vec<String> = names.iter().map(|name| Self::literal(name)).collect();
        format!("{{{}}}", written.join(", "))
    }

    fn literal(text: &str) -> String {
        Value::String(text.to_owned()).to_string()
    }
}
