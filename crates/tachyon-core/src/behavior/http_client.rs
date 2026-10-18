use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde_json::{Map, Value, json};
use std::ops::Range;
use std::path::Path;

pub(crate) struct HttpClientSource;

impl HttpClientSource {
    const CLASS: &'static str = "httpclient";
    const STREAM: &'static str = "streamrequest";
    const SOCKET: &'static str = "socketrequest";
    const REQUESTS: [(&'static str, &'static str, &'static str); 5] = [
        ("getrequest", "GET", "GetRequest"),
        ("postrequest", "POST", "PostRequest"),
        ("putrequest", "PUT", "PutRequest"),
        ("patchrequest", "PATCH", "PatchRequest"),
        ("deleterequest", "DELETE", "DeleteRequest"),
    ];
    const PARAMETERS: [(&'static str, &'static str, &'static str); 4] = [
        ("pathvariable", "pathVariables", "PathVariable"),
        ("requestparam", "query", "RequestParam"),
        ("requestbody", "body", "RequestBody"),
        ("requestheader", "headers", "RequestHeader"),
    ];
    const CACHE_OPTION_REFUSAL: &'static str = "@HTTPClient no longer takes \"cache\": its IndexedDB fetch cache was removed (ADR 0087). Mark the class @CacheSettings and each read method @Cached, like Spring's @Cacheable. For reads that work offline, add a rule for the path to tac.cache in client/manifest.json";
    const OTHER_ROLES: [&'static str; 9] = [
        "service",
        "store",
        "helper",
        "converter",
        "setup",
        "entity",
        "client",
        "controller",
        "requestroute",
    ];
    const ROLE_SUFFIXES: [&'static str; 4] = ["Client", "Service", "Store", "Helper"];

    pub(crate) fn is_name(name: &str) -> bool {
        let name = Self::normalized(name);
        name == Self::CLASS
            || name == Self::STREAM
            || name == Self::SOCKET
            || Self::request(&name).is_some()
    }

    pub(crate) fn class_path(
        path: &Path,
        source: &str,
        class: &str,
    ) -> Result<Option<String>, Failure> {
        Ok(Self::classes_in(path, source)?
            .into_iter()
            .find(|client| client.name == class)
            .map(|client| client.path))
    }

    fn classes_in(path: &Path, source: &str) -> Result<Vec<ClientClass>, Failure> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let mask = crate::lexical::code_mask(extension, source);
        let code = Self::masked(source, &mask.code);
        let input = SourceText {
            path,
            source,
            code: &code,
            extension,
        };
        input.classes(&input.marker_lines())
    }

    pub(crate) fn rewrite(path: &Path, source: &str) -> Result<Option<String>, Failure> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let mask = crate::lexical::code_mask(extension, source);
        let code = Self::masked(source, &mask.code);
        let input = SourceText {
            path,
            source,
            code: &code,
            extension,
        };
        let lines = input.marker_lines();
        if !lines.iter().any(|line| Self::is_name(&line.name)) {
            return Ok(None);
        }
        let classes = input.classes(&lines)?;
        let mut edits = Vec::new();
        let mut streaming = Vec::new();
        for line in &lines {
            let normalized = Self::normalized(&line.name);
            if normalized == Self::STREAM || normalized == Self::SOCKET {
                streaming.push(input.stream(line, &classes)?);
                edits.push((line.span.clone(), String::new()));
            }
        }
        for class in &classes {
            let streams = streaming.contains(&class.name);
            edits.push((class.marker.clone(), class.replacement(extension, streams)));
        }
        for line in &lines {
            let Some((verb, written)) = Self::request(&Self::normalized(&line.name)) else {
                continue;
            };
            let request = input.request(line, verb, written, &classes)?;
            edits.push((line.span.clone(), request.marker));
            edits.extend(request.blanks);
        }
        Ok(Some(Self::apply(source, edits)))
    }

    pub(crate) fn without_clients(path: &Path, source: &str) -> Result<Option<String>, Failure> {
        let edits: Vec<(Range<usize>, String)> = Self::classes_in(path, source)?
            .iter()
            .map(|class| {
                let end = (class.body.end + 1).min(source.len());
                (class.marker.start..end, String::new())
            })
            .collect();
        Ok((!edits.is_empty()).then(|| Self::apply(source, edits)))
    }

    fn apply(source: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
        edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let mut output = source.to_owned();
        for (range, replacement) in edits {
            output.replace_range(range, &replacement);
        }
        output
    }

    pub(crate) fn masked(source: &str, visible: &[bool]) -> String {
        let mut bytes = source.as_bytes().to_vec();
        for (byte, shown) in bytes.iter_mut().zip(visible) {
            if !shown && !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
        String::from_utf8(bytes).unwrap_or_default()
    }

    pub(crate) fn normalized(name: &str) -> String {
        name.rsplit("::")
            .next()
            .unwrap_or(name)
            .replace('_', "")
            .to_ascii_lowercase()
    }

    fn request(name: &str) -> Option<(&'static str, &'static str)> {
        for (marker, verb, written) in Self::REQUESTS {
            if marker == name {
                return Some((verb, written));
            }
        }
        None
    }

    fn parameter(name: &str) -> Option<(&'static str, &'static str)> {
        let name = Self::normalized(name);
        for (marker, key, written) in Self::PARAMETERS {
            if marker == name {
                return Some((key, written));
            }
        }
        None
    }

    fn prefix(extension: &str) -> &'static str {
        match extension {
            "rs" | "php" | "rb" => "#[",
            "cs" => "[",
            _ => "@",
        }
    }

    fn marker_text(extension: &str, name: &str, arguments: &str) -> String {
        let written = if extension == "py" {
            crate::behavior::RoutingRename::snake_case(name)
        } else {
            name.to_owned()
        };
        let call = if arguments.is_empty() {
            written
        } else {
            format!("{written}({arguments})")
        };
        match Self::prefix(extension) {
            "@" => format!("@{call}"),
            opening => format!("{opening}{call}]"),
        }
    }

    pub(crate) fn adapter_name(class: &str) -> String {
        let mut stem = class;
        for suffix in Self::ROLE_SUFFIXES {
            if let Some(rest) = class.strip_suffix(suffix)
                && !rest.is_empty()
            {
                stem = rest;
                break;
            }
        }
        let mut characters = stem.chars();
        let first = characters.next().unwrap_or_default().to_ascii_lowercase();
        format!("{first}{}", characters.as_str())
    }
}

struct MarkerLine {
    name: String,
    arguments: String,
    span: Range<usize>,
    next_code: usize,
}

struct ClientClass {
    name: String,
    marker: Range<usize>,
    body: Range<usize>,
    path: String,
    adapter: String,
    has_other_role: bool,
}

impl ClientClass {
    fn replacement(&self, extension: &str, streams: bool) -> String {
        if self.has_other_role {
            return String::new();
        }
        let mut options = Map::new();
        if streams {
            options.insert(String::from("streams"), json!(true));
        }
        let arguments = if options.is_empty() {
            String::new()
        } else {
            Value::Object(options).to_string()
        };
        HttpClientSource::marker_text(extension, "Client", &arguments)
    }
}

struct RequestEdit {
    marker: String,
    blanks: Vec<(Range<usize>, String)>,
}

struct SourceText<'a> {
    path: &'a Path,
    source: &'a str,
    code: &'a str,
    extension: &'a str,
}

