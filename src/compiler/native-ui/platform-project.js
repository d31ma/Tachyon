// @ts-check
import { mkdir, writeFile } from 'fs/promises';
import path from 'path';
import { QUICKJS_NG_VERSION, quickJSDriverHeader, quickJSDriverSource } from './quickjs-driver.js';

const WINDOWS_APP_SDK_VERSION = '1.8.260710003';
const WINDOWS_CPP_WINRT_VERSION = '3.0.260715.1';
const HYBRID_LOCAL_ASSET_BOOTSTRAP = `<script>(()=>{const append=Node.prototype.appendChild;Node.prototype.appendChild=function(child){for(const name of ["src","href"]){const value=child?.getAttribute?.(name);const local=value?.startsWith("/")&&!value.startsWith("//");if(local){child.setAttribute(name,value.slice(1));if(name==="src"&&child.tagName==="SCRIPT"&&child.type==="module")child.removeAttribute("type")}}return append.call(this,child)}})();</script>`;
const HYBRID_BOUNDARY_NAVIGATION_SCRIPT = `<script>document.addEventListener("click",event=>{const target=event.composedPath().find(node=>node instanceof Element&&node.hasAttribute("href"));if(!target)return;const href=target.getAttribute("href");if(href==null||href.startsWith("#")||href.startsWith("//")||/^[a-z][a-z0-9+.-]*:/i.test(href))return;const apple=window.webkit?.messageHandlers?.tachyonBoundaryNavigate;if(!apple)return;const route=href===""?"/":"/"+href.replace(/^\\.?\\/+/,"");event.preventDefault();apple.postMessage(route)},true);</script>`;
const HYBRID_BOUNDARY_THEME_SCRIPT = `<script>(()=>{const root=document.documentElement;const publish=()=>{const theme=root.getAttribute("w-theme")||"light";window.webkit?.messageHandlers?.tachyonBoundaryTheme?.postMessage(theme);if(window.TachyonBoundary)window.TachyonBoundary.setTheme(theme)};new MutationObserver(publish).observe(root,{attributes:true,attributeFilter:["w-theme"]});window.addEventListener("load",publish);requestAnimationFrame(publish)})();</script>`;
const HYBRID_BOUNDARY_RESIZE_SCRIPT = `<script>(()=>{const boundary=document.getElementById("tachyon-boundary");const publish=()=>{const height=Math.ceil(Math.max(boundary.getBoundingClientRect().height,boundary.scrollHeight));window.webkit?.messageHandlers?.tachyonBoundarySize?.postMessage(height);if(window.TachyonBoundary)window.TachyonBoundary.setHeight(height)};new ResizeObserver(publish).observe(boundary);new MutationObserver(()=>requestAnimationFrame(publish)).observe(boundary,{attributes:true,childList:true,subtree:true});window.addEventListener("load",publish);document.fonts?.ready?.then(publish);requestAnimationFrame(publish)})();</script>`;

/** @param {string} value */
function swiftIdentifier(value) {
    const result = value.replace(/[^A-Za-z0-9_]/g, '');
    return /^[A-Za-z_]/.test(result) ? result : `Tachyon${result}`;
}

/** @param {any} host */
function swiftMacOSHost(host) {
    if (host.target !== 'macos') return '';
    const managed = host.managedContentOrigins.length > 0;
    const managedCases = managed ? `
        case "contentSurface.open":
            let id = try requireString(payload, "id")
            let url = try requireAllowedURL(payload, "url")
            if surfaces[id] != nil { throw TachyonNativeHostError.message("Managed content surface already exists: " + id) }
            let surface = TachyonManagedSurface(
                id: id,
                url: url,
                persistent: payload["persistentSession"] as? Bool == true,
                allowedOrigins: managedContentOrigins,
                permissionOrigins: permissionOrigins,
                requestedPermissions: requestedPermissions)
            surfaces[id] = surface
            activeSurfaceID = id
            surfaceChanged?(surface.webView)
            return ["id": id, "open": true, "pending": false, "presentation": "composed"]
        case "contentSurface.navigate":
            let surface = try requireSurface(payload)
            surface.navigate(try requireAllowedURL(payload, "url"))
            return surface.state()
        case "contentSurface.state": return try requireSurface(payload).state()
        case "contentSurface.goBack": let surface = try requireSurface(payload); surface.goBack(); return surface.state()
        case "contentSurface.goForward": let surface = try requireSurface(payload); surface.goForward(); return surface.state()
        case "contentSurface.reload": let surface = try requireSurface(payload); surface.reload(); return surface.state()
        case "contentSurface.close":
            let id = try requireString(payload, "id")
            guard let surface = surfaces.removeValue(forKey: id) else { throw TachyonNativeHostError.message("Unknown managed content surface: " + id) }
            surface.close()
            if activeSurfaceID == id {
                activeSurfaceID = surfaces.keys.sorted().first
                surfaceChanged?(activeSurfaceID.flatMap { surfaces[$0]?.webView })
            }
            return ["id": id, "open": false, "presentation": "composed"]
` : '';
    const managedTypes = managed ? `
final class TachyonManagedSurface: NSObject, WKNavigationDelegate, WKUIDelegate {
    let id: String
    let webView: WKWebView
    let allowedOrigins: Set<String>
    let permissionOrigins: [String: [String]]
    let requestedPermissions: Set<String>
    private(set) var isOpen = true

    init(id: String, url: URL, persistent: Bool, allowedOrigins: Set<String>, permissionOrigins: [String: [String]], requestedPermissions: Set<String>) {
        self.id = id
        self.allowedOrigins = allowedOrigins
        self.permissionOrigins = permissionOrigins
        self.requestedPermissions = requestedPermissions
        let configuration = WKWebViewConfiguration()
        configuration.processPool = WKProcessPool()
        if !persistent { configuration.websiteDataStore = .nonPersistent() }
        self.webView = WKWebView(frame: .zero, configuration: configuration)
        super.init()
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.load(URLRequest(url: url))
    }

    func close() { isOpen = false; webView.stopLoading(); webView.removeFromSuperview() }
    func navigate(_ url: URL) { webView.load(URLRequest(url: url)) }
    func goBack() { if webView.canGoBack { webView.goBack() } }
    func goForward() { if webView.canGoForward { webView.goForward() } }
    func reload() { webView.reload() }
    func state() -> [String: Any] {
        ["id": id, "open": isOpen, "presentation": "composed", "url": webView.url?.absoluteString ?? "", "canGoBack": webView.canGoBack, "canGoForward": webView.canGoForward]
    }

    private func exactOrigin(_ url: URL?) -> String? {
        guard let url, url.scheme == "https", let host = url.host else { return nil }
        let port = url.port.flatMap { $0 == 443 ? nil : ":" + String($0) } ?? ""
        return "https://" + host + port
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        decisionHandler(exactOrigin(action.request.url).map(allowedOrigins.contains) == true ? .allow : .cancel)
    }

    @available(macOS 12.0, *)
    func webView(_ webView: WKWebView, requestMediaCapturePermissionFor origin: WKSecurityOrigin, initiatedByFrame frame: WKFrameInfo, type: WKMediaCaptureType, decisionHandler: @escaping (WKPermissionDecision) -> Void) {
        let port = origin.port == 0 || origin.port == 443 ? "" : ":" + String(origin.port)
        let exactOrigin = origin.protocol + "://" + origin.host + port
        let names: [String]
        switch type {
        case .camera: names = ["camera"]
        case .microphone: names = ["microphone"]
        case .cameraAndMicrophone: names = ["camera", "microphone"]
        @unknown default: names = []
        }
        let allowed = !names.isEmpty && names.allSatisfy { permission in
            requestedPermissions.contains(permission) && (permissionOrigins[permission] ?? []).contains(exactOrigin)
        }
        decisionHandler(allowed ? .prompt : .deny)
    }
}

struct TachyonManagedSurfaceView: NSViewRepresentable {
    let webView: WKWebView
    func makeNSView(context: Context) -> WKWebView { webView }
    func updateNSView(_ view: WKWebView, context: Context) {}
}
` : '';
    return `
#if os(macOS)
enum TachyonNativeHostError: LocalizedError {
    case message(String)
    var errorDescription: String? { if case .message(let value) = self { return value }; return "Native host error" }
}

${managedTypes}

final class TachyonNativeHost {
    static let shared = TachyonNativeHost()
    let capabilities = Set<String>(${JSON.stringify(host.hostCapabilities)})
    let requestedPermissions = Set<String>(${JSON.stringify([...host.requestedDevicePermissions].sort())})
    let managedContentOrigins = Set<String>(${JSON.stringify(host.managedContentOrigins)})
    let permissionOrigins: [String: [String]] = (try? JSONDecoder().decode([String: [String]].self, from: Data(#"${JSON.stringify(host.permissionOrigins)}"#.utf8))) ?? [:]
    private var shortcuts: [String: String] = [:]
    private var globalShortcutMonitor: Any?
    private var localShortcutMonitor: Any?
    ${managed ? 'private var surfaces: [String: TachyonManagedSurface] = [:]\n    private var activeSurfaceID: String?\n    var surfaceChanged: ((WKWebView?) -> Void)?' : ''}
    var emit: ((String, Any) -> Void)?

    func handle(capability: String, payloadJSON: String) -> String {
        do {
            let data = Data(payloadJSON.utf8)
            let payload = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
            let value = try invoke(capability: capability, payload: payload)
            return try envelope(ok: true, value: value)
        } catch {
            return (try? envelope(ok: false, value: error.localizedDescription)) ?? "{\\\"ok\\\":false,\\\"error\\\":\\\"Native host capability failed\\\"}"
        }
    }

    private func envelope(ok: Bool, value: Any) throws -> String {
        let object: [String: Any] = ok ? ["ok": true, "value": value] : ["ok": false, "error": String(describing: value)]
        return String(data: try JSONSerialization.data(withJSONObject: object), encoding: .utf8)!
    }

    private func requireString(_ payload: [String: Any], _ key: String) throws -> String {
        guard let value = payload[key] as? String, !value.isEmpty else { throw TachyonNativeHostError.message("Native capability payload requires " + key) }
        return value
    }

    private func requireAllowedURL(_ payload: [String: Any], _ key: String) throws -> URL {
        let raw = try requireString(payload, key)
        guard let url = URL(string: raw), url.scheme == "https", let host = url.host else { throw TachyonNativeHostError.message("Managed content requires an HTTPS URL") }
        let origin = "https://" + host + (url.port.flatMap { $0 == 443 ? nil : ":" + String($0) } ?? "")
        guard managedContentOrigins.contains(origin) else { throw TachyonNativeHostError.message("Managed content origin is not allowed: " + origin) }
        return url
    }

    ${managed ? `private func requireSurface(_ payload: [String: Any]) throws -> TachyonManagedSurface {
        let id = try requireString(payload, "id")
        guard let surface = surfaces[id] else { throw TachyonNativeHostError.message("Unknown managed content surface: " + id) }
        return surface
    }` : ''}

    private func primaryWindow() throws -> NSWindow {
        let window = NSApp.windows.first
        guard let window else { throw TachyonNativeHostError.message("Application window is unavailable") }
        return window
    }

    private func appWindowState() throws -> [String: Any] {
        let window = try primaryWindow()
        return ["alwaysOnTop": window.level == .floating, "opacity": window.alphaValue, "clickThrough": window.ignoresMouseEvents, "captureProtection": window.sharingType == .none]
    }

    private func invoke(capability: String, payload: [String: Any]) throws -> Any {
        if capability != "__tachyon.hostInfo" && !capabilities.contains(capability) {
            throw TachyonNativeHostError.message("Unsupported native capability: " + capability)
        }
        switch capability {
        case "__tachyon.hostInfo": return ["target": "macos", "platform": "desktop", "capabilities": Array(capabilities).sorted()]
        case "capabilities.state":
            let requested = try requireString(payload, "capability")
            if !capabilities.contains(requested) { return "unsupported" }
            if requested.hasPrefix("screenCapture.") { return CGPreflightScreenCaptureAccess() ? "granted" : "prompt" }
            return "granted"
        case "app.info": return ["name": ${JSON.stringify(host.appName)}, "runtime": "macos-swiftui", "version": ${JSON.stringify(host.version)}]
        case "clipboard.readText": return NSPasteboard.general.string(forType: .string) ?? ""
        case "clipboard.writeText":
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(String(describing: payload["text"] ?? ""), forType: .string)
            return ["written": true]
        case "openUrl":
            let raw = try requireString(payload, "url")
            guard let url = URL(string: raw), ["http", "https"].contains(url.scheme?.lowercased() ?? "") else { throw TachyonNativeHostError.message("openUrl requires an http(s) URL") }
            NSWorkspace.shared.open(url)
            return ["opened": true]
        case "window.state": return try appWindowState()
        case "window.alwaysOnTop": try primaryWindow().level = payload["enabled"] as? Bool == true ? .floating : .normal; return try appWindowState()
        case "window.opacity":
            let value = payload["value"] as? Double ?? 1
            guard value >= 0.1 && value <= 1 else { throw TachyonNativeHostError.message("Window opacity must be between 0.1 and 1") }
            try primaryWindow().alphaValue = value
            return try appWindowState()
        case "window.clickThrough": try primaryWindow().ignoresMouseEvents = payload["enabled"] as? Bool == true; return try appWindowState()
        case "window.captureProtection": try primaryWindow().sharingType = payload["enabled"] as? Bool == true ? .none : .readOnly; return try appWindowState()
        case "shortcuts.register": return try registerShortcut(payload)
        case "shortcuts.unregister":
            let id = try requireString(payload, "id")
            let removed = shortcuts.removeValue(forKey: id) != nil
            return ["shortcuts": shortcutSnapshot(), "unregistered": removed]
        case "shortcuts.unregisterAll":
            let count = shortcuts.count
            shortcuts.removeAll()
            return ["shortcuts": shortcutSnapshot(), "unregistered": count]
        case "shortcuts.list": return ["shortcuts": shortcutSnapshot()]
        ${managedCases}
        case "screenCapture.state": return ["supported": true, "permission": CGPreflightScreenCaptureAccess() ? "granted" : "prompt", "format": "png", "destinations": ["clipboard", "file", "both"]]
        case "screenCapture.listWindows": return try listWindows(payload)
        case "screenCapture.captureWindow": return try captureWindow(payload)
        case "fs.paths":
            let manager = FileManager.default
            return ["appData": manager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?.path ?? NSHomeDirectory(), "cache": manager.urls(for: .cachesDirectory, in: .userDomainMask).first?.path ?? NSHomeDirectory(), "documents": manager.urls(for: .documentDirectory, in: .userDomainMask).first?.path ?? NSHomeDirectory()]
        case "fs.readText": let path = try requireString(payload, "path"); return ["path": path, "text": try String(contentsOfFile: path, encoding: .utf8)]
        case "fs.writeText": let path = try requireString(payload, "path"); let text = String(describing: payload["text"] ?? ""); try text.write(toFile: path, atomically: true, encoding: .utf8); return ["path": path, "bytes": text.utf8.count, "written": true]
        case "fs.readDir": let path = try requireString(payload, "path"); return ["path": path, "entries": try FileManager.default.contentsOfDirectory(atPath: path)]
        case "fs.stat": let path = try requireString(payload, "path"); var directory: ObjCBool = false; let exists = FileManager.default.fileExists(atPath: path, isDirectory: &directory); return ["path": path, "exists": exists, "type": directory.boolValue ? "directory" : "file"]
        case "fs.mkdir": let path = try requireString(payload, "path"); try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true); return ["path": path, "created": true]
        case "fs.remove": let path = try requireString(payload, "path"); if FileManager.default.fileExists(atPath: path) { try FileManager.default.removeItem(atPath: path) }; return ["path": path, "removed": true]
        case "shell.exec": return try runProcess(payload)
        default: throw TachyonNativeHostError.message("Unsupported native capability: " + capability)
        }
    }

    private func shortcutSnapshot() -> [[String: String]] {
        shortcuts.keys.sorted().map { ["id": $0, "accelerator": shortcuts[$0] ?? ""] }
    }

    private func registerShortcut(_ payload: [String: Any]) throws -> [String: Any] {
        let id = try requireString(payload, "id")
        let accelerator = try requireString(payload, "accelerator")
        if shortcuts[id] != nil && payload["replace"] as? Bool != true { throw TachyonNativeHostError.message("Shortcut id is already registered: " + id) }
        if shortcuts.contains(where: { $0.key != id && $0.value.caseInsensitiveCompare(accelerator) == .orderedSame }) { throw TachyonNativeHostError.message("Shortcut accelerator is already registered: " + accelerator) }
        shortcuts[id] = accelerator
        installShortcutMonitors()
        return ["shortcuts": shortcutSnapshot(), "shortcut": ["id": id, "accelerator": accelerator]]
    }

    private func installShortcutMonitors() {
        if globalShortcutMonitor == nil { globalShortcutMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { [weak self] event in self?.handleShortcut(event) } }
        if localShortcutMonitor == nil { localShortcutMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in self?.handleShortcut(event); return event } }
    }

    private func handleShortcut(_ event: NSEvent) {
        let key = event.charactersIgnoringModifiers?.uppercased() ?? ""
        for (id, accelerator) in shortcuts where shortcutMatches(accelerator, key: key, flags: event.modifierFlags) {
            emit?("shortcut.activated", ["id": id])
        }
    }

    private func shortcutMatches(_ accelerator: String, key: String, flags: NSEvent.ModifierFlags) -> Bool {
        let parts = accelerator.split(separator: "+").map { String($0).lowercased() }
        guard parts.last?.uppercased() == key else { return false }
        let modifiers = Set(parts.dropLast())
        let primary = modifiers.contains("primary") || modifiers.contains("command") || modifiers.contains("meta")
        return flags.contains(.command) == primary
            && flags.contains(.control) == modifiers.contains("control")
            && flags.contains(.option) == (modifiers.contains("alt") || modifiers.contains("option"))
            && flags.contains(.shift) == modifiers.contains("shift")
    }

    private func listWindows(_ payload: [String: Any]) throws -> [String: Any] {
        if !CGPreflightScreenCaptureAccess() { _ = CGRequestScreenCaptureAccess() }
        let entries = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []
        let ownPID = ProcessInfo.processInfo.processIdentifier
        let windows = entries.compactMap { entry -> [String: Any]? in
            guard let id = entry[kCGWindowNumber as String] as? NSNumber,
                  let owner = entry[kCGWindowOwnerName as String] as? String else { return nil }
            if payload["excludeCurrentApp"] as? Bool != false,
               (entry[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == ownPID { return nil }
            let bounds = entry[kCGWindowBounds as String] as? [String: Any] ?? [:]
            let frame: [String: Any] = [
                "x": (bounds["X"] as? NSNumber)?.doubleValue ?? 0,
                "y": (bounds["Y"] as? NSNumber)?.doubleValue ?? 0,
                "width": (bounds["Width"] as? NSNumber)?.doubleValue ?? 0,
                "height": (bounds["Height"] as? NSNumber)?.doubleValue ?? 0
            ]
            return ["windowId": id.stringValue, "application": owner, "title": entry[kCGWindowName as String] as? String ?? "", "frame": frame]
        }
        return ["windows": windows, "permission": "granted"]
    }

    private func captureWindow(_ payload: [String: Any]) throws -> [String: Any] {
        guard let id = UInt32(try requireString(payload, "windowId")) else { throw TachyonNativeHostError.message("screenCapture.captureWindow requires a numeric windowId") }
        if !CGPreflightScreenCaptureAccess(), !CGRequestScreenCaptureAccess() { throw TachyonNativeHostError.message("Screen capture permission was denied") }
        typealias CaptureFunction = @convention(c) (CGRect, CGWindowListOption, CGWindowID, CGWindowImageOption) -> Unmanaged<CGImage>?
        guard let process = dlopen(nil, RTLD_LAZY),
              let symbol = dlsym(process, "CGWindowListCreateImage") else { throw TachyonNativeHostError.message("Window capture is unavailable on this macOS version") }
        let capture = unsafeBitCast(symbol, to: CaptureFunction.self)
        guard let image = capture(.null, .optionIncludingWindow, CGWindowID(id), [.boundsIgnoreFraming, .bestResolution])?.takeRetainedValue() else { throw TachyonNativeHostError.message("Unable to capture the selected window") }
        let data = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])!
        let destination = String(describing: payload["destination"] ?? "clipboard")
        var filePath = ""
        if destination == "clipboard" || destination == "both" { NSPasteboard.general.clearContents(); NSPasteboard.general.setData(data, forType: .png) }
        if destination == "file" || destination == "both" {
            let directory = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first ?? FileManager.default.temporaryDirectory
            let url = directory.appendingPathComponent("tachyon-capture-" + String(Int(Date().timeIntervalSince1970)) + ".png")
            try data.write(to: url, options: .atomic)
            filePath = url.path
        }
        return ["windowId": String(id), "destination": destination, "format": "png", "bytes": data.count, "clipboard": destination == "clipboard" || destination == "both", "path": filePath]
    }

    private func runProcess(_ payload: [String: Any]) throws -> [String: Any] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        let command = try requireString(payload, "command")
        let arguments = payload["args"] as? [String] ?? []
        process.arguments = [command] + arguments
        if let cwd = payload["cwd"] as? String, !cwd.isEmpty { process.currentDirectoryURL = URL(fileURLWithPath: cwd, isDirectory: true) }
        let stdout = Pipe(); let stderr = Pipe(); process.standardOutput = stdout; process.standardError = stderr
        try process.run(); process.waitUntilExit()
        return ["exitCode": Int(process.terminationStatus), "stdout": String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? "", "stderr": String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""]
    }
}
#endif
`;
}

