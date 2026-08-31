//! What a native host needs to know about a project's routes.
//!
//! This replaced a planner that lowered every view into per-platform widgets.
//! The ambition there was that a Tac view would become real `SwiftUI`, GTK,
//! Win32 and Android controls, with a web fallback wherever no adapter
//! existed. Dogfooding the framework's own site settled it: five routes
//! produced two adapters — a column and a line of text — against twenty-three
//! fallbacks, because a real design is built from components the adapter table
//! has never heard of. Every one of those fallbacks then had to be given an
//! accessible name it did not need, and the result still looked nothing like
//! the site.
//!
//! A native target now hosts the application's own web bundle, so it renders
//! exactly as the browser does and looks the same on every platform. Matching
//! the operating system's own controls is a choice the developer makes with
//! whichever cross-platform UI they already use, not something the framework
//! imposes and then approximates.
//!
//! What is left is this: which routes exist, and which document each one
//! loads.

use crate::Failure;
use crate::failure::diagnostic;
use serde::Serialize;
use std::fmt::Write as _;

/// One route a native host can open.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct NativeRouteIndexEntry {
    pub(super) route: String,
    /// The document to load, relative to the staged `WebBundle`.
    pub(super) document: String,
}

/// The routes a native application ships, and where each one lives.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct NativeRouteIndex {
    pub(super) contract_version: u8,
    pub(super) entry_route: String,
    pub(super) entry_document: String,
    pub(super) routes: Vec<NativeRouteIndexEntry>,
}

impl NativeRouteIndex {
    /// Builds the index from the route graph and the web bundle it produced.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the entry route names no document, which
    /// leaves the host with nothing to open.
    pub(super) fn build(routes: &[crate::RouteNode], entry_route: &str) -> Result<Self, Failure> {
        let mut entries: Vec<NativeRouteIndexEntry> = routes
            .iter()
            .filter_map(|route| {
                route
                    .template_output_path()
                    .map(|output| NativeRouteIndexEntry {
                        route: String::from(route.route()),
                        document: crate::compiler::portable_path(std::path::Path::new(&output)),
                    })
            })
            .collect();
        entries.sort_by(|left, right| left.route.cmp(&right.route));

        let entry_document = entries
            .iter()
            .find(|entry| entry.route == entry_route)
            .map(|entry| entry.document.clone())
            .ok_or_else(|| {
                Failure::one(diagnostic(
                    1601,
                    format!("Native entry route '{entry_route}' has no document to open."),
                    Some(String::from(
                        "Point entryRoute at a route with a tac.html, so the \
                         application has something to load.",
                    )),
                    None,
                ))
            })?;

        Ok(Self {
            // Version 2 drops the Native UI tree: a host loads a document
            // rather than rebuilding the view out of platform widgets.
            contract_version: 2,
            entry_route: String::from(entry_route),
            entry_document,
            routes: entries,
        })
    }
}

