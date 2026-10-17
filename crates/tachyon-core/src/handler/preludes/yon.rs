#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Integer(i128),
    Unsigned(u128),
    Text(String),
    List(Vec<Json>),
    Map(Vec<(String, Json)>),
}

impl Json {
    #[must_use]
    pub fn map<K: Into<String>, V: Into<Self>>(entries: impl IntoIterator<Item = (K, V)>) -> Self {
        Self::Map(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        )
    }

    #[must_use]
    pub fn list<V: Into<Self>>(values: impl IntoIterator<Item = V>) -> Self {
        Self::List(values.into_iter().map(Into::into).collect())
    }

    #[must_use]
    pub fn parse(source: &str) -> Result<Self, JsonError> {
        if source.len() > YON_JSON_MAX_BYTES {
            return Err(JsonError);
        }
        let mut reader = YonJsonReader {
            bytes: source.as_bytes(),
            at: 0,
        };
        let value = reader.value(0)?;
        reader.skip();
        if reader.at != reader.bytes.len() {
            return Err(JsonError);
        }
        Ok(value)
    }

    pub fn stringify(&self) -> Result<String, JsonError> {
        let mut out = String::new();
        self.append_json(&mut out, 0)?;
        Ok(out)
    }

    fn append_json(&self, out: &mut String, depth: usize) -> Result<(), JsonError> {
        if depth > 64 {
            return Err(JsonError);
        }
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Integer(value) => out.push_str(&value.to_string()),
            Self::Unsigned(value) => out.push_str(&value.to_string()),
            Self::Number(value) => {
                if !value.is_finite() {
                    return Err(JsonError);
                }
                out.push_str(&value.to_string());
            }
            Self::Text(text) => Self::append_string(out, text)?,
            Self::List(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    value.append_json(out, depth + 1)?;
                }
                out.push(']');
            }
            Self::Map(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    Self::append_string(out, key)?;
                    out.push(':');
                    value.append_json(out, depth + 1)?;
                }
                out.push('}');
            }
        }
        if out.len() > YON_JSON_MAX_BYTES {
            return Err(JsonError);
        }
        Ok(())
    }

    fn append_string(out: &mut String, text: &str) -> Result<(), JsonError> {
        if text.len() > YON_JSON_MAX_BYTES {
            return Err(JsonError);
        }
        out.push('"');
        for character in text.chars() {
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
            if out.len() > YON_JSON_MAX_BYTES {
                return Err(JsonError);
            }
        }
        out.push('"');
        Ok(())
    }

    #[must_use]
    pub fn write(&self) -> String {
        match self {
            Self::Null => String::from("null"),
            Self::Bool(value) => String::from(if *value { "true" } else { "false" }),
            Self::Integer(value) => value.to_string(),
            Self::Unsigned(value) => value.to_string(),
            Self::Number(value) => {
                if value.is_finite() {
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

    #[must_use]
    pub fn items(&self) -> &[Self] {
        match self {
            Self::List(values) => values,
            _ => &[],
        }
    }

    #[must_use]
    pub fn entries(&self) -> &[(String, Self)] {
        match self {
            Self::Map(entries) => entries,
            _ => &[],
        }
    }

    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Null => String::new(),
            other => other.write(),
        }
    }

    #[must_use]
    pub fn number(&self) -> f64 {
        match self {
            Self::Number(value) => *value,
            Self::Integer(value) => *value as f64,
            Self::Unsigned(value) => *value as f64,
            Self::Text(value) => value.parse().unwrap_or_default(),
            Self::Bool(true) => 1.0,
            _ => 0.0,
        }
    }

    #[must_use]
    pub fn boolean(&self) -> bool {
        match self {
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0,
            Self::Integer(value) => *value != 0,
            Self::Unsigned(value) => *value != 0,
            Self::Text(value) => !value.is_empty(),
            Self::List(values) => !values.is_empty(),
            Self::Map(entries) => !entries.is_empty(),
            Self::Null => false,
        }
    }

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
        Self::Integer(i128::from(value))
    }
}

impl From<i32> for Json {
    fn from(value: i32) -> Self {
        Self::Integer(i128::from(value))
    }
}

impl From<usize> for Json {
    fn from(value: usize) -> Self {
        Self::Unsigned(value as u128)
    }
}

macro_rules! yon_signed_json {
    ($($kind:ty),*) => { $(impl From<$kind> for Json {
        fn from(value: $kind) -> Self { Self::Integer(value as i128) }
    })* };
}
macro_rules! yon_unsigned_json {
    ($($kind:ty),*) => { $(impl From<$kind> for Json {
        fn from(value: $kind) -> Self { Self::Unsigned(value as u128) }
    })* };
}
yon_signed_json!(i8, i16, isize, i128);
yon_unsigned_json!(u8, u16, u32, u64, u128);
impl From<f32> for Json {
    fn from(value: f32) -> Self {
        Self::Number(f64::from(value))
    }
}
impl From<()> for Json {
    fn from(_: ()) -> Self {
        Self::Null
    }
}
impl<T: Into<Json>, const N: usize> From<[T; N]> for Json {
    fn from(values: [T; N]) -> Self {
        Self::list(values)
    }
}

impl<T: Into<Json>> From<Vec<T>> for Json {
    fn from(values: Vec<T>) -> Self {
        Self::list(values)
    }
}

impl<T: Into<Json>> From<std::collections::VecDeque<T>> for Json {
    fn from(values: std::collections::VecDeque<T>) -> Self {
        Self::list(values)
    }
}

