use crate::Failure;
use crate::failure::diagnostic;
use serde::Serialize;
use std::cmp::Ordering;
use std::fmt::Write as _;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct NativeRouteIndexEntry {
    pub(super) route: String,
    pub(super) document: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct NativeRouteIndex {
    pub(super) contract_version: u8,
    pub(super) entry_route: String,
    pub(super) entry_document: String,
    pub(super) routes: Vec<NativeRouteIndexEntry>,
}

impl NativeRouteIndex {
    pub(super) fn build(routes: &[crate::RouteNode], entry_route: &str) -> Result<Self, Failure> {
        let mut entries = Vec::new();
        for route in routes {
            let Some(output) = route.template_output_path() else {
                continue;
            };
            entries.push(NativeRouteIndexEntry {
                route: String::from(route.route()),
                document: crate::compiler::portable_path(std::path::Path::new(&output)),
            });
        }
        entries.sort_by(|left, right| left.route.cmp(&right.route));
        let entry_document = Self::entry_document(&entries, entry_route)?;
        Ok(Self {
            contract_version: 2,
            entry_route: String::from(entry_route),
            entry_document,
            routes: entries,
        })
    }

    fn entry_document(
        entries: &[NativeRouteIndexEntry],
        entry_route: &str,
    ) -> Result<String, Failure> {
        for entry in entries {
            if entry.route == entry_route {
                return Ok(entry.document.clone());
            }
        }
        Err(Failure::one(diagnostic(
            1601,
            format!("Native entry route '{entry_route}' has no document to open."),
            Some(String::from(
                "Point start_url in client/manifest.json at a route with a \
                 tac.html, so the application has something to load.",
            )),
            None,
        )))
    }
}

pub(super) fn swift_routes(index: &NativeRouteIndex) -> String {
    let mut pairs = Vec::new();
    for entry in &index.routes {
        let route = super::host::quoted_string_escape(&entry.route);
        let document = super::host::quoted_string_escape(&entry.document);
        pairs.push(format!("\"{route}\": \"{document}\""));
    }
    format!("[{}]", pairs.join(", "))
}

pub(super) fn route_documents_json(index: &NativeRouteIndex) -> String {
    let mut documents = std::collections::BTreeMap::new();
    for entry in &index.routes {
        documents.insert(&entry.route, &entry.document);
    }
    serde_json::to_string(&documents).unwrap_or_else(|_| String::from("{}"))
}

pub(super) fn c_local_bundle(
    index: &NativeRouteIndex,
    companions: &[super::registry::NativeCompanionInput],
    origin: &str,
) -> String {
    let mut entries = index.routes.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| c_route_order(left, right));
    let language = c_companion_language(companions);
    let mut routes = String::new();
    for entry in entries {
        let _ = writeln!(
            routes,
            "  {{\"{}\", \"{}\", {language}}},",
            super::host::c_string_escape(&entry.route),
            super::host::c_string_escape(&entry.document)
        );
    }
    include_str!("local_bundle.h")
        .replace("__LOCAL_ORIGIN__", origin)
        .replace("__LOCAL_ROUTES__", &routes)
}

fn c_route_order(left: &NativeRouteIndexEntry, right: &NativeRouteIndexEntry) -> Ordering {
    let left_segments = left.route.split('/').count();
    let right_segments = right.route.split('/').count();
    let deeper_first = right_segments.cmp(&left_segments);
    if deeper_first != Ordering::Equal {
        return deeper_first;
    }
    let fewer_parameters_first = parameter_count(&left.route).cmp(&parameter_count(&right.route));
    if fewer_parameters_first != Ordering::Equal {
        return fewer_parameters_first;
    }
    left.route.cmp(&right.route)
}

fn parameter_count(route: &str) -> usize {
    route.matches("/_").count()
}

