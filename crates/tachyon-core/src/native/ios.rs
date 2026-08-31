//! `iOS` host generation.
//!
//! A full-screen web view over the application's own bundle; see
//! `native/routes.rs` for why it is no longer a tree of `UIKit` controls.
//! Phase 5 targets the iOS Simulator. Device provisioning and distribution
//! signing remain deferred and are recorded in `docs/SUPPORT_TIERS.md`.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, first_line, native_tool_failure, quoted_string_escape, run_tool,
    stage_application, write, write_host_source, xml_escape,
};
use super::routes::NativeRouteIndex;
use crate::Failure;
use std::path::{Path, PathBuf};

/// Minimum iOS deployment version accepted by the generated host.
const DEPLOYMENT_TARGET: &str = "17.0";

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct IosHostGenerator;

impl IosHostGenerator {
    pub(super) async fn generate(
        application: &NativeApplication,
        index: &NativeRouteIndex,
        companions: &[super::registry::NativeCompanionInput],
        web_bundle: &Path,
        stage: &Path,
        package: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle_name = format!("{}.app", application.executable_name);
        let bundle = stage.join(&bundle_name);
        stage_application(index, web_bundle, stage, &bundle)?;

        let companion = super::macos::stage_swift_companion(companions, stage)?;
        let swift_path = stage.join("project").join("TachyonHost.swift");
        write_host_source(
            &swift_path,
            &swift_source(
                application,
                super::macos::companion_call(companion.as_deref(), None),
                index,
            ),
        )?;
        // iOS reads a flat list of icon files from the bundle rather than a
        // path, so the manifest's icon is staged under the names it looks for.
        let icons = stage_icons(application, web_bundle, &bundle).await?;
        write(
            &bundle.join("Info.plist"),
            info_plist(application, &icons).as_bytes(),
        )?;
        if !package {
            return Ok(GeneratedHost {
                application_bundle: PathBuf::from("project/TachyonHost.swift"),
                toolchain_name: String::from("source"),
                toolchain_version: String::from("not-packaged"),
            });
        }

        let executable = bundle.join(&application.executable_name);
        let swift_version =
            compile_swift(&swift_path, companion.as_deref(), None, &executable).await?;
        sign_bundle(&bundle).await?;
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(bundle_name),
            toolchain_name: String::from("swift"),
            toolchain_version: swift_version,
        })
    }
}

/// Returns the simulator triple used for the current build machine.
fn simulator_triple() -> String {
    let architecture = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    format!("{architecture}-apple-ios{DEPLOYMENT_TARGET}-simulator")
}

