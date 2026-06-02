// @ts-check
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import path from 'path';

export default class YonCompiledRunner {
    static cacheVersion = 'yon-compiled-runner:v2';
    static buildLockStaleMs = 120_000;
    static httpMethods = Object.freeze(['GET', 'POST', 'PUT', 'DELETE', 'PATCH', 'HEAD', 'OPTIONS']);

    /**
     * @param {string} value
     * @returns {string}
     */
    static safeId(value) {
        return String(Bun.hash(value)).replace(/[^0-9a-z]/gi, '');
    }

    /** @returns {boolean} */
    static isProduction() {
        return process.env.NODE_ENV === 'production';
    }

    /**
     * @param {string} language
     * @param {string} handlerPath
     * @returns {string[]}
     */
    static serviceSourcePaths(language, handlerPath) {
        const sourceDirs = YonCompiledRunner.companionSourceDirs(handlerPath);
        const extension = YonCompiledRunner.serviceExtension(language);
        return sourceDirs.flatMap((sourceDir) => {
            if (!existsSync(sourceDir))
                return [];
            return readdirSync(sourceDir, { withFileTypes: true })
                .filter((entry) => entry.isFile() && entry.name.endsWith(extension))
                .map((entry) => path.join(sourceDir, entry.name));
        })
            .sort();
    }

    /**
     * @param {string} language
     * @param {string} handlerPath
     * @returns {string}
     */
    static cacheKey(language, handlerPath) {
        const parts = [
            YonCompiledRunner.cacheVersion,
            language,
            path.resolve(handlerPath),
        ];
        for (const sourcePath of [handlerPath, ...YonCompiledRunner.serviceSourcePaths(language, handlerPath)]) {
            const stats = statSync(sourcePath);
            const contentHash = YonCompiledRunner.safeId(readFileSync(sourcePath, 'utf8'));
            parts.push(`${path.resolve(sourcePath)}:${stats.mtimeMs}:${stats.size}:${contentHash}`);
        }
        return YonCompiledRunner.safeId(parts.join('|'));
    }

    /**
     * @param {string} language
     * @param {string} handlerPath
     * @returns {string}
     */
    static workspace(language, handlerPath) {
        const stats = statSync(handlerPath);
        const id = YonCompiledRunner.isProduction()
            ? YonCompiledRunner.cacheKey(language, handlerPath)
            : YonCompiledRunner.safeId(`${language}:${handlerPath}:${stats.mtimeMs}:${stats.size}`);
        const root = path.join(tmpdir(), 'tachyon-yon-handlers', id);
        mkdirSync(root, { recursive: true });
        return root;
    }

    /**
     * @param {string} root
     * @param {string} markerName
     * @returns {boolean}
     */
    static productionArtifactReady(root, markerName) {
        return YonCompiledRunner.isProduction() && existsSync(path.join(root, markerName));
    }

    /**
     * @param {string} root
     * @param {string} markerName
     */
    static markProductionArtifactReady(root, markerName) {
        if (YonCompiledRunner.isProduction())
            writeFileSync(path.join(root, markerName), YonCompiledRunner.cacheVersion);
    }

    /**
     * @param {number} milliseconds
     * @returns {Promise<void>}
     */
    static sleep(milliseconds) {
        return new Promise((resolve) => setTimeout(resolve, milliseconds));
    }

