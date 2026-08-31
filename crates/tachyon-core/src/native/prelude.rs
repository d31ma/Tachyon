// Appended to a tac.rs companion compiled into a native host. The author
// writes a plain struct and impl; everything below is the protocol.
//
// Rust is a desktop companion: the macOS host links this as a static library
// and calls it from Swift, and the Win32 and GTK hosts load it as a shared one.
// Nothing here is WebAssembly — the companion runs in the host's own process
// with the whole platform in reach.
//
// The JSON is scanned and written here rather than by a crate, because this
// path compiles one file with `rustc` and no Cargo: there is nowhere to
// declare a dependency. The protocol's shape is fixed and small, so scanning
// it is a page of code rather than a manifest.


// The browser's two lifetimes, on the desktop. What survives every launch goes
// to a file under the platform's own config directory; a session is this
// process, because a native app has no tabs for a tab-scoped value to belong
// to. One file and no dependency, because this path compiles with `rustc` and
// no Cargo.
use std::collections::HashMap as TacMap;

thread_local! {
    static TAC_SESSION: core::cell::RefCell<TacMap<String, String>> =
        core::cell::RefCell::new(TacMap::new());
    static TAC_LOCAL: core::cell::RefCell<TacMap<String, String>> =
        core::cell::RefCell::new(tac_store_load());
}

/// Where a persisted value lives, by platform convention.
fn tac_store_path() -> std::path::PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .or_else(|_| std::env::var("LOCALAPPDATA"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".config")
        });
    base.join("tachyon").join(TAC_APPLICATION_ID).join("store.txt")
}

fn tac_store_load() -> TacMap<String, String> {
    let mut values = TacMap::new();
    if let Ok(text) = std::fs::read_to_string(tac_store_path()) {
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('\t') {
                values.insert(String::from(key), String::from(value));
            }
        }
    }
    values
}

/// Reads a value that survives every launch.
pub fn tac_local(key: &str, fallback: &str) -> String {
    TAC_LOCAL.with(|values| {
        values
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or_else(|| String::from(fallback))
    })
}

/// Writes a value that survives every launch.
pub fn tac_set_local(key: &str, value: &str) {
    TAC_LOCAL.with(|values| {
        values
            .borrow_mut()
            .insert(String::from(key), String::from(value));
        let path = tac_store_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = values
            .borrow()
            .iter()
            .map(|(key, value)| format!("{key}\t{value}"))
            .collect::<Vec<_>>()
            .join("\n");
        // An unwritable store loses the value, not the application.
        let _ = std::fs::write(path, text);
    });
}

/// Reads a value that lasts this process.
pub fn tac_session(key: &str, fallback: &str) -> String {
    TAC_SESSION.with(|values| {
        values
            .borrow()
            .get(key)
            .cloned()
            .unwrap_or_else(|| String::from(fallback))
    })
}

/// Writes a value that lasts this process.
pub fn tac_set_session(key: &str, value: &str) {
    TAC_SESSION.with(|values| {
        values
            .borrow_mut()
            .insert(String::from(key), String::from(value));
    });
}

/// A value crossing the boundary, in the few shapes the protocol carries.
pub enum TacValue {
    Null,
    Flag(bool),
    Int(i64),
    Float(f64),
    Text(String),
    List(Vec<TacValue>),
}

