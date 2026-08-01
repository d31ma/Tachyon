//! `macOS` `SwiftUI` host generation.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, first_line, native_io, native_tool_failure, quoted_string_escape, run_tool,
    stage_application, write, write_host_source, xml_escape,
};
use super::planner::{NativeRouteIndex, PlannedNativeRoute};
use crate::Failure;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MacOsHostGenerator;

impl MacOsHostGenerator {
    pub(super) async fn generate(
        application: &NativeApplication,
        routes: &[PlannedNativeRoute],
        index: &NativeRouteIndex,
        web_bundle: &Path,
        stage: &Path,
        package: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle = stage.join(format!("{}.app", application.executable_name));
        let contents = bundle.join("Contents");
        let resources = contents.join("Resources");
        stage_application(application, routes, index, web_bundle, stage, &resources)?;

        let swift_path = stage.join("project").join("TachyonHost.swift");
        write_host_source(&swift_path, &swift_source(application))?;
        write(
            &contents.join("Info.plist"),
            info_plist(application).as_bytes(),
        )?;
        if !package {
            return Ok(GeneratedHost {
                application_bundle: PathBuf::from("project/TachyonHost.swift"),
                toolchain_name: String::from("source"),
                toolchain_version: String::from("not-packaged"),
            });
        }

        let executable_directory = contents.join("MacOS");
        native_io(
            std::fs::create_dir_all(&executable_directory),
            &executable_directory,
        )?;
        let executable = executable_directory.join(&application.executable_name);
        let swift_version = compile_swift(&swift_path, &executable).await?;
        sign_bundle(&bundle).await?;
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(format!("{}.app", application.executable_name)),
            toolchain_name: String::from("swift"),
            toolchain_version: swift_version,
        })
    }
}

async fn compile_swift(source: &Path, executable: &Path) -> Result<String, Failure> {
    if !cfg!(target_os = "macos") {
        return Err(native_tool_failure(
            1605,
            "The macOS host requires an Apple macOS build machine.",
        ));
    }
    let version = first_line(
        &run_tool("/usr/bin/xcrun", &["swiftc", "--version"]).await?,
        "Swift unknown",
    );
    let source = source
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Swift source path is not valid Unicode."))?;
    let executable = executable
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Application path is not valid Unicode."))?;
    run_tool(
        "/usr/bin/xcrun",
        &[
            "swiftc",
            "-parse-as-library",
            "-O",
            "-framework",
            "AppKit",
            "-framework",
            "SwiftUI",
            "-framework",
            "WebKit",
            "-o",
            executable,
            source,
        ],
    )
    .await?;
    Ok(version)
}

async fn sign_bundle(bundle: &Path) -> Result<(), Failure> {
    let bundle = bundle
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Application path is not valid Unicode."))?;
    run_tool(
        "/usr/bin/codesign",
        &["--force", "--sign", "-", "--timestamp=none", bundle],
    )
    .await
    .map(|_| ())
}

fn info_plist(application: &NativeApplication) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleDisplayName</key><string>{name}</string>
<key>CFBundleExecutable</key><string>{executable}</string>
<key>CFBundleIdentifier</key><string>{identifier}</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
"#,
        name = xml_escape(&application.name),
        executable = application.executable_name,
        identifier = application.application_id,
        version = application.version,
    )
}

fn swift_source(application: &NativeApplication) -> String {
    SWIFT_HOST
        .replace(
            "__APP_TYPE__",
            &format!("{}App", application.executable_name),
        )
        .replace("__APP_NAME__", &quoted_string_escape(&application.name))
        .replace(
            "__BUNDLE_ID__",
            &quoted_string_escape(&application.application_id),
        )
}

const SWIFT_HOST: &str = r#"import AppKit
import Foundation
import SwiftUI
import WebKit

private struct NativeAccessibility: Decodable {
    let role: String?
    let label: String?
}

private struct NativeNode: Decodable, Identifiable {
    let kind: String
    let id: String?
    let adapter: String?
    let properties: [String: String]?
    let events: [String: String]?
    let accessibility: NativeAccessibility?
    let children: [NativeNode]?
    let value: String?
    let source: String?
    let location: String?
    let bridge: String?
    let reason: String?
}

