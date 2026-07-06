// @ts-check
/**
 * Native Tac worker compiler. Takes handler-shaped source files and compiles
 * them to native executables (desktop) or host-native artifacts (mobile) using
 * the system toolchain for the target language.
 *
 * Desktop model: each worker file becomes an executable that reads a JSON
 * request from stdin and writes a JSON response envelope to stdout. The native
 * host spawns the executable per request.
 */

import { mkdtemp, readFile, rm, writeFile } from 'fs/promises';
import { tmpdir } from 'os';
import path from 'path';
import { existsSync } from 'fs';

/**
 * @typedef {import('../index.js').TacWorkerScript} TacWorkerScript
 */

/**
 * Compile a Tac worker source file to a native executable for a desktop target.
 * Currently supports Rust on macOS/Windows/Linux.
 *
 * @param {TacWorkerScript} worker
 * @param {string} outputPath — absolute path for the compiled executable
 * @param {{ target: string }} options
 * @returns {Promise<void>}
 */
export async function compileNativeWorkerExecutable(worker, outputPath, options) {
    const { language } = worker.provider;
    if (language !== 'rust') {
        throw new Error(`Native executable workers for '${language}' are not yet implemented on desktop targets.`);
    }
    await compileRustWorkerExecutable(worker, outputPath, options);
}

/**
 * @param {TacWorkerScript} worker
 * @param {string} outputPath
 * @param {{ target: string }} _options
 */
async function compileRustWorkerExecutable(worker, outputPath, _options) {
    const source = await readFile(worker.sourcePath, 'utf8');
    const methods = detectRustMethods(source);
    if (methods.length === 0) {
        throw new Error(`Tac worker '${worker.sourcePath}' defines no Handler HTTP methods.`);
    }
    const implBlock = extractHandlerImpl(source);
    if (!implBlock) {
        throw new Error(`Tac worker '${worker.sourcePath}' must contain an 'impl Handler { ... }' block.`);
    }

    const tmp = await mkdtemp(path.join(tmpdir(), 'tachyon-native-worker-'));
    try {
        await writeFile(path.join(tmp, 'yon_json.rs'), rustJsonSupportSource());
        await writeFile(path.join(tmp, 'main.rs'), rustWorkerMainSource(implBlock, methods));

        const binaryName = process.platform === 'win32' ? 'worker.exe' : 'worker';
        const intermediateBinary = path.join(tmp, binaryName);

        const rustc = process.env.RUSTC || 'rustc';
        const args = [
            rustc,
            '--edition=2021',
            '-O',
            '-o', intermediateBinary,
            'main.rs',
        ];

        const proc = Bun.spawn({
            cmd: args,
            cwd: tmp,
            stdout: 'pipe',
            stderr: 'pipe',
        });
        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ]);
        if (exitCode !== 0) {
            throw new Error(`rustc failed to compile Tac worker '${worker.sourcePath}':\n${stderr}\n${stdout}`);
        }

        await Bun.write(outputPath, await Bun.file(intermediateBinary).arrayBuffer());
        if (process.platform !== 'win32') {
            // chmod +x
            const { chmod } = await import('fs/promises');
            await chmod(outputPath, 0o755);
        }
    } finally {
        await rm(tmp, { recursive: true, force: true });
    }
}

/**
 * Extract the user's `impl Handler { ... }` block as-is.
 * @param {string} source
 * @returns {string | null}
 */
