#![allow(clippy::expect_used)]

use std::path::Path;

struct CsharpTacHelperTests;

impl CsharpTacHelperTests {
    const HELPER: &'static str = "[Helper]\npublic sealed class LabelHelper {\n private readonly string label;\n public LabelHelper(string label) { this.label = label; }\n public string Read() { return label; }\n}\n";
    const CALLER: &'static str = "[Setup]\npublic class LabelSetup {\n public static BehaviorAdapters configure(BehaviorExecution execution) {\n var helper = new LabelHelper(\"owned\");\n helper.Read();\n return new BehaviorAdapters();\n }\n}\n";

    fn lower(source: &str) -> Result<Option<String>, crate::Failure> {
        let path = Path::new("client/setups/LabelSetup.cs");
        let stripped = crate::behavior::ServiceRoles::read(path, source)?.source;
        super::lower(path, &stripped, "tac")
    }
}

#[test]
fn csharp_tac_helper_preserves_explicit_setup_and_service_constructors() {
    let service = "[Service]\npublic class LabelService {\n private readonly LabelHelper helper;\n public LabelService() { helper = new LabelHelper(\"service\"); }\n public string Read() { return helper.Read(); }\n}\n";
    let source = format!(
        "{}{}{service}",
        CsharpTacHelperTests::HELPER,
        CsharpTacHelperTests::CALLER
    );
    let lowered = CsharpTacHelperTests::lower(&source)
        .expect("caller-owned Helper")
        .expect("lowered source");
    assert!(!lowered.contains("[Helper]"));
    assert!(lowered.contains("public LabelHelper(string label)"));
    assert!(lowered.contains("new LabelHelper(\"owned\")"));
    assert!(lowered.contains("new LabelHelper(\"service\")"));
    assert!(!lowered.contains("construct<LabelHelper>"));
}

#[test]
fn csharp_tac_helper_does_not_take_comments_strings_or_self_recursion_as_callers() {
    for caller in [
        "",
        "// new LabelHelper(\"owned\");\n",
        "class Note { string text = \"new LabelHelper()\"; }\n",
        "class Note { object Run() { return Behaviors.construct<LabelHelper>(); } }\n",
        "class Note { object Run() { return Behaviors.construct(typeof(LabelHelper)); } }\n",
        "class Note { object Run() { return new LabelHelperSuffix(); } }\n",
    ] {
        let source = format!("{}{caller}", CsharpTacHelperTests::HELPER);
        let error = CsharpTacHelperTests::lower(&source).expect_err("missing explicit caller");
        assert!(
            error
                .to_string()
                .contains("explicit caller-owned construction"),
            "{error}"
        );
    }
    let recursive = "[Helper]\nclass LabelHelper {\n public LabelHelper() { var recursive = new LabelHelper(); }\n}\n";
    let error =
        CsharpTacHelperTests::lower(recursive).expect_err("self recursion is not ownership proof");
    assert!(
        error.to_string().contains("outside its own class"),
        "{error}"
    );
}

#[test]
fn csharp_tac_helper_refuses_managed_helper_dependencies_even_with_another_real_caller() {
    for marker in ["[Needs]", "[NeedsAll]"] {
        let service = if marker == "[Needs]" {
            "[Service]\nclass ReaderService {\n [Needs]\n private readonly LabelHelper helper;\n}\n"
                .to_owned()
        } else {
            "[Service]\n[NeedsAll]\nclass ReaderService {\n private readonly LabelHelper helper;\n}\n".to_owned()
        };
        let source = format!(
            "{}{}{service}",
            CsharpTacHelperTests::HELPER,
            CsharpTacHelperTests::CALLER
        );
        let error = CsharpTacHelperTests::lower(&source).expect_err("managed Helper dependency");
        assert!(
            error.to_string().contains("managed Helper injection"),
            "{marker}: {error}"
        );
    }
}

#[test]
fn csharp_tac_helper_capture_refuses_class_ownership_and_hooks_at_authored_spans() {
    for marker in [
        "[Needs]",
        "[NeedsAll]",
        "[Lifetime({\"value\":\"prototype\"})]",
    ] {
        let source = format!("[Helper]\n{marker}\nclass LabelHelper {{}}\n");
        let path = Path::new("client/helpers/nested/LabelHelper.cs");
        let error = crate::behavior::HelperRole::refuse_in_tac(path, &source)
            .expect_err("managed Helper role");
        let diagnostic = &error.diagnostics()[0];
        assert_eq!(diagnostic.code.number(), 1308);
        assert_eq!(diagnostic.spans[0].file, path.to_string_lossy());
        assert_eq!(
            diagnostic.spans[0].start,
            u64::try_from(source.find(marker).expect("marker")).expect("marker offset")
        );
    }
    for marker in ["OnStart", "OnStop", "OnEvent"] {
        let source =
            format!("[Helper]\nclass LabelHelper {{\n [{marker}]\n public void Run() {{}}\n}}\n");
        let error = crate::behavior::HelperRole::refuse_in_tac(
            Path::new("client/helpers/LabelHelper.cs"),
            &source,
        )
        .expect_err("unsupported Helper hook");
        assert!(
            error.to_string().contains("lifecycle hooks"),
            "{marker}: {error}"
        );
        assert_eq!(
            error.diagnostics()[0].spans[0].start,
            u64::try_from(source.find(&format!("[{marker}]")).expect("hook span"))
                .expect("hook offset")
        );
    }
}

