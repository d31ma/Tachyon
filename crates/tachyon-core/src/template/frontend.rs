use super::expression::Expression;
use super::model::{
    Assignment, AttributeValue, ConditionalBranch, EventArgument, EventBinding, HydrationPolicy,
    SourceRange, TemplateAttribute, TemplateNode, TemplateNodeKind, TemplateProgram, TextPart,
    is_trivia,
};
use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::html::{is_event_handler_attribute, is_standard_html_tag, valid_custom_element};
use html5gum::{DefaultEmitter, HtmlString, Token, Tokenizer};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_HTML_BYTES: u64 = 1_048_576;
const MAX_TEMPLATE_DEPTH: usize = 64;
const MAX_DIAGNOSTICS: usize = 100;
const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TemplateFrontend;

impl TemplateFrontend {
    pub(crate) fn compile_file(
        path: &Path,
        source_path: &str,
        components: &BTreeSet<String>,
    ) -> Result<TemplateProgram, Failure> {
        let source = read_source(path, source_path)?;
        Self::compile(&source, source_path, components)
    }

    pub(crate) fn compile(
        source: &str,
        source_path: &str,
        components: &BTreeSet<String>,
    ) -> Result<TemplateProgram, Failure> {
        if source.trim().is_empty() {
            return Err(Failure::one(template_diagnostic(
                1301,
                source_path,
                0,
                source.len(),
                "Template source is empty.",
                "Add at least one semantic HTML element.",
            )));
        }
        if let Some(index) = source.find('\0') {
            return Err(Failure::one(template_diagnostic(
                1301,
                source_path,
                index,
                index + 1,
                "Template source contains a NUL byte.",
                "Remove the NUL byte and save the source as UTF-8.",
            )));
        }
        let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
        Parser::new(&normalized, source_path, components).parse()
    }
}

