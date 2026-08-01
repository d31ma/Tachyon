use super::model::{AttributeValue, TemplateAttribute};
use super::{TemplateFrontend, TemplateNode, TemplateNodeKind, TemplateProgram};
use crate::Failure;
use crate::failure::{diagnostic, source_span};
use sha2::{Digest, Sha256};

/// Attribute a component's `@scope` rule selects on.
pub(crate) const SCOPE_ATTRIBUTE: &str = "data-tac-scope";
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component as PathComponent, Path, PathBuf};

const COMPONENT_ROOT: &str = "client/components";
const MAX_COMPONENTS: usize = 1_024;
const MAX_COMPANION_BYTES: u64 = 1_048_576;

#[derive(Clone, Debug, Default)]
pub(crate) struct ComponentRegistry {
    components: BTreeMap<String, ComponentDefinition>,
    digest: String,
}

impl ComponentRegistry {
    pub(crate) fn discover(project_root: &Path) -> Result<Self, Failure> {
        let Some(root) = regular_component_root(project_root)? else {
            return Ok(Self::default());
        };

        let mut discovered = BTreeMap::<String, ComponentFiles>::new();
        let mut diagnostics = Vec::new();
        visit_components(
            project_root,
            &root,
            &root,
            &mut discovered,
            &mut diagnostics,
        );
        if discovered.len() > MAX_COMPONENTS {
            diagnostics.push(component_diagnostic(
                1401,
                COMPONENT_ROOT,
                0,
                COMPONENT_ROOT.len(),
                "Component registry exceeds the limit of 1,024.",
                "Split the application or remove unused components.",
            ));
        }
        let names = discovered.keys().cloned().collect::<BTreeSet<_>>();
        let mut components = BTreeMap::new();
        let mut hasher = Sha256::new();
        for (name, mut files) in discovered {
            let Some(template) = files.template.take() else {
                if let Some(script) = files
                    .script_real_compiler
                    .as_ref()
                    .or(files.script.as_ref())
                {
                    diagnostics.push(component_diagnostic(
                        1401,
                        &portable_path(
                            script
                                .strip_prefix(project_root)
                                .unwrap_or(script.as_path()),
                        ),
                        0,
                        0,
                        &format!("Component '{name}' has a script companion but no tac.html."),
                        "Add the component template or remove the orphan companion.",
                    ));
                }
                continue;
            };
            let relative_template =
                portable_path(template.strip_prefix(project_root).unwrap_or(&template));
            match TemplateFrontend::compile_file(&template, &relative_template, &names) {
                Ok(program) => {
                    hash_file(&mut hasher, project_root, &template, &mut diagnostics);
                    if let Some(script) = files
                        .script_real_compiler
                        .as_ref()
                        .or(files.script.as_ref())
                    {
                        validate_script(project_root, script, &mut diagnostics);
                        hash_file(&mut hasher, project_root, script, &mut diagnostics);
                    }
                    components.insert(
                        name.clone(),
                        define_component(
                            program,
                            &name,
                            files,
                            project_root,
                            &mut hasher,
                            &mut diagnostics,
                        ),
                    );
                }
                Err(failure) => diagnostics.extend_from_slice(failure.diagnostics()),
            }
        }
        detect_cycles(&components, &mut diagnostics);
        sort_diagnostics(&mut diagnostics);
        if !diagnostics.is_empty() {
            return Err(Failure::new(diagnostics));
        }
        Ok(Self {
            components,
            digest: hex_digest(hasher.finalize()),
        })
    }

    pub(crate) fn names(&self) -> BTreeSet<String> {
        self.components.keys().cloned().collect()
    }

    pub(crate) fn get(&self, name: &str) -> Option<&ComponentDefinition> {
        self.components.get(name)
    }

    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }
}