impl<T: Into<Json>> From<std::collections::HashSet<T>> for Json {
    fn from(values: std::collections::HashSet<T>) -> Self {
        Self::list(values)
    }
}

impl<T: Into<Json>> From<std::collections::BTreeSet<T>> for Json {
    fn from(values: std::collections::BTreeSet<T>) -> Self {
        Self::list(values)
    }
}

impl<T: Into<Json>> From<Box<T>> for Json {
    fn from(value: Box<T>) -> Self {
        (*value).into()
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

const YON_JSON_MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsonError;
impl std::fmt::Display for JsonError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("Invalid or unsupported JSON value.")
    }
}
impl std::error::Error for JsonError {}

struct YonJsonReader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl YonJsonReader<'_> {
    fn value(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.skip();
        if depth > 64 {
            return Err(JsonError);
        }
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Json::Text),
            Some(b't') => self.literal(b"true", Json::Bool(true)),
            Some(b'f') => self.literal(b"false", Json::Bool(false)),
            Some(b'n') => self.literal(b"null", Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(JsonError),
        }
    }
    fn literal(&mut self, text: &[u8], value: Json) -> Result<Json, JsonError> {
        if self.bytes.get(self.at..self.at + text.len()) != Some(text) {
            return Err(JsonError);
        }
        self.at += text.len();
        Ok(value)
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
    fn skip(&mut self) {
        while self.peek().is_some_and(|b| b" \n\r\t".contains(&b)) {
            self.at += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.skip();
        if self.peek() != Some(byte) {
            return false;
        }
        self.at += 1;
        true
    }
    fn require(&mut self, byte: u8) -> Result<(), JsonError> {
        if self.take(byte) {
            Ok(())
        } else {
            Err(JsonError)
        }
    }
    fn object(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.require(b'{')?;
        let mut entries = Vec::new();
        if self.take(b'}') {
            return Ok(Json::Map(entries));
        }
        loop {
            self.skip();
            let key = self.string()?;
            self.require(b':')?;
            let value = self.value(depth + 1)?;
            if let Some((_, previous)) = entries.iter_mut().find(|(name, _)| name == &key) {
                *previous = value;
            } else {
                entries.push((key, value));
            }
            if self.take(b'}') {
                return Ok(Json::Map(entries));
            }
            self.require(b',')?;
        }
    }
    fn array(&mut self, depth: usize) -> Result<Json, JsonError> {
        self.require(b'[')?;
        let mut values = Vec::new();
        if self.take(b']') {
            return Ok(Json::List(values));
        }
        loop {
            values.push(self.value(depth + 1)?);
            if self.take(b']') {
                return Ok(Json::List(values));
            }
            self.require(b',')?;
        }
    }
    fn hex(&mut self) -> Result<u32, JsonError> {
        let bytes = self.bytes.get(self.at..self.at + 4).ok_or(JsonError)?;
        if !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err(JsonError);
        }
        let digits = std::str::from_utf8(bytes).map_err(|_| JsonError)?;
        let value = u32::from_str_radix(digits, 16).map_err(|_| JsonError)?;
        self.at += 4;
        Ok(value)
    }
    fn unicode(&mut self) -> Result<char, JsonError> {
        let first = self.hex()?;
        let point = if (0xd800..=0xdbff).contains(&first) {
            if self.bytes.get(self.at..self.at + 2) != Some(b"\\u") {
                return Err(JsonError);
            }
            self.at += 2;
            let second = self.hex()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err(JsonError);
            }
            0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
        } else {
            first
        };
        char::from_u32(point).ok_or(JsonError)
    }
    fn string(&mut self) -> Result<String, JsonError> {
        self.require(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self.peek().ok_or(JsonError)?;
            self.at += 1;
            match byte {
                b'"' => return Ok(out),
                0..=0x1f => return Err(JsonError),
                b'\\' => {
                    let escaped = self.peek().ok_or(JsonError)?;
                    self.at += 1;
                    match escaped {
                        b'"' | b'\\' | b'/' => out.push(char::from(escaped)),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'u' => out.push(self.unicode()?),
                        _ => return Err(JsonError),
                    }
                }
                _ if byte < 0x80 => out.push(char::from(byte)),
                _ => {
                    let start = self.at - 1;
                    let text = std::str::from_utf8(&self.bytes[start..]).map_err(|_| JsonError)?;
                    let c = text.chars().next().ok_or(JsonError)?;
                    self.at = start + c.len_utf8();
                    out.push(c);
                }
            }
        }
    }
    fn digits(&mut self) -> Result<(), JsonError> {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == start {
            Err(JsonError)
        } else {
            Ok(())
        }
    }
    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        if self.peek() == Some(b'0') {
            self.at += 1;
        } else {
            self.digits()?;
        }
        let mut fractional = false;
        if self.peek() == Some(b'.') {
            fractional = true;
            self.at += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            fractional = true;
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digits()?;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).map_err(|_| JsonError)?;
        if !fractional {
            if let Ok(value) = text.parse::<i128>() {
                return Ok(Json::Integer(value));
            }
            return text
                .parse::<u128>()
                .map(Json::Unsigned)
                .map_err(|_| JsonError);
        }
        let value: f64 = text.parse().map_err(|_| JsonError)?;
        if !value.is_finite() {
            return Err(JsonError);
        }
        Ok(Json::Number(value))
    }
}

pub struct __YonFrame {
    raw: String,
    parsed: Json,
}

impl __YonFrame {
    #[must_use]
    pub fn method(&self) -> String {
        self.parsed.get("method").text()
    }

    #[must_use]
    pub fn route(&self) -> String {
        self.parsed.get("route").text()
    }