struct Parser<'a> {
    source: &'a str,
    source_path: &'a str,
    components: &'a BTreeSet<String>,
    roots: Vec<TemplateNode>,
    stack: Vec<OpenNode>,
    diagnostics: Vec<tachyon_diagnostics::Diagnostic>,
    has_doctype: bool,
    is_document: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, source_path: &'a str, components: &'a BTreeSet<String>) -> Self {
        Self {
            source,
            source_path,
            components,
            roots: Vec::new(),
            stack: Vec::new(),
            diagnostics: Vec::new(),
            has_doctype: false,
            is_document: false,
        }
    }

    fn parse(mut self) -> Result<TemplateProgram, Failure> {
        let emitter = DefaultEmitter::<usize>::new_with_span();
        for result in Tokenizer::new_with_emitter(self.source, emitter) {
            let token = match result {
                Ok(token) => token,
                Err(infallible) => match infallible {},
            };
            match token {
                Token::StartTag(tag) => self.start_tag(tag),
                Token::EndTag(tag) => self.end_tag(
                    &String::from_utf8_lossy(&tag.name),
                    SourceRange {
                        start: tag.span.start,
                        end: tag.span.end,
                    },
                ),
                Token::String(text) => {
                    let range = SourceRange {
                        start: text.span.start,
                        end: text.span.end,
                    };
                    let raw = self.source.get(range.start..range.end).unwrap_or_default();
                    let parts = if self.stack.last().is_some_and(|open| open.tag == "style") {
                        Ok(vec![TextPart::Literal(String::from(raw), range)])
                    } else {
                        text_parts(raw, self.source_path, range.start)
                    };
                    match parts {
                        Ok(parts) => self.push_node(TemplateNodeKind::Text(parts), range),
                        Err(failure) => self.extend_failure(&failure),
                    }
                }
                Token::Comment(comment) => {
                    let range = SourceRange {
                        start: comment.span.start,
                        end: comment.span.end,
                    };
                    let raw = self.source.get(range.start..range.end).unwrap_or_default();
                    self.push_node(TemplateNodeKind::Comment(String::from(raw)), range);
                }
                Token::Doctype(_) => {
                    self.has_doctype = true;
                    self.is_document = true;
                }
                Token::Error(error) => self.push_diagnostic(template_diagnostic(
                    1301,
                    self.source_path,
                    error.span.start,
                    error.span.end,
                    "Template contains an HTML tokenizer error.",
                    "Correct the malformed token before building the view.",
                )),
            }
        }

        while let Some(open) = self.stack.pop() {
            self.push_diagnostic(template_diagnostic(
                1301,
                self.source_path,
                open.range.start,
                open.range.end,
                &format!("Template element '<{}>' is not closed.", open.tag),
                "Close every non-void template element explicitly.",
            ));
            let node = open.finish(self.source_path);
            self.append(node);
        }
        validate_conditionals(&self.roots, &mut self.diagnostics);
        sort_diagnostics(&mut self.diagnostics);
        if !self.diagnostics.is_empty() {
            return Err(Failure::new(self.diagnostics));
        }
        let mut nodes = self.roots;
        desugar_switches(&mut nodes, self.source_path)?;
        Ok(TemplateProgram {
            source_path: String::from(self.source_path),
            nodes,
            is_document: self.is_document,
            has_doctype: self.has_doctype,
        })
    }

    fn start_tag(&mut self, tag: html5gum::StartTag<usize>) {
        let name = String::from_utf8_lossy(&tag.name).into_owned();
        let range = SourceRange {
            start: tag.span.start,
            end: tag.span.end,
        };
        if name == "html" {
            self.is_document = true;
        }
        let attributes = match self.attributes(&name, tag.attributes, range) {
            Ok(attributes) => attributes,
            Err(failure) => {
                self.extend_failure(&failure);
                BTreeMap::new()
            }
        };
        let kind = match self.node_kind(&name, attributes, range) {
            Ok(kind) => kind,
            Err(failure) => {
                self.extend_failure(&failure);
                TemplateNodeKind::Element {
                    tag: name.clone(),
                    attributes: BTreeMap::new(),
                    children: Vec::new(),
                    void: tag.self_closing || VOID_TAGS.contains(&name.as_str()),
                }
            }
        };
        let closed = tag.self_closing || VOID_TAGS.contains(&name.as_str());
        if closed {
            self.push_node(kind, range);
            return;
        }
        if self.stack.len() >= MAX_TEMPLATE_DEPTH {
            self.push_diagnostic(template_diagnostic(
                1301,
                self.source_path,
                range.start,
                range.end,
                "Template nesting exceeds the limit of 64.",
                "Flatten the view or component structure.",
            ));
            return;
        }
        self.stack.push(OpenNode {
            tag: name,
            kind,
            range,
        });
    }

    fn end_tag(&mut self, name: &str, range: SourceRange) {
        let Some(position) = self.stack.iter().rposition(|open| open.tag == name) else {
            self.push_diagnostic(template_diagnostic(
                1301,
                self.source_path,
                range.start,
                range.end,
                &format!("Closing tag '</{name}>' has no matching open element."),
                "Balance template element start and end tags.",
            ));
            return;
        };
        while self.stack.len() > position + 1 {
            let open = self.stack.pop().unwrap_or_else(|| unreachable!());
            self.push_diagnostic(template_diagnostic(
                1301,
                self.source_path,
                open.range.start,
                range.end,
                &format!("Element '<{}>' closes out of order.", open.tag),
                "Use explicitly balanced, properly nested template markup.",
            ));
            let node = open.finish(self.source_path);
            self.append(node);
        }
        let mut open = self.stack.pop().unwrap_or_else(|| unreachable!());
        open.range.end = range.end;
        let node = open.finish(self.source_path);
        self.append(node);
    }

    /// Records one `on:<event>` binding as a delegated marker attribute.
    ///
    /// The value is never evaluated as JavaScript. It is parsed into a handler
    /// name and literal arguments, so a view cannot introduce executable code
    /// into the generated document.
    fn bind_event(
        &self,
        event: &str,
        value: &str,
        range: SourceRange,
        parsed: &mut BTreeMap<String, TemplateAttribute>,
        diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
    ) {
        match event_binding(event, value, self.source_path, range) {
            Ok((marker, binding)) => {
                parsed.insert(
                    marker,
                    TemplateAttribute {
                        value: AttributeValue::Event(binding),
                        range,
                    },
                );
            }
            Err(message) => diagnostics.push(template_diagnostic(
                1306,
                self.source_path,
                range.start,
                range.end,
                message,
                "Bind an exported handler, as in on:click=\"increment()\".",
            )),
        }
    }

    fn attributes(
        &self,
        tag: &str,
        attributes: BTreeMap<HtmlString, html5gum::Spanned<HtmlString, usize>>,
        tag_range: SourceRange,
    ) -> Result<BTreeMap<String, TemplateAttribute>, Failure> {
        let mut parsed = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for (name, value) in attributes {
            let name = String::from_utf8_lossy(&name).into_owned();
            let range = SourceRange {
                start: value_span_start(value.span.start, tag_range.start),
                end: value.span.end.max(tag_range.start),
            };
            let value = String::from_utf8_lossy(&value.value).into_owned();
            if let Some(event) = name.strip_prefix("on:") {
                self.bind_event(event, &value, range, &mut parsed, &mut diagnostics);
                continue;
            }
            if is_event_handler_attribute(&name) {
                diagnostics.push(template_diagnostic(
                    1306,
                    self.source_path,
                    range.start,
                    range.end,
                    "Native event attributes are outside the view contract.",
                    "Use on:<event> to bind an exported handler instead.",
                ));
                continue;
            }
            if name == ":hydrate" {
                diagnostics.push(template_diagnostic(
                    1404,
                    self.source_path,
                    range.start,
                    range.end,
                    "Island hydration policy must be a static literal.",
                    "Use hydrate=\"load|idle|visible|interaction|never\".",
                ));
                continue;
            }
            if let Some(dynamic_name) = name.strip_prefix(':') {
                if dynamic_name.is_empty() || tag == "logic" && dynamic_name == "else" {
                    diagnostics.push(template_diagnostic(
                        1304,
                        self.source_path,
                        range.start,
                        range.end,
                        "Dynamic attribute name is invalid.",
                        "Place ':' before a valid HTML attribute or component property name.",
                    ));
                    continue;
                }
                if matches!((tag, dynamic_name), ("loop", "for") | ("for", "each")) {
                    parsed.insert(
                        String::from(dynamic_name),
                        TemplateAttribute {
                            value: AttributeValue::Control(value),
                            range,
                        },
                    );
                    continue;
                }
                match Expression::parse(&value, self.source_path, range.start) {
                    Ok(expression) => {
                        parsed.insert(
                            String::from(dynamic_name),
                            TemplateAttribute {
                                value: AttributeValue::Dynamic(expression),
                                range,
                            },
                        );
                    }
                    Err(failure) => diagnostics.extend_from_slice(failure.diagnostics()),
                }
            } else {
                parsed.insert(
                    name,
                    TemplateAttribute {
                        value: AttributeValue::Static(value),
                        range,
                    },
                );
            }
        }
        if diagnostics.is_empty() {
            Ok(parsed)
        } else {
            Err(Failure::new(diagnostics))
        }
    }

    fn node_kind(
        &self,
        name: &str,
        mut attributes: BTreeMap<String, TemplateAttribute>,
        range: SourceRange,
    ) -> Result<TemplateNodeKind, Failure> {
        match name {
            "logic" => conditional_logic(&mut attributes, self.source_path, range),
            "if" => direct_if(&mut attributes, self.source_path, range),
            "else" => direct_else(&mut attributes, self.source_path, range),
            "switch" => switch_value(&mut attributes, self.source_path, range),
            "case" => switch_case(&mut attributes, self.source_path, range),
            "loop" => iteration(&mut attributes, ":for", "for", self.source_path, range),
            "for" => iteration(&mut attributes, ":each", "each", self.source_path, range),
            "script" => Err(Failure::one(template_diagnostic(
                1306,
                self.source_path,
                range.start,
                range.end,
                "Executable script elements are outside the Phase 3 view contract.",
                "Use a colocated Tac island companion.",
            ))),
            "slot" => {
                reject_attributes(&attributes, self.source_path, range, "slot")?;
                Ok(TemplateNodeKind::Slot)
            }
            _ if self.components.contains(name) => {
                let hydrate = take_hydrate(&mut attributes, self.source_path, range)?;
                Ok(TemplateNodeKind::Component {
                    name: String::from(name),
                    properties: attributes,
                    hydrate,
                    children: Vec::new(),
                })
            }
            // A registered component is matched earlier; anything left must be
            // a standard element or a custom-element-shaped tag.
            _ if is_standard_html_tag(name) || valid_custom_element(name) => {
                if attributes.contains_key("hydrate") {
                    return Err(Failure::one(template_diagnostic(
                        1404,
                        self.source_path,
                        range.start,
                        range.end,
                        "Only a registered Tac component may declare hydrate.",
                        "Remove hydrate or register the component template.",
                    )));
                }
                Ok(TemplateNodeKind::Element {
                    tag: String::from(name),
                    attributes,
                    children: Vec::new(),
                    void: VOID_TAGS.contains(&name),
                })
            }
            _ => Err(Failure::one(template_diagnostic(
                1402,
                self.source_path,
                range.start,
                range.end,
                &format!("Unknown Tac component '<{name}>'."),
                "Register a custom-element-shaped component or use standard HTML.",
            ))),
        }
    }

    fn push_node(&mut self, kind: TemplateNodeKind, range: SourceRange) {
        self.append(TemplateNode {
            kind,
            source_path: String::from(self.source_path),
            range,
        });
    }

    fn append(&mut self, node: TemplateNode) {
        let range = node.range;
        let Some(parent) = self.stack.last_mut() else {
            self.roots.push(node);
            return;
        };
        // A leaf such as `<slot>` never holds children, but malformed markup
        // can still leave one open on the stack. Refuse the child rather than
        // silently reparenting it, and never panic on the recovered state.
        let refused = match children_mut(&mut parent.kind) {
            Some(children) => {
                children.push(node);
                None
            }
            None => Some(parent.tag.clone()),
        };
        let Some(tag) = refused else {
            return;
        };
        self.push_diagnostic(template_diagnostic(
            1301,
            self.source_path,
            range.start,
            range.end,
            &format!("Element '<{tag}>' cannot contain child content."),
            "Leave the element empty and close it immediately.",
        ));
    }

    fn extend_failure(&mut self, failure: &Failure) {
        for value in failure.diagnostics() {
            self.push_diagnostic(value.clone());
        }
    }

    fn push_diagnostic(&mut self, value: tachyon_diagnostics::Diagnostic) {
        if self.diagnostics.len() < MAX_DIAGNOSTICS {
            self.diagnostics.push(value);
        }
    }
}

