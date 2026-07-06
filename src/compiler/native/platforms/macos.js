// @ts-check
import path from 'path';
import { mkdir, writeFile } from 'fs/promises';
import PlatformGenerator from '../platform-generator.js';

/**
 * Generates a buildable macOS .app host project using WKWebView.
 *
 * Output layout:
 *   <outputRoot>/
 *     Resources/                 # copied Tac assets
 *     Sources/
 *       TachyonApp.swift
 *     TachyonApp/
 *       Info.plist
 *       TachyonApp.entitlements
 *     build.sh                   # compiles the .app bundle with swiftc
 *     README.md
 *     tachyon.host.json
 */
export default class MacOSGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        const sourcesDir = path.join(this.outputRoot, 'Sources');
        const appDir = path.join(this.outputRoot, 'TachyonApp');
        await mkdir(sourcesDir, { recursive: true });
        await mkdir(appDir, { recursive: true });

        await writeFile(path.join(sourcesDir, 'TachyonApp.swift'), this.swiftSource());
        await writeFile(path.join(appDir, 'Info.plist'), this.infoPlist());
        await writeFile(path.join(appDir, 'TachyonApp.entitlements'), this.entitlements());
        await writeFile(path.join(appDir, 'PkgInfo'), 'APPL????');
        await this.writeExecutable('build.sh', this.buildScript());
    }

    swiftSource() {
        const appName = this.appName.replace(/"/g, '\\"');
        const bridgeScript = this.getBridgeScript().replace(/`/g, '\\`').replace(/\\/g, '\\\\');
        return `import Cocoa
import WebKit

// Serves Contents/Resources from the custom "tachyon" scheme. Unlike
// file:// (an opaque origin), a scheme-handled origin is a secure context,
// which is what unlocks Web Workers (Tac Workers), OPFS (the FYLO browser
// mirror) and module scripts inside WKWebView — matching the iOS and
// Android hosts. Serves directory indexes and extension-less deep links.
class TachyonSchemeHandler: NSObject, WKURLSchemeHandler {
    static let mimeTypes: [String: String] = [
        "html": "text/html",
        "js": "text/javascript",
        "mjs": "text/javascript",
        "css": "text/css",
        "json": "application/json",
        "webmanifest": "application/manifest+json",
        "svg": "image/svg+xml",
        "png": "image/png",
        "jpg": "image/jpeg",
        "jpeg": "image/jpeg",
        "gif": "image/gif",
        "ico": "image/x-icon",
        "wasm": "application/wasm",
        "woff": "font/woff",
        "woff2": "font/woff2",
        "txt": "text/plain",
        "xml": "application/xml",
    ]

    func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
        guard let url = urlSchemeTask.request.url,
              let resourcesURL = Bundle.main.resourceURL else {
            urlSchemeTask.didFailWithError(NativeBridgeError.message("Missing app resources"))
            return
        }
        var relativePath = url.path
        if relativePath.isEmpty || relativePath == "/" || relativePath.hasSuffix("/") {
            relativePath += "index.html"
        }
        relativePath = String(relativePath.drop(while: { $0 == "/" }))

        var fileURL = resourcesURL.appendingPathComponent(relativePath)
        var body = try? Data(contentsOf: fileURL)
        if body == nil && !relativePath.contains(".") {
            // Extension-less deep link (/atlas): fall back to its index.
            fileURL = resourcesURL.appendingPathComponent(relativePath + "/index.html")
            body = try? Data(contentsOf: fileURL)
        }
        guard let data = body else {
            let notFound = HTTPURLResponse(url: url, statusCode: 404, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": "text/plain"])!
            urlSchemeTask.didReceive(notFound)
            urlSchemeTask.didReceive(Data("Not Found".utf8))
            urlSchemeTask.didFinish()
            return
        }
        let mime = TachyonSchemeHandler.mimeTypes[fileURL.pathExtension.lowercased()] ?? "application/octet-stream"
        let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: [
            "Content-Type": mime,
            "Content-Length": String(data.count),
        ])!
        urlSchemeTask.didReceive(response)
        urlSchemeTask.didReceive(data)
        urlSchemeTask.didFinish()
    }

    func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {}
}

// The app's own scheme stays in the WebView; external links (GitHub, docs,
// mailto) open in the user's default browser — navigating the WebView away
// would strand them with no way back into the app.
class TachyonNavigationDelegate: NSObject, WKNavigationDelegate {
    static let shared = TachyonNavigationDelegate()

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = navigationAction.request.url, url.scheme != "tachyon" else {
            decisionHandler(.allow)
            return
        }
        if url.scheme == "http" || url.scheme == "https" || url.scheme == "mailto" {
            NSWorkspace.shared.open(url)
            decisionHandler(.cancel)
            return
        }
        decisionHandler(.allow)
    }
}

class ViewController: NSViewController {
    var webView: WKWebView!

    override func loadView() {
        let config = WKWebViewConfiguration()
        config.setURLSchemeHandler(TachyonSchemeHandler(), forURLScheme: "tachyon")
        config.userContentController.add(MessageHandler.shared, name: "tachyon")

        let script = WKUserScript(
            source: "${this.escapeSwiftString(bridgeScript)}",
            injectionTime: .atDocumentStart,
            forMainFrameOnly: false
        )
        config.userContentController.addUserScript(script)

        webView = WKWebView(frame: .zero, configuration: config)
        webView.autoresizingMask = [.width, .height]
        webView.navigationDelegate = TachyonNavigationDelegate.shared
        MessageHandler.shared.webView = webView
        self.view = webView
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        if let appURL = URL(string: "tachyon://localhost/") {
            webView.load(URLRequest(url: appURL))
        }
    }
}

class MessageHandler: NSObject, WKScriptMessageHandler {
    static let shared = MessageHandler()
    weak var webView: WKWebView?

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let text = message.body as? String,
              let data = text.data(using: .utf8),
              let envelope = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              envelope["type"] as? String == "tac:native-request" else {
            reply(id: 0, ok: false, value: nil, error: "Invalid native request envelope")
            return
        }
        let id = envelope["id"] as? Int ?? 0
        let capability = envelope["capability"] as? String ?? ""
        let payload = envelope["payload"] as? [String: Any] ?? [:]
        do {
            let value = try handle(capability: capability, payload: payload)
            reply(id: id, ok: true, value: value, error: nil)
        } catch {
            reply(id: id, ok: false, value: nil, error: String(describing: error))
        }
    }

    func handle(capability: String, payload: [String: Any]) throws -> Any {
        switch capability {
        case "app.info":
            return [
                "name": Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "${appName}",
                "runtime": "macos-wkwebview",
                "version": Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0.0"
            ]
        case "fs.readText":
            let filePath = try requireString(payload["path"], "path")
            return ["path": filePath, "text": try String(contentsOfFile: filePath, encoding: .utf8)]
        case "fs.writeText":
            let filePath = try requireString(payload["path"], "path")
            let text = String(describing: payload["text"] ?? "")
            try text.write(toFile: filePath, atomically: true, encoding: .utf8)
            return ["path": filePath, "bytes": text.utf8.count, "written": true]
        case "fs.readDir":
            let dirPath = try requireString(payload["path"], "path")
            let names = try FileManager.default.contentsOfDirectory(atPath: dirPath)
            let entries = names.map { name -> [String: String] in
                var isDirectory: ObjCBool = false
                let child = (dirPath as NSString).appendingPathComponent(name)
                FileManager.default.fileExists(atPath: child, isDirectory: &isDirectory)
                return ["name": name, "type": isDirectory.boolValue ? "directory" : "file"]
            }
            return ["path": dirPath, "entries": entries]
        case "shell.exec":
            let command = try requireString(payload["command"], "command")
            let args = payload["args"] as? [String] ?? []
            let cwd = payload["cwd"] as? String
            return try runProcess(command: command, args: args, cwd: cwd)
        case "tachyon.worker":
            let route = try requireString(payload["route"], "route")
            let method = payload["method"] as? String ?? "GET"
            let request = payload["request"] as? [String: Any] ?? [:]
            return try runWorker(route: route, method: method, request: request)
        default:
            throw NativeBridgeError.message("Unsupported native capability: " + capability)
        }
    }

    func requireString(_ value: Any?, _ key: String) throws -> String {
        guard let text = value as? String, !text.isEmpty else {
            throw NativeBridgeError.message("Native capability payload requires non-empty string: " + key)
        }
        return text
    }

    func runProcess(command: String, args: [String], cwd: String?) throws -> [String: Any] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [command] + args
        if let cwd = cwd, !cwd.isEmpty {
            process.currentDirectoryURL = URL(fileURLWithPath: cwd, isDirectory: true)
        }
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        try process.run()
        process.waitUntilExit()
        let stdoutText = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let stderrText = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        return [
            "command": command,
            "args": args,
            "cwd": cwd ?? "",
            "exitCode": Int(process.terminationStatus),
            "stdout": stdoutText,
            "stderr": stderrText
        ]
    }

    func runWorker(route: String, method: String, request: [String: Any]) throws -> Any {
        guard let resourcesURL = Bundle.main.resourceURL else {
            throw NativeBridgeError.message("Resources directory not found")
        }
        let workerDir = resourcesURL.appendingPathComponent("workers", isDirectory: true).appendingPathComponent(route, isDirectory: true)
        let fileManager = FileManager.default
        guard let entries = try? fileManager.contentsOfDirectory(atPath: workerDir.path) else {
            throw NativeBridgeError.message("Worker directory not found: " + route)
        }
        guard let executable = entries.first(where: { !$0.hasSuffix(".json") && !$0.hasSuffix(".schema") && !$0.hasPrefix(".") }) else {
            throw NativeBridgeError.message("No worker executable found for route: " + route)
        }
        let executableURL = workerDir.appendingPathComponent(executable)
        guard fileManager.isExecutableFile(atPath: executableURL.path) else {
            throw NativeBridgeError.message("Worker executable is not executable: " + executable)
        }

        let envelope: [String: Any] = ["method": method, "request": request]
        let inputData = try JSONSerialization.data(withJSONObject: envelope)

        let process = Process()
        process.executableURL = executableURL
        let stdin = Pipe()
        let stdout = Pipe()
        let stderr = Pipe()
        process.standardInput = stdin
        process.standardOutput = stdout
        process.standardError = stderr
        try process.run()
        stdin.fileHandleForWriting.write(inputData)
        stdin.fileHandleForWriting.closeFile()
        process.waitUntilExit()

        let stdoutText = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        if process.terminationStatus != 0 {
            let stderrText = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
            throw NativeBridgeError.message("Worker exited with code \\(process.terminationStatus): \\(stderrText)")
        }
        guard let data = stdoutText.data(using: .utf8),
              let response = try? JSONSerialization.jsonObject(with: data) else {
            throw NativeBridgeError.message("Worker returned invalid JSON: " + stdoutText)
        }
        return response
    }

    func reply(id: Int, ok: Bool, value: Any?, error: String?) {
        var response: [String: Any] = ["type": "tac:native-response", "id": id, "ok": ok]
        if ok {
            response["value"] = value ?? NSNull()
        } else {
            response["error"] = error ?? "Native capability failed"
        }
        guard let data = try? JSONSerialization.data(withJSONObject: response),
              let json = String(data: data, encoding: .utf8) else { return }
        let escaped = json.replacingOccurrences(of: "\\\\", with: "\\\\\\\\").replacingOccurrences(of: "'", with: "\\\\'")
        let script = "if (window.__tcNativeBridge__.messageHandler) window.__tcNativeBridge__.messageHandler('" + escaped + "')"
        if let webView = webView {
            webView.evaluateJavaScript(script, completionHandler: nil)
        }
    }
}

enum NativeBridgeError: Error {
    case message(String)
}

class AppDelegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!

    // Standard menu bar: without a main menu a WKWebView app has no working
    // Cmd+C/V/Q at all. App authors add their own items here — build an
    // NSMenu, add it to mainMenu, and point custom actions at any target
    // (including evaluateJavaScript on the WebView for web-side handlers).
    func buildMainMenu() -> NSMenu {
        let appName = "${appName}"
        let mainMenu = NSMenu()

        let appMenuItem = NSMenuItem()
        mainMenu.addItem(appMenuItem)
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "About " + appName, action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Hide " + appName, action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Quit " + appName, action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appMenuItem.submenu = appMenu

        let editMenuItem = NSMenuItem()
        mainMenu.addItem(editMenuItem)
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        editMenu.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "Z")
        editMenu.addItem(NSMenuItem.separator())
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editMenuItem.submenu = editMenu

        let windowMenuItem = NSMenuItem()
        mainMenu.addItem(windowMenuItem)
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        windowMenuItem.submenu = windowMenu
        NSApp.windowsMenu = windowMenu

        return mainMenu
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.mainMenu = buildMainMenu()
        for iconExtension in ["icns", "png"] {
            if let iconURL = Bundle.main.url(forResource: "TachyonIcon", withExtension: iconExtension),
               let icon = NSImage(contentsOf: iconURL) {
                NSApp.applicationIconImage = icon
                break
            }
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "${appName}"
        window.center()

        let viewController = ViewController()
        viewController.view.frame = window.contentView?.bounds ?? .zero
        window.contentView = viewController.view

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }
}

@main
enum TachyonMain {
    static let appDelegate = AppDelegate()

    static func main() {
        let app = NSApplication.shared
        app.delegate = appDelegate
        app.run()
    }
}
`;
    }

    infoPlist() {
        return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${this.appName}</string>
    <key>CFBundleIdentifier</key>
    <string>${this.appId}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${this.appName}</string>
    <key>CFBundleIconFile</key>
    <string>TachyonIcon</string>
    <key>CFBundleDisplayName</key>
    <string>${this.appName}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${this.version}</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
`;
    }

    entitlements() {
        const sandboxValue = this.hasRawOsCapabilities() ? '<false/>' : '<true/>';
        return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    ${sandboxValue}
    <key>com.apple.security.network.client</key>
    <true/>
    <key>com.apple.security.files.user-selected.read-write</key>
    <true/>
</dict>
</plist>
`;
    }

    buildScript() {
        return `#!/bin/sh
set -e

APP_NAME="${this.appName}"
APP_BUNDLE="$APP_NAME.app"
OUTPUT_ROOT="$(cd "$(dirname "$0")" && pwd)"
BUILD_DIR="$OUTPUT_ROOT/build"

echo "Building $APP_BUNDLE..."

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/$APP_BUNDLE/Contents/MacOS"
mkdir -p "$BUILD_DIR/$APP_BUNDLE/Contents/Resources"

cp "$OUTPUT_ROOT/TachyonApp/Info.plist" "$BUILD_DIR/$APP_BUNDLE/Contents/Info.plist"
cp "$OUTPUT_ROOT/TachyonApp/PkgInfo" "$BUILD_DIR/$APP_BUNDLE/Contents/PkgInfo"
cp "$OUTPUT_ROOT/TachyonApp/TachyonApp.entitlements" "$BUILD_DIR/$APP_BUNDLE/Contents/Resources/TachyonApp.entitlements"
cp -R "$OUTPUT_ROOT/Resources/"* "$BUILD_DIR/$APP_BUNDLE/Contents/Resources/"

swiftc \\
    -O \\
    -parse-as-library \\
    -target "$(uname -m)-apple-macos11" \\
    -framework Cocoa \\
    -framework WebKit \\
    "$OUTPUT_ROOT/Sources/TachyonApp.swift" \\
    -o "$BUILD_DIR/$APP_BUNDLE/Contents/MacOS/$APP_NAME"

if command -v xattr >/dev/null 2>&1; then
    xattr -cr "$BUILD_DIR/$APP_BUNDLE"
fi

if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - --entitlements "$BUILD_DIR/$APP_BUNDLE/Contents/Resources/TachyonApp.entitlements" "$BUILD_DIR/$APP_BUNDLE"
fi

echo "Built: $BUILD_DIR/$APP_BUNDLE"
echo "Run with: open \"$BUILD_DIR/$APP_BUNDLE\""
`;
    }

    buildReadme() {
        return `# ${this.appName} — macOS native host

This folder contains a buildable WKWebView host for the Tac frontend.

## Build

Requires macOS 11+ and Xcode command-line tools.

\`\`\`sh
sh build.sh
\`\`\`

The resulting \`${this.appName}.app\` is written to \`build/${this.appName}.app\`.

## Run

\`\`\`sh
open "build/${this.appName}.app"
\`\`\`

## Architecture

- Static Tac assets live in \`Resources/\`.
- \`Sources/TachyonApp.swift\` creates the \`WKWebView\` and loads \`Resources/index.html\`.
- \`window.__tcNativeBridge__\` exposes a minimal JS↔native message contract.
`;
    }

    /**
     * Escapes a JS string for embedding in a Swift string literal.
     * @param {string} value
     * @returns {string}
     */
    escapeSwiftString(value) {
        return value
            .replace(/\\/g, '\\\\')
            .replace(/"/g, '\\"')
            .replace(/\n/g, '\\n')
            .replace(/\r/g, '\\r')
            .replace(/\t/g, '\\t');
    }
}
