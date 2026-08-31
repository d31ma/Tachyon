//! `macOS` host generation.
//!
//! The host is a window around the application's own web bundle. It used to
//! rebuild the view out of `SwiftUI` controls; see `native/routes.rs` for why
//! that went.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, first_line, native_io, native_tool_failure, quoted_string_escape, run_tool,
    stage_application, write, write_host_source, xml_escape,
};
use super::routes::NativeRouteIndex;
use crate::Failure;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MacOsHostGenerator;

impl MacOsHostGenerator {
    pub(super) async fn generate(
        application: &NativeApplication,
        index: &NativeRouteIndex,
        companions: &[super::registry::NativeCompanionInput],
        web_bundle: &Path,
        stage: &Path,
        package: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle = stage.join(format!("{}.app", application.executable_name));
        let contents = bundle.join("Contents");
        let resources = contents.join("Resources");
        stage_application(index, web_bundle, stage, &resources)?;

        // Compiled by its own compiler, with AppKit in reach: Swift into the
        // host binary, Rust as a static library the host links and calls
        // through the C ABI.
        let companion = stage_swift_companion(companions, stage)?;
        let rust_companion = super::rust::stage(companions, stage, &application.application_id)?;
        let swift_path = stage.join("project").join("TachyonHost.swift");
        write_host_source(
            &swift_path,
            &swift_source(
                application,
                companion_call(companion.as_deref(), rust_companion.as_deref()),
                index,
            ),
        )?;
        // The manifest's icon becomes the bundle's, so the Dock shows the
        // same artwork the browser tab does.
        let icon = stage_icon(application, web_bundle, &resources).await?;
        write(
            &contents.join("Info.plist"),
            info_plist(application, icon.as_deref()).as_bytes(),
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
        // A Rust companion becomes a static library before the Swift compile,
        // so the two link into one binary.
        let library = match &rust_companion {
            Some(source) => {
                let path = stage.join("project").join("libtachyoncompanion.a");
                super::rust::compile(source, super::rust::Linkage::Static, None, &path).await?;
                Some(path)
            }
            None => None,
        };
        let swift_version = compile_swift(
            &swift_path,
            companion.as_deref(),
            library.as_deref(),
            &executable,
        )
        .await?;
        sign_bundle(&bundle).await?;
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(format!("{}.app", application.executable_name)),
            toolchain_name: String::from("swift"),
            toolchain_version: swift_version,
        })
    }
}

/// Stages every route's selected Swift companion beside the host source.
///
/// Returns the staged path, or `None` when no route selected Swift.
pub(super) fn stage_swift_companion(
    companions: &[super::registry::NativeCompanionInput],
    stage: &Path,
) -> Result<Option<PathBuf>, Failure> {
    let Some(authored) =
        super::registry::source(companions, crate::project::NativeCompanion::Swift)?
    else {
        return Ok(None);
    };
    let staged = stage.join("project").join("TachyonCompanion.swift");
    write(
        &staged,
        format!("{authored}\n{SWIFT_COMPANION_PRELUDE}").as_bytes(),
    )?;
    Ok(Some(staged))
}

/// The prelude appended to a Swift companion compiled into this host.
const SWIFT_COMPANION_PRELUDE: &str = include_str!("prelude.swift");

/// The Swift companion prelude, for the publish-channel drift test.
#[cfg(test)]
pub(super) const fn companion_prelude() -> &'static str {
    SWIFT_COMPANION_PRELUDE
}

/// The Swift expression the host calls to reach its companion.
///
/// Written in only when one was staged: naming a symbol that is not in the
/// binary is a compile error, not a runtime fallback.
pub(super) fn companion_call(swift: Option<&Path>, rust: Option<&Path>) -> &'static str {
    if swift.is_some() && rust.is_some() {
        "tacRouteMembers(((try? JSONSerialization.jsonObject(with: Data(payload.utf8))) as? [String: Any])?[\"route\"] as? String ?? \"\") != nil ? tacNativeInvoke(payload) : tacRustInvoke(payload)"
    } else if swift.is_some() {
        "tacNativeInvoke(payload)"
    } else if rust.is_some() {
        "tacRustInvoke(payload)"
    } else {
        r#"((try? JSONSerialization.jsonObject(with: Data(payload.utf8))) as? [String: Any])?["op"] as? String == "init" ? "{\"value\":{\"fields\":[],\"methods\":[]}}" : "{\"error\":\"This application has no native companion.\"}""#
    }
}