struct OpenNode {
    tag: String,
    kind: TemplateNodeKind,
    range: SourceRange,
}

impl OpenNode {
    fn finish(self, source_path: &str) -> TemplateNode {
        TemplateNode {
            kind: self.kind,
            source_path: String::from(source_path),
            range: self.range,
        }
    }
}

/// Parses `<switch :value="expression">`.
fn switch_value(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    source_path: &str,
    range: SourceRange,
) -> Result<TemplateNodeKind, Failure> {
    let Some(value) = take_dynamic(attributes, "value") else {
        return Err(control_failure(
            source_path,
            range,
            "<switch> requires :value=\"expression\".",
        ));
    };
    reject_attributes(attributes, source_path, range, "switch")?;
    Ok(TemplateNodeKind::Switch {
        value,
        children: Vec::new(),
    })
}

/// Parses `<case :when="expression">` or `<case default>`.
fn switch_case(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    source_path: &str,
    range: SourceRange,
) -> Result<TemplateNodeKind, Failure> {
    if let Some(when) = take_dynamic(attributes, "when") {
        reject_attributes(attributes, source_path, range, "case")?;
        return Ok(TemplateNodeKind::Case {
            when: Some(when),
            children: Vec::new(),
        });
    }
    if take_static_flag(attributes, "default") {
        reject_attributes(attributes, source_path, range, "case")?;
        return Ok(TemplateNodeKind::Case {
            when: None,
            children: Vec::new(),
        });
    }
    Err(control_failure(
        source_path,
        range,
        "<case> requires exactly one of :when or default.",
    ))
}

/// Rewrites every `<switch>` into the conditional chain it stands for.
///
/// A switch is sugar: `<case :when="a">` is `value == a`, the first match
/// wins, and `<case default>` is the else. Desugaring here means the renderer,
/// the native planner, and the view IR never learn a third control shape.
fn desugar_switches(nodes: &mut Vec<TemplateNode>, source_path: &str) -> Result<(), Failure> {
    let mut index = 0;
    while index < nodes.len() {
        if let TemplateNodeKind::Switch { .. } = nodes[index].kind {
            let node = nodes.remove(index);
            let expanded = expand_switch(node, source_path)?;
            let count = expanded.len();
            for (offset, branch) in expanded.into_iter().enumerate() {
                nodes.insert(index + offset, branch);
            }
            index += count;
            continue;
        }
        if let Some(children) = children_mut(&mut nodes[index].kind) {
            desugar_switches(children, source_path)?;
        }
        index += 1;
    }
    Ok(())
}

