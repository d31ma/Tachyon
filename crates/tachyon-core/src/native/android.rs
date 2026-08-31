//! Android host generation.
//!
//! A full-screen `WebView` over the application's own bundle; see
//! `native/routes.rs` for why it is no longer a tree of Android views. The
//! project uses pinned `AndroidX` `WebKit` for a source-frame-aware bridge. The Kotlin
//! plugin is added only when the project declares a `tac.kt` companion, which
//! is compiled into the APK rather than to WebAssembly: that is what lets it
//! reach the Android SDK. Building an APK requires an Android SDK on the build
//! machine; the generator fails closed when one is absent.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, first_line, native_io, native_tool_failure, quoted_string_escape, run_tool_in,
    stage_application, write, write_host_source, xml_escape,
};
use super::routes::NativeRouteIndex;
use crate::Failure;
use std::fs;
use std::path::{Path, PathBuf};

/// Android Gradle Plugin pinned by every generated project.
const ANDROID_GRADLE_PLUGIN: &str = "8.7.3";
/// Google Maven AAR metadata requires compile SDK 34 and AGP 8.1.1 or newer.
const ANDROIDX_WEBKIT: &str = "1.14.0";
/// Kotlin plugin pinned by projects that declare a native companion.
const KOTLIN_PLUGIN: &str = "2.0.21";
/// Compile and target SDK pinned by every generated project.
const COMPILE_SDK: u32 = 35;
/// Minimum supported Android API level.
const MIN_SDK: u32 = 26;
/// Java package segments that cannot appear in a generated package name.
const JAVA_KEYWORDS: [&str; 16] = [
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const",
    "do", "else", "for", "if", "int", "new",
];

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AndroidHostGenerator;

impl AndroidHostGenerator {
    pub(super) async fn generate(
        application: &NativeApplication,
        index: &NativeRouteIndex,
        companions: &[super::registry::NativeCompanionInput],
        web_bundle: &Path,
        stage: &Path,
        package_artifact: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle = stage.join(&application.executable_name);
        let project = bundle.join("project");
        let assets = project.join("app/src/main/assets");
        stage_application(index, web_bundle, stage, &assets)?;

        let package = java_package(&application.application_id);
        let package_path = package.replace('.', "/");
        write(
            &project.join("settings.gradle.kts"),
            settings_gradle(application).as_bytes(),
        )?;
        write(
            &project.join("gradle.properties"),
            b"org.gradle.jvmargs=-Xmx2g\nandroid.useAndroidX=true\nandroid.nonTransitiveRClass=true\n",
        )?;
        // A Kotlin companion is compiled *as Kotlin* into this APK rather than
        // to WebAssembly, which is the whole point: it reaches the Android SDK
        // because it was built for this target.
        let has_companion = stage_companion(companions, &project, &package, &package_path)?;
        write(
            &project.join("app/build.gradle.kts"),
            app_gradle(application, &package, has_companion).as_bytes(),
        )?;
        write(
            &project.join("app/src/main/AndroidManifest.xml"),
            android_manifest(application).as_bytes(),
        )?;
        stage_icon(application, web_bundle, &project)?;
        write(
            &project.join("app/src/main/res/values/strings.xml"),
            strings_xml(application).as_bytes(),
        )?;
        write_host_source(
            &project.join(format!(
                "app/src/main/java/{package_path}/MainActivity.java"
            )),
            &java_source(application, &package, has_companion, index),
        )?;

        if !package_artifact {
            return Ok(GeneratedHost {
                application_bundle: PathBuf::from(&application.executable_name).join("project"),
                toolchain_name: String::from("source"),
                toolchain_version: String::from("not-packaged"),
            });
        }

        let (gradle_version, apk) = assemble(&project, &bundle, application).await?;
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(&application.executable_name).join(apk),
            toolchain_name: String::from("gradle"),
            toolchain_version: gradle_version,
        })
    }
}

/// The prelude appended to a Kotlin companion compiled into this host.
const KOTLIN_COMPANION_PRELUDE: &str = include_str!("prelude.kt");

/// The Kotlin companion prelude, for the publish-channel drift test.
#[cfg(test)]
pub(super) const fn companion_prelude() -> &'static str {
    KOTLIN_COMPANION_PRELUDE
}

/// Stages the entry route's native Kotlin companion into the generated project.
///
/// Returns whether one was staged; every project that has not asked for a
/// companion gets neither the file nor the Kotlin plugin that compiles it.
fn stage_companion(
    companions: &[super::registry::NativeCompanionInput],
    project: &Path,
    package: &str,
    package_path: &str,
) -> Result<bool, Failure> {
    let Some(authored) =
        super::registry::source(companions, crate::project::NativeCompanion::Kotlin)?
    else {
        return Ok(false);
    };
    // The package is written here rather than by the author: the host reaches
    // the companion by name, and a file in the default package is unreachable
    // from one that is not.
    write(
        &project.join(format!(
            "app/src/main/kotlin/{package_path}/TachyonCompanion.kt"
        )),
        format!("package {package}\n\n{authored}\n{KOTLIN_COMPANION_PRELUDE}").as_bytes(),
    )?;
    Ok(true)
}

