use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::template::{
    AttributeValue, ComponentRegistry, EventArgument, TemplateFrontend, TemplateNode,
    TemplateNodeKind, TextPart, is_trivia,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use tachyon_contracts::{
    NativeAccessibility, NativeNode, NativeTarget, NativeUi, WebSurfaceBridge, WebSurfaceSource,
};

const MAX_NATIVE_DEPTH: usize = 64;
const MAX_NATIVE_NODES: usize = 100_000;
const MAX_WEB_SURFACES: usize = 1_024;
const MAX_FALLBACK_BYTES: usize = 10 * 1_024 * 1_024;
const MAX_STATE_ENTRIES: usize = 1_024;
const MAX_STATE_VALUE_BYTES: usize = 4 * 1_024;
const GENERATED_CONTROLLER_SCRIPT: &str =
    r#"<script type="module" src="/.tachyon/native-controller.js"></script>"#;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct NativeRouteIndexEntry {
    pub(super) route: String,
    pub(super) document: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct NativeRouteIndex {
    pub(super) contract_version: u8,
    pub(super) entry_route: String,
    pub(super) routes: Vec<NativeRouteIndexEntry>,
    pub(super) initial_state: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WebSurfaceArtifact {
    pub(super) id: String,
    pub(super) document: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PlannedNativeRoute {
    pub(super) route: String,
    pub(super) document_key: String,
    pub(super) native_ui: NativeUi,
    pub(super) initial_state: BTreeMap<String, String>,
    pub(super) web_surfaces: Vec<WebSurfaceArtifact>,
    pub(super) web_surface_count: usize,
    pub(super) native_node_count: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct NativePlanner;

impl NativePlanner {
    #[cfg(any(test, feature = "fuzzing"))]
    pub(super) fn plan(
        target: NativeTarget,
        route: &str,
        source_path: &str,
        rendered_html: &str,
        styles: &str,
    ) -> Result<PlannedNativeRoute, Failure> {
        Self::plan_internal(
            target,
            route,
            source_path,
            rendered_html,
            styles,
            None,
            BTreeMap::new(),
        )
    }

    pub(super) fn plan_with_components_and_state(
        target: NativeTarget,
        route: &str,
        source_path: &str,
        authored_html: &str,
        styles: &str,
        components: &ComponentRegistry,
        initial_state: BTreeMap<String, String>,
    ) -> Result<PlannedNativeRoute, Failure> {
        Self::plan_internal(
            target,
            route,
            source_path,
            authored_html,
            styles,
            Some(components),
            initial_state,
        )
    }

    fn plan_internal(
        target: NativeTarget,
        route: &str,
        source_path: &str,
        rendered_html: &str,
        styles: &str,
        components: Option<&ComponentRegistry>,
        mut initial_state: BTreeMap<String, String>,
    ) -> Result<PlannedNativeRoute, Failure> {
        let source = strip_generated_assets(rendered_html);
        let (source, page_state) = lower_page_island(&source);
        initial_state.extend(page_state);
        let names = components.map_or_else(BTreeSet::new, ComponentRegistry::names);
        let program = TemplateFrontend::compile(&source, source_path, &names)?;
        let expanded_nodes = if let Some(components) = components {
            expand_native_components(&program.nodes, components, None, None)?
        } else {
            program.nodes
        };
        let root_source = find_body(&expanded_nodes)
            .or_else(|| expanded_nodes.iter().find(|node| !is_trivia(node)))
            .ok_or_else(|| native_failure(1602, source_path, 0, 0, "Resolved view is empty."))?;
        validate_initial_state(source_path, &initial_state)?;
        let mut context = PlanningContext {
            target,
            source: &source,
            source_path,
            route_key: route_key(route),
            styles,
            sequence: 0,
            native_nodes: 0,
            web_surfaces: Vec::new(),
            surface_count: 0,
            initial_state,
            action_references: Vec::new(),
        };
        let root = context.plan_node(root_source, 0)?.ok_or_else(|| {
            native_failure(
                1602,
                source_path,
                root_source.range.start,
                root_source.range.end,
                "Resolved view has no native-visible root.",
            )
        })?;
        context.validate_actions()?;
        Ok(PlannedNativeRoute {
            route: String::from(route),
            document_key: route_key(route),
            native_ui: NativeUi::v1(target, root),
            initial_state: context.initial_state,
            web_surfaces: context.web_surfaces,
            web_surface_count: context.surface_count,
            native_node_count: context.native_nodes,
        })
    }
}

fn validate_initial_state(
    source_path: &str,
    state: &BTreeMap<String, String>,
) -> Result<(), Failure> {
    if state.len() > MAX_STATE_ENTRIES {
        return Err(native_failure(
            1603,
            source_path,
            0,
            0,
            "Native state exceeds the limit of 1,024 entries.",
        ));
    }
    for (name, value) in state {
        if !valid_state_name(name) {
            return Err(native_failure(
                1603,
                source_path,
                0,
                0,
                "Native state binding name is invalid.",
            ));
        }
        if value.len() > MAX_STATE_VALUE_BYTES {
            return Err(native_failure(
                1603,
                source_path,
                0,
                0,
                "Native state value exceeds the 4 KiB limit.",
            ));
        }
    }
    Ok(())
}

struct PlanningContext<'a> {
    target: NativeTarget,
    source: &'a str,
    source_path: &'a str,
    /// Distinguishes one route's generated ids from another's.
    route_key: String,
    /// The route's own stylesheets, carried into every fallback document.
    styles: &'a str,
    sequence: usize,
    native_nodes: usize,
    web_surfaces: Vec<WebSurfaceArtifact>,
    surface_count: usize,
    initial_state: BTreeMap<String, String>,
    action_references: Vec<(String, String, usize, usize)>,
}

impl PlanningContext<'_> {
    fn plan_node(
        &mut self,
        node: &TemplateNode,
        depth: usize,
    ) -> Result<Option<NativeNode>, Failure> {
        if depth > MAX_NATIVE_DEPTH {
            return Err(self.failure(1602, node, "Native view exceeds the depth limit of 64."));
        }
        self.native_nodes += 1;
        if self.native_nodes > MAX_NATIVE_NODES {
            return Err(self.failure(
                1602,
                node,
                "Native view exceeds the limit of 100,000 nodes.",
            ));
        }
        match &node.kind {
            TemplateNodeKind::Text(parts) => {
                let value = decode_html(&text_parts(parts));
                if value.trim().is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(NativeNode::Text { value }))
                }
            }
            // Switch and Case are desugared into a conditional chain during
            // parsing, so the planner never receives one.
            TemplateNodeKind::Comment(_)
            | TemplateNodeKind::Switch { .. }
            | TemplateNodeKind::Case { .. }
            | TemplateNodeKind::Slot => Ok(None),
            TemplateNodeKind::Element {
                tag,
                attributes,
                children,
                ..
            } => self.plan_element(node, tag, attributes, children, depth),
            TemplateNodeKind::Component { .. } => {
                Err(self.failure(1602, node, "Unexpanded component reached native planning."))
            }
            TemplateNodeKind::Conditional { .. } | TemplateNodeKind::Iteration { .. } => Err(self
                .failure(
                    1602,
                    node,
                    "Unresolved compiler syntax reached native planning.",
                )),
        }
    }

    fn plan_element(
        &mut self,
        node: &TemplateNode,
        tag: &str,
        attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
        children: &[TemplateNode],
        depth: usize,
    ) -> Result<Option<NativeNode>, Failure> {
        if matches!(tag, "head" | "title" | "meta" | "link" | "style") {
            return Ok(None);
        }
        if tag == "iframe" {
            return self.remote_surface(node, attributes);
        }
        if tag == "tachyon-island" || adapter_for(tag, attributes).is_none() {
            return self.local_surface(node, tag, attributes);
        }

        let adapter = adapter_for(tag, attributes).unwrap_or_else(|| unreachable!());
        let id = self.next_id();
        let mut properties = lower_properties(attributes);
        let mut events = BTreeMap::new();
        self.collect_state(node, tag, attributes, &mut properties, &mut events)?;
        let text = visible_text(children);
        let accessibility = accessibility(tag, attributes, &text);
        validate_accessibility(tag, attributes, accessibility.as_ref())
            .map_err(|message| self.failure(1603, node, message))?;

        let child_source = if tag == "details" {
            children
                .iter()
                .filter(|child| element_tag(child).is_none_or(|value| value != "summary"))
                .collect::<Vec<_>>()
        } else {
            children.iter().collect()
        };
        if tag == "details" {
            let summary = children
                .iter()
                .find(|child| element_tag(child).is_some_and(|value| value == "summary"))
                .map_or_else(|| String::from("Details"), visible_node_text);
            properties.insert(String::from("label"), Value::String(summary));
        }

        let mut planned_children = Vec::new();
        for child in child_source {
            if let Some(planned) = self.plan_node(child, depth + 1)? {
                planned_children.push(planned);
            }
        }
        Ok(Some(NativeNode::NativeElement {
            id: Some(id),
            adapter: String::from(adapter),
            properties,
            events,
            accessibility,
            children: planned_children,
        }))
    }

    fn collect_state(
        &mut self,
        node: &TemplateNode,
        tag: &str,
        attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
        properties: &mut BTreeMap<String, Value>,
        events: &mut BTreeMap<String, String>,
    ) -> Result<(), Failure> {
        if let Some(binding) = static_attribute(attributes, "data-tachyon-bind") {
            if !valid_state_name(&binding) {
                return Err(self.failure(1603, node, "Native state binding name is invalid."));
            }
            properties.insert(String::from("binding"), Value::String(binding.clone()));
            if let Some(prefix) = static_attribute(attributes, "data-tachyon-prefix") {
                properties.insert(String::from("prefix"), Value::String(prefix));
            }
            if let Some(initial) = static_attribute(attributes, "data-tachyon-state") {
                if initial.len() > MAX_STATE_VALUE_BYTES {
                    return Err(self.failure(
                        1603,
                        node,
                        "Native state value exceeds the 4 KiB limit.",
                    ));
                }
                if let Some(previous) = self.initial_state.insert(binding.clone(), initial.clone())
                    && previous != initial
                {
                    return Err(self.failure(
                        1603,
                        node,
                        "Native state has more than one initial declaration.",
                    ));
                }
                if self.initial_state.len() > MAX_STATE_ENTRIES {
                    return Err(self.failure(
                        1603,
                        node,
                        "Native state exceeds the limit of 1,024 entries.",
                    ));
                }
            }
            if matches!(tag, "input" | "textarea") {
                events.insert(String::from("input"), String::from("dispatch"));
            }
        }
        let authored_action =
            attributes
                .get("data-tac-on-click")
                .and_then(|attribute| match &attribute.value {
                    AttributeValue::Event(binding)
                        if binding
                            .assign
                            .as_ref()
                            .is_some_and(|assignment| assignment.operator == "+=")
                            && matches!(
                                binding.arguments.as_slice(),
                                [EventArgument::Literal(value)] if value == "1"
                            ) =>
                    {
                        binding
                            .assign
                            .as_ref()
                            .map(|assignment| format!("increment:{}", assignment.target))
                    }
                    _ => None,
                });
        if let Some(action) =
            static_attribute(attributes, "data-tachyon-action").or(authored_action)
        {
            let Some((verb, binding)) = action.split_once(':') else {
                return Err(self.failure(1603, node, "Native action syntax is invalid."));
            };
            if !matches!(verb, "increment" | "toggle") || !valid_state_name(binding) {
                return Err(self.failure(1603, node, "Native action syntax is invalid."));
            }
            let verb = String::from(verb);
            let binding = String::from(binding);
            properties.insert(String::from("action"), Value::String(action));
            events.insert(String::from("click"), String::from("dispatch"));
            self.action_references
                .push((verb, binding, node.range.start, node.range.end));
        }
        Ok(())
    }

    fn validate_actions(&self) -> Result<(), Failure> {
        for (verb, binding, start, end) in &self.action_references {
            let Some(initial) = self.initial_state.get(binding) else {
                return Err(native_failure(
                    1603,
                    self.source_path,
                    *start,
                    *end,
                    &format!("Native action references undeclared state '{binding}'."),
                ));
            };
            if verb == "increment" && initial.parse::<i64>().is_err() {
                return Err(native_failure(
                    1603,
                    self.source_path,
                    *start,
                    *end,
                    &format!("Native increment state '{binding}' must start as an integer."),
                ));
            }
        }
        Ok(())
    }

    fn local_surface(
        &mut self,
        node: &TemplateNode,
        tag: &str,
        attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
    ) -> Result<Option<NativeNode>, Failure> {
        self.reserve_surface(node)?;
        let fragment = self
            .source
            .get(node.range.start..node.range.end)
            .ok_or_else(|| self.failure(1604, node, "WebSurface source range is invalid."))?;
        if fragment.len() > MAX_FALLBACK_BYTES {
            return Err(self.failure(1604, node, "WebSurface document exceeds the 10 MiB limit."));
        }
        let label = accessible_label(attributes)
            .or_else(|| island_label(attributes))
            .or_else(|| {
                let text = visible_node_text(node);
                (!text.trim().is_empty()).then_some(text)
            });
        if label.as_ref().is_none_or(|value| value.trim().is_empty()) {
            return Err(self.failure(
                1603,
                node,
                "WebSurface fallback requires an accessible name.",
            ));
        }
        let id = self.next_id();
        // A node id restarts at n_000001 for every route, which is fine inside
        // one document and fatal as a file name: `/` and `/docs` both write
        // WebSurfaces/n_000004, and the second route to build silently
        // replaced the first one's page. The payload is therefore stored under
        // the route as well, while the id stays what the contract specifies.
        let directory = format!("{}_{id}", self.route_key);
        let location = format!("WebSurfaces/{directory}/index.html");
        let reason = if tag == "tachyon-island" {
            String::from("Hydrated Tac island requires browser behavior.")
        } else {
            format!(
                "Element '<{tag}>' has no {} native adapter.",
                target_label(self.target)
            )
        };
        self.web_surfaces.push(WebSurfaceArtifact {
            id: directory,
            document: fallback_document(fragment, self.styles),
        });
        Ok(Some(NativeNode::WebSurface {
            id: Some(id),
            source: WebSurfaceSource::LocalBundle,
            location,
            bridge: WebSurfaceBridge::None,
            reason,
            accessibility: Some(NativeAccessibility {
                role: Some(String::from("group")),
                label,
            }),
        }))
    }

    fn remote_surface(
        &mut self,
        node: &TemplateNode,
        attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
    ) -> Result<Option<NativeNode>, Failure> {
        self.reserve_surface(node)?;
        let Some(location) = static_attribute(attributes, "src") else {
            return Err(self.failure(1604, node, "Remote WebSurface requires an HTTPS src."));
        };
        if !valid_https_url(&location) {
            return Err(self.failure(
                1604,
                node,
                "Remote WebSurface permits only a bounded HTTPS URL.",
            ));
        }
        let label = accessible_label(attributes);
        if label.as_ref().is_none_or(|value| value.trim().is_empty()) {
            return Err(self.failure(1603, node, "Remote WebSurface requires an accessible name."));
        }
        let id = self.next_id();
        Ok(Some(NativeNode::WebSurface {
            id: Some(id),
            source: WebSurfaceSource::RemoteUrl,
            location,
            bridge: WebSurfaceBridge::None,
            reason: String::from("Remote iframe content is isolated from native capabilities."),
            accessibility: Some(NativeAccessibility {
                role: Some(String::from("document")),
                label,
            }),
        }))
    }

    fn reserve_surface(&mut self, node: &TemplateNode) -> Result<(), Failure> {
        if self.surface_count >= MAX_WEB_SURFACES {
            return Err(self.failure(
                1604,
                node,
                "Application exceeds the limit of 1,024 WebSurfaces.",
            ));
        }
        self.surface_count += 1;
        Ok(())
    }

    fn next_id(&mut self) -> String {
        self.sequence += 1;
        format!("n_{:06}", self.sequence)
    }

    fn failure(&self, number: u16, node: &TemplateNode, message: &str) -> Failure {
        native_failure(
            number,
            self.source_path,
            node.range.start,
            node.range.end,
            message,
        )
    }
}

