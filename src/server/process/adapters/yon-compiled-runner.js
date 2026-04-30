// @ts-check
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import path from 'path';

export default class YonCompiledRunner {
    /**
     * @param {string} value
     * @returns {string}
     */
    static safeId(value) {
        return String(Bun.hash(value)).replace(/[^0-9a-z]/gi, '');
    }

    /**
     * @param {string} language
     * @param {string} handlerPath
     * @returns {string}
     */
    static workspace(language, handlerPath) {
        const stats = statSync(handlerPath);
        const id = YonCompiledRunner.safeId(`${language}:${handlerPath}:${stats.mtimeMs}:${stats.size}`);
        const root = path.join(tmpdir(), 'tachyon-yon-handlers', id);
        mkdirSync(root, { recursive: true });
        return root;
    }

    /**
     * Writes a handler source file into a compiler workspace. Legacy Yon routes
     * may still start with a shebang, but static compilers reject that line
     * after we copy the route into a native source file such as `POST.java` or
     * `user.rs`.
     * @param {string} handlerPath
     * @param {string} targetPath
     * @returns {string}
     */
    static writeSourceWithoutShebang(handlerPath, targetPath) {
        const source = readFileSync(handlerPath, 'utf8').replace(/^#![^\r\n]*(?:\r?\n)?/, '');
        writeFileSync(targetPath, source);
        return source;
    }

    /**
     * Class-based compiled handlers use the HTTP method filename as the route
     * class name, e.g. `POST.java` -> `POST`.
     * @param {string} handlerPath
     * @returns {string}
     */
    static routeClassName(handlerPath) {
        return path.basename(handlerPath).split('.', 1)[0] || 'Handler';
    }

    /**
     * @param {string} handlerPath
     * @returns {string | null}
     */
    static servicesPath(handlerPath) {
        const normalized = path.resolve(handlerPath).replaceAll('\\', '/');
        const markerIndex = normalized.indexOf('/server/routes/');
        if (markerIndex === -1)
            return null;
        return path.join(normalized.slice(0, markerIndex), 'server', 'services');
    }

    /**
     * @param {string} language
     * @returns {string}
     */
    static serviceExtension(language) {
        if (language === 'csharp')
            return '.cs';
        if (language === 'rust')
            return '.rs';
        return `.${language}`;
    }

    /**
     * @param {string} language
     * @param {string} handlerPath
     * @param {string} root
     */
    static copyServiceSources(language, handlerPath, root) {
        const servicesPath = YonCompiledRunner.servicesPath(handlerPath);
        if (!servicesPath || !existsSync(servicesPath))
            return;
        const extension = YonCompiledRunner.serviceExtension(language);
        for (const entry of readdirSync(servicesPath, { withFileTypes: true })) {
            if (entry.isFile() && entry.name.endsWith(extension)) {
                copyFileSync(path.join(servicesPath, entry.name), path.join(root, entry.name));
            }
        }
    }

    /**
     * @param {string[]} cmd
     * @param {string} cwd
     * @param {string | null} input
     * @returns {Promise<string>}
     */
    static async runCommand(cmd, cwd, input = null) {
        const proc = Bun.spawn({
            cmd,
            cwd,
            stdin: input === null ? 'ignore' : 'pipe',
            stdout: 'pipe',
            stderr: 'pipe',
            env: process.env,
        });
        if (input !== null && proc.stdin) {
            proc.stdin.write(input);
            proc.stdin.end();
        }
        const [stdout, stderr, exitCode] = await Promise.all([
            new Response(proc.stdout).text(),
            new Response(proc.stderr).text(),
            proc.exited,
        ]);
        if (exitCode !== 0)
            throw new Error(stderr || stdout || `${cmd[0]} exited with code ${exitCode}`);
        return stdout;
    }

    /**
     * @returns {string | null}
     */
    static windowsVcVarsPath() {
        if (process.platform !== 'win32')
            return null;
        const candidates = [
            'C:/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Auxiliary/Build/vcvarsall.bat',
            'C:/Program Files/Microsoft Visual Studio/2022/BuildTools/VC/Auxiliary/Build/vcvarsall.bat',
            'C:/Program Files/Microsoft Visual Studio/2022/Community/VC/Auxiliary/Build/vcvarsall.bat',
            'C:/Program Files/Microsoft Visual Studio/2022/Professional/VC/Auxiliary/Build/vcvarsall.bat',
            'C:/Program Files/Microsoft Visual Studio/2022/Enterprise/VC/Auxiliary/Build/vcvarsall.bat',
        ];
        return candidates.find((candidate) => existsSync(candidate)) ?? null;
    }

    /**
     * @returns {string}
     */
    static windowsVcArch() {
        return 'x64';
    }

    /**
     * Windows ARM installations commonly include x64 MSVC build tools first.
     * Targeting x64 keeps Rust handlers usable on Windows ARM through emulation
     * without requiring the optional ARM64 Visual Studio libraries.
     * @returns {string[]}
     */
    static rustTargetArgs() {
        return process.platform === 'win32' && process.arch === 'arm64'
            ? ['--target', 'x86_64-pc-windows-msvc']
            : [];
    }

    /**
     * WinGet updates PATH for new shells, but long-running parent processes can
     * keep the old environment. Resolve Dart directly from the default package
     * location so installed adapters work immediately after setup.
     * @returns {string}
     */
    static dartExecutable() {
        if (process.platform !== 'win32')
            return 'dart';
        const localAppData = process.env.LOCALAPPDATA;
        const candidate = localAppData
            ? path.join(localAppData, 'Microsoft', 'WinGet', 'Packages', 'Google.DartSDK_Microsoft.Winget.Source_8wekyb3d8bbwe', 'dart-sdk', 'bin', 'dart.exe')
            : '';
        return candidate && existsSync(candidate) ? candidate : 'dart';
    }

    /**
     * @param {string} root
     * @param {string} binPath
     * @returns {Promise<void>}
     */
    static async compileRust(root, binPath) {
        const vcvars = YonCompiledRunner.windowsVcVarsPath();
        if (vcvars) {
            writeFileSync(path.join(root, 'build-rust.cmd'), [
                '@echo off',
                `call "${vcvars}" ${YonCompiledRunner.windowsVcArch()} >nul`,
                `set "PATH=${process.env.USERPROFILE}\\.cargo\\bin;%PATH%"`,
                `rustc ${YonCompiledRunner.rustTargetArgs().join(' ')} main.rs -o "${binPath}"`,
            ].join('\r\n'));
            await YonCompiledRunner.runCommand(['cmd', '/d', '/c', 'build-rust.cmd'], root);
            return;
        }
        await YonCompiledRunner.runCommand(['rustc', ...YonCompiledRunner.rustTargetArgs(), 'main.rs', '-o', binPath], root);
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runDart(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('dart', handlerPath);
        YonCompiledRunner.copyServiceSources('dart', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'user.dart'));
        writeFileSync(path.join(root, 'main.dart'), `import 'dart:convert';
import 'dart:io';
import 'user.dart' as user;

Future<void> main() async {
  final input = await stdin.transform(utf8.decoder).join();
  final request = jsonDecode(input.isEmpty ? '{}' : input) as Map<String, dynamic>;
  final result = await user.handler(request);
  if (result == null) return;
  stdout.write(result is String ? result : jsonEncode(result));
}
`);
        return YonCompiledRunner.runCommand([YonCompiledRunner.dartExecutable(), 'run', 'main.dart'], root, requestText);
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runGo(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('go', handlerPath);
        YonCompiledRunner.copyServiceSources('go', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'user.go'));
        writeFileSync(path.join(root, 'go.mod'), 'module yonhandler\n\ngo 1.22\n');
        writeFileSync(path.join(root, 'main.go'), `package main

import (
  "encoding/json"
  "fmt"
  "io"
  "os"
)

func main() {
  bytes, err := io.ReadAll(os.Stdin)
  if err != nil { panic(err) }
  request := map[string]any{}
  if len(bytes) > 0 {
    if err := json.Unmarshal(bytes, &request); err != nil { panic(err) }
  }
  result := Handler(request)
  if result == nil { return }
  if text, ok := result.(string); ok {
    fmt.Print(text)
    return
  }
  output, err := json.Marshal(result)
  if err != nil { panic(err) }
  os.Stdout.Write(output)
}
`);
        return YonCompiledRunner.runCommand(['go', 'run', '.'], root, requestText);
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runCSharp(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('csharp', handlerPath);
        const routeClassName = YonCompiledRunner.routeClassName(handlerPath);
        YonCompiledRunner.copyServiceSources('csharp', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, `${routeClassName}.cs`));
        writeFileSync(path.join(root, 'YonRoute.csproj'), `<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
</Project>
`);
        writeFileSync(path.join(root, 'Program.cs'), `using System.Collections.Generic;
using System.Reflection;
using System.Text.Json;

var input = await Console.In.ReadToEndAsync();
using var document = JsonDocument.Parse(string.IsNullOrWhiteSpace(input) ? "{}" : input);
var route = CreateRoute();
var handler = route.GetType().GetMethod("Handler", BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static)
    ?? throw new MissingMethodException("${routeClassName}", "Handler");
object? result = handler.Invoke(handler.IsStatic ? null : route, ResolveHandlerArguments(handler, document.RootElement));
if (result is null) return;
if (result is string text) {
    Console.Write(text);
    return;
}
Console.Write(JsonSerializer.Serialize(result));

static object CreateRoute()
{
    var routeType = typeof(${routeClassName});
    var constructors = routeType
        .GetConstructors()
        .OrderByDescending(constructor => constructor.GetParameters().Length);
    foreach (var constructor in constructors)
    {
        try
        {
            var args = constructor
                .GetParameters()
                .Select(parameter => ResolveDependency(parameter.ParameterType, default))
                .ToArray();
            return constructor.Invoke(args);
        }
        catch (InvalidOperationException)
        {
        }
    }

    return Activator.CreateInstance(routeType)
        ?? throw new InvalidOperationException("Unable to create ${routeClassName}.");
}

static object?[] ResolveHandlerArguments(MethodInfo handler, JsonElement request)
{
    return handler
        .GetParameters()
        .Select(parameter => ResolveDependency(parameter.ParameterType, request))
        .ToArray();
}

static object ResolveDependency(Type type, JsonElement request)
{
    if (type == typeof(JsonElement))
    {
        return request;
    }

    if (type == typeof(Func<JsonElement, Dictionary<string, object?>>))
    {
        return ResolveLanguageDescription();
    }

    throw new InvalidOperationException($"No dependency is registered for {type.FullName}.");
}

static Func<JsonElement, Dictionary<string, object?>> ResolveLanguageDescription()
{
    var serviceType = ResolveType("CSharpLanguageService");
    var service = Activator.CreateInstance(serviceType)
        ?? throw new InvalidOperationException("Unable to create CSharpLanguageService.");
    var describe = serviceType.GetMethod("Describe", new[] { typeof(JsonElement) })
        ?? throw new MissingMethodException("CSharpLanguageService", "Describe");
    return request =>
    {
        if (describe.Invoke(service, new object[] { request }) is Dictionary<string, object?> response)
        {
            return response;
        }

        throw new InvalidOperationException("CSharpLanguageService.Describe returned an invalid response.");
    };
}

static Type ResolveType(string name)
{
    foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
    {
        var type = assembly.GetType(name);
        if (type is not null)
        {
            return type;
        }
    }

    throw new InvalidOperationException($"{name} service is not available.");
}
`);
        return YonCompiledRunner.runCommand(['dotnet', 'run', '--project', 'YonRoute.csproj'], root, requestText);
    }

    /** @returns {string} */
    static javaJsonSupportSource() {
        return `import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class YonJson {
  private YonJson() {}

  public static Object parse(String input) {
    return new Parser(input == null || input.isBlank() ? "{}" : input).parse();
  }

  @SuppressWarnings("unchecked")
  public static Map<String, Object> parseObject(String input) {
    Object value = parse(input);
    if (value instanceof Map<?, ?> map) return (Map<String, Object>) map;
    throw new IllegalArgumentException("Expected Tachyon request JSON to be an object");
  }

  public static String stringify(Object value) {
    if (value == null) return "null";
    if (value instanceof String text) return quote(text);
    if (value instanceof Number || value instanceof Boolean) return value.toString();
    if (value instanceof Map<?, ?> map) {
      StringBuilder out = new StringBuilder("{");
      boolean first = true;
      for (Map.Entry<?, ?> entry : map.entrySet()) {
        if (!first) out.append(',');
        first = false;
        out.append(quote(String.valueOf(entry.getKey()))).append(':').append(stringify(entry.getValue()));
      }
      return out.append('}').toString();
    }
    if (value instanceof Iterable<?> items) {
      StringBuilder out = new StringBuilder("[");
      boolean first = true;
      for (Object item : items) {
        if (!first) out.append(',');
        first = false;
        out.append(stringify(item));
      }
      return out.append(']').toString();
    }
    return quote(value.toString());
  }

  private static String quote(String text) {
    StringBuilder out = new StringBuilder("\\"");
    for (int i = 0; i < text.length(); i += 1) {
      char ch = text.charAt(i);
      switch (ch) {
        case '"' -> out.append("\\\\\\"");
        case '\\\\' -> out.append("\\\\\\\\");
        case '\\b' -> out.append("\\\\b");
        case '\\f' -> out.append("\\\\f");
        case '\\n' -> out.append("\\\\n");
        case '\\r' -> out.append("\\\\r");
        case '\\t' -> out.append("\\\\t");
        default -> {
          if (ch < 0x20) out.append(String.format("\\\\u%04x", (int) ch));
          else out.append(ch);
        }
      }
    }
    return out.append('"').toString();
  }

  private static final class Parser {
    private final String text;
    private int index = 0;

    Parser(String text) { this.text = text; }

    Object parse() {
      Object value = readValue();
      skipWhitespace();
      if (index != text.length()) throw error("Unexpected trailing JSON");
      return value;
    }

    private Object readValue() {
      skipWhitespace();
      if (index >= text.length()) throw error("Unexpected end of JSON");
      char ch = text.charAt(index);
      if (ch == '{') return readObject();
      if (ch == '[') return readArray();
      if (ch == '"') return readString();
      if (ch == 't') return readLiteral("true", Boolean.TRUE);
      if (ch == 'f') return readLiteral("false", Boolean.FALSE);
      if (ch == 'n') return readLiteral("null", null);
      if (ch == '-' || Character.isDigit(ch)) return readNumber();
      throw error("Unexpected JSON token");
    }

    private Map<String, Object> readObject() {
      expect('{');
      Map<String, Object> object = new LinkedHashMap<>();
      skipWhitespace();
      if (peek('}')) { index += 1; return object; }
      while (true) {
        skipWhitespace();
        String key = readString();
        skipWhitespace();
        expect(':');
        object.put(key, readValue());
        skipWhitespace();
        if (peek('}')) { index += 1; return object; }
        expect(',');
      }
    }

    private List<Object> readArray() {
      expect('[');
      List<Object> array = new ArrayList<>();
      skipWhitespace();
      if (peek(']')) { index += 1; return array; }
      while (true) {
        array.add(readValue());
        skipWhitespace();
        if (peek(']')) { index += 1; return array; }
        expect(',');
      }
    }

    private String readString() {
      expect('"');
      StringBuilder out = new StringBuilder();
      while (index < text.length()) {
        char ch = text.charAt(index++);
        if (ch == '"') return out.toString();
        if (ch != '\\\\') { out.append(ch); continue; }
        if (index >= text.length()) throw error("Invalid escape sequence");
        char escaped = text.charAt(index++);
        switch (escaped) {
          case '"' -> out.append('"');
          case '\\\\' -> out.append('\\\\');
          case '/' -> out.append('/');
          case 'b' -> out.append('\\b');
          case 'f' -> out.append('\\f');
          case 'n' -> out.append('\\n');
          case 'r' -> out.append('\\r');
          case 't' -> out.append('\\t');
          case 'u' -> {
            if (index + 4 > text.length()) throw error("Invalid unicode escape");
            out.append((char) Integer.parseInt(text.substring(index, index + 4), 16));
            index += 4;
          }
          default -> throw error("Invalid escape sequence");
        }
      }
      throw error("Unterminated string");
    }

    private Object readNumber() {
      int start = index;
      if (peek('-')) index += 1;
      while (index < text.length() && Character.isDigit(text.charAt(index))) index += 1;
      boolean decimal = false;
      if (peek('.')) {
        decimal = true;
        index += 1;
        while (index < text.length() && Character.isDigit(text.charAt(index))) index += 1;
      }
      if (index < text.length() && (text.charAt(index) == 'e' || text.charAt(index) == 'E')) {
        decimal = true;
        index += 1;
        if (index < text.length() && (text.charAt(index) == '+' || text.charAt(index) == '-')) index += 1;
        while (index < text.length() && Character.isDigit(text.charAt(index))) index += 1;
      }
      String number = text.substring(start, index);
      return decimal ? Double.parseDouble(number) : Long.parseLong(number);
    }

    private Object readLiteral(String literal, Object value) {
      if (!text.startsWith(literal, index)) throw error("Invalid literal");
      index += literal.length();
      return value;
    }

    private void skipWhitespace() {
      while (index < text.length() && Character.isWhitespace(text.charAt(index))) index += 1;
    }

    private boolean peek(char expected) {
      return index < text.length() && text.charAt(index) == expected;
    }

    private void expect(char expected) {
      if (!peek(expected)) throw error("Expected '" + expected + "'");
      index += 1;
    }

    private IllegalArgumentException error(String message) {
      return new IllegalArgumentException(message + " at character " + index);
    }
  }
}
`;
    }

    /**
     * @param {string} handlerClassName
     * @returns {string}
     */
    static javaMainSource(handlerClassName) {
        return `import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.Map;

public class Main {
  public static void main(String[] args) throws Exception {
    Method handler = findHandler();
    Object request = YonJson.parseObject(new String(System.in.readAllBytes()));
    Object result = handler.invoke(null, request);
    if (result == null) return;
    if (result instanceof String text) System.out.print(text);
    else System.out.print(YonJson.stringify(result));
  }

  private static Method findHandler() {
    for (Method method : ${handlerClassName}.class.getDeclaredMethods()) {
      if (!method.getName().equals("handler")) continue;
      if (!Modifier.isStatic(method.getModifiers())) continue;
      if (method.getParameterCount() != 1) continue;
      Class<?> type = method.getParameterTypes()[0];
      if (type == Object.class || Map.class.isAssignableFrom(type)) {
        method.setAccessible(true);
        return method;
      }
    }
    throw new IllegalArgumentException("${handlerClassName}.handler must accept Object or Map<String, Object>");
  }
}
`;
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runJava(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('java', handlerPath);
        const routeClassName = YonCompiledRunner.routeClassName(handlerPath);
        YonCompiledRunner.copyServiceSources('java', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, `${routeClassName}.java`));
        writeFileSync(path.join(root, 'YonJson.java'), YonCompiledRunner.javaJsonSupportSource());
        writeFileSync(path.join(root, 'Main.java'), YonCompiledRunner.javaMainSource(routeClassName));
        const sources = readdirSync(root).filter((entry) => entry.endsWith('.java'));
        await YonCompiledRunner.runCommand(['javac', ...sources], root);
        return YonCompiledRunner.runCommand(['java', '-cp', root, 'Main'], root, requestText);
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runKotlin(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('kotlin', handlerPath);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'User.kt'));
        const jarPath = path.join(root, 'app.jar');
        writeFileSync(path.join(root, 'Main.kt'), `fun main() {
  val requestJson = generateSequence(::readLine).joinToString("\\n")
  val result = handler(requestJson)
  print(result)
}
`);
        await YonCompiledRunner.runCommand(['kotlinc', 'User.kt', 'Main.kt', '-include-runtime', '-d', jarPath], root);
        return YonCompiledRunner.runCommand(['java', '-jar', jarPath], root, requestText);
    }