private struct NativeDocument: Decodable {
    let contract_version: Int
    let target: String
    let root: NativeNode
}

private struct RouteEntry: Decodable {
    let route: String
    let document: String
}

private struct NativeIndex: Decodable {
    let contract_version: Int
    let entry_route: String
    let routes: [RouteEntry]
    let initial_state: [String: [String: String]]
}

@MainActor
private final class NativeModel: ObservableObject {
    static let shared = NativeModel()
    @Published var root: NativeNode?
    @Published var state: [String: String] = [:]
    @Published var error: String?
    private var index: NativeIndex?
    private(set) var route = "/"
    private(set) var lifecycle = "created"

    private init() {
        record("controller.created")
        do {
            let indexURL = Bundle.main.url(forResource: "NativeIndex", withExtension: "json")!
            index = try JSONDecoder().decode(NativeIndex.self, from: Data(contentsOf: indexURL))
            open(index?.entry_route ?? "/")
        } catch {
            self.error = "Unable to load native application resources."
            record("controller.failed")
        }
    }

    func mount() {
        guard lifecycle == "created" || lifecycle == "suspended" else { return }
        lifecycle = "mounted"
        record("controller.mounted")
    }

    func activate() {
        guard lifecycle != "destroyed", lifecycle != "active" else { return }
        lifecycle = "active"
        record("controller.active")
    }

    func suspend() {
        guard lifecycle == "active" || lifecycle == "mounted" else { return }
        lifecycle = "suspended"
        record("controller.suspended")
    }

    func destroy() {
        guard lifecycle != "destroyed" else { return }
        lifecycle = "destroyed"
        record("controller.destroyed")
    }

    func open(_ route: String) {
        guard let index,
              let entry = index.routes.first(where: { $0.route == route })
                ?? index.routes.first(where: { matches($0.route, route) })
        else {
            error = "Native route is unavailable."
            record("route.failed")
            return
        }
        do {
            let relative = entry.document.replacingOccurrences(of: ".json", with: "")
            let url = Bundle.main.url(forResource: relative, withExtension: "json", subdirectory: "NativeUI")!
            let document = try JSONDecoder().decode(NativeDocument.self, from: Data(contentsOf: url))
            guard document.contract_version == 1, document.target == "macos" else {
                throw CocoaError(.fileReadCorruptFile)
            }
            self.route = route
            state = index.initial_state[entry.route] ?? [:]
            root = document.root
            error = nil
            record("route.opened")
        } catch {
            self.error = "Unable to decode native route."
            record("route.failed")
        }
    }

    private func matches(_ pattern: String, _ candidate: String) -> Bool {
        let expected = pattern.split(separator: "/", omittingEmptySubsequences: true)
        let actual = candidate.split(separator: "/", omittingEmptySubsequences: true)
        guard expected.count == actual.count else { return false }
        return zip(expected, actual).allSatisfy { part, value in
            part.hasPrefix("_") ? !value.isEmpty : part == value
        }
    }

    func binding(_ name: String) -> Binding<String> {
        Binding(
            get: { self.state[name] ?? "" },
            set: { value in
                var updated = self.state
                updated[name] = String(value.prefix(4096))
                self.state = updated
                self.record("state.input")
            }
        )
    }

    func dispatch(_ action: String?) {
        guard lifecycle != "destroyed",
              let action,
              let separator = action.firstIndex(of: ":") else { return }
        let verb = String(action[..<separator])
        let key = String(action[action.index(after: separator)...])
        if verb == "increment", let value = Int(state[key] ?? "") {
            var updated = state
            updated[key] = String(value + 1)
            state = updated
            record("state.increment")
        } else if verb == "toggle" {
            var updated = state
            updated[key] = state[key] == "true" ? "false" : "true"
            state = updated
            record("state.toggle")
        }
    }