async fn compile_swift(
    source: &Path,
    companion: Option<&Path>,
    library: Option<&Path>,
    executable: &Path,
) -> Result<String, Failure> {
    if !cfg!(target_os = "macos") {
        return Err(native_tool_failure(
            1605,
            "The iOS host requires an Apple macOS build machine with Xcode.",
        ));
    }
    let version = first_line(
        &run_tool(
            "/usr/bin/xcrun",
            &["--sdk", "iphonesimulator", "swiftc", "--version"],
        )
        .await?,
        "Swift unknown",
    );
    let source = source
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Swift source path is not valid Unicode."))?;
    let executable = executable
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Application path is not valid Unicode."))?;
    let triple = simulator_triple();
    let companion = companion
        .map(|path| {
            path.to_str().ok_or_else(|| {
                native_tool_failure(1605, "Companion source path is not valid Unicode.")
            })
        })
        .transpose()?;
    let mut arguments = vec![
        "--sdk",
        "iphonesimulator",
        "swiftc",
        "-parse-as-library",
        "-O",
        "-target",
        &triple,
        "-framework",
        "UIKit",
        "-framework",
        "SwiftUI",
        "-framework",
        "WebKit",
        "-o",
        executable,
        source,
    ];
    // Compiled together, so the companion is part of the application rather
    // than something it loads.
    arguments.extend(companion);
    let library = library
        .map(|path| {
            path.to_str().ok_or_else(|| {
                native_tool_failure(1605, "Companion library path is not valid Unicode.")
            })
        })
        .transpose()?;
    arguments.extend(library);
    run_tool("/usr/bin/xcrun", &arguments).await?;
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

/// Stages the manifest's icon at the sizes iOS asks a bundle for.
///
/// `sips` resizes; there is no asset catalog because compiling one needs
/// `actool` and an Xcode project, and a bundle may simply carry its icons as
/// files. Returns the names for `CFBundleIconFiles`.
///
/// # Errors
///
/// Returns diagnostics when `sips` refuses the image.
async fn stage_icons(
    application: &NativeApplication,
    web_bundle: &Path,
    bundle: &Path,
) -> Result<Vec<String>, Failure> {
    let Some(source) = application.largest_icon() else {
        return Ok(Vec::new());
    };
    let origin = web_bundle.join(source.trim_start_matches('/'));
    if !origin.is_file() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    // The home screen and the settings list, at both densities.
    for (points, scale) in [(60u32, 2u32), (60, 3), (76, 2), (83, 2), (29, 2), (40, 2)] {
        let pixels = points * scale;
        let name = format!("AppIcon{points}x{points}@{scale}x.png");
        run_tool(
            "sips",
            &[
                "-z",
                &pixels.to_string(),
                &pixels.to_string(),
                &origin.to_string_lossy(),
                "--out",
                &bundle.join(&name).to_string_lossy(),
            ],
        )
        .await?;
        names.push(name);
    }
    Ok(names)
}

fn info_plist(application: &NativeApplication, icons: &[String]) -> String {
    // Declared only when there are files to name.
    let icon_block = if icons.is_empty() {
        String::new()
    } else {
        use std::fmt::Write as _;

        let files = icons.iter().fold(String::new(), |mut out, name| {
            let _ = write!(out, "<string>{name}</string>");
            out
        });
        format!(
            "\n<key>CFBundleIcons</key><dict><key>CFBundlePrimaryIcon</key><dict>\
             <key>CFBundleIconFiles</key><array>{files}</array>\
             </dict></dict>"
        )
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleDisplayName</key><string>{name}</string>
<key>CFBundleExecutable</key><string>{executable}</string>
<key>CFBundleIdentifier</key><string>{identifier}</string>{icon_block}
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array>
<key>DTPlatformName</key><string>iphonesimulator</string>
<key>MinimumOSVersion</key><string>{deployment}</string>
<key>UIDeviceFamily</key><array><integer>1</integer><integer>2</integer></array>
<key>UILaunchScreen</key><dict/>
<key>UISupportedInterfaceOrientations</key><array><string>UIInterfaceOrientationPortrait</string></array>
</dict></plist>
"#,
        name = xml_escape(&application.name),
        executable = application.executable_name,
        identifier = application.application_id,
        version = application.version,
        deployment = DEPLOYMENT_TARGET,
        icon_block = icon_block,
    )
}

fn swift_source(
    application: &NativeApplication,
    companion_call: &str,
    index: &NativeRouteIndex,
) -> String {
    SWIFT_HOST
        .replace("__TACHYON_JSON_REQUEST__", super::macos::APPLE_JSON_REQUEST)
        .replace(
            "__ENTRY_DOCUMENT__",
            &quoted_string_escape(&index.entry_document),
        )
        .replace("__ENTRY_ROUTE__", &quoted_string_escape(&index.entry_route))
        .replace("__ROUTE_DOCUMENTS__", &super::routes::swift_routes(index))
        .replace(
            "__NATIVE_SHIM__",
            &super::host::native_shim(&application.window),
        )
        .replace("__TACHYON_COMPANION_CALL__", companion_call)
        .replace(
            "__TACHYON_COMPANION_EMIT__",
            super::macos::companion_emit(companion_call),
        )
        .replace(
            "__TACHYON_RUST_SHIM__",
            if companion_call.contains("tacRustInvoke") {
                super::macos::RUST_SHIM
            } else {
                ""
            },
        )
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

/// The generated host source, for the cross-host drift tests.
///
/// The dispatch arms live in this string rather than in Rust, so reading it
/// is the only way to assert what this host does and does not implement.
#[cfg(test)]
pub(super) const fn host_source() -> &'static str {
    SWIFT_HOST
}

const SWIFT_HOST: &str = r##"import Foundation
import UIKit
import WebKit

// The application's own web bundle in a full-screen web view. See
// `native/routes.rs` for why this is no longer a tree of UIKit controls.

__TACHYON_RUST_SHIM__
__TACHYON_JSON_REQUEST__

private let tachyonEntryDocument = "__ENTRY_DOCUMENT__"
private let tachyonRouteDocuments: [String: String] = __ROUTE_DOCUMENTS__

private func tachyonRoute(_ path: String) -> String? {
    guard path.utf8.count <= 2048, !path.contains("\\"), !path.split(separator: "/").contains("..") else { return nil }
    if tachyonRouteDocuments[path] != nil { return path }
    if let document = tachyonRouteDocuments.first(where: { "/" + $0.value == path }) { return document.key }
    let parts = path.split(separator: "/")
    // Prefer exact routes, then the most-specific dynamic pattern.
    let patterns = tachyonRouteDocuments.keys.sorted {
        let a = $0.split(separator: "/").filter { !$0.hasPrefix("_") }.count
        let b = $1.split(separator: "/").filter { !$0.hasPrefix("_") }.count
        return a == b ? $0 < $1 : a > b
    }
    for route in patterns {
        let expected = route.split(separator: "/")
        if expected.count == parts.count && zip(expected, parts).allSatisfy({ $0.0.hasPrefix("_") || $0.0 == $0.1 }) { return route }
    }
    return nil
}

private func tachyonDocument(_ path: String) -> String? {
    if let route = tachyonRoute(path) { return tachyonRouteDocuments[route] }
    let parts = path.split(separator: "/")
    guard parts.count > 1 else { return nil }
    for count in stride(from: parts.count - 1, through: 1, by: -1) {
        let prefix = "/" + parts.prefix(count).joined(separator: "/")
        if let route = tachyonRoute(prefix), route.contains("/_"), let document = tachyonRouteDocuments[route] {
            let directory = (document as NSString).deletingLastPathComponent
            return directory + "/" + parts.dropFirst(count).joined(separator: "/")
        }
    }
    return nil
}

@MainActor
private final class Telemetry {
    static let shared = Telemetry()
    var route = "__ENTRY_ROUTE__"

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

// Served rather than loaded from file://, because a bundle is a module graph:
// WebKit refuses cross-origin module imports from file URLs, so the page would
// load and none of its JavaScript would.
private final class TachyonAssetSchemeHandler: NSObject, WKURLSchemeHandler {
    static let shared = TachyonAssetSchemeHandler()

    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url,
              url.scheme == "tachyon-app",
              url.host == "bundle", url.port == nil, url.user == nil, url.password == nil,
              let resources = Bundle.main.resourceURL
        else {
            task.didFailWithError(NSError(domain: "TachyonAsset", code: 1))
            return
        }
        let root = resources.appendingPathComponent("WebBundle").standardizedFileURL
        var path = url.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        if let document = tachyonDocument(url.path) { path = document }
        var file = root.appendingPathComponent(path).standardizedFileURL
        // A client-routed application asks for a path with no file behind it;
        // the document that owns that route is the answer, not a 404.
        var isDirectory: ObjCBool = false
        if !FileManager.default.fileExists(atPath: file.path, isDirectory: &isDirectory)
            || isDirectory.boolValue {
            file = root.appendingPathComponent(path)
                .appendingPathComponent("index.html").standardizedFileURL
        }
        file = file.resolvingSymlinksInPath()
        guard !path.split(separator: "/").contains(".."),
              file.path.hasPrefix(root.resolvingSymlinksInPath().path + "/"),
              let attributes = try? FileManager.default.attributesOfItem(atPath: file.path),
              let size = attributes[.size] as? NSNumber, size.uint64Value <= 16 * 1024 * 1024,
              let data = try? Data(contentsOf: file)
        else {
            task.didFailWithError(NSError(domain: "TachyonAsset", code: 2))
            return
        }
        let response = HTTPURLResponse(
            url: url,
            statusCode: 200,
            httpVersion: "HTTP/1.1",
            headerFields: [
                "Content-Type": TachyonAssetSchemeHandler.mimeType(file.pathExtension),
                "Content-Length": String(data.count),
            ]
        )
        guard let response else {
            task.didFailWithError(NSError(domain: "TachyonAsset", code: 3))
            return
        }
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
        case "json", "map": return "application/json"
        case "svg": return "image/svg+xml"
        case "png": return "image/png"
        case "jpg", "jpeg": return "image/jpeg"
        case "webp": return "image/webp"
        case "woff2": return "font/woff2"
        case "wasm": return "application/wasm"
        default: return "application/octet-stream"
        }
    }
}