/// Converts one `<switch>` node into its conditional branches.
fn expand_switch(node: TemplateNode, source_path: &str) -> Result<Vec<TemplateNode>, Failure> {
    let range = node.range;
    let node_source = node.source_path.clone();
    let TemplateNodeKind::Switch { value, children } = node.kind else {
        unreachable!("caller matched a switch")
    };
    let mut branches = Vec::new();
    let mut seen_default = false;
    for child in children {
        let child_range = child.range;
        let TemplateNodeKind::Case { when, mut children } = child.kind else {
            // Whitespace and comments between cases are not content.
            if matches!(&child.kind, TemplateNodeKind::Comment(_)) {
                continue;
            }
            if let TemplateNodeKind::Text(parts) = &child.kind
                && parts.iter().all(is_blank_text)
            {
                continue;
            }
            return Err(control_failure(
                source_path,
                child_range,
                "<switch> may only contain <case> children.",
            ));
        };
        if seen_default {
            return Err(control_failure(
                source_path,
                child_range,
                "<case default> must be the last case.",
            ));
        }
        desugar_switches(&mut children, source_path)?;
        let (condition, branch) = if let Some(when) = when {
            let branch = if branches.is_empty() {
                ConditionalBranch::If
            } else {
                ConditionalBranch::ElseIf
            };
            (Some(value.equals(&when)), branch)
        } else {
            seen_default = true;
            (None, ConditionalBranch::Else)
        };
        if condition.is_none() && branches.is_empty() {
            return Err(control_failure(
                source_path,
                child_range,
                "<switch> needs at least one <case :when> before <case default>.",
            ));
        }
        branches.push(TemplateNode {
            kind: TemplateNodeKind::Conditional {
                condition,
                branch,
                children,
            },
            range: child_range,
            source_path: node_source.clone(),
        });
    }
    if branches.is_empty() {
        return Err(control_failure(
            source_path,
            range,
            "<switch> requires at least one <case>.",
        ));
    }
    Ok(branches)
}

/// Returns a node's children when it has any.
fn children_mut(kind: &mut TemplateNodeKind) -> Option<&mut Vec<TemplateNode>> {
    match kind {
        TemplateNodeKind::Element { children, .. }
        | TemplateNodeKind::Conditional { children, .. }
        | TemplateNodeKind::Iteration { children, .. }
        | TemplateNodeKind::Switch { children, .. }
        | TemplateNodeKind::Case { children, .. }
        | TemplateNodeKind::Component { children, .. } => Some(children),
        TemplateNodeKind::Text(_) | TemplateNodeKind::Comment(_) | TemplateNodeKind::Slot => None,
    }
}

/// Reports whether one text part is only whitespace.
fn is_blank_text(part: &TextPart) -> bool {
    match part {
        TextPart::Literal(value, _) => value.trim().is_empty(),
        TextPart::Interpolation(..) => false,
    }
}

fn conditional_logic(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    source_path: &str,
    range: SourceRange,
) -> Result<TemplateNodeKind, Failure> {
    let condition = take_dynamic(attributes, "if")
        .map(|value| (ConditionalBranch::If, value))
        .or_else(|| {
            take_dynamic(attributes, "else-if").map(|value| (ConditionalBranch::ElseIf, value))
        });
    if let Some((branch, condition)) = condition {
        reject_attributes(attributes, source_path, range, "logic")?;
        return Ok(TemplateNodeKind::Conditional {
            condition: Some(condition),
            branch,
            children: Vec::new(),
        });
    }
    if take_static_flag(attributes, "else") {
        reject_attributes(attributes, source_path, range, "logic")?;
        return Ok(TemplateNodeKind::Conditional {
            condition: None,
            branch: ConditionalBranch::Else,
            children: Vec::new(),
        });
    }
    Err(control_failure(
        source_path,
        range,
        "<logic> requires exactly one of :if, :else-if, or else.",
    ))
}

fn direct_if(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    source_path: &str,
    range: SourceRange,
) -> Result<TemplateNodeKind, Failure> {
    let Some(condition) = take_dynamic(attributes, "when") else {
        return Err(Failure::one(template_diagnostic(
            1103,
            source_path,
            range.start,
            range.end,
            "<if> requires the Phase 3 :when=\"expression\" contract.",
            "Replace legacy condition attributes with :when.",
        )));
    };
    reject_attributes(attributes, source_path, range, "if")?;
    Ok(TemplateNodeKind::Conditional {
        condition: Some(condition),
        branch: ConditionalBranch::If,
        children: Vec::new(),
    })
}

fn direct_else(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    source_path: &str,
    range: SourceRange,
) -> Result<TemplateNodeKind, Failure> {
    reject_attributes(attributes, source_path, range, "else")?;
    Ok(TemplateNodeKind::Conditional {
        condition: None,
        branch: ConditionalBranch::Else,
        children: Vec::new(),
    })
}

fn iteration(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    _display_name: &str,
    attribute_name: &str,
    source_path: &str,
    range: SourceRange,
) -> Result<TemplateNodeKind, Failure> {
    let Some(attribute) = attributes.remove(attribute_name) else {
        return Err(control_failure(
            source_path,
            range,
            &format!(
                "<{}> requires :{}=\"item of values\".",
                if attribute_name == "for" {
                    "loop"
                } else {
                    "for"
                },
                attribute_name
            ),
        ));
    };
    let AttributeValue::Control(declaration) = attribute.value else {
        return Err(control_failure(
            source_path,
            range,
            "Iteration declaration must use a dynamic control attribute.",
        ));
    };
    let separator = if attribute_name == "for" {
        " of "
    } else {
        " in "
    };
    let Some((binding, iterable)) = declaration.split_once(separator) else {
        return Err(control_failure(
            source_path,
            range,
            "Iteration must use 'binding of expression' or 'binding in expression'.",
        ));
    };
    if !valid_binding(binding.trim()) {
        return Err(control_failure(
            source_path,
            range,
            "Iteration binding is not a portable identifier.",
        ));
    }
    let iterable_start = attribute
        .range
        .start
        .saturating_add(declaration.find(iterable).unwrap_or_default());
    let iterable = Expression::parse(iterable.trim(), source_path, iterable_start)?;
    reject_attributes(attributes, source_path, range, "iteration")?;
    Ok(TemplateNodeKind::Iteration {
        binding: String::from(binding.trim()),
        iterable,
        children: Vec::new(),
    })
}