    /**
     * @param {string} root
     * @param {string} markerName
     * @param {() => Promise<void>} build
     * @returns {Promise<void>}
     */
    static async prepareArtifact(root, markerName, build) {
        if (!YonCompiledRunner.isProduction()) {
            await build();
            return;
        }
        if (YonCompiledRunner.productionArtifactReady(root, markerName))
            return;
        const lockPath = path.join(root, '.yon-build.lock');
        let ownsLock = false;
        while (!ownsLock) {
            if (YonCompiledRunner.productionArtifactReady(root, markerName))
                return;
            try {
                mkdirSync(lockPath);
                ownsLock = true;
            }
            catch {
                try {
                    const lockAgeMs = Date.now() - statSync(lockPath).mtimeMs;
                    if (lockAgeMs > YonCompiledRunner.buildLockStaleMs)
                        rmSync(lockPath, { recursive: true, force: true });
                }
                catch { /* lock disappeared between attempts */ }
                await YonCompiledRunner.sleep(25);
            }
        }
        try {
            if (!YonCompiledRunner.productionArtifactReady(root, markerName)) {
                await build();
                YonCompiledRunner.markProductionArtifactReady(root, markerName);
            }
        }
        finally {
            rmSync(lockPath, { recursive: true, force: true });
        }
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
     * @param {string} handlerPath
     * @returns {string[]}
     */
    static companionSourceDirs(handlerPath) {
        const normalized = path.resolve(handlerPath).replaceAll('\\', '/');
        const markerIndex = normalized.indexOf('/server/routes/');
        if (markerIndex === -1)
            return [];
        const serverRoot = path.join(normalized.slice(0, markerIndex), 'server');
        return [
            path.join(serverRoot, 'repositories'),
            path.join(serverRoot, 'services'),
        ];
    }

    /**
     * @param {string} language
     * @returns {string}
     */
    static serviceExtension(language) {
        if (language === 'csharp')
            return '.cs';
        if (language === 'cpp')
            return '.cpp';
        if (language === 'kotlin')
            return '.kt';
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
        const extension = YonCompiledRunner.serviceExtension(language);
        for (const sourceDir of YonCompiledRunner.companionSourceDirs(handlerPath)) {
            if (!existsSync(sourceDir))
                continue;
            for (const entry of readdirSync(sourceDir, { withFileTypes: true })) {
                if (entry.isFile() && entry.name.endsWith(extension)) {
                    copyFileSync(path.join(sourceDir, entry.name), path.join(root, entry.name));
                }
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
     * @param {string} command
     * @returns {boolean}
     */
    static commandExists(command) {
        try {
            return Bun.spawnSync({ cmd: [command, '--version'], stdout: 'pipe', stderr: 'pipe' }).exitCode === 0;
        }
        catch {
            return false;
        }
    }

    /** @returns {string} */
    static cppExecutable() {
        if (YonCompiledRunner.commandExists('clang++'))
            return 'clang++';
        if (YonCompiledRunner.commandExists('g++'))
            return 'g++';
        return 'c++';
    }

    /** @returns {string} */
    static swiftExecutable() {
        return 'swiftc';
    }

    /** @returns {string} */
    static kotlinExecutable() {
        return 'kotlinc';
    }

    /** @returns {string} */
    static rustExecutable() {
        return 'rustc';
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runDart(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('dart', handlerPath);
        const marker = '.yon-dart-production-ready';
        await YonCompiledRunner.prepareArtifact(root, marker, async () => {
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
            if (YonCompiledRunner.isProduction()) {
                await YonCompiledRunner.runCommand([YonCompiledRunner.dartExecutable(), 'compile', 'kernel', '--verbosity=error', '-o', 'main.dill', 'main.dart'], root);
            }
        });
        return YonCompiledRunner.isProduction()
            ? YonCompiledRunner.runCommand([YonCompiledRunner.dartExecutable(), 'run', 'main.dill'], root, requestText)
            : YonCompiledRunner.runCommand([YonCompiledRunner.dartExecutable(), 'run', 'main.dart'], root, requestText);
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runCSharp(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('csharp', handlerPath);
        const marker = '.yon-csharp-production-ready';
        await YonCompiledRunner.prepareArtifact(root, marker, async () => {
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
            if (YonCompiledRunner.isProduction()) {
                await YonCompiledRunner.runCommand(['dotnet', 'publish', 'YonRoute.csproj', '-c', 'Release', '-o', 'publish', '--nologo'], root);
            }
        });
        return YonCompiledRunner.isProduction()
            ? YonCompiledRunner.runCommand(['dotnet', path.join(root, 'publish', 'YonRoute.dll')], root, requestText)
            : YonCompiledRunner.runCommand(['dotnet', 'run', '--project', 'YonRoute.csproj'], root, requestText);
    }

    /** @returns {string} */
    static cppJsonSupportSource() {
        return `#pragma once
#include <cctype>
#include <cmath>
#include <cstddef>
#include <iomanip>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <variant>
#include <vector>

class YonJson {
public:
    using Object = std::map<std::string, YonJson>;
    using Array = std::vector<YonJson>;
    using Value = std::variant<std::nullptr_t, bool, double, std::string, Object, Array>;

    YonJson() : value_(nullptr) {}
    YonJson(std::nullptr_t) : value_(nullptr) {}
    YonJson(bool value) : value_(value) {}
    YonJson(int value) : value_(static_cast<double>(value)) {}
    YonJson(long value) : value_(static_cast<double>(value)) {}
    YonJson(long long value) : value_(static_cast<double>(value)) {}
    YonJson(double value) : value_(value) {}
    YonJson(const char* value) : value_(std::string(value == nullptr ? "" : value)) {}
    YonJson(std::string value) : value_(std::move(value)) {}
    YonJson(Object value) : value_(std::move(value)) {}
    YonJson(Array value) : value_(std::move(value)) {}

    static YonJson object(std::initializer_list<std::pair<std::string, YonJson>> entries) {
        Object out;
        for (const auto& entry : entries) out.emplace(entry.first, entry.second);
        return YonJson(std::move(out));
    }

    static YonJson array(std::initializer_list<YonJson> entries) {
        return YonJson(Array(entries));
    }

    static YonJson parse(const std::string& input) {
        Parser parser(input.empty() ? "{}" : input);
        return parser.parse();
    }

    const YonJson* get(const std::string& key) const {
        const auto* object = std::get_if<Object>(&value_);
        if (object == nullptr) return nullptr;
        auto match = object->find(key);
        return match == object->end() ? nullptr : &match->second;
    }

    std::string asString(const std::string& fallback = "") const {
        if (const auto* text = std::get_if<std::string>(&value_)) return *text;
        if (const auto* number = std::get_if<double>(&value_)) {
            if (std::isfinite(*number) && std::floor(*number) == *number) {
                return std::to_string(static_cast<long long>(*number));
            }
            std::ostringstream out;
            out << std::setprecision(15) << *number;
            return out.str();
        }
        if (const auto* flag = std::get_if<bool>(&value_)) return *flag ? "true" : "false";
        return fallback;
    }

    std::string stringify() const {
        if (std::holds_alternative<std::nullptr_t>(value_)) return "null";
        if (const auto* flag = std::get_if<bool>(&value_)) return *flag ? "true" : "false";
        if (const auto* number = std::get_if<double>(&value_)) return stringifyNumber(*number);
        if (const auto* text = std::get_if<std::string>(&value_)) return quote(*text);
        if (const auto* object = std::get_if<Object>(&value_)) {
            std::string out = "{";
            bool first = true;
            for (const auto& entry : *object) {
                if (!first) out += ",";
                first = false;
                out += quote(entry.first);
                out += ":";
                out += entry.second.stringify();
            }
            out += "}";
            return out;
        }
        const auto& array = std::get<Array>(value_);
        std::string out = "[";
        bool first = true;
        for (const auto& entry : array) {
            if (!first) out += ",";
            first = false;
            out += entry.stringify();
        }
        out += "]";
        return out;
    }

private:
    class Parser {
    public:
        explicit Parser(std::string text) : text_(std::move(text)) {}

        YonJson parse() {
            YonJson value = readValue();
            skipWhitespace();
            if (index_ != text_.size()) throw error("Unexpected trailing JSON");
            return value;
        }

    private:
        YonJson readValue() {
            skipWhitespace();
            if (index_ >= text_.size()) throw error("Unexpected end of JSON");
            const char current = text_[index_];
            if (current == '{') return readObject();
            if (current == '[') return readArray();
            if (current == '"') return YonJson(readString());
            if (current == 't') return readLiteral("true", YonJson(true));
            if (current == 'f') return readLiteral("false", YonJson(false));
            if (current == 'n') return readLiteral("null", YonJson(nullptr));
            if (current == '-' || std::isdigit(static_cast<unsigned char>(current))) return readNumber();
            throw error("Unexpected JSON token");
        }

        YonJson readObject() {
            expect('{');
            Object object;
            skipWhitespace();
            if (peek('}')) {
                index_ += 1;
                return YonJson(std::move(object));
            }
            while (true) {
                skipWhitespace();
                std::string key = readString();
                skipWhitespace();
                expect(':');
                object.emplace(std::move(key), readValue());
                skipWhitespace();
                if (peek('}')) {
                    index_ += 1;
                    return YonJson(std::move(object));
                }
                expect(',');
            }
        }

        YonJson readArray() {
            expect('[');
            Array array;
            skipWhitespace();
            if (peek(']')) {
                index_ += 1;
                return YonJson(std::move(array));
            }
            while (true) {
                array.push_back(readValue());
                skipWhitespace();
                if (peek(']')) {
                    index_ += 1;
                    return YonJson(std::move(array));
                }
                expect(',');
            }
        }

        std::string readString() {
            expect('"');
            std::string out;
            while (index_ < text_.size()) {
                char current = text_[index_++];
                if (current == '"') return out;
                if (current != '\\\\') {
                    out += current;
                    continue;
                }
                if (index_ >= text_.size()) throw error("Invalid escape sequence");
                char escaped = text_[index_++];
                switch (escaped) {
                    case '"': out += '"'; break;
                    case '\\\\': out += '\\\\'; break;
                    case '/': out += '/'; break;
                    case 'b': out += '\\b'; break;
                    case 'f': out += '\\f'; break;
                    case 'n': out += '\\n'; break;
                    case 'r': out += '\\r'; break;
                    case 't': out += '\\t'; break;
                    case 'u':
                        if (index_ + 4 > text_.size()) throw error("Invalid unicode escape");
                        out += '?';
                        index_ += 4;
                        break;
                    default:
                        throw error("Invalid escape sequence");
                }
            }
            throw error("Unterminated string");
        }

        YonJson readNumber() {
            const size_t start = index_;
            if (peek('-')) index_ += 1;
            while (index_ < text_.size() && std::isdigit(static_cast<unsigned char>(text_[index_]))) index_ += 1;
            if (peek('.')) {
                index_ += 1;
                while (index_ < text_.size() && std::isdigit(static_cast<unsigned char>(text_[index_]))) index_ += 1;
            }
            if (index_ < text_.size() && (text_[index_] == 'e' || text_[index_] == 'E')) {
                index_ += 1;
                if (index_ < text_.size() && (text_[index_] == '+' || text_[index_] == '-')) index_ += 1;
                while (index_ < text_.size() && std::isdigit(static_cast<unsigned char>(text_[index_]))) index_ += 1;
            }
            return YonJson(std::stod(text_.substr(start, index_ - start)));
        }

        YonJson readLiteral(const char* literal, YonJson value) {
            const std::string expected(literal);
            if (text_.compare(index_, expected.size(), expected) != 0) throw error("Invalid literal");
            index_ += expected.size();
            return value;
        }

        void skipWhitespace() {
            while (index_ < text_.size() && std::isspace(static_cast<unsigned char>(text_[index_]))) index_ += 1;
        }

        bool peek(char expected) const {
            return index_ < text_.size() && text_[index_] == expected;
        }

        void expect(char expected) {
            if (!peek(expected)) throw error(std::string("Expected '") + expected + "'");
            index_ += 1;
        }

        std::runtime_error error(const std::string& message) const {
            return std::runtime_error(message + " at character " + std::to_string(index_));
        }

        std::string text_;
        size_t index_ = 0;
    };

    static std::string stringifyNumber(double value) {
        if (std::isfinite(value) && std::floor(value) == value) {
            return std::to_string(static_cast<long long>(value));
        }
        std::ostringstream out;
        out << std::setprecision(15) << value;
        return out.str();
    }

    static std::string quote(const std::string& text) {
        std::string out = "\\"";
        for (const char current : text) {
            switch (current) {
                case '"': out += "\\\\\\""; break;
                case '\\\\': out += "\\\\\\\\"; break;
                case '\\b': out += "\\\\b"; break;
                case '\\f': out += "\\\\f"; break;
                case '\\n': out += "\\\\n"; break;
                case '\\r': out += "\\\\r"; break;
                case '\\t': out += "\\\\t"; break;
                default:
                    if (static_cast<unsigned char>(current) < 0x20) {
                        std::ostringstream escaped;
                        escaped << "\\\\u" << std::hex << std::setw(4) << std::setfill('0') << static_cast<int>(current);
                        out += escaped.str();
                    } else {
                        out += current;
                    }
            }
        }
        out += "\\"";
        return out;
    }

    Value value_;
};
`;
    }

    /**
     * @param {string} source
     * @returns {string[]}
     */
    static cppMethods(source) {
        return YonCompiledRunner.httpMethods.filter((method) => new RegExp(`\\bstatic\\s+[\\w:<>,?\\[\\]&*\\s]+\\s+${method}\\s*\\(`).test(source));
    }

    /**
     * @param {string[]} methods
     * @returns {string}
     */
    static cppMainSource(methods) {
        const dispatch = methods.map((method) => `    if (method == "${method}") { writeResponse(Handler::${method}(request)); return 0; }`).join('\n');
        return `#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include "YonJson.hpp"
#include "Handler.cpp"

void writeResponse(const YonJson& value) {
    std::cout << value.stringify();
}

void writeResponse(const std::string& value) {
    std::cout << value;
}

void writeResponse(const char* value) {
    if (value != nullptr) std::cout << value;
}

template <typename T>
void writeResponse(const T& value) {
    std::cout << YonJson(value).stringify();
}

int main() {
    std::ostringstream input;
    input << std::cin.rdbuf();
    YonJson request = YonJson::parse(input.str());
    const YonJson* methodValue = request.get("method");
    const std::string method = methodValue == nullptr ? "" : methodValue->asString();
    if (method.empty()) throw std::runtime_error("Missing HTTP method in request payload");
${dispatch}
    throw std::runtime_error("Handler class does not implement static " + method + "()");
}
`;
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runCpp(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('cpp', handlerPath);
        const marker = '.yon-cpp-production-ready';
        const binaryName = process.platform === 'win32' ? 'handler.exe' : 'handler';
        const binaryPath = path.join(root, binaryName);
        await YonCompiledRunner.prepareArtifact(root, marker, async () => {
            YonCompiledRunner.copyServiceSources('cpp', handlerPath, root);
            const source = YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'Handler.cpp'));
            const methods = YonCompiledRunner.cppMethods(source);
            if (methods.length === 0)
                throw new Error('C++ route must define class Handler with at least one static HTTP method');
            writeFileSync(path.join(root, 'YonJson.hpp'), YonCompiledRunner.cppJsonSupportSource());
            writeFileSync(path.join(root, 'main.cpp'), YonCompiledRunner.cppMainSource(methods));
            await YonCompiledRunner.runCommand([YonCompiledRunner.cppExecutable(), '-std=c++17', '-O2', '-o', binaryPath, 'main.cpp'], root);
        });
        return YonCompiledRunner.runCommand([binaryPath], root, requestText);
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
        const marker = '.yon-java-production-ready';
        await YonCompiledRunner.prepareArtifact(root, marker, async () => {
            YonCompiledRunner.copyServiceSources('java', handlerPath, root);
            YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'Handler.java'));
            writeFileSync(path.join(root, 'YonJson.java'), YonCompiledRunner.javaJsonSupportSource());
            writeFileSync(path.join(root, 'Main.java'), YonCompiledRunner.javaMainSource());
            const sources = readdirSync(root).filter((entry) => entry.endsWith('.java'));
            await YonCompiledRunner.runCommand(['javac', ...sources], root);
        });
        return YonCompiledRunner.runCommand(['java', '-cp', root, 'Main'], root, requestText);
    }

    /**
     * @param {string} source
     * @returns {string[]}
     */
    static swiftMethods(source) {
        return YonCompiledRunner.httpMethods.filter((method) => new RegExp(`\\bstatic\\s+func\\s+${method}\\s*\\(`).test(source));
    }

    /**
     * @param {string[]} methods
     * @returns {string}
     */
    static swiftMainSource(methods) {
        const dispatch = methods.map((method) => `    case "${method}": result = Handler.${method}(request)`).join('\n');
        return `import Foundation

let inputData = FileHandle.standardInput.readDataToEndOfFile()
let inputText = String(data: inputData, encoding: .utf8) ?? ""
let payload = inputText.isEmpty ? "{}" : inputText
let parsed = try JSONSerialization.jsonObject(with: Data(payload.utf8), options: [.fragmentsAllowed])
guard let request = parsed as? [String: Any] else {
    FileHandle.standardError.write(Data("Expected Tachyon request JSON to be an object".utf8))
    exit(1)
}
let method = (request["method"] as? String) ?? ""
if method.isEmpty {
    FileHandle.standardError.write(Data("Missing HTTP method in request payload".utf8))
    exit(1)
}
var result: Any? = nil
switch method {
${dispatch}
default:
    FileHandle.standardError.write(Data("Handler class does not implement static \\(method)()".utf8))
    exit(1)
}
guard let value = result else { exit(0) }
if let text = value as? String {
    FileHandle.standardOutput.write(Data(text.utf8))
} else {
    FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed]))
}
`;
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runSwift(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('swift', handlerPath);
        const marker = '.yon-swift-production-ready';
        const binaryName = process.platform === 'win32' ? 'handler.exe' : 'handler';
        const binaryPath = path.join(root, binaryName);
        await YonCompiledRunner.prepareArtifact(root, marker, async () => {
            YonCompiledRunner.copyServiceSources('swift', handlerPath, root);
            const source = YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'Handler.swift'));
            const methods = YonCompiledRunner.swiftMethods(source);
            if (methods.length === 0)
                throw new Error('Swift route must define Handler with at least one static HTTP method');
            writeFileSync(path.join(root, 'main.swift'), YonCompiledRunner.swiftMainSource(methods));
            const sources = readdirSync(root).filter((entry) => entry.endsWith('.swift'));
            const optimization = YonCompiledRunner.isProduction() ? ['-O'] : [];
            await YonCompiledRunner.runCommand([YonCompiledRunner.swiftExecutable(), ...optimization, '-o', binaryPath, ...sources], root);
        });
        return YonCompiledRunner.runCommand([binaryPath], root, requestText);
    }

    /** @returns {string} */
    static kotlinJsonSupportSource() {
        return `object YonJson {
    fun parse(input: String): Any? = Parser(if (input.isBlank()) "{}" else input).parse()

    @Suppress("UNCHECKED_CAST")
    fun parseObject(input: String): Map<String, Any?> {
        val value = parse(input)
        if (value is Map<*, *>) return value as Map<String, Any?>
        throw IllegalArgumentException("Expected Tachyon request JSON to be an object")
    }

    fun stringify(value: Any?): String = when (value) {
        null -> "null"
        is String -> quote(value)
        is Boolean -> value.toString()
        is Number -> numberToString(value)
        is Map<*, *> -> buildString {
            append('{')
            var first = true
            for ((key, entry) in value) {
                if (!first) append(',')
                first = false
                append(quote(key.toString()))
                append(':')
                append(stringify(entry))
            }
            append('}')
        }
        is Iterable<*> -> buildString {
            append('[')
            var first = true
            for (item in value) {
                if (!first) append(',')
                first = false
                append(stringify(item))
            }
            append(']')
        }
        else -> quote(value.toString())
    }

    private fun numberToString(value: Number): String {
        if (value is Double) {
            if (value.isFinite() && value == Math.floor(value)) return value.toLong().toString()
            return value.toString()
        }
        return value.toString()
    }

    private fun quote(text: String): String {
        val out = StringBuilder("\\"")
        for (ch in text) {
            when (ch) {
                '"' -> out.append("\\\\\\"")
                '\\\\' -> out.append("\\\\\\\\")
                '\\b' -> out.append("\\\\b")
                '\\u000C' -> out.append("\\\\f")
                '\\n' -> out.append("\\\\n")
                '\\r' -> out.append("\\\\r")
                '\\t' -> out.append("\\\\t")
                else -> if (ch < ' ') out.append("\\\\u%04x".format(ch.code)) else out.append(ch)
            }
        }
        out.append('"')
        return out.toString()
    }

    private class Parser(private val text: String) {
        private var index = 0

        fun parse(): Any? {
            val value = readValue()
            skipWhitespace()
            if (index != text.length) throw error("Unexpected trailing JSON")
            return value
        }

        private fun readValue(): Any? {
            skipWhitespace()
            if (index >= text.length) throw error("Unexpected end of JSON")
            return when (text[index]) {
                '{' -> readObject()
                '[' -> readArray()
                '"' -> readString()
                't' -> readLiteral("true", true)
                'f' -> readLiteral("false", false)
                'n' -> readLiteral("null", null)
                else -> {
                    val ch = text[index]
                    if (ch == '-' || ch.isDigit()) readNumber() else throw error("Unexpected JSON token")
                }
            }
        }

        private fun readObject(): Map<String, Any?> {
            expect('{')
            val obj = LinkedHashMap<String, Any?>()
            skipWhitespace()
            if (peek('}')) { index++; return obj }
            while (true) {
                skipWhitespace()
                val key = readString()
                skipWhitespace()
                expect(':')
                obj[key] = readValue()
                skipWhitespace()
                if (peek('}')) { index++; return obj }
                expect(',')
            }
        }

        private fun readArray(): List<Any?> {
            expect('[')
            val arr = ArrayList<Any?>()
            skipWhitespace()
            if (peek(']')) { index++; return arr }
            while (true) {
                arr.add(readValue())
                skipWhitespace()
                if (peek(']')) { index++; return arr }
                expect(',')
            }
        }

        private fun readString(): String {
            expect('"')
            val out = StringBuilder()
            while (index < text.length) {
                val ch = text[index++]
                if (ch == '"') return out.toString()
                if (ch != '\\\\') { out.append(ch); continue }
                if (index >= text.length) throw error("Invalid escape sequence")
                when (text[index++]) {
                    '"' -> out.append('"')
                    '\\\\' -> out.append('\\\\')
                    '/' -> out.append('/')
                    'b' -> out.append('\\b')
                    'f' -> out.append('\\u000C')
                    'n' -> out.append('\\n')
                    'r' -> out.append('\\r')
                    't' -> out.append('\\t')
                    'u' -> {
                        if (index + 4 > text.length) throw error("Invalid unicode escape")
                        out.append(text.substring(index, index + 4).toInt(16).toChar())
                        index += 4
                    }
                    else -> throw error("Invalid escape sequence")
                }
            }
            throw error("Unterminated string")
        }

        private fun readNumber(): Any {
            val start = index
            if (peek('-')) index++
            while (index < text.length && text[index].isDigit()) index++
            var decimal = false
            if (peek('.')) {
                decimal = true
                index++
                while (index < text.length && text[index].isDigit()) index++
            }
            if (index < text.length && (text[index] == 'e' || text[index] == 'E')) {
                decimal = true
                index++
                if (index < text.length && (text[index] == '+' || text[index] == '-')) index++
                while (index < text.length && text[index].isDigit()) index++
            }
            val number = text.substring(start, index)
            return if (decimal) number.toDouble() else number.toLong()
        }

        private fun readLiteral(literal: String, value: Any?): Any? {
            if (!text.startsWith(literal, index)) throw error("Invalid literal")
            index += literal.length
            return value
        }

        private fun skipWhitespace() {
            while (index < text.length && text[index].isWhitespace()) index++
        }

        private fun peek(expected: Char) = index < text.length && text[index] == expected

        private fun expect(expected: Char) {
            if (!peek(expected)) throw error("Expected '\$expected'")
            index++
        }

        private fun error(message: String) = IllegalArgumentException("\$message at character \$index")
    }
}
`;
    }

    /**
     * @param {string} source
     * @returns {string[]}
     */
    static kotlinMethods(source) {
        return YonCompiledRunner.httpMethods.filter((method) => new RegExp(`\\bfun\\s+${method}\\s*\\(`).test(source));
    }

    /**
     * @param {string[]} methods
     * @returns {string}
     */
    static kotlinMainSource(methods) {
        const dispatch = methods.map((method) => `        "${method}" -> Handler.${method}(request)`).join('\n');
        return `fun main() {
    val input = System.\`in\`.readBytes().toString(Charsets.UTF_8)
    val request = YonJson.parseObject(input)
    val method = request["method"] as? String
    if (method.isNullOrEmpty()) throw IllegalArgumentException("Missing HTTP method in request payload")
    val result: Any? = when (method) {
${dispatch}
        else -> throw IllegalArgumentException("Handler class does not implement static \$method()")
    }
    if (result != null) {
        if (result is String) print(result) else print(YonJson.stringify(result))
    }
}
`;
    }

    /**
     * @param {string} handlerPath
     * @param {string} requestText
     * @returns {Promise<string>}
     */
    static async runKotlin(handlerPath, requestText) {
        const root = YonCompiledRunner.workspace('kotlin', handlerPath);
        const marker = '.yon-kotlin-production-ready';
        const jarPath = path.join(root, 'app.jar');
        await YonCompiledRunner.prepareArtifact(root, marker, async () => {
            YonCompiledRunner.copyServiceSources('kotlin', handlerPath, root);
            const source = YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'Handler.kt'));
            const methods = YonCompiledRunner.kotlinMethods(source);
            if (methods.length === 0)
                throw new Error('Kotlin route must define Handler with at least one static HTTP method');
            writeFileSync(path.join(root, 'YonJson.kt'), YonCompiledRunner.kotlinJsonSupportSource());
            writeFileSync(path.join(root, 'main.kt'), YonCompiledRunner.kotlinMainSource(methods));
            const sources = readdirSync(root).filter((entry) => entry.endsWith('.kt'));
            await YonCompiledRunner.runCommand([YonCompiledRunner.kotlinExecutable(), ...sources, '-include-runtime', '-d', 'app.jar'], root);
        });
        return YonCompiledRunner.runCommand(['java', '-cp', jarPath, 'MainKt'], root, requestText);
    }