impl TacValue {
    /// The whole number a setter expects. JSON has one number type, so a
    /// companion counting things reads an integer out of whatever arrived.
    pub fn as_int(&self) -> i64 {
        match self {
            Self::Int(value) => *value,
            Self::Float(value) => *value as i64,
            Self::Flag(value) => i64::from(*value),
            Self::Text(value) => value.parse().unwrap_or_default(),
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f64 {
        match self {
            Self::Int(value) => *value as f64,
            Self::Float(value) => *value,
            Self::Text(value) => value.parse().unwrap_or_default(),
            _ => 0.0,
        }
    }

    pub fn as_flag(&self) -> bool {
        match self {
            Self::Flag(value) => *value,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::Text(value) => !value.is_empty(),
            Self::List(values) => !values.is_empty(),
            Self::Null => false,
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::Flag(value) => value.to_string(),
            _ => String::new(),
        }
    }
}

/// One member the island may reach, declared in `tac()`.
pub enum TacMember {
    Field {
        read: fn() -> TacValue,
        write: Option<fn(TacValue)>,
    },
    Method(fn(Vec<TacValue>) -> TacValue),
}

// ── Scanning ──────────────────────────────────────────────────────────────

struct TacScanner<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> TacScanner<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            at: 0,
        }
    }

    fn skip_space(&mut self) {
        while self.at < self.bytes.len() && self.bytes[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> u8 {
        self.skip_space();
        *self.bytes.get(self.at).unwrap_or(&0)
    }

    fn value(&mut self) -> TacValue {
        match self.peek() {
            b'{' => self.object_value(),
            b'[' => self.list(),
            b'"' => TacValue::Text(self.text()),
            b't' => {
                self.at += 4;
                TacValue::Flag(true)
            }
            b'f' => {
                self.at += 5;
                TacValue::Flag(false)
            }
            b'n' => {
                self.at += 4;
                TacValue::Null
            }
            _ => self.number(),
        }
    }

    /// Reads an object, keeping only the members the protocol names.
    fn object(&mut self) -> Vec<(String, TacValue)> {
        let mut entries = Vec::new();
        if self.peek() != b'{' {
            return entries;
        }
        self.at += 1;
        loop {
            if self.peek() == b'}' {
                self.at += 1;
                return entries;
            }
            let key = self.text();
            if self.peek() == b':' {
                self.at += 1;
            }
            entries.push((key, self.value()));
            if self.peek() == b',' {
                self.at += 1;
            } else {
                if self.peek() == b'}' {
                    self.at += 1;
                }
                return entries;
            }
        }
    }

    fn object_value(&mut self) -> TacValue {
        // An object reaching a companion is a payload it did not ask for, so
        // only its presence is carried across.
        self.object();
        TacValue::Null
    }

    fn list(&mut self) -> TacValue {
        let mut values = Vec::new();
        self.at += 1;
        loop {
            if self.peek() == b']' {
                self.at += 1;
                return TacValue::List(values);
            }
            values.push(self.value());
            if self.peek() == b',' {
                self.at += 1;
            } else {
                if self.peek() == b']' {
                    self.at += 1;
                }
                return TacValue::List(values);
            }
        }
    }

    fn text(&mut self) -> String {
        let mut out = String::new();
        if self.peek() != b'"' {
            return out;
        }
        self.at += 1;
        while self.at < self.bytes.len() {
            let byte = self.bytes[self.at];
            self.at += 1;
            match byte {
                b'"' => return out,
                b'\\' => {
                    let escape = *self.bytes.get(self.at).unwrap_or(&b'"');
                    self.at += 1;
                    out.push(match escape {
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => {
                            let hex = self
                                .bytes
                                .get(self.at..self.at + 4)
                                .and_then(|slice| core::str::from_utf8(slice).ok())
                                .and_then(|slice| u32::from_str_radix(slice, 16).ok())
                                .unwrap_or(0xFFFD);
                            self.at += 4;
                            char::from_u32(hex).unwrap_or('\u{fffd}')
                        }
                        other => other as char,
                    });
                }
                _ => {
                    // Multi-byte UTF-8 arrives a byte at a time; collecting the
                    // whole sequence keeps the string as the page wrote it.
                    let start = self.at - 1;
                    let width = match byte {
                        0x00..=0x7F => 1,
                        0xC0..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        _ => 4,
                    };
                    self.at = start + width;
                    out.push_str(
                        core::str::from_utf8(self.bytes.get(start..self.at).unwrap_or(&[]))
                            .unwrap_or("\u{fffd}"),
                    );
                }
            }
        }
        out
    }

    fn number(&mut self) -> TacValue {
        self.skip_space();
        let start = self.at;
        while self.at < self.bytes.len()
            && matches!(self.bytes[self.at], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        {
            self.at += 1;
        }
        let raw = core::str::from_utf8(self.bytes.get(start..self.at).unwrap_or(&[])).unwrap_or("0");
        raw.parse::<i64>().map_or_else(
            |_| TacValue::Float(raw.parse().unwrap_or(0.0)),
            TacValue::Int,
        )
    }
}

// ── Writing ───────────────────────────────────────────────────────────────

fn tac_quote(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other if (other as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", other as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn tac_write(value: &TacValue) -> String {
    match value {
        TacValue::Null => String::from("null"),
        TacValue::Flag(inner) => inner.to_string(),
        TacValue::Int(inner) => inner.to_string(),
        TacValue::Float(inner) => {
            if !inner.is_finite() {
                String::from("null")
            } else if inner.fract() == 0.0 && inner.abs() < 9_007_199_254_740_992.0 {
                (*inner as i64).to_string()
            } else {
                inner.to_string()
            }
        }
        TacValue::Text(inner) => tac_quote(inner),
        TacValue::List(values) => {
            let items: Vec<String> = values.iter().map(tac_write).collect();
            format!("[{}]", items.join(","))
        }
    }
}

// ── Protocol ──────────────────────────────────────────────────────────────

// Bound recursion before the scanner enters nested arrays.
fn tac_bounded_json(raw: &str) -> bool {
    if raw.len() > 65_536 { return false; }
    let mut depth = 0u8;
    let mut quoted = false;
    let mut escaped = false;
    for byte in raw.bytes() {
        if quoted {
            if escaped { escaped = false; }
            else if byte == b'\\' { escaped = true; }
            else if byte == b'"' { quoted = false; }
        } else {
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => { depth += 1; if depth > 64 { return false; } }
                b'}' | b']' => { if depth == 0 { return false; } depth -= 1; }
                _ => {}
            }
        }
    }
    depth == 0 && !quoted && raw.trim_start().starts_with('{')
}

fn tac_handle(raw: &str) -> String {
    if !tac_bounded_json(raw) {
        return String::from("{\"error\":\"Invalid or oversized companion request.\"}");
    }
    let request = TacScanner::new(raw).object();
    let read = |name: &str| request.iter().find(|(key, _)| key == name).map(|(_, value)| value);
    let Some(TacValue::Text(operation)) = read("op") else {
        return String::from("{\"error\":\"Companion request has no operation.\"}");
    };
    let Some(TacValue::Text(route)) = read("route") else {
        return String::from("{\"error\":\"Missing companion route.\",\"code\":\"TY_NATIVE_ROUTE\"}");
    };
    let Some(members) = tac_route_members(route) else {
        if operation == "init" { return String::from("{\"value\":{\"fields\":[],\"methods\":[]}}"); }
        return String::from("{\"error\":\"Unknown companion route.\",\"code\":\"TY_NATIVE_ROUTE\"}");
    };

    if operation == "init" {
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        for (name, member) in &members {
            match member {
                TacMember::Field { .. } => fields.push(TacValue::Text(String::from(*name))),
                TacMember::Method(_) => methods.push(TacValue::Text(String::from(*name))),
            }
        }
        return format!(
            "{{\"value\":{{\"fields\":{},\"methods\":{}}}}}",
            tac_write(&TacValue::List(fields)),
            tac_write(&TacValue::List(methods))
        );
    }

    let Some(TacValue::Text(name)) = read("name") else {
        return String::from("{\"error\":\"Companion request has no member name.\"}");
    };
    let Some((_, member)) = members.iter().find(|(key, _)| key == name) else {
        return format!("{{\"error\":{}}}", tac_quote(&format!("Unknown companion member: {name}")));
    };

    match (operation.as_str(), member) {
        ("get", TacMember::Field { read: get, .. }) => {
            format!("{{\"value\":{}}}", tac_write(&get()))
        }
        ("set", TacMember::Field { write: Some(set), .. }) => {
            set(TacScanner::new(raw).object().into_iter().find_map(|(key, value)| {
                (key == "value").then_some(value)
            }).unwrap_or(TacValue::Null));
            String::from("{\"value\":null}")
        }
        ("set", TacMember::Field { write: None, .. }) => {
            format!("{{\"error\":{}}}", tac_quote(&format!("Companion field is read-only: {name}")))
        }
        ("call", TacMember::Method(invoke)) => {
            let arguments = match TacScanner::new(raw)
                .object()
                .into_iter()
                .find_map(|(key, value)| (key == "args").then_some(value))
            {
                Some(TacValue::List(values)) => values,
                _ => Vec::new(),
            };
            format!("{{\"value\":{}}}", tac_write(&invoke(arguments)))
        }
        _ => format!(
            "{{\"error\":{}}}",
            tac_quote(&format!("Companion member does not support {operation}: {name}"))
        ),
    }
}

// The host's view of the companion: two ordinary C functions, which is all a
// Swift, C or Java host can call. The answer is allocated here and freed here,
// so neither side has to know how the other's allocator works.

/// Answers one companion request. The pointer returned must be handed back to
/// `tac_native_free`.
///
/// # Safety
///
/// `request` must be a NUL-terminated UTF-8 string, which is what every host
/// passes: the protocol is JSON text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tac_native_invoke(request: *const core::ffi::c_char) -> *mut core::ffi::c_char {
    let text = if request.is_null() {
        String::from("{}")
    } else {
        let bytes = unsafe { core::ffi::CStr::from_ptr(request) }.to_bytes();
        String::from_utf8_lossy(bytes).into_owned()
    };
    let answer = tac_handle(&text);
    match std::ffi::CString::new(answer) {
        Ok(owned) => owned.into_raw(),
        // A NUL inside the answer can only come from a companion returning one
        // in a string; the host reads an error rather than a truncated value.
        Err(_) => std::ffi::CString::new("{\"error\":\"Companion answer contains a NUL byte.\"}")
            .unwrap_or_default()
            .into_raw(),
    }
}

/// Frees one answer.
///
/// # Safety
///
/// `answer` must be a pointer this module returned and has not freed yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tac_native_free(answer: *mut core::ffi::c_char) {
    if !answer.is_null() {
        drop(unsafe { std::ffi::CString::from_raw(answer) });
    }
}