    func record(_ event: String) {
        let allowed = String(event.prefix(128))
        let directory = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Logs/Tachyon", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("__BUNDLE_ID__.jsonl")
        let entry = "{\"event\":\"" + allowed + "\",\"route\":\"" + String(route.prefix(256)) + "\"}\n"
        guard let data = entry.data(using: .utf8) else { return }
        if !FileManager.default.fileExists(atPath: url.path) {
            FileManager.default.createFile(atPath: url.path, contents: data)
        } else if let handle = try? FileHandle(forWritingTo: url) {
            try? handle.seekToEnd()
            try? handle.write(contentsOf: data)
            try? handle.close()
        }
    }
}

private func nodeText(_ node: NativeNode) -> String {
    if node.kind == "text" { return node.value ?? "" }
    return (node.children ?? []).map(nodeText).joined()
}

private func accessibility(_ view: AnyView, node: NativeNode) -> AnyView {
    if node.adapter == "control.button" { return view }
    var result = view
    let role = node.accessibility?.role ?? ""
    let containerRoles = ["main", "navigation", "banner", "contentinfo", "group", "list", "listitem"]
    if containerRoles.contains(role) {
        result = AnyView(result.accessibilityElement(children: .contain))
    } else if node.adapter?.hasPrefix("layout.") != true, let id = node.id {
        result = AnyView(result.accessibilityIdentifier(id))
    }
    if let label = node.accessibility?.label,
       !label.isEmpty,
       containerRoles.contains(role) || nodeText(node).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        result = AnyView(result.accessibilityLabel(Text(label)))
    }
    if node.accessibility?.role == "heading" {
        result = AnyView(result.accessibilityAddTraits(.isHeader))
    }
    if node.properties?["aria-hidden"] == "true" {
        result = AnyView(result.accessibilityHidden(true))
    }
    return result
}

private struct AccessibleButton: NSViewRepresentable {
    let title: String
    let label: String
    let identifier: String
    let action: () -> Void

    final class Coordinator: NSObject {
        var action: () -> Void
        init(action: @escaping () -> Void) { self.action = action }
        @objc func press() { action() }
    }

    func makeCoordinator() -> Coordinator { Coordinator(action: action) }

    func makeNSView(context: Context) -> NSButton {
        let button = NSButton(
            title: title,
            target: context.coordinator,
            action: #selector(Coordinator.press)
        )
        button.bezelStyle = .rounded
        button.setAccessibilityLabel(label)
        button.setAccessibilityIdentifier(identifier)
        return button
    }

    func updateNSView(_ button: NSButton, context: Context) {
        context.coordinator.action = action
        button.title = title
        button.setAccessibilityLabel(label)
        button.setAccessibilityIdentifier(identifier)
    }
}

private struct AccessibleTextField: NSViewRepresentable {
    let placeholder: String
    let label: String
    let identifier: String
    @Binding var value: String

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var value: Binding<String>
        init(value: Binding<String>) { self.value = value }
        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            value.wrappedValue = field.stringValue
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(value: $value) }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(string: value)
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.setAccessibilityLabel(label)
        field.setAccessibilityIdentifier(identifier)
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.value = $value
        if field.stringValue != value { field.stringValue = value }
        field.placeholderString = placeholder
        field.setAccessibilityLabel(label)
        field.setAccessibilityIdentifier(identifier)
    }
}

