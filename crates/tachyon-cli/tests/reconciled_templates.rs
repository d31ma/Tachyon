//! Regression gates for view features omitted from the first integration.
#![allow(clippy::expect_used)]

use std::fs;
use std::process::{Command, Output};

fn build(source: &str) -> (tempfile::TempDir, Output) {
    let project = tempfile::tempdir().expect("fixture");
    let pages = project.path().join("client/pages");
    fs::create_dir_all(&pages).expect("pages");
    fs::write(pages.join("tac.html"), source).expect("view");
    let output = Command::new(env!("CARGO_BIN_EXE_ty"))
        .arg("build")
        .arg(project.path())
        .output()
        .expect("compiler");
    (project, output)
}

fn client_plan(project: &tempfile::TempDir) -> serde_json::Value {
    let html = fs::read_to_string(project.path().join("dist/index.html")).expect("bootstrap");
    let (_, rest) = html
        .split_once("data-tachyon-runtime>")
        .expect("plan script");
    let (json, _) = rest.split_once("</script>").expect("plan end");
    serde_json::from_str(json).expect("render plan")
}

#[test]
fn counted_loops_emit_bounded_client_instructions_not_server_rendered_views() {
    let (project, output) =
        build("<main><loop :for=\"let i = 0; i < 3; i++\"><p>{i}</p></loop></main>");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = client_plan(&project);
    let counted = &plan["nodes"][0]["children"][0];
    assert_eq!(counted["k"], "counted");
    assert_eq!(counted["binding"], "i");
    assert_eq!(counted["comparison"], "lt");
    let html = fs::read_to_string(project.path().join("dist/index.html")).expect("bootstrap");
    assert!(!html.contains("<p>0</p>"), "view must remain client-owned");
}

#[test]
fn declared_iterable_bindings_and_legacy_bindings_remain_equivalent() {
    for declaration in ["const item of items", "let item of items", "item of items"] {
        let (project, output) = build(&format!(
            "<main><loop :for=\"{declaration}\"><p>{{item}}</p></loop></main>"
        ));
        assert!(
            output.status.success(),
            "{declaration}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let plan = client_plan(&project);
        assert_eq!(plan["nodes"][0]["children"][0]["binding"], "item");
    }
}

#[test]
fn decreasing_counted_loops_keep_their_direction_in_the_plan() {
    let (project, output) =
        build("<main><loop :for=\"let i = 4; i >= 0; i -= 2\"><p>{i}</p></loop></main>");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        client_plan(&project)["nodes"][0]["children"][0]["comparison"],
        "ge"
    );
}

#[test]
fn invalid_or_nonterminating_counted_declarations_are_rejected() {
    for declaration in [
        "let i = 0; i < 3; i--",
        "let i = 0; i < 3; j++",
        "let i = 0; j < 3; i++",
        "const i = 0; i < 3; i++",
        "let i = 0; i < 3; i += 0",
        "let i = 0; i < 3; i += -2",
        "let i = 0; i < 3; i += 1; run()",
    ] {
        let (_project, output) = build(&format!(
            "<main><loop :for=\"{declaration}\"><p>{{i}}</p></loop></main>"
        ));
        assert!(!output.status.success(), "accepted {declaration}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("TY1302"));
    }
}

#[test]
fn fragment_bootstraps_supply_language_and_mobile_viewport() {
    let (project, output) = build("<main>Mobile</main>");
    assert!(output.status.success());
    let html = fs::read_to_string(project.path().join("dist/index.html")).expect("bootstrap");
    assert!(html.contains("<html lang=\"en\">"));
    assert!(html.contains("name=\"viewport\" content=\"width=device-width, initial-scale=1\""));
}

#[test]
fn html_entities_decode_once_without_becoming_template_code_or_markup() {
    let (project, output) =
        build("<main title=\"a &amp;lt; b\">&amp;lt; &#123;missing&#125; &lt;script&gt;</main>");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = client_plan(&project);
    assert_eq!(plan["nodes"][0]["attributes"][0]["value"], "a &lt; b");
    assert_eq!(
        plan["nodes"][0]["children"][0]["parts"][0]["value"],
        "&lt; {missing} <script>"
    );
}

#[test]
fn utf8_bom_does_not_become_visible_template_content() {
    let (project, output) = build("\u{feff}<main>ready</main>");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(client_plan(&project)["nodes"][0]["tag"], "main");
}