fn take_hydrate(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    source_path: &str,
    range: SourceRange,
) -> Result<Option<HydrationPolicy>, Failure> {
    let Some(attribute) = attributes.remove("hydrate") else {
        return Ok(None);
    };
    let AttributeValue::Static(value) = attribute.value else {
        return Err(Failure::one(template_diagnostic(
            1404,
            source_path,
            range.start,
            range.end,
            "Island hydration policy must be a static literal.",
            "Use hydrate=\"load|idle|visible|interaction|never\".",
        )));
    };
    HydrationPolicy::parse(&value).map(Some).ok_or_else(|| {
        Failure::one(template_diagnostic(
            1404,
            source_path,
            attribute.range.start,
            attribute.range.end,
            "Island hydration policy is not supported.",
            "Use load, idle, visible, interaction, or never.",
        ))
    })
}

fn take_dynamic(
    attributes: &mut BTreeMap<String, TemplateAttribute>,
    name: &str,
) -> Option<Expression> {
    attributes
        .remove(name)
        .and_then(|attribute| match attribute.value {
            AttributeValue::Dynamic(expression) => Some(expression),
            AttributeValue::Static(_) | AttributeValue::Control(_) | AttributeValue::Event(_) => {
                None
            }
        })
}

fn take_static_flag(attributes: &mut BTreeMap<String, TemplateAttribute>, name: &str) -> bool {
    attributes.remove(name).is_some_and(
        |attribute| matches!(attribute.value, AttributeValue::Static(value) if value.is_empty()),
    )
}

fn reject_attributes(
    attributes: &BTreeMap<String, TemplateAttribute>,
    source_path: &str,
    range: SourceRange,
    element: &str,
) -> Result<(), Failure> {
    if attributes.is_empty() {
        return Ok(());
    }
    Err(control_failure(
        source_path,
        range,
        &format!("<{element}> contains unsupported attributes."),
    ))
}

fn validate_conditionals(
    nodes: &[TemplateNode],
    diagnostics: &mut Vec<tachyon_diagnostics::Diagnostic>,
) {
    let mut chain = false;
    for node in nodes {
        match &node.kind {
            TemplateNodeKind::Conditional {
                branch: ConditionalBranch::If,
                ..
            } => chain = true,
            TemplateNodeKind::Conditional {
                branch: ConditionalBranch::ElseIf,
                ..
            } if chain => {}
            TemplateNodeKind::Conditional {
                branch: ConditionalBranch::Else,
                ..
            } if chain => chain = false,
            TemplateNodeKind::Conditional {
                branch: ConditionalBranch::ElseIf | ConditionalBranch::Else,
                ..
            } => diagnostics.push(template_diagnostic(
                1302,
                &node.source_path,
                node.range.start,
                node.range.end,
                "Conditional else branch has no preceding if branch.",
                "Place else-if or else immediately after an if branch.",
            )),
            _ if is_trivia(node) => {}
            _ => chain = false,
        }
        if let Some(children) = node.kind.children() {
            validate_conditionals(children, diagnostics);
        }
    }
}

fn text_parts(
    value: &str,
    source_path: &str,
    source_start: usize,
) -> Result<Vec<TextPart>, Failure> {
    let mut parts = Vec::new();
    let mut literal_start = 0;
    let mut current = 0;
    while current < value.len() {
        if value.as_bytes()[current] == b'\\'
            && value
                .as_bytes()
                .get(current + 1)
                .is_some_and(|next| matches!(next, b'{' | b'}'))
        {
            if literal_start < current {
                parts.push(TextPart::Literal(
                    String::from(&value[literal_start..current]),
                    SourceRange {
                        start: source_start + literal_start,
                        end: source_start + current,
                    },
                ));
            }
            parts.push(TextPart::Literal(
                String::from(&value[current + 1..current + 2]),
                SourceRange {
                    start: source_start + current,
                    end: source_start + current + 2,
                },
            ));
            current += 2;
            literal_start = current;
            continue;
        }
        if value.as_bytes()[current] != b'{' {
            let character = value[current..]
                .chars()
                .next()
                .unwrap_or_else(|| unreachable!());
            current += character.len_utf8();
            continue;
        }
        if literal_start < current {
            parts.push(TextPart::Literal(
                String::from(&value[literal_start..current]),
                SourceRange {
                    start: source_start + literal_start,
                    end: source_start + current,
                },
            ));
        }
        let Some(relative_end) = value[current + 1..].find('}') else {
            return Err(Failure::one(template_diagnostic(
                1303,
                source_path,
                source_start + current,
                source_start + value.len(),
                "Text interpolation is missing '}'.",
                "Close the bounded template expression.",
            )));
        };
        let end = current + 1 + relative_end;
        let expression_source = value[current + 1..end].trim();
        if expression_source.starts_with('!') {
            return Err(Failure::one(template_diagnostic(
                1306,
                source_path,
                source_start + current,
                source_start + end + 1,
                "Raw HTML interpolation is not supported.",
                "Use escaped interpolation and semantic component structure.",
            )));
        }
        let leading = value[current + 1..end].len() - value[current + 1..end].trim_start().len();
        let expression = Expression::parse(
            expression_source,
            source_path,
            source_start + current + 1 + leading,
        )?;
        parts.push(TextPart::Interpolation(
            expression,
            SourceRange {
                start: source_start + current,
                end: source_start + end + 1,
            },
        ));
        current = end + 1;
        literal_start = current;
    }
    if literal_start < value.len() {
        parts.push(TextPart::Literal(
            String::from(&value[literal_start..]),
            SourceRange {
                start: source_start + literal_start,
                end: source_start + value.len(),
            },
        ));
    }
    Ok(parts)
}