/// Assembles the debug APK and copies it beside the generated project.
async fn assemble(
    project: &Path,
    bundle: &Path,
    application: &NativeApplication,
) -> Result<(String, String), Failure> {
    if std::env::var_os("ANDROID_HOME").is_none() && std::env::var_os("ANDROID_SDK_ROOT").is_none()
    {
        return Err(native_tool_failure(
            1605,
            "The Android host requires ANDROID_HOME or ANDROID_SDK_ROOT on the build machine.",
        ));
    }
    let version = first_line(
        &run_tool_in("gradle", &["--version"], Some(project)).await?,
        "Gradle unknown",
    );
    run_tool_in(
        "gradle",
        &["--no-daemon", "--console=plain", "assembleDebug"],
        Some(project),
    )
    .await?;
    let built = project.join("app/build/outputs/apk/debug/app-debug.apk");
    let name = format!("{}.apk", application.executable_name);
    let published = bundle.join(&name);
    native_io(fs::copy(&built, &published), &published)?;
    Ok((version, name))
}

/// Returns a valid Java package derived from a reverse-DNS application id.
fn java_package(application_id: &str) -> String {
    application_id
        .split('.')
        .map(|segment| {
            let sanitized = segment
                .chars()
                .map(|value| {
                    if value.is_ascii_alphanumeric() {
                        value
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            if sanitized.is_empty()
                || sanitized.starts_with(|value: char| value.is_ascii_digit())
                || JAVA_KEYWORDS.contains(&sanitized.as_str())
            {
                format!("_{sanitized}")
            } else {
                sanitized
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn settings_gradle(application: &NativeApplication) -> String {
    format!(
        r#"pluginManagement {{
  repositories {{
    google()
    mavenCentral()
    gradlePluginPortal()
  }}
}}
dependencyResolutionManagement {{
  repositories {{
    google()
    mavenCentral()
  }}
}}
rootProject.name = "{name}"
include(":app")
"#,
        name = application.executable_name,
    )
}

fn app_gradle(application: &NativeApplication, package: &str, has_companion: bool) -> String {
    // The Kotlin plugin is added only for a project that has a companion: it
    // is what compiles one, and a project without one should not pay for a
    // plugin, a standard library and a slower build it never uses.
    let kotlin_plugin = if has_companion {
        format!("\n  id(\"org.jetbrains.kotlin.android\") version \"{KOTLIN_PLUGIN}\"")
    } else {
        String::new()
    };
    let kotlin_options = if has_companion {
        "\n  kotlinOptions {\n    jvmTarget = \"17\"\n  }\n"
    } else {
        ""
    };
    format!(
        r#"plugins {{
  id("com.android.application") version "{plugin}"{kotlin_plugin}
}}

android {{
  namespace = "{package}"
  compileSdk = {compile_sdk}
{kotlin_options}

  defaultConfig {{
    applicationId = "{package}"
    minSdk = {min_sdk}
    targetSdk = {compile_sdk}
    versionCode = 1
    versionName = "{version}"
  }}

  buildTypes {{
    getByName("debug") {{
      isMinifyEnabled = false
    }}
  }}

  compileOptions {{
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
  }}

  packaging {{
    resources.excludes.add("META-INF/*")
  }}

  androidResources {{
    // These assets are the compiler's owned bundle. AAPT's default <dir>_*
    // rule would silently omit Tachyon's dynamic route directories.
    ignoreAssetsPattern = "!.svn:!.git:!.ds_store:!*.scc:!CVS:!thumbs.db:!picasa.ini:!*~"
  }}
}}

dependencies {{
  implementation("androidx.webkit:webkit:{webkit}")
}}
"#,
        plugin = ANDROID_GRADLE_PLUGIN,
        webkit = ANDROIDX_WEBKIT,
        package = package,
        compile_sdk = COMPILE_SDK,
        min_sdk = MIN_SDK,
        version = application.version,
    )
}

fn android_manifest(application: &NativeApplication) -> String {
    // Declared only when there is one: naming a drawable that is not in the
    // APK fails the resource link, which is a build error rather than a
    // missing picture.
    let icon = application.largest_icon().map_or_else(String::new, |_| {
        String::from("\n      android:icon=\"@mipmap/ic_launcher\"")
    });
    r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
  <application
      android:label="@string/app_name"__ICON__
      android:allowBackup="false"
      android:usesCleartextTraffic="false"
      android:theme="@android:style/Theme.Material.Light.NoActionBar">
    <activity
        android:name=".MainActivity"
        android:exported="true"
        android:label="@string/app_name">
      <intent-filter>
        <action android:name="android.intent.action.MAIN"/>
        <category android:name="android.intent.category.LAUNCHER"/>
      </intent-filter>
    </activity>
  </application>
</manifest>
"#
    .replace("__ICON__", &icon)
}

/// Copies the manifest's icon into the density bucket Android reads.
///
/// One bucket, not five: the source is a single square image, and Android
/// scales a launcher icon it finds in the largest bucket down for smaller
/// screens. Generating five copies of one image would be five copies of one
/// image.
fn stage_icon(
    application: &NativeApplication,
    web_bundle: &Path,
    project: &Path,
) -> Result<(), Failure> {
    let Some(source) = application.largest_icon() else {
        return Ok(());
    };
    let origin = web_bundle.join(source.trim_start_matches('/'));
    if !origin.is_file() {
        return Ok(());
    }
    let destination = project.join("app/src/main/res/mipmap-xxxhdpi/ic_launcher.png");
    if let Some(parent) = destination.parent() {
        native_io(fs::create_dir_all(parent), parent)?;
    }
    native_io(fs::copy(&origin, &destination).map(|_| ()), &destination)
}

fn strings_xml(application: &NativeApplication) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <resources>\n  <string name=\"app_name\">{name}</string>\n</resources>\n",
        name = xml_escape(&application.name),
    )
}

fn java_source(
    application: &NativeApplication,
    package: &str,
    has_companion: bool,
    index: &NativeRouteIndex,
) -> String {
    JAVA_HOST
        .replace("__ENTRY_DOCUMENT__", &quoted_string_escape(&index.entry_document))
        // Written into a <script> as the document is served, so it is a Java
        // string literal that has to survive both Java and HTML.
        .replace("__NATIVE_SHIM__", &quoted_string_escape(&super::host::native_shim(&application.window)))
        .replace("__ANDROID_BRIDGE_SHIM__", &quoted_string_escape(ANDROID_BRIDGE_SHIM))
        .replace("__PACKAGE__", package)
        .replace(
            "__BUNDLE_ID__",
            &quoted_string_escape(&application.application_id),
        )
        .replace("__APP_NAME__", &quoted_string_escape(&application.name))
        .replace("__ROUTE_DOCUMENTS__", &quoted_string_escape(&super::routes::route_documents_json(index)))
        // The call is written in only when a companion was staged: naming a
        // class that is not in the APK is a compile error, not a runtime one.
        .replace(
            "__TACHYON_COMPANION_CALL__",
            if has_companion {
                "TachyonCompanionKt.tacNativeInvoke(request)"
            } else {
                "emptyCompanion(request)"
            },
        )
        // Same gate: TacBridge comes from the companion's prelude, so without
        // one there is no class to name.
        .replace(
            "__TACHYON_COMPANION_EMIT__",
            if has_companion {
                "TacStore.setPreferences(getSharedPreferences(\"tachyon\", MODE_PRIVATE)); TacBridge.setEmit(this::relayPublish);"
            } else {
                "// No companion, so nothing publishes."
            },
        )
}

/// The generated host source, for the cross-host drift tests.
///
/// The dispatch arms live in this string rather than in Rust, so reading it
/// is the only way to assert what this host does and does not implement.
#[cfg(test)]
pub(super) const fn host_source() -> &'static str {
    JAVA_HOST
}