// MARK: - JavaScript host bridge
//
// One function is the whole native surface: a capability name and a JSON
// payload in, a JSON answer out. It is reached from the page rather than from
// a separate engine, because the page is now the application.

@MainActor
private final class NativeBridge: NSObject, WKScriptMessageHandlerWithReply {
    static let shared = NativeBridge()
    weak var webView: WKWebView?

    func userContentController(
        _ controller: WKUserContentController,
        didReceive message: WKScriptMessage,
        replyHandler: @escaping (Any?, String?) -> Void
    ) {
        guard message.frameInfo.isMainFrame,
              message.frameInfo.request.url?.scheme == "tachyon-app",
              message.frameInfo.request.url?.host == "bundle",
              message.frameInfo.request.url?.port == nil, message.frameInfo.request.url?.user == nil,
              message.frameInfo.request.url?.password == nil,
              let body = message.body as? [String: Any],
              let capability = body["capability"] as? String
        else {
            replyHandler(nil, "malformed host call")
            return
        }
        let payload = body["payload"] as? String ?? "{}"
        guard capability.utf8.count <= 64,
              let request = tachyonParseJSONRequest(payload) else {
            replyHandler(nil, "invalid or oversized host call"); return
        }
        if capability == "companion.invoke" {
            guard let path = message.frameInfo.request.url?.path,
                  let route = tachyonRoute(path), request["route"] as? String == route else {
                replyHandler(nil, "companion route does not belong to this page"); return
            }
        }
        guard let canonicalPayload = tachyonCanonicalJSONRequest(request) else {
            replyHandler(nil, "invalid or oversized host call"); return
        }
        replyHandler(handle(capability, canonicalPayload), nil)
    }