/// Generates only escaped, compiler-owned route/document pairs for Apple hosts.
pub(super) fn swift_routes(index: &NativeRouteIndex) -> String {
    let pairs = index
        .routes
        .iter()
        .map(|entry| {
            format!(
                "\"{}\": \"{}\"",
                super::host::quoted_string_escape(&entry.route),
                super::host::quoted_string_escape(&entry.document)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{pairs}]")
}

pub(super) fn route_documents_json(index: &NativeRouteIndex) -> String {
    let map = index
        .routes
        .iter()
        .map(|entry| (&entry.route, &entry.document))
        .collect::<std::collections::BTreeMap<_, _>>();
    serde_json::to_string(&map).unwrap_or_else(|_| String::from("{}"))
}

/// Emits bounded local-origin routing shared by the C `WebView` hosts.
pub(super) fn c_local_bundle(
    index: &NativeRouteIndex,
    companions: &[super::registry::NativeCompanionInput],
    origin: &str,
) -> String {
    let mut entries = index.routes.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .route
            .split('/')
            .count()
            .cmp(&left.route.split('/').count())
            .then_with(|| {
                left.route
                    .matches("/_")
                    .count()
                    .cmp(&right.route.matches("/_").count())
            })
            .then_with(|| left.route.cmp(&right.route))
    });
    let entries = entries
        .into_iter()
        .fold(String::new(), |mut output, entry| {
            let language = companions
                .iter()
                .find(|item| item.route == entry.route)
                .map_or(0, |item| match item.language {
                    crate::project::NativeCompanion::Rust => 1,
                    crate::project::NativeCompanion::CSharp => 2,
                    _ => 0,
                });
            let _ = writeln!(
                output,
                "  {{\"{}\", \"{}\", {language}}},",
                super::host::c_string_escape(&entry.route),
                super::host::c_string_escape(&entry.document)
            );
            output
        });
    include_str!("local_bundle.h")
        .replace("__LOCAL_ORIGIN__", origin)
        .replace("__LOCAL_ROUTES__", &entries)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::NativeRouteIndex;

    #[test]
    fn an_entry_route_without_a_document_is_a_diagnostic() {
        // A host with nothing to open is a window that never draws, and the
        // build is the last place that can say so.
        let failure = NativeRouteIndex::build(&[], "/").expect_err("no routes");
        assert!(failure.to_string().contains("TY1601"), "{failure}");
    }

    #[cfg(unix)]
    #[test]
    fn c_host_route_boundary_executes_against_hostile_urls_and_payloads() {
        let directory = tempfile::tempdir().expect("C boundary fixture");
        let index = NativeRouteIndex {
            contract_version: 2,
            entry_route: String::from("/"),
            entry_document: String::from("index.html"),
            routes: vec![
                super::NativeRouteIndexEntry {
                    route: String::from("/"),
                    document: String::from("index.html"),
                },
                super::NativeRouteIndexEntry {
                    route: String::from("/items/_id"),
                    document: String::from("items/_id/index.html"),
                },
                super::NativeRouteIndexEntry {
                    route: String::from("/items/special"),
                    document: String::from("items/special/index.html"),
                },
            ],
        };
        let helper = super::c_local_bundle(&index, &[], "https://tachyon.local");
        let harness = r#"
int main(void) {
  char path[4096], output[4096];
  assert(tachyon_local_path("https://tachyon.local/items/7/?q=1",path,sizeof(path)));
  assert(strcmp(path,"/items/7/")==0);
  assert(strcmp(tachyon_document_route(path)->route,"/items/_id")==0);
  assert(tachyon_bundle_path(path,output,sizeof(output)) && strcmp(output,"items/_id/index.html")==0);
  assert(tachyon_bundle_path("/items/7/client.js",output,sizeof(output)) && strcmp(output,"items/_id/client.js")==0);
  assert(strcmp(tachyon_document_route("/items/special/")->route,"/items/special")==0);
  assert(tachyon_bundle_path("/shared/site.css",output,sizeof(output)) && strcmp(output,"shared/site.css")==0);
  const char *hostile[]={"https://tachyon.local.evil/","https://tachyon.local@evil/","http://tachyon.local/","file:///etc/passwd","https://tachyon.local/%2e%2e/secrets","https://tachyon.local/a%2fb","https://tachyon.local/%00","https://tachyon.local/%","https://tachyon.local/a\\b","https://tachyon.local/a//b"};
  for(size_t i=0;i<sizeof(hostile)/sizeof(hostile[0]);i++) assert(!tachyon_local_path(hostile[i],path,sizeof(path)));
  assert(tachyon_payload_route_matches("{\"op\":\"get\",\"route\":\"/items/_id\"}","/items/_id"));
  assert(!tachyon_payload_route_matches("{\"nested\":{\"route\":\"/items/_id\"}}","/items/_id"));
  assert(!tachyon_payload_route_matches("{\"route\":\"/other\"}","/items/_id"));
  assert(!tachyon_payload_route_matches("{\"route\":\"/items/_id\",\"route\":\"/other\"}","/items/_id"));
  assert(!tachyon_payload_route_matches("{\"route\":\"/items/_id\",\"\\u0072oute\":\"/other\"}","/items/_id"));
  assert(!tachyon_payload_route_matches("{\"route\":\"/items/_id", "/items/_id"));
  assert(tachyon_payload_string_matches("{\"op\":\"init\",\"route\":\"/\"}","op","init"));
  return 0;
}
"#;
        let source = directory.path().join("boundary.c");
        let binary = directory.path().join("boundary");
        std::fs::write(
            &source,
            format!(
                "#include <stdio.h>\n#include <string.h>\n#include <assert.h>\n{helper}\n{harness}"
            ),
        )
        .expect("fixture");
        let compile = std::process::Command::new("cc")
            .args(["-std=c17", "-Wall", "-Wextra", "-Werror"])
            .arg(&source)
            .arg("-o")
            .arg(&binary)
            .output()
            .expect("compile C boundary");
        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        assert!(
            std::process::Command::new(&binary)
                .status()
                .expect("C boundary execution")
                .success()
        );
    }
}