    /** @returns {string} */
    static rustJsonSupportSource() {
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

    pub fn array(entries: Vec<YonJson>) -> YonJson {
        YonJson::Array(entries)
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
                    if !first {
                        out.push(',');
                    }
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
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    out.push_str(&value.stringify());
                }
                out.push(']');
                out
            }
        }
    }
}

impl From<&str> for YonJson {
    fn from(value: &str) -> Self {
        YonJson::String(value.to_string())
    }
}

impl From<String> for YonJson {
    fn from(value: String) -> Self {
        YonJson::String(value)
    }
}

impl From<bool> for YonJson {
    fn from(value: bool) -> Self {
        YonJson::Bool(value)
    }
}

impl From<i32> for YonJson {
    fn from(value: i32) -> Self {
        YonJson::Number(value as f64)
    }
}

impl From<i64> for YonJson {
    fn from(value: i64) -> Self {
        YonJson::Number(value as f64)
    }
}

impl From<u64> for YonJson {
    fn from(value: u64) -> Self {
        YonJson::Number(value as f64)
    }
}

impl From<usize> for YonJson {
    fn from(value: usize) -> Self {
        YonJson::Number(value as f64)
    }
}

impl From<f64> for YonJson {
    fn from(value: f64) -> Self {
        YonJson::Number(value)
    }
}

