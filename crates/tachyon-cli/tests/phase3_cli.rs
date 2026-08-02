//! Phase 3 behavior tests against the compiled `ty` executable.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn ty() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ty"))
}

fn run(command: &mut Command) -> Output {
    command.output().expect("the ty process should start")
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture directory");
    fs::write(path, contents).expect("fixture source");
}

#[test]
fn client_shared_assets_are_published_at_the_stable_shared_path() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        r#"<main><img src="/shared/assets/mark.svg" alt="mark"></main>"#,
    );
    write(
        &project.path().join("client/shared/assets/mark.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n",
    );
    write(
        &project.path().join("client/shared/data/example.json"),
        "{\"ready\":true}\n",
    );
    write(
        &project.path().join("client/shared/scripts/imports.js"),
        "import '../styles/site.css'\nimport './runtime.js'\n",
    );
    write(
        &project.path().join("client/shared/styles/site.css"),
        "body { color: teal; }\n",
    );

    let output = run(ty().arg("build").arg(project.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/assets/mark.svg"))
            .expect("published SVG"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/data/example.json"))
            .expect("published JSON"),
        "{\"ready\":true}\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/scripts/imports.js"))
            .expect("published browser entry"),
        "import './runtime.js'\n"
    );
    assert_eq!(
        fs::read_to_string(project.path().join("dist/shared/styles/site.css"))
            .expect("published stylesheet"),
        "body { color: teal; }\n"
    );
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn yon_context_controls_components_ir_and_source_maps_form_one_vertical_slice() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/products/yon.html"),
        r#"
<main :data-title="title">
  <h1>{title}</h1>
  <logic :if="featured"><p>Featured</p></logic>
  <logic else><p>Standard</p></logic>
  <ul>
    <loop :for="product of products">
      <li>
        <product-card :product="product" :currency="currency">
          <strong>{product.sku}</strong>
        </product-card>
      </li>
    </loop>
  </ul>
  <w-badge state="ok"></w-badge>
</main>
"#,
    );
    write(
        &project.path().join("server/routes/products/yon.js"),
        r#"
export class Handler {
  static title = 'Products "Summer"'

  static async GET() {
    return {
      featured: true,
      products: [{ sku: 'A-1', name: '<unsafe>' }],
    }
  }
}
"#,
    );
    write(
        &project.path().join("server/routes/products/yon.py"),
        r#"
class Handler:
    currency = "CAD"

    @staticmethod
    async def GET(request):
        return {"meta": {"route": request["route"]}}
"#,
    );
    write(
        &project
            .path()
            .join("client/components/product/card/tac.html"),
        r#"<article :data-sku="product.sku" :data-product="product"><slot></slot><span>{product.name}</span><span>{currency}</span></article>"#,
    );

    let output = run(ty().arg("build").arg(project.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("compiled=1 reused=0"));

    let html =
        fs::read_to_string(project.path().join("dist/products/index.html")).expect("rendered page");
    assert!(html.contains(r#"data-title="Products &quot;Summer&quot;""#));
    assert!(html.contains("<h1>Products &quot;Summer&quot;</h1>"));
    assert!(html.contains("<p>Featured</p>"));
    assert!(!html.contains("<p>Standard</p>"));
    assert!(html.contains(r#"data-sku="A-1""#));
    assert!(html.contains(r#"data-product="{&quot;name&quot;:&quot;\u003cunsafe&gt;&quot;,&quot;sku&quot;:&quot;A-1&quot;}""#));
    assert!(html.contains("<strong>A-1</strong>"));
    assert!(html.contains("<span>&lt;unsafe&gt;</span><span>CAD</span>"));
    assert!(html.contains("<w-badge"));
    for control in ["<logic", "<loop", "<if", "<else", "<for"] {
        assert!(!html.contains(control), "{control} leaked into HTML");
    }

    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(project.path().join("dist/route-manifest.json")).expect("manifest"),
    )
    .expect("manifest JSON");
    let route = &manifest["routes"][0];
    assert_eq!(
        route["context"]["static_exports"],
        serde_json::json!(["currency", "title"])
    );
    assert_eq!(
        route["context"]["response_exports"],
        serde_json::json!(["featured", "meta", "products"])
    );

    let ir: serde_json::Value = serde_json::from_slice(
        &fs::read(project.path().join("dist/.tachyon/view-ir/products.json")).expect("View IR"),
    )
    .expect("View IR JSON");
    assert_eq!(ir["contract_version"], 1);
    assert_eq!(ir["source"], "server/routes/products/yon.html");
    let ir_text = serde_json::to_string(&ir).expect("canonical IR");
    assert!(!ir_text.contains(r#""tag":"logic""#));
    assert!(!ir_text.contains(r#""tag":"loop""#));

    let source_map: serde_json::Value = serde_json::from_slice(
        &fs::read(
            project
                .path()
                .join("dist/.tachyon/source-maps/products.map.json"),
        )
        .expect("source map"),
    )
    .expect("source map JSON");
    assert_eq!(source_map["contract_version"], 1);
    assert_eq!(source_map["output"], "products/index.html");
    assert!(source_map["sources"].as_array().is_some_and(|sources| {
        sources
            .iter()
            .any(|source| source == "client/components/product/card/tac.html")
    }));
}

#[test]
fn direct_control_aliases_and_contextual_escaping_render_deterministically() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("server/routes/aliases/yon.html"),
        r#"
<section>
  <!-- control aliases and dynamic attributes -->
  <if :when="show"><p>Hidden</p></if>
  <else><p>{message}</p></else>
  <logic :if="archived"><b>Archived</b></logic>
  <logic :else-if="show"><b>Shown</b></logic>
  <logic else><b>Current</b></logic>
  <button :disabled="disabled" :hidden="hidden" :required="required">Buy</button>
  <output>{disabled}|{count}|{hidden}|{meta}</output>
  <for :each="item in items"><i :data-id="item.id">{item.label}</i></for>
</section>
"#,
    );
    write(
        &project.path().join("server/routes/aliases/yon.js"),
        r#"
export class Handler {
  static GET() {
    return {
      show: false,
      archived: false,
      disabled: false,
      hidden: null,
      required: true,
      count: 7,
      meta: { safe: true },
      message: '<script>alert(1)</script>',
      items: [{ id: '" onclick="bad', label: 'one & two' }, { id: '2', label: 'two' }],
    }
  }
}
"#,
    );

    let first = run(ty().arg("build").arg(project.path()));
    assert!(first.status.success(), "{}", stderr(&first));
    let first_html = fs::read(project.path().join("dist/aliases/index.html")).expect("first HTML");
    let second = run(ty().arg("build").arg(project.path()));
    assert!(second.status.success(), "{}", stderr(&second));
    assert_eq!(
        first_html,
        fs::read(project.path().join("dist/aliases/index.html")).expect("second HTML")
    );
    let html = String::from_utf8(first_html).expect("UTF-8 HTML");
    assert!(!html.contains("<p>Hidden</p>"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(html.contains(r#"data-id="&quot; onclick=&quot;bad""#));
    assert!(html.contains("one &amp; two"));
    assert!(html.contains("<b>Current</b>"));
    assert!(!html.contains(" disabled"));
    assert!(!html.contains(" hidden"));
    assert!(html.contains("<button required>Buy</button>"));
    assert!(html.contains(r"<output>false|7||{&quot;safe&quot;:true}</output>"));
    assert_eq!(html.matches("<i ").count(), 2);
}

#[test]
fn islands_emit_ssr_public_props_modules_and_all_static_policies() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        r#"
<!doctype html><html><head><title>Islands</title></head><body>
<main>
  <demo-interactive label="SSR" hydrate="interaction"></demo-interactive>
  <demo-idle label="Idle" hydrate="idle"></demo-idle>
  <demo-visible label="Visible" hydrate="visible"></demo-visible>
  <demo-load label="Load" hydrate="load"></demo-load>
  <demo-default label="Default"></demo-default>
  <demo-never label="Never" hydrate="never"></demo-never>
</main>
</body></html>
"#,
    );
    write(
        &project.path().join("client/pages/fragment/tac.html"),
        r#"<section><demo-load label="Fragment" hydrate="load"></demo-load></section>"#,
    );
    for (second, label, companion) in [
        ("interactive", "interactive", true),
        ("idle", "idle", true),
        ("visible", "visible", true),
        ("load", "load", true),
        ("default", "default", true),
        ("never", "never", false),
    ] {
        let directory = project
            .path()
            .join(format!("client/components/demo/{second}"));
        write(
            &directory.join("tac.html"),
            &format!("<button data-kind=\"{label}\">{{label}}</button>"),
        );
        if companion {
            write(
                &directory.join("tac.js"),
                r"
export default class Demo {
  constructor(props) { this.props = props }
  async hydrate(root) { root.dataset.activated = this.props.label }
}
",
            );
        }
    }

    let output = run(ty().arg("build").arg(project.path()));
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("compiled=2 reused=0"));
    let html = fs::read_to_string(project.path().join("dist/index.html")).expect("HTML");
    for policy in ["interaction", "idle", "visible", "load", "never"] {
        assert!(html.contains(&format!(r#"data-tachyon-hydrate="{policy}""#)));
    }
    assert!(html.contains(">SSR</button>"));
    assert!(html.contains(">Never</button>"));
    assert!(html.contains(r#"<script type="module" src="/.tachyon/islands.js"></script>"#));
    assert!(project.path().join("dist/.tachyon/islands.js").is_file());
    for component in ["demo-interactive", "demo-idle", "demo-visible", "demo-load"] {
        assert!(
            project
                .path()
                .join(format!("dist/.tachyon/components/{component}.js"))
                .is_file()
        );
    }
    assert!(html.contains(r#"data-tachyon-component="demo-default" data-tachyon-hydrate="load""#));
    assert!(
        project
            .path()
            .join("dist/.tachyon/components/demo-default.js")
            .is_file()
    );
    assert!(
        !project
            .path()
            .join("dist/.tachyon/components/demo-never.js")
            .exists()
    );
    assert!(!html.contains(r#"data-tachyon-module="/.tachyon/components/demo-never.js""#));
    let fragment =
        fs::read_to_string(project.path().join("dist/fragment/index.html")).expect("fragment HTML");
    // The island runtime is injected at the end of the body, followed only by
    // the offline-cache registration every page carries.
    assert!(
        fragment.ends_with(
            r#"<script type="module" src="/.tachyon/islands.js"></script><script type="module" src="/.tachyon/register-sw.js"></script></body></html>"#
        ),
        "{fragment}"
    );
}

#[test]
fn incremental_reuse_is_verified_and_handler_routes_remain_volatile() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main><shared-card label=\"One\"></shared-card></main>",
    );
    let component = project
        .path()
        .join("client/components/shared/card/tac.html");
    write(&component, "<p>{label}</p>");

    let first = run(ty().arg("build").arg(project.path()));
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(stdout(&first).contains("compiled=1 reused=0"));
    let published = fs::read(project.path().join("dist/index.html")).expect("first");

    let second = run(ty().arg("build").arg(project.path()));
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(stdout(&second).contains("compiled=0 reused=1"));
    assert_eq!(
        published,
        fs::read(project.path().join("dist/index.html")).expect("reused")
    );

    write(&component, "<p class=\"changed\">{label}</p>");
    let changed = run(ty().arg("build").arg(project.path()));
    assert!(changed.status.success(), "{}", stderr(&changed));
    assert!(stdout(&changed).contains("compiled=1 reused=0"));

    write(
        &project.path().join("dist/.tachyon/build-state.json"),
        "{not-json",
    );
    let corrupt = run(ty().arg("build").arg(project.path()));
    assert!(corrupt.status.success(), "{}", stderr(&corrupt));
    assert!(stdout(&corrupt).contains("compiled=1 reused=0"));

    let forced = run(ty()
        .arg("build")
        .arg(project.path())
        .arg("--no-incremental"));
    assert!(forced.status.success(), "{}", stderr(&forced));
    assert!(stdout(&forced).contains("compiled=1 reused=0"));

    write(
        &project.path().join("server/routes/volatile/yon.html"),
        "<p>{value}</p>",
    );
    write(
        &project.path().join("server/routes/volatile/yon.js"),
        "export class Handler { static GET() { return { value: 'fresh' } } }",
    );
    let volatile_first = run(ty().arg("build").arg(project.path()));
    assert!(
        volatile_first.status.success(),
        "{}",
        stderr(&volatile_first)
    );
    let volatile_second = run(ty().arg("build").arg(project.path()));
    assert!(
        volatile_second.status.success(),
        "{}",
        stderr(&volatile_second)
    );
    assert!(stdout(&volatile_second).contains("compiled=1 reused=1"));
}

#[test]
fn diagnostics_recover_across_sources_and_failed_builds_preserve_output() {
    let project = tempfile::tempdir().expect("project");
    write(
        &project.path().join("client/pages/tac.html"),
        "<main>Known good</main>",
    );
    let baseline = run(ty().arg("build").arg(project.path()));
    assert!(baseline.status.success(), "{}", stderr(&baseline));
    let published = fs::read(project.path().join("dist/index.html")).expect("baseline");

    write(
        &project.path().join("client/pages/tac.html"),
        // A call parses now and fails at render, so it no longer produces a
        // parse diagnostic. Trailing syntax still does, which is what this
        // test needs: two files, each contributing one parse error.
        "<main>{missing value}</main>",
    );
    write(
        &project.path().join("client/pages/second/tac.html"),
        "<logic else>orphan</logic>",
    );
    let failed = run(ty()
        .args(["--diagnostic-format", "json", "build"])
        .arg(project.path()));
    assert!(!failed.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&failed.stderr).expect("Diagnostics v1 JSON");
    let diagnostics = report["diagnostics"].as_array().expect("diagnostics");
    assert!(diagnostics.len() >= 2);
    assert!(diagnostics.iter().any(|value| value["code"] == "TY1303"));
    assert!(diagnostics.iter().any(|value| value["code"] == "TY1302"));
    assert_eq!(
        published,
        fs::read(project.path().join("dist/index.html")).expect("retained output")
    );
}

#[test]
fn context_collisions_component_cycles_and_invalid_islands_fail_closed() {
    let collision = tempfile::tempdir().expect("collision");
    write(
        &collision.path().join("server/routes/yon.html"),
        "<p>{duplicate}</p>",
    );
    write(
        &collision.path().join("server/routes/yon.js"),
        "export class Handler { static duplicate = 'static'; static GET() { return { duplicate: 'response' } } }",
    );
    let output = run(ty().arg("build").arg(collision.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1503"));

    let cycle = tempfile::tempdir().expect("cycle");
    write(
        &cycle.path().join("client/pages/tac.html"),
        "<cycle-one></cycle-one>",
    );
    write(
        &cycle.path().join("client/components/cycle/one/tac.html"),
        "<cycle-two></cycle-two>",
    );
    write(
        &cycle.path().join("client/components/cycle/two/tac.html"),
        "<cycle-one></cycle-one>",
    );
    let output = run(ty().arg("build").arg(cycle.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1403"));

    let island = tempfile::tempdir().expect("island");
    write(
        &island.path().join("client/pages/tac.html"),
        r#"<missing-module hydrate="visible"></missing-module>"#,
    );
    write(
        &island
            .path()
            .join("client/components/missing/module/tac.html"),
        "<p>SSR</p>",
    );
    let output = run(ty().arg("build").arg(island.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1405"));
}

#[test]
fn adversarial_parser_component_and_context_shapes_fail_through_the_binary() {
    let parser = tempfile::tempdir().expect("parser");
    write(
        &parser.path().join("client/pages/tac.html"),
        r#"
<main onclick="bad()" :hydrate="policy" :title="bad syntax">
  <script>bad()</script>
  <web-widget hydrate="load"></web-widget>
  <unknown>bad</unknown>
  <logic>bad</logic>
  <loop>bad</loop>
  <slot name="named"></slot>
</main>
"#,
    );
    let output = run(ty()
        .args(["--diagnostic-format", "json", "build"])
        .arg(parser.path()));
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stderr).expect("report");
    let codes = report["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|value| value["code"].as_str())
        .collect::<Vec<_>>();
    for code in ["TY1303", "TY1306", "TY1402", "TY1404"] {
        assert!(codes.contains(&code), "missing {code}: {codes:?}");
    }

    let components = tempfile::tempdir().expect("components");
    write(
        &components
            .path()
            .join("client/components/bad/template/tac.html"),
        "<main>",
    );
    // A polyglot component companion has no adapter, so it is what must still
    // raise TY1401. tac.css is deliberately not used here: it is supported, and
    // a test that treats a working feature as a failure stops proving anything
    // the day it starts working.
    write(
        &components
            .path()
            .join("client/components/bad/template/tac.py"),
        "x = 1",
    );
    write(
        &components.path().join("client/pages/tac.html"),
        "<main>page</main>",
    );
    let output = run(ty().arg("build").arg(components.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1301"));
    assert!(stderr(&output).contains("TY1401"));

    let component_shapes = tempfile::tempdir().expect("component shapes");
    write(
        &component_shapes.path().join("client/pages/tac.html"),
        "<main>page</main>",
    );
    write(
        &component_shapes
            .path()
            .join("client/components/orphan/script/tac.js"),
        "export default class Orphan {}",
    );
    write(
        &component_shapes
            .path()
            .join("client/components/single/tac.html"),
        "<p>invalid path</p>",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let target = component_shapes.path().join("outside.html");
        write(&target, "<p>linked</p>");
        let link = component_shapes
            .path()
            .join("client/components/linked/card/tac.html");
        fs::create_dir_all(link.parent().expect("link parent")).expect("link directory");
        symlink(target, link).expect("component symlink");
    }
    let output = run(ty().arg("build").arg(component_shapes.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1401"));

    let missing_slot = tempfile::tempdir().expect("missing slot");
    write(
        &missing_slot.path().join("client/pages/tac.html"),
        "<main><no-slot>child</no-slot></main>",
    );
    write(
        &missing_slot
            .path()
            .join("client/components/no/slot/tac.html"),
        "<p>component</p>",
    );
    let output = run(ty().arg("build").arg(missing_slot.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1404"));

    for (source, code) in [
        (
            "export class Handler { static GET() { return [] } }",
            "TY1501",
        ),
        ("export class Handler { static ['bad-key'] = 1 }", "TY1502"),
        (
            "export class Handler { static platform = 'override' }",
            "TY1502",
        ),
    ] {
        let context = tempfile::tempdir().expect("context");
        write(
            &context.path().join("server/routes/yon.html"),
            "<p>view</p>",
        );
        write(&context.path().join("server/routes/yon.js"), source);
        let output = run(ty().arg("build").arg(context.path()));
        assert!(!output.status.success());
        assert!(stderr(&output).contains(code), "{}", stderr(&output));
    }

    let many_exports = (0..1_025)
        .map(|index| format!("key{index}: {index}"))
        .collect::<Vec<_>>()
        .join(",");
    let context = tempfile::tempdir().expect("context export budget");
    write(
        &context.path().join("server/routes/yon.html"),
        "<p>view</p>",
    );
    write(
        &context.path().join("server/routes/yon.js"),
        &format!("export class Handler {{ static GET() {{ return {{ {many_exports} }} }} }}"),
    );
    let output = run(ty().arg("build").arg(context.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1504"));

    let context = tempfile::tempdir().expect("context nesting budget");
    write(
        &context.path().join("server/routes/yon.html"),
        "<p>view</p>",
    );
    write(
        &context.path().join("server/routes/yon.js"),
        &format!(
            "export class Handler {{ static GET() {{ return {{ deep: {} }} }} }}",
            "[".repeat(34) + "0" + &"]".repeat(34)
        ),
    );
    let output = run(ty().arg("build").arg(context.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1502"));

    let root_file = tempfile::tempdir().expect("component root file");
    write(
        &root_file.path().join("client/pages/tac.html"),
        "<main>page</main>",
    );
    write(
        &root_file.path().join("client/components"),
        "not a directory",
    );
    let output = run(ty().arg("build").arg(root_file.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1401"));

    for bytes in [b"bad\0script".to_vec(), vec![b'x'; 1_048_577]] {
        let companion = tempfile::tempdir().expect("companion");
        write(
            &companion.path().join("client/pages/tac.html"),
            "<main><bad-script></bad-script></main>",
        );
        write(
            &companion
                .path()
                .join("client/components/bad/script/tac.html"),
            "<p>safe</p>",
        );
        let script = companion.path().join("client/components/bad/script/tac.js");
        fs::write(script, bytes).expect("invalid companion");
        let output = run(ty().arg("build").arg(companion.path()));
        assert!(!output.status.success());
        assert!(stderr(&output).contains("TY1401"));
    }

    for bytes in [vec![0xff], vec![b'x'; 1_048_577]] {
        let view = tempfile::tempdir().expect("invalid view");
        let path = view.path().join("client/pages/tac.html");
        fs::create_dir_all(path.parent().expect("view parent")).expect("view directory");
        fs::write(path, bytes).expect("invalid view bytes");
        let output = run(ty().arg("build").arg(view.path()));
        assert!(!output.status.success());
        assert!(stderr(&output).contains("TY1301"));
    }

    let deep_view = tempfile::tempdir().expect("deep view");
    let source = "<div>".repeat(65) + "x" + &"</div>".repeat(65);
    write(&deep_view.path().join("client/pages/tac.html"), &source);
    let output = run(ty().arg("build").arg(deep_view.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY13"));

    let iteration = tempfile::tempdir().expect("iteration");
    write(
        &iteration.path().join("server/routes/yon.html"),
        "<!-- comment --><loop :for=\"item of items\">{item}</loop>",
    );
    write(
        &iteration.path().join("server/routes/yon.js"),
        "export class Handler { static GET() { return { items: {} } } }",
    );
    let output = run(ty().arg("build").arg(iteration.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1303"));

    let budget = tempfile::tempdir().expect("iteration budget");
    write(
        &budget.path().join("server/routes/yon.html"),
        "<loop :for=\"item of items\">{item}</loop>",
    );
    write(
        &budget.path().join("server/routes/yon.js"),
        "export class Handler { static GET() { return { items: Array.from({ length: 10001 }, (_, i) => i) } } }",
    );
    let output = run(ty().arg("build").arg(budget.path()));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("TY1305"));
}