/** @param {any} host */
function swiftNativeView(host) {
    const appType = swiftIdentifier(host.appName || 'TachyonApp');
    const hybrid = host.hasWebViewFallbacks;
    const managed = host.target === 'macos' && host.managedContentOrigins.length > 0;
    const hybridSupport = hybrid ? `
private func tachyonHybridDocument(_ fragment: String, theme: String) -> String {
    """
    <!doctype html><html w-theme="\\(theme)"><head><meta name="viewport" content="width=device-width,initial-scale=1">
    ${HYBRID_LOCAL_ASSET_BOOTSTRAP}
    <link rel="stylesheet" href="imports.css"><script defer src="imports.js"></script>
    <style>html,body{margin:0;min-height:0!important;height:auto!important;background:transparent;overflow:hidden;scrollbar-width:none}::-webkit-scrollbar{display:none;width:0;height:0}#tachyon-boundary{display:flow-root;container-type:inline-size}#tachyon-boundary w-app-bar{display:block;min-height:56px}#tachyon-boundary [w-dropdown].open{position:static!important;inset:auto!important}</style></head><body><div id="tachyon-boundary">\\(fragment)</div>${HYBRID_BOUNDARY_NAVIGATION_SCRIPT.replaceAll('\\', '\\\\')}${HYBRID_BOUNDARY_THEME_SCRIPT}${HYBRID_BOUNDARY_RESIZE_SCRIPT}</body></html>
    """
}

private func tachyonHybridResourceURL() -> URL? {
    guard let root = Bundle.main.resourceURL else { return nil }
    let nested = root.appendingPathComponent("WebBundle", isDirectory: true)
    return FileManager.default.fileExists(atPath: nested.path) ? nested : root
}

private let tachyonHybridProcessPool = WKProcessPool()

final class TachyonHybridCoordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler {
    var loadedHTML = ""
    var loadedTheme = ""
    let height: Binding<CGFloat>
    let navigate: (String) -> Void
    let setTheme: (String) -> Void
    init(height: Binding<CGFloat>, navigate: @escaping (String) -> Void, setTheme: @escaping (String) -> Void) {
        self.height = height
        self.navigate = navigate
        self.setTheme = setTheme
    }
    func update(_ webView: WKWebView, html: String, theme: String) {
        if loadedHTML != html {
            loadedHTML = html
            loadedTheme = theme
            webView.loadHTMLString(tachyonHybridDocument(html, theme: theme), baseURL: tachyonHybridResourceURL())
        } else if loadedTheme != theme {
            loadedTheme = theme
            webView.evaluateJavaScript("document.documentElement.setAttribute('w-theme','\\(theme)');localStorage.setItem('w-theme','\\(theme)')")
        }
    }
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        webView.evaluateJavaScript("document.getElementById('tachyon-boundary').getBoundingClientRect().height") { value, _ in
            self.applyHeight(value)
        }
    }
    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        if message.name == "tachyonBoundarySize" { applyHeight(message.body) }
        else if message.name == "tachyonBoundaryNavigate", let route = message.body as? String { navigate(route) }
        else if message.name == "tachyonBoundaryTheme", let theme = message.body as? String { setTheme(theme) }
    }
    private func applyHeight(_ value: Any?) {
        guard let number = value as? NSNumber else { return }
        DispatchQueue.main.async { self.height.wrappedValue = max(1, CGFloat(truncating: number)) }
    }
}

#if os(macOS)
private final class TachyonPassthroughWebView: WKWebView {
    private var scrollMonitor: Any?

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window != nil, scrollMonitor == nil {
            scrollMonitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
                guard let self,
                      event.window == self.window,
                      self.bounds.contains(self.convert(event.locationInWindow, from: nil)) else { return event }
                self.forwardScroll(event)
                return nil
            }
        } else if window == nil, let scrollMonitor {
            NSEvent.removeMonitor(scrollMonitor)
            self.scrollMonitor = nil
        }
    }

    override func scrollWheel(with event: NSEvent) {
        forwardScroll(event)
    }

    private func forwardScroll(_ event: NSEvent) {
        var ancestor = superview
        var outerScrollView: NSScrollView?
        while let current = ancestor {
            if let scrollView = current as? NSScrollView { outerScrollView = scrollView }
            ancestor = current.superview
        }
        if let outerScrollView {
            outerScrollView.scrollWheel(with: event)
            return
        }
        super.scrollWheel(with: event)
    }

    deinit {
        if let scrollMonitor { NSEvent.removeMonitor(scrollMonitor) }
    }
}

private struct TachyonHybridRepresentable: NSViewRepresentable {
    let html: String
    let theme: String
    @Binding var height: CGFloat
    let navigate: (String) -> Void
    let setTheme: (String) -> Void
    func makeCoordinator() -> TachyonHybridCoordinator {
        TachyonHybridCoordinator(height: $height, navigate: navigate, setTheme: setTheme)
    }
    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration(); configuration.processPool = tachyonHybridProcessPool
        configuration.userContentController.add(context.coordinator, name: "tachyonBoundarySize")
        configuration.userContentController.add(context.coordinator, name: "tachyonBoundaryNavigate")
        configuration.userContentController.add(context.coordinator, name: "tachyonBoundaryTheme")
        let view = TachyonPassthroughWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.setValue(false, forKey: "drawsBackground")
        context.coordinator.update(view, html: html, theme: theme); return view
    }
    func updateNSView(_ view: WKWebView, context: Context) {
        context.coordinator.update(view, html: html, theme: theme)
    }
}
#else
private struct TachyonHybridRepresentable: UIViewRepresentable {
    let html: String
    let theme: String
    @Binding var height: CGFloat
    let navigate: (String) -> Void
    let setTheme: (String) -> Void
    func makeCoordinator() -> TachyonHybridCoordinator {
        TachyonHybridCoordinator(height: $height, navigate: navigate, setTheme: setTheme)
    }
    func makeUIView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration(); configuration.processPool = tachyonHybridProcessPool
        configuration.userContentController.add(context.coordinator, name: "tachyonBoundarySize")
        configuration.userContentController.add(context.coordinator, name: "tachyonBoundaryNavigate")
        configuration.userContentController.add(context.coordinator, name: "tachyonBoundaryTheme")
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.isOpaque = false
        context.coordinator.update(view, html: html, theme: theme); return view
    }
    func updateUIView(_ view: WKWebView, context: Context) {
        context.coordinator.update(view, html: html, theme: theme)
    }
}
#endif

struct TachyonHybridWebView: View {
    let html: String
    @ObservedObject var model: TachyonNativeModel
    @State private var height: CGFloat = 1
    var body: some View {
        TachyonHybridRepresentable(
            html: html,
            theme: model.theme,
            height: $height,
            navigate: { route in model.open(route) },
            setTheme: { theme in model.setTheme(theme) }
        )
            .frame(maxWidth: .infinity)
            .frame(height: max(1, height))
    }
}
` : '';
    const needsWebKit = hybrid || (host.target === 'macos' && host.managedContentOrigins.length > 0);
    return `import Foundation
import JavaScriptCore
import SwiftUI
	${needsWebKit ? 'import WebKit' : ''}
#if os(macOS)
import AppKit
#endif

${swiftMacOSHost(host)}

struct TachyonNativeNode: Decodable, Identifiable {
    let kind: String
    let tag: String?
    let value: String?
    let id: String?
    let attributes: [String: String]?
    let style: [String: String]?
    let events: [String: String]?
    let html: String?
    let children: [TachyonNativeNode]?
}

private struct TachyonNativeRoute: Decodable { let route: String; let root: TachyonNativeNode }
private struct TachyonNativeBundle: Decodable {
    let schemaVersion: Int
    let entryRoute: String
    let controller: String
    let routes: [TachyonNativeRoute]
}

private struct TachyonNativeSnapshot: Decodable {
    let schemaVersion: Int
    let route: String
    let root: TachyonNativeNode
}

final class TachyonNativeController {
    private let context: JSContext
    private let api: JSValue

    init() throws {
        guard let context = JSContext() else { throw NSError(domain: "TachyonNativeUI", code: 3) }
        self.context = context
        context.exceptionHandler = { _, exception in
            print("Tachyon controller: " + (exception?.toString() ?? "unknown JavaScript error"))
        }
        #if os(macOS)
        let nativeHostCall: @convention(block) (String, String) -> String = { capability, payload in
            TachyonNativeHost.shared.handle(capability: capability, payloadJSON: payload)
        }
        context.setObject(nativeHostCall, forKeyedSubscript: "__tachyonNativeHostCall" as NSString)
        #endif
        let resolveURL: @convention(block) (String, String) -> [String: String] = { input, base in
            let baseURL = base.isEmpty ? nil : URL(string: base)
            guard let url = URL(string: input, relativeTo: baseURL)?.absoluteURL else {
                return ["href": input, "origin": "null", "protocol": "", "host": "", "hostname": "", "port": "", "pathname": input, "search": "", "hash": ""]
            }
            let components = URLComponents(url: url, resolvingAgainstBaseURL: true)
            let scheme = components?.scheme ?? ""
            let host = components?.host ?? ""
            let port = components?.port.map(String.init) ?? ""
            return [
                "href": url.absoluteString,
                "origin": host.isEmpty ? "null" : scheme + "://" + host + (port.isEmpty ? "" : ":" + port),
                "protocol": scheme.isEmpty ? "" : scheme + ":",
                "host": host + (port.isEmpty ? "" : ":" + port),
                "hostname": host,
                "port": port,
                "pathname": components?.percentEncodedPath ?? "",
                "search": components?.percentEncodedQuery.map { "?" + $0 } ?? "",
                "hash": components?.percentEncodedFragment.map { "#" + $0 } ?? "",
            ]
        }
        context.setObject(resolveURL, forKeyedSubscript: "__tachyonResolveURL" as NSString)
        context.evaluateScript("""
        (() => {
          if (typeof globalThis.queueMicrotask === "undefined") {
            globalThis.queueMicrotask = callback => Promise.resolve().then(callback);
          }
          if (typeof globalThis.URL === "undefined") {
            globalThis.URL = class URL {
              constructor(input, base) { Object.assign(this, __tachyonResolveURL(String(input), base == null ? "" : String(base))); }
              toString() { return this.href; }
              toJSON() { return this.href; }
            };
          }
          if (typeof globalThis.URLSearchParams === "undefined") {
            globalThis.URLSearchParams = class URLSearchParams {
              constructor(init = "") {
                this._entries = [];
                if (typeof init === "string") {
                  const source = init.charAt(0) === "?" ? init.slice(1) : init;
                  if (source) for (const part of source.split("&")) {
                    const pair = part.split("=");
                    this.append(decodeURIComponent(pair.shift() || ""), decodeURIComponent(pair.join("=") || ""));
                  }
                } else if (init && typeof init[Symbol.iterator] === "function") {
                  for (const pair of init) this.append(pair[0], pair[1]);
                } else if (init && typeof init === "object") {
                  for (const key of Object.keys(init)) this.append(key, init[key]);
                }
              }
              append(key, value) { this._entries.push([String(key), String(value)]); }
              set(key, value) { this.delete(key); this.append(key, value); }
              get(key) { const item = this._entries.find(entry => entry[0] === String(key)); return item ? item[1] : null; }
              getAll(key) { return this._entries.filter(entry => entry[0] === String(key)).map(entry => entry[1]); }
              has(key) { return this._entries.some(entry => entry[0] === String(key)); }
              delete(key) { const name = String(key); this._entries = this._entries.filter(entry => entry[0] !== name); }
              entries() { return this._entries[Symbol.iterator](); }
              [Symbol.iterator]() { return this.entries(); }
              toString() { return this._entries.map(entry => encodeURIComponent(entry[0]) + "=" + encodeURIComponent(entry[1])).join("&"); }
            };
          }
          if (typeof globalThis.Headers === "undefined") {
            globalThis.Headers = class Headers {
              constructor(init = {}) {
                this._values = Object.create(null);
                if (init instanceof globalThis.Headers) for (const pair of init) this.append(pair[0], pair[1]);
                else if (init && typeof init[Symbol.iterator] === "function") for (const pair of init) this.append(pair[0], pair[1]);
                else if (init && typeof init === "object") for (const key of Object.keys(init)) this.set(key, init[key]);
              }
              _key(key) { return String(key).toLowerCase(); }
              append(key, value) { const name = this._key(key); const text = String(value); this._values[name] = this._values[name] ? this._values[name] + ", " + text : text; }
              set(key, value) { this._values[this._key(key)] = String(value); }
              get(key) { const name = this._key(key); return Object.prototype.hasOwnProperty.call(this._values, name) ? this._values[name] : null; }
              has(key) { return Object.prototype.hasOwnProperty.call(this._values, this._key(key)); }
              delete(key) { delete this._values[this._key(key)]; }
              entries() { return Object.entries(this._values)[Symbol.iterator](); }
              [Symbol.iterator]() { return this.entries(); }
            };
          }
        })();
        """)
        let nested = Bundle.main.url(forResource: "tachyon.native-controller", withExtension: "js", subdirectory: "WebBundle")
        guard let url = nested ?? Bundle.main.url(forResource: "tachyon.native-controller", withExtension: "js") else {
            throw NSError(domain: "TachyonNativeUI", code: 4,
                userInfo: [NSLocalizedDescriptionKey: "tachyon.native-controller.js is missing"])
        }
        context.evaluateScript(try String(contentsOf: url, encoding: .utf8), withSourceURL: url)
        guard let api = context.objectForKeyedSubscript("__tachyonNativeUI"), !api.isUndefined else {
            throw NSError(domain: "TachyonNativeUI", code: 5,
                userInfo: [NSLocalizedDescriptionKey: "Native controller did not install its API"])
        }
        self.api = api
    }

    func call(_ method: String, payload: String? = nil, completion: @escaping (Result<String, Error>) -> Void) {
        let arguments: [Any] = payload.map { [$0] } ?? []
        guard let promise = api.invokeMethod(method, withArguments: arguments) else {
            completion(.failure(NSError(domain: "TachyonNativeUI", code: 6))); return
        }
        let success: @convention(block) (JSValue) -> Void = { value in completion(.success(value.toString())) }
        let failure: @convention(block) (JSValue) -> Void = { value in
            completion(.failure(NSError(domain: "TachyonNativeUI", code: 7,
                userInfo: [NSLocalizedDescriptionKey: value.toString() ?? "Controller rejected request"])))
        }
        promise.invokeMethod("then", withArguments: [
            JSValue(object: success, in: context) as Any,
            JSValue(object: failure, in: context) as Any,
        ])
    }
}

@MainActor final class TachyonNativeModel: ObservableObject {
    @Published var root: TachyonNativeNode?
    @Published var error: String?
    @Published var theme = UserDefaults.standard.string(forKey: "tachyon.theme") ?? "light"
    ${managed ? '@Published var managedWebView: WKWebView?' : ''}
    private var controller: TachyonNativeController?
    private var routes: [TachyonNativeRoute] = []

    var preferredColorScheme: ColorScheme? {
        theme == "dark" ? .dark : theme == "light" ? .light : nil
    }

    init() { load() }

    func load() {
        do {
            let nested = Bundle.main.url(forResource: "tachyon.native-ui", withExtension: "json", subdirectory: "WebBundle")
            guard let url = nested ?? Bundle.main.url(forResource: "tachyon.native-ui", withExtension: "json") else {
                throw NSError(domain: "TachyonNativeUI", code: 1,
                    userInfo: [NSLocalizedDescriptionKey: "tachyon.native-ui.json is missing"])
            }
            let bundle = try JSONDecoder().decode(TachyonNativeBundle.self, from: Data(contentsOf: url))
            guard bundle.schemaVersion == 1 else { throw NSError(domain: "TachyonNativeUI", code: 2) }
            routes = bundle.routes
            root = bundle.routes.first(where: { $0.route == bundle.entryRoute })?.root
            controller = try TachyonNativeController()
            #if os(macOS)
            TachyonNativeHost.shared.emit = { [weak self] event, payload in self?.emit(event, payload: payload) }
            ${managed ? 'TachyonNativeHost.shared.surfaceChanged = { [weak self] webView in self?.managedWebView = webView }' : ''}
            #endif
            controller?.call("render") { [weak self] result in
                Task { @MainActor in self?.apply(result) }
            }
        } catch { self.error = String(describing: error) }
    }

    private func apply(_ result: Result<String, Error>) {
        do {
            let json = try result.get()
            let snapshot = try JSONDecoder().decode(TachyonNativeSnapshot.self, from: Data(json.utf8))
            guard snapshot.schemaVersion == 1 else { throw NSError(domain: "TachyonNativeUI", code: 8) }
            root = snapshot.root
        } catch { self.error = String(describing: error) }
    }

    func open(_ route: String) {
        let pathname = route.hasPrefix("/") ? route : "/" + route
        let pathSegments = pathname.split(separator: "/")
        let bundled = routes.first(where: { $0.route == pathname }) ?? routes.first(where: { candidate in
            let templateSegments = candidate.route.split(separator: "/")
            return templateSegments.count == pathSegments.count && zip(templateSegments, pathSegments).allSatisfy { template, value in
                template.hasPrefix(":") || template == value
            }
        })
        if let bundled { root = bundled.root; error = nil }
        controller?.call("open", payload: route) { [weak self] result in
            Task { @MainActor in self?.apply(result) }
        }
    }

    func setTheme(_ value: String) {
        guard ["light", "dark", "auto"].contains(value), theme != value else { return }
        theme = value
        UserDefaults.standard.set(value, forKey: "tachyon.theme")
    }

    func dispatch(_ node: TachyonNativeNode, type: String, value: String? = nil) {
        guard let elementId = node.id, !elementId.isEmpty else { return }
        let payload: [String: Any] = ["elementId": elementId, "type": type, "value": value ?? ""]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8) else { return }
        controller?.call("dispatch", payload: json) { [weak self] result in
            Task { @MainActor in self?.apply(result) }
        }
    }

    private func emit(_ event: String, payload: Any) {
        let envelope: [String: Any] = ["type": "tac:host-event", "event": event, "payload": payload]
        guard let data = try? JSONSerialization.data(withJSONObject: envelope),
              let json = String(data: data, encoding: .utf8) else { return }
        controller?.call("emit", payload: json) { [weak self] result in
            Task { @MainActor in self?.apply(result) }
        }
    }
}

private func nodeText(_ node: TachyonNativeNode) -> String {
    if node.kind == "text" { return node.value ?? "" }
    return (node.children ?? []).map(nodeText).joined()
}

private func tachyonNativeImage(_ source: String) -> AnyView {
#if os(macOS)
    let remoteURL = URL(string: source).flatMap { $0.scheme == nil ? nil : $0 }
    let relativePath = source.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    let localURL = Bundle.main.resourceURL?
        .appendingPathComponent("WebBundle", isDirectory: true)
        .appendingPathComponent(relativePath)
    if let url = remoteURL ?? localURL, let image = NSImage(contentsOf: url) {
        return AnyView(Image(nsImage: image).resizable().scaledToFit())
    }
    return AnyView(EmptyView())
#else
    if #available(iOS 15.0, *), let url = URL(string: source) {
        return AnyView(AsyncImage(url: url))
    }
    return AnyView(EmptyView())
#endif
}

${hybridSupport}

func TachyonNativeNodeView(_ node: TachyonNativeNode, model: TachyonNativeModel) -> AnyView {
    if node.kind == "text" { return AnyView(Text(node.value ?? "")) }
    ${hybrid ? 'if node.kind == "webview" { return AnyView(TachyonHybridWebView(html: node.html ?? "", model: model)) }' : ''}
    let children = node.children ?? []
    switch node.tag ?? "" {
    case "h1": return AnyView(Text(nodeText(node)).font(.largeTitle).bold())
    case "h2": return AnyView(Text(nodeText(node)).font(.title).bold())
    case "h3", "h4", "h5", "h6": return AnyView(Text(nodeText(node)).font(.headline))
    case "p", "span", "label", "strong", "em", "small", "code", "pre":
        return AnyView(Text(nodeText(node)))
    case "button":
        return AnyView(Button(nodeText(node)) { model.dispatch(node, type: "click") })
    case "input":
        return AnyView(TextField(node.attributes?["placeholder"] ?? "", text: .constant(node.attributes?["value"] ?? "")))
    case "img":
        if let source = node.attributes?["src"] { return tachyonNativeImage(source) }
        return AnyView(EmptyView())
    case "hr": return AnyView(Divider())
    default:
        return AnyView(VStack(alignment: .leading, spacing: 0) {
            ForEach(children.indices, id: \\.self) { index in
                TachyonNativeNodeView(children[index], model: model)
            }
        }.frame(maxWidth: .infinity, alignment: .leading))
    }
}

private func tachyonContainsHeader(_ node: TachyonNativeNode) -> Bool {
    if node.tag == "header" { return true }
    return node.children?.contains(where: tachyonContainsHeader) ?? false
}

private func tachyonNativeSurface(theme: String) -> Color {
    if theme == "dark" { return Color(red: 0.075, green: 0.102, blue: 0.110) }
    if theme == "light" { return Color(red: 0.973, green: 0.980, blue: 0.976) }
#if os(macOS)
    return Color(NSColor.windowBackgroundColor)
#else
    return Color(UIColor.systemBackground)
#endif
}

struct TachyonNativeRootView: View {
    @StateObject private var model = TachyonNativeModel()
    var body: some View {
        ${managed ? `GeometryReader { geometry in
            HStack(spacing: 0) {` : ''}
        Group {
            if let root = model.root,
               let children = root.children,
               let header = children.first,
               tachyonContainsHeader(header) {
                VStack(alignment: .leading, spacing: 0) {
                    TachyonNativeNodeView(header, model: model)
                        .background(tachyonNativeSurface(theme: model.theme)).zIndex(2)
                    ScrollView(.vertical, showsIndicators: false) {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(children.indices.dropFirst()), id: \\.self) { index in
                                TachyonNativeNodeView(children[index], model: model)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            else {
                ScrollView(.vertical, showsIndicators: false) {
                    Group {
                        if let root = model.root { TachyonNativeNodeView(root, model: model) }
                        else if let error = model.error { Text(error).foregroundColor(.red) }
                        else { ProgressView() }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        ${managed ? `.frame(width: model.managedWebView == nil ? geometry.size.width : geometry.size.width * 0.25)
                if let webView = model.managedWebView {
                    TachyonManagedSurfaceView(webView: webView)
                        .frame(width: geometry.size.width * 0.75)
                }
            }
        }` : ''}
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .preferredColorScheme(model.preferredColorScheme)
    }
}

#if os(macOS)
final class TachyonNativeAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        DispatchQueue.main.async {
            guard let window = NSApp.windows.first else { return }
            window.styleMask.remove(.fullSizeContentView)
            window.titlebarAppearsTransparent = false
            window.setContentSize(NSSize(width: 1200, height: 800))
            window.center()
        }
    }
}
#endif

@main struct ${appType}App: App {
#if os(macOS)
    @NSApplicationDelegateAdaptor(TachyonNativeAppDelegate.self) private var appDelegate
#endif
    var body: some Scene { WindowGroup { TachyonNativeRootView() } }
}
`;
}

