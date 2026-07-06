// @ts-check
import path from 'path';
import { mkdir, writeFile } from 'fs/promises';
import PlatformGenerator from '../platform-generator.js';

/**
 * Generates a buildable Windows host project using Microsoft WebView2.
 *
 * Output layout:
 *   <outputRoot>/
 *     Resources/                 # copied Tac assets
 *     src/
 *       main.cpp
 *     CMakeLists.txt
 *     build.bat
 *     build.sh
 *     README.md
 *     tachyon.host.json
 */
export default class WindowsGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        const srcDir = path.join(this.outputRoot, 'src');
        await mkdir(srcDir, { recursive: true });

        await writeFile(path.join(srcDir, 'main.cpp'), this.cppSource());
        await writeFile(path.join(srcDir, 'app.rc'), 'IDI_APP_ICON ICON "../Resources/TachyonIcon.ico"\n');
        await writeFile(path.join(this.outputRoot, 'CMakeLists.txt'), this.cmakeLists());
        await writeFile(path.join(this.outputRoot, 'build.bat'), this.buildBatch());
        await this.writeExecutable('build.sh', this.buildShell());
    }

    cppSource() {
        const appName = this.appName.replace(/"/g, '\\"');
        const nativeHostScript = `window.__tcNativeHost__ = { postMessage(message) { window.chrome.webview.postMessage(typeof message === 'string' ? message : JSON.stringify(message)); } };\\n${this.getBridgeScript()}`;
        const bridgeScript = nativeHostScript.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n');
        return `#include <windows.h>
#include <wrl.h>
#include <WebView2.h>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <future>
#include <sstream>
#include <string>
#include <vector>
#include <shlwapi.h>

#pragma comment(lib, "user32.lib")
#pragma comment(lib, "ole32.lib")
#pragma comment(lib, "oleaut32.lib")
#pragma comment(lib, "shlwapi.lib")

using namespace Microsoft::WRL;

static constexpr WCHAR WINDOW_CLASS[] = L"${appName}";
static constexpr WCHAR WINDOW_TITLE[] = L"${appName}";

static ComPtr<ICoreWebView2Controller> webViewController;
static ComPtr<ICoreWebView2> webView;

static std::wstring GetResourcePath() {
    WCHAR path[MAX_PATH];
    GetModuleFileNameW(nullptr, path, MAX_PATH);
    PathRemoveFileSpecW(path);
    std::wstring resources(path);
    resources += L"\\\\Resources";
    return resources;
}

static std::wstring GetIndexPath() {
    return GetResourcePath() + L"\\\\index.html";
}

static std::wstring GetIndexUri() {
    WCHAR uri[4096];
    DWORD uriLength = 4096;
    if (SUCCEEDED(UrlCreateFromPathW(GetIndexPath().c_str(), uri, &uriLength, 0))) {
        return std::wstring(uri, uriLength);
    }
    return L"file:///" + GetIndexPath();
}

static std::string Narrow(const std::wstring& value) {
    if (value.empty()) return "";
    int size = WideCharToMultiByte(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), nullptr, 0, nullptr, nullptr);
    std::string result(size, '\\0');
    WideCharToMultiByte(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), result.data(), size, nullptr, nullptr);
    return result;
}

static std::wstring Widen(const std::string& value) {
    if (value.empty()) return L"";
    int size = MultiByteToWideChar(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), nullptr, 0);
    std::wstring result(size, L'\\0');
    MultiByteToWideChar(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), result.data(), size);
    return result;
}

static std::string JsonEscape(const std::string& value) {
    std::string out;
    for (char c : value) {
        if (c == '\\\\' || c == '"') out += '\\\\';
        if (c == '\\n') out += "\\\\n";
        else out += c;
    }
    return out;
}

static std::string JsSingleQuoteEscape(const std::string& value) {
    std::string out;
    for (char c : value) {
        if (c == '\\\\' || c == '\\'') out += '\\\\';
        if (c == '\\n') out += "\\\\n";
        else out += c;
    }
    return out;
}

static std::string ExtractJsonString(const std::string& json, const std::string& key) {
    std::string pattern = "\\"" + key + "\\"";
    size_t pos = json.find(pattern);
    if (pos == std::string::npos) return "";
    pos = json.find(':', pos + pattern.size());
    if (pos == std::string::npos) return "";
    pos += 1;
    while (pos < json.size() && isspace(static_cast<unsigned char>(json[pos]))) pos++;
    if (pos >= json.size() || json[pos] != '"') return "";
    pos += 1;
    std::string out;
    while (pos < json.size() && json[pos] != '"') {
        if (json[pos] == '\\\\' && pos + 1 < json.size()) {
            pos += 1;
            out += json[pos] == 'n' ? '\\n' : json[pos];
        } else {
            out += json[pos];
        }
        pos += 1;
    }
    return out;
}

static std::string ExtractJsonValue(const std::string& json, const std::string& key) {
    std::string pattern = "\\"" + key + "\\"";
    size_t pos = json.find(pattern);
    if (pos == std::string::npos) return "";
    pos = json.find(':', pos + pattern.size());
    if (pos == std::string::npos) return "";
    pos += 1;
    while (pos < json.size() && isspace(static_cast<unsigned char>(json[pos]))) pos++;

    size_t start = pos;
    if (pos < json.size() && json[pos] == '"') {
        pos += 1;
        while (pos < json.size()) {
            if (json[pos] == '\\\\' && pos + 1 < json.size()) { pos += 2; continue; }
            if (json[pos] == '"') { pos += 1; break; }
            pos += 1;
        }
    } else if (pos < json.size() && json[pos] == '{') {
        int depth = 0;
        while (pos < json.size()) {
            if (json[pos] == '"') {
                pos += 1;
                while (pos < json.size()) {
                    if (json[pos] == '\\\\' && pos + 1 < json.size()) { pos += 2; continue; }
                    if (json[pos] == '"') { pos += 1; break; }
                    pos += 1;
                }
                continue;
            }
            if (json[pos] == '{') depth++;
            if (json[pos] == '}') { depth--; pos++; if (depth == 0) break; }
            else pos++;
        }
    } else if (pos < json.size() && json[pos] == '[') {
        int depth = 0;
        while (pos < json.size()) {
            if (json[pos] == '"') {
                pos += 1;
                while (pos < json.size()) {
                    if (json[pos] == '\\\\' && pos + 1 < json.size()) { pos += 2; continue; }
                    if (json[pos] == '"') { pos += 1; break; }
                    pos += 1;
                }
                continue;
            }
            if (json[pos] == '[') depth++;
            if (json[pos] == ']') { depth--; pos++; if (depth == 0) break; }
            else pos++;
        }
    } else {
        while (pos < json.size() && json[pos] != ',' && json[pos] != '}' && json[pos] != ']' && !isspace(static_cast<unsigned char>(json[pos]))) pos++;
    }

    return json.substr(start, pos - start);
}

static int ExtractJsonInt(const std::string& json, const std::string& key) {
    std::string pattern = "\\"" + key + "\\"";
    size_t pos = json.find(pattern);
    if (pos == std::string::npos) return 0;
    pos = json.find(':', pos + pattern.size());
    if (pos == std::string::npos) return 0;
    return std::atoi(json.c_str() + pos + 1);
}

static std::vector<std::string> ExtractJsonStringArray(const std::string& json, const std::string& key) {
    std::vector<std::string> values;
    std::string pattern = "\\"" + key + "\\"";
    size_t pos = json.find(pattern);
    if (pos == std::string::npos) return values;
    pos = json.find(':', pos + pattern.size());
    if (pos == std::string::npos) return values;
    pos += 1;
    while (pos < json.size() && isspace(static_cast<unsigned char>(json[pos]))) pos++;
    if (pos >= json.size() || json[pos] != '[') return values;
    pos += 1;
    while (pos < json.size() && json[pos] != ']') {
        while (pos < json.size() && (isspace(static_cast<unsigned char>(json[pos])) || json[pos] == ',')) pos++;
        if (pos >= json.size() || json[pos] == ']') break;
        if (json[pos] != '"') break;
        pos += 1;
        std::string out;
        while (pos < json.size() && json[pos] != '"') {
            if (json[pos] == '\\\\' && pos + 1 < json.size()) {
                pos += 1;
                out += json[pos] == 'n' ? '\\n' : json[pos];
            } else {
                out += json[pos];
            }
            pos += 1;
        }
        if (pos < json.size() && json[pos] == '"') pos += 1;
        values.push_back(out);
    }
    return values;
}

static std::string ReadTextFile(const std::string& filePath) {
    std::ifstream input(filePath, std::ios::binary);
    std::ostringstream buffer;
    buffer << input.rdbuf();
    return buffer.str();
}

static void WriteTextFile(const std::string& filePath, const std::string& text) {
    std::ofstream output(filePath, std::ios::binary);
    output << text;
}

static std::string QuoteProcessArg(const std::string& value) {
    if (value.empty()) return "\\"\\"";
    bool needsQuotes = value.find_first_of(" \\t\\"") != std::string::npos;
    if (!needsQuotes) return value;
    std::string quoted = "\\"";
    int backslashes = 0;
    for (char c : value) {
        if (c == '\\\\') {
            backslashes += 1;
            continue;
        }
        if (c == '"') {
            quoted.append(static_cast<size_t>(backslashes * 2 + 1), '\\\\');
            quoted += c;
            backslashes = 0;
            continue;
        }
        quoted.append(static_cast<size_t>(backslashes), '\\\\');
        backslashes = 0;
        quoted += c;
    }
    quoted.append(static_cast<size_t>(backslashes * 2), '\\\\');
    quoted += "\\"";
    return quoted;
}

static std::string ReadPipe(HANDLE pipe) {
    std::string output;
    char buffer[512];
    DWORD read = 0;
    while (ReadFile(pipe, buffer, sizeof(buffer), &read, nullptr) && read > 0) {
        output.append(buffer, read);
    }
    return output;
}

static std::string RunShellCommand(const std::string& command, const std::vector<std::string>& args, const std::string& cwd) {
    SECURITY_ATTRIBUTES security = {};
    security.nLength = sizeof(SECURITY_ATTRIBUTES);
    security.bInheritHandle = TRUE;

    HANDLE stdoutRead = nullptr;
    HANDLE stdoutWrite = nullptr;
    HANDLE stderrRead = nullptr;
    HANDLE stderrWrite = nullptr;
    if (!CreatePipe(&stdoutRead, &stdoutWrite, &security, 0) || !CreatePipe(&stderrRead, &stderrWrite, &security, 0)) {
        return "";
    }
    SetHandleInformation(stdoutRead, HANDLE_FLAG_INHERIT, 0);
    SetHandleInformation(stderrRead, HANDLE_FLAG_INHERIT, 0);

    std::string commandLine = QuoteProcessArg(command);
    for (const auto& arg : args) commandLine += " " + QuoteProcessArg(arg);

    STARTUPINFOA startup = {};
    startup.cb = sizeof(STARTUPINFOA);
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdOutput = stdoutWrite;
    startup.hStdError = stderrWrite;
    startup.hStdInput = GetStdHandle(STD_INPUT_HANDLE);

    PROCESS_INFORMATION process = {};
    std::string mutableCommandLine = commandLine;
    BOOL created = CreateProcessA(
        nullptr,
        mutableCommandLine.data(),
        nullptr,
        nullptr,
        TRUE,
        CREATE_NO_WINDOW,
        nullptr,
        cwd.empty() ? nullptr : cwd.c_str(),
        &startup,
        &process
    );
    CloseHandle(stdoutWrite);
    CloseHandle(stderrWrite);
    if (!created) {
        CloseHandle(stdoutRead);
        CloseHandle(stderrRead);
        return "";
    }
    auto stdoutFuture = std::async(std::launch::async, ReadPipe, stdoutRead);
    auto stderrFuture = std::async(std::launch::async, ReadPipe, stderrRead);
    WaitForSingleObject(process.hProcess, INFINITE);
    DWORD exitCode = 0;
    GetExitCodeProcess(process.hProcess, &exitCode);
    std::string stdoutText = stdoutFuture.get();
    std::string stderrText = stderrFuture.get();
    CloseHandle(stdoutRead);
    CloseHandle(stderrRead);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);

    std::string argsJson = "[";
    for (size_t index = 0; index < args.size(); index += 1) {
        if (index > 0) argsJson += ",";
        argsJson += "\\"" + JsonEscape(args[index]) + "\\"";
    }
    argsJson += "]";
    return "{\\"command\\":\\"" + JsonEscape(command) + "\\",\\"args\\":" + argsJson + ",\\"cwd\\":\\"" + JsonEscape(cwd) + "\\",\\"exitCode\\":" + std::to_string(exitCode) + ",\\"stdout\\":\\"" + JsonEscape(stdoutText) + "\\",\\"stderr\\":\\"" + JsonEscape(stderrText) + "\\"}";
}

static bool IsWorkerMetadataFile(const std::string& name) {
    if (name.size() > 5 && name.compare(name.size() - 5, 5, ".json") == 0) return true;
    if (name.size() > 7 && name.compare(name.size() - 7, 7, ".schema") == 0) return true;
    return false;
}

static std::string FindWorkerExecutable(const std::string& route) {
    std::filesystem::path workerDir = std::filesystem::path(Narrow(GetResourcePath())) / "workers" / route;
    if (!std::filesystem::is_directory(workerDir)) return "";
    for (const auto& entry : std::filesystem::directory_iterator(workerDir)) {
        if (!entry.is_regular_file()) continue;
        std::string name = entry.path().filename().string();
        if (IsWorkerMetadataFile(name)) continue;
        return entry.path().string();
    }
    return "";
}

static std::string RunWorkerJson(const std::string& route, const std::string& method, const std::string& requestJson) {
    std::string executablePath = FindWorkerExecutable(route);
    if (executablePath.empty()) return "";

    SECURITY_ATTRIBUTES security = {};
    security.nLength = sizeof(SECURITY_ATTRIBUTES);
    security.bInheritHandle = TRUE;

    HANDLE stdoutRead = nullptr;
    HANDLE stdoutWrite = nullptr;
    HANDLE stderrRead = nullptr;
    HANDLE stderrWrite = nullptr;
    HANDLE stdinRead = nullptr;
    HANDLE stdinWrite = nullptr;
    if (!CreatePipe(&stdoutRead, &stdoutWrite, &security, 0) ||
        !CreatePipe(&stderrRead, &stderrWrite, &security, 0) ||
        !CreatePipe(&stdinRead, &stdinWrite, &security, 0)) {
        return "";
    }
    SetHandleInformation(stdoutRead, HANDLE_FLAG_INHERIT, 0);
    SetHandleInformation(stderrRead, HANDLE_FLAG_INHERIT, 0);
    SetHandleInformation(stdinWrite, HANDLE_FLAG_INHERIT, 0);

    std::string commandLine = QuoteProcessArg(executablePath);

    STARTUPINFOA startup = {};
    startup.cb = sizeof(STARTUPINFOA);
    startup.dwFlags = STARTF_USESTDHANDLES;
    startup.hStdOutput = stdoutWrite;
    startup.hStdError = stderrWrite;
    startup.hStdInput = stdinRead;

    PROCESS_INFORMATION process = {};
    std::string mutableCommandLine = commandLine;
    BOOL created = CreateProcessA(
        nullptr,
        mutableCommandLine.data(),
        nullptr,
        nullptr,
        TRUE,
        CREATE_NO_WINDOW,
        nullptr,
        nullptr,
        &startup,
        &process
    );
    CloseHandle(stdoutWrite);
    CloseHandle(stderrWrite);
    CloseHandle(stdinRead);
    if (!created) {
        CloseHandle(stdoutRead);
        CloseHandle(stderrRead);
        CloseHandle(stdinWrite);
        return "";
    }

    std::string envelope = "{\\"method\\":\\"" + method + "\\",\\"request\\":" + (requestJson.empty() ? "{}" : requestJson) + "}";
    DWORD written = 0;
    WriteFile(stdinWrite, envelope.data(), static_cast<DWORD>(envelope.size()), &written, nullptr);
    CloseHandle(stdinWrite);

    auto stdoutFuture = std::async(std::launch::async, ReadPipe, stdoutRead);
    auto stderrFuture = std::async(std::launch::async, ReadPipe, stderrRead);
    WaitForSingleObject(process.hProcess, INFINITE);
    DWORD exitCode = 0;
    GetExitCodeProcess(process.hProcess, &exitCode);
    std::string stdoutText = stdoutFuture.get();
    std::string stderrText = stderrFuture.get();
    CloseHandle(stdoutRead);
    CloseHandle(stderrRead);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);

    if (exitCode != 0) return "";
    return stdoutText;
}

static std::string HandleNativeCapability(const std::string& message) {
    int id = ExtractJsonInt(message, "id");
    std::string capability = ExtractJsonString(message, "capability");
    std::string value;
    try {
        if (capability == "app.info") {
            value = "{\\"name\\":\\"" + JsonEscape("${appName}") + "\\",\\"runtime\\":\\"windows-webview2\\"}";
        } else if (capability == "fs.readText") {
            std::string filePath = ExtractJsonString(message, "path");
            value = "{\\"path\\":\\"" + JsonEscape(filePath) + "\\",\\"text\\":\\"" + JsonEscape(ReadTextFile(filePath)) + "\\"}";
        } else if (capability == "fs.writeText") {
            std::string filePath = ExtractJsonString(message, "path");
            std::string text = ExtractJsonString(message, "text");
            WriteTextFile(filePath, text);
            value = "{\\"path\\":\\"" + JsonEscape(filePath) + "\\",\\"bytes\\":" + std::to_string(text.size()) + ",\\"written\\":true}";
        } else if (capability == "fs.readDir") {
            std::string dirPath = ExtractJsonString(message, "path");
            std::string entries = "[";
            bool first = true;
            for (const auto& entry : std::filesystem::directory_iterator(dirPath)) {
                if (!first) entries += ",";
                first = false;
                entries += "{\\"name\\":\\"" + JsonEscape(entry.path().filename().string()) + "\\",\\"type\\":\\"" + (entry.is_directory() ? "directory" : "file") + "\\"}";
            }
            entries += "]";
            value = "{\\"path\\":\\"" + JsonEscape(dirPath) + "\\",\\"entries\\":" + entries + "}";
        } else if (capability == "shell.exec") {
            value = RunShellCommand(
                ExtractJsonString(message, "command"),
                ExtractJsonStringArray(message, "args"),
                ExtractJsonString(message, "cwd")
            );
        } else if (capability == "tachyon.worker") {
            std::string route = ExtractJsonString(message, "route");
            std::string method = ExtractJsonString(message, "method");
            std::string requestJson = ExtractJsonValue(message, "request");
            if (route.empty() || method.empty()) {
                return "{\\"type\\":\\"tac:native-response\\",\\"id\\":" + std::to_string(id) + ",\\"ok\\":false,\\"error\\":\\"Tachyon worker requires route and method\\"}";
            }
            std::string workerResult = RunWorkerJson(route, method, requestJson);
            if (workerResult.empty()) {
                return "{\\"type\\":\\"tac:native-response\\",\\"id\\":" + std::to_string(id) + ",\\"ok\\":false,\\"error\\":\\"Unable to execute worker\\"}";
            }
            return "{\\"type\\":\\"tac:native-response\\",\\"id\\":" + std::to_string(id) + ",\\"ok\\":true,\\"value\\":" + workerResult + "}";
        } else {
            return "{\\"type\\":\\"tac:native-response\\",\\"id\\":" + std::to_string(id) + ",\\"ok\\":false,\\"error\\":\\"Unsupported native capability\\"}";
        }
        return "{\\"type\\":\\"tac:native-response\\",\\"id\\":" + std::to_string(id) + ",\\"ok\\":true,\\"value\\":" + value + "}";
    } catch (const std::exception& error) {
        return "{\\"type\\":\\"tac:native-response\\",\\"id\\":" + std::to_string(id) + ",\\"ok\\":false,\\"error\\":\\"" + JsonEscape(error.what()) + "\\"}";
    }
}

static std::wstring BridgeReplyScript(const std::string& response) {
    std::string escaped = JsSingleQuoteEscape(response);
    std::string script = "if(window.__tcNativeBridge__.messageHandler)window.__tcNativeBridge__.messageHandler('" + escaped + "')";
    return Widen(script);
}

static LRESULT CALLBACK WindowProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    switch (msg) {
        case WM_SIZE:
            if (webViewController) {
                RECT bounds;
                GetClientRect(hwnd, &bounds);
                webViewController->put_Bounds(bounds);
            }
            return 0;
        case WM_DESTROY:
            PostQuitMessage(0);
            return 0;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

int WINAPI wWinMain(HINSTANCE hInstance, HINSTANCE, LPWSTR, int nCmdShow) {
    WNDCLASSEXW wc = {};
    wc.cbSize = sizeof(WNDCLASSEXW);
    wc.lpfnWndProc = WindowProc;
    wc.hInstance = hInstance;
    wc.hCursor = LoadCursor(nullptr, IDC_ARROW);
    wc.lpszClassName = WINDOW_CLASS;
    RegisterClassExW(&wc);

    HWND hwnd = CreateWindowExW(
        0, WINDOW_CLASS, WINDOW_TITLE,
        WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        CW_USEDEFAULT, CW_USEDEFAULT, 1280, 800,
        nullptr, nullptr, hInstance, nullptr);

    ShowWindow(hwnd, nCmdShow);
    UpdateWindow(hwnd);

    HRESULT hr = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    if (FAILED(hr)) return 1;

    ComPtr<ICoreWebView2Environment> environment;
    hr = CreateCoreWebView2EnvironmentWithOptions(
        nullptr, GetResourcePath().c_str(), nullptr,
        Callback<ICoreWebView2CreateCoreWebView2EnvironmentCompletedHandler>(
            [hwnd](HRESULT result, ICoreWebView2Environment* env) -> HRESULT {
                if (FAILED(result) || !env) return result;
                env->CreateCoreWebView2Controller(
                    hwnd,
                    Callback<ICoreWebView2CreateCoreWebView2ControllerCompletedHandler>(
                        [hwnd](HRESULT result, ICoreWebView2Controller* controller) -> HRESULT {
                            if (FAILED(result) || !controller) return result;
                            webViewController = controller;
                            controller->get_CoreWebView2(&webView);

                            ComPtr<ICoreWebView2Settings> settings;
                            webView->get_Settings(&settings);
                            settings->put_IsWebMessageEnabled(TRUE);

                            webView->AddScriptToExecuteOnDocumentCreated(
                                L"${bridgeScript}", nullptr);

                            webView->AddWebMessageReceivedHandler(
                                Callback<ICoreWebView2WebMessageReceivedEventHandler>(
                                    [](ICoreWebView2* sender, ICoreWebView2WebMessageReceivedEventArgs* args) -> HRESULT {
                                        LPWSTR message;
                                        args->TryGetWebMessageAsString(&message);
                                        if (message) {
                                            std::wstring reply = BridgeReplyScript(HandleNativeCapability(Narrow(message)));
                                            CoTaskMemFree(message);
                                            sender->ExecuteScript(reply.c_str(), nullptr);
                                        }
                                        return S_OK;
                                    }).Get(), nullptr);

                            std::wstring indexUri = GetIndexUri();
                            webView->Navigate(indexUri.c_str());

                            RECT bounds;
                            GetClientRect(hwnd, &bounds);
                            controller->put_Bounds(bounds);
                            return S_OK;
                        }).Get());
                return S_OK;
            }).Get());

    if (FAILED(hr)) return 1;

    MSG msg;
    while (GetMessage(&msg, nullptr, 0, 0)) {
        TranslateMessage(&msg);
        DispatchMessage(&msg);
    }

    CoUninitialize();
    return 0;
}
`;
    }

    cmakeLists() {
        return `cmake_minimum_required(VERSION 3.20)
project(${this.appName} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(\${PROJECT_NAME} WIN32 src/main.cpp src/app.rc)

target_link_libraries(\${PROJECT_NAME} PRIVATE
    user32
    ole32
    oleaut32
    shlwapi
)

# WebView2 loader
find_library(WEBVIEW2_LOADER WebView2Loader.dll.lib PATHS $ENV{ProgramFiles(x86)}/Microsoft Edge WebView2\\nuget/win-x64\\native/x64)
if(WEBVIEW2_LOADER)
    target_link_libraries(\${PROJECT_NAME} PRIVATE \${WEBVIEW2_LOADER})
endif()

set_target_properties(\${PROJECT_NAME} PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY \${CMAKE_BINARY_DIR}/\${PROJECT_NAME}
)

# Copy resources next to the executable
add_custom_command(TARGET \${PROJECT_NAME} POST_BUILD
    COMMAND \${CMAKE_COMMAND} -E copy_directory
    \${CMAKE_SOURCE_DIR}/Resources $<TARGET_FILE_DIR:\${PROJECT_NAME}>/Resources
)
`;
    }

    buildBatch() {
        return `@echo off
setlocal

echo Building ${this.appName} (Windows WebView2 host)...

if not exist build mkdir build
cd build
cmake -G "Visual Studio 17 2022" -A x64 .. || goto :error
cmake --build . --config Release || goto :error

echo Built: build\\Release\\${this.appName}.exe
exit /b 0

:error
echo Build failed.
exit /b 1
`;
    }

    buildShell() {
        return `#!/bin/sh
set -e

APP_NAME="${this.appName}"
OUTPUT_ROOT="$(cd "$(dirname "$0")" && pwd)"

echo "Building $APP_NAME (Windows WebView2 host)..."

mkdir -p "$OUTPUT_ROOT/build"
cd "$OUTPUT_ROOT/build"
cmake -G "MinGW Makefiles" .. 2>/dev/null || cmake -G "Unix Makefiles" ..
cmake --build . --config Release

echo "Built: $OUTPUT_ROOT/build/$APP_NAME/$APP_NAME.exe"
`;
    }

    buildReadme() {
        return `# ${this.appName} — Windows native host

This folder contains a buildable WebView2 host for the Tac frontend.

## Prerequisites

- Windows 10/11
- Visual Studio 2022 with C++ workload
- Microsoft Edge WebView2 Runtime (installed by default on Windows 11)

## Build

Open a "Developer Command Prompt for VS 2022" and run:

\`\`\`cmd
build.bat
\`\`\`

The executable is written to \`build\\Release\\${this.appName}.exe\`.

## Run

\`\`\`cmd
build\\Release\\${this.appName}.exe
\`\`\`

## Architecture

- Static Tac assets live in \`Resources/\`.
- \`src/main.cpp\` creates a Win32 window hosting \`ICoreWebView2\`.
- \`window.__tcNativeBridge__\` exposes a minimal JS↔native message contract.
`;
    }
}