    private func handle(_ capability: String, _ payload: String) -> String {
        let data = payload.data(using: .utf8) ?? Data()
        let parsed = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        switch capability {
        case "companion.invoke":
            return __TACHYON_COMPANION_CALL__
        case "route.open":
            let route = String((parsed["route"] as? String ?? "/").prefix(256))
            Telemetry.shared.route = route
            webView?.evaluateJavaScript("location.assign(\(NativeBridge.jsonString(route)))")
            Telemetry.shared.record("route.opened")
        default:
            // iOS has no status item, no window to show or hide, and an
            // application may not terminate itself: naming the platform is
            // what tells a developer this is a target difference rather than
            // a typo.
            return "{\"ok\":false,\"error\":\"ios host does not implement capability '\(NativeBridge.safeName(capability))'\"}"
        }
        return "{\"ok\":true,\"value\":{}}"
    }

    private static func jsonString(_ value: String) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: [value])) ?? Data("[\"/\"]".utf8)
        let text = String(data: data, encoding: .utf8) ?? "[\"/\"]"
        return String(text.dropFirst().dropLast())
    }

    private static func safeName(_ value: String) -> String {
        let allowed = value.prefix(64).filter { character in
            character.isLetter || character.isNumber || character == "." || character == "_" || character == "-"
        }
        return allowed.isEmpty ? "unnamed" : String(allowed)
    }
}

