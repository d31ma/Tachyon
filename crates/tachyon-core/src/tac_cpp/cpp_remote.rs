use super::{CppAttribute, CppClass, CppFailure, CppMember, CppMemberKind, CppParameter};
use crate::Failure;
use std::path::Path;

pub(crate) struct CppRemote<'a> {
    class: &'a CppClass,
    base: String,
    adapter: String,
}

impl<'a> CppRemote<'a> {
    const VERBS: [(&'static str, &'static str); 5] = [
        ("GetRequest", "Get"),
        ("PostRequest", "Post"),
        ("PutRequest", "Put"),
        ("PatchRequest", "Patch"),
        ("DeleteRequest", "Delete"),
    ];
    const TASK_PREFIXES: [&'static str; 2] = ["tachyon::Task<", "Task<"];
    const SEGMENT: &'static str = "tachyon::http::TacBrowserRequest::segment";
    const ONE_REQUEST: &'static str = "Each method of a C++ [[HTTPClient]] class has one request marker ([[GetRequest(\"/path\")]], [[PostRequest]], [[PutRequest]], [[PatchRequest]] or [[DeleteRequest]]), returns Task<Answer> and has no body: Tachyon writes it";
    const PATH_FORM: &'static str = "A C++ request path is one string that starts with /, for example [[GetRequest(\"/orders/{id}\")]]";

    pub(crate) fn check(path: &Path, class: &'a CppClass) -> Result<(), Failure> {
        Self::check_members(path, class)
    }

    fn check_members(path: &Path, class: &CppClass) -> Result<(), Failure> {
        for member in &class.members {
            Self::check_member(path, member)?;
        }
        Ok(())
    }

    pub(crate) fn definitions(class: &'a CppClass) -> String {
        let remote = Self {
            class,
            base: Self::written_path(
                class
                    .markers
                    .iter()
                    .find(|marker| marker.name == "HTTPClient"),
            ),
            adapter: crate::behavior::HttpClientSource::adapter_name(&class.name),
        };
        remote.every_definition()
    }

    fn check_member(path: &Path, member: &CppMember) -> Result<(), Failure> {
        let verbs = Self::VERBS
            .iter()
            .filter(|(marker, _)| member.marked(marker))
            .count();
        let fits = [
            member.kind == CppMemberKind::Method,
            verbs == 1,
            member.is_task(),
        ] == [true, true, true];
        fits.then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, member.name_at, Self::ONE_REQUEST))?;
        let marker = Self::request_marker(member);
        let written = marker.map_or("", |marker| marker.arguments.as_str());
        let valid = serde_json::from_str::<String>(written).is_ok_and(|path| path.starts_with('/'));
        valid
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(path, member.name_at, Self::PATH_FORM))
    }

    fn request_marker(member: &CppMember) -> Option<&CppAttribute> {
        member
            .markers
            .iter()
            .find(|marker| Self::VERBS.iter().any(|(name, _)| marker.name == *name))
    }

    fn written_path(marker: Option<&CppAttribute>) -> String {
        let arguments = marker.map_or("", |marker| marker.arguments.as_str());
        serde_json::from_str::<String>(arguments).unwrap_or_default()
    }

    fn every_definition(&self) -> String {
        let mut text = String::new();
        for member in &self.class.members {
            text.push_str(&self.definition(member));
        }
        text
    }

    fn definition(&self, member: &CppMember) -> String {
        let marker = Self::request_marker(member);
        let verb = Self::VERBS
            .iter()
            .find(|(name, _)| marker.is_some_and(|marker| marker.name == *name))
            .map_or("Get", |(_, verb)| verb);
        let path = format!("{}{}", self.base, Self::written_path(marker));
        let answer = Self::answer_type(member);
        let parameters = Self::parameter_list(&member.parameters);
        let constant = [" ", " const "][usize::from(member.is_const)];
        let target = Self::path_expression(&path);
        let parts = Self::request_parts(&member.parameters);
        let guards = super::CppChecks::guards(member);
        let adapter = Self::literal(&self.adapter);
        format!(
            "inline {} {}::{}({parameters}){constant}{{{guards}\n    tachyon::http::HttpRequestMessage request(tachyon::http::HttpMethod::{verb}, {target});\n{parts}    co_return co_await tachyon::http::TacRemoteCall::answer<{answer}>({adapter}, std::move(request));\n}}\n",
            member.type_text, self.class.name, member.name
        )
    }

    fn answer_type(member: &CppMember) -> String {
        let plain = member.plain_type();
        let inner = Self::TASK_PREFIXES
            .iter()
            .find_map(|prefix| plain.strip_prefix(prefix))
            .unwrap_or(&plain);
        inner.strip_suffix('>').unwrap_or(inner).trim().to_owned()
    }

    fn parameter_list(parameters: &[CppParameter]) -> String {
        let written: Vec<String> = parameters
            .iter()
            .map(|parameter| format!("{} {}", parameter.type_text, parameter.name))
            .collect();
        written.join(", ")
    }

    fn path_expression(path: &str) -> String {
        let mut pieces = vec![String::from("std::string()")];
        let mut rest = path;
        while let Some(open) = rest.find('{') {
            let close = rest[open..]
                .find('}')
                .map_or(rest.len(), |close| open + close);
            pieces.push(Self::literal(&rest[..open]));
            pieces.push(format!("{}({})", Self::SEGMENT, &rest[open + 1..close]));
            rest = rest.get(close + 1..).unwrap_or_default();
        }
        pieces.push(Self::literal(rest));
        pieces.join(" + ")
    }

    fn literal(text: &str) -> String {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    }

    fn request_parts(parameters: &[CppParameter]) -> String {
        let mut text = String::new();
        for parameter in parameters {
            text.push_str(&Self::request_part(parameter));
        }
        text
    }

    fn request_part(parameter: &CppParameter) -> String {
        format!(
            "{}{}{}",
            Self::body_part(parameter),
            Self::query_part(parameter),
            Self::header_part(parameter)
        )
    }

    fn marker<'p>(parameter: &'p CppParameter, wanted: &str) -> Option<&'p CppAttribute> {
        parameter
            .markers
            .iter()
            .find(|marker| marker.name == wanted)
    }

    fn body_part(parameter: &CppParameter) -> String {
        Self::marker(parameter, "RequestBody").map_or_else(String::new, |_| {
            format!(
                "    request.content = tachyon::http::JsonContent::create({});\n",
                parameter.name
            )
        })
    }

    fn query_part(parameter: &CppParameter) -> String {
        Self::marker(parameter, "RequestParam").map_or_else(String::new, |marker| {
            let key = Self::literal(&Self::named(marker, &parameter.name));
            format!(
                "    tachyon::http::TacBrowserRequest::addQuery(request, {key}, {});\n",
                parameter.name
            )
        })
    }

    fn header_part(parameter: &CppParameter) -> String {
        Self::marker(parameter, "RequestHeader").map_or_else(String::new, |marker| {
            let key = Self::literal(&Self::named(marker, &parameter.name));
            format!(
                "    request.headers.add({key}, tachyon::http::TacBrowserRequest::text({}));\n",
                parameter.name
            )
        })
    }

    fn named(marker: &CppAttribute, fallback: &str) -> String {
        serde_json::from_str::<String>(&marker.arguments).unwrap_or_else(|_| fallback.to_owned())
    }
}