fn stringify_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{}", value)
    }
}

fn quote(text: &str) -> String {
    let mut out = String::from("\\"");
    for current in text.chars() {
        match current {
            '"' => {
                out.push('\\\\');
                out.push('"');
            }
            '\\\\' => {
                out.push('\\\\');
                out.push('\\\\');
            }
            '\\n' => {
                out.push('\\\\');
                out.push('n');
            }
            '\\r' => {
                out.push('\\\\');
                out.push('r');
            }
            '\\t' => {
                out.push('\\\\');
                out.push('t');
            }
            '\\u{0008}' => {
                out.push('\\\\');
                out.push('b');
            }
            '\\u{000c}' => {
                out.push('\\\\');
                out.push('f');
            }
            value if value < ' ' => {
                out.push('\\\\');
                out.push('u');
                out.push_str(&format!("{:04x}", value as u32));
            }
            value => out.push(value),
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
    fn new(text: &str) -> Parser {
        Parser {
            chars: text.chars().collect(),
            index: 0,
        }
    }

    fn parse(mut self) -> Result<YonJson, String> {
        let value = self.read_value()?;
        self.skip_whitespace();
        if self.index != self.chars.len() {
            return Err(self.error("Unexpected trailing JSON"));
        }
        Ok(value)
    }

    fn read_value(&mut self) -> Result<YonJson, String> {
        self.skip_whitespace();
        let Some(current) = self.current() else {
            return Err(self.error("Unexpected end of JSON"));
        };
        match current {
            '{' => self.read_object(),
            '[' => self.read_array(),
            '"' => self.read_string().map(YonJson::String),
            't' => self.read_literal("true", YonJson::Bool(true)),
            'f' => self.read_literal("false", YonJson::Bool(false)),
            'n' => self.read_literal("null", YonJson::Null),
            '-' | '0'..='9' => self.read_number(),
            _ => Err(self.error("Unexpected JSON token")),
        }
    }

    fn read_object(&mut self) -> Result<YonJson, String> {
        self.expect('{')?;
        let mut object = BTreeMap::new();
        self.skip_whitespace();
        if self.peek('}') {
            self.index += 1;
            return Ok(YonJson::Object(object));
        }
        loop {
            self.skip_whitespace();
            let key = self.read_string()?;
            self.skip_whitespace();
            self.expect(':')?;
            object.insert(key, self.read_value()?);
            self.skip_whitespace();
            if self.peek('}') {
                self.index += 1;
                return Ok(YonJson::Object(object));
            }
            self.expect(',')?;
        }
    }

    fn read_array(&mut self) -> Result<YonJson, String> {
        self.expect('[')?;
        let mut array = Vec::new();
        self.skip_whitespace();
        if self.peek(']') {
            self.index += 1;
            return Ok(YonJson::Array(array));
        }
        loop {
            array.push(self.read_value()?);
            self.skip_whitespace();
            if self.peek(']') {
                self.index += 1;
                return Ok(YonJson::Array(array));
            }
            self.expect(',')?;
        }
    }

    fn read_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        while self.index < self.chars.len() {
            let current = self.chars[self.index];
            self.index += 1;
            if current == '"' {
                return Ok(out);
            }
            if current != '\\\\' {
                out.push(current);
                continue;
            }
            let Some(escaped) = self.current() else {
                return Err(self.error("Invalid escape sequence"));
            };
            self.index += 1;
            match escaped {
                '"' => out.push('"'),
                '\\\\' => out.push('\\\\'),
                '/' => out.push('/'),
                'b' => out.push('\\u{0008}'),
                'f' => out.push('\\u{000c}'),
                'n' => out.push('\\n'),
                'r' => out.push('\\r'),
                't' => out.push('\\t'),
                'u' => {
                    if self.index + 4 > self.chars.len() {
                        return Err(self.error("Invalid unicode escape"));
                    }
                    let hex: String = self.chars[self.index..self.index + 4].iter().collect();
                    let code = u32::from_str_radix(&hex, 16)
                        .map_err(|_| self.error("Invalid unicode escape"))?;
                    let Some(value) = char::from_u32(code) else {
                        return Err(self.error("Invalid unicode escape"));
                    };
                    out.push(value);
                    self.index += 4;
                }
                _ => return Err(self.error("Invalid escape sequence")),
            }
        }
        Err(self.error("Unterminated string"))
    }

    fn read_number(&mut self) -> Result<YonJson, String> {
        let start = self.index;
        if self.peek('-') {
            self.index += 1;
        }
        while matches!(self.current(), Some('0'..='9')) {
            self.index += 1;
        }
        if self.peek('.') {
            self.index += 1;
            while matches!(self.current(), Some('0'..='9')) {
                self.index += 1;
            }
        }
        if matches!(self.current(), Some('e' | 'E')) {
            self.index += 1;
            if matches!(self.current(), Some('+' | '-')) {
                self.index += 1;
            }
            while matches!(self.current(), Some('0'..='9')) {
                self.index += 1;
            }
        }
        let number: String = self.chars[start..self.index].iter().collect();
        number
            .parse::<f64>()
            .map(YonJson::Number)
            .map_err(|_| self.error("Invalid number"))
    }

    fn read_literal(&mut self, literal: &str, value: YonJson) -> Result<YonJson, String> {
        for expected in literal.chars() {
            if self.current() != Some(expected) {
                return Err(self.error("Invalid literal"));
            }
            self.index += 1;
        }
        Ok(value)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.current(), Some(' ' | '\\n' | '\\r' | '\\t')) {
            self.index += 1;
        }
    }

    fn current(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn peek(&self, expected: char) -> bool {
        self.current() == Some(expected)
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        if !self.peek(expected) {
            return Err(self.error(&format!("Expected '{}'", expected)));
        }
        self.index += 1;
        Ok(())
    }

    fn error(&self, message: &str) -> String {
        format!("{} at character {}", message, self.index)
    }
}
`;
    }

    /**
     * @param {string} source
     * @returns {string[]}
     */
    static rustMethods(source) {
        return YonCompiledRunner.httpMethods.filter((method) => new RegExp(`\\b(?:pub\\s+)?fn\\s+${method}\\s*\\(`).test(source));
    }

    /**
     * @param {string[]} methods
     * @param {string[]} serviceFiles
     * @returns {string}
     */
    static rustMainSource(methods, serviceFiles = []) {
        const serviceIncludes = serviceFiles.map((file) => `include!(${JSON.stringify(file)});`).join('\n');
        const dispatch = methods.map((method) => `        "${method}" => write_response(Handler::${method}(&request)),`).join('\n');
        return `#![allow(non_snake_case)]
mod yon_json;
pub use yon_json::YonJson;
${serviceIncludes}
include!("Handler.rs");

use std::io::{self, Read};

trait YonResponse {
    fn into_output(self) -> Option<String>;
}

impl YonResponse for YonJson {
    fn into_output(self) -> Option<String> {
        Some(self.stringify())
    }
}

impl YonResponse for String {
    fn into_output(self) -> Option<String> {
        Some(self)
    }
}

impl YonResponse for &str {
    fn into_output(self) -> Option<String> {
        Some(self.to_string())
    }
}

impl YonResponse for () {
    fn into_output(self) -> Option<String> {
        None
    }
}

impl<T: YonResponse> YonResponse for Option<T> {
    fn into_output(self) -> Option<String> {
        self.and_then(|value| value.into_output())
    }
}

fn write_response<T: YonResponse>(value: T) -> Result<(), String> {
    if let Some(output) = value.into_output() {
        print!("{}", output);
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let request = YonJson::parse(&input)?;
    let method = request
        .get("method")
        .map(|value| value.as_string())
        .unwrap_or_default();
    if method.is_empty() {
        return Err("Missing HTTP method in request payload".to_string());
    }
    match method.as_str() {
${dispatch}
        _ => Err(format!("Handler class does not implement static {}()", method)),
    }
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
        const marker = '.yon-rust-production-ready';
        const binaryName = process.platform === 'win32' ? 'handler.exe' : 'handler';
        const binaryPath = path.join(root, binaryName);
        await YonCompiledRunner.prepareArtifact(root, marker, async () => {
            YonCompiledRunner.copyServiceSources('rust', handlerPath, root);
            const source = YonCompiledRunner.writeSourceWithoutShebang(handlerPath, path.join(root, 'Handler.rs'));
            const methods = YonCompiledRunner.rustMethods(source);
            if (methods.length === 0)
                throw new Error('Rust route must define impl Handler with at least one HTTP method');
            const serviceFiles = readdirSync(root)
                .filter((entry) => entry.endsWith('.rs') && !['Handler.rs', 'main.rs', 'yon_json.rs'].includes(entry))
                .sort();
            writeFileSync(path.join(root, 'yon_json.rs'), YonCompiledRunner.rustJsonSupportSource());
            writeFileSync(path.join(root, 'main.rs'), YonCompiledRunner.rustMainSource(methods, serviceFiles));
            const optimization = YonCompiledRunner.isProduction() ? ['-O'] : [];
            await YonCompiledRunner.runCommand([YonCompiledRunner.rustExecutable(), '--edition=2021', ...optimization, '-o', binaryPath, 'main.rs'], root);
        });
        return YonCompiledRunner.runCommand([binaryPath], root, requestText);
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
                : language === 'cpp'
                    ? await YonCompiledRunner.runCpp(handlerPath, requestText)
                    : language === 'java'
                        ? await YonCompiledRunner.runJava(handlerPath, requestText)
                        : language === 'swift'
                            ? await YonCompiledRunner.runSwift(handlerPath, requestText)
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