#[test]
fn csharp_tac_helper_keeps_folder_and_role_guards() {
    for (path, source) in [
        ("client/helpers/LabelHelper.cs", "class LabelHelper {}"),
        (
            "client/helpers/LabelService.cs",
            "[Service]\nclass LabelService {}",
        ),
        (
            "client/services/LabelHelper.cs",
            "[Helper]\nclass LabelHelper {}",
        ),
    ] {
        assert!(
            crate::annotation_layout::FolderRoleCheck::check(Path::new(path), source).is_err(),
            "{path}"
        );
    }
    assert!(
        crate::behavior::HelperRole::refuse_in_tac(
            Path::new("client/helpers/LabelHelper.swift"),
            "@Helper\nfinal class LabelHelper {}"
        )
        .is_err()
    );
}

#[test]
fn csharp_tac_helper_generates_a_typed_constructor_only_for_explicit_callers() {
    let source = "[Helper]\nclass InputHelper {\n [Needs]\n private readonly string label;\n public string Read() { return label; }\n}\n[Setup]\nclass InputSetup {\n public static BehaviorAdapters configure(BehaviorExecution execution) {\n var helper = new InputHelper(\"typed\");\n helper.Read();\n return new BehaviorAdapters();\n }\n}\n";
    let lowered = CsharpTacHelperTests::lower(source)
        .expect("explicit constructor request")
        .expect("lowered source");
    assert!(
        lowered.contains("public InputHelper(string label)"),
        "{lowered}"
    );
    assert!(lowered.contains("this.label = label;"), "{lowered}");
    assert!(lowered.contains("new InputHelper(\"typed\")"), "{lowered}");
}

#[test]
fn csharp_tac_helper_ordinary_http_fixture_retains_context_owned_composition() {
    let root = tempfile::tempdir().expect("actual HTTP fixture");
    let entry = Path::new("client/services/HttpProbeService.cs");
    let source =
        include_str!("../../../tests/fixtures/tac-dotnet-http/client/services/HttpProbeService.cs");
    for (relative, source) in [
        ("client/services/HttpProbeService.cs", source),
        (
            "client/setups/BrowserHttpSetup.cs",
            include_str!(
                "../../../tests/fixtures/tac-dotnet-http/client/setups/BrowserHttpSetup.cs"
            ),
        ),
        (
            "client/helpers/CustomHttpHelper.cs",
            include_str!(
                "../../../tests/fixtures/tac-dotnet-http/client/helpers/CustomHttpHelper.cs"
            ),
        ),
        (
            "client/setups/OverrideHttpSetup.cs",
            include_str!(
                "../../../tests/fixtures/tac-dotnet-http/client/setups/OverrideHttpSetup.cs"
            ),
        ),
    ] {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, source).expect("source");
    }
    let roles = crate::behavior::ServiceRoles::read(entry, source).expect("root roles");
    let composed = crate::annotation_modules::assemble(root.path(), entry, &roles.source, "tac")
        .expect("validated discovery")
        .expect("Helper and Setup assembly");
    let events = crate::events::extract(entry, &composed, "tac").expect("owned root lifecycle");
    let source = events
        .as_ref()
        .map_or(composed.as_str(), |module| module.source.as_str());
    let lowered = crate::behavior::lower(entry, source, "tac")
        .expect("explicit Helper lowering")
        .expect("runtime");
    assert!(lowered.contains("class CustomHttpHelper : BehaviorHTTPAdapter"));
    assert!(lowered.contains("new CustomHttpHelper()"));
    assert!(lowered.contains("context.Dispose();"));
    assert!(!lowered.contains("[Helper]"));
}

#[test]
fn csharp_tac_helper_keeps_bare_marker_argument_validation() {
    let source = "[Helper(\"custom\")]\nclass LabelHelper {}\n";
    assert!(
        crate::behavior::ServiceRoles::read(Path::new("client/helpers/LabelHelper.cs"), source)
            .is_err()
    );
}