// Reply messages are bound to the sending document by AndroidX. The local map
// also bounds abandoned requests and is retired when the document leaves.
const ANDROID_BRIDGE_SHIM: &str = r"
;(() => {
  const host = globalThis.__tachyonAndroidHost;
  const pending = new Map();
  let nextId = 0;
  let closed = false;
  if (host) host.onmessage = event => {
    let reply;
    try { reply = JSON.parse(event.data); } catch { return; }
    const entry = pending.get(reply.id);
    if (!entry || typeof reply.result !== 'string') return;
    pending.delete(reply.id);
    clearTimeout(entry.timer);
    entry.resolve(reply.result);
  };
  globalThis.__tachyonHostPost = (capability, payload) => new Promise((resolve, reject) => {
    if (closed || globalThis.top !== globalThis || !host) {
      reject(new Error('Android native bridge is unavailable for this document.')); return;
    }
    if (typeof capability !== 'string' || capability.length > 64 || typeof payload !== 'string'
        || payload.length > 65536 || new TextEncoder().encode(payload).length > 65536) {
      reject(new Error('Invalid or oversized host call.')); return;
    }
    if (pending.size >= 128) { reject(new Error('Too many pending native calls.')); return; }
    const id = String(++nextId);
    const timer = setTimeout(() => {
      pending.delete(id); reject(new Error('Android native call timed out.'));
    }, 10000);
    pending.set(id, { resolve, reject, timer });
    try { host.postMessage(JSON.stringify({ id, capability, payload })); }
    catch (error) { clearTimeout(timer); pending.delete(id); reject(error); }
  });
  addEventListener('pagehide', () => {
    closed = true;
    for (const entry of pending.values()) {
      clearTimeout(entry.timer); entry.reject(new Error('Native document navigated.'));
    }
    pending.clear();
  });
  addEventListener('pageshow', () => { closed = false; });
})();
";

