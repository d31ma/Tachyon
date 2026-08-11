use super::model::{ConditionalBranch, EventArgument, EventBinding};
use super::{
    AttributeValue, ComponentRegistry, HydrationPolicy, Scope, TemplateAttribute, TemplateNode,
    TemplateNodeKind, TemplateProgram, TextPart, is_trivia,
};
use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use tachyon_contracts::ViewSourceMap;

const MAX_CLIENT_PLAN_BYTES: usize = 10 * 1_024 * 1_024;
const MAX_CLIENT_PLAN_NODES: usize = 100_000;

/// The result of compiling one `tac.html` view for browser-owned rendering.
///
/// `html` is deliberately only a bootstrap document. The authored view is
/// represented by `data-tachyon-view`, and no Tac expression or structural
/// control is evaluated while producing it.
#[derive(Clone, Debug)]
pub(crate) struct ClientRenderedView {
    pub(crate) html: String,
    pub(crate) source_map: ViewSourceMap,
    pub(crate) components: BTreeSet<String>,
    pub(crate) page_bindings: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ClientViewRenderer<'a> {
    components: &'a ComponentRegistry,
}

impl<'a> ClientViewRenderer<'a> {
    pub(crate) const fn new(components: &'a ComponentRegistry) -> Self {
        Self { components }
    }

    pub(crate) fn render(
        self,
        program: &TemplateProgram,
        output_path: &str,
        module: Option<&str>,
        initial_state: &Scope,
    ) -> Result<ClientRenderedView, Failure> {
        let mut encoder = Encoder {
            components: self.components,
            used_components: BTreeSet::new(),
            component_owner_depth: 0,
            page_bindings: false,
            node_count: 0,
        };
        let plan = json!({
            "schemaVersion": 1,
            "document": program.is_document,
            "module": module,
            "state": initial_state,
            "nodes": encoder.nodes(&program.nodes)?,
        });
        let plan =
            serde_json::to_string(&plan).map_err(|error| serialization_failure(program, &error))?;
        if plan.len() > MAX_CLIENT_PLAN_BYTES {
            return Err(Failure::one(diagnostic(
                1305,
                "Tac client render plan exceeds the 10 MiB limit.",
                Some(String::from(
                    "Split the view into fewer or smaller components.",
                )),
                source_span(&program.source_path, 0, 0),
            )));
        }
        let plan = plan
            // JSON embedded in an HTML script-data block must not be able to
            // spell an end tag, even when authored text contains one.
            .replace('<', "\\u003c");
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\">\
             <script id=\"tachyon-view\" type=\"application/json\" data-tachyon-runtime>{plan}</script>\
             <script type=\"module\" src=\"/.tachyon/tac-client.js\" data-tachyon-runtime></script>\
             </head><body><noscript>Tac requires JavaScript to render this view.</noscript></body></html>"
        );
        Ok(ClientRenderedView {
            html,
            source_map: ViewSourceMap::v1(
                String::from(output_path),
                vec![program.source_path.clone()],
                Vec::new(),
            ),
            components: encoder.used_components,
            page_bindings: encoder.page_bindings,
        })
    }
}

struct Encoder<'a> {
    components: &'a ComponentRegistry,
    used_components: BTreeSet<String>,
    component_owner_depth: usize,
    page_bindings: bool,
    node_count: usize,
}