/** @param {any} host */
function androidNativeActivity(host) {
    const hybrid = host.hasWebViewFallbacks;
    return `package ${host.appId}

import android.os.Bundle
${hybrid ? 'import android.graphics.Color\nimport android.webkit.JavascriptInterface\nimport android.webkit.WebResourceRequest\nimport android.webkit.WebResourceResponse\nimport android.webkit.WebView\nimport androidx.compose.ui.viewinterop.AndroidView\nimport androidx.webkit.WebViewAssetLoader\nimport androidx.webkit.WebViewClientCompat' : ''}
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.cash.quickjs.QuickJs
import org.json.JSONObject

class MainActivity : ComponentActivity() {
    private lateinit var controller: TachyonNativeController

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val bundle = assets.open("tachyon.native-ui.json").bufferedReader().use { JSONObject(it.readText()) }
        val route = bundle.getString("entryRoute")
        val routes = bundle.getJSONArray("routes")
        val initialRoot = (0 until routes.length()).map { routes.getJSONObject(it) }
            .first { it.getString("route") == route }.getJSONObject("root")
        val root = mutableStateOf(initialRoot)
        controller = TachyonNativeController(
            assets.open(bundle.getString("controller")).bufferedReader().use { it.readText() },
            onSnapshot = { snapshot -> runOnUiThread { root.value = JSONObject(snapshot).getJSONObject("root") } },
        )
        controller.render()
        setContent {
            var theme by remember { mutableStateOf("light") }
            val useDarkTheme = theme == "dark" || (theme == "auto" && isSystemInDarkTheme())
            MaterialTheme(colorScheme = if (useDarkTheme) darkColorScheme() else lightColorScheme()) {
                Surface(Modifier.fillMaxSize()) {
                    Column(Modifier.fillMaxSize().safeDrawingPadding().verticalScroll(rememberScrollState())) {
                        TachyonNativeNode(
                            root.value,
                            dispatch = { id, type, value -> controller.dispatch(id, type, value) },
                            navigate = { nextRoute ->
                                val nextRoot = (0 until routes.length()).map { routes.getJSONObject(it) }
                                    .firstOrNull { it.getString("route") == nextRoute }?.getJSONObject("root")
                                if (nextRoot != null) root.value = nextRoot
                                controller.open(nextRoute)
                            },
                            theme = theme,
                            setTheme = { theme = it },
                        )
                    }
                }
            }
        }
    }

    override fun onDestroy() { if (::controller.isInitialized) controller.close(); super.onDestroy() }
}

interface TachyonNativeCallback {
    fun resolve(snapshot: String)
    fun reject(message: String)
}

class TachyonNativeController(source: String, private val onSnapshot: (String) -> Unit) : AutoCloseable {
    private val quickJs = QuickJs.create()
    private val callback = object : TachyonNativeCallback {
        override fun resolve(snapshot: String) = onSnapshot(snapshot)
        override fun reject(message: String) { throw IllegalStateException(message) }
    }

    init {
        quickJs.set("__tachyonNativeHost", TachyonNativeCallback::class.java, callback)
        quickJs.evaluate(source, "tachyon.native-controller.js")
    }

    private fun call(method: String, argument: String = "") {
        quickJs.evaluate("globalThis.__tachyonNativeUI." + method + "(" + argument + ")" +
            ".then(function(value){__tachyonNativeHost.resolve(String(value))}," +
            "function(error){__tachyonNativeHost.reject(String(error))})")
    }

    fun render() = call("render")
    fun open(route: String) = call("open", JSONObject.quote(route))
    fun dispatch(elementId: String, type: String, value: String?) = call("dispatch", JSONObject.quote(JSONObject()
        .put("elementId", elementId).put("type", type).put("value", value).toString()))
    override fun close() = quickJs.close()
}

private fun textContent(node: JSONObject): String {
    if (node.optString("kind") == "text") return node.optString("value")
    val children = node.optJSONArray("children") ?: return ""
    return (0 until children.length()).joinToString("") { textContent(children.getJSONObject(it)) }
}

${hybrid ? `private fun hybridDocument(fragment: String, theme: String): String =
    "<!doctype html><html w-theme=\\"" + theme + "\\"><head><meta name=\\"viewport\\" content=\\"width=device-width,initial-scale=1\\">" +
    ${JSON.stringify(HYBRID_LOCAL_ASSET_BOOTSTRAP)} +
    "<link rel=\\"stylesheet\\" href=\\"imports.css\\"><script defer src=\\"imports.js\\"></script>" +
    "<style>html,body{margin:0;min-height:0!important;height:auto!important;background:transparent;overflow:hidden;scrollbar-width:none}::-webkit-scrollbar{display:none;width:0;height:0}#tachyon-boundary{display:flow-root;container-type:inline-size}#tachyon-boundary w-app-bar{display:block;min-height:56px}#tachyon-boundary [w-dropdown].open{position:static!important;inset:auto!important}</style></head>" +
    "<body><div id=\\"tachyon-boundary\\">" + fragment + "</div>" +
    ${JSON.stringify(HYBRID_BOUNDARY_NAVIGATION_SCRIPT)} +
    ${JSON.stringify(HYBRID_BOUNDARY_THEME_SCRIPT)} +
    ${JSON.stringify(HYBRID_BOUNDARY_RESIZE_SCRIPT)} + "</body></html>"

@Composable fun TachyonHybridWebView(
    html: String,
    navigate: (String) -> Unit,
    theme: String,
    onThemeChange: (String) -> Unit,
) {
    var contentHeight by remember(html) { mutableStateOf(1f) }
    AndroidView(
        modifier = Modifier.fillMaxWidth().height(contentHeight.dp),
        factory = { context ->
            val assetLoader = WebViewAssetLoader.Builder()
                .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(context))
                .build()
            WebView(context).apply {
            setBackgroundColor(Color.TRANSPARENT)
            isVerticalScrollBarEnabled = false
            isHorizontalScrollBarEnabled = false
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            addJavascriptInterface(object {
                @JavascriptInterface fun setHeight(value: Float) {
                    this@apply.post { contentHeight = value.coerceAtLeast(1f) }
                }
                @JavascriptInterface fun navigate(route: String) {
                    this@apply.post { navigate(route) }
                }
                @JavascriptInterface fun setTheme(value: String) {
                    this@apply.post { onThemeChange(value) }
                }
            }, "TachyonBoundary")
            webViewClient = object : WebViewClientCompat() {
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                    val url = request.url
                    if (url.host == "appassets.androidplatform.net" && url.path?.startsWith("/assets/") == true) {
                        val route = url.path!!.removePrefix("/assets").ifEmpty { "/" }
                        view.post { navigate(route) }
                        return true
                    }
                    return false
                }
                override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? =
                    assetLoader.shouldInterceptRequest(request.url)
                override fun onPageFinished(view: WebView, url: String?) {
                    view.evaluateJavascript("document.getElementById('tachyon-boundary').getBoundingClientRect().height.toString()") { value ->
                        contentHeight = (value.trim('\\"').toFloatOrNull() ?: 160f).coerceAtLeast(1f)
                    }
                }
            }
        } },
        update = { view ->
            if (view.tag != html) {
                view.tag = html
                view.loadDataWithBaseURL("https://appassets.androidplatform.net/assets/", hybridDocument(html, theme), "text/html", "UTF-8", null)
            } else {
                val quotedTheme = JSONObject.quote(theme)
                view.evaluateJavascript(
                    "document.documentElement.setAttribute('w-theme'," + quotedTheme + ");" +
                        "localStorage.setItem('w-theme'," + quotedTheme + ")",
                    null,
                )
            }
        },
    )
}` : ''}

@Composable fun TachyonNativeNode(
    node: JSONObject,
    dispatch: (String, String, String?) -> Unit,
    navigate: (String) -> Unit,
    theme: String,
    setTheme: (String) -> Unit,
) {
    if (node.optString("kind") == "text") { Text(node.optString("value")); return }
    ${hybrid ? 'if (node.optString("kind") == "webview") { TachyonHybridWebView(node.optString("html"), navigate, theme, setTheme); return }' : ''}
    val tag = node.optString("tag")
    val id = node.optString("id")
    val children = node.optJSONArray("children")
    val content: @Composable () -> Unit = {
        if (children != null) for (index in 0 until children.length())
            TachyonNativeNode(children.getJSONObject(index), dispatch, navigate, theme, setTheme)
    }
    when (tag) {
        "h1" -> Text(textContent(node), style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
        "h2" -> Text(textContent(node), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        "h3", "h4", "h5", "h6" -> Text(textContent(node), style = MaterialTheme.typography.headlineSmall)
        "p", "span", "label", "strong", "em", "small", "code", "pre" -> Text(textContent(node))
        "button" -> Button(onClick = { dispatch(id, "click", null) }) { Text(textContent(node)) }
        "input" -> { var value by remember { mutableStateOf(node.optJSONObject("attributes")?.optString("value") ?: "") }
            TextField(value = value, onValueChange = { value = it; dispatch(id, "input", it) }) }
        "tr" -> Row { content() }
        else -> Column { content() }
    }
}
`;
}