#[derive(Clone)]
struct NativeSlot {
    nodes: Vec<TemplateNode>,
    scope: Option<BTreeMap<String, String>>,
}

fn expand_native_components(
    nodes: &[TemplateNode],
    components: &ComponentRegistry,
    scope: Option<&BTreeMap<String, String>>,
    slot: Option<&NativeSlot>,
) -> Result<Vec<TemplateNode>, Failure> {
    let mut expanded = Vec::new();
    for node in nodes {
        match &node.kind {
            TemplateNodeKind::Component {
                name,
                properties,
                children,
                ..
            } => {
                expanded.extend(expand_native_component_node(
                    node, name, properties, children, components, scope,
                )?);
            }
            TemplateNodeKind::Slot => {
                if let Some(slot) = slot {
                    expanded.extend(expand_native_components(
                        &slot.nodes,
                        components,
                        slot.scope.as_ref(),
                        None,
                    )?);
                }
            }
            TemplateNodeKind::Element {
                tag,
                attributes,
                children,
                void,
            } => {
                let mut clone = node.clone();
                clone.kind = TemplateNodeKind::Element {
                    tag: tag.clone(),
                    attributes: if let Some(scope) = scope {
                        resolve_native_attributes(node, attributes, scope)?
                    } else {
                        attributes.clone()
                    },
                    children: expand_native_components(children, components, scope, slot)?,
                    void: *void,
                };
                expanded.push(clone);
            }
            TemplateNodeKind::Text(parts) => {
                let mut clone = node.clone();
                clone.kind = TemplateNodeKind::Text(if let Some(scope) = scope {
                    resolve_native_text(node, parts, scope)?
                } else {
                    parts.clone()
                });
                expanded.push(clone);
            }
            TemplateNodeKind::Comment(_) => expanded.push(node.clone()),
            TemplateNodeKind::Conditional { .. }
            | TemplateNodeKind::Iteration { .. }
            | TemplateNodeKind::Switch { .. }
            | TemplateNodeKind::Case { .. } => {
                return Err(native_failure(
                    1602,
                    &node.source_path,
                    node.range.start,
                    node.range.end,
                    "Unresolved compiler syntax reached native component expansion.",
                ));
            }
        }
    }
    Ok(expanded)
}