const JAVA_HOST: &str = r#"package __PACKAGE__;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.graphics.Insets;
import android.view.ViewGroup;
import android.view.WindowInsets;
import android.widget.FrameLayout;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import androidx.webkit.WebMessageCompat;
import androidx.webkit.JavaScriptReplyProxy;
import androidx.webkit.WebViewCompat;
import androidx.webkit.WebViewFeature;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/**
 * A full-screen WebView over the application's own bundle.
 *
 * This used to build Android views from a lowered Native UI tree. See
 * native/routes.rs for why it does not any more: on a real design almost
 * nothing had an adapter, and what did looked nothing like the rest.
 */
public final class MainActivity extends Activity {
  private static final String ASSET_ORIGIN = "https://appassets.tachyon.local/";
  private static final String ENTRY_DOCUMENT = "__ENTRY_DOCUMENT__";
  private static final String NATIVE_SHIM = "__NATIVE_SHIM__";
  private static final String ANDROID_BRIDGE_SHIM = "__ANDROID_BRIDGE_SHIM__";
  private static final String ROUTE_DOCUMENTS = "__ROUTE_DOCUMENTS__";

  private WebView view;
  private volatile String route = "";
  private volatile long documentGeneration;
  private final java.util.concurrent.ThreadPoolExecutor bridgeWorker =
      new java.util.concurrent.ThreadPoolExecutor(1, 1, 0L, java.util.concurrent.TimeUnit.MILLISECONDS,
          new java.util.concurrent.ArrayBlockingQueue<Runnable>(128));

  /** Called only after the WebView has authenticated the actual sending frame. */
  private final class HostBridge {
    public String call(String capability, String payload, String ownerRoute) {
      if ("companion.invoke".equals(capability)) {
        try {
          if (!ownerRoute.equals(new org.json.JSONObject(payload).optString("route"))) {
            return "{\"error\":\"Companion route does not belong to this page.\"}";
          }
        } catch (org.json.JSONException error) {
          return "{\"error\":\"Malformed companion request.\"}";
        }
        // Recorded because it is the one event that proves the whole path: the
        // bundle loaded, its modules ran, the bridge answered, and the
        // companion compiled into this APK was reached.
        record("companion.invoked");
        // Compiled into this APK, so this is a direct Kotlin call.
        return companionInvoke(payload);
      }
      if ("route.open".equals(capability)) {
        record("route.opened");
        return "{\"ok\":true,\"value\":{}}";
      }
      // Naming both halves: which capability, and which platform declined it.
      return "{\"ok\":false,\"error\":\"android host does not implement capability '"
          + safeName(capability) + "'\"}";
    }
  }

  private boolean currentDocument(WebView web, long generation, String ownerRoute) {
    String url = web.getUrl();
    if (url == null || generation != documentGeneration || ownerRoute.isEmpty()) return false;
    Uri current = Uri.parse(url);
    return isAssetUrl(current) && ownerRoute.equals(route)
        && ownerRoute.equals(canonicalRoute(current.getPath()));
  }

  private void installHostBridge() {
    // No legacy fallback: its callbacks cannot authenticate the sending frame.
    if (!WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) return;
    WebViewCompat.addWebMessageListener(view, "__tachyonAndroidHost",
        java.util.Collections.singleton("https://appassets.tachyon.local"),
        this::receiveHostMessage);
  }

  private void receiveHostMessage(WebView web, WebMessageCompat message, Uri sourceOrigin,
      boolean isMainFrame, JavaScriptReplyProxy replyProxy) {
    if (!isMainFrame || !isAssetUrl(sourceOrigin)) return;
    final long generation = documentGeneration;
    final String ownerRoute = route;
    if (!currentDocument(web, generation, ownerRoute)
        || message.getType() != WebMessageCompat.TYPE_STRING) return;
    try {
      BridgeRequest request = BridgeRequest.parse(message.getData());
      final long deadline = android.os.SystemClock.uptimeMillis() + 10000;
      bridgeWorker.execute(() -> {
        if (generation != documentGeneration || !ownerRoute.equals(route)
            || android.os.SystemClock.uptimeMillis() > deadline) return;
        String result = invokeBounded(request, ownerRoute);
        String reply = "{\"id\":" + org.json.JSONObject.quote(request.id)
            + ",\"result\":" + org.json.JSONObject.quote(result) + "}";
        web.post(() -> {
          if (currentDocument(web, generation, ownerRoute)
              && android.os.SystemClock.uptimeMillis() <= deadline) replyProxy.postMessage(reply);
        });
      });
    } catch (org.json.JSONException | java.util.concurrent.RejectedExecutionException ignored) {
      // Malformed or overloaded requests cannot invoke a capability.
    }
  }

