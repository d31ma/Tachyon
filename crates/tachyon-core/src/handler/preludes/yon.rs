// Appended to a `yon.rs` handler. Everything below is the protocol.
//
// The author writes a struct and an impl — the same shape a service and a
// repository beside it have, and the same shape a `yon.js` and a `yon.py`
// have. Reading standard input, dispatching on the method and writing the
// response envelope is Tachyon's half of the bargain, not theirs.
//
// This is a per-language runtime, which the direct protocol deliberately had
// none of: "the legacy ships a bespoke runner per language; this defines one
// direct protocol instead". That held while Yon ran any language at all. It
// runs eight now, and eight is few enough to give each one the shape the
// adapters already gave JavaScript and Python.
//
// The JSON is written here rather than taken from a crate, because this path
// compiles one file with `rustc` and no Cargo: there is nowhere to declare a
// dependency on serde. `Json` is in scope without an import, the same way it
// is in Java and Kotlin, so a handler builds a response out of values rather
// than out of a string it escaped by hand.

/// A JSON value.
///
/// Rust has no JSON in its standard library, and this path compiles one file
/// with `rustc` and no Cargo — there is nowhere to declare a dependency on
/// serde. So one is supplied here rather than asked for, the same way Java and
/// Kotlin get theirs, and it is in scope without an import.
///
/// A map keeps insertion order, because a response whose keys move between
/// requests is a diff nobody wanted to read.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Json>),
    Map(Vec<(String, Json)>),
}

impl Json {
    /// A JSON object, from anything that yields key-value pairs.
    ///
    /// ```ignore
    /// Json::map([("products", Json::list(["anvil"]))])
    /// ```
    #[must_use]
    pub fn map<K: Into<String>, V: Into<Self>>(
        entries: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        Self::Map(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        )
    }

    /// A JSON array, from anything that yields values.
    #[must_use]
    pub fn list<V: Into<Self>>(values: impl IntoIterator<Item = V>) -> Self {
        Self::List(values.into_iter().map(Into::into).collect())
    }

    /// Reads one JSON document. Anything malformed reads as `Null` rather than
    /// failing: a handler asking for a field that is not there wants an empty
    /// answer, not a panic in the middle of a request.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let mut reader = YonJsonReader {
            bytes: source.as_bytes(),
            at: 0,
        };
        reader.value()
    }

    /// Writes this value as JSON.
    #[must_use]
    pub fn write(&self) -> String {
        match self {
            Self::Null => String::from("null"),
            Self::Bool(value) => String::from(if *value { "true" } else { "false" }),
            Self::Number(value) => {
                // A whole number crosses as an integer, because a handler
                // counting things wrote one and expects one back.
                if value.fract() == 0.0 && value.is_finite() {
                    format!("{}", *value as i64)
                } else if value.is_finite() {
                    format!("{value}")
                } else {
                    String::from("null")
                }
            }
            Self::Text(value) => Self::quote(value),
            Self::List(values) => {
                let inner: Vec<String> = values.iter().map(Self::write).collect();
                format!("[{}]", inner.join(","))
            }
            Self::Map(entries) => {
                let inner: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{}:{}", Self::quote(key), value.write()))
                    .collect();
                format!("{{{}}}", inner.join(","))
            }
        }
    }

    /// One field of an object, or `Null` when it is absent or this is not one.
    #[must_use]
    pub fn get(&self, name: &str) -> &Self {
        match self {
            Self::Map(entries) => entries
                .iter()
                .find(|(key, _)| key == name)
                .map_or(&Self::Null, |(_, value)| value),
            _ => &Self::Null,
        }
    }

    /// The values of an array, empty for anything else.
    #[must_use]
    pub fn items(&self) -> &[Self] {
        match self {
            Self::List(values) => values,
            _ => &[],
        }
    }

    /// The entries of an object, empty for anything else.
    #[must_use]
    pub fn entries(&self) -> &[(String, Self)] {
        match self {
            Self::Map(entries) => entries,
            _ => &[],
        }
    }

    /// This value as text. A number or a boolean reads as what it would be
    /// written as; anything absent reads as empty.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Null => String::new(),
            other => other.write(),
        }
    }

    /// This value as a number, or zero when it is not one.
    #[must_use]
    pub fn number(&self) -> f64 {
        match self {
            Self::Number(value) => *value,
            Self::Text(value) => value.parse().unwrap_or_default(),
            Self::Bool(true) => 1.0,
            _ => 0.0,
        }
    }

    /// This value as a boolean. Absent, false, zero and empty are all false.
    #[must_use]
    pub fn boolean(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0,
            Self::Text(value) => !value.is_empty(),
            Self::List(values) => !values.is_empty(),
            Self::Map(entries) => !entries.is_empty(),
            Self::Null => false,
        }
    }

    /// Whether this value is absent.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    fn quote(value: &str) -> String {
        let mut out = String::from("\"");
        for character in value.chars() {
            match character {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                _ if (character as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", character as u32));
                }
                _ => out.push(character),
            }
        }
        out.push('"');
        out
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.write())
    }
}