    /** @returns {string} */
    static rustJsonSupportSource() {
        return `use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

impl JsonValue {
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(object) => object.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsonValue::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JsonValue::Number(value) => value.parse().ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Number(value) => value.parse().ok(),
            _ => None,
        }
    }
}

impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", stringify(self))
    }
}

pub fn parse(input: &str) -> Result<JsonValue, String> {
    let source = if input.trim().is_empty() { "{}" } else { input };
    let mut parser = Parser { chars: source.chars().collect(), index: 0 };
    let value = parser.read_value()?;
    parser.skip_whitespace();
    if parser.index != parser.chars.len() {
        return Err(format!("Unexpected trailing JSON at character {}", parser.index));
    }
    Ok(value)
}

pub fn stringify(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.clone(),
        JsonValue::String(value) => quote(value),
        JsonValue::Array(items) => {
            let body = items.iter().map(stringify).collect::<Vec<_>>().join(",");
            format!("[{}]", body)
        }
        JsonValue::Object(object) => {
            let body = object
                .iter()
                .map(|(key, value)| format!("{}:{}", quote(key), stringify(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{}}}", body)
        }
    }
}

fn quote(text: &str) -> String {
    let mut out = String::from("\\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\\\\""),
            '\\\\' => out.push_str("\\\\\\\\"),
            '\\u{08}' => out.push_str("\\\\b"),
            '\\u{0c}' => out.push_str("\\\\f"),
            '\\n' => out.push_str("\\\\n"),
            '\\r' => out.push_str("\\\\r"),
            '\\t' => out.push_str("\\\\t"),
            ch if ch < ' ' => out.push_str(&format!("\\\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

struct Parser {
    chars: Vec<char>,
    index: usize,
}

impl Parser {
    fn read_value(&mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('{') => self.read_object(),
            Some('[') => self.read_array(),
            Some('"') => self.read_string().map(JsonValue::String),
            Some('t') => self.read_literal("true", JsonValue::Bool(true)),
            Some('f') => self.read_literal("false", JsonValue::Bool(false)),
            Some('n') => self.read_literal("null", JsonValue::Null),
            Some('-') => self.read_number(),
            Some(ch) if ch.is_ascii_digit() => self.read_number(),
            Some(_) => Err(format!("Unexpected JSON token at character {}", self.index)),
            None => Err("Unexpected end of JSON".to_string()),
        }
    }

    fn read_object(&mut self) -> Result<JsonValue, String> {
        self.expect('{')?;
        let mut object = BTreeMap::new();
        self.skip_whitespace();
        if self.consume('}') { return Ok(JsonValue::Object(object)); }
        loop {
            self.skip_whitespace();
            let key = self.read_string()?;
            self.skip_whitespace();
            self.expect(':')?;
            object.insert(key, self.read_value()?);
            self.skip_whitespace();
            if self.consume('}') { return Ok(JsonValue::Object(object)); }
            self.expect(',')?;
        }
    }

    fn read_array(&mut self) -> Result<JsonValue, String> {
        self.expect('[')?;
        let mut array = Vec::new();
        self.skip_whitespace();
        if self.consume(']') { return Ok(JsonValue::Array(array)); }
        loop {
            array.push(self.read_value()?);
            self.skip_whitespace();
            if self.consume(']') { return Ok(JsonValue::Array(array)); }
            self.expect(',')?;
        }
    }

    fn read_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        while let Some(ch) = self.next() {
            if ch == '"' { return Ok(out); }
            if ch != '\\\\' {
                out.push(ch);
                continue;
            }
            let escaped = self.next().ok_or_else(|| "Invalid escape sequence".to_string())?;
            match escaped {
                '"' => out.push('"'),
                '\\\\' => out.push('\\\\'),
                '/' => out.push('/'),
                'b' => out.push('\\u{08}'),
                'f' => out.push('\\u{0c}'),
                'n' => out.push('\\n'),
                'r' => out.push('\\r'),
                't' => out.push('\\t'),
                'u' => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        hex.push(self.next().ok_or_else(|| "Invalid unicode escape".to_string())?);
                    }
                    let value = u32::from_str_radix(&hex, 16).map_err(|_| "Invalid unicode escape".to_string())?;
                    out.push(char::from_u32(value).ok_or_else(|| "Invalid unicode escape".to_string())?);
                }
                _ => return Err("Invalid escape sequence".to_string()),
            }
        }
        Err("Unterminated string".to_string())
    }

    fn read_number(&mut self) -> Result<JsonValue, String> {
        let start = self.index;
        self.consume('-');
        self.consume_digits();
        if self.consume('.') { self.consume_digits(); }
        if matches!(self.peek(), Some('e') | Some('E')) {
            self.index += 1;
            if matches!(self.peek(), Some('+') | Some('-')) { self.index += 1; }
            self.consume_digits();
        }
        Ok(JsonValue::Number(self.chars[start..self.index].iter().collect()))
    }

    fn read_literal(&mut self, literal: &str, value: JsonValue) -> Result<JsonValue, String> {
        for expected in literal.chars() {
            self.expect(expected)?;
        }
        Ok(value)
    }

    fn consume_digits(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_ascii_digit()) {
            self.index += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
            self.index += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn next(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.index += 1;
        Some(value)
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if self.consume(expected) {
            Ok(())
        } else {
            Err(format!("Expected '{}' at character {}", expected, self.index))
        }
    }
}
`;
    }