  private String invokeBounded(BridgeRequest request, String ownerRoute) {
    try {
      String result = new HostBridge().call(request.capability, request.payload, ownerRoute);
      return withinByteLimit(result, 65536) ? result
          : "{\"error\":\"Native result exceeds the size limit.\"}";
    } catch (RuntimeException error) {
      return "{\"error\":\"Native host call failed.\"}";
    }
  }

  private static final class BridgeRequest {
    final String id;
    final String capability;
    final String payload;

    private BridgeRequest(String id, String capability, String payload) {
      this.id = id;
      this.capability = capability;
      this.payload = payload;
    }

    static BridgeRequest parse(String data) throws org.json.JSONException {
      if (!boundedJson(data, 131072)) throw new org.json.JSONException("Invalid envelope budget.");
      org.json.JSONObject request = new org.json.JSONObject(data);
      Object id = request.get("id");
      Object capability = request.get("capability");
      Object payload = request.get("payload");
      if (!(id instanceof String) || !((String) id).matches("[1-9][0-9]{0,15}")
          || !(capability instanceof String) || ((String) capability).length() > 64
          || !(payload instanceof String) || !boundedJson((String) payload, 65536)) {
        throw new org.json.JSONException("Invalid request budget or fields.");
      }
      return new BridgeRequest((String) id, (String) capability, (String) payload);
    }
  }

  private static boolean withinByteLimit(String value, int limit) {
    return value != null && value.length() <= limit
        && value.getBytes(StandardCharsets.UTF_8).length <= limit;
  }

  // JSONObject is recursive. Bound structural depth before either untrusted
  // parse, while ignoring brackets and escaped quotes inside JSON strings.
  private static boolean boundedJson(String value, int byteLimit) {
    if (!withinByteLimit(value, byteLimit)) return false;
    int depth = 0;
    boolean quoted = false;
    boolean escaped = false;
    for (int index = 0; index < value.length(); index++) {
      char character = value.charAt(index);
      if (quoted) {
        if (escaped) escaped = false;
        else if (character == '\\') escaped = true;
        else if (character == '"') quoted = false;
        continue;
      }
      if (character == '"') {
        if (!jsonStringStartsHere(value, index)) return false;
        quoted = true;
        continue;
      }
      // Android's parser accepts single quotes and comments; allowing those
      // would let its string boundaries disagree with this JSON-only scanner.
      if (nonJsonDelimiter(character)) return false;
      depth += structuralDepthChange(character);
      if (depth < 0 || depth > 64) return false;
    }
    return depth == 0 && !quoted;
  }

  private static boolean nonJsonDelimiter(char character) {
    return character == '\'' || character == '/' || character == '#';
  }

  private static boolean jsonStringStartsHere(String value, int index) {
    // JSONTokener permits quotes embedded in unquoted literals. Those are not
    // string delimiters to it, so they must not change this scanner's state.
    for (int previous = index - 1; previous >= 0; previous--) {
      char character = value.charAt(previous);
      if (" \t\r\n".indexOf(character) >= 0) continue;
      return "[{,:".indexOf(character) >= 0;
    }
    return true;
  }

  private static int structuralDepthChange(char character) {
    switch (character) {
      case '{': case '[': return 1;
      case '}': case ']': return -1;
      default: return 0;
    }
  }

  private static synchronized String companionInvoke(String request) {
    return __TACHYON_COMPANION_CALL__;
  }

  private static String emptyCompanion(String request) {
    try {
      if ("init".equals(new org.json.JSONObject(request).optString("op")))
        return "{\"value\":{\"fields\":[],\"methods\":[]}}";
    } catch (org.json.JSONException ignored) {
      return "{\"error\":\"Malformed companion request.\"}";
    }
    return "{\"error\":\"This application has no native companion.\"}";
  }

  /**
   * Hands the companion its sink for tacPublish.
   *
   * The other direction of the bridge: everything else here is the page asking
   * a question, and a companion watching the platform — connectivity, battery,
   * a sensor — has no question to answer because nobody asked one.
   *
   * Called from onCreate, so a rotation replaces the sink rather than adding
   * one. The lambda holds this Activity, which is why it is replaced rather
   * than accumulated.
   */
  private void installCompanionEmit() {
    __TACHYON_COMPANION_EMIT__
  }

  /**
   * Relays one publish into the page, on the thread the WebView lives on.
   *
   * A companion may publish from any thread it likes; a WebView may only be
   * touched from the UI one. The payload is the companion's own JSON object,
   * and JSON is a JavaScript expression already.
   */
  private void relayPublish(String payload) {
    WebView web = view;
    if (web == null) {
      return;
    }
    web.post(
        () -> web.evaluateJavascript("globalThis.__tachyonCompanionPublish(" + payload + ")", null));
  }