// So `YonResponse::json(Json::map(…))` works: the response takes anything that
// becomes a string, and a JSON value becomes the JSON it is.
impl From<Json> for String {
    fn from(value: Json) -> Self {
        value.write()
    }
}

impl From<&str> for Json {
    fn from(value: &str) -> Self {
        Self::Text(String::from(value))
    }
}

impl From<String> for Json {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<bool> for Json {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for Json {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<i64> for Json {
    fn from(value: i64) -> Self {
        #[allow(clippy::cast_precision_loss)]
        Self::Number(value as f64)
    }
}

impl From<i32> for Json {
    fn from(value: i32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<usize> for Json {
    fn from(value: usize) -> Self {
        #[allow(clippy::cast_precision_loss)]
        Self::Number(value as f64)
    }
}

impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(values: Vec<T>) -> Self {
        Self::list(values)
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::Null, Into::into)
    }
}

impl<K: Into<String>, V: Into<Json>> From<std::collections::HashMap<K, V>> for Json {
    fn from(entries: std::collections::HashMap<K, V>) -> Self {
        Self::map(entries)
    }
}

impl<K: Into<String>, V: Into<Json>> From<std::collections::BTreeMap<K, V>> for Json {
    fn from(entries: std::collections::BTreeMap<K, V>) -> Self {
        Self::map(entries)
    }
}

/// Reads JSON from bytes. Bytes rather than chars because every structural
/// character in JSON is ASCII, so only a string's contents need decoding.
struct YonJsonReader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl YonJsonReader<'_> {
    fn value(&mut self) -> Json {
        self.skip();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Json::Text(self.string()),
            Some(b't') => {
                self.at += 4;
                Json::Bool(true)
            }
            Some(b'f') => {
                self.at += 5;
                Json::Bool(false)
            }
            Some(b'n') => {
                self.at += 4;
                Json::Null
            }
            Some(_) => self.number(),
            None => Json::Null,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn object(&mut self) -> Json {
        let mut entries = Vec::new();
        self.at += 1;
        loop {
            self.skip();
            match self.peek() {
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                Some(b',') => {
                    self.at += 1;
                    continue;
                }
                None => break,
                _ => {}
            }
            let key = self.string();
            self.skip();
            if self.peek() == Some(b':') {
                self.at += 1;
            }
            entries.push((key, self.value()));
        }
        Json::Map(entries)
    }

    fn array(&mut self) -> Json {
        let mut values = Vec::new();
        self.at += 1;
        loop {
            self.skip();
            match self.peek() {
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                Some(b',') => {
                    self.at += 1;
                    continue;
                }
                None => break,
                _ => {}
            }
            values.push(self.value());
        }
        Json::List(values)
    }

    fn string(&mut self) -> String {
        let mut out = String::new();
        if self.peek() != Some(b'"') {
            return out;
        }
        self.at += 1;
        while let Some(byte) = self.peek() {
            self.at += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let Some(escaped) = self.peek() else { break };
                    self.at += 1;
                    match escaped {
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'u' => {
                            let digits = self
                                .bytes
                                .get(self.at..self.at + 4)
                                .and_then(|slice| std::str::from_utf8(slice).ok())
                                .unwrap_or("");
                            self.at += digits.len();
                            if let Some(value) = u32::from_str_radix(digits, 16)
                                .ok()
                                .and_then(char::from_u32)
                            {
                                out.push(value);
                            }
                        }
                        other => out.push(char::from(other)),
                    }
                }
                // A multi-byte character is copied through whole: only the
                // structure of JSON is ASCII, not its contents.
                _ if byte < 0x80 => out.push(char::from(byte)),
                _ => {
                    let start = self.at - 1;
                    while self.peek().is_some_and(|next| next & 0xC0 == 0x80) {
                        self.at += 1;
                    }
                    if let Ok(text) = std::str::from_utf8(&self.bytes[start..self.at]) {
                        out.push_str(text);
                    }
                }
            }
        }
        out
    }

    fn number(&mut self) -> Json {
        let start = self.at;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_digit() || b"+-.eE".contains(&byte))
        {
            self.at += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.at])
            .ok()
            .and_then(|text| text.parse().ok())
            .map_or(Json::Null, Json::Number)
    }
}