    /**
     * @param {string[]} modules
     * @returns {string}
     */
    static rustMainSource(modules = []) {
        const serviceModules = modules.map((module) => `pub mod ${module};`).join('\n');
        return `use std::io::{self, Read};

mod user;
pub mod yon_json;
${serviceModules}

fn main() {
    let mut request_json = String::new();
    io::stdin().read_to_string(&mut request_json).unwrap();
    let request = yon_json::parse(&request_json).expect("invalid Tachyon request JSON");
    let result = user::handler(&request);
    print!("{}", result);
}
`;
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runRust(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('rust', handlerPath);
        YonCompiledRunner.copyServiceSources('rust', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'user.rs'));
        const binPath = path.join(root, process.platform === 'win32' ? 'app.exe' : 'app');
        writeFileSync(path.join(root, 'yon_json.rs'), YonCompiledRunner.rustJsonSupportSource());
        const modules = readdirSync(root)
            .filter((entry) => entry.endsWith('.rs') && !['main.rs', 'user.rs', 'yon_json.rs'].includes(entry))
            .map((entry) => entry.slice(0, -3));
        writeFileSync(path.join(root, 'main.rs'), YonCompiledRunner.rustMainSource(modules));
        await YonCompiledRunner.compileRust(root, binPath);
        return YonCompiledRunner.runCommand([binPath], root, requestText);
    }