/// The Swift statement that hands the companion its sink for `tacPublish`.
///
/// Derived from the call rather than passed alongside it: they are always the
/// same companion, and a second argument threaded through two hosts is a
/// second thing that can disagree.
pub(super) fn companion_emit(companion_call: &str) -> &'static str {
    if companion_call.contains("tacRustInvoke") && companion_call.contains("tacNativeInvoke") {
        "TacBridge.emit = tachyonRelayPublish; tacRustNativeSetEmit(tacRustEmit)"
    } else if companion_call.contains("tacRustInvoke") {
        "tacRustNativeSetEmit(tacRustEmit)"
    } else if companion_call.contains("tacNativeInvoke") {
        "TacBridge.emit = tachyonRelayPublish"
    } else {
        "// No companion, so nothing publishes."
    }
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
    let companion = companion
        .map(|path| {
            path.to_str().ok_or_else(|| {
                native_tool_failure(1605, "Companion source path is not valid Unicode.")
            })
        })
        .transpose()?;
    let mut arguments = vec![
        "swiftc",
        "-parse-as-library",
        "-O",
        "-framework",
        "AppKit",
        "-framework",
        "SwiftUI",
        "-framework",
        "WebKit",
        "-framework",
        "JavaScriptCore",
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

/// Builds an `.icns` from the manifest's largest raster icon.
///
/// `sips` and `iconutil` are macOS's own, so no third-party rasteriser is
/// pulled in. A project with no raster icon simply gets the system's default
/// application icon, which is what a missing icon should look like.
///
/// # Errors
///
/// Returns diagnostics when either tool refuses the image.
async fn stage_icon(
    application: &NativeApplication,
    web_bundle: &Path,
    resources: &Path,
) -> Result<Option<String>, Failure> {
    let Some(source) = application.largest_icon() else {
        return Ok(None);
    };
    let origin = web_bundle.join(source.trim_start_matches('/'));
    if !origin.is_file() {
        return Ok(None);
    }
    let iconset = resources.join("AppIcon.iconset");
    super::host::native_io(std::fs::create_dir_all(&iconset), &iconset)?;
    // The sizes `iconutil` expects; each also at @2x, which is the same image
    // at twice the edge.
    for edge in [16u32, 32, 128, 256, 512] {
        for (scale, suffix) in [(1u32, String::new()), (2, String::from("@2x"))] {
            let pixels = edge * scale;
            let out = iconset.join(format!("icon_{edge}x{edge}{suffix}.png"));
            run_tool(
                "sips",
                &[
                    "-z",
                    &pixels.to_string(),
                    &pixels.to_string(),
                    &origin.to_string_lossy(),
                    "--out",
                    &out.to_string_lossy(),
                ],
            )
            .await?;
        }
    }
    let icns = resources.join("AppIcon.icns");
    run_tool(
        "iconutil",
        &[
            "-c",
            "icns",
            &iconset.to_string_lossy(),
            "-o",
            &icns.to_string_lossy(),
        ],
    )
    .await?;
    // The source images are build input, not something to ship in the bundle.
    let _ = std::fs::remove_dir_all(&iconset);
    Ok(Some(String::from("AppIcon")))
}

fn info_plist(application: &NativeApplication, icon: Option<&str>) -> String {
    // Named without its extension, which is what CFBundleIconFile wants.
    let icon = icon.map_or_else(String::new, |name| {
        format!("\n<key>CFBundleIconFile</key><string>{name}</string>")
    });
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleDisplayName</key><string>{name}</string>
<key>CFBundleExecutable</key><string>{executable}</string>
<key>CFBundleIdentifier</key><string>{identifier}</string>{icon}
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
        icon = icon,
    )
}

