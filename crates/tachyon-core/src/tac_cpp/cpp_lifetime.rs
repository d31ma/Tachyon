use super::{CppAttribute, CppClass, CppFailure, CppInjection, CppMember, CppMemberKind};
use crate::Failure;
use serde_json::Value;
use std::path::Path;

pub(crate) struct CppLifetime;

impl CppLifetime {
    const SINGLETON: &'static str = "singleton";
    const PROTOTYPE: &'static str = "prototype";
    const PUBLISHER: &'static str = "ApplicationEventPublisher";
    const OPTIONAL: &'static str = "std::optional<";
    const SCOPES: [&'static str; 2] = [Self::SINGLETON, Self::PROTOTYPE];
    const LIFETIME_FORM: &'static str = "[[Lifetime]] takes one JSON object whose value is \"singleton\" or \"prototype\", for example [[Lifetime({\"value\":\"prototype\"})]]";
    const APPLICATION: &'static str = "Lifetime \"application\" keeps one object for a native application, and C++ services run as WebAssembly in the page, with no native host (ADR 0099). Use \"singleton\" or \"prototype\"";
    const NAMED_FORM: &'static str = "[[Named]] takes the class name of the C++ service or [[HTTPClient]] that fills the field, for example [[Named(\"FastGreeterService\")]]";
    const NO_CONSTRUCTOR: &'static str = "A C++ [[NeedsAll]] class declares no constructor: Tachyon fills its fields. Give a field that is not a collaborator or a component property an initial value";
    const NEVER_EMPTY: &'static str = "A C++ [[NeedsAll]] field without an initial value is a component property or a collaborator, so it is never empty: write the plain type (`std::string path;`) instead of std::optional or a pointer, or give the field an initial value";

    pub(crate) fn check(path: &Path, class: &CppClass) -> Result<(), Failure> {
        Self::scope(path, class)?;
        class
            .members
            .iter()
            .try_for_each(|member| Self::check_named(path, member))?;
        Self::needs_all(class)
            .then(|| Self::check_needs_all(path, class))
            .transpose()
            .map(drop)
    }

    pub(crate) fn is_prototype(class: &CppClass) -> bool {
        let written = Self::written_scope(class);
        written.map_or_else(|| Self::needs_all(class), |scope| scope == Self::PROTOTYPE)
    }

    pub(crate) fn needs_all(class: &CppClass) -> bool {
        class.markers.iter().any(|marker| marker.name == "NeedsAll")
    }

    pub(crate) fn injections(class: &CppClass) -> Vec<CppInjection> {
        let needs_all = Self::needs_all(class);
        class
            .members
            .iter()
            .filter(|member| Self::is_injected(member, needs_all))
            .map(|member| CppInjection {
                field: member.name.clone(),
                provider: Self::provider(member),
                terminator: member.terminator,
            })
            .collect()
    }

    pub(crate) fn props(class: &CppClass) -> Vec<&CppMember> {
        let needs_all = Self::needs_all(class);
        class
            .members
            .iter()
            .filter(|member| [needs_all, Self::is_prop(member)] == [true, true])
            .collect()
    }

    fn is_injected(member: &CppMember, needs_all: bool) -> bool {
        let unfilled_reference = [
            needs_all,
            member.is_instance(CppMemberKind::Field),
            member.type_text.ends_with('&'),
            !member.has_initializer,
        ] == [true, true, true, true];
        [member.marked("Needs"), unfilled_reference].contains(&true)
    }

    fn is_prop(member: &CppMember) -> bool {
        [
            member.is_instance(CppMemberKind::Field),
            !member.has_initializer,
            !member.type_text.contains('&'),
            !member.is_const,
            member.plain_type() != Self::PUBLISHER,
            !member.marked("LocalState"),
            !member.marked("SessionState"),
        ] == [true; 7]
    }

    fn provider(member: &CppMember) -> String {
        member
            .markers
            .iter()
            .find(|marker| marker.name == "Named")
            .and_then(Self::named_class)
            .unwrap_or_else(|| member.plain_type())
    }

    fn named_class(marker: &CppAttribute) -> Option<String> {
        serde_json::from_str::<String>(&marker.arguments)
            .ok()
            .filter(|name| Self::is_class_name(name))
    }

    fn is_class_name(name: &str) -> bool {
        let mut characters = name.chars();
        let first = characters
            .next()
            .is_some_and(|first| first.is_ascii_uppercase());
        [first, characters.all(|next| next.is_ascii_alphanumeric())] == [true, true]
    }

    fn check_named(path: &Path, member: &CppMember) -> Result<(), Failure> {
        let named = member.markers.iter().find(|marker| marker.name == "Named");
        named.map_or(Ok(()), |marker| {
            Self::named_class(marker)
                .map(drop)
                .ok_or_else(|| CppFailure::unreadable(path, marker.at, Self::NAMED_FORM))
        })
    }

    fn written_scope(class: &CppClass) -> Option<String> {
        let marker = class
            .markers
            .iter()
            .find(|marker| marker.name == "Lifetime")?;
        let options: Value = serde_json::from_str(&marker.arguments).ok()?;
        options
            .as_object()
            .filter(|object| object.len() == 1)?
            .get("value")?
            .as_str()
            .map(str::to_owned)
    }

    fn scope(path: &Path, class: &CppClass) -> Result<(), Failure> {
        class
            .markers
            .iter()
            .find(|marker| marker.name == "Lifetime")
            .map_or(Ok(()), |marker| Self::scope_of(path, class, marker))
    }

    fn scope_of(path: &Path, class: &CppClass, marker: &CppAttribute) -> Result<(), Failure> {
        match Self::written_scope(class).as_deref() {
            Some(scope) if Self::SCOPES.contains(&scope) => Ok(()),
            Some("application") => Err(CppFailure::pending(path, marker.at, Self::APPLICATION)),
            _ => Err(CppFailure::unreadable(path, marker.at, Self::LIFETIME_FORM)),
        }
    }

    fn check_needs_all(path: &Path, class: &CppClass) -> Result<(), Failure> {
        class
            .members
            .iter()
            .find(|member| member.kind == CppMemberKind::Constructor)
            .map_or(Ok(()), |constructor| {
                Err(CppFailure::unreadable(
                    path,
                    constructor.name_at,
                    Self::NO_CONSTRUCTOR,
                ))
            })?;
        let empty = Self::props(class)
            .into_iter()
            .find(|member| Self::may_be_empty(member));
        empty.map_or(Ok(()), |member| {
            Err(CppFailure::coded(
                1331,
                path,
                member.name_at,
                Self::NEVER_EMPTY,
            ))
        })
    }

    fn may_be_empty(member: &CppMember) -> bool {
        [
            member.type_text.contains('*'),
            member.plain_type().starts_with(Self::OPTIONAL),
        ]
        .contains(&true)
    }
}
