use super::expression::Expression;
use std::collections::BTreeMap;
use tachyon_contracts::{ViewIr, ViewNode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceRange {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TemplateProgram {
    pub(crate) source_path: String,
    pub(crate) nodes: Vec<TemplateNode>,
    pub(crate) is_document: bool,
    pub(crate) has_doctype: bool,
}

impl TemplateProgram {
    pub(crate) fn view_ir(&self) -> ViewIr {
        let nodes = lower_siblings(&self.nodes);
        let root = if nodes.len() == 1 {
            nodes.into_iter().next().unwrap_or_else(|| unreachable!())
        } else {
            ViewNode::Element {
                tag: String::from("html"),
                attributes: BTreeMap::new(),
                children: vec![ViewNode::Element {
                    tag: String::from("body"),
                    attributes: BTreeMap::new(),
                    children: nodes,
                }],
            }
        };
        ViewIr::v1(self.source_path.clone(), root)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TemplateNode {
    pub(crate) kind: TemplateNodeKind,
    pub(crate) source_path: String,
    pub(crate) range: SourceRange,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TemplateNodeKind {
    Element {
        tag: String,
        attributes: BTreeMap<String, TemplateAttribute>,
        children: Vec<TemplateNode>,
        void: bool,
    },
    Text(Vec<TextPart>),
    Comment(String),
    Conditional {
        condition: Option<Expression>,
        branch: ConditionalBranch,
        children: Vec<TemplateNode>,
    },
    Iteration {
        binding: String,
        iterable: Expression,
        children: Vec<TemplateNode>,
    },
    CountedIteration {
        binding: String,
        from: Expression,
        comparison: tachyon_contracts::CountedComparison,
        to: Expression,
        step: Expression,
        children: Vec<TemplateNode>,
    },
    /// `<switch :value>`, desugared into a conditional chain after parsing.
    Switch {
        value: Expression,
        children: Vec<TemplateNode>,
    },
    /// `<case :when>` or `<case default>`, always a child of a `<switch>`.
    Case {
        when: Option<Expression>,
        children: Vec<TemplateNode>,
    },
    Component {
        name: String,
        properties: BTreeMap<String, TemplateAttribute>,
        hydrate: Option<HydrationPolicy>,
        children: Vec<TemplateNode>,
    },
    Slot,
}

impl TemplateNodeKind {
    pub(crate) fn children(&self) -> Option<&[TemplateNode]> {
        match self {
            Self::Element { children, .. }
            | Self::Conditional { children, .. }
            | Self::Iteration { children, .. }
            | Self::CountedIteration { children, .. }
            | Self::Switch { children, .. }
            | Self::Case { children, .. }
            | Self::Component { children, .. } => Some(children),
            Self::Text(_) | Self::Comment(_) | Self::Slot => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TemplateAttribute {
    pub(crate) value: AttributeValue,
    pub(crate) range: SourceRange,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AttributeValue {
    Static(String),
    Dynamic(Expression),
    Control(String),
    /// An `on:<event>` binding, serialised once the render scope is known.
    Event(EventBinding),
}

/// One parsed `on:<event>` binding.
///
/// A binding either calls an exported handler or assigns to a field on the
/// island's companion instance. Assignment is only meaningful where an
/// instance exists, which the compiler checks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EventBinding {
    pub(crate) handler: String,
    pub(crate) arguments: Vec<EventArgument>,
    pub(crate) assign: Option<Assignment>,
}

/// The target and operator of an assigning binding.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Assignment {
    pub(crate) target: String,
    pub(crate) operator: String,
}

/// One argument of an event binding.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum EventArgument {
    /// A literal, already in JSON form.
    Literal(String),
    /// A dotted path read off the dispatched event; empty means the event.
    EventPath(String),
    /// A template expression, evaluated where the binding is rendered, so a
    /// loop can pass the item it is iterating.
    Scope(Expression),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TextPart {
    Literal(String, SourceRange),
    Interpolation(Expression, SourceRange),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConditionalBranch {
    If,
    ElseIf,
    Else,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HydrationPolicy {
    Load,
    Idle,
    Visible,
    Interaction,
    Never,
}

impl HydrationPolicy {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Idle => "idle",
            Self::Visible => "visible",
            Self::Interaction => "interaction",
            Self::Never => "never",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "load" => Some(Self::Load),
            "idle" => Some(Self::Idle),
            "visible" => Some(Self::Visible),
            "interaction" => Some(Self::Interaction),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}

fn lower_siblings(nodes: &[TemplateNode]) -> Vec<ViewNode> {
    let mut lowered = Vec::new();
    let mut index = 0;
    while index < nodes.len() {
        let node = &nodes[index];
        if let TemplateNodeKind::Conditional {
            branch: ConditionalBranch::If,
            condition: Some(condition),
            children,
        } = &node.kind
        {
            let (otherwise, consumed) = lower_conditional_tail(nodes, index + 1);
            lowered.push(ViewNode::Conditional {
                condition: String::from(condition.source()),
                then: lower_siblings(children),
                otherwise,
            });
            index = consumed;
            continue;
        }
        if let Some(node) = lower_node(node) {
            lowered.push(node);
        }
        index += 1;
    }
    lowered
}

fn lower_conditional_tail(nodes: &[TemplateNode], mut index: usize) -> (Vec<ViewNode>, usize) {
    let mut trivia = Vec::new();
    while index < nodes.len() && is_trivia(&nodes[index]) {
        if let Some(node) = lower_node(&nodes[index]) {
            trivia.push(node);
        }
        index += 1;
    }
    let Some(node) = nodes.get(index) else {
        return (Vec::new(), index);
    };
    match &node.kind {
        TemplateNodeKind::Conditional {
            branch: ConditionalBranch::ElseIf,
            condition: Some(condition),
            children,
        } => {
            let (otherwise, consumed) = lower_conditional_tail(nodes, index + 1);
            (
                vec![ViewNode::Conditional {
                    condition: String::from(condition.source()),
                    then: lower_siblings(children),
                    otherwise,
                }],
                consumed,
            )
        }
        TemplateNodeKind::Conditional {
            branch: ConditionalBranch::Else,
            children,
            ..
        } => (lower_siblings(children), index + 1),
        _ => (trivia, index),
    }
}

fn lower_node(node: &TemplateNode) -> Option<ViewNode> {
    match &node.kind {
        TemplateNodeKind::Element {
            tag,
            attributes,
            children,
            ..
        } => Some(ViewNode::Element {
            tag: tag.clone(),
            attributes: lower_attributes(attributes),
            children: lower_siblings(children),
        }),
        TemplateNodeKind::Text(parts) => Some(ViewNode::Text {
            value: parts
                .iter()
                .map(|part| match part {
                    TextPart::Literal(value, _) => value.clone(),
                    TextPart::Interpolation(expression, _) => {
                        format!("{{{}}}", expression.source())
                    }
                })
                .collect(),
        }),
        // Switch and Case are desugared into a conditional chain during
        // parsing, so neither reaches the view IR.
        TemplateNodeKind::Comment(_)
        | TemplateNodeKind::Conditional { .. }
        | TemplateNodeKind::Switch { .. }
        | TemplateNodeKind::Case { .. } => None,
        TemplateNodeKind::Iteration {
            binding,
            iterable,
            children,
        } => Some(ViewNode::Iteration {
            binding: binding.clone(),
            iterable: String::from(iterable.source()),
            body: lower_siblings(children),
            empty: Vec::new(),
        }),
        TemplateNodeKind::Component {
            name,
            properties,
            hydrate,
            children,
        } => {
            let mut properties = lower_attributes(properties);
            if let Some(policy) = hydrate {
                properties.insert(String::from("hydrate"), String::from(policy.name()));
            }
            Some(ViewNode::Component {
                name: name.clone(),
                properties,
                children: lower_siblings(children),
            })
        }
        TemplateNodeKind::CountedIteration {
            binding,
            from,
            comparison,
            to,
            step,
            children,
        } => Some(ViewNode::CountedIteration {
            binding: binding.clone(),
            from: String::from(from.source()),
            comparison: *comparison,
            to: String::from(to.source()),
            step: String::from(step.source()),
            body: lower_siblings(children),
        }),
        TemplateNodeKind::Slot => Some(ViewNode::Element {
            tag: String::from("slot"),
            attributes: BTreeMap::new(),
            children: Vec::new(),
        }),
    }
}

fn lower_attributes(attributes: &BTreeMap<String, TemplateAttribute>) -> BTreeMap<String, String> {
    attributes
        .iter()
        .map(|(name, attribute)| match &attribute.value {
            AttributeValue::Static(value) => (name.clone(), value.clone()),
            AttributeValue::Dynamic(expression) => {
                (format!(":{name}"), String::from(expression.source()))
            }
            AttributeValue::Control(value) => (format!(":{name}"), value.clone()),
            // The view IR records the authored binding, not its payload.
            AttributeValue::Event(binding) => (name.clone(), binding.handler.clone()),
        })
        .collect()
}

pub(crate) fn is_trivia(node: &TemplateNode) -> bool {
    match &node.kind {
        TemplateNodeKind::Text(parts) => parts.iter().all(|part| match part {
            TextPart::Literal(value, _) => value.trim().is_empty(),
            TextPart::Interpolation(_, _) => false,
        }),
        TemplateNodeKind::Comment(_) => true,
        _ => false,
    }
}