/// The dispatch arms for the window controls this application granted.
fn window_controls(window: &super::config::WindowConfiguration) -> String {
    let mut arms = String::new();
    if window.grants("minimize") {
        arms.push_str("        case \"window.minimize\":\n            NSApplication.shared.keyWindow?.miniaturize(nil)\n            return \"{\\\"ok\\\":true}\"\n");
    }
    if window.grants("maximize") {
        arms.push_str("        case \"window.maximize\":\n            NSApplication.shared.keyWindow?.zoom(nil)\n            return \"{\\\"ok\\\":true}\"\n");
    }
    if window.grants("fullscreen") {
        arms.push_str("        case \"window.fullscreen\":\n            NSApplication.shared.keyWindow?.toggleFullScreen(nil)\n            return \"{\\\"ok\\\":true}\"\n");
    }
    if window.grants("close") {
        arms.push_str("        case \"window.close\":\n            NSApplication.shared.keyWindow?.performClose(nil)\n            return \"{\\\"ok\\\":true}\"\n");
    }
    if window.grants("resize") {
        arms.push_str("        case \"window.resize\":\n            NativeBridge.resize(payload)\n            return \"{\\\"ok\\\":true}\"\n");
    }
    if window.grants("title") {
        arms.push_str("        case \"window.title\":\n            NativeBridge.retitle(payload)\n            return \"{\\\"ok\\\":true}\"\n");
    }
    arms
}

fn swift_source(
    application: &NativeApplication,
    companion_call: &str,
    index: &NativeRouteIndex,
) -> String {
    let window = &application.window;
    SWIFT_HOST
        .replace("__TACHYON_JSON_REQUEST__", APPLE_JSON_REQUEST)
        .replace("__WINDOW_WIDTH__", &window.width.to_string())
        .replace("__WINDOW_HEIGHT__", &window.height.to_string())
        .replace(
            "__WINDOW_MIN_WIDTH__",
            &window.min_width.unwrap_or(0).to_string(),
        )
        .replace(
            "__WINDOW_MIN_HEIGHT__",
            &window.min_height.unwrap_or(0).to_string(),
        )
        // Only what the manifest granted reaches the switch. A control the
        // application did not ask for is not refused at run time — there is
        // no arm for it to reach.
        .replace("__WINDOW_CONTROLS__", &window_controls(window))
        .replace("__ENTRY_DOCUMENT__", &quoted_string_escape(&index.entry_document))
        .replace("__ENTRY_ROUTE__", &quoted_string_escape(&index.entry_route))
        .replace("__ROUTE_DOCUMENTS__", &super::routes::swift_routes(index))
        .replace("__NATIVE_SHIM__", &super::host::native_shim(&application.window))
        .replace(
            "__APP_TYPE__",
            &format!("{}App", application.executable_name),
        )
        .replace("__APP_NAME__", &quoted_string_escape(&application.name))
        .replace(
            "__BUNDLE_ID__",
            &quoted_string_escape(&application.application_id),
        )
        // The call is written in only when a companion was staged: referring
        // to a symbol that is not in the binary is a compile error, not a
        // runtime one.
        .replace("__TACHYON_COMPANION_CALL__", companion_call)
        .replace("__TACHYON_COMPANION_EMIT__", companion_emit(companion_call))
        .replace(
            "__TACHYON_RUST_SHIM__",
            if companion_call.contains("tacRustInvoke") {
                RUST_SHIM
            } else {
                ""
            },
        )
}

/// One bounded Foundation JSON boundary shared by Apple hosts and companions.
pub(super) const APPLE_JSON_REQUEST: &str = include_str!("apple_json.swift");