/** @param {any} host */
function linuxHostSupport(host) {
    const managed = host.managedContentOrigins.length > 0;
    const capture = host.requestedDevicePermissions.has('screenCapture');
    const capabilityChecks = /** @type {string[]} */ (host.hostCapabilities).map((capability) => `g_strcmp0(capability, ${JSON.stringify(capability)}) == 0`).join(' || ') || 'FALSE';
    const allowedOrigins = /** @type {string[]} */ (host.managedContentOrigins).map((origin) => `g_strcmp0(origin, ${JSON.stringify(origin)}) == 0`).join(' || ') || 'FALSE';
    const permissionChecks = /** @type {[string, string[]][]} */ (Object.entries(host.permissionOrigins)).flatMap(([permission, origins]) =>
        origins.map((origin) => `(g_strcmp0(permission, ${JSON.stringify(permission)}) == 0 && g_strcmp0(origin, ${JSON.stringify(origin)}) == 0)`),
    ).join(' || ') || 'FALSE';
    const managedSupport = managed ? `
typedef struct { char* id; WebKitWebView* view; gboolean persistent; } TachyonManagedSurface;
static GHashTable* managed_surfaces = NULL;
static TachyonManagedSurface* active_managed_surface = NULL;

static char* exact_origin(const char* uri) {
    GError* error = NULL;
    GUri* parsed = g_uri_parse(uri, G_URI_FLAGS_NONE, &error);
    if (!parsed || g_strcmp0(g_uri_get_scheme(parsed), "https") != 0 || !g_uri_get_host(parsed)) {
        if (parsed) g_uri_unref(parsed); if (error) g_error_free(error); return NULL;
    }
    int port = g_uri_get_port(parsed);
    char* origin = port > 0 && port != 443
        ? g_strdup_printf("https://%s:%d", g_uri_get_host(parsed), port)
        : g_strdup_printf("https://%s", g_uri_get_host(parsed));
    g_uri_unref(parsed); if (error) g_error_free(error); return origin;
}

static gboolean managed_origin_allowed(const char* origin) { return origin && (${allowedOrigins}); }
static gboolean permission_origin_allowed(const char* permission, const char* origin) { return origin && (${permissionChecks}); }

static gboolean managed_decide_policy(WebKitWebView* view, WebKitPolicyDecision* decision, WebKitPolicyDecisionType type, gpointer data) {
    (void)view; (void)data;
    if (type != WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION && type != WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION) return FALSE;
    if (type == WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION) { webkit_policy_decision_ignore(decision); return TRUE; }
    WebKitNavigationPolicyDecision* navigation = WEBKIT_NAVIGATION_POLICY_DECISION(decision);
    WebKitURIRequest* request = webkit_navigation_action_get_request(webkit_navigation_policy_decision_get_navigation_action(navigation));
    char* origin = exact_origin(webkit_uri_request_get_uri(request));
    gboolean allowed = managed_origin_allowed(origin); g_free(origin);
    if (allowed) webkit_policy_decision_use(decision); else webkit_policy_decision_ignore(decision);
    return TRUE;
}

static gboolean managed_permission_request(WebKitWebView* view, WebKitPermissionRequest* request, gpointer data) {
    (void)data;
    const char* uri = webkit_web_view_get_uri(view);
    char* origin = exact_origin(uri ? uri : "");
    gboolean allowed = FALSE;
    if (WEBKIT_IS_USER_MEDIA_PERMISSION_REQUEST(request)) {
        WebKitUserMediaPermissionRequest* media = WEBKIT_USER_MEDIA_PERMISSION_REQUEST(request);
        gboolean audio = webkit_user_media_permission_is_for_audio_device(media);
        gboolean video = webkit_user_media_permission_is_for_video_device(media);
        allowed = (!audio || permission_origin_allowed("microphone", origin))
            && (!video || permission_origin_allowed("camera", origin));
    }
    g_free(origin);
    if (allowed) webkit_permission_request_allow(request); else webkit_permission_request_deny(request);
    return TRUE;
}

static void attach_managed_surface(TachyonManagedSurface* surface) {
    if (!surface || !managed_slot) return;
    if (active_managed_surface && active_managed_surface != surface) {
        GtkWidget* previous = GTK_WIDGET(active_managed_surface->view);
        if (gtk_widget_get_parent(previous)) gtk_container_remove(GTK_CONTAINER(managed_slot), previous);
    }
    active_managed_surface = surface;
    GtkWidget* view = GTK_WIDGET(surface->view);
    if (!gtk_widget_get_parent(view)) gtk_container_add(GTK_CONTAINER(managed_slot), view);
    gtk_widget_show_all(managed_slot);
    if (main_layout) gtk_paned_set_position(GTK_PANED(main_layout), MAX(1, gtk_widget_get_allocated_width(main_layout) / 4));
}

static void managed_surface_free(gpointer value) {
    TachyonManagedSurface* surface = (TachyonManagedSurface*)value;
    if (!surface) return;
    GtkWidget* view = GTK_WIDGET(surface->view);
    if (gtk_widget_get_parent(view)) gtk_container_remove(GTK_CONTAINER(managed_slot), view);
    if (active_managed_surface == surface) active_managed_surface = NULL;
    g_object_unref(surface->view);
    g_free(surface->id); g_free(surface);
}

static TachyonManagedSurface* require_surface(JsonObject* payload, char** error) {
    const char* id = json_object_get_string_member_with_default(payload, "id", "");
    TachyonManagedSurface* surface = managed_surfaces ? (TachyonManagedSurface*)g_hash_table_lookup(managed_surfaces, id) : NULL;
    if (!surface && error) *error = duplicate_host_text("Unknown managed content surface");
    return surface;
}
` : '';
    const managedCases = managed ? `
    if (g_strcmp0(capability, "contentSurface.open") == 0) {
        const char* id = json_object_get_string_member_with_default(payload, "id", "");
        const char* url = json_object_get_string_member_with_default(payload, "url", "");
        char* origin = exact_origin(url);
        if (!g_regex_match_simple("^[A-Za-z0-9._-]{1,128}$", id, 0, 0) || !managed_origin_allowed(origin)) { g_free(origin); result = fail_host(error, "Managed content id or origin is not allowed"); goto done; }
        g_free(origin);
        if (!managed_surfaces) managed_surfaces = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, managed_surface_free);
        if (g_hash_table_contains(managed_surfaces, id)) { result = fail_host(error, "Managed content surface already exists"); goto done; }
        TachyonManagedSurface* surface = g_new0(TachyonManagedSurface, 1); surface->id = g_strdup(id); surface->persistent = json_object_get_boolean_member_with_default(payload, "persistentSession", FALSE);
        WebKitWebContext* context = surface->persistent
            ? webkit_web_context_get_default() : webkit_web_context_new_ephemeral();
        surface->view = WEBKIT_WEB_VIEW(webkit_web_view_new_with_context(context));
        g_object_ref_sink(surface->view);
        if (!surface->persistent) g_object_unref(context);
        g_signal_connect(surface->view, "decide-policy", G_CALLBACK(managed_decide_policy), NULL);
        g_signal_connect(surface->view, "permission-request", G_CALLBACK(managed_permission_request), NULL);
        g_hash_table_insert(managed_surfaces, g_strdup(id), surface); attach_managed_surface(surface);
        webkit_web_view_load_uri(surface->view, url);
        char* value = g_strdup_printf("{\\\"id\\\":\\\"%s\\\",\\\"open\\\":true,\\\"pending\\\":false,\\\"presentation\\\":\\\"composed\\\"}", surface->id); result = success_json(value, result_json); g_free(value); goto done;
    }
    if (g_strcmp0(capability, "contentSurface.state") == 0) {
        TachyonManagedSurface* surface = require_surface(payload, error); if (!surface) goto done;
        const char* uri = webkit_web_view_get_uri(surface->view); JsonBuilder* builder = json_builder_new(); json_builder_begin_object(builder); json_builder_set_member_name(builder, "id"); json_builder_add_string_value(builder, surface->id); json_builder_set_member_name(builder, "open"); json_builder_add_boolean_value(builder, TRUE); json_builder_set_member_name(builder, "presentation"); json_builder_add_string_value(builder, "composed"); json_builder_set_member_name(builder, "persistentSession"); json_builder_add_boolean_value(builder, surface->persistent); json_builder_set_member_name(builder, "url"); json_builder_add_string_value(builder, uri ? uri : ""); json_builder_set_member_name(builder, "canGoBack"); json_builder_add_boolean_value(builder, webkit_web_view_can_go_back(surface->view)); json_builder_set_member_name(builder, "canGoForward"); json_builder_add_boolean_value(builder, webkit_web_view_can_go_forward(surface->view)); json_builder_end_object(builder);
        JsonNode* node = json_builder_get_root(builder); JsonGenerator* generator = json_generator_new(); json_generator_set_root(generator, node); char* value = json_generator_to_data(generator, NULL); result = success_json(value, result_json); g_free(value); g_object_unref(generator); json_node_free(node); g_object_unref(builder); goto done;
    }
    if (g_str_has_prefix(capability, "contentSurface.")) {
        TachyonManagedSurface* surface = require_surface(payload, error); if (!surface) goto done;
        if (g_strcmp0(capability, "contentSurface.navigate") == 0) {
            const char* url = json_object_get_string_member_with_default(payload, "url", ""); char* origin = exact_origin(url);
            if (!managed_origin_allowed(origin)) { g_free(origin); result = fail_host(error, "Managed content origin is not allowed"); goto done; }
            g_free(origin); webkit_web_view_load_uri(surface->view, url);
        } else if (g_strcmp0(capability, "contentSurface.goBack") == 0 && webkit_web_view_can_go_back(surface->view)) webkit_web_view_go_back(surface->view);
        else if (g_strcmp0(capability, "contentSurface.goForward") == 0 && webkit_web_view_can_go_forward(surface->view)) webkit_web_view_go_forward(surface->view);
        else if (g_strcmp0(capability, "contentSurface.reload") == 0) webkit_web_view_reload(surface->view);
        else if (g_strcmp0(capability, "contentSurface.close") == 0) { char* id = g_strdup(surface->id); gboolean was_active = surface == active_managed_surface; if (was_active) active_managed_surface = NULL; g_hash_table_remove(managed_surfaces, id); if (was_active && g_hash_table_size(managed_surfaces) > 0) { GHashTableIter iterator; gpointer next_value = NULL; g_hash_table_iter_init(&iterator, managed_surfaces); if (g_hash_table_iter_next(&iterator, NULL, &next_value)) attach_managed_surface((TachyonManagedSurface*)next_value); } else if (was_active && managed_slot) gtk_widget_hide(managed_slot); char* value = g_strdup_printf("{\\\"id\\\":\\\"%s\\\",\\\"open\\\":false,\\\"presentation\\\":\\\"composed\\\"}", id); result = success_json(value, result_json); g_free(value); g_free(id); goto done; }
        result = success_json("{\\\"open\\\":true}", result_json); goto done;
    }
` : '';
    const captureSupport = capture ? `
static gboolean screen_capture_granted = FALSE;

static gboolean portal_window_capture_available(void) {
    GDBusProxy* proxy = portal_proxy("org.freedesktop.portal.Screenshot");
    if (!proxy) return FALSE;
    GVariant* targets = g_dbus_proxy_get_cached_property(proxy, "AvailableTargets");
    guint32 available = 0;
    if (targets) { available = g_variant_get_uint32(targets); g_variant_unref(targets); }
    g_object_unref(proxy);
    return (available & 2u) != 0;
}

static char* portal_capture_window(GError** error) {
    GVariantBuilder options; g_variant_builder_init(&options, G_VARIANT_TYPE_VARDICT);
    char* token = portal_token("screenshot");
    g_variant_builder_add(&options, "{sv}", "handle_token", g_variant_new_string(token));
    g_variant_builder_add(&options, "{sv}", "interactive", g_variant_new_boolean(TRUE));
    g_variant_builder_add(&options, "{sv}", "target", g_variant_new_uint32(2));
    g_free(token);
    GVariant* results = portal_request("org.freedesktop.portal.Screenshot", "Screenshot",
        g_variant_new("(s@a{sv})", "", g_variant_builder_end(&options)), error);
    if (!results) return NULL;
    const char* uri = NULL;
    char* copy = g_variant_lookup(results, "uri", "&s", &uri) && uri ? g_strdup(uri) : NULL;
    if (!copy && error && !*error) *error = g_error_new_literal(G_IO_ERROR, G_IO_ERROR_FAILED, "The screenshot portal returned no image");
    g_variant_unref(results); return copy;
}

static int capture_portal_window(JsonObject* payload, char** result_json, char** error) {
    const char* id = json_object_get_string_member_with_default(payload, "windowId", "");
    const char* destination = json_object_get_string_member_with_default(payload, "destination", "clipboard");
    const char* format = json_object_get_string_member_with_default(payload, "format", "png");
    if (g_strcmp0(id, "portal:window-picker") != 0) return fail_host(error, "Linux screen capture requires the portal:window-picker windowId");
    if (g_strcmp0(format, "png") != 0) return fail_host(error, "Linux screen capture supports PNG only");
    gboolean to_clipboard = g_strcmp0(destination, "clipboard") == 0 || g_strcmp0(destination, "both") == 0;
    gboolean to_file = g_strcmp0(destination, "file") == 0 || g_strcmp0(destination, "both") == 0;
    if (!to_clipboard && !to_file) return fail_host(error, "Screen capture destination must be clipboard, file, or both");
    if (!portal_window_capture_available()) return fail_host(error, "Window capture is unsupported by the active desktop portal");

    GError* portal_error = NULL; char* uri = portal_capture_window(&portal_error);
    if (!uri) { int failed = fail_host(error, portal_error ? portal_error->message : "Screen capture was cancelled"); if (portal_error) g_error_free(portal_error); return failed; }
    GFile* source = g_file_new_for_uri(uri); char* contents = NULL; gsize length = 0;
    if (!g_file_load_contents(source, NULL, &contents, &length, NULL, &portal_error)) {
        int failed = fail_host(error, portal_error ? portal_error->message : "Unable to read portal screenshot");
        if (portal_error) g_error_free(portal_error); g_object_unref(source); g_free(uri); return failed;
    }
    if (to_clipboard) {
        GdkPixbufLoader* loader = gdk_pixbuf_loader_new_with_type("png", &portal_error);
        if (!loader || !gdk_pixbuf_loader_write(loader, (const guchar*)contents, length, &portal_error) || !gdk_pixbuf_loader_close(loader, &portal_error)) {
            int failed = fail_host(error, portal_error ? portal_error->message : "Unable to decode portal screenshot");
            if (portal_error) g_error_free(portal_error); if (loader) g_object_unref(loader); g_free(contents); g_object_unref(source); g_free(uri); return failed;
        }
        gtk_clipboard_set_image(gtk_clipboard_get(GDK_SELECTION_CLIPBOARD), gdk_pixbuf_loader_get_pixbuf(loader));
        g_object_unref(loader);
    }
    char* output_path = g_strdup("");
    if (to_file) {
        const char* downloads = g_get_user_special_dir(G_USER_DIRECTORY_DOWNLOAD);
        if (!downloads) downloads = g_get_tmp_dir();
        g_free(output_path); output_path = g_strdup_printf("%s/tachyon-capture-%" G_GINT64_FORMAT ".png", downloads, g_get_real_time());
        if (!g_file_set_contents(output_path, contents, (gssize)length, &portal_error)) {
            int failed = fail_host(error, portal_error ? portal_error->message : "Unable to save portal screenshot");
            if (portal_error) g_error_free(portal_error); g_free(output_path); g_free(contents); g_object_unref(source); g_free(uri); return failed;
        }
    }
    screen_capture_granted = TRUE;
    JsonBuilder* builder = json_builder_new(); json_builder_begin_object(builder);
    json_builder_set_member_name(builder, "windowId"); json_builder_add_string_value(builder, id);
    json_builder_set_member_name(builder, "destination"); json_builder_add_string_value(builder, destination);
    json_builder_set_member_name(builder, "format"); json_builder_add_string_value(builder, "png");
    json_builder_set_member_name(builder, "bytes"); json_builder_add_int_value(builder, (gint64)length);
    json_builder_set_member_name(builder, "clipboard"); json_builder_add_boolean_value(builder, to_clipboard);
    json_builder_set_member_name(builder, "path"); json_builder_add_string_value(builder, output_path);
    json_builder_end_object(builder); int result = success_builder(builder, result_json);
    g_object_unref(builder); g_free(output_path); g_free(contents); g_object_unref(source); g_free(uri); return result;
}
` : '';
    const captureCases = capture ? `
    if (g_strcmp0(capability, "screenCapture.state") == 0) {
        gboolean supported = portal_window_capture_available();
        char* value = g_strdup_printf("{\\\"supported\\\":%s,\\\"permission\\\":\\\"%s\\\",\\\"format\\\":\\\"png\\\",\\\"destinations\\\":[\\\"clipboard\\\",\\\"file\\\",\\\"both\\\"]}", supported ? "true" : "false", supported ? (screen_capture_granted ? "granted" : "prompt") : "unsupported");
        result = success_json(value, result_json); g_free(value); goto done;
    }
    if (g_strcmp0(capability, "screenCapture.listWindows") == 0) {
        if (!portal_window_capture_available()) { result = success_json("{\\\"windows\\\":[],\\\"permission\\\":\\\"unsupported\\\"}", result_json); goto done; }
        result = success_json(screen_capture_granted
            ? "{\\\"windows\\\":[{\\\"windowId\\\":\\\"portal:window-picker\\\",\\\"title\\\":\\\"Choose a window…\\\",\\\"application\\\":\\\"XDG Desktop Portal\\\",\\\"frame\\\":{\\\"x\\\":0,\\\"y\\\":0,\\\"width\\\":0,\\\"height\\\":0}}],\\\"permission\\\":\\\"granted\\\"}"
            : "{\\\"windows\\\":[{\\\"windowId\\\":\\\"portal:window-picker\\\",\\\"title\\\":\\\"Choose a window…\\\",\\\"application\\\":\\\"XDG Desktop Portal\\\",\\\"frame\\\":{\\\"x\\\":0,\\\"y\\\":0,\\\"width\\\":0,\\\"height\\\":0}}],\\\"permission\\\":\\\"prompt\\\"}", result_json); goto done;
    }
    if (g_strcmp0(capability, "screenCapture.captureWindow") == 0) { result = capture_portal_window(payload, result_json, error); goto done; }
` : '';
    const desktopSupport = `
#define PORTAL_BUS "org.freedesktop.portal.Desktop"
#define PORTAL_PATH "/org/freedesktop/portal/desktop"
${capture ? 'static gboolean portal_window_capture_available(void);\nstatic gboolean screen_capture_granted;' : ''}

typedef struct { GMainLoop* loop; const char* path; guint response; GVariant* results; gboolean received; } PortalResponse;
typedef struct { char* id; char* accelerator; char* session; int keycode; unsigned int modifiers; } LinuxShortcut;
static GPtrArray* linux_shortcuts = NULL;
static gboolean window_click_through = FALSE;
static guint portal_shortcut_subscription = 0;
static gboolean x11_shortcut_grab_failed = FALSE;

static void linux_shortcut_free(gpointer value) {
    LinuxShortcut* shortcut = (LinuxShortcut*)value; if (!shortcut) return;
    if (shortcut->session) {
        GDBusConnection* bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, NULL);
        if (bus) { g_dbus_connection_call_sync(bus, PORTAL_BUS, shortcut->session, "org.freedesktop.portal.Session", "Close", NULL, NULL, G_DBUS_CALL_FLAGS_NONE, 2000, NULL, NULL); g_object_unref(bus); }
    }
    g_free(shortcut->id); g_free(shortcut->accelerator); g_free(shortcut->session); g_free(shortcut);
}

static LinuxShortcut* find_linux_shortcut(const char* id, guint* index) {
    for (guint i = 0; linux_shortcuts && i < linux_shortcuts->len; i++) {
        LinuxShortcut* shortcut = (LinuxShortcut*)g_ptr_array_index(linux_shortcuts, i);
        if (g_strcmp0(shortcut->id, id) == 0) { if (index) *index = i; return shortcut; }
    }
    return NULL;
}

static GDBusProxy* portal_proxy(const char* interface_name) {
    GError* error = NULL; GDBusProxy* proxy = g_dbus_proxy_new_for_bus_sync(G_BUS_TYPE_SESSION, G_DBUS_PROXY_FLAGS_NONE,
        NULL, PORTAL_BUS, PORTAL_PATH, interface_name, NULL, &error);
    if (!proxy || !g_dbus_proxy_get_name_owner(proxy)) { if (proxy) g_object_unref(proxy); proxy = NULL; }
    if (error) g_error_free(error); return proxy;
}

static char* portal_token(const char* prefix) {
    static guint counter = 0; return g_strdup_printf("tachyon_%s_%u_%u", prefix, (guint)getpid(), ++counter);
}

static void portal_response_signal(GDBusConnection* bus, const char* sender, const char* path, const char* interface_name, const char* signal, GVariant* parameters, gpointer data) {
    (void)bus; (void)sender; (void)path; (void)interface_name; (void)signal;
    PortalResponse* response = (PortalResponse*)data;
    if (response->path && g_strcmp0(response->path, path) != 0) return;
    g_variant_get(parameters, "(u@a{sv})", &response->response, &response->results);
    response->received = TRUE; g_main_loop_quit(response->loop);
}

static gboolean portal_request_timeout(gpointer data) { PortalResponse* response = (PortalResponse*)data; if (!response->received) g_main_loop_quit(response->loop); return G_SOURCE_REMOVE; }

static GVariant* portal_request(const char* interface_name, const char* method, GVariant* parameters, GError** error) {
    GDBusConnection* bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, error); if (!bus) return NULL;
    PortalResponse response = { g_main_loop_new(NULL, FALSE), NULL, 2, NULL, FALSE };
    // Subscribe before invoking the portal method. A backend may emit Response
    // immediately after returning its handle, so subscribing afterwards loses
    // valid fast responses (the race described by the Request portal contract).
    guint subscription = g_dbus_connection_signal_subscribe(bus, PORTAL_BUS, "org.freedesktop.portal.Request", "Response",
        NULL, NULL, G_DBUS_SIGNAL_FLAGS_NONE, portal_response_signal, &response, NULL);
    GVariant* reply = g_dbus_connection_call_sync(bus, PORTAL_BUS, PORTAL_PATH, interface_name, method, parameters,
        G_VARIANT_TYPE("(o)"), G_DBUS_CALL_FLAGS_NONE, -1, NULL, error);
    if (!reply) {
        g_dbus_connection_signal_unsubscribe(bus, subscription); g_main_loop_unref(response.loop); g_object_unref(bus); return NULL;
    }
    const char* request_path = NULL; g_variant_get(reply, "(&o)", &request_path);
    response.path = request_path;
    guint timeout = g_timeout_add_seconds(120, portal_request_timeout, &response); g_main_loop_run(response.loop);
    if (response.received) g_source_remove(timeout);
    g_dbus_connection_signal_unsubscribe(bus, subscription); g_main_loop_unref(response.loop); g_variant_unref(reply); g_object_unref(bus);
    if (!response.received || response.response != 0) {
        if (response.results) g_variant_unref(response.results);
        if (error && !*error) *error = g_error_new_literal(G_IO_ERROR, response.received ? G_IO_ERROR_CANCELLED : G_IO_ERROR_TIMED_OUT, response.received ? "Portal request was cancelled" : "Portal request timed out");
        return NULL;
    }
    return response.results;
}

static gboolean is_x11_backend(void) { GdkDisplay* display = gdk_display_get_default(); return display && GDK_IS_X11_DISPLAY(display); }
static gboolean global_shortcuts_portal_available(void) {
    GDBusProxy* proxy = portal_proxy("org.freedesktop.portal.GlobalShortcuts"); if (!proxy) return FALSE;
    GVariant* version = g_dbus_proxy_get_cached_property(proxy, "version"); gboolean available = version && g_variant_get_uint32(version) >= 1;
    if (version) g_variant_unref(version); g_object_unref(proxy); return available;
}

static char* xdg_shortcut_trigger(const char* accelerator) {
    char** parts = g_strsplit(accelerator ? accelerator : "", "+", -1); GString* output = g_string_new(NULL); int count = g_strv_length(parts);
    for (int i = 0; i < count; i++) {
        char* upper = g_ascii_strup(parts[i], -1); const char* mapped = upper;
        if (g_strcmp0(upper, "PRIMARY") == 0 || g_strcmp0(upper, "CONTROL") == 0) mapped = "CTRL";
        else if (g_strcmp0(upper, "COMMAND") == 0 || g_strcmp0(upper, "META") == 0 || g_strcmp0(upper, "SUPER") == 0) mapped = "LOGO";
        else if (g_strcmp0(upper, "OPTION") == 0) mapped = "ALT";
        if (output->len) g_string_append_c(output, '+');
        if (i == count - 1 && strlen(mapped) == 1) g_string_append_c(output, g_ascii_tolower(mapped[0])); else g_string_append(output, mapped);
        g_free(upper);
    }
    g_strfreev(parts); return g_string_free(output, FALSE);
}

static gboolean parse_x11_accelerator(const char* accelerator, int* keycode, unsigned int* modifiers) {
    char** parts = g_strsplit(accelerator ? accelerator : "", "+", -1); int count = g_strv_length(parts); *modifiers = 0; KeySym symbol = NoSymbol;
    for (int i = 0; i < count; i++) {
        char* upper = g_ascii_strup(parts[i], -1);
        if (g_strcmp0(upper, "PRIMARY") == 0 || g_strcmp0(upper, "CONTROL") == 0 || g_strcmp0(upper, "CTRL") == 0) *modifiers |= ControlMask;
        else if (g_strcmp0(upper, "SHIFT") == 0) *modifiers |= ShiftMask;
        else if (g_strcmp0(upper, "ALT") == 0 || g_strcmp0(upper, "OPTION") == 0) *modifiers |= Mod1Mask;
        else if (g_strcmp0(upper, "COMMAND") == 0 || g_strcmp0(upper, "META") == 0 || g_strcmp0(upper, "SUPER") == 0) *modifiers |= Mod4Mask;
        else if (i == count - 1) { char* key = strlen(parts[i]) == 1 ? g_ascii_strdown(parts[i], -1) : g_strdup(parts[i]); symbol = XStringToKeysym(key); g_free(key); }
        else { g_free(upper); g_strfreev(parts); return FALSE; }
        g_free(upper);
    }
    Display* display = is_x11_backend() ? gdk_x11_display_get_xdisplay(gdk_display_get_default()) : NULL;
    *keycode = display && symbol != NoSymbol ? XKeysymToKeycode(display, symbol) : 0; g_strfreev(parts);
    return *keycode != 0 && *modifiers != 0;
}

static void emit_linux_host_event(const char* name, const char* shortcut_id) {
    if (!controller) return; JsonBuilder* builder = json_builder_new(); json_builder_begin_object(builder);
    json_builder_set_member_name(builder, "type"); json_builder_add_string_value(builder, "tac:host-event");
    json_builder_set_member_name(builder, "event"); json_builder_add_string_value(builder, name);
    json_builder_set_member_name(builder, "payload"); json_builder_begin_object(builder); json_builder_set_member_name(builder, "id"); json_builder_add_string_value(builder, shortcut_id); json_builder_end_object(builder); json_builder_end_object(builder);
    JsonGenerator* generator = json_generator_new(); JsonNode* node = json_builder_get_root(builder); json_generator_set_root(generator, node); char* event = json_generator_to_data(generator, NULL); char* error = NULL;
    char* snapshot = tachyon_ui_controller_emit(controller, event, &error); if (snapshot) { apply_snapshot(snapshot); tachyon_ui_controller_free_string(snapshot); } else if (error) g_warning("Tachyon host event: %s", error);
    tachyon_ui_controller_free_string(error); g_free(event); json_node_free(node); g_object_unref(generator); g_object_unref(builder);
}

static GdkFilterReturn x11_shortcut_filter(GdkXEvent* native_event, GdkEvent* event, gpointer data) {
    (void)event; (void)data; XEvent* xevent = (XEvent*)native_event; if (xevent->type != KeyPress) return GDK_FILTER_CONTINUE;
    unsigned int state = xevent->xkey.state & ~(LockMask | Mod2Mask);
    for (guint i = 0; linux_shortcuts && i < linux_shortcuts->len; i++) { LinuxShortcut* shortcut = (LinuxShortcut*)g_ptr_array_index(linux_shortcuts, i); if (shortcut->keycode == xevent->xkey.keycode && shortcut->modifiers == state) { emit_linux_host_event("shortcut.activated", shortcut->id); return GDK_FILTER_REMOVE; } }
    return GDK_FILTER_CONTINUE;
}

static void portal_shortcut_activated(GDBusConnection* bus, const char* sender, const char* path, const char* interface_name, const char* signal, GVariant* parameters, gpointer data) {
    (void)bus; (void)sender; (void)path; (void)interface_name; (void)signal; (void)data;
    const char* session = NULL; const char* id = NULL; guint64 timestamp = 0; GVariant* options = NULL;
    g_variant_get(parameters, "(&o&st@a{sv})", &session, &id, &timestamp, &options); (void)timestamp;
    LinuxShortcut* shortcut = find_linux_shortcut(id, NULL); if (shortcut && g_strcmp0(shortcut->session, session) == 0) emit_linux_host_event("shortcut.activated", shortcut->id);
    g_variant_unref(options);
}

static gboolean bind_portal_shortcut(LinuxShortcut* shortcut, GError** error) {
    GVariantBuilder session_options; g_variant_builder_init(&session_options, G_VARIANT_TYPE_VARDICT);
    char* request_token = portal_token("shortcut"); char* session_token = portal_token("session");
    g_variant_builder_add(&session_options, "{sv}", "handle_token", g_variant_new_string(request_token));
    g_variant_builder_add(&session_options, "{sv}", "session_handle_token", g_variant_new_string(session_token)); g_free(request_token); g_free(session_token);
    GVariant* created = portal_request("org.freedesktop.portal.GlobalShortcuts", "CreateSession", g_variant_new("(@a{sv})", g_variant_builder_end(&session_options)), error);
    if (!created) return FALSE; const char* session = NULL;
    if (!g_variant_lookup(created, "session_handle", "&s", &session) || !session) { g_variant_unref(created); if (error && !*error) *error = g_error_new_literal(G_IO_ERROR, G_IO_ERROR_FAILED, "GlobalShortcuts portal returned no session"); return FALSE; }
    shortcut->session = g_strdup(session); g_variant_unref(created);
    char* trigger = xdg_shortcut_trigger(shortcut->accelerator);
    GVariantBuilder properties; g_variant_builder_init(&properties, G_VARIANT_TYPE_VARDICT);
    g_variant_builder_add(&properties, "{sv}", "description", g_variant_new_string(shortcut->id));
    g_variant_builder_add(&properties, "{sv}", "preferred_trigger", g_variant_new_string(trigger)); g_free(trigger);
    GVariantBuilder shortcuts; g_variant_builder_init(&shortcuts, G_VARIANT_TYPE("a(sa{sv})"));
    g_variant_builder_add(&shortcuts, "(s@a{sv})", shortcut->id, g_variant_builder_end(&properties));
    GVariantBuilder bind_options; g_variant_builder_init(&bind_options, G_VARIANT_TYPE_VARDICT); request_token = portal_token("bind");
    g_variant_builder_add(&bind_options, "{sv}", "handle_token", g_variant_new_string(request_token)); g_free(request_token);
    GVariant* bound = portal_request("org.freedesktop.portal.GlobalShortcuts", "BindShortcuts",
        g_variant_new("(o@a(sa{sv})s@a{sv})", shortcut->session, g_variant_builder_end(&shortcuts), "", g_variant_builder_end(&bind_options)), error);
    if (!bound) return FALSE;
    GVariant* bound_shortcuts = g_variant_lookup_value(bound, "shortcuts", G_VARIANT_TYPE("a(sa{sv})")); gboolean accepted = FALSE;
    if (bound_shortcuts) { GVariantIter iterator; const char* bound_id = NULL; GVariant* properties = NULL; g_variant_iter_init(&iterator, bound_shortcuts); while (g_variant_iter_next(&iterator, "(&s@a{sv})", &bound_id, &properties)) { if (g_strcmp0(bound_id, shortcut->id) == 0) accepted = TRUE; g_variant_unref(properties); } g_variant_unref(bound_shortcuts); }
    g_variant_unref(bound);
    if (!accepted) { if (error && !*error) *error = g_error_new_literal(G_IO_ERROR, G_IO_ERROR_PERMISSION_DENIED, "The desktop portal did not bind the requested shortcut"); return FALSE; }
    if (!portal_shortcut_subscription) { GDBusConnection* bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, error); if (!bus) return FALSE; portal_shortcut_subscription = g_dbus_connection_signal_subscribe(bus, PORTAL_BUS, "org.freedesktop.portal.GlobalShortcuts", "Activated", PORTAL_PATH, NULL, G_DBUS_SIGNAL_FLAGS_NONE, portal_shortcut_activated, NULL, NULL); g_object_unref(bus); }
    return TRUE;
}

static int x11_shortcut_error(Display* display, XErrorEvent* event) { (void)display; if (event->error_code == BadAccess) x11_shortcut_grab_failed = TRUE; return 0; }

static gboolean grab_x11_shortcut(LinuxShortcut* shortcut, GError** error) {
    if (!parse_x11_accelerator(shortcut->accelerator, &shortcut->keycode, &shortcut->modifiers)) { if (error) *error = g_error_new_literal(G_IO_ERROR, G_IO_ERROR_INVALID_ARGUMENT, "Invalid shortcut accelerator"); return FALSE; }
    Display* display = gdk_x11_display_get_xdisplay(gdk_display_get_default()); Window root = DefaultRootWindow(display); unsigned int locks[] = { 0, LockMask, Mod2Mask, LockMask | Mod2Mask };
    XSync(display, False); x11_shortcut_grab_failed = FALSE; int (*previous_handler)(Display*, XErrorEvent*) = XSetErrorHandler(x11_shortcut_error);
    for (guint i = 0; i < G_N_ELEMENTS(locks); i++) XGrabKey(display, shortcut->keycode, shortcut->modifiers | locks[i], root, False, GrabModeAsync, GrabModeAsync);
    XSync(display, False); XSetErrorHandler(previous_handler);
    if (x11_shortcut_grab_failed) { for (guint i = 0; i < G_N_ELEMENTS(locks); i++) XUngrabKey(display, shortcut->keycode, shortcut->modifiers | locks[i], root); XSync(display, False); if (error) *error = g_error_new_literal(G_IO_ERROR, G_IO_ERROR_EXISTS, "Shortcut accelerator is already registered"); return FALSE; }
    return TRUE;
}

static void ungrab_x11_shortcut(LinuxShortcut* shortcut) {
    if (!is_x11_backend() || !shortcut->keycode) return; Display* display = gdk_x11_display_get_xdisplay(gdk_display_get_default()); Window root = DefaultRootWindow(display); unsigned int locks[] = { 0, LockMask, Mod2Mask, LockMask | Mod2Mask };
    for (guint i = 0; i < G_N_ELEMENTS(locks); i++) XUngrabKey(display, shortcut->keycode, shortcut->modifiers | locks[i], root); XSync(display, False);
}

static int success_builder(JsonBuilder* builder, char** result_json) {
    JsonNode* node = json_builder_get_root(builder); JsonGenerator* generator = json_generator_new(); json_generator_set_root(generator, node); char* value = json_generator_to_data(generator, NULL);
    int result = success_json(value, result_json); g_free(value); g_object_unref(generator); json_node_free(node); return result;
}

static void add_shortcut_snapshot(JsonBuilder* builder) {
    json_builder_set_member_name(builder, "shortcuts"); json_builder_begin_array(builder);
    for (guint i = 0; linux_shortcuts && i < linux_shortcuts->len; i++) { LinuxShortcut* shortcut = (LinuxShortcut*)g_ptr_array_index(linux_shortcuts, i); json_builder_begin_object(builder); json_builder_set_member_name(builder, "id"); json_builder_add_string_value(builder, shortcut->id); json_builder_set_member_name(builder, "accelerator"); json_builder_add_string_value(builder, shortcut->accelerator); json_builder_end_object(builder); }
    json_builder_end_array(builder);
}

static const char* linux_capability_state(const char* capability) {
    if (!capability_enabled(capability)) return "unsupported";
    if (g_str_has_prefix(capability, "shortcuts.")) return is_x11_backend() ? "granted" : (global_shortcuts_portal_available() ? (linux_shortcuts && linux_shortcuts->len ? "granted" : "prompt") : "unsupported");
    if (g_strcmp0(capability, "window.clickThrough") == 0) return window && gtk_widget_get_window(window) ? "granted" : "unsupported";
    ${capture ? 'if (g_str_has_prefix(capability, "screenCapture.")) return portal_window_capture_available() ? (screen_capture_granted ? "granted" : "prompt") : "unsupported";' : ''}
    return "granted";
}
`;
    const desktopCases = `
    if (g_strcmp0(capability, "window.clickThrough") == 0) {
        GdkWindow* native_window = gtk_widget_get_window(window); if (!native_window) { result = fail_host(error, "Native window is not ready for click-through"); goto done; }
        gboolean enabled = json_object_get_boolean_member_with_default(payload, "enabled", FALSE); cairo_region_t* empty = enabled ? cairo_region_create() : NULL;
        gdk_window_input_shape_combine_region(native_window, empty, 0, 0); if (empty) cairo_region_destroy(empty); window_click_through = enabled;
        char* value = g_strdup_printf("{\\\"alwaysOnTop\\\":%s,\\\"opacity\\\":%.3f,\\\"clickThrough\\\":%s}", window_always_on_top ? "true" : "false", gtk_widget_get_opacity(window), enabled ? "true" : "false"); result = success_json(value, result_json); g_free(value); goto done;
    }
    if (g_strcmp0(capability, "shortcuts.register") == 0) {
        const char* id = json_object_get_string_member_with_default(payload, "id", ""); const char* accelerator = json_object_get_string_member_with_default(payload, "accelerator", ""); gboolean replace = json_object_get_boolean_member_with_default(payload, "replace", FALSE); guint existing_index = 0; LinuxShortcut* existing = find_linux_shortcut(id, &existing_index);
        if (!*id || !*accelerator) { result = fail_host(error, "Shortcut id and accelerator are required"); goto done; }
        if (existing && !replace) { result = fail_host(error, "Shortcut id is already registered"); goto done; }
        if (existing) { ungrab_x11_shortcut(existing); g_ptr_array_remove_index(linux_shortcuts, existing_index); }
        if (!linux_shortcuts) linux_shortcuts = g_ptr_array_new_with_free_func(linux_shortcut_free);
        LinuxShortcut* shortcut = g_new0(LinuxShortcut, 1); shortcut->id = g_strdup(id); shortcut->accelerator = g_strdup(accelerator); GError* shortcut_error = NULL;
        gboolean registered = is_x11_backend() ? grab_x11_shortcut(shortcut, &shortcut_error) : (global_shortcuts_portal_available() && bind_portal_shortcut(shortcut, &shortcut_error));
        if (!registered) { result = fail_host(error, shortcut_error ? shortcut_error->message : "No supported global-shortcut backend is available"); if (shortcut_error) g_error_free(shortcut_error); linux_shortcut_free(shortcut); goto done; }
        g_ptr_array_add(linux_shortcuts, shortcut); JsonBuilder* builder = json_builder_new(); json_builder_begin_object(builder); add_shortcut_snapshot(builder); json_builder_set_member_name(builder, "shortcut"); json_builder_begin_object(builder); json_builder_set_member_name(builder, "id"); json_builder_add_string_value(builder, id); json_builder_set_member_name(builder, "accelerator"); json_builder_add_string_value(builder, accelerator); json_builder_end_object(builder); json_builder_end_object(builder); result = success_builder(builder, result_json); g_object_unref(builder); goto done;
    }
    if (g_strcmp0(capability, "shortcuts.unregister") == 0) {
        const char* id = json_object_get_string_member_with_default(payload, "id", ""); guint index = 0; LinuxShortcut* shortcut = find_linux_shortcut(id, &index); gboolean removed = shortcut != NULL; if (shortcut) { ungrab_x11_shortcut(shortcut); g_ptr_array_remove_index(linux_shortcuts, index); }
        JsonBuilder* builder = json_builder_new(); json_builder_begin_object(builder); add_shortcut_snapshot(builder); json_builder_set_member_name(builder, "unregistered"); json_builder_add_boolean_value(builder, removed); json_builder_end_object(builder); result = success_builder(builder, result_json); g_object_unref(builder); goto done;
    }
    if (g_strcmp0(capability, "shortcuts.unregisterAll") == 0) {
        guint count = linux_shortcuts ? linux_shortcuts->len : 0; for (guint i = 0; linux_shortcuts && i < linux_shortcuts->len; i++) ungrab_x11_shortcut((LinuxShortcut*)g_ptr_array_index(linux_shortcuts, i)); if (linux_shortcuts) g_ptr_array_set_size(linux_shortcuts, 0);
        JsonBuilder* builder = json_builder_new(); json_builder_begin_object(builder); add_shortcut_snapshot(builder); json_builder_set_member_name(builder, "unregistered"); json_builder_add_int_value(builder, count); json_builder_end_object(builder); result = success_builder(builder, result_json); g_object_unref(builder); goto done;
    }
    if (g_strcmp0(capability, "shortcuts.list") == 0) { JsonBuilder* builder = json_builder_new(); json_builder_begin_object(builder); add_shortcut_snapshot(builder); json_builder_end_object(builder); result = success_builder(builder, result_json); g_object_unref(builder); goto done; }
`;
    return `static char* duplicate_host_text(const char* value) {
    size_t length = value ? strlen(value) : 0; char* copy = (char*)malloc(length + 1);
    if (!copy) return NULL; if (length) memcpy(copy, value, length); copy[length] = 0; return copy;
}

static int fail_host(char** error, const char* message) { if (error) *error = duplicate_host_text(message); return 0; }
static int success_json(const char* value_json, char** result_json) {
    size_t length = strlen(value_json) + 32; char* envelope = (char*)malloc(length);
    if (!envelope) return 0; snprintf(envelope, length, "{\\\"ok\\\":true,\\\"value\\\":%s}", value_json);
    *result_json = envelope; return 1;
}
static gboolean capability_enabled(const char* capability) { return ${capabilityChecks}; }
static gboolean window_always_on_top = FALSE;

${managedSupport}
${desktopSupport}
${captureSupport}

static int handle_native_capability(const char* capability, const char* payload_json, char** result_json, char** error, void* user_data) {
    (void)user_data; int result = 0;
    if (g_strcmp0(capability, "__tachyon.hostInfo") == 0)
        return success_json(${JSON.stringify(JSON.stringify({ target: 'linux', platform: 'desktop', capabilities: host.hostCapabilities }))}, result_json);
    if (!capability_enabled(capability)) return fail_host(error, "Unsupported native capability");
    JsonParser* parser = json_parser_new();
    if (!json_parser_load_from_data(parser, payload_json ? payload_json : "{}", -1, NULL)) { g_object_unref(parser); return fail_host(error, "Invalid native capability payload"); }
    JsonObject* payload = json_node_get_object(json_parser_get_root(parser));
    if (g_strcmp0(capability, "capabilities.state") == 0) {
        const char* requested = json_object_get_string_member_with_default(payload, "capability", "");
        const char* state = linux_capability_state(requested); char* value = g_strdup_printf("\\\"%s\\\"", state); result = success_json(value, result_json); g_free(value); goto done;
    }
    if (g_strcmp0(capability, "app.info") == 0) { result = success_json(${JSON.stringify(JSON.stringify({ name: host.appName, runtime: 'linux-gtk', version: host.version }))}, result_json); goto done; }
    if (g_strcmp0(capability, "clipboard.readText") == 0) {
        GtkClipboard* clipboard = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD); char* text = gtk_clipboard_wait_for_text(clipboard); JsonNode* node = json_node_new(JSON_NODE_VALUE); json_node_set_string(node, text ? text : ""); JsonGenerator* generator = json_generator_new(); json_generator_set_root(generator, node); char* value = json_generator_to_data(generator, NULL);
        result = success_json(value, result_json); g_free(value); g_object_unref(generator); json_node_free(node); g_free(text); goto done;
    }
    if (g_strcmp0(capability, "clipboard.writeText") == 0) { const char* text = json_object_get_string_member_with_default(payload, "text", ""); gtk_clipboard_set_text(gtk_clipboard_get(GDK_SELECTION_CLIPBOARD), text, -1); result = success_json("{\\\"written\\\":true}", result_json); goto done; }
    if (g_strcmp0(capability, "openUrl") == 0) { const char* url = json_object_get_string_member_with_default(payload, "url", ""); if (!g_str_has_prefix(url, "https://") && !g_str_has_prefix(url, "http://")) { result = fail_host(error, "openUrl requires an http(s) URL"); goto done; } GError* launch_error = NULL; gboolean opened = g_app_info_launch_default_for_uri(url, NULL, &launch_error); if (!opened) { result = fail_host(error, launch_error ? launch_error->message : "Unable to open URL"); if (launch_error) g_error_free(launch_error); goto done; } result = success_json("{\\\"opened\\\":true}", result_json); goto done; }
    if (g_strcmp0(capability, "window.state") == 0) { char* value = g_strdup_printf("{\\\"alwaysOnTop\\\":%s,\\\"opacity\\\":%.3f,\\\"clickThrough\\\":%s}", window_always_on_top ? "true" : "false", gtk_widget_get_opacity(window), window_click_through ? "true" : "false"); result = success_json(value, result_json); g_free(value); goto done; }
    if (g_strcmp0(capability, "window.alwaysOnTop") == 0) { gboolean enabled = json_object_get_boolean_member_with_default(payload, "enabled", FALSE); gtk_window_set_keep_above(GTK_WINDOW(window), enabled); window_always_on_top = enabled; char* value = g_strdup_printf("{\\\"alwaysOnTop\\\":%s,\\\"opacity\\\":%.3f}", enabled ? "true" : "false", gtk_widget_get_opacity(window)); result = success_json(value, result_json); g_free(value); goto done; }
    if (g_strcmp0(capability, "window.opacity") == 0) { double opacity = json_object_get_double_member_with_default(payload, "value", 1.0); if (opacity < 0.1 || opacity > 1.0) { result = fail_host(error, "Window opacity must be between 0.1 and 1"); goto done; } gtk_widget_set_opacity(window, opacity); char* reply = g_strdup_printf("{\\\"alwaysOnTop\\\":%s,\\\"opacity\\\":%.3f}", window_always_on_top ? "true" : "false", opacity); result = success_json(reply, result_json); g_free(reply); goto done; }
    ${desktopCases}
    ${managedCases}
    ${captureCases}
    result = fail_host(error, "Unsupported native capability");
done:
    g_object_unref(parser); return result;
}
`;
}