private final class TachyonAssetSchemeHandler: NSObject, WKURLSchemeHandler {
    static let shared = TachyonAssetSchemeHandler()

    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url,
              url.scheme == "tachyon-app",
              url.host == "bundle",
              let resources = Bundle.main.resourceURL
        else {
            task.didFailWithError(NSError(domain: "TachyonAsset", code: 1))
            return
        }
        let root = resources.standardizedFileURL
        let path = url.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let direct = root.appendingPathComponent(path).standardizedFileURL
        let bundled = root.appendingPathComponent("WebBundle").appendingPathComponent(path).standardizedFileURL
        let file = FileManager.default.fileExists(atPath: direct.path) ? direct : bundled
        guard !path.isEmpty,
              !path.split(separator: "/").contains(".."),
              file.path.hasPrefix(root.path + "/"),
              let data = try? Data(contentsOf: file)
        else {
            task.didFailWithError(NSError(domain: "TachyonAsset", code: 2))
            return
        }
        let response = URLResponse(
            url: url,
            mimeType: TachyonAssetSchemeHandler.mimeType(file.pathExtension),
            expectedContentLength: data.count,
            textEncodingName: nil
        )
        task.didReceive(response)
        task.didReceive(data)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}

    private static func mimeType(_ extensionName: String) -> String {
        switch extensionName.lowercased() {
        case "html": return "text/html"
        case "css": return "text/css"
        case "js", "mjs": return "text/javascript"
        case "json": return "application/json"
        case "svg": return "image/svg+xml"
        case "png": return "image/png"
        case "wasm": return "application/wasm"
        default: return "application/octet-stream"
        }
    }
}

private let tachyonSurfaceHeightScript = """
(() => { const top = document.body.getBoundingClientRect().top; let bottom = top;
for (const node of document.body.querySelectorAll('*')) {
  const rect = node.getBoundingClientRect();
  if (rect.width || rect.height) bottom = Math.max(bottom, rect.bottom);
}
return Math.ceil(bottom - top); })()
"""

private final class WebSurfaceCoordinator: NSObject, WKNavigationDelegate {
    let node: NativeNode
    let record: (String) -> Void
    let open: @MainActor (String) -> Void
    /// What the document reported it needs, or nothing until it has loaded.
    var measured: CGFloat?
    private var measurementTimer: Timer?

    init(
        node: NativeNode,
        record: @escaping (String) -> Void,
        open: @escaping @MainActor (String) -> Void
    ) {
        self.node = node
        self.record = record
        self.open = open
        super.init()
        record("websurface.created")
    }

    /// A macOS `WKWebView` exposes no scroll view to measure, so the document
    /// is asked for its content extent. The bounded timer also catches layout
    /// changes caused by island interaction, such as an expanding menu.
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        measure(webView)
        measurementTimer?.invalidate()
        measurementTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) {
            [weak self, weak webView] _ in
            if let webView { self?.measure(webView) }
        }
    }

    private func measure(_ webView: WKWebView) {
        guard node.source == "local_bundle" else { return }
        webView.evaluateJavaScript(tachyonSurfaceHeightScript) {
            [weak self, weak webView] value, _ in
            guard let height = (value as? NSNumber).map({ CGFloat($0.doubleValue) }),
                  height > 0
            else { return }
            guard self?.measured != height else { return }
            self?.measured = height
            webView?.invalidateIntrinsicContentSize()
        }
    }

    func webView(
        _ webView: WKWebView,
        decidePolicyFor action: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let url = action.request.url else {
            decisionHandler(.cancel)
            return
        }
        if node.source == "local_bundle" {
            let contained = url.scheme == "tachyon-app" && url.host == "bundle"
            if contained,
               action.navigationType == .linkActivated,
               !url.path.hasPrefix("/WebSurfaces/") {
                Task { @MainActor in open(url.path.isEmpty ? "/" : url.path) }
                decisionHandler(.cancel)
                return
            }
            decisionHandler(contained ? .allow : .cancel)
            return
        }
        if node.source == "remote_url",
           let declared = URL(string: node.location ?? ""),
           url.scheme == "https",
           url.host == declared.host,
           (url.port ?? 443) == (declared.port ?? 443) {
            decisionHandler(.allow)
            return
        }
        decisionHandler(.cancel)
    }

    deinit {
        measurementTimer?.invalidate()
        record("websurface.destroyed")
    }
}

private let tachyonWebDataStore = WKWebsiteDataStore.nonPersistent()

private struct WebSurfaceView: NSViewRepresentable {
    let node: NativeNode
    let model: NativeModel