    #[must_use]
    pub fn parameter(&self, name: &str) -> String {
        self.parsed.get("parameters").get(name).text()
    }

    #[must_use]
    pub fn body(&self) -> String {
        self.parsed.get("body").get("data").text()
    }

    #[must_use]
    pub fn json(&self) -> Result<Json, JsonError> {
        Json::parse(&self.body())
    }

    #[must_use]
    pub fn parsed(&self) -> &Json {
        &self.parsed
    }

    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn arguments(&self) -> Vec<Json> {
        self.parsed.get("arguments").items().to_vec()
    }
}

pub trait YonRead: Sized {
    fn read(value: &Json) -> Self;
}

impl YonRead for Json {
    fn read(value: &Json) -> Self {
        value.clone()
    }
}

impl YonRead for String {
    fn read(value: &Json) -> Self {
        value.text()
    }
}

impl YonRead for bool {
    fn read(value: &Json) -> Self {
        matches!(value, Json::Bool(true))
    }
}

impl YonRead for f64 {
    fn read(value: &Json) -> Self {
        value.number()
    }
}

impl YonRead for f32 {
    fn read(value: &Json) -> Self {
        value.number() as f32
    }
}

macro_rules! yon_read_whole_numbers {
    ($($kind:ty),*) => { $(impl YonRead for $kind {
        fn read(value: &Json) -> Self {
            match value {
                Json::Integer(number) => *number as $kind,
                Json::Unsigned(number) => *number as $kind,
                Json::Number(number) => *number as $kind,
                _ => 0,
            }
        }
    })* };
}
yon_read_whole_numbers!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl<T: YonRead> YonRead for Vec<T> {
    fn read(value: &Json) -> Self {
        value.items().iter().map(T::read).collect()
    }
}

impl<T: YonRead> YonRead for std::collections::VecDeque<T> {
    fn read(value: &Json) -> Self {
        value.items().iter().map(T::read).collect()
    }
}

impl<T: YonRead + Eq + std::hash::Hash> YonRead for std::collections::HashSet<T> {
    fn read(value: &Json) -> Self {
        value.items().iter().map(T::read).collect()
    }
}

impl<T: YonRead + Ord> YonRead for std::collections::BTreeSet<T> {
    fn read(value: &Json) -> Self {
        value.items().iter().map(T::read).collect()
    }
}

impl<T: YonRead> YonRead for Box<T> {
    fn read(value: &Json) -> Self {
        Box::new(T::read(value))
    }
}

impl<T: YonRead> YonRead for Option<T> {
    fn read(value: &Json) -> Self {
        match value {
            Json::Null => None,
            other => Some(T::read(other)),
        }
    }
}

pub struct __YonAnswer {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    streamed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct YonResourceError;
impl std::fmt::Display for YonResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Yon managed resource operation failed.")
    }
}
impl std::error::Error for YonResourceError {}

pub struct DataSource;
impl DataSource {
    pub fn new() -> Self { Self }
    pub fn query(&self, sql: &str, parameters: Vec<Json>) -> Result<Json, YonResourceError> {
        self.sql(sql, parameters, "rows")
    }
    pub fn execute(&self, sql: &str, parameters: Vec<Json>) -> Result<Json, YonResourceError> {
        self.sql(sql, parameters, "execute")
    }
    pub fn command(&self, command: Json) -> Result<Json, YonResourceError> {
        yon_resource(command)
    }
    fn sql(&self, sql: &str, parameters: Vec<Json>, mode: &str) -> Result<Json, YonResourceError> {
        yon_resource(Json::map([
            ("operation", Json::from("sql")), ("sql", Json::from(sql)),
            ("parameters", Json::from(parameters)), ("mode", Json::from(mode)),
        ]))
    }
    pub fn find_by_id(&self, collection: &str, id: &str) -> Result<Json, YonResourceError> {
        self.document("findById", vec![("collection", collection.into()), ("id", id.into())])
    }
    pub fn insert(&self, collection: &str, document: Json) -> Result<Json, YonResourceError> {
        self.document("insert", vec![("collection", collection.into()), ("value", document)])
    }
    pub fn patch(&self, collection: &str, id: &str, fields: Json) -> Result<Json, YonResourceError> {
        self.document("patch", vec![("collection", collection.into()), ("id", id.into()), ("value", fields)])
    }
    pub fn delete_by_id(&self, collection: &str, id: &str) -> Result<Json, YonResourceError> {
        self.document("deleteById", vec![("collection", collection.into()), ("id", id.into())])
    }
    pub fn scroll(&self, collection: &str, filter: Json, cursor: &str, size: u32) -> Result<Json, YonResourceError> {
        let cursor = if cursor.is_empty() { Json::Null } else { Json::from(cursor) };
        let command = Json::map([
            ("operation", Json::from("document")), ("command", Json::from("scroll")),
            ("collection", Json::from(collection)), ("where", filter),
            ("cursor", cursor), ("size", Json::from(size)),
        ]);
        let body = command.stringify().map_err(|_| YonResourceError)?;
        yon_resource_exchange(&body, std::time::Duration::from_secs(30))
    }
    pub fn create_collection(&self, name: &str) -> Result<Json, YonResourceError> {
        self.document("createCollection", vec![("collection", name.into())])
    }
    pub fn drop_collection(&self, name: &str) -> Result<Json, YonResourceError> {
        self.document("dropCollection", vec![("collection", name.into())])
    }
    pub fn collection_exists(&self, name: &str) -> Result<Json, YonResourceError> {
        self.document("collectionExists", vec![("collection", name.into())])
    }
    pub fn publish(&self, topic: &str, payload: Json, key: &str) -> Result<Json, YonResourceError> {
        self.queue("publish", vec![("topic", topic.into()), ("payload", payload), ("key", key.into())])
    }
    pub fn claim(&self, topic: &str, group: &str, max_messages: u32, lease_ms: u32) -> Result<Json, YonResourceError> {
        let fields = vec![("topic", topic.into()), ("group", group.into()), ("maxMessages", max_messages.into()), ("leaseMs", lease_ms.into())];
        self.queue("claim", fields)
    }
    pub fn ack(&self, topic: &str, group: &str, delivery: &Json) -> Result<Json, YonResourceError> {
        let fields = vec![("topic", topic.into()), ("group", group.into()), ("id", delivery.get("id").clone()), ("receipt", delivery.get("receipt").clone())];
        self.queue("ack", fields)
    }
    pub fn nack(&self, topic: &str, group: &str, delivery: &Json, delay_ms: u32) -> Result<Json, YonResourceError> {
        let fields = vec![("topic", topic.into()), ("group", group.into()), ("id", delivery.get("id").clone()), ("receipt", delivery.get("receipt").clone()), ("delayMs", delay_ms.into())];
        self.queue("nack", fields)
    }
    fn document(&self, command: &str, mut fields: Vec<(&str, Json)>) -> Result<Json, YonResourceError> {
        fields.extend([("operation", "document".into()), ("command", command.into())]);
        yon_resource(Json::map(fields))
    }
    fn queue(&self, command: &str, mut fields: Vec<(&str, Json)>) -> Result<Json, YonResourceError> {
        fields.extend([("operation", "queue".into()), ("command", command.into())]);
        yon_resource(Json::map(fields))
    }
}

