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
function swiftNativeView(host) {
    const appType = swiftIdentifier(host.appName || 'TachyonApp');
    const hybrid = host.hasWebViewFallbacks;
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
    return `import Foundation
import JavaScriptCore
import SwiftUI
	${hybrid ? 'import WebKit' : ''}
#if os(macOS)
import AppKit
#endif

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
function linuxNativeSource(host) {
    const hybrid = host.hasWebViewFallbacks;
    return `#include <gtk/gtk.h>
#include <json-glib/json-glib.h>
${hybrid ? '#include <webkit2/webkit2.h>' : ''}
#include "tachyon_ui_controller.h"

#define APP_NAME ${JSON.stringify(host.appName)}
typedef JsonObject TachyonNativeNode;
static TachyonUIController* controller = NULL;
static GtkWidget* window = NULL;

static void apply_snapshot(const char* snapshot);

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
    GtkWidget* child = gtk_bin_get_child(GTK_BIN(window)); if (child) gtk_container_remove(GTK_CONTAINER(window), child);
    gtk_container_add(GTK_CONTAINER(window), render_node(root)); gtk_widget_show_all(window); g_object_unref(parser);
}

static void activate(GtkApplication* app, gpointer user_data) {
    (void)user_data;
    char* error = NULL;
    controller = tachyon_ui_controller_create("Resources/tachyon.native-controller.js", &error);
    if (!controller) g_error("Tachyon controller: %s", error ? error : "initialization failed");
    window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), APP_NAME);
    gtk_window_set_default_size(GTK_WINDOW(window), 1000, 700);
    char* snapshot = tachyon_ui_controller_render(controller, &error);
    if (!snapshot) g_error("Tachyon controller: %s", error ? error : "render failed");
    apply_snapshot(snapshot); tachyon_ui_controller_free_string(snapshot); tachyon_ui_controller_free_string(error);
}

int main(int argc, char** argv) {
    GtkApplication* app = gtk_application_new("${host.appId}", G_APPLICATION_FLAGS_NONE);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    tachyon_ui_controller_destroy(controller);
    g_object_unref(app);
    return status;
}
`;
}

/** @param {any} host */
function windowsNativeSource(host) {
    const hybrid = host.hasWebViewFallbacks;
    return `#include <windows.h>
#include <winrt/Microsoft.UI.Xaml.h>
#include <winrt/Microsoft.UI.Xaml.Controls.h>
#include <winrt/Microsoft.UI.Xaml.Controls.Primitives.h>
${hybrid ? '#include <winrt/Microsoft.Web.WebView2.Core.h>' : ''}
#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <filesystem>
#include <fstream>
#include <sstream>
#include "tachyon_ui_controller.h"

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;
${hybrid ? 'using namespace Microsoft::Web::WebView2::Core;' : ''}
using namespace Windows::Data::Json;

static TachyonUIController* controller = nullptr;
static Window mainWindow{ nullptr };

static void ApplySnapshot(const char* snapshot);

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
    mainWindow.Content(TachyonNativeNode(document.GetNamedObject(L"root")));
}

int __stdcall wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
    init_apartment(apartment_type::single_threaded);
    Application::Start([](auto&&) {
        mainWindow = Window();
        mainWindow.Title(L"${host.appName.replaceAll('"', '\\"')}");
        char* error = nullptr;
        controller = tachyon_ui_controller_create("Resources/tachyon.native-controller.js", &error);
        if (!controller) throw hresult_error(E_FAIL, to_hstring(error ? error : "Controller initialization failed"));
        char* snapshot = tachyon_ui_controller_render(controller, &error);
        if (!snapshot) throw hresult_error(E_FAIL, to_hstring(error ? error : "Controller render failed"));
        ApplySnapshot(snapshot); tachyon_ui_controller_free_string(snapshot); tachyon_ui_controller_free_string(error);
        mainWindow.Activate();
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
        const buildScript = host.hasWebViewFallbacks
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
pkg_check_modules(GTK REQUIRED gtk+-3.0 json-glib-1.0${host.hasWebViewFallbacks ? ' webkit2gtk-4.1' : ''})
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
target_link_libraries(\${PROJECT_NAME} PRIVATE qjs)
`);
        await writeFile(path.join(host.outputRoot, 'build.bat'), '@cmake -S . -B build\r\n@cmake --build build --config Release\r\n');
    }
}