#[test]
fn csharp_tac_helper_keeps_yon_managed_roles_and_lowering() {
    let path = Path::new("server/controllers/LabelController.cs");
    let source = "[Service]\npublic class LabelService {\n public string Read() { return \"yon\"; }\n}\n[Helper]\n[NeedsAll]\npublic class OwnedHelper {\n private readonly LabelService labelService;\n public string Read() { return labelService.Read(); }\n [OnStop]\n public void Close() { labelService.Read(); }\n}\n[RequestRoute(\"/label\")]\n[NeedsAll]\npublic class LabelController {\n private readonly OwnedHelper ownedHelper;\n [GetRoute]\n public BehaviorJSON getLabel() { return BehaviorJSON.From(ownedHelper.Read()); }\n}\n";
    let roles = crate::behavior::ServiceRoles::read(path, source).expect("Yon role consumption");
    assert!(!roles.source.contains("[Helper]"));
    assert!(!roles.source.contains("[NeedsAll]"));
    assert!(roles.source.contains("[OnStop]"));
    crate::behavior::HelperRole::refuse_in_tac(path, source)
        .expect("Tac adapter does not restrict Yon source");
    let lowered = crate::yon::lower_with_routes(path, source, None)
        .expect("real Yon managed provider pipeline")
        .expect("managed Helper emission");
    assert!(lowered.source.contains("new OwnedHelper("));
    assert!(lowered.source.contains("LabelService"));
    assert!(lowered.source.contains("labelService.Read();"));
    assert!(!lowered.source.contains("[NeedsAll]"));
}

#[test]
fn csharp_tac_helper_retains_markers_only_in_the_client_realm() {
    for path in [
        "client/helpers/LabelHelper.cs",
        "client/shared/native/LabelHelper.cs",
        "/owned/project/client/helpers/nested/LabelHelper.cs",
    ] {
        assert!(
            crate::behavior::HelperRole::keeps_tac_marker(Path::new(path)),
            "{path}"
        );
    }
    for path in [
        "server/helpers/LabelHelper.cs",
        "server/shared/LabelHelper.cs",
        "server/client/LabelHelper.cs",
        "LabelHelper.cs",
        "client/helpers/LabelHelper.kt",
    ] {
        assert!(
            !crate::behavior::HelperRole::keeps_tac_marker(Path::new(path)),
            "{path}"
        );
    }
}

#[test]
fn csharp_tac_helper_allows_only_actual_interfaces_not_base_classes() {
    let caller = CsharpTacHelperTests::CALLER;
    let interface = "public interface LabelContract { string Read(); }\n";
    let helper = CsharpTacHelperTests::HELPER
        .replace("class LabelHelper {", "class LabelHelper : LabelContract {");
    let source = format!("{interface}{helper}{caller}");
    let lowered = CsharpTacHelperTests::lower(&source)
        .expect("actual same-language interface")
        .expect("helper source");
    assert!(lowered.contains("class LabelHelper : LabelContract"));
    let path = Path::new("server/helpers/LabelHelper.cs");
    let input = super::super::Input {
        path,
        source: &source,
        code: crate::annotation_layout::masked_source(path, &source).expect("authored source mask"),
    };
    let mut ty = super::declarations(&input, "cs")
        .expect("real source declarations")
        .into_iter()
        .find(|ty| ty.name == "LabelHelper")
        .expect("Helper declaration");
    ty.policies
        .insert("helper".to_owned(), serde_json::Value::Bool(true));
    assert!(
        !super::csharp_helper_interfaces::CsharpHelperInterfaces::allowed(&input, "cs", &ty)
            .expect("Yon does not use the Tac interface allowance")
    );
    for base in ["BaseClass", "MissingContract", "LabelContract, BaseClass"] {
        let helper = CsharpTacHelperTests::HELPER.replace(
            "class LabelHelper {",
            &format!("class LabelHelper : {base} {{"),
        );
        let source = format!("{interface}public class BaseClass {{}}\n{helper}{caller}");
        let error =
            CsharpTacHelperTests::lower(&source).expect_err("no concrete or unknown inheritance");
        assert!(error.to_string().contains("cannot inherit"), "{error}");
    }
    let service = "[Service]\n[CacheSettings({\"name\":\"probe\",\"ttlMs\":1000,\"maxBytes\":65536,\"maxEntries\":16})]\nclass LabelService : LabelContract { public string Read() { return \"service\"; } }\n";
    let error = CsharpTacHelperTests::lower(&format!("{interface}{service}"))
        .expect_err("Service inheritance unchanged");
    assert!(error.to_string().contains("cannot inherit"), "{error}");
}

#[test]
fn csharp_tac_helper_hook_capture_ignores_strings_comments_and_other_owners() {
    let source = format!(
        "{}// [OnStop]\nclass Note {{ string text = \"[OnStart]\"; }}\n[Service]\nclass ReaderService {{\n [OnStart]\n public void Start() {{}}\n}}\n",
        CsharpTacHelperTests::HELPER
    );
    crate::behavior::HelperRole::refuse_in_tac(Path::new("client/helpers/LabelHelper.cs"), &source)
        .expect("only Helper-owned real hooks are refused");
}