pub struct RedisTemplate;
impl RedisTemplate {
    pub fn new() -> Self { Self }
    pub fn get(&self, key: &str) -> Result<Json, YonResourceError> {
        self.call("get", key, Vec::new())
    }
    pub fn set(&self, key: &str, value: Json, ttl_seconds: u32) -> Result<Json, YonResourceError> {
        self.call("set", key, vec![("value", value), ("ttlSeconds", ttl_seconds.into())])
    }
    pub fn delete(&self, key: &str) -> Result<Json, YonResourceError> {
        self.call("delete", key, Vec::new())
    }
    fn call(&self, command: &str, key: &str, mut extra: Vec<(&str, Json)>) -> Result<Json, YonResourceError> {
        extra.extend([("operation", "redis".into()), ("command", command.into()), ("key", key.into())]);
        yon_resource(Json::map(extra))
    }
}

struct YonResourceLimits;

impl YonResourceLimits {
    const MAX_BYTES: usize = 1024 * 1024;
    const MAX_HEAD_BYTES: usize = 8192;
}

fn yon_resource(command: Json) -> Result<Json, YonResourceError> {
    let body = command.stringify().map_err(|_| YonResourceError)?;
    yon_resource_exchange(&body, std::time::Duration::from_secs(5))
}

fn yon_resource_exchange(body: &str, timeout: std::time::Duration) -> Result<Json, YonResourceError> {
    use std::io::{Read as _, Write as _};
    let endpoint = std::env::var("YON_RESOURCE_ENDPOINT").map_err(|_| YonResourceError)?;
    let host = endpoint.strip_prefix("http://").and_then(|value| value.strip_suffix('/')).ok_or(YonResourceError)?;
    let address: std::net::SocketAddrV4 = host.parse().map_err(|_| YonResourceError)?;
    if *address.ip() != std::net::Ipv4Addr::LOCALHOST { return Err(YonResourceError); }
    let token = std::env::var("YON_RESOURCE_TOKEN").map_err(|_| YonResourceError)?;
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) { return Err(YonResourceError); }
    if body.len() > YonResourceLimits::MAX_BYTES { return Err(YonResourceError); }
    let mut stream = std::net::TcpStream::connect_timeout(&address.into(), timeout).map_err(|_| YonResourceError)?;
    stream.set_read_timeout(Some(timeout)).map_err(|_| YonResourceError)?;
    stream.set_write_timeout(Some(timeout)).map_err(|_| YonResourceError)?;
    let request = format!("POST / HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).map_err(|_| YonResourceError)?;
    let mut response = Vec::new();
    let longest_response = YonResourceLimits::MAX_HEAD_BYTES + YonResourceLimits::MAX_BYTES + 1;
    stream.take(longest_response as u64).read_to_end(&mut response).map_err(|_| YonResourceError)?;
    let split = response.windows(4).position(|bytes| bytes == b"\r\n\r\n").ok_or(YonResourceError)?;
    if split > YonResourceLimits::MAX_HEAD_BYTES || response.len() - split - 4 > YonResourceLimits::MAX_BYTES || !response.starts_with(b"HTTP/1.1 200 ") { return Err(YonResourceError); }
    let headers = std::str::from_utf8(&response[..split]).map_err(|_| YonResourceError)?;
    let mut length = None;
    for line in headers.split("\r\n").skip(1) {
        let (name, value) = line.split_once(':').ok_or(YonResourceError)?;
        if name.eq_ignore_ascii_case("transfer-encoding") { return Err(YonResourceError); }
        if name.eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            if length.is_some() || value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) { return Err(YonResourceError); }
            length = Some(value.parse::<usize>().map_err(|_| YonResourceError)?);
        }
    }
    if length != Some(response.len() - split - 4) { return Err(YonResourceError); }
    let text = std::str::from_utf8(&response[split + 4..]).map_err(|_| YonResourceError)?;
    Json::parse(text).map_err(|_| YonResourceError)
}