/// One request, as the protocol delivers it.
pub struct YonRequest {
    raw: String,
    parsed: Json,
}

impl YonRequest {
    /// The HTTP method, upper case.
    #[must_use]
    pub fn method(&self) -> String {
        self.parsed.get("method").text()
    }

    /// The route as matched, with its parameters still in it.
    #[must_use]
    pub fn route(&self) -> String {
        self.parsed.get("route").text()
    }

    /// One bound route parameter, or an empty string when the route has none.
    ///
    /// A dynamic segment reaches a handler the same way in every language Yon
    /// runs, so this is named for what it is rather than for how it arrived.
    #[must_use]
    pub fn parameter(&self, name: &str) -> String {
        self.parsed.get("parameters").get(name).text()
    }

    /// The request body as text, empty when none was sent.
    #[must_use]
    pub fn body(&self) -> String {
        self.parsed.get("body").get("data").text()
    }

    /// The request body already parsed, for the common case of JSON in.
    #[must_use]
    pub fn json(&self) -> Json {
        Json::parse(&self.body())
    }

    /// The whole request as JSON, for anything the accessors do not cover.
    #[must_use]
    pub fn parsed(&self) -> &Json {
        &self.parsed
    }

    /// The whole request as the text it arrived as.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// One response, built the way the other layers build their return values.
pub struct YonResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl YonResponse {
    /// A JSON body with a 200. The common case, so it is the short one.
    #[must_use]
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![(
                String::from("content-type"),
                String::from("application/json"),
            )],
            body: body.into(),
        }
    }

    /// A response with no body, for a 204 or a redirect.
    #[must_use]
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: String::new(),
        }
    }

    #[must_use]
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers
            .push((String::from(name), String::from(value)));
        self
    }

    fn write(&self) -> String {
        // A header value is a list because one header may repeat, so the
        // values are grouped under the name rather than written as one object
        // entry each: two entries with the same key is a JSON object where the
        // second silently wins, and `header()` said it would keep both.
        let mut grouped: Vec<(String, Vec<Json>)> = Vec::new();
        for (name, value) in &self.headers {
            if let Some((_, values)) = grouped.iter_mut().find(|(key, _)| key == name) {
                values.push(Json::from(value.clone()));
            } else {
                grouped.push((name.clone(), vec![Json::from(value.clone())]));
            }
        }
        let headers = Json::map(
            grouped
                .into_iter()
                .map(|(name, values)| (name, Json::List(values))),
        );
        Json::map([
            (String::from("status"), Json::from(i64::from(self.status))),
            (String::from("headers"), headers),
            (String::from("body"), Json::from(self.body.clone())),
        ])
        .write()
    }
}

/// A method the handler did not write.
///
/// Every method has a default, so an author declares only the ones their route
/// answers — which is what makes `impl OrdersController { fn GET(…) }` a complete
/// handler rather than the first of five stubs.
pub trait YonRoutes {
    fn GET(_request: &YonRequest) -> YonResponse {
        YonResponse::empty(405)
    }
    fn POST(_request: &YonRequest) -> YonResponse {
        YonResponse::empty(405)
    }
    fn PUT(_request: &YonRequest) -> YonResponse {
        YonResponse::empty(405)
    }
    fn PATCH(_request: &YonRequest) -> YonResponse {
        YonResponse::empty(405)
    }
    fn DELETE(_request: &YonRequest) -> YonResponse {
        YonResponse::empty(405)
    }
    fn OPTIONS(_request: &YonRequest) -> YonResponse {
        YonResponse::empty(405)
    }
}

// An inherent method wins over a trait one, so a `fn GET` the author wrote is
// the one called and every method they did not write falls to the default
// above. That is the whole of the dispatch.
impl YonRoutes for __YON_CONTROLLER__ {}

