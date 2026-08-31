mod client;
mod component;
mod context;
mod expression;
mod frontend;
mod model;

pub(crate) use client::{ClientViewRenderer, TAC_CLIENT_RUNTIME};
pub(crate) use component::{ComponentDefinition, ComponentRegistry, SCOPE_ATTRIBUTE};
pub(crate) use context::client_route_context;
pub(crate) use expression::Scope;
pub(crate) use frontend::TemplateFrontend;
pub(crate) use model::{
    AttributeValue, HydrationPolicy, TemplateAttribute, TemplateNode, TemplateNodeKind,
    TemplateProgram, TextPart, is_trivia,
};
