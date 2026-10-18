use super::{CppAttribute, CppClass, CppFailure, CppMember, CppMemberKind};
use crate::Failure;
use crate::behavior::{WrittenMarker, WrittenPolicies, WrittenQuery};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::Path;

pub(crate) struct CppStore;

impl CppStore {
    const STORE: &'static str = "Store";
    const ENTITY: &'static str = "Entity";
    const TABLE: &'static str = "Table";
    const QUERY: &'static str = "Query";
    const PAGE: &'static str = "PageDefaults";
    const ADAPTER: &'static str = "indexeddb";
    const CRUD: [&'static str; 3] = ["findById", "findAll", "deleteById"];
    const INTEGERS: [&'static str; 7] = [
        "int",
        "long",
        "longlong",
        "std::int32_t",
        "std::int64_t",
        "std::size_t",
        "unsigned",
    ];
    const ADAPTERS: &'static str = "A C++ Store keeps its rows in the browser's IndexedDB for now: write [[Entity({\"adapter\":\"indexeddb\"})]]. Other adapters need a C++ [[Setup]], which comes later (ADR 0099)";
    const NEEDS_TABLE: &'static str = "A C++ [[Entity]] names its table, for example `struct [[Entity({\"adapter\":\"indexeddb\"}), Table(\"notes\")]] NoteEntity`";
    const ONE_ID: &'static str = "A C++ [[Entity]] has exactly one public field marked [[Id]], for example `[[Id]] std::string id;`";
    const QUERY_ONLY: &'static str = "A C++ Store holds only queries: instance methods with no body that Tachyon writes, for example `std::vector<NoteEntity> findByText(const std::string& text);`. Put other work in a service";
    const QUERY_MARKERS: &'static str = "A C++ Store method carries only [[Query]], and its page parameter only a bare [[PageDefaults]]";
    const OVERLOADED: &'static str = "A C++ Store method has one name; rename one of the methods";
    const NO_ENTITY: &'static str =
        "A C++ [[Store]] names an [[Entity]] model in client/entities or client/models";

    pub(crate) fn is_store(class: &CppClass) -> bool {
        Self::marker(class, Self::STORE).is_some()
    }

    pub(crate) fn entity(
        path: &Path,
        source: &str,
        class: &CppClass,
    ) -> Result<Option<Value>, Failure> {
        let Some(marker) = Self::marker(class, Self::ENTITY) else {
            return Ok(None);
        };
        let written = WrittenPolicies::new(path, source, super::CppSource::EXTENSION);
        let mut entity = written.entity(&Self::written(marker))?;
        if entity["adapter"] != Self::ADAPTER {
            return Err(CppFailure::pending(path, marker.at, Self::ADAPTERS));
        }
        let table = Self::marker(class, Self::TABLE)
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::NEEDS_TABLE))?;
        let table = written.table(&Self::written(table))?;
        let ids: Vec<&CppMember> = class
            .members
            .iter()
            .filter(|member| member.marked("Id"))
            .collect();
        let [id] = ids.as_slice() else {
            return Err(CppFailure::unreadable(path, marker.at, Self::ONE_ID));
        };
        if !id.is_public_instance(CppMemberKind::Field) {
            return Err(CppFailure::unreadable(path, id.name_at, Self::ONE_ID));
        }
        entity["name"] = json!(class.name);
        entity["id"] = json!(id.name);
        entity["table"] = json!(table);
        Ok(Some(entity))
    }

    pub(crate) fn check(path: &Path, source: &str, class: &CppClass) -> Result<(), Failure> {
        let Some(marker) = Self::marker(class, Self::STORE) else {
            return Ok(());
        };
        let written = WrittenPolicies::new(path, source, super::CppSource::EXTENSION);
        written.store(&Self::written(marker))?;
        let mut names: Vec<&str> = Vec::new();
        for member in &class.members {
            Self::check_member(path, source, member)?;
            if names.contains(&member.name.as_str()) {
                return Err(CppFailure::unreadable(
                    path,
                    member.name_at,
                    Self::OVERLOADED,
                ));
            }
            names.push(&member.name);
            if let Some(query) = Self::marker_of(&member.markers, Self::QUERY) {
                written.query(&Self::written(query))?;
            }
        }
        Ok(())
    }

    pub(crate) fn definitions(
        path: &Path,
        source: &str,
        class: &CppClass,
        entities: &[Value],
    ) -> Result<Option<(String, String)>, Failure> {
        let Some(marker) = Self::marker(class, Self::STORE) else {
            return Ok(None);
        };
        let written = WrittenPolicies::new(path, source, super::CppSource::EXTENSION);
        let store = written.store(&Self::written(marker))?;
        let entity = entities
            .iter()
            .find(|entity| entity["name"] == store["entity"])
            .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::NO_ENTITY))?;
        let mut text = Self::plan(&written, marker.at, &store, entity)?;
        for member in &class.members {
            let query = Self::marker_of(&member.markers, Self::QUERY)
                .map(|query| written.query(&Self::written(query)))
                .transpose()?;
            let page = member
                .parameters
                .iter()
                .position(|parameter| Self::marker_of(&parameter.markers, Self::PAGE).is_some())
                .map(|index| json!(index));
            let resolved = written.resolve_query(
                &store,
                entity,
                &WrittenQuery {
                    at: member.name_at,
                    method: &member.name,
                    arity: member.parameters.len(),
                    query: query.as_ref(),
                    page: page.as_ref(),
                },
            )?;
            Self::check_returns(path, member, &resolved, entity)?;
            text.push_str(&Self::definition(&class.name, member, &resolved));
        }
        let name = entity["name"].as_str().unwrap_or_default().to_owned();
        Ok(Some((name, text)))
    }

    fn check_member(path: &Path, source: &str, member: &CppMember) -> Result<(), Failure> {
        let bodyless = source.as_bytes().get(member.terminator) == Some(&b';');
        let named = member
            .parameters
            .iter()
            .all(|parameter| !parameter.name.is_empty());
        let query = [
            member.is_instance(CppMemberKind::Method),
            bodyless,
            !member.is_task(),
            named,
        ] == [true; 4];
        query
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, member.name_at, Self::QUERY_ONLY))?;
        let method_markers = member
            .markers
            .iter()
            .filter(|marker| super::CppSpelling::of(&marker.name) == super::CppSpelling::Exact)
            .all(|marker| marker.name == Self::QUERY);
        let page_markers: Vec<&CppAttribute> = member
            .parameters
            .iter()
            .flat_map(|parameter| parameter.markers.iter())
            .filter(|marker| super::CppSpelling::of(&marker.name) == super::CppSpelling::Exact)
            .collect();
        let bare_page = page_markers
            .iter()
            .all(|marker| [marker.name == Self::PAGE, !marker.has_arguments()] == [true, true]);
        let fits = [method_markers, bare_page, page_markers.len() <= 1] == [true; 3];
        fits.then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, member.name_at, Self::QUERY_MARKERS))
    }

    fn plan(
        written: &WrittenPolicies<'_>,
        at: usize,
        store: &Value,
        entity: &Value,
    ) -> Result<String, Failure> {
        let mut functions = String::new();
        for method in Self::CRUD {
            let resolved = written.resolve_query(
                store,
                entity,
                &WrittenQuery {
                    at,
                    method,
                    arity: 1,
                    query: None,
                    page: None,
                },
            )?;
            let literal = Value::String(resolved.to_string()).to_string();
            let _ = write!(
                functions,
                "    static const json::JsonNode& {method}() {{\n        static const json::JsonNode plan = json::JsonNode::parse({literal});\n        return plan;\n    }}\n"
            );
        }
        let name = entity["name"].as_str().unwrap_or_default();
        Ok(format!(
            "namespace tachyon {{\ntemplate <>\nstruct TacStorePlan<::{name}> {{\n{functions}}};\n}}\n"
        ))
    }

    fn check_returns(
        path: &Path,
        member: &CppMember,
        plan: &Value,
        entity: &Value,
    ) -> Result<(), Failure> {
        let name = entity["name"].as_str().unwrap_or_default();
        let operation = plan["operation"].as_str().unwrap_or_default();
        let wanted: Vec<String> = match operation {
            "find" => vec![format!("std::vector<{name}>")],
            "findOne" => vec![format!("std::optional<{name}>")],
            "exists" => vec![String::from("bool")],
            _ => Self::INTEGERS
                .iter()
                .map(|text| (*text).to_owned())
                .collect(),
        };
        let written: String = member
            .plain_type()
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        if wanted.contains(&written) {
            return Ok(());
        }
        let shown = wanted
            .iter()
            .map(|text| text.replace("longlong", "long long"))
            .collect::<Vec<_>>()
            .join(" or ");
        Err(CppFailure::unreadable(
            path,
            member.name_at,
            &format!(
                "The C++ Store method {} runs a {operation} query, so it returns {shown}",
                member.name
            ),
        ))
    }

    fn definition(class: &str, member: &CppMember, plan: &Value) -> String {
        let parameters = member
            .parameters
            .iter()
            .map(|parameter| format!("{} {}", parameter.type_text, parameter.name))
            .collect::<Vec<_>>()
            .join(", ");
        let encoded = member
            .parameters
            .iter()
            .map(|parameter| {
                format!(
                    "::tachyon::json::JsonCodec<std::decay_t<decltype({0})>>::toJson({0})",
                    parameter.name
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let constant = [" ", " const "][usize::from(member.is_const)];
        let literal = Value::String(plan.to_string()).to_string();
        format!(
            "inline {} {class}::{}({parameters}){constant}{{\n    static const ::tachyon::json::JsonNode tachyonQuery = ::tachyon::json::JsonNode::parse({literal});\n    return ::tachyon::TacStore::query<{}>(tachyonQuery, {{{encoded}}});\n}}\n",
            member.type_text,
            member.name,
            member.plain_type()
        )
    }

    fn marker<'c>(class: &'c CppClass, name: &str) -> Option<&'c CppAttribute> {
        Self::marker_of(&class.markers, name)
    }

    fn marker_of<'c>(markers: &'c [CppAttribute], name: &str) -> Option<&'c CppAttribute> {
        markers.iter().find(|marker| marker.name == name)
    }

    fn written(marker: &CppAttribute) -> WrittenMarker<'_> {
        WrittenMarker {
            name: &marker.name,
            arguments: &marker.arguments,
            at: marker.at,
        }
    }
}