/** @param {any} host */
function linuxNativeSource(host) {
    const hybrid = host.hasWebViewFallbacks;
    const managed = host.managedContentOrigins.length > 0;
    const needsWebKit = hybrid || host.managedContentOrigins.length > 0;
    return `#include <gtk/gtk.h>
#include <gdk/gdkx.h>
#include <json-glib/json-glib.h>
${needsWebKit ? '#include <webkit2/webkit2.h>' : ''}
#include <X11/Xlib.h>
#include <X11/keysym.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include "tachyon_ui_controller.h"

#define APP_NAME ${JSON.stringify(host.appName)}
typedef JsonObject TachyonNativeNode;
static TachyonUIController* controller = NULL;
static GtkWidget* window = NULL;
${managed ? 'static GtkWidget* main_layout = NULL;\nstatic GtkWidget* trusted_slot = NULL;\nstatic GtkWidget* managed_slot = NULL;' : ''}

static void apply_snapshot(const char* snapshot);

${linuxHostSupport(host)}

static void dispatch_click(GtkButton* button, gpointer user_data) {
    (void)button;
    JsonBuilder* builder = json_builder_new();
    json_builder_begin_object(builder);
    json_builder_set_member_name(builder, "elementId"); json_builder_add_string_value(builder, (const char*)user_data);
    json_builder_set_member_name(builder, "type"); json_builder_add_string_value(builder, "click");
    json_builder_end_object(builder);
    JsonGenerator* generator = json_generator_new(); json_generator_set_root(generator, json_builder_get_root(builder));
    char* event_json = json_generator_to_data(generator, NULL); char* error = NULL;
    char* snapshot = tachyon_ui_controller_dispatch(controller, event_json, &error);
    if (snapshot) { apply_snapshot(snapshot); tachyon_ui_controller_free_string(snapshot); }
    else g_warning("Tachyon controller: %s", error ? error : "dispatch failed");
    tachyon_ui_controller_free_string(error); g_free(event_json); g_object_unref(generator); g_object_unref(builder);
}

static GtkWidget* render_node(TachyonNativeNode* node) {
    const char* kind = json_object_get_string_member_with_default(node, "kind", "");
    if (g_strcmp0(kind, "text") == 0)
        return gtk_label_new(json_object_get_string_member_with_default(node, "value", ""));
    ${hybrid ? `if (g_strcmp0(kind, "webview") == 0) {
        const char* fragment = json_object_get_string_member_with_default(node, "html", "");
        char* document = g_strdup_printf(
            "<!doctype html><html><head><meta name=\\"viewport\\" content=\\"width=device-width,initial-scale=1\\">"
            "<link rel=\\"stylesheet\\" href=\\"imports.css\\"><script type=\\"module\\" src=\\"imports.js\\"></script>"
            "<style>html,body{margin:0;background:transparent;overflow:hidden}</style></head><body>%s</body></html>", fragment);
        char* resources = g_canonicalize_filename("Resources", NULL);
        char* base_uri = g_filename_to_uri(resources, NULL, NULL);
        GtkWidget* view = webkit_web_view_new();
        webkit_web_view_load_html(WEBKIT_WEB_VIEW(view), document, base_uri);
        gtk_widget_set_size_request(view, -1, 160);
        g_free(base_uri); g_free(resources); g_free(document);
        return view;
    }` : ''}
    const char* tag = json_object_get_string_member_with_default(node, "tag", "");
    JsonArray* children = json_object_has_member(node, "children") ? json_object_get_array_member(node, "children") : NULL;
    if (g_strcmp0(tag, "button") == 0) {
        GtkWidget* button = gtk_button_new();
        if (children && json_array_get_length(children))
            gtk_button_set_label(GTK_BUTTON(button), json_object_get_string_member_with_default(json_array_get_object_element(children, 0), "value", ""));
        if (json_object_has_member(node, "id") && json_object_get_string_member(node, "id"))
            g_signal_connect_data(button, "clicked", G_CALLBACK(dispatch_click),
                g_strdup(json_object_get_string_member(node, "id")), (GClosureNotify)g_free, 0);
        return button;
    }
    if (g_strcmp0(tag, "input") == 0) return gtk_entry_new();
    GtkWidget* box = gtk_box_new(g_strcmp0(tag, "tr") == 0 ? GTK_ORIENTATION_HORIZONTAL : GTK_ORIENTATION_VERTICAL, 8);
    for (guint i = 0; children && i < json_array_get_length(children); i++)
        gtk_box_pack_start(GTK_BOX(box), render_node(json_array_get_object_element(children, i)), FALSE, FALSE, 0);
    return box;
}

static void apply_snapshot(const char* snapshot) {
    JsonParser* parser = json_parser_new(); GError* error = NULL;
    if (!json_parser_load_from_data(parser, snapshot, -1, &error)) g_error("%s", error->message);
    TachyonNativeNode* root = json_object_get_object_member(json_node_get_object(json_parser_get_root(parser)), "root");
    ${managed ? `GtkWidget* child = gtk_bin_get_child(GTK_BIN(trusted_slot)); if (child) gtk_container_remove(GTK_CONTAINER(trusted_slot), child);
    gtk_container_add(GTK_CONTAINER(trusted_slot), render_node(root)); gtk_widget_show_all(window);
    if (!active_managed_surface) gtk_widget_hide(managed_slot);` : `GtkWidget* child = gtk_bin_get_child(GTK_BIN(window)); if (child) gtk_container_remove(GTK_CONTAINER(window), child);
    gtk_container_add(GTK_CONTAINER(window), render_node(root)); gtk_widget_show_all(window);`} g_object_unref(parser);
}

${managed ? `static void maintain_split_ratio(GtkWidget* widget, GtkAllocation* allocation, gpointer user_data) {
    (void)widget; (void)user_data;
    if (active_managed_surface) gtk_paned_set_position(GTK_PANED(main_layout), MAX(1, allocation->width / 4));
}` : ''}

static void activate(GtkApplication* app, gpointer user_data) {
    (void)user_data;
    char* error = NULL;
    controller = tachyon_ui_controller_create("Resources/tachyon.native-controller.js", handle_native_capability, NULL, &error);
    if (!controller) g_error("Tachyon controller: %s", error ? error : "initialization failed");
    window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), APP_NAME);
    gtk_window_set_default_size(GTK_WINDOW(window), 1000, 700);
    if (is_x11_backend()) gdk_window_add_filter(NULL, x11_shortcut_filter, NULL);
    ${managed ? `main_layout = gtk_paned_new(GTK_ORIENTATION_HORIZONTAL);
    trusted_slot = gtk_event_box_new(); managed_slot = gtk_event_box_new();
    gtk_paned_pack1(GTK_PANED(main_layout), trusted_slot, TRUE, FALSE);
    gtk_paned_pack2(GTK_PANED(main_layout), managed_slot, TRUE, FALSE);
    gtk_paned_set_position(GTK_PANED(main_layout), 250);
    g_signal_connect(main_layout, "size-allocate", G_CALLBACK(maintain_split_ratio), NULL);
    gtk_container_add(GTK_CONTAINER(window), main_layout);` : ''}
    char* snapshot = tachyon_ui_controller_render(controller, &error);
    if (!snapshot) g_error("Tachyon controller: %s", error ? error : "render failed");
    apply_snapshot(snapshot); tachyon_ui_controller_free_string(snapshot); tachyon_ui_controller_free_string(error);
}

int main(int argc, char** argv) {
    GtkApplication* app = gtk_application_new("${host.appId}", G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    ${managed ? 'if (managed_surfaces) g_hash_table_destroy(managed_surfaces);' : ''}
    if (is_x11_backend()) gdk_window_remove_filter(NULL, x11_shortcut_filter, NULL);
    if (linux_shortcuts) g_ptr_array_free(linux_shortcuts, TRUE);
    tachyon_ui_controller_destroy(controller);
    g_object_unref(app);
    return status;
}
`;
}

