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
     * after we copy the route into a native source file.
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
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runDart(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('dart', handlerPath);
        YonCompiledRunner.copyServiceSources('dart', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'handler.dart'));
        writeFileSync(path.join(root, 'main.dart'), `import 'dart:convert';
import 'dart:io';
import 'dart:mirrors';
import 'handler.dart' as user;

Future<void> main() async {
  final input = await stdin.transform(utf8.decoder).join();
  final request = jsonDecode(input.isEmpty ? '{}' : input) as Map<String, dynamic>;
  final method = request['method'] as String?;
  if (method == null || method.isEmpty) {
    throw Exception('Missing HTTP method in request payload');
  }
  final handlerMirror = reflectClass(user.Handler);
  final symbol = Symbol(method);
  if (!handlerMirror.staticMembers.containsKey(symbol)) {
    throw Exception('Handler class does not implement static \$method()');
  }
  final result = handlerMirror.invoke(symbol, [request]).reflectee;
  final resolved = result is Future ? await result : result;
  if (resolved == null) return;
  stdout.write(resolved is String ? resolved : jsonEncode(resolved));
}
`);
        return YonCompiledRunner.runCommand([YonCompiledRunner.dartExecutable(), 'run', 'main.dart'], root, requestText);
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runCSharp(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('csharp', handlerPath);
        YonCompiledRunner.copyServiceSources('csharp', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'Handler.cs'));
        writeFileSync(path.join(root, 'YonRoute.csproj'), `<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
</Project>
`);
        writeFileSync(path.join(root, 'Program.cs'), `using System.Reflection;
using System.Text.Json;

var input = await Console.In.ReadToEndAsync();
using var document = JsonDocument.Parse(string.IsNullOrWhiteSpace(input) ? "{}" : input);
var method = document.RootElement.TryGetProperty("method", out var methodElement)
    ? methodElement.GetString()
    : null;
if (string.IsNullOrEmpty(method))
    throw new InvalidOperationException("Missing HTTP method in request payload");

var handlerType = typeof(Handler);
var dispatch = handlerType.GetMethod(method, BindingFlags.Public | BindingFlags.Static)
    ?? throw new MissingMethodException("Handler", method);

object? result = dispatch.Invoke(null, new object[] { document.RootElement });
if (result is null) return;
if (result is string text)
{
    Console.Write(text);
    return;
}
Console.Write(JsonSerializer.Serialize(result));
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
     * @returns {string}
     */
    static javaMainSource() {
        return `import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.Map;

public class Main {
  public static void main(String[] args) throws Exception {
    Map<String, Object> request = YonJson.parseObject(new String(System.in.readAllBytes()));
    String method = (String) request.get("method");
    if (method == null || method.isEmpty()) {
      throw new IllegalArgumentException("Missing HTTP method in request payload");
    }
    Method dispatch = findMethod(method);
    Object result = dispatch.invoke(null, request);
    if (result == null) return;
    if (result instanceof String text) System.out.print(text);
    else System.out.print(YonJson.stringify(result));
  }

  private static Method findMethod(String httpMethod) {
    for (Method method : Handler.class.getDeclaredMethods()) {
      if (!method.getName().equals(httpMethod)) continue;
      if (!Modifier.isStatic(method.getModifiers())) continue;
      if (method.getParameterCount() != 1) continue;
      Class<?> type = method.getParameterTypes()[0];
      if (type == Object.class || Map.class.isAssignableFrom(type)) {
        method.setAccessible(true);
        return method;
      }
    }
    throw new IllegalArgumentException("Handler class does not implement static " + httpMethod + "()");
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
        YonCompiledRunner.copyServiceSources('java', handlerPath, root);
        YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'Handler.java'));
        writeFileSync(path.join(root, 'YonJson.java'), YonCompiledRunner.javaJsonSupportSource());
        writeFileSync(path.join(root, 'Main.java'), YonCompiledRunner.javaMainSource());
        const sources = readdirSync(root).filter((entry) => entry.endsWith('.java'));
        await YonCompiledRunner.runCommand(['javac', ...sources], root);
        return YonCompiledRunner.runCommand(['java', '-cp', root, 'Main'], root, requestText);
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
            : language === 'csharp'
                ? await YonCompiledRunner.runCSharp(handlerPath, requestText)
                : language === 'java'
                    ? await YonCompiledRunner.runJava(handlerPath, requestText)
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
