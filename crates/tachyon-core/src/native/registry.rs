//! Route-scoped compilation and dispatch for target-native companions.

use crate::Failure;
use crate::project::NativeCompanion;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// One target-selected companion from the immutable project snapshot.
#[derive(Clone, Debug)]
pub(super) struct NativeCompanionInput {
    pub(super) language: NativeCompanion,
    pub(super) source: PathBuf,
    pub(super) route: String,
}

/// Emits isolated language namespaces and an explicit canonical-route table.
pub(super) fn source(
    companions: &[NativeCompanionInput],
    language: NativeCompanion,
) -> Result<Option<String>, Failure> {
    let selected = companions
        .iter()
        .filter(|item| item.language == language)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(None);
    }
    let mut imports = BTreeSet::new();
    let mut namespaces = String::new();
    let mut routes = Vec::new();
    for (index, item) in selected.iter().enumerate() {
        let authored = std::fs::read_to_string(&item.source).map_err(|error| {
            super::host::native_tool_failure(
                1605,
                &format!("Cannot read captured native companion: {error}"),
            )
        })?;
        let authored = crate::without_bom(&authored);
        let portable = item.source.to_string_lossy();
        let table = match language {
            NativeCompanion::Swift => crate::companion::swift_member_table(authored, &portable)?,
            NativeCompanion::Kotlin => crate::companion::kotlin_member_table(authored, &portable)?,
            NativeCompanion::CSharp => crate::companion::csharp_member_table(authored, &portable)?,
            NativeCompanion::Rust => crate::companion::rust_member_table(authored, &portable)?,
        };
        let namespace = format!("TachyonRoute{index}");
        let mut body = String::new();
        for line in authored.lines() {
            if matches!(language, NativeCompanion::Swift | NativeCompanion::Kotlin)
                && line.trim_start().starts_with("import ")
            {
                imports.insert(line.to_owned());
            } else {
                body.push_str(line);
                body.push('\n');
            }
        }
        body.push_str(&table);
        namespaces.push_str(&namespace_source(language, &namespace, &body));
        routes.push((item.route.clone(), namespace));
    }
    let mut result = imports.into_iter().collect::<Vec<_>>().join("\n");
    result.push('\n');
    result.push_str(&namespaces);
    result.push_str(&dispatch(language, &routes));
    Ok(Some(result))
}

fn namespace_source(language: NativeCompanion, namespace: &str, source: &str) -> String {
    match language {
        NativeCompanion::Swift => {
            // Authored classes retain their members; only file-level tables
            // become namespace storage, keeping per-route state independent.
            let source = source
                .replace("private let tacInstance", "private static let tacInstance")
                .replace("\nlet tac:", "\nstatic let tac:")
                .replace("\nlet tac =", "\nstatic let tac =")
                .replace("\nvar tac:", "\nstatic var tac:");
            format!("private enum {namespace} {{\n{source}\n}}\n")
        }
        NativeCompanion::Kotlin => format!("private object {namespace} {{\n{source}\n}}\n"),
        NativeCompanion::CSharp => format!("namespace {namespace} {{\n{source}\n}}\n"),
        NativeCompanion::Rust => {
            format!("#[allow(non_snake_case)]\nmod {namespace} {{\nuse super::*;\n{source}\n}}\n")
        }
    }
}

fn dispatch(language: NativeCompanion, routes: &[(String, String)]) -> String {
    let entries = routes
        .iter()
        .map(|(route, namespace)| {
            let route = serde_json::to_string(route).unwrap_or_else(|_| String::from("\"\""));
            match language {
                NativeCompanion::Swift => format!("case {route}: return {namespace}.tac\n"),
                NativeCompanion::Kotlin => format!("{route} -> {namespace}.tac\n"),
                NativeCompanion::CSharp => format!("{route} => {namespace}.TacBridge.Tac,\n"),
                NativeCompanion::Rust => format!("{route} => Some({namespace}::tac()),\n"),
            }
        })
        .collect::<String>();
    match language {
        NativeCompanion::Swift => format!(
            "\nfunc tacRouteMembers(_ route: String) -> [String: TacMember]? {{\nswitch route {{\n{entries}default: return nil\n}}\n}}\n"
        ),
        NativeCompanion::Kotlin => format!(
            "\nprivate fun tacRouteMembers(route: String): Map<String, Any>? = when (route) {{\n{entries}else -> null\n}}\n"
        ),
        NativeCompanion::CSharp => format!(
            "\ninternal static class TacRoutes {{\ninternal static Dictionary<string, TacMember> Members(string route) => route switch {{\n{entries}_ => null\n}};\n}}\n"
        ),
        NativeCompanion::Rust => format!(
            "\nfn tac_route_members(route: &str) -> Option<Vec<(&'static str, TacMember)>> {{\nmatch route {{\n{entries}_ => None\n}}\n}}\n"
        ),
    }
}