fn expand_native_component_node(
    node: &TemplateNode,
    name: &str,
    properties: &BTreeMap<String, crate::template::TemplateAttribute>,
    children: &[TemplateNode],
    components: &ComponentRegistry,
    scope: Option<&BTreeMap<String, String>>,
) -> Result<Vec<TemplateNode>, Failure> {
    let component = components.get(name).ok_or_else(|| {
        native_failure(
            1602,
            &node.source_path,
            node.range.start,
            node.range.end,
            "Native component is unresolved.",
        )
    })?;
    if children.iter().any(|child| !is_trivia(child))
        && !native_contains_slot(&component.program().nodes)
    {
        return Err(native_failure(
            1602,
            &node.source_path,
            node.range.start,
            node.range.end,
            &format!("Component '<{name}>' received children but declares no <slot>."),
        ));
    }
    let component_scope = resolve_native_properties(node, properties, scope)?;
    let component_slot = NativeSlot {
        nodes: children.to_vec(),
        scope: scope.cloned(),
    };
    expand_native_components(
        &component.program().nodes,
        components,
        Some(&component_scope),
        Some(&component_slot),
    )
}

fn resolve_native_text(
    node: &TemplateNode,
    parts: &[TextPart],
    scope: &BTreeMap<String, String>,
) -> Result<Vec<TextPart>, Failure> {
    parts
        .iter()
        .map(|part| match part {
            TextPart::Literal(_, _) => Ok(part.clone()),
            TextPart::Interpolation(expression, range) => scope
                .get(expression.source())
                .cloned()
                .map(|value| TextPart::Literal(value, *range))
                .ok_or_else(|| {
                    native_failure(
                        1602,
                        &node.source_path,
                        range.start,
                        range.end,
                        "Native component text requires a statically known property.",
                    )
                }),
        })
        .collect()
}

fn resolve_native_properties(
    node: &TemplateNode,
    properties: &BTreeMap<String, crate::template::TemplateAttribute>,
    outer_scope: Option<&BTreeMap<String, String>>,
) -> Result<BTreeMap<String, String>, Failure> {
    properties
        .iter()
        .map(|(name, attribute)| match &attribute.value {
            AttributeValue::Static(value) => Ok((name.clone(), value.clone())),
            AttributeValue::Dynamic(_) => outer_scope
                .ok_or_else(|| {
                    native_failure(
                        1602,
                        &node.source_path,
                        attribute.range.start,
                        attribute.range.end,
                        "Native component property requires a statically known value.",
                    )
                })
                .and_then(|scope| resolve_native_attribute(node, attribute, scope))
                .map(|value| (name.clone(), value)),
            AttributeValue::Control(_) | AttributeValue::Event(_) => Err(native_failure(
                1602,
                &node.source_path,
                attribute.range.start,
                attribute.range.end,
                "Native component property cannot contain control or event syntax.",
            )),
        })
        .collect()
}

