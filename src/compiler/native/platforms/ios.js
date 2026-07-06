// @ts-check
import path from 'path';
import { mkdir, writeFile } from 'fs/promises';
import PlatformGenerator from '../platform-generator.js';

/**
 * Generates an iOS host project scaffold using WKWebView.
 *
 * Because iOS apps require an Xcode project to build and sign, this generator
 * produces the Swift source files and a step-by-step README. The user creates
 * a new iOS App project in Xcode, drops the generated files in, and copies the
 * WebBundle folder to the app target.
 *
 * Output layout:
 *   <outputRoot>/
 *     WebBundle/                 # copied Tac assets
 *     Sources/
 *       TachyonApp.swift
 *       TachyonWebView.swift
 *     README.md
 *     tachyon.host.json
 */
export default class IOSGenerator extends PlatformGenerator {
    /** @param {ConstructorParameters<typeof PlatformGenerator>[0]} options */
    constructor(options) {
        super(options);
        // A top-level folder named exactly "Resources" makes codesign/installd
        // treat the flat iOS bundle as an old-style shallow macOS bundle
        // ("bundle format unrecognized" → install fails with "Missing bundle
        // ID"), so the web bundle ships under a different name.
        this.resourcesDir = path.join(this.outputRoot, 'WebBundle');
    }

    async generateProjectFiles() {
        const sourcesDir = path.join(this.outputRoot, 'Sources');
        await mkdir(sourcesDir, { recursive: true });

        await writeFile(path.join(sourcesDir, 'TachyonApp.swift'), this.appSource());
        await writeFile(path.join(sourcesDir, 'TachyonWebView.swift'), this.webViewSource());
        await writeFile(path.join(this.outputRoot, 'project.yml'), this.xcodegenSpec());
    }

    /**
     * XcodeGen spec: `xcodegen generate` turns this into a buildable
     * .xcodeproj (used by the `.ipa` packager and available for manual
     * builds). WebBundle/ ships as a folder reference so the WebView finds
     * `WebBundle/index.html` inside the app bundle.
     */
    xcodegenSpec() {
        return `name: ${this.appName}
options:
  createIntermediateGroups: true
targets:
  ${this.appName}:
    type: application
    platform: iOS
    deploymentTarget: "15.0"
    sources:
      - path: Sources
      - path: Assets.xcassets
      - path: WebBundle
        type: folder
    info:
      path: Info.plist
      properties:
        CFBundleDisplayName: ${this.appName}
        CFBundleShortVersionString: "${this.version}"
        CFBundleVersion: "1"
        UILaunchScreen: {}
    settings:
      base:
        PRODUCT_BUNDLE_IDENTIFIER: ${this.appId}
        TARGETED_DEVICE_FAMILY: "1,2"
        SWIFT_VERSION: "5.9"
        ASSETCATALOG_COMPILER_APPICON_NAME: AppIcon
`;
    }

    appSource() {
        const typeName = this.sourceIdentifier('TachyonApp');
        return `import SwiftUI

@main
struct ${typeName}App: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}

struct ContentView: View {
    var body: some View {
        TachyonWebView()
            .ignoresSafeArea()
    }
}
`;
    }