pub struct YonGateway;
impl YonGateway {
    const ANSWER_MARGIN: std::time::Duration = std::time::Duration::from_secs(1);

    #[must_use]
    pub fn call(key: &str, arguments: &str) -> Json {
        let body = format!("{{\"operation\":\"gateway\",\"key\":{},\"arguments\":{arguments}}}", Json::quote(key));
        if body.len() > YonResourceLimits::MAX_BYTES {
            return Self::failure("invalid");
        }
        let Some(invocation) = YonRelayInvocation::current().get() else {
            return Self::failure("timeout");
        };
        let time_left = invocation.time_left();
        if time_left.is_zero() {
            return Self::failure("timeout");
        }
        match yon_resource_exchange(&body, time_left + Self::ANSWER_MARGIN) {
            Ok(answer) => answer,
            Err(_) if invocation.time_left().is_zero() => Self::failure("timeout"),
            Err(_) => Self::failure("adapter"),
        }
    }

    fn failure(kind: &str) -> Json {
        Json::map([("error", kind)])
    }
}

pub struct YonTakesFrame;
pub struct YonTakesRequest;
pub struct YonTakesNothing;

pub trait YonEntry<Shape> {
    fn answer(self, frame: &__YonFrame) -> __YonAnswer;
}

impl<Method, Answer> YonEntry<YonTakesFrame> for Method
where
    Method: Fn(&__YonFrame) -> Answer,
    Answer: YonReturn,
{
    fn answer(self, frame: &__YonFrame) -> __YonAnswer {
        yon_response(self(frame))
    }
}

impl<Method, Answer> YonEntry<YonTakesRequest> for Method
where
    Method: Fn(&http::Request<Vec<u8>>) -> Answer,
    Answer: YonReturn,
{
    fn answer(self, frame: &__YonFrame) -> __YonAnswer {
        yon_response(self(&yon_native_request(frame)))
    }
}

impl<Method, Answer> YonEntry<YonTakesNothing> for Method
where
    Method: Fn() -> Answer,
    Answer: YonReturn,
{
    fn answer(self, _frame: &__YonFrame) -> __YonAnswer {
        yon_response(self())
    }
}

fn yon_entry<Shape>(method: impl YonEntry<Shape>, frame: &__YonFrame) -> __YonAnswer {
    method.answer(frame)
}

pub trait YonReturn {
    fn into_response(self) -> __YonAnswer;
}
impl YonReturn for __YonAnswer {
    fn into_response(self) -> __YonAnswer {
        self
    }
}
impl<T: Into<Json>> YonReturn for Result<T, YonResourceError> {
    fn into_response(self) -> __YonAnswer {
        match self {
            Ok(value) => value.into_response(),
            Err(_) => __YonAnswer::json("{\"error\":\"Yon managed resource operation failed.\"}").status(500),
        }
    }
}
impl<T: Into<Json>> YonReturn for T {
    fn into_response(self) -> __YonAnswer {
        match self.into().stringify() {
            Ok(body) => __YonAnswer::json(body),
            Err(_) => __YonAnswer::json("{\"error\":\"Handler returned an invalid JSON value.\"}")
                .status(500),
        }
    }
}
impl YonReturn for http::Response<Vec<u8>> {
    fn into_response(self) -> __YonAnswer {
        let mut answer = __YonAnswer::empty(self.status().as_u16());
        for (name, value) in self.headers() {
            answer = answer.header(name.as_str(), &String::from_utf8_lossy(value.as_bytes()));
        }
        answer.body = String::from_utf8_lossy(self.body()).into_owned();
        answer
    }
}
impl YonReturn for Option<http::Response<Vec<u8>>> {
    fn into_response(self) -> __YonAnswer {
        match self {
            Some(response) => response.into_response(),
            None => __YonAnswer::empty(204),
        }
    }
}
fn yon_response(value: impl YonReturn) -> __YonAnswer {
    value.into_response()
}

fn yon_native_request(frame: &__YonFrame) -> http::Request<Vec<u8>> {
    let target = match frame.parsed.get("target") {
        Json::Text(target) => target.clone(),
        _ => frame.route(),
    };
    let mut builder = http::Request::builder()
        .method(frame.method().as_str())
        .uri(target.as_str());
    for (name, values) in frame.parsed.get("headers").entries() {
        for value in values.items() {
            builder = builder.header(name.as_str(), value.text());
        }
    }
    builder
        .body(frame.body().into_bytes())
        .unwrap_or_else(|_| http::Request::new(Vec::new()))
}

fn yon_http_response(answer: __YonAnswer) -> http::Response<Vec<u8>> {
    let mut builder = http::Response::builder().status(answer.status);
    for (name, value) in &answer.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder
        .body(answer.body.into_bytes())
        .unwrap_or_else(|_| http::Response::new(Vec::new()))
}