fn resolve_native_attributes(
    node: &TemplateNode,
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
    scope: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, crate::template::TemplateAttribute>, Failure> {
    attributes
        .iter()
        .map(|(name, attribute)| {
            if matches!(attribute.value, AttributeValue::Dynamic(_)) {
                let mut resolved = attribute.clone();
                resolved.value =
                    AttributeValue::Static(resolve_native_attribute(node, attribute, scope)?);
                Ok((name.clone(), resolved))
            } else {
                Ok((name.clone(), attribute.clone()))
            }
        })
        .collect()
}

fn resolve_native_attribute(
    node: &TemplateNode,
    attribute: &crate::template::TemplateAttribute,
    scope: &BTreeMap<String, String>,
) -> Result<String, Failure> {
    match &attribute.value {
        AttributeValue::Static(value) => Ok(value.clone()),
        AttributeValue::Dynamic(expression) => {
            scope.get(expression.source()).cloned().ok_or_else(|| {
                native_failure(
                    1602,
                    &node.source_path,
                    attribute.range.start,
                    attribute.range.end,
                    "Native component property requires a statically known value.",
                )
            })
        }
        AttributeValue::Control(_) | AttributeValue::Event(_) => Err(native_failure(
            1602,
            &node.source_path,
            attribute.range.start,
            attribute.range.end,
            "Native component property cannot contain control or event syntax.",
        )),
    }
}

fn native_contains_slot(nodes: &[TemplateNode]) -> bool {
    nodes.iter().any(|node| {
        matches!(node.kind, TemplateNodeKind::Slot)
            || node.kind.children().is_some_and(native_contains_slot)
    })
}

/// Removes compiler-generated browser assets before native planning.
///
/// A native host has no browser navigation, no service worker, and no module
/// loader, so generated references are noise to it.
///
/// **Every** script is generated, and that is not an assumption: the view
/// contract rejects an authored `<script>` with `TY1306` before rendering, so
/// one can only be here because this compiler put it here. Matching a
/// generated path instead missed the route's own client module, which is
/// emitted as `/client.js` rather than under `/.tachyon/` — so a page with a
/// companion or an event binding, which is to say a real page, failed its
/// native build on the compiler's own output.
fn strip_generated_assets(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let (before, tail) = rest.split_at(start);
        output.push_str(before);
        let is_generated_script = tail.starts_with("<script");
        let is_generated_link = tail.starts_with("<link")
            && tail
                .find('>')
                .is_some_and(|end| tail[..end].contains("/.tachyon/"));
        // A script element always has a closing tag; a link never does.
        if is_generated_script && let Some(end) = tail.find("</script>") {
            rest = &tail[end + "</script>".len()..];
            continue;
        }
        if is_generated_link && let Some(end) = tail.find('>') {
            rest = &tail[end + 1..];
            continue;
        }
        output.push('<');
        rest = &tail[1..];
    }
    output.push_str(rest);
    output.replace(GENERATED_CONTROLLER_SCRIPT, "")
}