/// The Swift side of a Rust companion: two C declarations and the call.
pub(super) const RUST_SHIM: &str = r#"// A Rust companion is linked as a static library and reached through the C
// ABI — the one call every host can make. Declared only when one was linked:
// naming a symbol that is not in the binary is a link error.
@_silgen_name("tac_native_invoke")
private func tacRustNativeInvoke(_ request: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?

@_silgen_name("tac_native_free")
private func tacRustNativeFree(_ answer: UnsafeMutablePointer<CChar>?)

@_silgen_name("tac_native_set_emit")
private func tacRustNativeSetEmit(_ emit: @convention(c) (UnsafePointer<CChar>?) -> Void)

private func tacRustInvoke(_ request: String) -> String {
    request.withCString { pointer in
        guard let answer = tacRustNativeInvoke(pointer) else {
            return "{\"error\":\"The companion returned nothing.\"}"
        }
        defer { tacRustNativeFree(answer) }
        return String(cString: answer)
    }
}

/// The sink handed to a Rust companion. Copies before returning, because the
/// pointer is only borrowed for the length of the call.
private func tacRustEmit(_ payload: UnsafePointer<CChar>?) {
    guard let payload else { return }
    tachyonRelayPublish(String(cString: payload))
}"#;

/// The generated host source, for the capability-drift test.
///
/// The dispatch arms live in this string rather than in Rust, so the only way
/// to assert that a host implements what the bundle advertises is to read it.
#[cfg(test)]
pub(super) const fn host_source() -> &'static str {
    SWIFT_HOST
}

const SWIFT_HOST: &str = r##"import AppKit
import WebKit
import Foundation

// The application's own web bundle is what this window shows. Tachyon used to
// lower a Tac view into SwiftUI controls and fall back to a web view wherever
// no adapter existed; on a real design almost everything fell back, and what
// did not looked nothing like the rest. The bundle renders identically on
// every platform, and matching macOS specifically is a choice an author makes
// in their own UI layer.

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



    /// Resizes the key window to a payload's width and height.
    static func resize(_ payload: String) {
        let data = payload.data(using: .utf8) ?? Data()
        let parsed = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        guard let window = NSApplication.shared.keyWindow else { return }
        let width = (parsed["width"] as? NSNumber)?.doubleValue ?? Double(window.frame.width)
        let height = (parsed["height"] as? NSNumber)?.doubleValue ?? Double(window.frame.height)
        guard width.isFinite, height.isFinite,
              (64...16384).contains(width), (64...16384).contains(height) else { return }
        window.setContentSize(NSSize(width: width, height: height))
    }

    /// Sets the key window's title from a payload.
    static func retitle(_ payload: String) {
        let data = payload.data(using: .utf8) ?? Data()
        let parsed = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
        guard let title = parsed["title"] as? String else { return }
        NSApplication.shared.keyWindow?.title = String(title.prefix(256))
    }

    private func handle(_ capability: String, _ payload: String) -> String {
        // One channel, and no vocabulary. A tray, a window or a notification
        // belongs to AppKit, and a `tac.swift` compiled into this binary has
        // AppKit already — a verb list in the middle could only ever be the
        // subset Tachyon had got round to implementing.
        switch capability {
__WINDOW_CONTROLS__
        case "companion.invoke":
            // The companion is compiled into this binary, so this is a direct
            // Swift call rather than a module boundary. Its answer is the
            // protocol's own JSON, relayed unchanged.
            return __TACHYON_COMPANION_CALL__
        default:
            // Naming both halves: what was asked for, and which platform
            // declined it.
            return "{\"ok\":false,\"error\":\"macos host answers companion.invoke, not '\(NativeBridge.safeName(capability))'\"}"
        }
    }

    private static func jsonString(_ value: String) -> String {
        let data = (try? JSONSerialization.data(withJSONObject: [value])) ?? Data("[\"/\"]".utf8)
        let text = String(data: data, encoding: .utf8) ?? "[\"/\"]"
        return String(text.dropFirst().dropLast())
    }

    /// The capability is echoed into a JSON string, so anything that could
    /// close that string early is replaced rather than escaped.
    private static func safeName(_ value: String) -> String {
        let allowed = value.prefix(64).filter { character in
            character.isLetter || character.isNumber || character == "." || character == "_" || character == "-"
        }
        return allowed.isEmpty ? "unnamed" : String(allowed)
    }
}

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

@MainActor
private final class AppDelegate: NSObject, NSApplicationDelegate, WKNavigationDelegate {
    private var window: NSWindow?
    private var webView: WKWebView?