fn regular_component_root(project_root: &Path) -> Result<Option<PathBuf>, Failure> {
    let root = project_root.join(COMPONENT_ROOT);
    let metadata = match fs::symlink_metadata(&root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Failure::one(component_diagnostic(
                1401,
                COMPONENT_ROOT,
                0,
                0,
                &format!("Cannot inspect component root: {error}"),
                "Use a readable regular component directory.",
            )));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Failure::one(component_diagnostic(
            1401,
            COMPONENT_ROOT,
            0,
            COMPONENT_ROOT.len(),
            "Component root must be a regular directory.",
            "Replace the source root with a non-symlinked directory.",
        )));
    }
    Ok(Some(root))
}

#[derive(Clone, Debug)]
pub(crate) struct ComponentDefinition {
    program: TemplateProgram,
    script_path: Option<PathBuf>,
    style_path: Option<PathBuf>,
    wasm_path: Option<PathBuf>,
}

impl ComponentDefinition {
    pub(crate) fn program(&self) -> &TemplateProgram {
        &self.program
    }

    pub(crate) fn has_script(&self) -> bool {
        self.script_path.is_some() || self.wasm_path.is_some()
    }

    pub(crate) fn script_path(&self) -> Option<&Path> {
        self.script_path.as_deref()
    }

    pub(crate) fn style_path(&self) -> Option<&Path> {
        self.style_path.as_deref()
    }

    /// Source of a companion compiled to WebAssembly, when the component has
    /// one instead of a JavaScript module.
    pub(crate) fn wasm_path(&self) -> Option<&Path> {
        self.wasm_path.as_deref()
    }
}

#[derive(Clone, Debug, Default)]
struct ComponentFiles {
    template: Option<PathBuf>,
    script: Option<PathBuf>,
    script_real_compiler: Option<PathBuf>,
    style: Option<PathBuf>,
    wasm: Option<PathBuf>,
    wasm_real_compiler: Option<PathBuf>,
}

fn visit_components(
    project_root: &Path,
    root: &Path,
    directory: &Path,
    components: &mut BTreeMap<String, ComponentFiles>,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    let mut entries = match fs::read_dir(directory)
        .and_then(std::iter::Iterator::collect::<Result<Vec<_>, _>>)
    {
        Ok(entries) => entries,
        Err(error) => {
            let relative = portable_path(directory.strip_prefix(project_root).unwrap_or(directory));
            diagnostics.push(component_diagnostic(
                1401,
                &relative,
                0,
                relative.len(),
                &format!("Cannot enumerate component directory: {error}"),
                "Check the directory permissions.",
            ));
            return;
        }
    };
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(project_root).unwrap_or(&path);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostics.push(component_diagnostic(
                    1401,
                    &portable_path(relative),
                    0,
                    0,
                    &format!("Cannot inspect component source: {error}"),
                    "Use readable regular component sources.",
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            diagnostics.push(component_diagnostic(
                1401,
                &portable_path(relative),
                0,
                portable_path(relative).len(),
                "Symlinked component sources are not allowed.",
                "Replace the symlink with a regular file or directory.",
            ));
        } else if metadata.is_dir() {
            visit_components(project_root, root, &path, components, diagnostics);
        } else if metadata.is_file() {
            inspect_component_file(root, &path, components, diagnostics);
        }
    }
}