impl SourceText<'_> {
    fn failure(&self, at: usize, message: impl Into<String>) -> Failure {
        let portable = self.path.to_string_lossy().replace('\\', "/");
        Failure::one(diagnostic(
            1308,
            message,
            Some(String::from(
                "@HTTPClient(\"/api/orders\") on a class says where its requests go. @GetRequest, @PostRequest, @PutRequest, @PatchRequest and @DeleteRequest on a method send that request. See docs/BEHAVIOR_ANNOTATIONS.md.",
            )),
            source_span(&portable, at, (at + 1).min(self.source.len())),
        ))
    }

    fn marker_lines(&self) -> Vec<MarkerLine> {
        let prefix = HttpClientSource::prefix(self.extension);
        let mut lines = Vec::new();
        let mut offset = 0;
        for line in self.code.split_inclusive('\n') {
            let start = offset + line.len() - line.trim_start().len();
            let end = offset + line.trim_end().len();
            offset += line.len();
            let text = &self.code[start..end.max(start)];
            let Some(rest) = text.strip_prefix(prefix) else {
                continue;
            };
            let length = rest
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
                .count();
            if length == 0 {
                continue;
            }
            let name = rest[..length].to_owned();
            let arguments = self.arguments(start + prefix.len() + length, end);
            lines.push(MarkerLine {
                name,
                arguments,
                span: start..end,
                next_code: offset,
            });
        }
        lines
    }

    fn arguments(&self, after_name: usize, end: usize) -> String {
        let tail = &self.source[after_name..end];
        let Some(open) = tail.find('(') else {
            return String::new();
        };
        let Some(close) = tail.rfind(')') else {
            return String::new();
        };
        if close <= open {
            return String::new();
        }
        tail[open + 1..close].trim().to_owned()
    }

    fn declaration_after(&self, from: usize) -> Option<(usize, &str)> {
        let prefix = HttpClientSource::prefix(self.extension);
        let mut offset = from;
        for line in self.code[from..].split_inclusive('\n') {
            let trimmed = line.trim();
            let start = offset + line.len() - line.trim_start().len();
            offset += line.len();
            if trimmed.is_empty() || trimmed.starts_with(prefix) {
                continue;
            }
            return Some((start, trimmed));
        }
        None
    }

    fn classes(&self, lines: &[MarkerLine]) -> Result<Vec<ClientClass>, Failure> {
        let mut classes = Vec::new();
        for line in lines {
            if HttpClientSource::normalized(&line.name) != HttpClientSource::CLASS {
                continue;
            }
            classes.push(self.class(line, lines)?);
        }
        Ok(classes)
    }

    fn class(&self, line: &MarkerLine, lines: &[MarkerLine]) -> Result<ClientClass, Failure> {
        let Some((start, declaration)) = self.declaration_after(line.next_code) else {
            return Err(self.failure(line.span.start, "@HTTPClient must be followed by a class"));
        };
        let names =
            crate::annotation_layout::declaration_names(self.path, declaration).unwrap_or_default();
        let Some(name) = names.first().cloned() else {
            return Err(self.failure(
                line.span.start,
                "@HTTPClient marks a class: it says where the class's requests go. Put it directly above a class",
            ));
        };
        let (path, adapter) = self.class_arguments(line, &name)?;
        let has_other_role = self.block_roles(line, lines, start);
        Ok(ClientClass {
            name,
            marker: line.span.clone(),
            body: self.class_body(start),
            path,
            adapter,
            has_other_role,
        })
    }

    fn block_roles(&self, line: &MarkerLine, lines: &[MarkerLine], declaration: usize) -> bool {
        let mut block_start = line.span.start;
        for other in lines.iter().rev() {
            if other.span.start >= block_start {
                continue;
            }
            let between = &self.code[other.span.end..block_start];
            if !between.trim().is_empty() {
                break;
            }
            block_start = other.span.start;
        }
        for other in lines {
            if other.span.start < block_start || other.span.start >= declaration {
                continue;
            }
            let name = HttpClientSource::normalized(&other.name);
            if HttpClientSource::OTHER_ROLES.contains(&name.as_str()) {
                return true;
            }
        }
        false
    }

    fn class_arguments(&self, line: &MarkerLine, class: &str) -> Result<(String, String), Failure> {
        let text = line.arguments.trim();
        let default_adapter = HttpClientSource::adapter_name(class);
        if text.is_empty() {
            return Ok((String::new(), default_adapter));
        }
        if let Some(path) = Self::quoted(text) {
            return Ok((path.to_owned(), default_adapter));
        }
        let Ok(Value::Object(object)) = serde_json::from_str::<Value>(text) else {
            return Err(self.failure(
                line.span.start,
                "@HTTPClient takes a path in quotes, like @HTTPClient(\"/api/orders\"), or an object such as {\"path\": \"/api/orders\", \"adapter\": \"orders\"}",
            ));
        };
        if object.contains_key("cache") {
            return Err(self.failure(line.span.start, HttpClientSource::CACHE_OPTION_REFUSAL));
        }
        let unknown = object
            .keys()
            .any(|key| !matches!(key.as_str(), "path" | "adapter" | "setup"));
        let path = object.get("path").and_then(Value::as_str).unwrap_or("");
        let adapter = object
            .get("adapter")
            .and_then(Value::as_str)
            .map_or(default_adapter, str::to_owned);
        if unknown {
            return Err(self.failure(
                line.span.start,
                "@HTTPClient accepts only \"path\" (where requests go), \"adapter\" (the name of the HTTP adapter that sends them) and \"setup\" (the Setup whose adapters send them)",
            ));
        }
        Ok((path.to_owned(), adapter))
    }

    fn quoted(text: &str) -> Option<&str> {
        let quoted = text.len() >= 2
            && ((text.starts_with('"') && text.ends_with('"'))
                || (text.starts_with('\'') && text.ends_with('\'')));
        quoted.then(|| &text[1..text.len() - 1])
    }

    fn class_body(&self, declaration: usize) -> Range<usize> {
        if matches!(self.extension, "py" | "rb") {
            return self.python_body(declaration);
        }
        if self.extension == "go" {
            return declaration..declaration;
        }
        let Some(open) = self.code[declaration..]
            .find('{')
            .map(|at| declaration + at)
        else {
            return declaration..declaration;
        };
        let mut depth = 0_i32;
        for (offset, byte) in self.code.bytes().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return open..offset;
                    }
                }
                _ => {}
            }
        }
        open..self.code.len()
    }

    fn python_body(&self, declaration: usize) -> Range<usize> {
        let line_start = self.code[..declaration].rfind('\n').map_or(0, |at| at + 1);
        let indent = declaration - line_start;
        let Some(first_line) = self.code[declaration..].find('\n') else {
            return declaration..self.code.len();
        };
        let mut offset = declaration + first_line + 1;
        for line in self.code[offset..].split_inclusive('\n') {
            let blank = line.trim().is_empty();
            if !blank && line.len() - line.trim_start().len() <= indent {
                return declaration..offset;
            }
            offset += line.len();
        }
        declaration..self.code.len()
    }

    fn owner<'c>(&self, method: usize, classes: &'c [ClientClass]) -> Option<&'c ClientClass> {
        if self.extension == "go" {
            let receiver = crate::go_source::method_at(self.code, method)?.receiver_type;
            return classes.iter().find(|class| class.name == receiver);
        }
        if self.extension == "rs" {
            return classes
                .iter()
                .find(|class| self.rust_impl_contains(&class.name, method));
        }
        classes.iter().find(|class| class.body.contains(&method))
    }

    fn rust_impl_contains(&self, name: &str, method: usize) -> bool {
        let mut offset = 0;
        for line in self.code.split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            let mut words = line.split_whitespace();
            let is_impl = words.next() == Some("impl")
                && words
                    .next()
                    .is_some_and(|word| word.trim_end_matches('{') == name)
                && !line.contains(" for ");
            if is_impl && self.class_body(start).contains(&method) {
                return true;
            }
        }
        false
    }

    fn stream(&self, line: &MarkerLine, classes: &[ClientClass]) -> Result<String, Failure> {
        let Some((method, _)) = self.declaration_after(line.next_code) else {
            return Err(self.failure(
                line.span.start,
                format!("@{} must be followed by a method", line.name),
            ));
        };
        let Some(class) = self.owner(method, classes) else {
            return Err(self.failure(
                line.span.start,
                format!(
                    "@{} keeps a connection open, so its class needs @HTTPClient(\"/path\") to say where it is",
                    line.name
                ),
            ));
        };
        Ok(class.name.clone())
    }

    fn request(
        &self,
        line: &MarkerLine,
        verb: &str,
        written: &str,
        classes: &[ClientClass],
    ) -> Result<RequestEdit, Failure> {
        let Some((method, _)) = self.declaration_after(line.next_code) else {
            return Err(self.failure(
                line.span.start,
                format!("@{written} must be followed by a method"),
            ));
        };
        let Some(class) = self.owner(method, classes) else {
            return Err(self.failure(
                line.span.start,
                format!(
                    "@{written} means calling this method sends a {verb} request, so its class needs @HTTPClient(\"/path\") to say where requests go"
                ),
            ));
        };
        let path = self.request_path(line, written, &class.path)?;
        let mut policy = Map::new();
        policy.insert(String::from("client"), json!(class.adapter));
        policy.insert(String::from("method"), json!(verb));
        policy.insert(String::from("path"), json!(path));
        let blanks = self.request_parameters(method, line, &mut policy)?;
        let marker = HttpClientSource::marker_text(
            self.extension,
            "WebCall",
            &Value::Object(policy).to_string(),
        );
        Ok(RequestEdit { marker, blanks })
    }

    fn request_path(
        &self,
        line: &MarkerLine,
        written: &str,
        base: &str,
    ) -> Result<String, Failure> {
        let text = line.arguments.trim();
        if text.is_empty() {
            return Ok(if base.is_empty() {
                String::from("/")
            } else {
                base.to_owned()
            });
        }
        let Some(path) = Self::quoted(text) else {
            return Err(self.failure(
                line.span.start,
                format!(
                    "@{written} takes an optional path in quotes, like @{written}(\"/{{id}}\")"
                ),
            ));
        };
        Ok(format!("{base}{path}"))
    }

    fn request_parameters(
        &self,
        method: usize,
        line: &MarkerLine,
        policy: &mut Map<String, Value>,
    ) -> Result<Vec<(Range<usize>, String)>, Failure> {
        let Some(list) = self.parameter_list(method) else {
            return Err(self.failure(line.span.start, "A request method needs a parameter list"));
        };
        let mut blanks = Vec::new();
        let mut index = 0_u64;
        for (piece_start, piece) in Self::pieces(&self.code[list.clone()], list.start) {
            if index == 0 && self.is_receiver(piece.trim()) {
                continue;
            }
            self.parameter(piece_start, &piece, index, policy, &mut blanks)?;
            index += 1;
        }
        Ok(blanks)
    }

    fn is_receiver(&self, piece: &str) -> bool {
        match self.extension {
            "py" => piece == "self",
            "rs" => matches!(piece, "&self" | "&mut self" | "self"),
            _ => false,
        }
    }

    fn parameter(
        &self,
        start: usize,
        piece: &str,
        index: u64,
        policy: &mut Map<String, Value>,
        blanks: &mut Vec<(Range<usize>, String)>,
    ) -> Result<(), Failure> {
        let leading = piece.len() - piece.trim_start().len();
        let at = start + leading;
        let trimmed = piece.trim();
        if self.extension == "py"
            && let Some(annotated) = crate::python_parameters::PythonAnnotated::read(
                trimmed,
                &self.source[at..at + trimmed.len()],
                at,
                |name| HttpClientSource::parameter(name).is_some(),
            )
            && let Some((key, written)) = HttpClientSource::parameter(&annotated.marker)
        {
            for range in annotated.erased {
                blanks.push((range.clone(), " ".repeat(range.len())));
            }
            let wire = match &annotated.argument {
                Some(argument) => Self::quoted(argument).map(str::to_owned).ok_or_else(|| {
                    self.failure(
                        at,
                        format!(
                            "{} takes one name in quotes, like {}(\"page\")",
                            annotated.marker, annotated.marker
                        ),
                    )
                })?,
                None => annotated.name,
            };
            return self.record(at, key, written, wire, index, policy);
        }
        let prefix = HttpClientSource::prefix(self.extension);
        let Some(rest) = trimmed.strip_prefix(prefix) else {
            return Ok(());
        };
        let length = rest
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':'))
            .count();
        let Some((key, written)) = HttpClientSource::parameter(&rest[..length]) else {
            return self.refuse_route_marker(at, &rest[..length]);
        };
        if self.extension == "py" {
            return Err(self.failure(
                at,
                crate::annotation_rules::AnnotationRules::python_parameter(written),
            ));
        }
        let mut end = prefix.len() + length;
        let mut name = None;
        if trimmed[end..].starts_with('(') {
            let Some(close) = trimmed[end..].find(')') else {
                return Err(self.failure(at, format!("Close the arguments of @{written}")));
            };
            let original = &self.source[at + end + 1..at + end + close];
            name = Some(
                Self::quoted(original.trim())
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        self.failure(
                            at,
                            format!(
                                "@{written} takes one name in quotes, like @{written}(\"page\")"
                            ),
                        )
                    })?,
            );
            end += close + 1;
        }
        if prefix != "@" {
            if !trimmed[end..].starts_with(']') {
                return Err(self.failure(at, format!("Close @{written} with ']'")));
            }
            end += 1;
        }
        blanks.push((at..at + end, " ".repeat(end)));
        let wire = match name {
            Some(name) => name,
            None if key == "body" => String::new(),
            None => self.code_name(trimmed[end..].trim()).ok_or_else(|| {
                self.failure(at, format!("Cannot read the parameter after @{written}"))
            })?,
        };
        self.record(at, key, written, wire, index, policy)
    }

    fn record(
        &self,
        at: usize,
        key: &str,
        written: &str,
        wire: String,
        index: u64,
        policy: &mut Map<String, Value>,
    ) -> Result<(), Failure> {
        if key == "body" {
            if policy.contains_key("body") {
                return Err(self.failure(
                    at,
                    format!("A request has one body, so only one parameter may use @{written}"),
                ));
            }
            policy.insert(String::from("body"), json!(index));
            return Ok(());
        }
        let entry = policy
            .entry(String::from(key))
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(map) = entry {
            map.insert(wire, json!(index));
        }
        Ok(())
    }

    fn refuse_route_marker(&self, at: usize, name: &str) -> Result<(), Failure> {
        let normalized = HttpClientSource::normalized(name);
        let (instead, meaning) = match normalized.as_str() {
            "frompath" => (
                "PathVariable",
                "the value fills a part of the request path, like {id}",
            ),
            "fromquery" => ("RequestParam", "the value is sent in the query string"),
            "frombody" => ("RequestBody", "the value is sent as the request body"),
            "fromheader" => ("RequestHeader", "the value is sent as a request header"),
            _ => return Ok(()),
        };
        Err(self.failure(
            at,
            format!(
                "In a request method, @{name} is written @{instead}: it means {meaning}. @{name} belongs on controller routes"
            ),
        ))
    }

    fn code_name(&self, declaration: &str) -> Option<String> {
        let declaration = declaration.split('=').next().unwrap_or("").trim();
        let name = match self.extension {
            "ts" | "py" | "kt" | "rs" | "swift" => {
                let before = declaration.split(':').next().unwrap_or("").trim();
                before.rsplit(char::is_whitespace).next().unwrap_or(before)
            }
            "go" => declaration.split_whitespace().next().unwrap_or(""),
            "php" => declaration.rsplit('$').next().unwrap_or("").trim(),
            "js" => declaration,
            _ => declaration.rsplit(char::is_whitespace).next().unwrap_or(""),
        };
        let name = name.trim_start_matches("mut ").trim_end_matches('?');
        let valid = !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        valid.then(|| name.to_owned())
    }

    fn parameter_list(&self, method: usize) -> Option<Range<usize>> {
        let search = match self.extension {
            "rs" => method + self.code[method..].find("fn ")? + 3,
            "go" => {
                let receiver = method + self.code[method..].find('(')?;
                Self::closing(self.code, receiver)? + 1
            }
            _ => method,
        };
        let open = search + self.code[search..].find('(')?;
        let close = Self::closing(self.code, open)?;
        Some(open + 1..close)
    }

    fn closing(code: &str, open: usize) -> Option<usize> {
        let mut depth = 0_i32;
        for (offset, byte) in code.bytes().enumerate().skip(open) {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(offset);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn pieces(text: &str, start: usize) -> Vec<(usize, String)> {
        let mut pieces = Vec::new();
        let mut depth = 0_i32;
        let mut piece_start = 0;
        for (offset, byte) in text.bytes().enumerate() {
            match byte {
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b')' | b']' | b'}' | b'>' => depth -= 1,
                b',' if depth == 0 => {
                    pieces.push((start + piece_start, text[piece_start..offset].to_owned()));
                    piece_start = offset + 1;
                }
                _ => {}
            }
        }
        if !text[piece_start..].trim().is_empty() {
            pieces.push((start + piece_start, text[piece_start..].to_owned()));
        }
        pieces
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::HttpClientSource;
    use std::path::Path;

    struct Sample;

    impl Sample {
        const CLIENTS: [(&'static str, &'static str); 12] = [
            (
                "js",
                "@HTTPClient(\"/api/orders\")\nexport default class OrdersClient {\n  @GetRequest(\"/{id}\")\n  async getOrder(@PathVariable id, @RequestParam(\"view\") view) {}\n}\n",
            ),
            (
                "ts",
                "@HTTPClient(\"/api/orders\")\nexport default class OrdersClient {\n  @GetRequest(\"/{id}\")\n  async getOrder(@PathVariable id: string, @RequestParam(\"view\") view: string): Promise<unknown> {}\n}\n",
            ),
            (
                "py",
                "from typing import Annotated\n\n@http_client(\"/api/orders\")\nclass OrdersClient:\n    @get_request(\"/{id}\")\n    async def get_order(self, id: Annotated[str, path_variable], view: Annotated[str, request_param(\"view\")]):\n        pass\n",
            ),
            (
                "rb",
                "#[HTTPClient(\"/api/orders\")]\nclass OrdersClient\n  #[GetRequest(\"/{id}\")]\n  def get_order(\n    #[PathVariable]\n    id,\n    #[RequestParam(\"view\")]\n    view\n  )\n  end\nend\n",
            ),
            (
                "php",
                "<?php\n#[HTTPClient(\"/api/orders\")]\nclass OrdersClient {\n    #[GetRequest(\"/{id}\")]\n    public function getOrder(#[PathVariable] string $id, #[RequestParam(\"view\")] string $view) {}\n}\n",
            ),
            (
                "java",
                "@HTTPClient(\"/api/orders\")\nclass OrdersClient {\n    @GetRequest(\"/{id}\")\n    Object getOrder(@PathVariable String id, @RequestParam(\"view\") String view) {}\n}\n",
            ),
            (
                "kt",
                "@HTTPClient(\"/api/orders\")\nclass OrdersClient {\n    @GetRequest(\"/{id}\")\n    fun getOrder(@PathVariable id: String, @RequestParam(\"view\") view: String): Any? {}\n}\n",
            ),
            (
                "cs",
                "[HTTPClient(\"/api/orders\")]\nclass OrdersClient {\n    [GetRequest(\"/{id}\")]\n    public async Task<BehaviorJSON> GetOrder([PathVariable] string id, [RequestParam(\"view\")] string view) {}\n}\n",
            ),
            (
                "rs",
                "#[HTTPClient(\"/api/orders\")]\nstruct OrdersClient {}\nimpl OrdersClient {\n    #[GetRequest(\"/{id}\")]\n    fn get_order(&self, #[PathVariable] id: String, #[RequestParam(\"view\")] view: String) -> BehaviorResult<BehaviorValue> {}\n}\n",
            ),
            (
                "dart",
                "@HTTPClient(\"/api/orders\")\nclass OrdersClient {\n  @GetRequest(\"/{id}\")\n  Future<Object?> getOrder(@PathVariable String id, @RequestParam(\"view\") String view) async {}\n}\n",
            ),
            (
                "go",
                "// @HTTPClient(\"/api/orders\")\ntype OrdersClient struct {\n}\n\n// @GetRequest(\"/{id}\")\nfunc (client *OrdersClient) GetOrder(\n// @PathVariable\n id string, \n// @RequestParam(\"view\")\n view string) (BehaviorValue, error) {\n}\n",
            ),
            (
                "swift",
                "@HTTPClient(\"/api/orders\")\nclass OrdersClient {\n    @GetRequest(\"/{id}\")\n    func getOrder(@PathVariable id: String, @RequestParam(\"view\") view: String) async throws -> BehaviorJSON {}\n}\n",
            ),
        ];

        fn rewritten(extension: &str, source: &str) -> String {
            let path = format!("client/remotes/OrdersClient.{extension}");
            HttpClientSource::rewrite(Path::new(&path), source)
                .unwrap_or_else(|failure| panic!("{extension}: {failure}"))
                .expect("rewritten")
        }
    }

    #[test]
    fn every_language_turns_request_methods_into_one_exchange_policy() {
        let policy = r#"{"client":"orders","method":"GET","path":"/api/orders/{id}","pathVariables":{"id":0},"query":{"view":1}}"#;
        for (extension, source) in Sample::CLIENTS {
            let rewritten = Sample::rewritten(extension, source);
            assert!(rewritten.contains(policy), "{extension}: {rewritten}");
            for marker in [
                "HTTPClient",
                "http_client",
                "GetRequest",
                "get_request",
                "PathVariable",
                "path_variable",
                "RequestParam",
                "request_param",
            ] {
                assert!(!rewritten.contains(marker), "{extension}: {rewritten}");
            }
            let client = if extension == "py" {
                "@client"
            } else {
                "Client"
            };
            assert!(rewritten.contains(client), "{extension}: {rewritten}");
        }
    }

    #[test]
    fn a_service_keeps_its_role_and_names_its_adapter() {
        let source = "@Service\n@HTTPClient({\"path\": \"/api\", \"adapter\": \"catalog\"})\nexport default class ProductsService {\n  @PostRequest(\"/products\")\n  async postProduct(@RequestBody product, @RequestHeader(\"X-Tenant\") tenant) {}\n}\n";
        let rewritten =
            HttpClientSource::rewrite(Path::new("client/services/ProductsService.js"), source)
                .expect("rewrite")
                .expect("rewritten");
        assert!(!rewritten.contains("@Client"), "{rewritten}");
        assert!(
            rewritten.contains(r#"{"body":0,"client":"catalog","headers":{"X-Tenant":1},"method":"POST","path":"/api/products"}"#),
            "{rewritten}"
        );
    }

    #[test]
    fn a_request_method_needs_an_http_client_class() {
        let source = "export default class OrdersService {\n  @GetRequest(\"/orders\")\n  async getOrders() {}\n}\n";
        let failure =
            HttpClientSource::rewrite(Path::new("client/services/OrdersService.js"), source)
                .expect_err("no class marker")
                .to_string();
        assert!(
            failure.contains("@GetRequest means calling this method sends a GET request, so its class needs @HTTPClient"),
            "{failure}"
        );
    }

    #[test]
    fn the_cache_option_is_refused_with_cached_as_its_replacement() {
        let source = "@HTTPClient({\"path\": \"/api\", \"cache\": \"cache-first\"})\nexport default class OrdersClient {\n  @GetRequest(\"/orders\")\n  async getOrders() {}\n}\n";
        let failure =
            HttpClientSource::rewrite(Path::new("client/remotes/OrdersClient.js"), source)
                .expect_err("cache option")
                .to_string();
        for named in ["\"cache\"", "@CacheSettings", "@Cached", "tac.cache"] {
            assert!(failure.contains(named), "{failure}");
        }
    }
}