    static async run() {
        const language = process.argv[2];
        const handlerPath = process.argv[3];
        if (!language || !handlerPath)
            throw new Error('Usage: yon-compiled-runner.js <language> <handler-path>');
        if (!existsSync(handlerPath))
            throw new Error(`Handler not found: ${handlerPath}`);
        const requestText = await Bun.stdin.text();
        const output = language === 'dart'
            ? await YonCompiledRunner.runDart(handlerPath, requestText)
            : language === 'go'
                ? await YonCompiledRunner.runGo(handlerPath, requestText)
                : language === 'csharp'
                    ? await YonCompiledRunner.runCSharp(handlerPath, requestText)
                    : language === 'java'
                        ? await YonCompiledRunner.runJava(handlerPath, requestText)
                        : language === 'kotlin'
                            ? await YonCompiledRunner.runKotlin(handlerPath, requestText)
                            : language === 'rust'
                                ? await YonCompiledRunner.runRust(handlerPath, requestText)
                                : null;
        if (output === null)
            throw new Error(`Unsupported compiled handler language: ${language}`);
        Bun.stdout.write(output);
    }
}

if (import.meta.main) {
    YonCompiledRunner.run().catch((error) => {
        const message = error instanceof Error ? error.stack || error.message : String(error);
        Bun.stderr.write(message);
        process.exit(1);
    });
}