fn lower_page_island(html: &str) -> (String, BTreeMap<String, String>) {
    let Some(marker) = html.find("data-tachyon-page=\"true\"") else {
        return (String::from(html), BTreeMap::new());
    };
    let Some(open_start) = html[..marker].rfind("<tachyon-island") else {
        return (String::from(html), BTreeMap::new());
    };
    let Some(open_end_relative) = html[marker..].find('>') else {
        return (String::from(html), BTreeMap::new());
    };
    let open_end = marker + open_end_relative + 1;
    let opening = &html[open_start..open_end];
    let props = attribute_value(opening, "data-tachyon-props")
        .map(|value| decode_html(&value))
        .and_then(|value| serde_json::from_str::<BTreeMap<String, Value>>(&value).ok())
        .unwrap_or_default();
    let mut state = BTreeMap::new();
    for (name, value) in &props {
        let value = match value {
            Value::Null => String::new(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            Value::String(value) => value.clone(),
            Value::Array(_) | Value::Object(_) => continue,
        };
        state.insert(name.clone(), value);
    }
    let Some(close_start) = html[open_end..].rfind("</tachyon-island>") else {
        return (String::from(html), BTreeMap::new());
    };
    let close_start = open_end + close_start;
    let close_end = close_start + "</tachyon-island>".len();
    let mut unwrapped = String::with_capacity(html.len());
    unwrapped.push_str(&html[..open_start]);
    unwrapped.push_str(&html[open_end..close_start]);
    unwrapped.push_str(&html[close_end..]);
    let lowered = lower_page_expressions(&unwrapped, &props);
    (lower_page_actions(&lowered), state)
}

fn lower_page_expressions(html: &str, state: &BTreeMap<String, Value>) -> String {
    let mut output = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find("<tachyon-expr") {
        let start = cursor + relative;
        let Some(open_end_relative) = html[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_relative + 1;
        let close = "</tachyon-expr>";
        let Some(close_relative) = html[open_end..].find(close) else {
            break;
        };
        let end = open_end + close_relative + close.len();
        let opening = &html[start..open_end];
        let expression = attribute_value(opening, "data-tachyon-expression")
            .map(|value| decode_html(&value))
            .and_then(|value| serde_json::from_str::<Value>(&value).ok());
        let value = expression
            .as_ref()
            .and_then(|expression| evaluate_page_expression(expression, state))
            .unwrap_or_default();
        let identifier = expression
            .as_ref()
            .and_then(|expression| expression.as_object())
            .filter(|expression| expression.get("k").and_then(Value::as_str) == Some("id"))
            .and_then(|expression| expression.get("n").and_then(Value::as_str));
        let parent_start = html[..start].rfind("<p");
        let lowered_parent = parent_start.and_then(|parent_start| {
            let parent_end = html[parent_start..start].find('>')? + parent_start + 1;
            let close_end = html[end..].find("</p>")? + end;
            (close_end == end).then_some((parent_start, parent_end))
        });
        if let (Some(identifier), Some((parent_start, parent_end))) = (identifier, lowered_parent) {
            output.push_str(&html[cursor..parent_start]);
            let prefix = &html[parent_end..start];
            output.push_str("<output aria-label=\"");
            output.push_str(&escape_attribute(identifier));
            output.push_str("\" data-tachyon-bind=\"");
            output.push_str(&escape_attribute(identifier));
            output.push_str("\" data-tachyon-state=\"");
            output.push_str(&escape_attribute(&value));
            output.push_str("\" data-tachyon-prefix=\"");
            output.push_str(&escape_attribute(prefix));
            output.push_str("\">");
            output.push_str(&escape_text(prefix));
            output.push_str(&escape_text(&value));
            output.push_str("</output>");
            cursor = end + "</p>".len();
        } else {
            output.push_str(&html[cursor..start]);
            output.push_str(&escape_text(&value));
            cursor = end;
        }
    }
    output.push_str(&html[cursor..]);
    output
}

fn evaluate_page_expression(expression: &Value, state: &BTreeMap<String, Value>) -> Option<String> {
    let object = expression.as_object()?;
    match object.get("k")?.as_str()? {
        "id" => display_page_value(state.get(object.get("n")?.as_str()?)?),
        "lit" => display_page_value(object.get("v")?),
        "call" => {
            let callee = object.get("c")?.as_object()?;
            if callee.get("k")?.as_str()? != "get" || callee.get("p")?.as_str()? != "join" {
                return None;
            }
            let owner = callee.get("o")?.as_object()?;
            if owner.get("k")?.as_str()? != "id" {
                return None;
            }
            let values = state.get(owner.get("n")?.as_str()?)?.as_array()?;
            let separator = object
                .get("a")?
                .as_array()?
                .first()
                .and_then(|value| evaluate_page_expression(value, state))
                .unwrap_or_else(|| String::from(","));
            Some(
                values
                    .iter()
                    .map(display_page_value)
                    .collect::<Option<Vec<_>>>()?
                    .join(&separator),
            )
        }
        _ => None,
    }
}

fn display_page_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some(String::new()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        Value::String(value) => Some(value.clone()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn lower_page_actions(html: &str) -> String {
    let marker = "data-tac-on-click=\"";
    let mut output = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find(marker) {
        let start = cursor + relative;
        let value_start = start + marker.len();
        let Some(end_relative) = html[value_start..].find('"') else {
            break;
        };
        let end = value_start + end_relative + 1;
        let binding = decode_html(&html[value_start..end - 1]);
        let action = serde_json::from_str::<Value>(&binding)
            .ok()
            .and_then(|binding| {
                let binding = binding.as_object()?;
                let name = binding.get("s")?.as_str()?;
                let operator = binding.get("op")?.as_str()?;
                let argument = binding.get("a")?.as_array()?.first()?.as_object()?;
                (operator == "+=" && argument.get("v").and_then(Value::as_i64) == Some(1))
                    .then(|| format!("increment:{name}"))
            });
        output.push_str(&html[cursor..start]);
        if let Some(action) = action {
            output.push_str("data-tachyon-action=\"");
            output.push_str(&escape_attribute(&action));
            output.push('"');
        } else {
            output.push_str(&html[start..end]);
        }
        cursor = end;
    }
    output.push_str(&html[cursor..]);
    output
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    Some(String::from(&tag[start..end]))
}

fn find_body(nodes: &[TemplateNode]) -> Option<&TemplateNode> {
    for node in nodes {
        if element_tag(node).is_some_and(|tag| tag == "body") {
            return Some(node);
        }
        if let TemplateNodeKind::Element { children, .. } = &node.kind
            && let Some(body) = find_body(children)
        {
            return Some(body);
        }
    }
    None
}

fn element_tag(node: &TemplateNode) -> Option<&str> {
    match &node.kind {
        TemplateNodeKind::Element { tag, .. } => Some(tag),
        _ => None,
    }
}

fn adapter_for(
    tag: &str,
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
) -> Option<&'static str> {
    match tag {
        "html" | "body" | "main" | "section" | "article" | "div" | "header" | "footer" | "nav"
        | "form" => Some("layout.column"),
        "ul" | "ol" => Some("layout.list"),
        "li" => Some("layout.list_item"),
        "h1" => Some("text.heading1"),
        "h2" => Some("text.heading2"),
        "h3" => Some("text.heading3"),
        "h4" => Some("text.heading4"),
        "h5" => Some("text.heading5"),
        "h6" => Some("text.heading6"),
        "p" | "span" | "label" | "strong" | "em" | "small" | "code" | "pre" | "summary" => {
            Some("content.text")
        }
        "button" => Some("control.button"),
        "input" if supported_input(attributes) => Some("control.text_field"),
        "textarea" => Some("control.text_field"),
        "output" => Some("content.output"),
        "details" => Some("control.disclosure"),
        "a" if static_attribute(attributes, "href").is_some_and(|href| href.starts_with('/')) => {
            Some("navigation.link")
        }
        "img" => Some("content.image"),
        "hr" | "br" => Some("content.divider"),
        // A tag this compiler has never heard of still says what it is, if its
        // author declared a role. ARIA is the platform's own vocabulary for
        // exactly the question an adapter asks, so a design system that names
        // its roles — which it owes its users anyway — reaches native widgets
        // without this compiler knowing anything about it.
        _ => declared_role(attributes).and_then(|role| {
            if role == "heading" {
                return Some(heading_adapter(aria_level(attributes)));
            }
            adapter_for_role(&role)
        }),
    }
}

/// Maps an explicitly declared ARIA role onto a native adapter.
///
/// Only roles with an unambiguous native counterpart are mapped. A role this
/// does not know falls through to a `WebSurface`, which is what an unknown
/// element got before.
fn adapter_for_role(role: &str) -> Option<&'static str> {
    match role {
        "button" => Some("control.button"),
        "banner" => Some("layout.app_bar"),
        "textbox" | "searchbox" => Some("control.text_field"),
        // A heading without a level is a level-2 heading, which is what the
        // ARIA specification says a heading defaults to.
        "heading" => Some("text.heading2"),
        "list" => Some("layout.list"),
        "listitem" => Some("layout.list_item"),
        "img" => Some("content.image"),
        "status" => Some("content.output"),
        "separator" => Some("content.divider"),
        "group" | "region" | "main" | "navigation" | "contentinfo" | "form" | "article"
        | "list_box" | "none" | "presentation" => Some("layout.column"),
        _ => None,
    }
}

/// Reads the accessible name a component usage passed to its island.
///
/// An attribute written on a component becomes a property of its companion
/// rather than an attribute of the rendered island, so an island whose content
/// is entirely deferred had no name to offer and could never be planned. The
/// name the author wrote is still there, in the props.
fn island_label(
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
) -> Option<String> {
    let props = static_attribute(attributes, "data-tachyon-props")?;
    let parsed: Value = serde_json::from_str(&props).ok()?;
    ["aria-label", "label", "title"]
        .into_iter()
        .find_map(|key| parsed.get(key).and_then(Value::as_str).map(String::from))
        .filter(|value| !value.trim().is_empty())
}

/// Names the adapter for one heading level.
const fn heading_adapter(level: u8) -> &'static str {
    match level {
        1 => "text.heading1",
        3 => "text.heading3",
        4 => "text.heading4",
        5 => "text.heading5",
        6 => "text.heading6",
        _ => "text.heading2",
    }
}

/// Reads `aria-level`, defaulting to the level ARIA gives a bare heading.
fn aria_level(attributes: &BTreeMap<String, crate::template::TemplateAttribute>) -> u8 {
    static_attribute(attributes, "aria-level")
        .and_then(|value| value.trim().parse::<u8>().ok())
        .filter(|level| (1..=6).contains(level))
        .unwrap_or(2)
}

/// The role an element declares, if it declares one.
fn declared_role(
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
) -> Option<String> {
    static_attribute(attributes, "role").filter(|value| !value.trim().is_empty())
}

fn supported_input(attributes: &BTreeMap<String, crate::template::TemplateAttribute>) -> bool {
    static_attribute(attributes, "type").is_none_or(|value| {
        matches!(
            value.as_str(),
            "text" | "email" | "password" | "search" | "number"
        )
    })
}

fn lower_properties(
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
) -> BTreeMap<String, Value> {
    attributes
        .iter()
        .filter_map(|(name, attribute)| {
            if (name.starts_with("aria-") && name != "aria-hidden")
                || matches!(
                    name.as_str(),
                    "data-tachyon-action" | "data-tachyon-bind" | "data-tachyon-state"
                )
            {
                return None;
            }
            let AttributeValue::Static(value) = &attribute.value else {
                return None;
            };
            Some((name.clone(), Value::String(decode_html(value))))
        })
        .collect()
}

fn static_attribute(
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
    name: &str,
) -> Option<String> {
    attributes.get(name).and_then(|attribute| {
        if let AttributeValue::Static(value) = &attribute.value {
            Some(decode_html(value))
        } else {
            None
        }
    })
}

fn accessibility(
    tag: &str,
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
    text: &str,
) -> Option<NativeAccessibility> {
    if static_attribute(attributes, "aria-hidden").as_deref() == Some("true") {
        return None;
    }
    let role =
        static_attribute(attributes, "role").or_else(|| semantic_role(tag).map(String::from));
    let named_by_content = matches!(
        tag,
        "button" | "a" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "details"
    ) || role
        .as_deref()
        .is_some_and(|value| matches!(value, "button" | "heading" | "link"));
    let label = accessible_label(attributes).or_else(|| {
        named_by_content
            .then(|| String::from(text.trim()))
            .filter(|value| !value.is_empty())
    });
    if role.is_none() && label.is_none() {
        None
    } else {
        Some(NativeAccessibility { role, label })
    }
}

fn accessible_label(
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
) -> Option<String> {
    static_attribute(attributes, "aria-label")
        .or_else(|| static_attribute(attributes, "alt"))
        .filter(|value| !value.trim().is_empty())
}

fn semantic_role(tag: &str) -> Option<&'static str> {
    match tag {
        "main" => Some("main"),
        "nav" => Some("navigation"),
        "header" => Some("banner"),
        "footer" => Some("contentinfo"),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some("heading"),
        "button" => Some("button"),
        "input" | "textarea" => Some("textbox"),
        "output" => Some("status"),
        "a" => Some("link"),
        "img" => Some("image"),
        "details" => Some("group"),
        "ul" | "ol" => Some("list"),
        "li" => Some("listitem"),
        _ => None,
    }
}