fn inspect_component_file(
    root: &Path,
    path: &Path,
    components: &mut BTreeMap<String, ComponentFiles>,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        diagnostics.push(component_diagnostic(
            1401,
            COMPONENT_ROOT,
            0,
            0,
            "Component source paths must be valid Unicode.",
            "Rename the source using portable Unicode.",
        ));
        return;
    };
    if !matches!(
        name,
        "tac.html"
            | "tac.js"
            | "tac.ts"
            | "tac.css"
            | "tac.rs"
            | "tac.dart"
            | "tac.kt"
            | "tac.swift"
            | "tac.cs"
            | "tachyon-wasm.rs"
            | "tachyon-wasm.dart"
            | "tachyon-wasm.kt"
            | "tachyon-wasm.swift"
            | "tachyon-wasm.cs"
            | "tachyon-island.js"
            | "tachyon-island.ts"
    ) {
        if name.starts_with("tac.") {
            let relative = portable_path(path.strip_prefix(root).unwrap_or(path));
            diagnostics.push(component_diagnostic(
                1401,
                &relative,
                0,
                relative.len(),
                "Component companion is not a supported browser language.",
                "A browser companion is tac.js or tac.ts, tac.rs, tac.dart, \
                 tac.kt, tac.swift or tac.cs compiled to WebAssembly, and \
                 tac.css for styles. Logic in another language belongs in a \
                 Yon handler, where any language runs under the direct \
                 protocol.",
            ));
        }
        return;
    }
    let Some(parent) = path.parent() else {
        return;
    };
    let Some(component_name) = component_name(root, parent) else {
        let relative = portable_path(path.strip_prefix(root).unwrap_or(path));
        diagnostics.push(component_diagnostic(
            1401,
            &relative,
            0,
            relative.len(),
            "Component path must name a tag that is not a standard HTML element.",
            "Rename the directory: client/components/date-picker/tac.html names \
             <date-picker>, and client/components/product/card/tac.html names \
             <product-card>.",
        ));
        return;
    };
    let files = components.entry(component_name).or_default();
    match name {
        "tac.html" => files.template = Some(path.to_path_buf()),
        "tac.css" => files.style = Some(path.to_path_buf()),
        // A companion in a language that compiles to wasm, whichever of the two
        // module shapes in ADR 0011 its toolchain can emit.
        "tac.rs" | "tac.dart" | "tac.kt" | "tac.swift" | "tac.cs" => {
            files.wasm = Some(path.to_path_buf());
        }
        "tachyon-wasm.rs" | "tachyon-wasm.dart" | "tachyon-wasm.kt" | "tachyon-wasm.swift"
        | "tachyon-wasm.cs" => {
            files.wasm_real_compiler = Some(path.to_path_buf());
        }
        "tachyon-island.js" | "tachyon-island.ts" => {
            files.script_real_compiler = Some(path.to_path_buf());
        }
        _ => files.script = Some(path.to_path_buf()),
    }
}

/// Assembles one component, hashing and scoping its stylesheet when present.
fn define_component(
    mut program: TemplateProgram,
    name: &str,
    files: ComponentFiles,
    project_root: &Path,
    hasher: &mut Sha256,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) -> ComponentDefinition {
    if let Some(style) = &files.style {
        hash_file(hasher, project_root, style, diagnostics);
        // The scope attribute is what the emitted `@scope` rule selects on, so
        // a component's styles reach its own subtree and nothing else.
        mark_scope_roots(&mut program.nodes, name);
    }
    let script_path = files.script_real_compiler.or(files.script);
    let wasm_path = files.wasm_real_compiler.or(files.wasm);
    if let Some(wasm) = &wasm_path {
        validate_script(project_root, wasm, diagnostics);
        hash_file(hasher, project_root, wasm, diagnostics);
    }
    ComponentDefinition {
        program,
        script_path,
        style_path: files.style,
        wasm_path,
    }
}

/// Marks a component's root elements so its stylesheet can scope to them.
///
/// CSS `@scope` is the platform's own answer to component style scoping, so
/// there is no selector rewriting and no per-element attribute: one attribute
/// on each root is all the browser needs. Control-flow nodes are transparent,
/// so the mark lands on the elements a branch or a loop actually produces.
fn mark_scope_roots(nodes: &mut [TemplateNode], component: &str) {
    for node in nodes {
        match &mut node.kind {
            TemplateNodeKind::Element { attributes, .. } => {
                attributes.insert(
                    String::from(SCOPE_ATTRIBUTE),
                    TemplateAttribute {
                        value: AttributeValue::Static(String::from(component)),
                        range: node.range,
                    },
                );
            }
            TemplateNodeKind::Conditional { children, .. }
            | TemplateNodeKind::Iteration { children, .. } => {
                mark_scope_roots(children, component);
            }
            _ => {}
        }
    }
}