/// Runs a handler written in a language Yon does not run.
///
/// Yon runs the eight languages that can declare a layer. Go, Ruby, Elixir and
/// the rest cannot, so they are not routes — but they are still programs, and a
/// program that speaks Handler Protocol v1 on standard input and output is
/// exactly what Yon spawns anyway. The shim is that spawn, written in the
/// language doing the delegating rather than in the language being delegated
/// to: nothing has to be installed on the other side.
///
/// The command is explicit rather than inferred from the file name, because a
/// compiled language has no interpreter to infer — `["./bin/report"]` is a Go
/// binary and `["ruby", "server/delegates/report.rb"]` is a script, and the
/// shim does not need to know which. The working directory is the project
/// root, so a project-relative path reads the way it is written.
///
/// This belongs on a `@Delegate`: it owns work handed to something else, which
/// is the layer's whole definition.
#[must_use]
pub fn relay(command: &[&str], request: &YonRequest) -> YonResponse {
    use std::io::Write as _;
    use std::time::{Duration, Instant};

    let Some((program, arguments)) = command.split_first() else {
        return yon_relay_failed("A delegate command cannot be empty.");
    };
    let child = std::process::Command::new(program)
        .args(arguments)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(_) => return yon_relay_failed("Delegate could not be started."),
    };
    let stdout = child.stdout.take().map(|pipe| yon_relay_drain(pipe, 16 * 1024 * 1024));
    let stderr = child.stderr.take().map(|pipe| yon_relay_drain(pipe, 64 * 1024));
    // Written and dropped before the output is read, so the child sees end of
    // input rather than waiting for more while this side waits for an answer.
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(request.raw().as_bytes()).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return yon_relay_failed("Delegate invocation failed.");
        }
    }
    let requested = request.parsed().get("deadline_ms").number();
    let timeout_ms = if requested.is_finite() && requested >= 1.0 {
        (requested as u64).min(300_000)
    } else {
        30_000
    };
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let settle = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(1));
    let stdout = stdout.and_then(|receiver| receiver.recv_timeout(settle).ok());
    // Stderr is drained concurrently and bounded, but never reaches the
    // response. A missing drain is itself a generic upstream failure.
    let stderr_settled = stderr.is_none_or(|receiver| receiver.recv_timeout(settle).is_ok());
    let Some(status) = status else {
        return yon_relay_failed("Delegate invocation failed.");
    };
    let Some((stdout, overflow)) = stdout else {
        return yon_relay_failed("Delegate invocation failed.");
    };
    if !status.success() || overflow || !stderr_settled {
        return yon_relay_failed("Delegate invocation failed.");
    }
    let Ok(stdout) = String::from_utf8(stdout) else {
        return yon_relay_failed("Delegate returned an invalid response.");
    };
    yon_envelope(&stdout)
        .unwrap_or_else(|| yon_relay_failed("Delegate returned an invalid response."))
}

/// Drains a delegate pipe without allowing its contents to grow memory.
fn yon_relay_drain<R: std::io::Read + Send + 'static>(
    mut pipe: R,
    limit: usize,
) -> std::sync::mpsc::Receiver<(Vec<u8>, bool)> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut overflow = false;
        let mut chunk = [0_u8; 8 * 1024];
        loop {
            let count = match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => {
                    overflow = true;
                    break;
                }
            };
            let remaining = limit.saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..count.min(remaining)]);
            overflow |= count > remaining;
        }
        let _ = sender.send((kept, overflow));
    });
    receiver
}

/// A delegate that could not be run answers 502, the same as any other
/// upstream that did not reply. The reason is deliberately generic: process
/// errors and delegate stderr are diagnostics, never client response data.
fn yon_relay_failed(reason: &str) -> YonResponse {
    YonResponse::json(Json::map([("error", reason)])).status(502)
}

/// Reads a Handler Protocol v1 envelope back into a response.
fn yon_envelope(source: &str) -> Option<YonResponse> {
    let envelope = Json::parse(source);
    if envelope.is_null() {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let status = envelope.get("status").number() as u16;
    let mut response = YonResponse::json(envelope.get("body").text());
    response = response.status(if status == 0 { 200 } else { status });
    // The headers the relayed program set replace the assumed content type,
    // because a program that answered with a header meant that header.
    let headers = envelope.get("headers");
    if !headers.entries().is_empty() {
        response.headers.clear();
        for (name, value) in headers.entries() {
            // The protocol carries a list per name, and a bare string is
            // accepted too: a program that wrote one value meant one value.
            if value.items().is_empty() {
                response = response.header(name, &value.text());
            } else {
                for item in value.items() {
                    response = response.header(name, &item.text());
                }
            }
        }
    }
    Some(response)
}

fn main() {
    use std::io::{Read as _, Write as _};

    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);
    let parsed = Json::parse(&raw);
    let request = YonRequest { raw, parsed };

    let response = match request.method().as_str() {
        "GET" => __YON_CONTROLLER__::GET(&request),
        "POST" => __YON_CONTROLLER__::POST(&request),
        "PUT" => __YON_CONTROLLER__::PUT(&request),
        "PATCH" => __YON_CONTROLLER__::PATCH(&request),
        "DELETE" => __YON_CONTROLLER__::DELETE(&request),
        "OPTIONS" => __YON_CONTROLLER__::OPTIONS(&request),
        _ => YonResponse::empty(405),
    };
    let _ = std::io::stdout().write_all(response.write().as_bytes());
}