fn c_companion_language(companions: &[super::registry::NativeCompanionInput]) -> u8 {
    let mut language = 0;
    for item in companions {
        language = language.max(super::service_companions::ServiceCompanions::number(
            item.language,
        ));
    }
    language
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::NativeRouteIndex;
    #[cfg(unix)]
    use super::NativeRouteIndexEntry;

    #[test]
    fn an_entry_route_without_a_document_is_a_diagnostic() {
        let failure = NativeRouteIndex::build(&[], "/").expect_err("no routes");
        assert!(failure.to_string().contains("TY1601"), "{failure}");
    }

    #[cfg(unix)]
    fn route(route: &str, document: &str) -> NativeRouteIndexEntry {
        NativeRouteIndexEntry {
            route: String::from(route),
            document: String::from(document),
        }
    }

    #[cfg(unix)]
    fn boundary_index() -> NativeRouteIndex {
        NativeRouteIndex {
            contract_version: 2,
            entry_route: String::from("/"),
            entry_document: String::from("index.html"),
            routes: vec![
                route("/", "index.html"),
                route("/items/_id", "items/_id/index.html"),
                route("/items/special", "items/special/index.html"),
            ],
        }
    }

    #[cfg(unix)]
    fn boundary_harness() -> &'static str {
        r#"
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
"#
    }

    #[cfg(unix)]
    fn companion(
        service: &str,
        language: crate::project::NativeCompanion,
    ) -> super::super::registry::NativeCompanionInput {
        super::super::registry::NativeCompanionInput {
            snapshot_root: std::path::PathBuf::new(),
            language,
            source: std::path::PathBuf::new(),
            service: String::from(service),
            class: String::new(),
            web_assembly: false,
            application_provider: false,
        }
    }

    #[cfg(unix)]
    fn service_companion_harness() -> &'static str {
        r#"
int main(void) {
  (void)tachyon_local_path;
  (void)tachyon_bundle_path;
  (void)tachyon_payload_route_matches;
  assert(tachyon_service_companion("{\"op\":\"get\",\"route\":\"/\",\"service\":\"gateRoot\"}")==1);
  assert(tachyon_service_companion("{\"op\":\"call\",\"service\":\"gateSecond\",\"name\":\"doubled\"}")==2);
  assert(tachyon_service_companion("{\"op\":\"init\",\"service\":\"gateMissing\"}")==0);
  assert(tachyon_service_companion("{\"op\":\"init\",\"service\":\"webOnly\"}")==0);
  assert(tachyon_service_companion("{\"op\":\"init\",\"service\":\"applicationClock\"}")==0);
  assert(tachyon_service_companion("{\"props\":{\"service\":\"gateRoot\"},\"service\":\"gateSecond\"}")==2);
  assert(tachyon_service_companion("{\"service\":\"gateRoot\",\"service\":\"gateSecond\"}")==0);
  assert(tachyon_service_companion("{\"service\":\"gate\\u0052oot\"}")==0);
  return 0;
}
"#
    }

    #[cfg(unix)]
    #[test]
    fn each_native_call_reaches_the_companion_that_owns_its_service() {
        use crate::project::NativeCompanion;
        let mut web_only = companion("webOnly", NativeCompanion::CSharp);
        web_only.web_assembly = true;
        let mut provider = companion("applicationClock", NativeCompanion::Rust);
        provider.application_provider = true;
        let companions = [
            companion("gateRoot", NativeCompanion::Rust),
            companion("gateSecond", NativeCompanion::CSharp),
            companion("appleOnly", NativeCompanion::Swift),
            web_only,
            provider,
        ];
        let helper = super::c_local_bundle(&boundary_index(), &companions, "https://tachyon.local");
        let services = super::super::service_companions::ServiceCompanions::c_table(&companions);
        assert!(!services.contains("appleOnly"), "{services}");
        let directory = tempfile::tempdir().expect("C dispatch fixture");
        let source = directory.path().join("dispatch.c");
        let binary = directory.path().join("dispatch");
        std::fs::write(
            &source,
            format!(
                "#include <stdio.h>\n#include <string.h>\n#include <assert.h>\n{helper}\n{services}\n{}",
                service_companion_harness()
            ),
        )
        .expect("fixture");
        let compile = compile_c(&source, &binary);
        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = std::process::Command::new(&binary)
            .status()
            .expect("C dispatch execution");
        assert!(run.success());
    }

    #[cfg(unix)]
    fn compile_c(source: &std::path::Path, binary: &std::path::Path) -> std::process::Output {
        std::process::Command::new("cc")
            .args(["-std=c17", "-Wall", "-Wextra", "-Werror"])
            .arg(source)
            .arg("-o")
            .arg(binary)
            .output()
            .expect("compile C boundary")
    }

    #[cfg(unix)]
    #[test]
    fn c_host_route_boundary_executes_against_hostile_urls_and_payloads() {
        let directory = tempfile::tempdir().expect("C boundary fixture");
        let helper = super::c_local_bundle(&boundary_index(), &[], "https://tachyon.local");
        let harness = boundary_harness();
        let source = directory.path().join("boundary.c");
        let binary = directory.path().join("boundary");
        std::fs::write(
            &source,
            format!(
                "#include <stdio.h>\n#include <string.h>\n#include <assert.h>\n{helper}\n{harness}"
            ),
        )
        .expect("fixture");
        let compile = compile_c(&source, &binary);
        assert!(
            compile.status.success(),
            "{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = std::process::Command::new(&binary)
            .status()
            .expect("C boundary execution");
        assert!(run.success());
    }
}