fn yon_relay_message(request: &http::Request<Vec<u8>>) -> String {
    let mut headers: Vec<(String, Json)> = Vec::new();
    for name in request.headers().keys() {
        let values = request
            .headers()
            .get_all(name)
            .iter()
            .map(|value| Json::from(String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect::<Vec<_>>();
        headers.push((String::from(name.as_str()), Json::List(values)));
    }
    let route = request
        .uri()
        .path_and_query()
        .map_or_else(|| String::from("/"), |target| String::from(target.as_str()));
    let body = String::from_utf8_lossy(request.body()).into_owned();
    Json::map([
        (String::from("route"), Json::from(route)),
        (String::from("method"), Json::from(request.method().as_str())),
        (String::from("headers"), Json::map(headers)),
        (
            String::from("body"),
            Json::map([("encoding", Json::from("utf8")), ("data", Json::from(body))]),
        ),
    ])
    .write()
}

impl __YonAnswer {
    #[must_use]
    pub fn json(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            headers: vec![(
                String::from("content-type"),
                String::from("application/json"),
            )],
            body: body.into(),
            streamed: false,
        }
    }

    #[must_use]
    pub fn empty(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: String::new(),
            streamed: false,
        }
    }

    #[must_use]
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((String::from(name), String::from(value)));
        self
    }

    fn write(&self) -> String {
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

struct YonRelayInvocation {
    deadline: std::sync::Mutex<std::time::Instant>,
    request_id: String,
    environment: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
}

impl YonRelayInvocation {
    const DEFAULT_DEADLINE_MILLISECONDS: f64 = 30_000.0;
    const LONGEST_DEADLINE_SECONDS: f64 = 86_400.0;
    const FAILURE_MESSAGE: &'static str = "Component invocation failed.";
    const MAX_STDOUT_BYTES: usize = 16 * 1024 * 1024;
    const MAX_STDERR_BYTES: usize = 64 * 1024;

    fn current() -> &'static std::sync::OnceLock<Self> {
        static INVOCATION: std::sync::OnceLock<YonRelayInvocation> = std::sync::OnceLock::new();
        &INVOCATION
    }

    fn begin(
        started: std::time::Instant,
        environment: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
        request: &Json,
    ) {
        let seconds = Self::deadline_milliseconds(request) / 1000.0;
        let seconds = seconds.clamp(0.0, Self::LONGEST_DEADLINE_SECONDS);
        let invocation = Self {
            deadline: std::sync::Mutex::new(started + std::time::Duration::from_secs_f64(seconds)),
            request_id: Self::request_id(request),
            environment,
        };
        let _ = Self::current().set(invocation);
    }

    fn request_id(request: &Json) -> String {
        match request.get("request_id") {
            Json::Text(id) => id.clone(),
            _ => String::from("unknown"),
        }
    }

    fn deadline_milliseconds(request: &Json) -> f64 {
        let requested = request.get("deadline_ms");
        let is_number = matches!(requested, Json::Number(_) | Json::Integer(_) | Json::Unsigned(_));
        let milliseconds = requested.number();
        if !is_number || !milliseconds.is_finite() {
            return Self::DEFAULT_DEADLINE_MILLISECONDS;
        }
        milliseconds
    }

    fn time_left(&self) -> std::time::Duration {
        let deadline = *self.deadline.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        deadline.saturating_duration_since(std::time::Instant::now())
    }

    fn renew() {
        let Some(invocation) = Self::current().get() else {
            return;
        };
        let budget = std::time::Duration::from_secs_f64(Self::DEFAULT_DEADLINE_MILLISECONDS / 1000.0);
        *invocation.deadline.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
            std::time::Instant::now() + budget;
    }
}

fn yon_relay_environment() -> std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    let mut kept = std::collections::BTreeMap::new();
    for (name, value) in std::env::vars_os() {
        let upper = name.to_string_lossy().to_uppercase();
        if upper == "YON_APPLICATION_JSON" || upper.starts_with("YON_RESOURCE_") {
            continue;
        }
        kept.insert(name, value);
    }
    kept
}

#[must_use]
pub fn relay(command: &[&str], request: &http::Request<Vec<u8>>) -> http::Response<Vec<u8>> {
    yon_http_response(yon_relay_answer(command, &yon_relay_message(request)))
}

fn yon_relay_answer(command: &[&str], message: &str) -> __YonAnswer {
    let Some((program, arguments)) = command.split_first() else {
        return yon_relay_failure("start");
    };
    let Some(invocation) = YonRelayInvocation::current().get() else {
        return yon_relay_failure("timeout");
    };
    if invocation.time_left().is_zero() {
        return yon_relay_failure("timeout");
    }
    let child = std::process::Command::new(program)
        .args(arguments)
        .env_clear()
        .envs(&invocation.environment)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let Ok(child) = child else {
        return yon_relay_failure("start");
    };
    yon_relay_exchange(child, message, invocation)
}

fn yon_relay_exchange(
    mut child: std::process::Child,
    message: &str,
    invocation: &YonRelayInvocation,
) -> __YonAnswer {
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        yon_relay_stop(&mut child);
        return yon_relay_failure("start");
    };
    let stdout = yon_relay_drain(stdout, YonRelayInvocation::MAX_STDOUT_BYTES);
    let stderr = yon_relay_drain(stderr, YonRelayInvocation::MAX_STDERR_BYTES);
    yon_relay_send(stdin, message.to_owned());
    let Some(status) = yon_relay_wait(&mut child, invocation) else {
        yon_relay_stop(&mut child);
        return yon_relay_failure("timeout");
    };
    let (Ok((stdout, stdout_overflow)), Ok((_, stderr_overflow))) = (
        stdout.recv_timeout(invocation.time_left()),
        stderr.recv_timeout(invocation.time_left()),
    ) else {
        return yon_relay_failure("timeout");
    };
    if stdout_overflow || stderr_overflow {
        return yon_relay_failure("overflow");
    }
    if !status.success() {
        return yon_relay_failure("exit");
    }
    let Ok(stdout) = String::from_utf8(stdout) else {
        return yon_relay_failure("protocol");
    };
    yon_envelope(&stdout).unwrap_or_else(|| yon_relay_failure("protocol"))
}