    webViewSource() {
        const bridgeScript = this.getBridgeScript().replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n');
        return `import SwiftUI
import UIKit
import WebKit

class TachyonMessageHandler: NSObject, WKScriptMessageHandler {
    static let shared = TachyonMessageHandler()
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
                "name": Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String ?? "Tachyon",
                "runtime": "ios-wkwebview",
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
            throw NativeBridgeError.message("shell.exec is not available on iOS native hosts")
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

    func reply(id: Int, ok: Bool, value: Any?, error: String?) {
        var response: [String: Any] = ["type": "tac:native-response", "id": id, "ok": ok]
        if ok {
            response["value"] = value ?? NSNull()
        } else {
            response["error"] = error ?? "Native capability failed"
        }
        guard let data = try? JSONSerialization.data(withJSONObject: response),
              let json = String(data: data, encoding: .utf8) else { return }
        let escaped = json.replacingOccurrences(of: "\\\\", with: "\\\\\\\\")
                          .replacingOccurrences(of: "'", with: "\\\\'")
        let script = "if (window.__tcNativeBridge__.messageHandler) window.__tcNativeBridge__.messageHandler('" + escaped + "')"
        if let webView = webView {
            webView.evaluateJavaScript(script, completionHandler: nil)
        }
    }
}

enum NativeBridgeError: Error {
    case message(String)
}

// Serves the bundled WebBundle/ from the custom "tachyon" scheme. Unlike
// file:// (an opaque origin), a scheme-handled origin is a secure context,
// which is what unlocks Web Workers (Tac Workers), OPFS (the FYLO browser
// mirror) and module scripts inside WKWebView — the iOS counterpart of the
// Android host's WebViewAssetLoader origin. Serves directory indexes and
// extension-less deep links the same way.
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
              let resourcesURL = Bundle.main.resourceURL?.appendingPathComponent("WebBundle", isDirectory: true) else {
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
// mailto) open in the system browser — navigating the WebView away would
// strand the user with no way back into the app. target="_blank" clicks
// arrive with a nil target frame and take the same path.
class TachyonNavigationDelegate: NSObject, WKNavigationDelegate {
    static let shared = TachyonNavigationDelegate()

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = navigationAction.request.url, url.scheme != "tachyon" else {
            decisionHandler(.allow)
            return
        }
        if url.scheme == "http" || url.scheme == "https" || url.scheme == "mailto" {
            UIApplication.shared.open(url)
            decisionHandler(.cancel)
            return
        }
        decisionHandler(.allow)
    }
}

struct TachyonWebView: UIViewRepresentable {
    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.setURLSchemeHandler(TachyonSchemeHandler(), forURLScheme: "tachyon")
        config.userContentController.add(TachyonMessageHandler.shared, name: "tachyon")

        let script = WKUserScript(
            source: "${bridgeScript}",
            injectionTime: .atDocumentStart,
            forMainFrameOnly: false
        )
        config.userContentController.addUserScript(script)

        let webView = WKWebView(frame: .zero, configuration: config)
        webView.isOpaque = false
        webView.navigationDelegate = TachyonNavigationDelegate.shared
        TachyonMessageHandler.shared.webView = webView
        return webView
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {
        guard uiView.url == nil, let appURL = URL(string: "tachyon://localhost/") else { return }
        uiView.load(URLRequest(url: appURL))
    }
}
`;
    }

    buildReadme() {
        return `# ${this.appName} — iOS native host

This folder contains an iOS WKWebView host scaffold for the Tac frontend.

## Build

iOS apps must be built and signed with Xcode. Follow these steps:

1. Open **Xcode**.
2. Choose **File → New → Project → iOS → App**.
3. Set:
   - **Product Name**: \`${this.appName}\`
   - **Organization Identifier**: \`${this.appId}\`
   - **Interface**: SwiftUI
   - **Language**: Swift
4. Save the project anywhere.
5. Drag the files from \`Sources/\` into the Xcode project navigator (check "Copy items if needed").
6. Drag the \`WebBundle/\` folder into the project navigator and add it to the app target (check "Create folder references" so the folder structure is preserved).
7. Drag \`Assets.xcassets/AppIcon.appiconset\` into the app asset catalog to use the generated Tachyon app icon.
8. Build and run on a simulator or device.

## Architecture

- Static Tac assets live in \`WebBundle/\` (a top-level "Resources" folder
  breaks flat iOS bundle detection at install time).
- \`TachyonWebView.swift\` serves \`WebBundle/\` through a \`WKURLSchemeHandler\`
  at \`tachyon://localhost/\` — a secure-context origin, so Web Workers
  (Tac Workers) and OPFS (the FYLO browser mirror) work like they do in the
  Android host's WebViewAssetLoader origin. Service workers remain
  unavailable in WKWebView; the runtime degrades gracefully without them.
- \`window.__tcNativeBridge__\` exposes a minimal JS↔native message contract.
`;
    }
}