  private static String safeName(String value) {
    StringBuilder allowed = new StringBuilder();
    for (int index = 0; index < value.length() && index < 64; index += 1) {
      char character = value.charAt(index);
      if (Character.isLetterOrDigit(character) || character == '.' || character == '_'
          || character == '-') {
        allowed.append(character);
      }
    }
    return allowed.length() == 0 ? "unnamed" : allowed.toString();
  }

  @Override
  protected void onCreate(Bundle state) {
    super.onCreate(state);
    record("controller.created");
    view = new WebView(this);
    // Local qualification can inspect debug APKs; distributed release builds
    // never enable the WebView debugger.
    if ((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0) {
      WebView.setWebContentsDebuggingEnabled(true);
    }
    WebSettings settings = view.getSettings();
    settings.setJavaScriptEnabled(true);
    settings.setCacheMode(WebSettings.LOAD_NO_CACHE);
    settings.setDomStorageEnabled(true);
    settings.setAllowFileAccess(false);
    settings.setAllowContentAccess(false);
    settings.setGeolocationEnabled(false);
    view.setLayoutParams(
        new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
    installHostBridge();
    // Installed before the page loads, so a companion that publishes the
    // moment it wakes up has somewhere to publish to. The shim queues until
    // the page's modules take over, so nothing is lost either way.
    installCompanionEmit();
    view.setWebViewClient(
        new WebViewClient() {
          @Override
          public void onPageStarted(WebView web, String url, android.graphics.Bitmap favicon) {
            documentGeneration++;
            bridgeWorker.getQueue().clear();
            Uri target = Uri.parse(url);
            String canonical = isAssetUrl(target) ? canonicalRoute(target.getPath()) : null;
            route = canonical == null ? "" : canonical;
          }

          @Override
          public void onPageFinished(WebView web, String url) {
            Uri target = Uri.parse(url);
            if (!isAssetUrl(target) || canonicalRoute(target.getPath()) == null) return;
            record("controller.mounted");
            record("controller.active");
          }

          @Override
          public boolean shouldOverrideUrlLoading(WebView web, WebResourceRequest request) {
            Uri target = request.getUrl();
            if (isAssetUrl(target)) return false;
            openExternal(target);
            return true;
          }

          @Override
          public WebResourceResponse shouldInterceptRequest(
              WebView web, WebResourceRequest request) {
            return assetResponse(request.getUrl());
          }
        });
    // Android 15 draws every app targeting SDK 35 edge to edge, so without
    // this the page renders underneath the status and navigation bars and its
    // own header collides with the clock. The page is a document, not a game:
    // it wants the space it can actually use.
    //
    // The padding goes on a container rather than on the WebView, because a
    // WebView lays its page out over its own padding and the insets had no
    // visible effect at all.
    FrameLayout root = new FrameLayout(this);
    root.addView(view);
    setContentView(root);
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
      root.setOnApplyWindowInsetsListener(
          (padded, insets) -> {
            Insets bars =
                insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            padded.setPadding(bars.left, bars.top, bars.right, bars.bottom);
            return insets;
          });
      // The first dispatch happens before a listener set here could see it.
      root.requestApplyInsets();
    }
    // A unique host-owned bootstrap retires legacy workers before loading assets.
    view.loadUrl(ASSET_ORIGIN + "__tachyon_bootstrap__.html?launch=" + System.nanoTime());
    record("bridge.ready");
  }

  @Override
  protected void onResume() {
    super.onResume();
    record("controller.active");
  }

  @Override
  protected void onPause() {
    super.onPause();
    record("controller.suspended");
  }

  @Override
  protected void onDestroy() {
    documentGeneration++;
    route = "";
    bridgeWorker.shutdownNow();
    super.onDestroy();
    record("controller.destroyed");
  }

  private static boolean isAssetUrl(Uri target) {
    return "https".equals(target.getScheme())
        && "appassets.tachyon.local".equals(target.getHost())
        && target.getUserInfo() == null && target.getPort() == -1;
  }

  private static String bootstrapDocument() {
    String destination = org.json.JSONObject.quote(ASSET_ORIGIN + ENTRY_DOCUMENT);
    return "<!doctype html><meta charset=utf-8><title>Starting application</title><body><script>"
        + "(async()=>{try{const retire=async()=>{if(navigator.serviceWorker){const regs=await navigator.serviceWorker.getRegistrations();await Promise.all(regs.map(r=>r.unregister()));}"
        + "if(globalThis.caches){const keys=await caches.keys();await Promise.all(keys.filter(k=>k.startsWith('tachyon-static-')).map(k=>caches.delete(k)));}};"
        + "await Promise.race([retire(),new Promise((_,reject)=>setTimeout(()=>reject(new Error('timeout')),5000))]);location.replace("
        + destination + ");}catch(error){document.body.textContent='Unable to prepare the packaged application: '+String(error.message).slice(0,128);}})();</script>";
  }

  private WebResourceResponse assetResponse(Uri target) {
    if (!isAssetUrl(target)) {
      return emptyAssetResponse();
    }
    String path = target.getPath();
    if (path == null || path.contains("..") || path.contains("\\") || path.contains("//")) {
      return emptyAssetResponse();
    }
    if (path.equals("/__tachyon_bootstrap__.html")) {
      return new WebResourceResponse("text/html", "UTF-8", 200, "OK",
          java.util.Collections.singletonMap("Cache-Control", "no-store"),
          new ByteArrayInputStream(bootstrapDocument().getBytes(StandardCharsets.UTF_8)));
    }
    String document = routeDocument(path);
    String name = document != null ? document : path.substring(1);
    // Android's asset packager drops dot-prefixed directories, so the generated
    // runtime is staged under a visible alias. The bundle still asks for it by
    // its authored path.
    if (name.startsWith(".tachyon/")) {
      name = "tachyon-runtime/" + name.substring(".tachyon/".length());
    }
    try {
      return new WebResourceResponse(assetMimeType(name), "UTF-8", open(name));
    } catch (Exception error) {
      try {
        String bundled = "WebBundle/" + name;
        return new WebResourceResponse(assetMimeType(bundled), "UTF-8", open(bundled));
      } catch (Exception ignored) {
        return emptyAssetResponse();
      }
    }
  }

  private static String routeDocument(String path) {
    String canonical = canonicalRoute(path);
    if (canonical == null) {
      for (int slash = path.lastIndexOf('/'); slash > 0; slash = path.lastIndexOf('/', slash - 1)) {
        String route = canonicalRoute(path.substring(0, slash));
        if (route == null || !route.contains("/_")) continue;
        try {
          String document = new org.json.JSONObject(ROUTE_DOCUMENTS).getString(route);
          return document.substring(0, document.lastIndexOf('/') + 1) + path.substring(slash + 1);
        } catch (org.json.JSONException ignored) { return null; }
      }
      return null;
    }
    try { return new org.json.JSONObject(ROUTE_DOCUMENTS).getString(canonical); }
    catch (org.json.JSONException ignored) { return null; }
  }

  private static String canonicalRoute(String path) {
    if (path == null || path.length() > 2048 || path.contains("\\") || path.contains("..")) return null;
    try {
      org.json.JSONObject routes = new org.json.JSONObject(ROUTE_DOCUMENTS);
      if (routes.has(path)) return path;
      String[] parts = path.replaceAll("^/+|/+$", "").split("/");
      java.util.Iterator<String> keys = routes.keys();
      java.util.ArrayList<String> patterns = new java.util.ArrayList<>();
      while (keys.hasNext()) {
        String route = keys.next();
        if (("/" + routes.getString(route)).equals(path)) return route;
        patterns.add(route);
      }
      java.util.Collections.sort(patterns);
      String selected = null;
      int specificity = -1;
      for (String route : patterns) {
        String[] expected = route.replaceAll("^/+|/+$", "").split("/");
        if (parts.length != expected.length) continue;
        boolean matches = true;
        int exact = 0;
        for (int index = 0; index < parts.length; index++) {
          if (!expected[index].startsWith("_") && !expected[index].equals(parts[index])) matches = false;
          if (!expected[index].startsWith("_")) exact++;
        }
        if (matches && exact > specificity) { selected = route; specificity = exact; }
      }
      return selected;
    } catch (org.json.JSONException ignored) {
      return null;
    }
  }

  /**
   * Opens one staged asset, injecting the host shim into any document.
   *
   * The shim has to be in place before the bundle's own scripts run, and a
   * rewriting the document as it is served avoids requiring the optional
   * document-start-script feature in addition to the message listener.
   */
  private InputStream open(String name) throws Exception {
    InputStream stream = getAssets().open(name);
    if (!name.endsWith(".html")) {
      return stream;
    }
    ByteArrayOutputStream buffer = new ByteArrayOutputStream();
    byte[] chunk = new byte[8192];
    int read;
    while ((read = stream.read(chunk)) != -1) {
      if (buffer.size() + read > 16777216) {
        stream.close();
        throw new java.io.IOException("Native document exceeds the 16 MiB limit.");
      }
      buffer.write(chunk, 0, read);
    }
    stream.close();
    String document = new String(buffer.toByteArray(), StandardCharsets.UTF_8);
    // The Android half returns promises, preserving the shared asynchronous API.
    String script =
        "<script>"
            + NATIVE_SHIM
            + ANDROID_BRIDGE_SHIM
            + "</script>";
    int head = document.indexOf("<head>");
    document =
        head >= 0
            ? document.substring(0, head + 6) + script + document.substring(head + 6)
            : script + document;
    return new ByteArrayInputStream(document.getBytes(StandardCharsets.UTF_8));
  }

  private void openExternal(Uri target) {
    String scheme = target.getScheme();
    if (!"https".equals(scheme) && !"http".equals(scheme)) return;
    try {
      startActivity(new Intent(Intent.ACTION_VIEW, target));
    } catch (Exception ignored) {
      // A device without a registered browser keeps the application open.
    }
  }

  private static WebResourceResponse emptyAssetResponse() {
    return new WebResourceResponse(
        "text/plain", "UTF-8", new ByteArrayInputStream(new byte[0]));
  }

  private static String assetMimeType(String path) {
    if (path.endsWith(".html")) return "text/html";
    if (path.endsWith(".css")) return "text/css";
    if (path.endsWith(".js") || path.endsWith(".mjs")) return "text/javascript";
    if (path.endsWith(".json")) return "application/json";
    if (path.endsWith(".svg")) return "image/svg+xml";
    if (path.endsWith(".png")) return "image/png";
    if (path.endsWith(".webp")) return "image/webp";
    if (path.endsWith(".woff2")) return "font/woff2";
    if (path.endsWith(".wasm")) return "application/wasm";
    return "application/octet-stream";
  }

  private void record(String event) {
    try {
      File directory = new File(getFilesDir(), "tachyon");
      directory.mkdirs();
      File log = new File(directory, "__BUNDLE_ID__.jsonl");
      String entry = "{\"event\":\"" + event + "\",\"route\":\"" + route + "\"}\n";
      try (FileOutputStream stream = new FileOutputStream(log, true)) {
        stream.write(entry.getBytes(StandardCharsets.UTF_8));
      }
    } catch (Exception ignored) {
      // Telemetry is evidence, never a reason to fail a launch.
    }
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::{
        ANDROID_GRADLE_PLUGIN, ANDROIDX_WEBKIT, MIN_SDK, android_manifest, app_gradle,
        java_package, java_source,
    };
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
    fn the_host_is_a_web_view_over_the_applications_own_bundle() {
        let source = java_source(&application(), "dev.tachyon.native_catalog", true, &index());
        assert!(source.contains("package dev.tachyon.native_catalog;"));
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains("shouldInterceptRequest"));
        assert!(source.contains("https://appassets.tachyon.local/"));
        assert!(source.contains("TachyonCompanionKt.tacNativeInvoke"));
        assert!(source.contains("WebViewCompat.addWebMessageListener"));
        // Edge-to-edge is forced from SDK 35, so the page has to be told
        // where the system bars are or its header sits under the clock.
        assert!(source.contains("setOnApplyWindowInsetsListener"));
        // Nothing rebuilds the view out of Android widgets any more.
        assert!(!source.contains("buildNode"));
        assert!(!source.contains("LinearLayout"));
    }

    #[test]
    fn a_host_without_a_companion_names_no_companion_symbol() {
        let source = java_source(
            &application(),
            "dev.tachyon.native_catalog",
            false,
            &index(),
        );
        assert!(!source.contains("TachyonCompanionKt"));
        assert!(source.contains("has no native companion"));
    }

    #[test]
    fn android_bridge_authenticates_the_sending_frame_before_dispatch() {
        let source = java_source(&application(), "dev.tachyon.native_catalog", true, &index());
        assert!(!source.contains("addJavascriptInterface"));
        assert!(!source.contains("@JavascriptInterface"));
        assert!(source.contains("WebViewCompat.addWebMessageListener"));
        assert!(source.contains("WebViewFeature.WEB_MESSAGE_LISTENER"));
        assert!(source.contains("!isMainFrame || !isAssetUrl(sourceOrigin)"));
        assert!(
            source.contains("java.util.Collections.singleton(\"https://appassets.tachyon.local\")")
        );
    }

    #[test]
    fn gradle_and_manifest_pin_the_supported_android_surface() {
        let gradle = app_gradle(&application(), "dev.tachyon.native_catalog", false);
        assert!(gradle.contains(ANDROID_GRADLE_PLUGIN));
        assert!(gradle.contains(&format!("androidx.webkit:webkit:{ANDROIDX_WEBKIT}")));
        assert!(gradle.contains(&format!("minSdk = {MIN_SDK}")));
        assert!(gradle.contains("VERSION_17"));
        assert!(gradle.contains("ignoreAssetsPattern ="));
        // The Kotlin plugin is what compiles a companion, so it arrives with
        // one and stays out of every project that has none.
        assert!(app_gradle(&application(), "x.y", true).contains("org.jetbrains.kotlin.android"));
        assert!(!gradle.contains("kotlin"));

        let manifest = android_manifest(&application());
        assert!(manifest.contains("android:usesCleartextTraffic=\"false\""));
        assert!(manifest.contains("android.intent.category.LAUNCHER"));
    }

    #[test]
    fn java_packages_are_valid_for_hyphenated_numeric_and_reserved_segments() {
        assert_eq!(
            java_package("dev.tachyon.native-catalog"),
            "dev.tachyon.native_catalog"
        );
        assert_eq!(java_package("com.1st.app"), "com._1st.app");
        assert_eq!(java_package("com.class.app"), "com._class.app");
    }
}