/** @param {any} host */
function windowsHostSupport(host) {
    const managed = host.managedContentOrigins.length > 0;
    const capture = host.requestedDevicePermissions.has('screenCapture');
    const checks = /** @type {string[]} */ (host.hostCapabilities).map((capability) => `capability == L${JSON.stringify(capability)}`).join(' || ') || 'false';
    const allowedOrigins = /** @type {string[]} */ (host.managedContentOrigins).map((origin) => `origin == L${JSON.stringify(origin)}`).join(' || ') || 'false';
    const permissionChecks = /** @type {[string, string[]][]} */ (Object.entries(host.permissionOrigins)).flatMap(([permission, origins]) =>
        origins.map((origin) => `(permission == L${JSON.stringify(permission)} && origin == L${JSON.stringify(origin)})`),
    ).join(' || ') || 'false';
    const managedTypes = managed ? `
struct TachyonManagedSurface { WebView2 view{ nullptr }; bool persistent = false; bool pending = true; };
static std::map<std::wstring, TachyonManagedSurface> managedSurfaces;
static WebView2 activeManagedView{ nullptr };
static std::wstring activeManagedSurfaceID;
static bool ManagedOriginAllowed(std::wstring const& origin) { return ${allowedOrigins}; }
static bool PermissionOriginAllowed(std::wstring const& permission, std::wstring const& origin) { return ${permissionChecks}; }

static void EnsureMainLayout() {
    if (mainLayout) return;
    mainLayout = Grid();
    ColumnDefinition trusted; trusted.Width(GridLengthHelper::FromValueAndType(1, GridUnitType::Star));
    ColumnDefinition managed; managed.Width(GridLengthHelper::FromValueAndType(3, GridUnitType::Star));
    mainLayout.ColumnDefinitions().Append(trusted); mainLayout.ColumnDefinitions().Append(managed);
    mainWindow.Content(mainLayout);
}

static void RemoveFromMainLayout(UIElement const& element) {
    if (!mainLayout || !element) return;
    uint32_t index = 0;
    if (mainLayout.Children().IndexOf(element, index)) mainLayout.Children().RemoveAt(index);
}

static void AttachManagedSurface(WebView2 const& view) {
    EnsureMainLayout();
    RemoveFromMainLayout(activeManagedView);
    activeManagedView = view;
    Grid::SetColumn(view, 1);
    mainLayout.Children().Append(view);
}

static std::wstring ExactOrigin(hstring const& raw) {
    try {
        Windows::Foundation::Uri uri(raw);
        if (uri.SchemeName() != L"https") return L"";
        auto origin = std::wstring(L"https://") + std::wstring(uri.Host());
        if (uri.Port() > 0 && uri.Port() != 443) origin += L":" + std::to_wstring(uri.Port());
        return origin;
    } catch (...) { return L""; }
}

static void ConfigureManagedSurface(WebView2 const& view) {
    view.NavigationStarting([](WebView2 const&, CoreWebView2NavigationStartingEventArgs const& args) {
        if (!ManagedOriginAllowed(ExactOrigin(args.Uri()))) args.Cancel(true);
    });
    view.CoreWebView2Initialized([view](WebView2 const&, CoreWebView2InitializedEventArgs const& args) {
        if (FAILED(args.Exception())) return;
        auto core = view.CoreWebView2();
        core.NewWindowRequested([](CoreWebView2 const&, CoreWebView2NewWindowRequestedEventArgs const& args) { args.Handled(true); });
        core.PermissionRequested([](CoreWebView2 const&, CoreWebView2PermissionRequestedEventArgs const& args) {
            auto origin = ExactOrigin(args.Uri());
            std::wstring permission;
            if (args.PermissionKind() == CoreWebView2PermissionKind::Microphone) permission = L"microphone";
            else if (args.PermissionKind() == CoreWebView2PermissionKind::Camera) permission = L"camera";
            args.State(PermissionOriginAllowed(permission, origin) ? CoreWebView2PermissionState::Allow : CoreWebView2PermissionState::Deny);
            args.SavesInProfile(false);
        });
    });
}

static fire_and_forget InitializeManagedSurface(std::wstring id, WebView2 view, hstring url, bool persistent) {
    try {
        auto environment = co_await CoreWebView2Environment::CreateAsync();
        auto options = environment.CreateCoreWebView2ControllerOptions();
        options.IsInPrivateModeEnabled(!persistent);
        co_await view.EnsureCoreWebView2Async(environment, options);
        auto found = managedSurfaces.find(id); if (found == managedSurfaces.end()) co_return;
        found->second.view.Source(Windows::Foundation::Uri(url)); found->second.pending = false;
        JsonObject payload; payload.SetNamedValue(L"id", JsonValue::CreateStringValue(id)); EmitHostEvent(L"surface.opened", payload);
    } catch (...) {
        JsonObject payload; payload.SetNamedValue(L"id", JsonValue::CreateStringValue(id)); EmitHostEvent(L"surface.failed", payload);
    }
}
` : '';
    const managedCases = managed ? `
        if (capability == L"contentSurface.open") {
            auto id = payload.GetNamedString(L"id", L""); auto url = payload.GetNamedString(L"url", L"");
            if (id.empty() || !ManagedOriginAllowed(ExactOrigin(url))) throw hresult_invalid_argument(L"Managed content id or origin is not allowed");
            if (managedSurfaces.contains(std::wstring(id))) throw hresult_invalid_argument(L"Managed content surface already exists");
            TachyonManagedSurface surface; surface.view = WebView2(); surface.persistent = payload.GetNamedBoolean(L"persistentSession", false);
            ConfigureManagedSurface(surface.view); activeManagedSurfaceID = std::wstring(id); AttachManagedSurface(surface.view);
            managedSurfaces.emplace(std::wstring(id), surface);
            InitializeManagedSurface(std::wstring(id), surface.view, url, surface.persistent);
            JsonObject value; value.SetNamedValue(L"id", JsonValue::CreateStringValue(id)); value.SetNamedValue(L"open", JsonValue::CreateBooleanValue(true)); value.SetNamedValue(L"pending", JsonValue::CreateBooleanValue(true)); value.SetNamedValue(L"presentation", JsonValue::CreateStringValue(L"composed")); return HostSuccess(value, resultJson);
        }
        if (capability.rfind(L"contentSurface.", 0) == 0) {
            auto id = std::wstring(payload.GetNamedString(L"id", L"")); auto found = managedSurfaces.find(id);
            if (found == managedSurfaces.end()) throw hresult_invalid_argument(L"Unknown managed content surface");
            auto& surface = found->second;
            if (capability == L"contentSurface.navigate") { auto url = payload.GetNamedString(L"url", L""); if (!ManagedOriginAllowed(ExactOrigin(url))) throw hresult_invalid_argument(L"Managed content origin is not allowed"); surface.view.Source(Windows::Foundation::Uri(url)); }
            else if (capability == L"contentSurface.goBack" && surface.view.CanGoBack()) surface.view.GoBack();
            else if (capability == L"contentSurface.goForward" && surface.view.CanGoForward()) surface.view.GoForward();
            else if (capability == L"contentSurface.reload") surface.view.Reload();
            else if (capability == L"contentSurface.close") { bool wasActive = std::wstring(id) == activeManagedSurfaceID; if (wasActive) { RemoveFromMainLayout(activeManagedView); activeManagedView = nullptr; activeManagedSurfaceID.clear(); } managedSurfaces.erase(found); if (wasActive && !managedSurfaces.empty()) { activeManagedSurfaceID = managedSurfaces.begin()->first; AttachManagedSurface(managedSurfaces.begin()->second.view); } JsonObject value; value.SetNamedValue(L"id", JsonValue::CreateStringValue(id)); value.SetNamedValue(L"open", JsonValue::CreateBooleanValue(false)); value.SetNamedValue(L"presentation", JsonValue::CreateStringValue(L"composed")); return HostSuccess(value, resultJson); }
            JsonObject value; value.SetNamedValue(L"id", JsonValue::CreateStringValue(id)); value.SetNamedValue(L"open", JsonValue::CreateBooleanValue(true)); value.SetNamedValue(L"persistentSession", JsonValue::CreateBooleanValue(surface.persistent)); value.SetNamedValue(L"pending", JsonValue::CreateBooleanValue(surface.pending));
            value.SetNamedValue(L"presentation", JsonValue::CreateStringValue(L"composed")); value.SetNamedValue(L"url", JsonValue::CreateStringValue(surface.view.Source() ? surface.view.Source().AbsoluteUri() : L"")); value.SetNamedValue(L"canGoBack", JsonValue::CreateBooleanValue(surface.view.CanGoBack())); value.SetNamedValue(L"canGoForward", JsonValue::CreateBooleanValue(surface.view.CanGoForward())); return HostSuccess(value, resultJson);
        }
` : '';
    const captureSupport = capture ? `
static BOOL CALLBACK CollectCapturableWindow(HWND hwnd, LPARAM data) {
    if (!IsWindowVisible(hwnd) || GetWindow(hwnd, GW_OWNER)) return TRUE;
    DWORD pid = 0; GetWindowThreadProcessId(hwnd, &pid); if (pid == GetCurrentProcessId()) return TRUE;
    int length = GetWindowTextLengthW(hwnd); if (length <= 0) return TRUE;
    std::wstring title(static_cast<size_t>(length) + 1, L'\\0'); int copied = GetWindowTextW(hwnd, title.data(), length + 1); title.resize(static_cast<size_t>(copied));
    auto array = reinterpret_cast<JsonArray*>(data); JsonObject item;
    item.SetNamedValue(L"windowId", JsonValue::CreateStringValue(to_hstring(std::to_string(reinterpret_cast<uintptr_t>(hwnd)))));
    item.SetNamedValue(L"title", JsonValue::CreateStringValue(title)); item.SetNamedValue(L"application", JsonValue::CreateStringValue(L"Windows application"));
    RECT rect{}; GetWindowRect(hwnd, &rect); JsonObject frame; frame.SetNamedValue(L"x", JsonValue::CreateNumberValue(rect.left)); frame.SetNamedValue(L"y", JsonValue::CreateNumberValue(rect.top)); frame.SetNamedValue(L"width", JsonValue::CreateNumberValue(rect.right - rect.left)); frame.SetNamedValue(L"height", JsonValue::CreateNumberValue(rect.bottom - rect.top)); item.SetNamedValue(L"frame", frame); array->Append(item);
    return TRUE;
}

static CLSID PngEncoder() {
    UINT count = 0, bytes = 0; Gdiplus::GetImageEncodersSize(&count, &bytes); std::vector<BYTE> buffer(bytes);
    auto encoders = reinterpret_cast<Gdiplus::ImageCodecInfo*>(buffer.data()); Gdiplus::GetImageEncoders(count, bytes, encoders);
    for (UINT index = 0; index < count; ++index) if (wcscmp(encoders[index].MimeType, L"image/png") == 0) return encoders[index].Clsid;
    return CLSID{};
}

static JsonObject CaptureWindow(JsonObject const& payload) {
    auto text = to_string(payload.GetNamedString(L"windowId", L"0")); HWND hwnd = reinterpret_cast<HWND>(static_cast<uintptr_t>(std::stoull(text)));
    RECT rect{}; if (!GetWindowRect(hwnd, &rect)) throw hresult_error(HRESULT_FROM_WIN32(GetLastError()), L"Unable to read capture window bounds");
    int width = rect.right - rect.left, height = rect.bottom - rect.top; if (width <= 0 || height <= 0) throw hresult_error(E_FAIL, L"Invalid window dimensions for capture"); HDC screen = GetDC(nullptr); HDC memory = CreateCompatibleDC(screen); HBITMAP bitmap = CreateCompatibleBitmap(screen, width, height);
    auto previous = SelectObject(memory, bitmap); BOOL printed = PrintWindow(hwnd, memory, PW_RENDERFULLCONTENT); SelectObject(memory, previous); DeleteDC(memory); ReleaseDC(nullptr, screen);
    if (!printed) { DeleteObject(bitmap); throw hresult_error(E_FAIL, L"Unable to capture the selected window"); }
    auto destination = payload.GetNamedString(L"destination", L"clipboard"); std::wstring filePath;
    if (destination == L"file" || destination == L"both") {
        wchar_t temp[MAX_PATH]; GetTempPathW(MAX_PATH, temp); filePath = std::wstring(temp) + L"tachyon-capture-" + std::to_wstring(GetTickCount64()) + L".png";
        ULONG_PTR token = 0; Gdiplus::GdiplusStartupInput input; Gdiplus::GdiplusStartup(&token, &input, nullptr); Gdiplus::Bitmap image(bitmap, nullptr); auto encoder = PngEncoder(); if (image.Save(filePath.c_str(), &encoder, nullptr) != Gdiplus::Ok) { Gdiplus::GdiplusShutdown(token); DeleteObject(bitmap); throw hresult_error(E_FAIL, L"Unable to encode PNG capture"); } Gdiplus::GdiplusShutdown(token);
    }
    bool clipboard = destination == L"clipboard" || destination == L"both";
    if (clipboard) { if (!OpenClipboard(nullptr)) { DeleteObject(bitmap); throw hresult_error(E_FAIL, L"Unable to open clipboard"); } EmptyClipboard(); SetClipboardData(CF_BITMAP, bitmap); CloseClipboard(); } else DeleteObject(bitmap);
    JsonObject value; value.SetNamedValue(L"windowId", JsonValue::CreateStringValue(payload.GetNamedString(L"windowId", L""))); value.SetNamedValue(L"destination", JsonValue::CreateStringValue(destination)); value.SetNamedValue(L"format", JsonValue::CreateStringValue(L"png")); value.SetNamedValue(L"bytes", JsonValue::CreateNumberValue(static_cast<double>(width) * height * 4)); value.SetNamedValue(L"clipboard", JsonValue::CreateBooleanValue(clipboard)); value.SetNamedValue(L"path", JsonValue::CreateStringValue(filePath)); return value;
}
` : '';
    const captureCases = capture ? `
        if (capability == L"screenCapture.state") { JsonObject value; value.SetNamedValue(L"supported", JsonValue::CreateBooleanValue(true)); value.SetNamedValue(L"permission", JsonValue::CreateStringValue(L"granted")); value.SetNamedValue(L"format", JsonValue::CreateStringValue(L"png")); JsonArray destinations; destinations.Append(JsonValue::CreateStringValue(L"clipboard")); destinations.Append(JsonValue::CreateStringValue(L"file")); destinations.Append(JsonValue::CreateStringValue(L"both")); value.SetNamedValue(L"destinations", destinations); return HostSuccess(value, resultJson); }
        if (capability == L"screenCapture.listWindows") { JsonArray windows; EnumWindows(CollectCapturableWindow, reinterpret_cast<LPARAM>(&windows)); JsonObject value; value.SetNamedValue(L"windows", windows); value.SetNamedValue(L"permission", JsonValue::CreateStringValue(L"granted")); return HostSuccess(value, resultJson); }
        if (capability == L"screenCapture.captureWindow") return HostSuccess(CaptureWindow(payload), resultJson);
` : '';
    return `static std::wstring Widen(std::string const& value) {
    if (value.empty()) return L""; int size = MultiByteToWideChar(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), nullptr, 0); std::wstring result(size, L'\\0'); MultiByteToWideChar(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), result.data(), size); return result;
}
static std::string Narrow(std::wstring const& value) {
    if (value.empty()) return ""; int size = WideCharToMultiByte(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), nullptr, 0, nullptr, nullptr); std::string result(size, '\\0'); WideCharToMultiByte(CP_UTF8, 0, value.data(), static_cast<int>(value.size()), result.data(), size, nullptr, nullptr); return result;
}
static int CopyHostText(std::string const& value, char** output) { *output = static_cast<char*>(malloc(value.size() + 1)); if (!*output) return 0; memcpy(*output, value.c_str(), value.size() + 1); return 1; }
static int HostSuccess(IJsonValue const& value, char** resultJson) { JsonObject envelope; envelope.SetNamedValue(L"ok", JsonValue::CreateBooleanValue(true)); envelope.SetNamedValue(L"value", value); return CopyHostText(to_string(envelope.Stringify()), resultJson); }
static int HostFailure(std::string const& message, char** error) { return CopyHostText(message, error) ? 0 : 0; }
static bool CapabilityEnabled(std::wstring const& capability) { return ${checks}; }
static HWND MainWindowHandle() { HWND hwnd = nullptr; if (mainWindow) check_hresult(mainWindow.as<::IWindowNative>()->get_WindowHandle(&hwnd)); return hwnd; }

static std::string ReadClipboardText() {
    if (!OpenClipboard(nullptr)) throw hresult_error(E_FAIL, L"Unable to open clipboard"); HANDLE handle = GetClipboardData(CF_UNICODETEXT); if (!handle) { CloseClipboard(); return ""; }
    auto text = static_cast<const wchar_t*>(GlobalLock(handle)); std::string result = text ? Narrow(text) : ""; if (text) GlobalUnlock(handle); CloseClipboard(); return result;
}
static void WriteClipboardText(std::string const& value) {
    auto text = Widen(value); if (!OpenClipboard(nullptr)) throw hresult_error(E_FAIL, L"Unable to open clipboard"); EmptyClipboard(); SIZE_T bytes = (text.size() + 1) * sizeof(wchar_t); HGLOBAL memory = GlobalAlloc(GMEM_MOVEABLE, bytes); if (!memory) { CloseClipboard(); throw hresult_error(E_OUTOFMEMORY); } void* target = GlobalLock(memory); memcpy(target, text.c_str(), bytes); GlobalUnlock(memory); if (!SetClipboardData(CF_UNICODETEXT, memory)) { GlobalFree(memory); CloseClipboard(); throw hresult_error(E_FAIL, L"Unable to update clipboard"); } CloseClipboard();
}

static std::map<int, std::pair<std::wstring, std::wstring>> registeredShortcuts;
static int nextShortcutId = 0x5400; static WNDPROC previousWindowProc = nullptr;
static bool windowAlwaysOnTop = false; static bool windowCaptureProtected = false;
static double windowOpacity = 1.0;
static JsonArray ShortcutSnapshot() { JsonArray list; for (auto const& entry : registeredShortcuts) { JsonObject item; item.SetNamedValue(L"id", JsonValue::CreateStringValue(entry.second.first)); item.SetNamedValue(L"accelerator", JsonValue::CreateStringValue(entry.second.second)); list.Append(item); } return list; }
static JsonObject WindowSnapshot(HWND hwnd) { LONG_PTR style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE); JsonObject value; value.SetNamedValue(L"alwaysOnTop", JsonValue::CreateBooleanValue(windowAlwaysOnTop)); value.SetNamedValue(L"opacity", JsonValue::CreateNumberValue(windowOpacity)); value.SetNamedValue(L"clickThrough", JsonValue::CreateBooleanValue((style & WS_EX_TRANSPARENT) != 0)); value.SetNamedValue(L"captureProtection", JsonValue::CreateBooleanValue(windowCaptureProtected)); return value; }
static std::pair<UINT, UINT> ParseAccelerator(std::wstring accelerator) {
    UINT modifiers = 0, key = 0; size_t start = 0;
    while (start <= accelerator.size()) { auto end = accelerator.find(L'+', start); auto part = accelerator.substr(start, end == std::wstring::npos ? std::wstring::npos : end - start); for (auto& character : part) character = towupper(character);
        if (part == L"PRIMARY" || part == L"CONTROL") modifiers |= MOD_CONTROL; else if (part == L"SHIFT") modifiers |= MOD_SHIFT; else if (part == L"ALT" || part == L"OPTION") modifiers |= MOD_ALT; else if (part == L"COMMAND" || part == L"META") modifiers |= MOD_WIN; else if (part.size() == 1) key = VkKeyScanW(part[0]) & 0xff; else if (part.size() > 1 && part[0] == L'F') key = VK_F1 + std::stoi(part.substr(1)) - 1;
        if (end == std::wstring::npos) break; start = end + 1; }
    if (!modifiers || !key) throw hresult_invalid_argument(L"Invalid shortcut accelerator"); return { modifiers | MOD_NOREPEAT, key };
}
static void EmitHostEvent(std::wstring const& name, IJsonValue const& payload) {
    if (!controller) return; JsonObject event; event.SetNamedValue(L"type", JsonValue::CreateStringValue(L"tac:host-event")); event.SetNamedValue(L"event", JsonValue::CreateStringValue(name)); event.SetNamedValue(L"payload", payload); auto text = to_string(event.Stringify()); char* error = nullptr; char* snapshot = tachyon_ui_controller_emit(controller, text.c_str(), &error); if (snapshot) { ApplySnapshot(snapshot); tachyon_ui_controller_free_string(snapshot); } tachyon_ui_controller_free_string(error);
}
static LRESULT CALLBACK TachyonWindowProc(HWND hwnd, UINT message, WPARAM wParam, LPARAM lParam) {
    if (message == WM_HOTKEY) { auto found = registeredShortcuts.find(static_cast<int>(wParam)); if (found != registeredShortcuts.end()) { JsonObject payload; payload.SetNamedValue(L"id", JsonValue::CreateStringValue(found->second.first)); EmitHostEvent(L"shortcut.activated", payload); } return 0; }
    return previousWindowProc ? CallWindowProcW(previousWindowProc, hwnd, message, wParam, lParam) : DefWindowProcW(hwnd, message, wParam, lParam);
}

${managedTypes}
${captureSupport}

static int HandleNativeCapability(const char* rawCapability, const char* rawPayload, char** resultJson, char** error, void*) {
    try {
        std::wstring capability = Widen(rawCapability ? rawCapability : "");
        if (capability == L"__tachyon.hostInfo") return HostSuccess(JsonObject::Parse(L${JSON.stringify(JSON.stringify({ target: 'windows', platform: 'desktop', capabilities: host.hostCapabilities }))}), resultJson);
        if (!CapabilityEnabled(capability)) return HostFailure("Unsupported native capability", error);
        JsonObject payload = JsonObject::Parse(Widen(rawPayload ? rawPayload : "{}"));
        if (capability == L"capabilities.state") return HostSuccess(JsonValue::CreateStringValue(CapabilityEnabled(std::wstring(payload.GetNamedString(L"capability", L""))) ? L"granted" : L"unsupported"), resultJson);
        if (capability == L"app.info") return HostSuccess(JsonObject::Parse(L${JSON.stringify(JSON.stringify({ name: host.appName, runtime: 'windows-winui', version: host.version }))}), resultJson);
        if (capability == L"clipboard.readText") return HostSuccess(JsonValue::CreateStringValue(Widen(ReadClipboardText())), resultJson);
        if (capability == L"clipboard.writeText") { WriteClipboardText(to_string(payload.GetNamedString(L"text", L""))); JsonObject value; value.SetNamedValue(L"written", JsonValue::CreateBooleanValue(true)); return HostSuccess(value, resultJson); }
        if (capability == L"openUrl") { auto url = std::wstring(payload.GetNamedString(L"url", L"")); if (url.rfind(L"https://", 0) != 0 && url.rfind(L"http://", 0) != 0) throw hresult_invalid_argument(L"openUrl requires an http(s) URL"); if (reinterpret_cast<INT_PTR>(ShellExecuteW(nullptr, L"open", url.c_str(), nullptr, nullptr, SW_SHOWNORMAL)) <= 32) throw hresult_error(E_FAIL, L"Unable to open URL"); JsonObject value; value.SetNamedValue(L"opened", JsonValue::CreateBooleanValue(true)); return HostSuccess(value, resultJson); }
        HWND hwnd = MainWindowHandle();
        if (capability == L"window.state") return HostSuccess(WindowSnapshot(hwnd), resultJson);
        if (capability == L"window.alwaysOnTop") { bool enabled = payload.GetNamedBoolean(L"enabled", false); if (!SetWindowPos(hwnd, enabled ? HWND_TOPMOST : HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)) throw hresult_error(HRESULT_FROM_WIN32(GetLastError())); windowAlwaysOnTop = enabled; return HostSuccess(WindowSnapshot(hwnd), resultJson); }
        if (capability == L"window.opacity") { double opacity = payload.GetNamedNumber(L"value", 1.0); if (opacity < 0.1 || opacity > 1.0) throw hresult_invalid_argument(L"Window opacity must be between 0.1 and 1"); SetWindowLongPtrW(hwnd, GWL_EXSTYLE, GetWindowLongPtrW(hwnd, GWL_EXSTYLE) | WS_EX_LAYERED); if (!SetLayeredWindowAttributes(hwnd, 0, static_cast<BYTE>(opacity * 255), LWA_ALPHA)) throw hresult_error(HRESULT_FROM_WIN32(GetLastError())); windowOpacity = opacity; return HostSuccess(WindowSnapshot(hwnd), resultJson); }
        if (capability == L"window.clickThrough") { bool enabled = payload.GetNamedBoolean(L"enabled", false); LONG_PTR style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE); SetWindowLongPtrW(hwnd, GWL_EXSTYLE, enabled ? style | WS_EX_TRANSPARENT : style & ~WS_EX_TRANSPARENT); return HostSuccess(WindowSnapshot(hwnd), resultJson); }
        if (capability == L"window.captureProtection") { bool enabled = payload.GetNamedBoolean(L"enabled", false); if (!SetWindowDisplayAffinity(hwnd, enabled ? WDA_EXCLUDEFROMCAPTURE : WDA_NONE)) throw hresult_error(HRESULT_FROM_WIN32(GetLastError())); windowCaptureProtected = enabled; return HostSuccess(WindowSnapshot(hwnd), resultJson); }
        if (capability == L"shortcuts.register") { auto id = payload.GetNamedString(L"id", L""); auto accelerator = payload.GetNamedString(L"accelerator", L""); if (id.empty()) throw hresult_invalid_argument(L"Shortcut id is required"); bool replace = payload.GetNamedBoolean(L"replace", false); for (auto it = registeredShortcuts.begin(); it != registeredShortcuts.end(); ) { if (it->second.first == id) { if (!replace) throw hresult_invalid_argument(L"Shortcut id is already registered"); UnregisterHotKey(hwnd, it->first); it = registeredShortcuts.erase(it); } else ++it; } auto parsed = ParseAccelerator(std::wstring(accelerator)); int nativeId = nextShortcutId++; if (!RegisterHotKey(hwnd, nativeId, parsed.first, parsed.second)) throw hresult_error(HRESULT_FROM_WIN32(GetLastError()), L"Shortcut registration failed"); registeredShortcuts[nativeId] = { std::wstring(id), std::wstring(accelerator) }; JsonObject shortcut; shortcut.SetNamedValue(L"id", JsonValue::CreateStringValue(id)); shortcut.SetNamedValue(L"accelerator", JsonValue::CreateStringValue(accelerator)); JsonObject value; value.SetNamedValue(L"shortcuts", ShortcutSnapshot()); value.SetNamedValue(L"shortcut", shortcut); return HostSuccess(value, resultJson); }
        if (capability == L"shortcuts.unregister") { auto id = payload.GetNamedString(L"id", L""); bool removed = false; for (auto it = registeredShortcuts.begin(); it != registeredShortcuts.end(); ) { if (it->second.first == id) { UnregisterHotKey(hwnd, it->first); it = registeredShortcuts.erase(it); removed = true; } else ++it; } JsonObject value; value.SetNamedValue(L"shortcuts", ShortcutSnapshot()); value.SetNamedValue(L"unregistered", JsonValue::CreateBooleanValue(removed)); return HostSuccess(value, resultJson); }
        if (capability == L"shortcuts.unregisterAll") { double count = static_cast<double>(registeredShortcuts.size()); for (auto const& entry : registeredShortcuts) UnregisterHotKey(hwnd, entry.first); registeredShortcuts.clear(); JsonObject value; value.SetNamedValue(L"shortcuts", ShortcutSnapshot()); value.SetNamedValue(L"unregistered", JsonValue::CreateNumberValue(count)); return HostSuccess(value, resultJson); }
        if (capability == L"shortcuts.list") { JsonObject value; value.SetNamedValue(L"shortcuts", ShortcutSnapshot()); return HostSuccess(value, resultJson); }
        ${managedCases}
        ${captureCases}
        return HostFailure("Unsupported native capability", error);
    } catch (hresult_error const& failure) { return HostFailure(to_string(failure.message()), error); } catch (std::exception const& failure) { return HostFailure(failure.what(), error); }
}
`;
}

