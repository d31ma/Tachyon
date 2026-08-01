use super::expression::{Scope, truthy};
use super::model::{EventArgument, EventBinding};
use super::{
    AttributeValue, ComponentRegistry, HydrationPolicy, TemplateAttribute, TemplateNode,
    TemplateNodeKind, TemplateProgram, TextPart, is_trivia,
};
use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use tachyon_contracts::{ViewSourceMap, ViewSourceMapping};

const MAX_RENDERED_BYTES: usize = 10 * 1_024 * 1_024;
const MAX_RENDERED_NODES: usize = 100_000;
const MAX_LOOP_ITEMS: usize = 10_000;

#[derive(Clone, Debug)]
pub(crate) struct RenderedView {
    pub(crate) html: String,
    pub(crate) source_map: ViewSourceMap,
    pub(crate) islands: BTreeSet<String>,
    /// Whether any binding renders outside an island, which is what makes a
    /// page-level client module necessary.
    pub(crate) page_bindings: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ViewRenderer<'a> {
    components: &'a ComponentRegistry,
}

impl<'a> ViewRenderer<'a> {
    pub(crate) const fn new(components: &'a ComponentRegistry) -> Self {
        Self { components }
    }

    pub(crate) fn render(
        self,
        program: &TemplateProgram,
        route: &str,
        output_path: &str,
        scope: &Scope,
    ) -> Result<RenderedView, Failure> {
        self.render_internal(program, route, output_path, scope, None)
    }

    pub(crate) fn render_page_island(
        self,
        program: &TemplateProgram,
        route: &str,
        output_path: &str,
        scope: &Scope,
        module: &str,
    ) -> Result<RenderedView, Failure> {
        self.render_internal(program, route, output_path, scope, Some(module))
    }

    fn render_internal(
        self,
        program: &TemplateProgram,
        route: &str,
        output_path: &str,
        scope: &Scope,
        page_module: Option<&str>,
    ) -> Result<RenderedView, Failure> {
        let mut renderer = Renderer {
            components: self.components,
            route,
            html: String::new(),
            mappings: Vec::new(),
            sources: BTreeSet::new(),
            islands: BTreeSet::new(),
            island_depth: usize::from(page_module.is_some()),
            page_island: page_module.is_some(),
            page_bindings: false,
            island_index: 0,
            rendered_nodes: 0,
            suppressed_scope_roots: Vec::new(),
        };
        if !program.is_document {
            renderer.push_generated(
                "<!doctype html><html><head><meta charset=\"utf-8\"></head><body>",
            )?;
        } else if program.has_doctype {
            renderer.push_generated("<!doctype html>")?;
        }
        if let Some(module) = page_module {
            renderer.push_generated("<tachyon-island data-tachyon-id=\"")?;
            renderer.push_generated(&island_id(route, "$page", 0))?;
            renderer.push_generated(
                "\" data-tachyon-component=\"$page\" data-tachyon-hydrate=\"load\" \
                 data-tachyon-page=\"true\" data-tachyon-module=\"",
            )?;
            renderer.push_generated(&escape_attribute(module))?;
            renderer.push_generated("\" data-tachyon-props=\"")?;
            renderer.push_generated(&escape_attribute(&safe_json(&Value::Object(
                scope.clone().into_iter().collect(),
            ))))?;
            renderer.push_generated("\">")?;
        }
        renderer.render_siblings(&program.nodes, scope, None)?;
        if page_module.is_some() {
            renderer.push_generated("</tachyon-island>")?;
        }
        if page_module.is_some() || !renderer.islands.is_empty() {
            let script = "<script type=\"module\" src=\"/.tachyon/islands.js\"></script>";
            if program.is_document {
                let position = renderer
                    .html
                    .rfind("</body>")
                    .unwrap_or(renderer.html.len());
                if renderer.html.len().saturating_add(script.len()) > MAX_RENDERED_BYTES {
                    return Err(output_limit());
                }
                renderer.html.insert_str(position, script);
                for mapping in &mut renderer.mappings {
                    if mapping.generated_start >= position as u64 {
                        mapping.generated_start += script.len() as u64;
                    }
                    if mapping.generated_end >= position as u64 {
                        mapping.generated_end += script.len() as u64;
                    }
                }
            } else {
                renderer.push_generated(script)?;
            }
        }
        if !program.is_document {
            renderer.push_generated("</body></html>")?;
        }
        renderer
            .mappings
            .sort_by_key(|mapping| (mapping.generated_start, mapping.generated_end));
        let source_map = ViewSourceMap::v1(
            String::from(output_path),
            renderer.sources.into_iter().collect(),
            renderer.mappings,
        );
        Ok(RenderedView {
            html: renderer.html,
            source_map,
            islands: renderer.islands,
            page_bindings: renderer.page_bindings,
        })
    }
}