fn validate_accessibility(
    tag: &str,
    attributes: &BTreeMap<String, crate::template::TemplateAttribute>,
    accessibility: Option<&NativeAccessibility>,
) -> Result<(), &'static str> {
    let interactive = matches!(
        tag,
        "button" | "input" | "textarea" | "a" | "details" | "img"
    ) || declared_role(attributes).is_some_and(|role| {
        matches!(
            role.as_str(),
            "button" | "textbox" | "searchbox" | "link" | "img"
        )
    });
    if !interactive || static_attribute(attributes, "aria-hidden").as_deref() == Some("true") {
        return Ok(());
    }
    if accessibility
        .and_then(|value| value.label.as_deref())
        .is_none_or(|label| label.trim().is_empty())
    {
        Err("Interactive native element requires an accessible name.")
    } else {
        Ok(())
    }
}

fn visible_text(nodes: &[TemplateNode]) -> String {
    nodes.iter().map(visible_node_text).collect::<String>()
}

fn visible_node_text(node: &TemplateNode) -> String {
    match &node.kind {
        TemplateNodeKind::Text(parts) => decode_html(&text_parts(parts)),
        TemplateNodeKind::Element { children, .. } => visible_text(children),
        _ => String::new(),
    }
}

fn text_parts(parts: &[TextPart]) -> String {
    parts
        .iter()
        .map(|part| match part {
            TextPart::Literal(value, _) => value.clone(),
            TextPart::Interpolation(expression, _) => format!("{{{}}}", expression.source()),
        })
        .collect()
}

fn valid_state_name(value: &str) -> bool {
    value.len() <= 64
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn valid_https_url(value: &str) -> bool {
    if value.len() > 2_048
        || !value.starts_with("https://")
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace() || byte == b'\\')
    {
        return false;
    }
    let authority = value[8..].split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, None), |(host, port)| (host, Some(port)));
    if port
        .is_some_and(|value| value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Wraps one fallback fragment in a document that looks like the page it came
/// from.
///
/// The project's own stylesheets are inlined rather than linked: they are
/// stripped from the fragment as generated assets. Inline styles are already
/// permitted by the private resource document policy below. Without this a surface renders as unstyled
/// default HTML inside an otherwise native window.
fn fallback_document(fragment: &str, styles: &str) -> String {
    let fragment = native_asset_references(fragment);
    let styles = native_style_references(styles);
    let controller_runtime = fragment.contains("data-tachyon-action=").then_some(
        r#"<script type="module" src="../../WebBundle/tachyon-runtime/native-controller.js"></script>"#,
    );
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self'; script-src 'self'; connect-src 'self';\"><style>:root{{color-scheme:light dark;font-family:-apple-system,BlinkMacSystemFont,sans-serif}}html,body{{margin:0;background:transparent;color:CanvasText}}::-webkit-scrollbar{{display:none}}</style><style>{styles}</style><style>html,body{{min-height:0!important}}</style></head><body>{fragment}<script type=\"module\" src=\"../../WebBundle/tachyon-runtime/native-surface.js\"></script>{}</body></html>",
        controller_runtime.unwrap_or_default(),
    )
}

/// Repoints root-relative generated assets from an isolated surface to the
/// sibling native `WebBundle` while leaving navigation links untouched.
fn native_asset_references(fragment: &str) -> String {
    let mut output = fragment
        .replace(
            "data-tachyon-module=\"/.tachyon/",
            "data-tachyon-module=\"../../WebBundle/tachyon-runtime/",
        )
        .replace(
            "data-tachyon-module='/.tachyon/",
            "data-tachyon-module='../../WebBundle/tachyon-runtime/",
        )
        .replace(
            "data-tachyon-wasm=\"/.tachyon/",
            "data-tachyon-wasm=\"../../WebBundle/tachyon-runtime/",
        )
        .replace(
            "data-tachyon-wasm='/.tachyon/",
            "data-tachyon-wasm='../../WebBundle/tachyon-runtime/",
        );
    for attribute in ["src", "poster", "data-tachyon-module", "data-tachyon-wasm"] {
        for quote in ['\'', '"'] {
            output = output.replace(
                &format!("{attribute}={quote}/"),
                &format!("{attribute}={quote}../../WebBundle/"),
            );
        }
    }
    output
}

fn native_style_references(styles: &str) -> String {
    styles
        .replace("url('/", "url('../../WebBundle/")
        .replace("url(\"/", "url(\"../../WebBundle/")
        .replace("url(/", "url(../../WebBundle/")
}