/** @param {any} host */
function windowsNativeSource(host) {
    const hybrid = host.hasWebViewFallbacks;
    const managed = host.managedContentOrigins.length > 0;
    const needsWebView = hybrid || host.managedContentOrigins.length > 0;
    return `#include <windows.h>
#include <shellapi.h>
#include <gdiplus.h>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <cwctype>
#include <microsoft.ui.xaml.window.h>
#include <winrt/Microsoft.UI.Xaml.h>
#include <winrt/Microsoft.UI.Xaml.Controls.h>
#include <winrt/Microsoft.UI.Xaml.Controls.Primitives.h>
${needsWebView ? '#include <winrt/Microsoft.Web.WebView2.Core.h>' : ''}
#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <vector>
#include "tachyon_ui_controller.h"

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;
${needsWebView ? 'using namespace Microsoft::Web::WebView2::Core;' : ''}
using namespace Windows::Data::Json;

static TachyonUIController* controller = nullptr;
static Window mainWindow{ nullptr };
${managed ? 'static Grid mainLayout{ nullptr };\nstatic FrameworkElement trustedRoot{ nullptr };' : ''}

static void ApplySnapshot(const char* snapshot);

${windowsHostSupport(host)}

${hybrid ? `static hstring HybridDocument(hstring const& fragment) {
    return hstring(L"<!doctype html><html><head><base href=\\"https://appassets.tachyon/\\">"
        L"<meta name=\\"viewport\\" content=\\"width=device-width,initial-scale=1\\">"
        L"<link rel=\\"stylesheet\\" href=\\"imports.css\\"><script type=\\"module\\" src=\\"imports.js\\"></script>"
        L"<style>html,body{margin:0;background:transparent;overflow:hidden}</style></head><body>")
        + fragment + L"</body></html>";
}

static Windows::Foundation::IAsyncAction LoadHybrid(WebView2 view, hstring html) {
    co_await view.EnsureCoreWebView2Async();
    view.CoreWebView2().SetVirtualHostNameToFolderMapping(
        L"appassets.tachyon", std::filesystem::absolute(L"Resources").wstring(),
        CoreWebView2HostResourceAccessKind::Allow);
    view.NavigateToString(HybridDocument(html));
}` : ''}

FrameworkElement TachyonNativeNode(JsonObject const& node) {
    auto kind = node.GetNamedString(L"kind", L"");
    if (kind == L"text") { TextBlock text; text.Text(node.GetNamedString(L"value", L"")); return text; }
    ${hybrid ? `if (kind == L"webview") {
        WebView2 view;
        view.MinHeight(160);
        LoadHybrid(view, node.GetNamedString(L"html", L""));
        return view;
    }` : ''}
    auto tag = node.GetNamedString(L"tag", L"");
    if (tag == L"button") {
        Button button; button.Content(box_value(L"Button"));
        auto id = node.GetNamedString(L"id", L"");
        if (!id.empty()) button.Click([id](auto&&, auto&&) {
            JsonObject event; event.SetNamedValue(L"elementId", JsonValue::CreateStringValue(id));
            event.SetNamedValue(L"type", JsonValue::CreateStringValue(L"click"));
            auto utf8 = to_string(event.Stringify()); char* error = nullptr;
            char* snapshot = tachyon_ui_controller_dispatch(controller, utf8.c_str(), &error);
            if (snapshot) { ApplySnapshot(snapshot); tachyon_ui_controller_free_string(snapshot); }
            tachyon_ui_controller_free_string(error);
        });
        return button;
    }
    if (tag == L"input") return TextBox();
    StackPanel panel;
    auto children = node.GetNamedArray(L"children", JsonArray());
    for (uint32_t index = 0; index < children.Size(); ++index)
        panel.Children().Append(TachyonNativeNode(children.GetObjectAt(index)));
    return panel;
}

static void ApplySnapshot(const char* snapshot) {
    auto document = JsonObject::Parse(to_hstring(snapshot));
    ${managed ? `EnsureMainLayout();
    RemoveFromMainLayout(trustedRoot);
    trustedRoot = TachyonNativeNode(document.GetNamedObject(L"root"));
    Grid::SetColumn(trustedRoot, 0);
    mainLayout.Children().Append(trustedRoot);` : 'mainWindow.Content(TachyonNativeNode(document.GetNamedObject(L"root")));'}
}

int __stdcall wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
    init_apartment(apartment_type::single_threaded);
    Application::Start([](auto&&) {
        mainWindow = Window();
        mainWindow.Title(L"${host.appName.replaceAll('"', '\\"')}");
        char* error = nullptr;
        controller = tachyon_ui_controller_create("Resources/tachyon.native-controller.js", HandleNativeCapability, nullptr, &error);
        if (!controller) throw hresult_error(E_FAIL, to_hstring(error ? error : "Controller initialization failed"));
        char* snapshot = tachyon_ui_controller_render(controller, &error);
        if (!snapshot) throw hresult_error(E_FAIL, to_hstring(error ? error : "Controller render failed"));
        ApplySnapshot(snapshot); tachyon_ui_controller_free_string(snapshot); tachyon_ui_controller_free_string(error);
        mainWindow.Activate();
        auto hwnd = MainWindowHandle();
        previousWindowProc = reinterpret_cast<WNDPROC>(SetWindowLongPtrW(hwnd, GWLP_WNDPROC, reinterpret_cast<LONG_PTR>(TachyonWindowProc)));
    });
    return 0;
}
`;
}