impl Encoder<'_> {
    fn nodes(&mut self, nodes: &[TemplateNode]) -> Result<Vec<Value>, Failure> {
        nodes.iter().map(|node| self.node(node)).collect()
    }

    fn node(&mut self, node: &TemplateNode) -> Result<Value, Failure> {
        self.node_count = self.node_count.saturating_add(1);
        if self.node_count > MAX_CLIENT_PLAN_NODES {
            return Err(client_failure(
                node,
                1305,
                "Tac client render plan exceeds 100,000 nodes.",
            ));
        }
        match &node.kind {
            TemplateNodeKind::Element {
                tag,
                attributes,
                children,
                void,
            } => Ok(json!({
                "k": "element",
                "tag": tag,
                "attributes": self.attributes(attributes)?,
                "children": self.nodes(children)?,
                "void": void,
            })),
            TemplateNodeKind::Text(parts) => Ok(json!({
                "k": "text",
                "parts": parts.iter().map(text_part).collect::<Result<Vec<_>, _>>()?,
            })),
            TemplateNodeKind::Comment(value) => Ok(json!({ "k": "comment", "value": value })),
            TemplateNodeKind::Conditional {
                condition,
                branch,
                children,
            } => Ok(json!({
                "k": "conditional",
                "branch": match branch {
                    ConditionalBranch::If => "if",
                    ConditionalBranch::ElseIf => "else-if",
                    ConditionalBranch::Else => "else",
                },
                "condition": condition.as_ref().map(expression_value).transpose()?,
                "children": self.nodes(children)?,
            })),
            TemplateNodeKind::Iteration {
                binding,
                iterable,
                children,
            } => Ok(json!({
                "k": "iteration",
                "binding": binding,
                "iterable": expression_value(iterable)?,
                "children": self.nodes(children)?,
            })),
            TemplateNodeKind::Switch { value, children } => Ok(json!({
                "k": "switch",
                "value": expression_value(value)?,
                "children": self.nodes(children)?,
            })),
            TemplateNodeKind::Case { when, children } => Ok(json!({
                "k": "case",
                "when": when.as_ref().map(expression_value).transpose()?,
                "children": self.nodes(children)?,
            })),
            TemplateNodeKind::Component {
                name,
                properties,
                hydrate,
                children,
            } => self.component(node, name, properties, *hydrate, children),
            TemplateNodeKind::Slot => Ok(json!({ "k": "slot" })),
        }
    }

    fn component(
        &mut self,
        node: &TemplateNode,
        name: &str,
        properties: &BTreeMap<String, TemplateAttribute>,
        declared_policy: Option<HydrationPolicy>,
        children: &[TemplateNode],
    ) -> Result<Value, Failure> {
        let component = self.components.get(name).ok_or_else(|| {
            client_failure(node, 1402, &format!("Unknown Tac component '<{name}>'."))
        })?;
        if children.iter().any(|child| !is_trivia(child))
            && !contains_slot(&component.program().nodes)
        {
            return Err(client_failure(
                node,
                1404,
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
            return Err(client_failure(
                node,
                1405,
                &format!("Client component '<{name}>' has no Tac companion."),
            ));
        }
        let active = policy.is_some_and(|value| value != HydrationPolicy::Never);
        if active {
            self.used_components.insert(String::from(name));
        }
        let (module, wasm) = if active {
            if let Some(source) = component.wasm_path() {
                (
                    None,
                    Some(format!(
                        "/.tachyon/components/{name}{}",
                        crate::wasm::asset_suffix(source)
                    )),
                )
            } else {
                (Some(format!("/.tachyon/components/{name}.js")), None)
            }
        } else {
            (None, None)
        };
        let encoded_properties = self.attributes(properties)?;
        self.component_owner_depth += usize::from(active);
        let template = self.nodes(&component.program().nodes);
        self.component_owner_depth -= usize::from(active);
        let template = template?;
        let slot = self.nodes(children)?;
        Ok(json!({
            "k": "component",
            "name": name,
            "properties": encoded_properties,
            // `hydrate=` remains accepted as a scheduling compatibility
            // spelling, but this is a client mount policy, never hydration.
            "mount": policy.map(HydrationPolicy::name),
            "module": module,
            "wasm": wasm,
            "scope": component.style_path().is_some(),
            "template": template,
            "slot": slot,
        }))
    }

    fn attributes(
        &mut self,
        attributes: &BTreeMap<String, TemplateAttribute>,
    ) -> Result<Vec<Value>, Failure> {
        attributes
            .iter()
            .map(|(name, attribute)| match &attribute.value {
                AttributeValue::Static(value) | AttributeValue::Control(value) => {
                    Ok(json!({ "name": name, "value": value }))
                }
                AttributeValue::Dynamic(expression) => Ok(json!({
                    "name": name,
                    "expression": expression_value(expression)?,
                })),
                AttributeValue::Event(binding) => {
                    let event = name
                        .strip_prefix("data-tac-on-")
                        .unwrap_or(name)
                        .replace("__", ":");
                    self.page_bindings |= self.component_owner_depth == 0;
                    Ok(json!({ "name": name, "eventType": event, "event": event_value(binding)? }))
                }
            })
            .collect()
    }
}

fn text_part(part: &TextPart) -> Result<Value, Failure> {
    match part {
        TextPart::Literal(value, _) => Ok(json!({ "value": value })),
        TextPart::Interpolation(expression, _) => {
            Ok(json!({ "expression": expression_value(expression)? }))
        }
    }
}

fn event_value(binding: &EventBinding) -> Result<Value, Failure> {
    let arguments = binding
        .arguments
        .iter()
        .map(|argument| match argument {
            EventArgument::Literal(value) => serde_json::from_str::<Value>(value)
                .map(|value| json!({ "value": value }))
                .map_err(|error| Failure::one(diagnostic(1306, error.to_string(), None, None))),
            EventArgument::EventPath(path) => Ok(json!({ "event": path })),
            EventArgument::Scope(expression) => {
                Ok(json!({ "expression": expression_value(expression)? }))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "handler": binding.handler,
        "arguments": arguments,
        "assign": binding.assign.as_ref().map(|assignment| json!({
            "target": assignment.target,
            "operator": assignment.operator,
        })),
    }))
}

fn expression_value(expression: &super::expression::Expression) -> Result<Value, Failure> {
    serde_json::from_str(&expression.to_client_json()).map_err(|error| {
        Failure::one(diagnostic(
            1305,
            format!("Cannot serialize a Tac client expression: {error}"),
            None,
            None,
        ))
    })
}

fn contains_slot(nodes: &[TemplateNode]) -> bool {
    nodes.iter().any(|node| {
        matches!(node.kind, TemplateNodeKind::Slot)
            || node.kind.children().is_some_and(contains_slot)
    })
}

fn client_failure(node: &TemplateNode, code: u16, message: &str) -> Failure {
    Failure::one(diagnostic(
        code,
        message,
        None,
        source_span(&node.source_path, node.range.start, node.range.end),
    ))
}

fn serialization_failure(program: &TemplateProgram, error: &serde_json::Error) -> Failure {
    Failure::one(diagnostic(
        1305,
        format!("Cannot serialize the Tac client view: {error}"),
        None,
        source_span(&program.source_path, 0, 0),
    ))
}

/// Browser renderer for `tac.html`.
///
/// The runtime interprets compiler-produced JSON and never parses authored
/// JavaScript expressions. All Tac structure is created here in the browser;
/// the server response is only a bootstrap document.
pub(crate) const TAC_CLIENT_RUNTIME: &str = include_str!("tac-client.js");