/// Derives a component's tag from its directory path.
///
/// Any number of segments is joined by hyphens, so `clicker/` names
/// `<clicker>`, `date-picker/` names `<date-picker>`, and `product/card/`
/// names `<product-card>`.
///
/// A tag without a hyphen is not a custom element name, so it could in
/// principle collide with an element HTML gains later. A component is compiled
/// away rather than registered, so the collision that actually matters is with
/// an element that exists today — and that is refused by name.
fn component_name(root: &Path, directory: &Path) -> Option<String> {
    let relative = directory.strip_prefix(root).ok()?;
    let segments = relative
        .components()
        .filter_map(|component| match component {
            PathComponent::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    if segments.is_empty() || !segments.iter().all(|segment| valid_segment(segment)) {
        return None;
    }
    let name = segments.join("-");
    // Shadowing a real element would silently change what a template means.
    if crate::html::is_standard_html_tag(&name) {
        return None;
    }
    Some(name)
}

fn valid_segment(segment: &str) -> bool {
    segment
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn validate_script(
    project_root: &Path,
    path: &Path,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    let relative = portable_path(path.strip_prefix(project_root).unwrap_or(path));
    match fs::read(path) {
        Ok(bytes) if bytes.len() as u64 > MAX_COMPANION_BYTES => {
            diagnostics.push(component_diagnostic(
                1401,
                &relative,
                0,
                bytes.len(),
                "Tac island companion exceeds the 1 MiB limit.",
                "Reduce the browser companion below 1 MiB.",
            ));
        }
        Ok(bytes) if std::str::from_utf8(&bytes).is_err() || bytes.contains(&0) => {
            diagnostics.push(component_diagnostic(
                1401,
                &relative,
                0,
                bytes.len(),
                "Tac island companion must be NUL-free UTF-8.",
                "Save tac.js as UTF-8 without NUL bytes.",
            ));
        }
        Ok(_) => {}
        Err(error) => diagnostics.push(component_diagnostic(
            1401,
            &relative,
            0,
            0,
            &format!("Cannot read Tac island companion: {error}"),
            "Check the companion file permissions.",
        )),
    }
}

fn detect_cycles(
    components: &BTreeMap<String, ComponentDefinition>,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    let mut complete = BTreeSet::new();
    for name in components.keys() {
        let mut active = Vec::new();
        visit_cycle(name, components, &mut active, &mut complete, diagnostics);
    }
}

fn visit_cycle(
    name: &str,
    components: &BTreeMap<String, ComponentDefinition>,
    active: &mut Vec<String>,
    complete: &mut BTreeSet<String>,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    if complete.contains(name) {
        return;
    }
    if let Some(position) = active.iter().position(|active| active == name) {
        let mut cycle = active[position..].to_vec();
        cycle.push(String::from(name));
        let source = components.get(name).map_or(COMPONENT_ROOT, |component| {
            component.program.source_path.as_str()
        });
        diagnostics.push(component_diagnostic(
            1403,
            source,
            0,
            0,
            &format!("Tac component cycle detected: {}.", cycle.join(" -> ")),
            "Remove one component invocation from the cycle.",
        ));
        return;
    }
    active.push(String::from(name));
    if let Some(component) = components.get(name) {
        let mut references = BTreeSet::new();
        collect_component_references(&component.program.nodes, &mut references);
        for reference in references {
            visit_cycle(&reference, components, active, complete, diagnostics);
        }
    }
    active.pop();
    complete.insert(String::from(name));
}

fn collect_component_references(nodes: &[TemplateNode], references: &mut BTreeSet<String>) {
    for node in nodes {
        if let TemplateNodeKind::Component { name, .. } = &node.kind {
            references.insert(name.clone());
        }
        if let Some(children) = node.kind.children() {
            collect_component_references(children, references);
        }
    }
}

fn hash_file(
    hasher: &mut Sha256,
    project_root: &Path,
    path: &Path,
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    let relative = portable_path(path.strip_prefix(project_root).unwrap_or(path));
    match fs::read(path) {
        Ok(bytes) => {
            hasher.update(relative.as_bytes());
            hasher.update([0]);
            hasher.update(bytes);
            hasher.update([0]);
        }
        Err(error) => diagnostics.push(component_diagnostic(
            1401,
            &relative,
            0,
            0,
            &format!("Cannot hash component source: {error}"),
            "Check the source file permissions.",
        )),
    }
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn component_diagnostic(
    number: u16,
    source_path: &str,
    start: usize,
    end: usize,
    message: &str,
    help: &str,
) -> tachyon_diagnostics::Diagnostic {
    diagnostic(
        number,
        message,
        Some(String::from(help)),
        source_span(source_path, start, end),
    )
}

fn portable_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            PathComponent::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn sort_diagnostics(diagnostics: &mut [tachyon_diagnostics::Diagnostic]) {
    diagnostics.sort_by(|left, right| {
        let left_span = left.spans.first();
        let right_span = right.spans.first();
        left_span
            .map(|span| (&span.file, span.start, span.end))
            .cmp(&right_span.map(|span| (&span.file, span.start, span.end)))
            .then_with(|| left.code.cmp(&right.code))
    });
}

#[cfg(all(test, not(coverage)))]
mod tests {
    #![allow(clippy::expect_used)]

    use super::ComponentRegistry;
    use std::fs;

    fn write(path: &std::path::Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap_or_else(|| unreachable!()))
            .unwrap_or_else(|_| unreachable!());
        fs::write(path, contents).unwrap_or_else(|_| unreachable!());
    }

    #[test]
    fn a_component_directory_names_its_tag_at_any_depth() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let write = |relative: &str| {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().unwrap_or_else(|| unreachable!()))
                .unwrap_or_else(|_| unreachable!());
            fs::write(path, "<div>x</div>").unwrap_or_else(|_| unreachable!());
        };
        write("client/components/clicker/tac.html");
        write("client/components/date-picker/tac.html");
        write("client/components/product/card/tac.html");

        let registry = ComponentRegistry::discover(root.path()).expect("registry");
        let names = registry.names();
        // One segment, a hyphenated segment, and two segments all name a tag.
        assert!(names.contains("clicker"), "{names:?}");
        assert!(names.contains("date-picker"), "{names:?}");
        assert!(names.contains("product-card"), "{names:?}");
    }

    #[test]
    fn a_component_may_not_shadow_a_standard_html_element() {
        // Shadowing a real element would silently change what a template
        // means, so the name is refused rather than the tag reinterpreted.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let path = root.path().join("client/components/section/tac.html");
        fs::create_dir_all(path.parent().unwrap_or_else(|| unreachable!()))
            .unwrap_or_else(|_| unreachable!());
        fs::write(path, "<div>x</div>").unwrap_or_else(|_| unreachable!());

        let error = ComponentRegistry::discover(root.path()).expect_err("shadowed element");
        assert!(error.to_string().contains("TY1401"), "{error}");
        assert!(
            error.to_string().contains("standard HTML element"),
            "{error}"
        );
    }

    #[test]
    fn registry_discovers_components_scripts_and_a_stable_digest() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            &root.path().join("client/components/product/card/tac.html"),
            "<p>{label}</p>",
        );
        write(
            &root.path().join("client/components/product/card/tac.js"),
            "export default class Card {}",
        );
        let first = ComponentRegistry::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let second = ComponentRegistry::discover(root.path()).unwrap_or_else(|_| unreachable!());
        assert!(
            first
                .get("product-card")
                .is_some_and(super::ComponentDefinition::has_script)
        );
        assert_eq!(first.digest(), second.digest());
        assert!(
            first
                .get("product-card")
                .and_then(|value| value.script_path())
                .is_some()
        );
    }

    #[test]
    fn real_compiler_sidecar_wins_without_removing_the_legacy_companion() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let component = root.path().join("client/components/language/rust");
        write(&component.join("tac.html"), "<p>{count}</p>");
        write(&component.join("tac.rs"), "struct Legacy;");
        write(
            &component.join("tachyon-wasm.rs"),
            "#[no_mangle] pub extern \"C\" fn tac_alloc(_: i32) -> i32 { 0 }",
        );

        let registry = ComponentRegistry::discover(root.path()).expect("registry");
        let selected = registry
            .get("language-rust")
            .and_then(super::ComponentDefinition::wasm_path)
            .and_then(std::path::Path::file_name)
            .and_then(std::ffi::OsStr::to_str);
        assert_eq!(selected, Some("tachyon-wasm.rs"));
    }

    #[test]
    fn rewrite_island_sidecar_wins_without_removing_the_legacy_companion() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let component = root.path().join("client/components/dual/island");
        write(&component.join("tac.html"), "<p>{label}</p>");
        write(
            &component.join("tac.js"),
            "export default class LegacyIsland {}",
        );
        write(
            &component.join("tachyon-island.js"),
            "export default class RewriteIsland {}",
        );

        let registry = ComponentRegistry::discover(root.path()).expect("registry");
        let selected = registry
            .get("dual-island")
            .and_then(super::ComponentDefinition::script_path)
            .and_then(std::path::Path::file_name)
            .and_then(std::ffi::OsStr::to_str);
        assert_eq!(selected, Some("tachyon-island.js"));
    }

    #[test]
    fn invalid_shapes_orphans_and_cycles_fail_closed() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            &root.path().join("client/components/one/tac.html"),
            "<p>invalid single segment</p>",
        );
        write(
            &root.path().join("client/components/cycle/one/tac.html"),
            "<cycle-two></cycle-two>",
        );
        write(
            &root.path().join("client/components/cycle/two/tac.html"),
            "<cycle-one></cycle-one>",
        );
        write(
            &root.path().join("client/components/orphan/script/tac.js"),
            "export default class Orphan {}",
        );
        let error = ComponentRegistry::discover(root.path()).expect_err("invalid components");
        assert!(error.to_string().contains("TY1401"));
        assert!(error.to_string().contains("TY1403"));
    }

    #[test]
    fn invalid_component_roots_templates_companions_and_symlinks_fail_closed() {
        let root_file = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            &root_file.path().join("client/components"),
            "not a directory",
        );
        assert!(ComponentRegistry::discover(root_file.path()).is_err());

        let malformed = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            &malformed
                .path()
                .join("client/components/bad/template/tac.html"),
            "<main>",
        );
        // A polyglot companion has no adapter and must still raise TY1401.
        // tac.css is supported now, so it would prove nothing here.
        write(
            &malformed
                .path()
                .join("client/components/bad/template/tac.py"),
            "x = 1",
        );
        let error = ComponentRegistry::discover(malformed.path()).expect_err("malformed");
        assert!(error.to_string().contains("TY1301"));
        assert!(error.to_string().contains("TY1401"));

        let companion = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            &companion
                .path()
                .join("client/components/bad/script/tac.html"),
            "<p>safe</p>",
        );
        let script = companion.path().join("client/components/bad/script/tac.js");
        fs::write(&script, b"bad\0script").unwrap_or_else(|_| unreachable!());
        assert!(ComponentRegistry::discover(companion.path()).is_err());

        let oversized = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            &oversized
                .path()
                .join("client/components/large/script/tac.html"),
            "<p>safe</p>",
        );
        let script = oversized
            .path()
            .join("client/components/large/script/tac.js");
        fs::write(&script, vec![b'x'; 1_048_577]).unwrap_or_else(|_| unreachable!());
        assert!(ComponentRegistry::discover(oversized.path()).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let linked = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
            let outside = linked.path().join("outside");
            fs::create_dir_all(&outside).unwrap_or_else(|_| unreachable!());
            fs::create_dir_all(linked.path().join("client")).unwrap_or_else(|_| unreachable!());
            symlink(&outside, linked.path().join("client/components"))
                .unwrap_or_else(|_| unreachable!());
            assert!(ComponentRegistry::discover(linked.path()).is_err());
        }
    }
}