    func applicationDidFinishLaunching(_ notification: Notification) {
        Telemetry.shared.record("controller.created")
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(TachyonAssetSchemeHandler.shared, forURLScheme: "tachyon-app")
        configuration.websiteDataStore = .default()
        let shim = WKUserScript(
            source: TACHYON_NATIVE_SHIM,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        )
        configuration.userContentController.addUserScript(shim)
        configuration.userContentController.addScriptMessageHandler(
            NativeBridge.shared,
            contentWorld: .page,
            name: "tachyon"
        )

        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = self
        view.setValue(false, forKey: "drawsBackground")
        view.setAccessibilityIdentifier("tachyon-application")
        webView = view
        NativeBridge.shared.webView = view
        // Installed before the page loads, so a companion that publishes the
        // moment it wakes up has somewhere to publish to. The shim queues
        // until the page's modules take over, so nothing is lost either way.
        __TACHYON_COMPANION_EMIT__

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: __WINDOW_WIDTH__, height: __WINDOW_HEIGHT__),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "__APP_NAME__"
        window.contentView = view
        window.center()
        window.contentMinSize = NSSize(width: __WINDOW_MIN_WIDTH__, height: __WINDOW_MIN_HEIGHT__)
        window.makeKeyAndOrderFront(nil)
        self.window = window
        NSApplication.shared.activate(ignoringOtherApps: true)

        view.load(URLRequest(url: URL(string: "tachyon-app://bundle/" + tachyonEntryDocument)!))
        Telemetry.shared.record("bridge.ready")
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = action.request.url else { decisionHandler(.cancel); return }
        if url.scheme == "tachyon-app" && url.host == "bundle" && url.port == nil && url.user == nil && url.password == nil { decisionHandler(.allow); return }
        decisionHandler(.cancel)
        if action.targetFrame?.isMainFrame != false && ["https", "http"].contains(url.scheme ?? "") {
            NSWorkspace.shared.open(url)
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Telemetry.shared.record("controller.mounted")
        Telemetry.shared.record("controller.active")
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        Telemetry.shared.record("route.failed")
    }

    func applicationWillTerminate(_ notification: Notification) {
        Telemetry.shared.record("controller.destroyed")
    }
}

private let TACHYON_NATIVE_SHIM = #"""
__NATIVE_SHIM__
"""#

@main
private struct __APP_TYPE__ {
    static func main() {
        let application = NSApplication.shared
        let delegate = AppDelegate()
        application.delegate = delegate
        application.setActivationPolicy(.regular)
        // NSApplication.delegate is weak. Keep the owner of the window and
        // WebView alive for the entire native event loop, including -O builds.
        withExtendedLifetime(delegate) { application.run() }
    }
}
"##;

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
    fn the_host_is_a_window_around_the_applications_own_bundle() {
        // With a companion, so the generated call is the one a real build emits.
        let source = swift_source(&application(), "tacNativeInvoke(payload)", &index());
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains("TachyonAssetSchemeHandler"));
        assert!(source.contains("tachyon-app://bundle/"));
        assert!(source.contains("tacNativeInvoke(payload)"));
        // The bridge reaches the page rather than a separate engine.
        assert!(source.contains("WKScriptMessageHandlerWithReply"));
        assert!(source.contains("__tachyonNativeHostCall"));
        // Nothing rebuilds the view out of platform widgets any more.
        assert!(!source.contains("case \"control.button\""));
        assert!(!source.contains("nodeView"));
    }

    #[test]
    fn a_host_without_a_companion_names_no_companion_symbol() {
        // Referring to a symbol that is not in the binary is a compile error,
        // not a runtime fallback.
        let source = swift_source(&application(), super::companion_call(None, None), &index());
        assert!(!source.contains("tacNativeInvoke"));
        assert!(source.contains("has no native companion"));
    }

    #[test]
    fn plist_declares_a_launchable_bundle() {
        let plist = info_plist(&application(), Some("AppIcon"));
        assert!(plist.contains("dev.tachyon.native-catalog"));
        assert!(plist.contains("NativeCatalog"));
    }
}