    func makeCoordinator() -> WebSurfaceCoordinator {
        WebSurfaceCoordinator(node: node, record: model.record, open: model.open)
    }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = tachyonWebDataStore
        configuration.setURLSchemeHandler(
            TachyonAssetSchemeHandler.shared,
            forURLScheme: "tachyon-app"
        )
        configuration.defaultWebpagePreferences.allowsContentJavaScript = node.source == "local_bundle"
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.setValue(false, forKey: "drawsBackground")
        if node.source == "local_bundle",
           let location = node.location,
           var components = URLComponents(string: "tachyon-app://bundle/\(location)") {
            components.queryItems = [URLQueryItem(name: "tachyon-route", value: model.route)]
            guard let url = components.url else { return view }
            view.load(URLRequest(url: url))
        } else if let location = node.location, let url = URL(string: location) {
            view.load(URLRequest(url: url))
        }
        model.record("websurface.attached")
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {}

    /// A fallback subtree is as tall as its document. A fixed height clipped
    /// whatever rendered past it, and a native window has no scroll of its own
    /// to reveal the rest.
    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: WKWebView,
        context: Context
    ) -> CGSize? {
        CGSize(
            width: proposal.width ?? nsView.frame.width,
            height: context.coordinator.measured ?? 180
        )
    }

    static func dismantleNSView(_ view: WKWebView, coordinator: WebSurfaceCoordinator) {
        view.stopLoading()
        coordinator.record("websurface.detached")
    }
}

private struct StateOutputView: View {
    @ObservedObject var model: NativeModel
    let binding: String
    let prefix: String
    let label: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let label { Text(label).font(.caption) }
            Text(prefix + (model.state[binding] ?? "")).font(.title2).monospacedDigit()
        }
    }
}

private struct DisclosureNodeView: View {
    @ObservedObject var model: NativeModel
    let node: NativeNode
    let children: [NativeNode]

    var body: some View {
        let key = node.properties?["binding"] ?? (node.id ?? "disclosure")
        let expanded = Binding(
            get: { model.state[key] == "true" },
            set: {
                var updated = model.state
                updated[key] = $0 ? "true" : "false"
                model.state = updated
                model.record("state.disclosure")
            }
        )
        DisclosureGroup(node.properties?["label"] ?? "Details", isExpanded: expanded) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(children.indices, id: \.self) { nodeView(children[$0], model: model) }
            }
        }
    }
}