fn yon_relay_send(mut stdin: std::process::ChildStdin, payload: String) {
    use std::io::Write as _;

    std::thread::spawn(move || {
        let _ = stdin.write_all(payload.as_bytes());
    });
}

fn yon_relay_wait(
    child: &mut std::process::Child,
    invocation: &YonRelayInvocation,
) -> Option<std::process::ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if !invocation.time_left().is_zero() => {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Ok(None) | Err(_) => return None,
        }
    }
}

fn yon_relay_stop(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

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

fn yon_relay_failure(category: &str) -> __YonAnswer {
    let request_id = match YonRelayInvocation::current().get() {
        Some(invocation) => invocation.request_id.clone(),
        None => String::from("unknown"),
    };
    let event = Json::map([
        ("event", "handler.relay_failed"),
        ("request_id", request_id.as_str()),
        ("category", category),
    ]);
    eprintln!("{}", event.write());
    __YonAnswer::json(Json::map([("error", YonRelayInvocation::FAILURE_MESSAGE)])).status(502)
}

fn yon_envelope(source: &str) -> Option<__YonAnswer> {
    let envelope = Json::parse(source).ok()?;
    if !matches!(envelope, Json::Map(_)) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let status = envelope.get("status").number() as u16;
    let mut response = __YonAnswer::json(envelope.get("body").text());
    response = response.status(if status == 0 { 200 } else { status });
    let headers = envelope.get("headers");
    if !headers.entries().is_empty() {
        response.headers.clear();
        for (name, value) in headers.entries() {
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

pub trait YonSocketEndpoint {
    fn open(&self, session: &SocketSession) -> Option<String>;
    fn message(&self, session: &SocketSession, value: &Json) -> Option<String>;
    fn close(&self, session: &SocketSession) -> Option<String>;
    fn error(&self, session: &SocketSession, error: &str) -> Option<String>;
}

pub trait YonSocketResult {
    fn failure(self) -> Option<String>;
}

impl YonSocketResult for () {
    fn failure(self) -> Option<String> {
        None
    }
}

impl<Value, Failure: std::fmt::Display> YonSocketResult for Result<Value, Failure> {
    fn failure(self) -> Option<String> {
        self.err().map(|failure| failure.to_string())
    }
}

pub struct SocketPeer {
    request_id: String,
    id: String,
}

impl SocketPeer {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn send(&self, message: impl Into<Json>) {
        YonSocket::send(&self.request_id, message.into(), Some(&self.id));
    }
}

pub struct SocketSession {
    id: String,
    inbox: std::cell::RefCell<std::collections::VecDeque<Json>>,
    closed: std::cell::Cell<bool>,
    sessions: std::cell::RefCell<Option<Vec<String>>>,
}

impl SocketSession {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn send(&self, message: impl Into<Json>) {
        YonSocket::send(&self.id, message.into(), None);
    }

    pub fn close(&self) {
        self.closed.set(true);
        YonSocket::write(&self.id, vec![(String::from("kind"), Json::from("close"))]);
    }

    #[must_use]
    pub fn open_sessions(&self) -> Vec<SocketPeer> {
        *self.sessions.borrow_mut() = None;
        YonSocket::write(&self.id, vec![(String::from("kind"), Json::from("sessions"))]);
        while self.sessions.borrow().is_none() && !self.closed.get() {
            self.receive();
        }
        let ids = self.sessions.borrow().clone().unwrap_or_default();
        ids.into_iter()
            .map(|id| SocketPeer {
                request_id: self.id.clone(),
                id,
            })
            .collect()
    }

    fn next(&self) -> Option<Json> {
        while self.inbox.borrow().is_empty() && !self.closed.get() {
            self.receive();
        }
        self.inbox.borrow_mut().pop_front()
    }

    fn receive(&self) {
        let frame = YonSocket::read_frame().and_then(|raw| Json::parse(&raw).ok());
        let Some(frame) = frame else {
            self.closed.set(true);
            return;
        };
        if frame.get("request_id").text() != self.id {
            std::process::exit(70);
        }
        match frame.get("kind").text().as_str() {
            "message" => {
                let data = frame.get("body").get("data").text();
                let value = Json::parse(&data).unwrap_or(Json::Null);
                self.inbox.borrow_mut().push_back(value);
            }
            "sessions" => {
                let ids = frame.get("sessions").items().iter().map(Json::text).collect();
                *self.sessions.borrow_mut() = Some(ids);
            }
            _ => self.closed.set(true),
        }
    }
}

pub struct YonSocket;

impl YonSocket {
    const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

    fn framed() -> bool {
        std::env::var("YON_SOCKET_FRAMES").is_ok_and(|value| value == "1")
    }

    fn read_frame() -> Option<String> {
        use std::io::Read as _;
        let mut input = std::io::stdin().lock();
        let mut prefix = [0_u8; 4];
        input.read_exact(&mut prefix).ok()?;
        let length = usize::try_from(u32::from_be_bytes(prefix)).unwrap_or(usize::MAX);
        if length > Self::MAX_FRAME_BYTES {
            std::process::exit(70);
        }
        let mut payload = vec![0_u8; length];
        input.read_exact(&mut payload).ok()?;
        String::from_utf8(payload).ok()
    }

    fn write(request_id: &str, fields: Vec<(String, Json)>) {
        let _ = Self::write_payload(&Self::payload(request_id, fields));
    }

    fn payload(request_id: &str, mut fields: Vec<(String, Json)>) -> Vec<u8> {
        fields.push((String::from("protocol_version"), Json::from(1_i64)));
        fields.push((String::from("request_id"), Json::from(request_id)));
        Json::Map(fields).write().into_bytes()
    }

    fn write_payload(payload: &[u8]) -> bool {
        use std::io::Write as _;
        let length = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        let mut output = std::io::stdout().lock();
        output.write_all(&length.to_be_bytes()).is_ok()
            && output.write_all(payload).is_ok()
            && output.flush().is_ok()
    }

    fn send(request_id: &str, message: Json, to: Option<&str>) {
        let body = Json::map([("encoding", Json::from("utf8")), ("data", Json::from(message.write()))]);
        let mut fields = vec![(String::from("kind"), Json::from("event")), (String::from("body"), body)];
        if let Some(to) = to {
            fields.push((String::from("to"), Json::from(to)));
        }
        Self::write(request_id, fields);
    }

    fn call(action: impl FnOnce() -> Option<String>) -> Option<String> {
        YonRelayInvocation::renew();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)) {
            Ok(failure) => failure,
            Err(_) => Some(String::from("A socket method panicked.")),
        }
    }

    fn attempt(
        endpoint: &impl YonSocketEndpoint,
        session: &SocketSession,
        action: impl FnOnce() -> Option<String>,
    ) {
        let Some(failure) = Self::call(action) else {
            return;
        };
        let line = Json::map([
            ("event", Json::from("handler.socket_failed")),
            ("request_id", Json::from(session.id())),
            ("message", Json::from(failure.as_str())),
        ]);
        eprintln!("{}", line.write());
        let _ = Self::call(|| endpoint.error(session, &failure));
    }

    fn run(endpoint: &impl YonSocketEndpoint, request: &__YonFrame) -> ! {
        let session = SocketSession {
            id: request.parsed().get("request_id").text(),
            inbox: std::cell::RefCell::new(std::collections::VecDeque::new()),
            closed: std::cell::Cell::new(false),
            sessions: std::cell::RefCell::new(None),
        };
        Self::attempt(endpoint, &session, || endpoint.open(&session));
        while let Some(message) = session.next() {
            Self::attempt(endpoint, &session, || endpoint.message(&session, &message));
        }
        let _ = Self::call(|| endpoint.close(&session));
        std::process::exit(0);
    }
}

pub trait YonStreamItem {
    fn into_item(self) -> Option<Json>;
}

impl<T: Into<Json>> YonStreamItem for T {
    fn into_item(self) -> Option<Json> {
        Some(self.into())
    }
}

impl<T: Into<Json>, Failure> YonStreamItem for Result<T, Failure> {
    fn into_item(self) -> Option<Json> {
        self.ok().map(Into::into)
    }
}

pub struct YonStream;

impl YonStream {
    fn answer<Items>(request: &__YonFrame, items: Items) -> __YonAnswer
    where
        Items: IntoIterator,
        Items::Item: YonStreamItem,
    {
        let request_id = request.parsed().get("request_id").text();
        let sent = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::send_all(&request_id, items)
        }));
        match sent {
            Ok(Ok(())) => {}
            Ok(Err(code)) => Self::fail(&request_id, code),
            Err(_) => Self::fail(&request_id, "TY2204"),
        }
        let mut answer = __YonAnswer::empty(200);
        answer.streamed = true;
        answer
    }

    fn send_all<Items>(request_id: &str, items: Items) -> Result<(), &'static str>
    where
        Items: IntoIterator,
        Items::Item: YonStreamItem,
    {
        for item in items {
            let data = item.into_item().ok_or("TY2204")?;
            let data = data.stringify().map_err(|_| "TY2204")?;
            let body = Json::map([("encoding", Json::from("utf8")), ("data", Json::from(data))]);
            let fields = vec![(String::from("kind"), Json::from("event")), (String::from("body"), body)];
            let payload = YonSocket::payload(request_id, fields);
            if payload.len() > YonSocket::MAX_FRAME_BYTES {
                return Err("TY2203");
            }
            if !YonSocket::write_payload(&payload) {
                return Ok(());
            }
        }
        Ok(())
    }

    fn fail(request_id: &str, code: &str) {
        let message = match code {
            "TY2203" => "Streamed event exceeds the protocol frame limit.",
            _ => "The handler stream failed.",
        };
        let error = Json::map([
            ("code", Json::from(code)),
            ("message", Json::from(message)),
            ("retryable", Json::from(false)),
        ]);
        YonSocket::write(request_id, vec![
            (String::from("kind"), Json::from("response")),
            (String::from("status"), Json::from(500_i64)),
            (String::from("headers"), Json::Map(Vec::new())),
            (String::from("error"), error),
        ]);
    }
}

fn yon_stream<Items>(request: &__YonFrame, items: Items) -> __YonAnswer
where
    Items: IntoIterator,
    Items::Item: YonStreamItem,
{
    YonStream::answer(request, items)
}

fn main() {
    use std::io::{Read as _, Write as _};

    let started = std::time::Instant::now();
    let environment = yon_relay_environment();
    let mut raw = String::new();
    if YonSocket::framed() {
        raw = YonSocket::read_frame().unwrap_or_default();
    } else {
        let _ = std::io::stdin().read_to_string(&mut raw);
    }
    let Ok(parsed) = Json::parse(&raw) else {
        return;
    };
    YonRelayInvocation::begin(started, environment, &parsed);
    let request = __YonFrame { raw, parsed };

    let response = yon_dispatch(&request);
    if response.streamed {
        return;
    }
    let _ = std::io::stdout().write_all(response.write().as_bytes());
}