#[derive(Clone)]
struct SlotContent {
    nodes: Vec<TemplateNode>,
    scope: Scope,
}

struct Renderer<'a> {
    components: &'a ComponentRegistry,
    route: &'a str,
    html: String,
    mappings: Vec<ViewSourceMapping>,
    sources: BTreeSet<String>,
    islands: BTreeSet<String>,
    /// Depth of enclosing hydrated islands; deferral is allowed above zero.
    island_depth: usize,
    /// A route-level legacy Tac class owns the complete page subtree.
    page_island: bool,
    /// Set when a binding renders outside every island.
    page_bindings: bool,
    island_index: usize,
    rendered_nodes: usize,
    /// Hydrated components carry their style scope on the island host, exactly
    /// like the legacy component wrapper. Suppress that component's duplicate
    /// marker on its template roots while leaving nested component scopes
    /// untouched.
    suppressed_scope_roots: Vec<String>,
}

impl Renderer<'_> {
    fn render_siblings(
        &mut self,
        nodes: &[TemplateNode],
        scope: &Scope,
        slot: Option<&SlotContent>,
    ) -> Result<(), Failure> {
        let mut index = 0;
        while index < nodes.len() {
            let node = &nodes[index];
            if let TemplateNodeKind::Conditional {
                condition: Some(condition),
                children,
                ..
            } = &node.kind
            {
                if truthy(&condition.evaluate(scope)?) {
                    self.render_siblings(children, scope, slot)?;
                    index = skip_conditional_tail(nodes, index + 1);
                } else {
                    index = self.render_conditional_tail(nodes, index + 1, scope, slot)?;
                }
                continue;
            }
            if matches!(node.kind, TemplateNodeKind::Conditional { .. }) {
                index += 1;
                continue;
            }
            self.render_node(node, scope, slot)?;
            index += 1;
        }
        Ok(())
    }

    fn render_conditional_tail(
        &mut self,
        nodes: &[TemplateNode],
        mut index: usize,
        scope: &Scope,
        slot: Option<&SlotContent>,
    ) -> Result<usize, Failure> {
        while index < nodes.len() && is_trivia(&nodes[index]) {
            index += 1;
        }
        let Some(node) = nodes.get(index) else {
            return Ok(index);
        };
        match &node.kind {
            TemplateNodeKind::Conditional {
                condition: Some(condition),
                children,
                ..
            } => {
                if truthy(&condition.evaluate(scope)?) {
                    self.render_siblings(children, scope, slot)?;
                    Ok(skip_conditional_tail(nodes, index + 1))
                } else {
                    self.render_conditional_tail(nodes, index + 1, scope, slot)
                }
            }
            TemplateNodeKind::Conditional {
                condition: None,
                children,
                ..
            } => {
                self.render_siblings(children, scope, slot)?;
                Ok(index + 1)
            }
            _ => Ok(index),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render_node(
        &mut self,
        node: &TemplateNode,
        scope: &Scope,
        slot: Option<&SlotContent>,
    ) -> Result<(), Failure> {
        self.rendered_nodes += 1;
        if self.rendered_nodes > MAX_RENDERED_NODES {
            return Err(render_failure(
                1305,
                node,
                "Rendered view exceeds the limit of 100,000 nodes.",
            ));
        }
        let generated_start = self.html.len();
        match &node.kind {
            // Desugared into a conditional chain during parsing, so reaching
            // this arm would mean the desugar pass was skipped.
            TemplateNodeKind::Switch { .. } | TemplateNodeKind::Case { .. } => {
                return Err(render_failure(
                    1303,
                    node,
                    "Switch was not lowered before rendering.",
                ));
            }

            TemplateNodeKind::Element {
                tag,
                attributes,
                children,
                void,
            } => {
                self.push_generated("<")?;
                self.push_generated(tag)?;
                for (name, attribute) in attributes {
                    if name == super::SCOPE_ATTRIBUTE
                        && self.suppressed_scope_roots.last().is_some_and(|scope| {
                            matches!(
                                &attribute.value,
                                AttributeValue::Static(value) if value == scope
                            )
                        })
                    {
                        continue;
                    }
                    // An assignment writes to a companion instance, which only
                    // exists inside a hydrated island.
                    if let AttributeValue::Event(binding) = &attribute.value {
                        // An island's bindings resolve on its own companion, so
                        // only a binding outside every island needs the page's
                        // client module.
                        self.page_bindings |= self.island_depth == 0;
                        if binding.assign.is_some() && self.island_depth == 0 {
                            return Err(render_failure(
                                1306,
                                node,
                                "An assigning event binding needs a companion instance, which \
                                 exists only inside a hydrated island.",
                            ));
                        }
                    }
                    let value = property_value(attribute, scope)?;
                    if matches!(value, Value::Null | Value::Bool(false)) {
                        continue;
                    }
                    self.push_generated(" ")?;
                    self.push_generated(name)?;
                    if !matches!(value, Value::Bool(true))
                        && (!display_value(&value).is_empty()
                            || !matches!(attribute.value, AttributeValue::Static(_)))
                    {
                        self.push_generated("=\"")?;
                        self.push_generated(&escape_attribute(&display_value(&value)))?;
                        self.push_generated("\"")?;
                    }
                }
                self.push_generated(">")?;
                if !void {
                    self.render_siblings(children, scope, slot)?;
                    self.push_generated("</")?;
                    self.push_generated(tag)?;
                    self.push_generated(">")?;
                }
            }
            TemplateNodeKind::Text(parts) => {
                for part in parts {
                    match part {
                        TextPart::Literal(value, _) => self.push_generated(value)?,
                        TextPart::Interpolation(expression, _) => {
                            if self.page_island && self.island_depth > 0 {
                                self.push_generated("<tachyon-expr data-tachyon-expression=\"")?;
                                self.push_generated(&escape_attribute(
                                    &expression.to_client_json(),
                                ))?;
                                self.push_generated("\"></tachyon-expr>")?;
                                continue;
                            }
                            match expression.evaluate(scope) {
                                Ok(value) => {
                                    self.push_generated(&escape_text(&display_value(&value)))?;
                                }
                                // Inside an island the value can come from the
                                // companion, which has not run yet, so the
                                // expression travels to the client instead of
                                // failing the build. See ADR 0010.
                                Err(failure) if self.island_depth > 0 => {
                                    self.push_generated(
                                        "<tachyon-expr data-tachyon-expression=\"",
                                    )?;
                                    self.push_generated(&escape_attribute(
                                        &expression.to_client_json(),
                                    ))?;
                                    self.push_generated("\"></tachyon-expr>")?;
                                    let _ = failure;
                                }
                                Err(failure) => return Err(failure),
                            }
                        }
                    }
                }
            }
            TemplateNodeKind::Comment(value) => {
                self.push_generated("<!--")?;
                self.push_generated(value.trim_start_matches("<!--").trim_end_matches("-->"))?;
                self.push_generated("-->")?;
            }
            TemplateNodeKind::Iteration {
                binding,
                iterable,
                children,
            } => {
                let value = iterable.evaluate(scope)?;
                let Value::Array(items) = value else {
                    return Err(render_failure(
                        1303,
                        node,
                        "Iteration expression must evaluate to an array.",
                    ));
                };
                if items.len() > MAX_LOOP_ITEMS {
                    return Err(render_failure(
                        1305,
                        node,
                        "Iteration exceeds the limit of 10,000 items.",
                    ));
                }
                for item in items {
                    let mut local = scope.clone();
                    local.insert(binding.clone(), item);
                    self.render_siblings(children, &local, slot)?;
                }
            }
            TemplateNodeKind::Component {
                name,
                properties,
                hydrate,
                children,
            } => self.render_component(node, name, properties, *hydrate, children, scope)?,
            TemplateNodeKind::Slot => {
                if let Some(slot) = slot {
                    self.render_siblings(&slot.nodes, &slot.scope, None)?;
                }
            }
            TemplateNodeKind::Conditional { .. } => {}
        }
        let generated_end = self.html.len();
        if generated_end > generated_start {
            self.sources.insert(node.source_path.clone());
            self.mappings.push(ViewSourceMapping {
                generated_start: generated_start as u64,
                generated_end: generated_end as u64,
                source: node.source_path.clone(),
                source_start: node.range.start as u64,
                source_end: node.range.end as u64,
            });
        }
        Ok(())
    }

    fn render_component(
        &mut self,
        node: &TemplateNode,
        name: &str,
        properties: &BTreeMap<String, TemplateAttribute>,
        declared_policy: Option<HydrationPolicy>,
        children: &[TemplateNode],
        parent_scope: &Scope,
    ) -> Result<(), Failure> {
        let component = self.components.get(name).ok_or_else(|| {
            render_failure(1402, node, &format!("Unknown Tac component '<{name}>'."))
        })?;
        let mut props = Scope::new();
        for (property, attribute) in properties {
            props.insert(property.clone(), property_value(attribute, parent_scope)?);
        }
        if children.iter().any(|child| !is_trivia(child))
            && !contains_slot(&component.program().nodes)
        {
            return Err(render_failure(
                1404,
                node,
                &format!("Component '<{name}>' received children but declares no <slot>."),
            ));
        }
        let policy = declared_policy.or_else(|| {
            if component.has_script() {
                Some(HydrationPolicy::Load)
            } else {
                None
            }
        });
        if policy.is_some_and(|value| value != HydrationPolicy::Never) && !component.has_script() {
            return Err(render_failure(
                1405,
                node,
                &format!("Hydrated component '<{name}>' has no tac.js companion."),
            ));
        }
        if let Some(policy) = policy {
            self.island_index += 1;
            let island_id = island_id(self.route, name, self.island_index);
            self.push_generated("<tachyon-island data-tachyon-id=\"")?;
            self.push_generated(&island_id)?;
            self.push_generated("\" data-tachyon-component=\"")?;
            self.push_generated(name)?;
            self.push_generated("\" data-tachyon-hydrate=\"")?;
            self.push_generated(policy.name())?;
            self.push_generated("\"")?;
            if component.style_path().is_some() {
                self.push_generated(" ")?;
                self.push_generated(crate::template::SCOPE_ATTRIBUTE)?;
                self.push_generated("=\"")?;
                self.push_generated(name)?;
                self.push_generated("\"")?;
            }
            if policy != HydrationPolicy::Never {
                self.islands.insert(String::from(name));
                // A companion compiled to wasm is loaded through the ABI of
                // ADR 0011 rather than imported as a module.
                if let Some(source) = component.wasm_path() {
                    self.push_generated(" data-tachyon-wasm=\"/.tachyon/components/")?;
                    self.push_generated(name)?;
                    self.push_generated(crate::wasm::asset_suffix(source))?;
                    self.push_generated("\" data-tachyon-props=\"")?;
                } else {
                    self.push_generated(" data-tachyon-module=\"/.tachyon/components/")?;
                    self.push_generated(name)?;
                    self.push_generated(".js\" data-tachyon-props=\"")?;
                }
                self.push_generated(&escape_attribute(&safe_json(&Value::Object(
                    props.clone().into_iter().collect(),
                ))))?;
                self.push_generated("\"")?;
            }
            self.push_generated(">")?;
            let slot = SlotContent {
                nodes: children.to_vec(),
                scope: parent_scope.clone(),
            };
            // A never-hydrated island has no companion instance on the client,
            // so nothing may be deferred inside it.
            let hydrated = usize::from(policy != HydrationPolicy::Never);
            self.island_depth += hydrated;
            self.suppressed_scope_roots.push(String::from(name));
            let rendered = self
                .render_siblings(&component.program().nodes, &props, Some(&slot))
                .map_err(|failure| companion_scope_help(failure, name, component.has_script()));
            self.suppressed_scope_roots.pop();
            self.island_depth -= hydrated;
            rendered?;
            self.push_generated("</tachyon-island>")?;
        } else {
            let slot = SlotContent {
                nodes: children.to_vec(),
                scope: parent_scope.clone(),
            };
            self.render_siblings(&component.program().nodes, &props, Some(&slot))
                .map_err(|failure| companion_scope_help(failure, name, component.has_script()))?;
        }
        Ok(())
    }

    fn push_generated(&mut self, value: &str) -> Result<(), Failure> {
        if self.html.len().saturating_add(value.len()) > MAX_RENDERED_BYTES {
            return Err(output_limit());
        }
        self.html.push_str(value);
        Ok(())
    }
}

/// Explains why a component template cannot see its companion's fields.
///
/// "References a missing or incompatible value" is true but misleading here:
/// the value is missing because a component template renders before any
/// companion runs, so a field the companion assigns is never in scope. Saying
/// so is the difference between a developer checking a typo and understanding
/// where the data has to come from.
fn companion_scope_help(failure: Failure, component: &str, has_script: bool) -> Failure {
    if !has_script {
        return failure;
    }
    let diagnostics = failure
        .diagnostics()
        .iter()
        .cloned()
        .map(|mut diagnostic| {
            if diagnostic.code.number() == 1303 {
                diagnostic.help = Some(format!(
                    "A component template renders before any companion runs, so a field \
                     assigned in '{component}' tac.js is not in scope. Pass the value in as \
                     a property, or move the subtree into an island."
                ));
            }
            diagnostic
        })
        .collect();
    Failure::new(diagnostics)
}

fn skip_conditional_tail(nodes: &[TemplateNode], mut index: usize) -> usize {
    loop {
        while index < nodes.len() && is_trivia(&nodes[index]) {
            index += 1;
        }
        if nodes
            .get(index)
            .is_some_and(|node| matches!(node.kind, TemplateNodeKind::Conditional { .. }))
        {
            index += 1;
        } else {
            return index;
        }
    }
}

fn property_value(attribute: &TemplateAttribute, scope: &Scope) -> Result<Value, Failure> {
    match &attribute.value {
        AttributeValue::Static(value) | AttributeValue::Control(value) => {
            Ok(Value::String(value.clone()))
        }
        AttributeValue::Dynamic(expression) => expression.evaluate(scope),
        AttributeValue::Event(binding) => Ok(Value::String(event_payload(binding, scope)?)),
    }
}

/// Serialises an event binding once the render scope is known.
///
/// A scope argument is resolved here rather than at click time, so a loop
/// passes the item it is iterating: `on:click="select(tab.id)"` becomes that
/// row's id. The payload is JSON the client runtime reads; it is never source
/// the browser evaluates.
fn event_payload(binding: &EventBinding, scope: &Scope) -> Result<String, Failure> {
    let mut arguments = Vec::with_capacity(binding.arguments.len());
    for argument in &binding.arguments {
        arguments.push(match argument {
            EventArgument::Literal(json) => format!(r#"{{"v":{json}}}"#),
            EventArgument::EventPath(path) => format!(r#"{{"e":"{path}"}}"#),
            EventArgument::Scope(expression) => {
                format!(r#"{{"v":{}}}"#, safe_json(&expression.evaluate(scope)?))
            }
        });
    }
    if let Some(assignment) = &binding.assign {
        return Ok(format!(
            r#"{{"s":"{}","op":"{}","a":[{}]}}"#,
            assignment.target,
            assignment.operator,
            arguments.join(",")
        ));
    }
    Ok(format!(
        r#"{{"h":"{}","a":[{}]}}"#,
        binding.handler,
        arguments.join(",")
    ))
}

fn display_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => safe_json(value),
    }
}

fn contains_slot(nodes: &[TemplateNode]) -> bool {
    nodes.iter().any(|node| {
        matches!(node.kind, TemplateNodeKind::Slot)
            || node.kind.children().is_some_and(contains_slot)
    })
}

fn safe_json(value: &Value) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| String::from("null"))
        .replace('<', "\\u003c")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn escape_attribute(value: &str) -> String {
    escape_text(value)
}

fn island_id(route: &str, component: &str, index: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(route.as_bytes());
    hasher.update([0]);
    hasher.update(component.as_bytes());
    hasher.update([0]);
    hasher.update(index.to_le_bytes());
    let digest = hasher.finalize();
    let mut encoded = String::from("ty-");
    for byte in digest.iter().take(8) {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn render_failure(number: u16, node: &TemplateNode, message: &str) -> Failure {
    Failure::one(diagnostic(
        number,
        message,
        None,
        source_span(
            &node.source_path,
            node.range.start,
            node.range.end.max(node.range.start),
        ),
    ))
}

fn output_limit() -> Failure {
    Failure::one(diagnostic(
        1305,
        "Rendered view exceeds the 10 MiB output limit.",
        Some(String::from("Reduce generated content or split the route.")),
        None,
    ))
}

#[cfg(all(test, not(coverage)))]
mod tests {
    #![allow(clippy::expect_used)]

    use super::ViewRenderer;
    use crate::template::{ComponentRegistry, Scope, TemplateFrontend};
    use serde_json::json;
    use std::fs;

    #[test]
    fn controls_components_slots_and_escaping_render_without_scope_leaks() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let component = root.path().join("client/components/product/card/tac.html");
        fs::create_dir_all(component.parent().unwrap_or_else(|| unreachable!()))
            .unwrap_or_else(|_| unreachable!());
        fs::write(
            component,
            "<article :data-id=\"product.id\"><slot></slot>{product.name}</article>",
        )
        .unwrap_or_else(|_| unreachable!());
        let registry = ComponentRegistry::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let source = "<loop :for=\"product of products\"><product-card :product=\"product\"><b>{title}</b></product-card></loop>";
        let program = TemplateFrontend::compile(source, "client/pages/tac.html", &registry.names())
            .unwrap_or_else(|_| unreachable!());
        let scope = Scope::from([
            (String::from("title"), json!("Products")),
            (
                String::from("products"),
                json!([{"id": "\" bad", "name": "<unsafe>"}]),
            ),
        ]);
        let rendered = ViewRenderer::new(&registry)
            .render(&program, "/", "index.html", &scope)
            .unwrap_or_else(|_| unreachable!());
        assert!(rendered.html.contains("data-id=\"&quot; bad\""));
        assert!(rendered.html.contains("<b>Products</b>&lt;unsafe&gt;"));
        assert!(!rendered.html.contains("<loop"));
        assert_eq!(rendered.source_map.sources.len(), 2);
    }

    #[test]
    fn an_event_binding_resolves_scope_arguments_where_it_renders() {
        // A row that cannot say which row it is would make a list of clickable
        // items impossible, so a loop passes the item it is iterating.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let registry = ComponentRegistry::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let source = "<loop :for=\"tab of tabs\">\
                      <button on:click=\"select(tab.id)\">{tab.label}</button></loop>";
        let program = TemplateFrontend::compile(source, "client/pages/tac.html", &registry.names())
            .unwrap_or_else(|_| unreachable!());
        let scope = Scope::from([(
            String::from("tabs"),
            json!([{"id": "one", "label": "One"}, {"id": "two", "label": "Two"}]),
        )]);
        let rendered = ViewRenderer::new(&registry)
            .render(&program, "/", "index.html", &scope)
            .unwrap_or_else(|_| unreachable!());

        // Each row carries its own resolved argument, as JSON the runtime
        // reads rather than source the browser evaluates.
        assert!(
            rendered.html.contains(
                "{&quot;h&quot;:&quot;select&quot;,&quot;a&quot;:[{&quot;v&quot;:&quot;one&quot;}]}"
            ),
            "{}",
            rendered.html
        );
        assert!(
            rendered.html.contains("{&quot;v&quot;:&quot;two&quot;}"),
            "{}",
            rendered.html
        );
        // The authored expression never survives into the document.
        assert!(!rendered.html.contains("tab.id"), "{}", rendered.html);
    }

    #[test]
    fn an_assigning_binding_needs_an_island_companion() {
        // Assignment writes to a companion instance, so it is refused where
        // there is none. The diagnostic names the reason rather than the
        // handler.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let registry = ComponentRegistry::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let program = TemplateFrontend::compile(
            r#"<main><input on:input="query = $event.target.value"></main>"#,
            "client/pages/tac.html",
            &registry.names(),
        )
        .unwrap_or_else(|_| unreachable!());

        let failure = ViewRenderer::new(&registry)
            .render(&program, "/", "index.html", &Scope::new())
            .expect_err("a page has no companion instance");
        assert!(
            failure.to_string().contains("companion instance"),
            "{failure}"
        );
    }

    #[test]
    fn a_companion_field_in_a_component_template_says_why_it_is_missing() {
        // A component with a companion is an island by default, and an island
        // defers what the server cannot resolve (ADR 0010). The diagnostic
        // still matters for a component that opts out of hydration: there is
        // no instance on the client, so nothing can fill the value in, and
        // "missing or incompatible value" would send a developer hunting for a
        // typo in a name that is spelled correctly.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let directory = root.path().join("client/components/panel/rows");
        fs::create_dir_all(&directory).unwrap_or_else(|_| unreachable!());
        fs::write(directory.join("tac.html"), "<ul><li>{rows}</li></ul>")
            .unwrap_or_else(|_| unreachable!());
        fs::write(
            directory.join("tac.js"),
            "export default class { rows = [] }\n",
        )
        .unwrap_or_else(|_| unreachable!());

        let registry = ComponentRegistry::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let program = TemplateFrontend::compile(
            r#"<main><panel-rows hydrate="never" /></main>"#,
            "client/pages/tac.html",
            &registry.names(),
        )
        .unwrap_or_else(|_| unreachable!());

        let failure = ViewRenderer::new(&registry)
            .render(&program, "/", "index.html", &Scope::new())
            .expect_err("a never-hydrated component has no instance to defer to");
        let help = failure
            .diagnostics()
            .first()
            .and_then(|diagnostic| diagnostic.help.clone())
            .unwrap_or_default();
        assert!(help.contains("before any companion runs"), "{help}");
        assert!(help.contains("panel-rows"), "{help}");
    }

    #[test]
    fn boolean_attributes_and_missing_component_slots_are_explicit() {
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        let component = root.path().join("client/components/plain/card/tac.html");
        fs::create_dir_all(component.parent().unwrap_or_else(|| unreachable!()))
            .unwrap_or_else(|_| unreachable!());
        fs::write(&component, "<p>{label}</p>").unwrap_or_else(|_| unreachable!());
        let registry = ComponentRegistry::discover(root.path()).unwrap_or_else(|_| unreachable!());
        let attributes = TemplateFrontend::compile(
            "<input :disabled=\"disabled\" :hidden=\"missing\" :required=\"required\">",
            "client/pages/tac.html",
            &registry.names(),
        )
        .unwrap_or_else(|_| unreachable!());
        let scope = Scope::from([
            (String::from("disabled"), json!(false)),
            (String::from("missing"), json!(null)),
            (String::from("required"), json!(true)),
        ]);
        let rendered = ViewRenderer::new(&registry)
            .render(&attributes, "/", "index.html", &scope)
            .unwrap_or_else(|_| unreachable!());
        assert!(rendered.html.contains("<input required>"));
        assert!(!rendered.html.contains("disabled"));
        assert!(!rendered.html.contains("hidden"));

        let invalid = TemplateFrontend::compile(
            "<plain-card label=\"Card\"><strong>child</strong></plain-card>",
            "client/pages/tac.html",
            &registry.names(),
        )
        .unwrap_or_else(|_| unreachable!());
        let error = ViewRenderer::new(&registry)
            .render(&invalid, "/", "index.html", &Scope::new())
            .expect_err("missing slot");
        assert!(error.to_string().contains("TY1404"));
    }
}