fn decode_html(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(index) = remaining.find('&') {
        decoded.push_str(&remaining[..index]);
        let entity = &remaining[index..];
        let Some(end) = entity.find(';') else {
            decoded.push_str(entity);
            return decoded;
        };
        let token = &entity[1..end];
        let replacement = match token {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "#39" | "apos" => Some('\''),
            _ if token.starts_with("#x") => u32::from_str_radix(&token[2..], 16)
                .ok()
                .and_then(char::from_u32),
            _ if token.starts_with('#') => token[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        if let Some(character) = replacement {
            decoded.push(character);
        } else {
            decoded.push_str(&entity[..=end]);
        }
        remaining = &entity[end + 1..];
    }
    decoded.push_str(remaining);
    decoded
}

/// Returns the human-readable platform name used in fallback diagnostics.
fn target_label(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::Linux => "Linux",
        NativeTarget::Macos => "macOS",
        NativeTarget::Windows => "Windows",
        NativeTarget::Android => "Android",
        NativeTarget::Ios => "iOS",
    }
}

fn route_key(route: &str) -> String {
    if route == "/" {
        String::from("root")
    } else {
        route.trim_matches('/').replace(['/', ':'], "_")
    }
}

fn native_failure(
    number: u16,
    source_path: &str,
    start: usize,
    end: usize,
    message: &str,
) -> Failure {
    Failure::one(diagnostic(
        number,
        message,
        Some(String::from(
            "Use supported semantic HTML or an explicitly isolated WebSurface subtree.",
        )),
        source_span(source_path, start, end),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{MAX_STATE_ENTRIES, MAX_STATE_VALUE_BYTES, NativePlanner, valid_https_url};
    use crate::template::ComponentRegistry;
    use std::collections::BTreeMap;
    use std::fmt::Write;
    use std::fs;
    use tachyon_contracts::NativeTarget;

    #[test]
    fn supported_siblings_state_accessibility_and_fallback_plan_together() {
        let plan = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<!doctype html><html><head><title>Ignored</title></head><body><main aria-label="Demo"><h1>Catalog &amp; News</h1><button aria-label="Increase" data-tachyon-action="increment:count">Increase</button><output data-tachyon-bind="count" data-tachyon-state="0">0</output><fancy-chart aria-label="Chart"><p>Fallback</p></fancy-chart><footer>After</footer></main></body></html>"#,
            "",
        )
        .expect("plan");
        let json = serde_json::to_string(&plan.native_ui).expect("native JSON");
        assert!(json.contains("layout.column"));
        assert!(json.contains("Catalog & News"));
        assert!(json.contains("control.button"));
        assert!(json.contains("web_surface"));
        assert!(json.contains("After"));
        assert_eq!(plan.initial_state["count"], "0");
        assert_eq!(plan.web_surfaces.len(), 1);
        assert!(plan.web_surfaces[0].document.contains("Fallback"));
    }

    #[test]
    fn native_component_expansion_preserves_props_slots_repetition_and_multiple_roots() {
        let project = tempfile::tempdir().expect("project");
        let component = project.path().join("client/components/product/card");
        fs::create_dir_all(&component).expect("component directory");
        fs::write(
            component.join("tac.html"),
            r#"<h2>{title}</h2><section :aria-label="title"><slot></slot></section>"#,
        )
        .expect("component template");
        let components = ComponentRegistry::discover(project.path()).expect("registry");
        let plan = NativePlanner::plan_with_components_and_state(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main><product-card title="Alpha"><button aria-label="First">First</button></product-card><product-card title="Beta"><p>Second</p></product-card></main>"#,
            "",
            &components,
            BTreeMap::new(),
        )
        .expect("expanded native plan");
        let json = serde_json::to_string(&plan.native_ui).expect("native JSON");
        for expected in ["Alpha", "Beta", "First", "Second"] {
            assert!(json.contains(expected), "missing {expected}: {json}");
        }
        assert_eq!(json.matches("text.heading2").count(), 2, "{json}");
        // The page and both expanded component sections each lower to a
        // column; the component's two roots remain siblings under the page.
        assert_eq!(json.matches("layout.column").count(), 3, "{json}");
        assert_eq!(plan.web_surface_count, 0, "{json}");
    }

    #[test]
    fn native_component_expansion_preserves_root_page_expressions() {
        let project = tempfile::tempdir().expect("project");
        let component = project.path().join("client/components/product/card");
        fs::create_dir_all(&component).expect("component directory");
        fs::write(component.join("tac.html"), "<h2>{title}</h2>").expect("component template");
        let components = ComponentRegistry::discover(project.path()).expect("registry");
        let plan = NativePlanner::plan_with_components_and_state(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main><button aria-label="Add" on:click="count += 1">Add</button><p>Count: {count}</p><p>{required.join('|')}</p><product-card title="Static"></product-card></main>"#,
            "",
            &components,
            BTreeMap::from([(String::from("count"), String::from("0"))]),
        )
        .expect("root expression must survive component expansion");
        let json = serde_json::to_string(&plan.native_ui).expect("native JSON");
        assert!(json.contains("{required.join('|')}"), "{json}");
        assert!(json.contains("Static"), "{json}");
        assert!(json.contains("increment:count"), "{json}");
    }

    #[test]
    fn seeded_native_state_enforces_names_values_and_entry_limits() {
        let project = tempfile::tempdir().expect("project");
        let components = ComponentRegistry::discover(project.path()).expect("registry");
        let invalid_states = [
            BTreeMap::from([(String::from("bad-name"), String::from("0"))]),
            BTreeMap::from([(String::from("large"), "x".repeat(MAX_STATE_VALUE_BYTES + 1))]),
            (0..=MAX_STATE_ENTRIES)
                .map(|index| (format!("field_{index}"), String::from("0")))
                .collect(),
        ];
        for state in invalid_states {
            let error = NativePlanner::plan_with_components_and_state(
                NativeTarget::Macos,
                "/",
                "client/pages/tac.html",
                "<main>Bounded</main>",
                "",
                &components,
                state,
            )
            .expect_err("invalid seeded state must fail closed");
            assert!(error.to_string().contains("TY1603"), "{error}");
        }
    }

    #[test]
    fn unresolved_native_component_values_fail_closed() {
        let project = tempfile::tempdir().expect("project");
        let component = project.path().join("client/components/product/card");
        fs::create_dir_all(&component).expect("component directory");
        fs::write(component.join("tac.html"), "<p>{title}</p>").expect("component template");
        let components = ComponentRegistry::discover(project.path()).expect("registry");
        let error = NativePlanner::plan_with_components_and_state(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main><product-card :title="missing"></product-card></main>"#,
            "",
            &components,
            BTreeMap::new(),
        )
        .expect_err("dynamic property must not disappear");
        assert!(error.to_string().contains("TY1602"), "{error}");
    }

    #[test]
    fn invalid_state_accessibility_and_remote_content_fail_closed() {
        for (source, code) in [
            (
                r#"<main><button data-tachyon-action="increment:missing">Go</button></main>"#,
                "TY1603",
            ),
            (r"<main><input></main>", "TY1603"),
            (
                r#"<main><iframe aria-label="Remote" src="http://example.test"></iframe></main>"#,
                "TY1604",
            ),
            (r"<main><unknown-widget></unknown-widget></main>", "TY1603"),
        ] {
            let error = NativePlanner::plan(
                NativeTarget::Macos,
                "/",
                "client/pages/tac.html",
                source,
                "",
            )
            .expect_err("invalid native view");
            assert!(error.to_string().contains(code), "{}", error);
        }
    }

    #[test]
    fn rendered_input_diagnostics_keep_the_resolved_source_identity() {
        let error = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "resolved/index.html",
            "<main>{missing</main>",
            "",
        )
        .expect_err("malformed rendered interpolation must fail");
        assert_eq!(error.diagnostics()[0].spans[0].file, "resolved/index.html");
    }

    #[test]
    fn remote_surface_has_no_bridge_and_generated_scripts_are_removed() {
        let plan = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main aria-label="Remote"><iframe aria-label="Report" src="https://example.test/report"></iframe></main><script type="module" src="/.tachyon/tac-client.js"></script>"#,
            "",
        )
        .expect("plan");
        let json = serde_json::to_string(&plan.native_ui).expect("native JSON");
        assert!(json.contains(r#""source":"remote_url""#));
        assert!(json.contains(r#""bridge":"none""#));
        assert_eq!(plan.web_surface_count, 1);
    }

    #[test]
    fn state_machine_rejects_duplicate_invalid_and_non_numeric_declarations() {
        for source in [
            r#"<main><output data-tachyon-bind="count" data-tachyon-state="0">0</output><output data-tachyon-bind="count" data-tachyon-state="1">1</output></main>"#,
            r#"<main><output data-tachyon-bind="count" data-tachyon-state="no">no</output><button aria-label="Add" data-tachyon-action="increment:count">Add</button></main>"#,
            r#"<main><output data-tachyon-bind="count" data-tachyon-state="0">0</output><button aria-label="Add" data-tachyon-action="delete:count">Add</button></main>"#,
            r#"<main><output data-tachyon-bind="bad-name" data-tachyon-state="0">0</output></main>"#,
        ] {
            let error = NativePlanner::plan(
                NativeTarget::Macos,
                "/",
                "client/pages/tac.html",
                source,
                "",
            )
            .expect_err("state must fail closed");
            assert!(error.to_string().contains("TY1603"), "{error}");
        }
    }

    #[test]
    fn surface_and_depth_budgets_are_enforced() {
        let mut surfaces = String::from("<main>");
        for index in 0..=1_024 {
            write!(
                &mut surfaces,
                r#"<x-surface aria-label="Surface {index}">x</x-surface>"#
            )
            .expect("write surface");
        }
        surfaces.push_str("</main>");
        let surface_error = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            &surfaces,
            "",
        )
        .expect_err("surface limit");
        assert!(surface_error.to_string().contains("TY1604"));

        let mut depth = String::new();
        for _ in 0..66 {
            depth.push_str("<div>");
        }
        depth.push('x');
        for _ in 0..66 {
            depth.push_str("</div>");
        }
        let depth_error = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            &depth,
            "",
        )
        .expect_err("depth limit");
        assert!(
            depth_error.to_string().contains("depth")
                || depth_error.to_string().contains("nesting"),
            "{depth_error}"
        );
    }

    #[test]
    fn remote_urls_are_strict_and_hidden_nodes_reach_the_platform_adapter() {
        for value in [
            "http://example.test",
            "https://",
            "https://user@example.test",
            "https://bad host.test",
            "https://example.test:bad/path",
            "https://-bad.example/path",
            "https://example.test\\escape",
        ] {
            assert!(!valid_https_url(value), "{value}");
        }
        assert!(valid_https_url(
            "https://reports.example.test:443/path?q=bounded"
        ));

        let plan = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main><section aria-hidden="true"><p>Hidden</p></section></main>"#,
            "",
        )
        .expect("hidden plan");
        let json = serde_json::to_string(&plan.native_ui).expect("native JSON");
        assert!(json.contains(r#""aria-hidden":"true""#));
    }

    /// Two routes plan their nodes independently, so both reach `n_000004`.
    /// That is correct inside a document and fatal as a file name: the payloads
    /// are written to disk under it, and the second route built replaced the
    /// first one's page with its own.
    #[test]
    fn two_routes_never_write_one_web_surface_over_another() {
        let view = r#"<main><x-chart aria-label="Chart"><p>Fallback</p></x-chart></main>"#;
        let home = NativePlanner::plan(NativeTarget::Macos, "/", "client/pages/tac.html", view, "")
            .expect("home plan");
        let docs = NativePlanner::plan(
            NativeTarget::Macos,
            "/docs",
            "client/pages/docs/tac.html",
            view,
            "",
        )
        .expect("docs plan");

        assert_ne!(home.web_surfaces[0].id, docs.web_surfaces[0].id);
        let located = |plan: &super::PlannedNativeRoute| {
            serde_json::to_string(&plan.native_ui).expect("native JSON")
        };
        assert!(located(&home).contains("WebSurfaces/root_n_000002/index.html"));
        assert!(located(&docs).contains("WebSurfaces/docs_n_000002/index.html"));
        // The node id itself stays what the Native UI contract specifies.
        assert!(located(&home).contains(r#""id":"n_000002""#));
    }

    /// A design system's own tags mean nothing to this compiler, but the roles
    /// it declares for accessibility mean the same thing a native adapter asks
    /// about, so declaring them is enough to reach native widgets.
    #[test]
    fn a_declared_role_reaches_a_native_adapter_without_a_known_tag() {
        let plan = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main><w-card role="group"><w-title role="heading" aria-level="1">Title</w-title><w-btn role="button">Press</w-btn></w-card></main>"#,
            "",
        )
        .expect("role plan");
        let json = serde_json::to_string(&plan.native_ui).expect("native JSON");
        assert!(json.contains("text.heading1"), "{json}");
        assert!(json.contains("control.button"), "{json}");
        assert_eq!(plan.web_surface_count, 0, "{json}");
    }

    /// A fallback subtree that renders unstyled looks nothing like the page it
    /// was cut from, and the generated stylesheet link is stripped before it
    /// can help, so the styles travel with the document.
    #[test]
    fn a_fallback_document_carries_the_route_stylesheets() {
        let plan = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main><x-chart aria-label="Chart"><p>Fallback</p></x-chart></main>"#,
            "body{color:rebeccapurple}",
        )
        .expect("styled plan");
        assert!(
            plan.web_surfaces[0]
                .document
                .contains("body{color:rebeccapurple}")
        );
    }

    #[test]
    fn fallback_document_repoints_assets_without_installing_removed_island_runtime() {
        let plan = NativePlanner::plan(
            NativeTarget::Android,
            "/",
            "client/pages/tac.html",
            r#"<main><x-shell><tachyon-island data-tachyon-module="/.tachyon/components/demo.js"><img src="/shared/logo.svg"><a href="/docs">Docs</a></tachyon-island></x-shell></main>"#,
            ".logo{background:url('/shared/logo.svg')}",
        )
        .expect("fallback plan");
        let document = &plan.web_surfaces[0].document;
        assert!(
            document.contains("src=\"../../WebBundle/shared/logo.svg\""),
            "{document}"
        );
        assert!(
            document.contains("url('../../WebBundle/shared/logo.svg')"),
            "{document}"
        );
        assert!(!document.contains("islands.js"));
        assert!(document.contains("href=\"/docs\""));
    }

    #[test]
    fn fallback_document_has_a_deny_by_default_policy_and_no_native_bridge() {
        let plan = NativePlanner::plan(
            NativeTarget::Macos,
            "/",
            "client/pages/tac.html",
            r#"<main><x-chart aria-label="Chart"><p>Contained fallback</p></x-chart></main>"#,
            "",
        )
        .expect("fallback plan");
        let document = &plan.web_surfaces[0].document;
        assert!(document.contains("default-src 'none'"));
        assert!(document.contains("connect-src 'self'"));
        assert!(document.contains("script-src 'self'"));
        assert!(!document.contains("script-src 'self' file:"));
        assert!(!document.contains("WKScriptMessageHandler"));
    }
}