/** Generates the greenfield native-tree project for one platform. */
export default class NativeUIPlatformProject {
    /** @param {any} host */
    static async generate(host) {
        switch (host.target) {
            case 'macos': return this.generateMacOS(host);
            case 'ios': return this.generateIOS(host);
            case 'android': return this.generateAndroid(host);
            case 'linux': return this.generateLinux(host);
            case 'windows': return this.generateWindows(host);
            default: throw new Error(`Native UI project generation is unavailable for '${host.target}'.`);
        }
    }

    /** @param {any} host */
    static async generateMacOS(host) {
        const sources = path.join(host.outputRoot, 'Sources');
        const app = path.join(host.outputRoot, 'TachyonApp');
        await mkdir(sources, { recursive: true }); await mkdir(app, { recursive: true });
        await writeFile(path.join(sources, 'TachyonApp.swift'), swiftNativeView(host));
        await writeFile(path.join(app, 'Info.plist'), host.infoPlist());
        await writeFile(path.join(app, 'TachyonApp.entitlements'), host.entitlements());
        await writeFile(path.join(app, 'PkgInfo'), 'APPL????');
        const buildScript = host.hasWebViewFallbacks || host.managedContentOrigins.length > 0
            ? host.buildScript().replace('    -framework WebKit \\\n', '    -framework WebKit \\\n    -framework JavaScriptCore \\\n')
            : host.buildScript().replace('    -framework WebKit \\\n', '    -framework JavaScriptCore \\\n');
        await host.writeExecutable('build.sh', buildScript);
    }

    /** @param {any} host */
    static async generateIOS(host) {
        const sources = path.join(host.outputRoot, 'Sources'); await mkdir(sources, { recursive: true });
        const source = swiftNativeView(host);
        await writeFile(path.join(sources, 'TachyonNativeView.swift'), source);
        await writeFile(path.join(sources, 'TachyonApp.swift'), '// Native app entry is declared in TachyonNativeView.swift.\n');
        await writeFile(path.join(host.outputRoot, 'project.yml'), host.xcodegenSpec());
    }

    /** @param {any} host */
    static async generateAndroid(host) {
        const javaDir = path.join(host.outputRoot, 'app', 'src', 'main', 'java', ...host.appId.split('.'));
        const values = path.join(host.outputRoot, 'app', 'src', 'main', 'res', 'values');
        await mkdir(javaDir, { recursive: true }); await mkdir(values, { recursive: true });
        await writeFile(path.join(javaDir, 'MainActivity.kt'), androidNativeActivity(host));
        await writeFile(path.join(values, 'strings.xml'), host.stringsXml());
        await writeFile(path.join(host.outputRoot, 'app', 'src', 'main', 'AndroidManifest.xml'), host.manifestXml());
        await writeFile(path.join(host.outputRoot, 'app', 'build.gradle.kts'), host.appBuildGradle()
            .replace('    implementation("androidx.webkit:webkit:1.11.0")', `    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.activity:activity-compose:1.10.0")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.webkit:webkit:1.11.0")
    implementation("app.cash.quickjs:quickjs-android:0.9.2")`)
            .replace('    buildTypes {', `    buildFeatures { compose = true }
    composeOptions { kotlinCompilerExtensionVersion = "1.5.4" }
    buildTypes {`));
        await writeFile(path.join(host.outputRoot, 'build.gradle.kts'), host.rootBuildGradle());
        await writeFile(path.join(host.outputRoot, 'settings.gradle.kts'), host.settingsGradle());
        await writeFile(path.join(host.outputRoot, 'gradle.properties'), host.gradleProperties());
    }

    /** @param {any} host */
    static async generateLinux(host) {
        const src = path.join(host.outputRoot, 'src'); await mkdir(src, { recursive: true });
        await writeFile(path.join(src, 'main.c'), linuxNativeSource(host));
        await writeFile(path.join(src, 'tachyon_ui_controller.h'), quickJSDriverHeader());
        await writeFile(path.join(src, 'tachyon_ui_controller.c'), quickJSDriverSource());
        await writeFile(path.join(host.outputRoot, 'CMakeLists.txt'), `cmake_minimum_required(VERSION 3.16)
project(${host.appName} C)
include(FetchContent)
set(QJS_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(QJS_BUILD_TESTS OFF CACHE BOOL "" FORCE)
FetchContent_Declare(quickjs
    URL https://github.com/quickjs-ng/quickjs/archive/refs/tags/v${QUICKJS_NG_VERSION}.tar.gz
    URL_HASH SHA256=c4e813951b7c46845096a948e978c620b11ab4cf5fd622ca09c727ec31f42623)
FetchContent_MakeAvailable(quickjs)
find_package(PkgConfig REQUIRED)
pkg_check_modules(GTK REQUIRED gtk+-3.0 json-glib-1.0 x11${host.hasWebViewFallbacks || host.managedContentOrigins.length > 0 ? ' webkit2gtk-4.1' : ''})
add_executable(\${PROJECT_NAME} src/main.c src/tachyon_ui_controller.c)
target_include_directories(\${PROJECT_NAME} PRIVATE \${GTK_INCLUDE_DIRS})
target_link_libraries(\${PROJECT_NAME} PRIVATE \${GTK_LIBRARIES} qjs)
`);
        await host.writeExecutable('build.sh', '#!/bin/sh\nset -eu\ncmake -S . -B build\ncmake --build build\n');
    }

    /** @param {any} host */
    static async generateWindows(host) {
        const src = path.join(host.outputRoot, 'src'); await mkdir(src, { recursive: true });
        await writeFile(path.join(src, 'main.cpp'), windowsNativeSource(host));
        await writeFile(path.join(src, 'tachyon_ui_controller.h'), quickJSDriverHeader());
        await writeFile(path.join(src, 'tachyon_ui_controller.c'), quickJSDriverSource());
        await writeFile(path.join(src, 'app.rc'), 'IDI_APP_ICON ICON "../Resources/TachyonIcon.ico"\n');
        await writeFile(path.join(host.outputRoot, 'CMakeLists.txt'), `cmake_minimum_required(VERSION 3.20)
project(${host.appName} LANGUAGES C CXX)
include(FetchContent)
set(QJS_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(QJS_BUILD_TESTS OFF CACHE BOOL "" FORCE)
FetchContent_Declare(quickjs
    URL https://github.com/quickjs-ng/quickjs/archive/refs/tags/v${QUICKJS_NG_VERSION}.tar.gz
    URL_HASH SHA256=c4e813951b7c46845096a948e978c620b11ab4cf5fd622ca09c727ec31f42623)
FetchContent_MakeAvailable(quickjs)
add_executable(\${PROJECT_NAME} WIN32 src/main.cpp src/tachyon_ui_controller.c src/app.rc)
set_property(TARGET \${PROJECT_NAME} PROPERTY CXX_STANDARD 20)
set_property(TARGET \${PROJECT_NAME} PROPERTY VS_PACKAGE_REFERENCES "Microsoft.WindowsAppSDK_${WINDOWS_APP_SDK_VERSION};Microsoft.Windows.CppWinRT_${WINDOWS_CPP_WINRT_VERSION}")
set_property(TARGET \${PROJECT_NAME} PROPERTY VS_GLOBAL_WindowsPackageType "None")
target_link_libraries(\${PROJECT_NAME} PRIVATE
    "$(_FoundationLibFolder)/Microsoft.WindowsAppRuntime.lib"
    "$(_FoundationLibFolder)/Microsoft.WindowsAppRuntime.Bootstrap.lib"
    qjs gdiplus windowscodecs shell32 user32 gdi32)
`);
        await writeFile(path.join(host.outputRoot, 'build.bat'), '@cmake -S . -B build\r\n@cmake --build build --config Release\r\n');
    }
}