// ── Publishing to the page ────────────────────────────────────────────────
//
// Everything above is the page asking a question. This is the other direction,
// and the reason it exists: a companion watching something the platform tells
// it about — a power state, a device change, a file — has no question to
// answer, because nobody asked one.

/// The host's sink, installed through `tac_native_set_emit`.
///
/// A mutex rather than a `static mut`, because a companion may publish from a
/// thread it started itself. The pointer is copied out before the call, so a
/// sink that somehow published again would not deadlock on the lock it is
/// already inside.
static TAC_EMIT: std::sync::Mutex<Option<TacEmit>> = std::sync::Mutex::new(None);

/// What a host hands over: one UTF-8 JSON object, borrowed for the call.
type TacEmit = extern "C" fn(*const core::ffi::c_char);

/// Installs the host's sink. Passing null removes it.
///
/// # Safety
///
/// `emit` must stay callable for as long as this companion may publish, which
/// is what a host passing one of its own functions does.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tac_native_set_emit(emit: Option<TacEmit>) {
    if let Ok(mut installed) = TAC_EMIT.lock() {
        *installed = emit;
    }
}

/// Publishes a value to the page, where `@subscribe(name)` receives it.
///
/// A no-op until a host installs a sink, so the same companion source builds
/// for a target with no web view to publish into.
pub fn tac_publish(name: &str, value: TacValue) {
    let Some(emit) = TAC_EMIT.lock().ok().and_then(|installed| *installed) else {
        return;
    };
    let payload = format!("{{\"name\":{},\"value\":{}}}", tac_quote(name), tac_write(&value));
    // A NUL can only come from a companion putting one in a string. Dropping
    // that publish serves the page better than a truncated one.
    if let Ok(owned) = std::ffi::CString::new(payload) {
        emit(owned.as_ptr());
    }
}