@MainActor
private func nodeView(_ node: NativeNode, model: NativeModel) -> AnyView {
    if node.kind == "text" {
        return AnyView(Text(node.value ?? ""))
    }
    if node.kind == "web_surface" {
        return accessibility(
            AnyView(WebSurfaceView(node: node, model: model)),
            node: node
        )
    }
    let children = node.children ?? []
    let adapter = node.adapter ?? ""
    let built: AnyView
    switch adapter {
    case "layout.app_bar":
        built = AnyView(
            HStack(spacing: 12) {
                ForEach(children.indices, id: \.self) { nodeView(children[$0], model: model) }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.accentColor.opacity(0.12))
        )
    case "layout.column":
        let isMain = node.accessibility?.role == "main"
        let containsOnlyWebSurface = children.count == 1 && children.first?.kind == "web_surface"
        built = AnyView(
            VStack(alignment: .leading, spacing: isMain && !containsOnlyWebSurface ? 16 : 0) {
                ForEach(children.indices, id: \.self) { nodeView(children[$0], model: model) }
            }
            .padding(isMain && !containsOnlyWebSurface ? 24 : 0)
            .frame(maxWidth: .infinity, alignment: .leading)
        )
    case "layout.list", "layout.list_item":
        built = AnyView(
            VStack(alignment: .leading, spacing: 12) {
                ForEach(children.indices, id: \.self) { nodeView(children[$0], model: model) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        )
    case "text.heading1":
        built = AnyView(Text(nodeText(node)).font(.largeTitle).bold())
    case "text.heading2":
        built = AnyView(Text(nodeText(node)).font(.title).bold())
    case "text.heading3", "text.heading4", "text.heading5", "text.heading6":
        built = AnyView(Text(nodeText(node)).font(.headline).bold())
    case "content.text":
        built = AnyView(Text(nodeText(node)))
    case "control.button":
        built = AnyView(
            AccessibleButton(
                title: nodeText(node),
                label: node.accessibility?.label ?? nodeText(node),
                identifier: node.id ?? "",
                action: { model.dispatch(node.properties?["action"]) }
            )
            .fixedSize()
        )
    case "control.text_field":
        let binding = node.properties?["binding"] ?? ""
        let placeholder = node.properties?["placeholder"] ?? ""
        built = AnyView(
            AccessibleTextField(
                placeholder: placeholder,
                label: node.accessibility?.label ?? placeholder,
                identifier: node.id ?? "",
                value: model.binding(binding)
            )
        )
    case "content.output":
        if let binding = node.properties?["binding"] {
            built = AnyView(
                StateOutputView(
                    model: model,
                    binding: binding,
                    prefix: node.properties?["prefix"] ?? "",
                    label: node.accessibility?.label
                )
            )
        } else {
            built = AnyView(Text(nodeText(node)))
        }
    case "control.disclosure":
        built = AnyView(
            DisclosureNodeView(model: model, node: node, children: children)
        )
    case "navigation.link":
        built = AnyView(Button(nodeText(node)) { model.open(node.properties?["href"] ?? "/") }.buttonStyle(.link))
    case "content.image":
        built = AnyView(
            Label(node.accessibility?.label ?? "Image", systemImage: "photo")
        )
    case "content.divider":
        built = AnyView(Divider())
    default:
        built = AnyView(
            VStack(alignment: .leading, spacing: 8) {
                ForEach(children.indices, id: \.self) { nodeView(children[$0], model: model) }
            }
        )
    }
    return accessibility(built, node: node)
}

private struct RootView: View {
    @ObservedObject private var model = NativeModel.shared
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            Group {
                if let root = model.root {
                    nodeView(root, model: model)
                } else if let error = model.error {
                    Text(error).foregroundColor(.red)
                } else {
                    ProgressView()
                }
            }
            .frame(maxWidth: .infinity, alignment: .topLeading)
        }
        .frame(minWidth: 390, idealWidth: 420, minHeight: 640, idealHeight: 780)
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear { model.mount(); model.activate() }
        .onChange(of: scenePhase) { phase in
            if phase == .active { model.activate() }
            else { model.suspend() }
        }
    }
}

@MainActor
private final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationWillTerminate(_ notification: Notification) {
        NativeModel.shared.destroy()
    }
}

@main
private struct __APP_TYPE__: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    var body: some Scene {
        WindowGroup("__APP_NAME__") { RootView() }
            .windowStyle(.hiddenTitleBar)
            .defaultSize(width: 420, height: 780)
    }
}
"#;

#[cfg(test)]
mod tests {
    use super::{info_plist, swift_source};
    use crate::native::config::NativeApplication;

    fn application() -> NativeApplication {
        NativeApplication {
            name: String::from("Native Catalog"),
            executable_name: String::from("NativeCatalog"),
            application_id: String::from("dev.tachyon.native-catalog"),
            version: String::from("1.0.0"),
            entry_route: String::from("/"),
        }
    }

    #[test]
    fn generated_host_has_fixed_adapters_lifecycle_accessibility_and_no_bridge() {
        let source = swift_source(&application());
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains("websiteDataStore = tachyonWebDataStore"));
        assert!(source.contains("TachyonAssetSchemeHandler"));
        assert!(source.contains("tachyon-app://bundle/"));
        assert!(source.contains("(url.port ?? 443) == (declared.port ?? 443)"));
        assert!(source.contains("accessibilityIdentifier"));
        assert!(!source.contains("WKScriptMessageHandler"));
        assert!(source.contains("case \"control.button\""));
        assert!(source.contains("case \"layout.app_bar\""));
        assert!(source.contains(r#"document.target == "macos""#));
    }

    #[test]
    fn plist_contains_portable_bundle_metadata() {
        let plist = info_plist(&application());
        assert!(plist.contains("dev.tachyon.native-catalog"));
        assert!(plist.contains("<string>NativeCatalog</string>"));
        assert!(plist.contains("<string>Native Catalog</string>"));
    }
}