function extractHandlerImpl(source) {
    const startMatch = source.match(/impl\s+Handler\s*\{/);
    if (!startMatch || startMatch.index === undefined) return null;
    const start = startMatch.index + startMatch[0].length;
    let depth = 1;
    let end = start;
    while (end < source.length && depth > 0) {
        const ch = source[end];
        if (ch === '{') depth += 1;
        else if (ch === '}') depth -= 1;
        end += 1;
    }
    if (depth !== 0) return null;
    return source.slice(startMatch.index, end);
}

/**
 * @param {string} source
 * @returns {string[]}
 */
function detectRustMethods(source) {
    const HTTP_METHODS = ['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS'];
    const methods = [];
    for (const method of HTTP_METHODS) {
        const pattern = new RegExp(`\\b(?:pub\\s+)?fn\\s+${method}\\s*\\(`);
        if (pattern.test(source)) methods.push(method);
    }
    return methods;
}

/**
 * Generates the full main.rs for a native Rust worker.
 * @param {string} implBlock
 * @param {string[]} methods
 * @returns {string}
 */
function rustWorkerMainSource(implBlock, methods) {
    const dispatch = methods
        .map((method) => `        "${method}" => write_response(Handler::${method}(request.clone())),`)
        .join('\n');

    return `#![allow(non_snake_case)]
mod yon_json;
pub use yon_json::YonJson;

pub type Json = YonJson;

#[derive(Clone)]
pub struct Request {
    method: String,
    headers: YonJson,
    body: YonJson,
    query: YonJson,
    paths: YonJson,
}

impl Request {
    pub fn len(&self) -> i32 {
        match &self.body {
            YonJson::String(value) => value.len() as i32,
            other => other.stringify().len() as i32,
        }
    }

    pub fn body(&self) -> Json {
        self.body.clone()
    }

    pub fn json(&self) -> Json {
        YonJson::object(vec![
            ("method", YonJson::String(self.method.clone())),
            ("headers", self.headers.clone()),
            ("body", self.body.clone()),
            ("query", self.query.clone()),
            ("paths", self.paths.clone()),
        ])
    }

    pub fn query(&self, key: &str) -> String {
        self.lookup(&self.query, key)
    }

    pub fn path(&self, key: &str) -> String {
        self.lookup(&self.paths, key)
    }

    pub fn header(&self, key: &str) -> String {
        self.lookup(&self.headers, key)
    }

    pub fn platform(&self, _key: &str) -> String {
        String::new()
    }

    fn lookup(&self, source: &YonJson, key: &str) -> String {
        source.get(key).map(|value| value.as_string()).unwrap_or_default()
    }
}

pub fn json(value: Json) -> Json {
    value
}

struct Handler;

${implBlock}

fn write_response<T: Into<YonJson>>(value: T) -> Result<(), String> {
    let envelope = YonJson::object(vec![
        ("status", YonJson::Number(200.0)),
        ("headers", YonJson::object(vec![("Content-Type", YonJson::String("application/json".to_string()))])),
        ("body", YonJson::object(vec![("result", value.into())])),
    ]);
    println!("{}", envelope.stringify());
    Ok(())
}

fn main() -> Result<(), String> {
    use std::io::{self, Read};

    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| e.to_string())?;
    let payload = YonJson::parse(&input)?;
    let method = payload.get("method").map(|value| value.as_string()).unwrap_or_default();
    let request_value = payload.get("request").cloned().unwrap_or_else(|| YonJson::Object(std::collections::BTreeMap::new()));

    let request = Request {
        method: method.clone(),
        headers: request_value.get("headers").cloned().unwrap_or_else(|| YonJson::Object(std::collections::BTreeMap::new())),
        body: request_value.get("body").cloned().unwrap_or(YonJson::Null),
        query: request_value.get("query").cloned().unwrap_or_else(|| YonJson::Object(std::collections::BTreeMap::new())),
        paths: request_value.get("paths").cloned().unwrap_or_else(|| YonJson::Object(std::collections::BTreeMap::new())),
    };

    match method.as_str() {
${dispatch}
        _ => Err(format!("Handler class does not implement {}", method)),
    }
}
`;
}

/**
 * Minimal self-contained JSON parser/stringifier for native Rust workers.
 * Based on the Yon backend implementation.
 * @returns {string}
 */
function rustJsonSupportSource() {
    return `use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum YonJson {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(BTreeMap<String, YonJson>),
    Array(Vec<YonJson>),
}

impl YonJson {
    pub fn object(entries: Vec<(&str, YonJson)>) -> YonJson {
        let mut object = BTreeMap::new();
        for (key, value) in entries {
            object.insert(key.to_string(), value);
        }
        YonJson::Object(object)
    }

    pub fn parse(input: &str) -> Result<YonJson, String> {
        let payload = if input.trim().is_empty() { "{}" } else { input };
        Parser::new(payload).parse()
    }

    pub fn get(&self, key: &str) -> Option<&YonJson> {
        match self {
            YonJson::Object(object) => object.get(key),
            _ => None,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            YonJson::String(value) => value.clone(),
            YonJson::Number(value) => {
                if value.is_finite() && value.fract() == 0.0 {
                    format!("{}", *value as i64)
                } else {
                    format!("{}", value)
                }
            }
            YonJson::Bool(value) => value.to_string(),
            YonJson::Null => String::new(),
            _ => self.stringify(),
        }
    }

    pub fn stringify(&self) -> String {
        match self {
            YonJson::Null => "null".to_string(),
            YonJson::Bool(value) => value.to_string(),
            YonJson::Number(value) => stringify_number(*value),
            YonJson::String(value) => quote(value),
            YonJson::Object(object) => {
                let mut out = String::from("{");
                let mut first = true;
                for (key, value) in object {
                    if !first { out.push(','); }
                    first = false;
                    out.push_str(&quote(key));
                    out.push(':');
                    out.push_str(&value.stringify());
                }
                out.push('}');
                out
            }
            YonJson::Array(array) => {
                let mut out = String::from("[");
                let mut first = true;
                for value in array {
                    if !first { out.push(','); }
                    first = false;
                    out.push_str(&value.stringify());
                }
                out.push(']');
                out
            }
        }
    }
}

impl From<i32> for YonJson {
    fn from(value: i32) -> Self { YonJson::Number(value as f64) }
}

impl From<u32> for YonJson {
    fn from(value: u32) -> Self { YonJson::Number(value as f64) }
}

impl From<i64> for YonJson {
    fn from(value: i64) -> Self { YonJson::Number(value as f64) }
}

impl From<u64> for YonJson {
    fn from(value: u64) -> Self { YonJson::Number(value as f64) }
}

impl From<f64> for YonJson {
    fn from(value: f64) -> Self { YonJson::Number(value) }
}

impl From<bool> for YonJson {
    fn from(value: bool) -> Self { YonJson::Bool(value) }
}

impl From<String> for YonJson {
    fn from(value: String) -> Self { YonJson::String(value) }
}

impl From<&str> for YonJson {
    fn from(value: &str) -> Self { YonJson::String(value.to_string()) }
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\\\' => { out.push('\\\\'); out.push('\\\\'); }
            '"' => { out.push('\\\\'); out.push('"'); }
            '\\n' => { out.push('\\\\'); out.push('n'); }
            '\\r' => { out.push('\\\\'); out.push('r'); }
            '\\t' => { out.push('\\\\'); out.push('t'); }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn stringify_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{}", value)
    }
}

struct Parser {
    input: Vec<char>,
    index: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Parser { input: input.chars().collect(), index: 0 }
    }

    fn parse(&mut self) -> Result<YonJson, String> {
        self.skip_whitespace();
        let value = self.read_value()?;
        self.skip_whitespace();
        if self.index < self.input.len() {
            return Err(format!("Unexpected character at {}", self.index));
        }
        Ok(value)
    }

    fn current(&self) -> Option<char> {
        self.input.get(self.index).copied()
    }

    fn advance(&mut self) {
        self.index += 1;
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current() {
            if ch.is_whitespace() { self.advance(); } else { break; }
        }
    }

    fn read_value(&mut self) -> Result<YonJson, String> {
        self.skip_whitespace();
        match self.current() {
            Some('{') => self.read_object(),
            Some('[') => self.read_array(),
            Some('"') => self.read_string(),
            Some('t') => self.read_literal("true", YonJson::Bool(true)),
            Some('f') => self.read_literal("false", YonJson::Bool(false)),
            Some('n') => self.read_literal("null", YonJson::Null),
            Some(ch) if ch == '-' || ch.is_ascii_digit() => self.read_number(),
            _ => Err(format!("Unexpected character at {}", self.index)),
        }
    }

    fn read_object(&mut self) -> Result<YonJson, String> {
        self.advance(); // {
        let mut object = BTreeMap::new();
        self.skip_whitespace();
        if self.current() == Some('}') {
            self.advance();
            return Ok(YonJson::Object(object));
        }
        loop {
            self.skip_whitespace();
            let key = match self.read_value()? {
                YonJson::String(value) => value,
                _ => return Err("Object key must be a string".to_string()),
            };
            self.skip_whitespace();
            if self.current() != Some(':') {
                return Err("Expected ':' after object key".to_string());
            }
            self.advance();
            let value = self.read_value()?;
            object.insert(key, value);
            self.skip_whitespace();
            match self.current() {
                Some(',') => { self.advance(); }
                Some('}') => { self.advance(); break; }
                _ => return Err("Expected ',' or '}' in object".to_string()),
            }
        }
        Ok(YonJson::Object(object))
    }

    fn read_array(&mut self) -> Result<YonJson, String> {
        self.advance(); // [
        let mut array = Vec::new();
        self.skip_whitespace();
        if self.current() == Some(']') {
            self.advance();
            return Ok(YonJson::Array(array));
        }
        loop {
            let value = self.read_value()?;
            array.push(value);
            self.skip_whitespace();
            match self.current() {
                Some(',') => { self.advance(); }
                Some(']') => { self.advance(); break; }
                _ => return Err("Expected ',' or ']' in array".to_string()),
            }
        }
        Ok(YonJson::Array(array))
    }

    fn read_string(&mut self) -> Result<YonJson, String> {
        self.advance(); // \"
        let mut out = String::new();
        loop {
            match self.current() {
                Some('"') => { self.advance(); break; }
                Some('\\\\') => {
                    self.advance();
                    match self.current() {
                        Some('"') => out.push('"'),
                        Some('\\\\') => out.push('\\\\'),
                        Some('/') => out.push('/'),
                        Some('b') => out.push('\\u{0008}'),
                        Some('f') => out.push('\\u{000C}'),
                        Some('n') => out.push('\\n'),
                        Some('r') => out.push('\\r'),
                        Some('t') => out.push('\\t'),
                        Some('u') => {
                            self.advance();
                            let mut code = String::new();
                            for _ in 0..4 {
                                match self.current() {
                                    Some(ch) if ch.is_ascii_hexdigit() => { code.push(ch); self.advance(); }
                                    _ => return Err("Invalid unicode escape".to_string()),
                                }
                            }
                            let value = u32::from_str_radix(&code, 16).map_err(|e| e.to_string())?;
                            out.push(char::from_u32(value).ok_or("Invalid unicode value")?);
                            continue;
                        }
                        _ => return Err("Invalid escape".to_string()),
                    }
                    self.advance();
                }
                Some(ch) => { out.push(ch); self.advance(); }
                None => return Err("Unterminated string".to_string()),
            }
        }
        Ok(YonJson::String(out))
    }

    fn read_literal(&mut self, expected: &str, value: YonJson) -> Result<YonJson, String> {
        for ch in expected.chars() {
            if self.current() != Some(ch) {
                return Err(format!("Expected '{}' at {}", expected, self.index));
            }
            self.advance();
        }
        Ok(value)
    }

    fn read_number(&mut self) -> Result<YonJson, String> {
        let start = self.index;
        if self.current() == Some('-') { self.advance(); }
        while let Some(ch) = self.current() {
            if ch.is_ascii_digit() { self.advance(); } else { break; }
        }
        if self.current() == Some('.') {
            self.advance();
            while let Some(ch) = self.current() {
                if ch.is_ascii_digit() { self.advance(); } else { break; }
            }
        }
        if let Some(ch) = self.current() {
            if ch == 'e' || ch == 'E' {
                self.advance();
                if let Some(sign) = self.current() {
                    if sign == '+' || sign == '-' { self.advance(); }
                }
                while let Some(ch) = self.current() {
                    if ch.is_ascii_digit() { self.advance(); } else { break; }
                }
            }
        }
        let text: String = self.input[start..self.index].iter().collect();
        text.parse::<f64>().map(YonJson::Number).map_err(|e| e.to_string())
    }
}
`;
}

/**
 * Returns true when the given command is available on this machine.
 * @param {string} command
 * @returns {boolean}
 */
export function commandAvailable(command) {
    return typeof Bun.which === 'function' && Bun.which(command) !== null;
}

/**
 * Returns true when rustc is available.
 * @returns {boolean}
 */
export function rustAvailable() {
    return commandAvailable('rustc');
}