fn read_source(path: &Path, source_path: &str) -> Result<String, Failure> {
    let file = File::open(path).map_err(|error| {
        Failure::one(template_diagnostic(
            1301,
            source_path,
            0,
            0,
            &format!("Cannot read template source: {error}"),
            "Check the source file permissions.",
        ))
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_HTML_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            Failure::one(template_diagnostic(
                1301,
                source_path,
                0,
                0,
                &format!("Cannot read template source: {error}"),
                "Check the source file permissions.",
            ))
        })?;
    if bytes.len() as u64 > MAX_HTML_BYTES {
        return Err(Failure::one(template_diagnostic(
            1301,
            source_path,
            0,
            bytes.len(),
            "Template source exceeds the 1 MiB limit.",
            "Reduce the template below 1 MiB.",
        )));
    }
    String::from_utf8(bytes).map_err(|error| {
        let start = error.utf8_error().valid_up_to();
        Failure::one(template_diagnostic(
            1301,
            source_path,
            start,
            start.saturating_add(1),
            "Template source is not valid UTF-8.",
            "Save Tachyon HTML sources as UTF-8.",
        ))
    })
}

fn valid_binding(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
}

fn value_span_start(value_start: usize, tag_start: usize) -> usize {
    value_start.max(tag_start)
}

fn control_failure(source_path: &str, range: SourceRange, message: &str) -> Failure {
    Failure::one(template_diagnostic(
        1302,
        source_path,
        range.start,
        range.end,
        message,
        "Use the control syntax documented in PHASE_3_SPEC.md.",
    ))
}

/// Maximum characters accepted in one event binding value.
const MAX_EVENT_BINDING_BYTES: usize = 256;
/// Maximum literal arguments accepted by one event binding.
/// Longest `$event` property path a binding may read.
const MAX_EVENT_PATH_SEGMENTS: usize = 4;
const MAX_EVENT_ARGUMENTS: usize = 8;

/// Parses one `on:<event>` binding into its emitted marker and payload.
///
/// The grammar is deliberately small: a handler name, optionally followed by
/// literal arguments. Nothing here is evaluated as JavaScript, so a view can
/// never introduce executable code into the generated document.
/// Parses one `on:<event>` binding into a marker attribute and a JSON payload.
///
/// Event bindings follow the convention every template framework converged on:
/// `$event` names the dispatched event, and a dotted path reads a property off
/// it, so `on:input="setName($event.target.value)"` works the way an author
/// coming from Vue, Angular, or Alpine expects.
///
/// The binding is parsed into a structure, never evaluated as JavaScript. The
/// runtime reads that structure and calls one exported function, so a template
/// cannot introduce script.
fn event_binding(
    event: &str,
    value: &str,
    source_path: &str,
    range: SourceRange,
) -> Result<(String, EventBinding), &'static str> {
    if event.is_empty() || event.len() > 64 {
        return Err("Event name must be 1 to 64 characters.");
    }
    if !event
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b':')
    {
        return Err("Event name must be lowercase alphanumeric, with ':' for namespaces.");
    }
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_EVENT_BINDING_BYTES {
        return Err("Event binding must be 1 to 256 characters.");
    }
    if let Some((target, operator, expression)) = split_assignment(value) {
        let marker = format!("data-tac-on-{}", event.replace(':', "__"));
        return Ok((
            marker,
            EventBinding {
                handler: String::new(),
                arguments: vec![event_argument(expression, source_path, range)?],
                assign: Some(Assignment { target, operator }),
            },
        ));
    }

    let (name, arguments) = match value.split_once('(') {
        None => (value, Vec::new()),
        Some((name, rest)) => {
            let Some(inner) = rest.strip_suffix(')') else {
                return Err("Event binding must close its argument list.");
            };
            let inner = inner.trim();
            let mut arguments = Vec::new();
            if !inner.is_empty() {
                for argument in inner.split(',') {
                    if arguments.len() >= MAX_EVENT_ARGUMENTS {
                        return Err("Event binding accepts at most eight arguments.");
                    }
                    arguments.push(event_argument(argument.trim(), source_path, range)?);
                }
            }
            (name.trim(), arguments)
        }
    };

    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("Event handler must name one exported identifier.");
    }

    // Colons in a namespaced event name are encoded so the marker stays a
    // valid attribute name, matching the legacy compiler's encoding.
    let marker = format!("data-tac-on-{}", event.replace(':', "__"));
    Ok((
        marker,
        EventBinding {
            handler: String::from(name),
            arguments,
            assign: None,
        },
    ))
}

/// Splits an assigning binding into its target, operator, and value.
///
/// A binding either calls a handler or assigns to a field, which is what every
/// template framework offers. Returns `None` for anything that is not an
/// assignment, including the comparisons that share the `=` character.
fn split_assignment(value: &str) -> Option<(String, String, &str)> {
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'=' {
            continue;
        }
        // `==`, `!=`, `<=` and `>=` are comparisons, not assignment.
        if bytes.get(index + 1) == Some(&b'=')
            || matches!(
                bytes.get(index.wrapping_sub(1)),
                Some(b'!' | b'<' | b'>' | b'=')
            )
        {
            return None;
        }
        let (left, right) = value.split_at(index);
        let right = right.get(1..)?.trim();
        let mut left = left.trim();
        let mut operator = String::from("=");
        for compound in ['+', '-', '*', '/'] {
            if let Some(stripped) = left.strip_suffix(compound) {
                operator = format!("{compound}=");
                left = stripped.trim();
                break;
            }
        }
        if right.is_empty() || !is_assignable_target(left) {
            return None;
        }
        return Some((String::from(left), operator, right));
    }
    None
}