private let TACHYON_NATIVE_SHIM = #"""
__NATIVE_SHIM__
"""#

/// Relays one companion publish into the page.
///
/// A companion may publish from whatever thread it likes — a timer, a
/// notification, a URLSession callback — and a WKWebView may only be touched
/// from the main one. The hop belongs here rather than in every companion an
/// author writes.
///
/// The payload is the companion's own JSON object, and JSON is a JavaScript
/// expression already, so it is not re-encoded on the way through.
private func tachyonRelayPublish(_ payload: String) {
    Task { @MainActor in
        NativeBridge.shared.webView?.evaluateJavaScript(
            "globalThis.__tachyonCompanionPublish(" + payload + ")")
    }
}

private final class RootViewController: UIViewController, WKNavigationDelegate {
    private var webView: WKWebView?

    override func viewDidLoad() {
        super.viewDidLoad()
        Telemetry.shared.record("controller.created")
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(TachyonAssetSchemeHandler.shared, forURLScheme: "tachyon-app")
        configuration.websiteDataStore = .default()
        configuration.userContentController.addUserScript(
            WKUserScript(source: TACHYON_NATIVE_SHIM, injectionTime: .atDocumentStart, forMainFrameOnly: true)
        )
        configuration.userContentController.addScriptMessageHandler(
            NativeBridge.shared, contentWorld: .page, name: "tachyon"
        )
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = self
        view.accessibilityIdentifier = "tachyon-application"
        view.translatesAutoresizingMaskIntoConstraints = false
        self.view.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: self.view.topAnchor),
            view.bottomAnchor.constraint(equalTo: self.view.bottomAnchor),
            view.leadingAnchor.constraint(equalTo: self.view.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: self.view.trailingAnchor),
        ])
        webView = view
        NativeBridge.shared.webView = view
        // Installed before the page loads, so a companion that publishes the
        // moment it wakes up has somewhere to publish to. The shim queues
        // until the page's modules take over, so nothing is lost either way.
        __TACHYON_COMPANION_EMIT__
        view.load(URLRequest(url: URL(string: "tachyon-app://bundle/" + tachyonEntryDocument)!))
        Telemetry.shared.record("bridge.ready")
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = action.request.url else { decisionHandler(.cancel); return }
        if url.scheme == "tachyon-app" && url.host == "bundle" && url.port == nil && url.user == nil && url.password == nil { decisionHandler(.allow); return }
        decisionHandler(.cancel)
        if action.targetFrame?.isMainFrame != false && ["https", "http"].contains(url.scheme ?? "") {
            UIApplication.shared.open(url)
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Telemetry.shared.record("controller.mounted")
        Telemetry.shared.record("controller.active")
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        Telemetry.shared.record("route.failed")
    }
}

// The entry point is an attribute rather than a top-level call: the host is
// compiled with -parse-as-library, where an expression at file scope is an
// error rather than a program.
@UIApplicationMain
private final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = RootViewController()
        window.makeKeyAndVisible()
        self.window = window
        return true
    }

    func applicationWillTerminate(_ application: UIApplication) {
        Telemetry.shared.record("controller.destroyed")
    }
}
"##;

#[cfg(test)]
mod tests {
    use super::{DEPLOYMENT_TARGET, info_plist, simulator_triple, swift_source};
    use crate::native::config::NativeApplication;

    fn application() -> NativeApplication {
        NativeApplication {
            name: String::from("Native Catalog"),
            executable_name: String::from("NativeCatalog"),
            application_id: String::from("dev.tachyon.native-catalog"),
            version: String::from("1.0.0"),
            entry_route: String::from("/"),
            icons: Vec::new(),
            window: crate::native::config::WindowConfiguration::default(),
        }
    }

    fn index() -> crate::native::routes::NativeRouteIndex {
        crate::native::routes::NativeRouteIndex {
            contract_version: 2,
            entry_route: String::from("/"),
            entry_document: String::from("index.html"),
            routes: Vec::new(),
        }
    }

    #[test]
    fn the_host_is_a_full_screen_view_over_the_bundle() {
        let source = swift_source(&application(), "tacNativeInvoke(payload)", &index());
        assert!(source.contains("import UIKit"));
        assert!(!source.contains("import AppKit"));
        assert!(source.contains("controller.created"));
        assert!(source.contains("TachyonAssetSchemeHandler"));
        assert!(source.contains("tachyon-app://bundle/"));
        assert!(source.contains("WKScriptMessageHandlerWithReply"));
        assert!(!source.contains("UIViewRepresentable"));
    }

    #[test]
    fn plist_declares_a_launchable_simulator_bundle() {
        let plist = info_plist(&application(), &[String::from("AppIcon60x60@2x.png")]);
        assert!(plist.contains("dev.tachyon.native-catalog"));
        assert!(plist.contains("<string>iPhoneSimulator</string>"));
        assert!(plist.contains("<key>UILaunchScreen</key>"));
        assert!(plist.contains(DEPLOYMENT_TARGET));
    }

    #[test]
    fn simulator_triple_matches_the_build_machine() {
        let triple = simulator_triple();
        assert!(triple.ends_with("-simulator"));
        assert!(triple.contains(DEPLOYMENT_TARGET));
    }
}