/// Reports whether a name may be assigned on a companion instance.
///
/// A `$`-prefixed field is the persisted form, so both spellings are allowed.
fn is_assignable_target(name: &str) -> bool {
    let bare = name.trim_start_matches('$');
    !bare.is_empty()
        && bare.len() <= 64
        && bare
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bare
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Validates one event-binding argument.
///
/// An argument is a literal, a path off `$event`, or a template expression
/// evaluated where the binding renders — which is how a loop passes the item
/// it is iterating.
fn event_argument(
    value: &str,
    source_path: &str,
    range: SourceRange,
) -> Result<EventArgument, &'static str> {
    if let Some(path) = value.strip_prefix("$event") {
        return event_path(path).map(EventArgument::EventPath);
    }
    if let Ok(literal) = event_literal(value) {
        return Ok(EventArgument::Literal(literal));
    }
    Expression::parse(value, source_path, range.start)
        .map(EventArgument::Scope)
        .map_err(|_| {
            "Event binding arguments must be literals, $event property paths, or bounded \
             template expressions."
        })
}

/// Validates a dotted property path read off the dispatched event.
fn event_path(path: &str) -> Result<String, &'static str> {
    if path.is_empty() {
        return Ok(String::new());
    }
    let Some(path) = path.strip_prefix('.') else {
        return Err("Read a property off $event with a dot, as in $event.target.value.");
    };
    let segments: Vec<&str> = path.split('.').collect();
    if segments.len() > MAX_EVENT_PATH_SEGMENTS {
        return Err("$event property path is limited to four segments.");
    }
    for segment in &segments {
        if segment.is_empty()
            || segment.len() > 64
            || !segment
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            || !segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err("$event property path segments must be plain identifiers.");
        }
    }
    Ok(String::from(path))
}

/// Validates one literal event-binding argument, in JSON form.
fn event_literal(value: &str) -> Result<String, &'static str> {
    if value == "true" || value == "false" || value == "null" {
        return Ok(String::from(value));
    }
    if value.parse::<f64>().is_ok_and(f64::is_finite) {
        return Ok(String::from(value));
    }
    let quoted = value
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''));
    let quoted = quoted.or_else(|| {
        value
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
    });
    let Some(text) = quoted else {
        return Err(
            "Event binding arguments must be literals or $event property paths, as in \
             $event.target.value.",
        );
    };
    if text.contains(['\'', '"', '\\', ',', '(', ')']) || text.chars().any(char::is_control) {
        return Err("Event binding string arguments must be simple literals.");
    }
    Ok(format!("\"{text}\""))
}

fn template_diagnostic(
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{SourceRange, TemplateFrontend};
    use std::collections::BTreeSet;

    #[test]
    fn controls_components_and_interpolation_lower_to_view_ir() {
        let components = BTreeSet::from([String::from("product-card")]);
        let program = TemplateFrontend::compile(
            r#"<logic :if="show"><product-card :item="item">{item.name}</product-card></logic><logic else>none</logic>"#,
            "client/pages/tac.html",
            &components,
        )
        .unwrap_or_else(|_| unreachable!());
        let json = serde_json::to_value(program.view_ir()).unwrap_or_else(|_| unreachable!());
        assert_eq!(json["root"]["kind"], "conditional");
        let text = serde_json::to_string(&json).unwrap_or_else(|_| unreachable!());
        assert!(text.contains("\"kind\":\"conditional\""));
        assert!(text.contains("\"kind\":\"component\""));
        assert!(!text.contains("\"tag\":\"logic\""));
    }

    #[test]
    fn parser_recovers_multiple_structure_and_expression_failures() {
        let error = TemplateFrontend::compile(
            // A call parses now and is refused at render, so an expression
            // that is still a parse error is needed here.
            "<logic else>x</logic><p>{bad syntax}</p><div><span></div>",
            "client/pages/tac.html",
            &BTreeSet::new(),
        )
        .expect_err("invalid template");
        assert!(error.diagnostics().len() >= 3);
    }

    /// `<slot>` holds no children, but malformed token recovery can leave one
    /// open on the stack. Appending to it used to panic. Regression for the
    /// `template_frontend` fuzz artifact
    /// `crash-e40bfb3435ab0d4c4aaaa29c22cfa5dfcda5861c`.
    #[test]
    fn content_after_an_open_slot_fails_closed_instead_of_panicking() {
        let empty = BTreeSet::new();
        for source in [
            "<main><slot>text</slot></main>",
            "<slot><p>child</p></slot>",
            "<main><slot>{ text <pr<slot><p>child</p></slot>\n",
            "main><slot>{\u{d}\u{d}\u{d}\u{d}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}-\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{d}\u{d}<pr<main><slot>{\u{d}\u{d}\u{d}\u{d}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}-\u{11}\u{11}\u{11}\u{11}\u{11}\u{11}\u{d}\u{d}<pr\u{d}",
        ] {
            let error = TemplateFrontend::compile(source, "client/pages/tac.html", &empty)
                .expect_err(source);
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|value| value.code.number() == 1301),
                "{source}"
            );
        }
    }

    #[test]
    fn malformed_controls_attributes_and_sources_fail_closed() {
        let empty = BTreeSet::new();
        for source in [
            "",
            "<main>\0</main>",
            "<main>",
            "</main>",
            "<main onclick=\"bad()\">x</main>",
            "<main :hydrate=\"policy\">x</main>",
            "<main :=\"value\">x</main>",
            "<main :title=\"bad syntax\">x</main>",
            "<script>bad()</script>",
            "<web-widget hydrate=\"load\"></web-widget>",
            "<unknown>x</unknown>",
            "<logic>x</logic>",
            "<loop>x</loop>",
            "<for>x</for>",
            "<loop for=\"item of values\">x</loop>",
            "<loop :for=\"1item of values\">x</loop>",
            "<loop :for=\"item values\">x</loop>",
            "<slot name=\"named\"></slot>",
            "<p>{missing</p>",
            "<p>{!raw}</p>",
        ] {
            assert!(
                TemplateFrontend::compile(source, "client/pages/tac.html", &empty).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn every_control_form_comment_style_and_hydration_policy_parses() {
        let components = BTreeSet::from([String::from("demo-card")]);
        let source = r#"
<style>:root { color: red; }</style>
<!-- retained source comment -->
<p>\{literal\}</p>
<logic :if="false">a</logic>
<logic :else-if="true">b</logic>
<logic else>c</logic>
<if :when="false">d</if>
<else>e</else>
<loop :for="item of items">{item}</loop>
<for :each="item in items">{item}</for>
<demo-card hydrate="idle"></demo-card>
<demo-card hydrate="visible"></demo-card>
<demo-card hydrate="interaction"></demo-card>
<demo-card hydrate="never"></demo-card>
"#;
        let program = TemplateFrontend::compile(source, "client/pages/tac.html", &components)
            .unwrap_or_else(|_| unreachable!());
        assert!(!program.nodes.is_empty());
        for policy in ["", "later", "LOAD"] {
            let source = format!("<demo-card hydrate=\"{policy}\"></demo-card>");
            assert!(
                TemplateFrontend::compile(&source, "client/pages/tac.html", &components).is_err()
            );
        }
    }

    #[test]
    fn event_bindings_parse_into_markers_and_structured_payloads() {
        use super::{EventArgument, event_binding};

        const RANGE: SourceRange = SourceRange { start: 0, end: 0 };

        let bind =
            |event: &str, value: &str| event_binding(event, value, "client/pages/tac.html", RANGE);
        let parts = |event: &str, value: &str| {
            let (marker, binding) = bind(event, value).expect(value);
            (marker, binding.handler, binding.arguments)
        };

        assert_eq!(
            parts("click", "increment()"),
            (
                String::from("data-tac-on-click"),
                String::from("increment"),
                vec![]
            )
        );
        assert_eq!(
            parts("click", "addBy(5)"),
            (
                String::from("data-tac-on-click"),
                String::from("addBy"),
                vec![EventArgument::Literal(String::from("5"))]
            )
        );
        // A bare handler name binds with no arguments.
        assert_eq!(parts("input", "rename").2, vec![]);
        // A namespaced component event encodes its colon so the marker stays a
        // valid attribute name.
        assert_eq!(
            parts("update:selected", "choose()").0,
            String::from("data-tac-on-update__selected")
        );
        assert_eq!(
            parts("click", "setMode('dark', true, 2, null)").2,
            vec![
                EventArgument::Literal(String::from("\"dark\"")),
                EventArgument::Literal(String::from("true")),
                EventArgument::Literal(String::from("2")),
                EventArgument::Literal(String::from("null")),
            ]
        );
    }

    #[test]
    fn event_bindings_read_the_dispatched_event_and_the_render_scope() {
        use super::{EventArgument, event_binding};

        const RANGE: SourceRange = SourceRange { start: 0, end: 0 };

        let bind =
            |event: &str, value: &str| event_binding(event, value, "client/pages/tac.html", RANGE);
        let arguments = |value: &str| bind("input", value).expect(value).1.arguments;

        // `$event` is the name every template framework converged on, so a
        // binding reads the event and its properties the way an author expects.
        assert_eq!(
            arguments("go($event)"),
            vec![EventArgument::EventPath(String::new())]
        );
        assert_eq!(
            arguments("setName($event.target.value)"),
            vec![EventArgument::EventPath(String::from("target.value"))]
        );
        assert_eq!(
            arguments("field('email', $event.target.value)"),
            vec![
                EventArgument::Literal(String::from("\"email\"")),
                EventArgument::EventPath(String::from("target.value")),
            ]
        );

        // A scope path is an expression resolved where the binding renders, so
        // a loop can pass the item it is iterating.
        assert!(matches!(
            arguments("select(tab.id)").as_slice(),
            [EventArgument::Scope(_)]
        ));

        // A path must be plain identifiers and bounded in depth, so a binding
        // cannot reach arbitrarily into the event.
        assert!(bind("input", "go($event.a.b.c.d.e)").is_err());
        assert!(bind("input", "go($event.)").is_err());

        // Assignment parses into a target and an operator. Whether it is
        // allowed depends on having a companion instance, which only the
        // renderer knows, so that is checked there rather than here.
        let (_, assignment) = bind("input", "query = $event.target.value").expect("assignment");
        let target = assignment.assign.expect("assignment");
        assert_eq!(target.target, "query");
        assert_eq!(target.operator, "=");
        assert_eq!(
            assignment.arguments,
            vec![EventArgument::EventPath(String::from("target.value"))]
        );

        let (_, compound) = bind("click", "$clicks += 1").expect("compound assignment");
        let target = compound.assign.expect("assignment");
        assert_eq!(target.target, "$clicks");
        assert_eq!(target.operator, "+=");

        // A comparison shares the '=' character and is not an assignment.
        assert!(
            bind("click", "go(a == b)")
                .expect("comparison")
                .1
                .assign
                .is_none()
        );
    }

    #[test]
    fn event_bindings_never_admit_executable_code() {
        use super::event_binding;

        const RANGE: SourceRange = SourceRange { start: 0, end: 0 };
        let bind = |value: &str| event_binding("click", value, "client/pages/tac.html", RANGE);

        // Anything that is not a handler name plus bounded arguments must be
        // refused, so a view cannot introduce executable code.
        for value in [
            "fetch('/x').then(r=>r.json())",
            "increment(); steal()",
            "a.b()",
            "() => 1",
            "increment(",
            "new Function('x')()",
            "",
        ] {
            assert!(bind(value).is_err(), "{value} was accepted");
        }
        assert!(event_binding("", "increment()", "client/pages/tac.html", RANGE).is_err());

        // These two are accepted now that an argument may be a template
        // expression, and both are harmless: an argument is evaluated against
        // the render scope, never as JavaScript. `document` is not in scope, so
        // the build fails rather than reading a cookie, and a constant folds to
        // a string. Neither reaches the browser as source.
        for value in [
            "increment(unquoted)",
            "increment('a' + 'b')",
            "alert(document.cookie)",
        ] {
            let (_, binding) = bind(value).expect(value);
            assert!(
                binding
                    .arguments
                    .iter()
                    .all(|argument| !matches!(argument, super::EventArgument::EventPath(_))),
                "{value}"
            );
        }
    }
}
