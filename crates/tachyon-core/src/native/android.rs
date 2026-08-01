//! Android host generation.
//!
//! The generated project uses platform Android views and the platform
//! `org.json` parser, so it needs neither the Kotlin plugin nor any third-party
//! runtime dependency. Building an APK requires an Android SDK on the build
//! machine; the generator fails closed when one is absent.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, first_line, native_io, native_tool_failure, quoted_string_escape, run_tool_in,
    stage_application, write, write_host_source, xml_escape,
};
use super::planner::{NativeRouteIndex, PlannedNativeRoute};
use crate::Failure;
use std::fs;
use std::path::{Path, PathBuf};

/// Android Gradle Plugin pinned by every generated project.
const ANDROID_GRADLE_PLUGIN: &str = "8.7.3";
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
        routes: &[PlannedNativeRoute],
        index: &NativeRouteIndex,
        web_bundle: &Path,
        stage: &Path,
        package_artifact: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle = stage.join(&application.executable_name);
        let project = bundle.join("project");
        let assets = project.join("app/src/main/assets");
        stage_application(application, routes, index, web_bundle, stage, &assets)?;

        let package = java_package(&application.application_id);
        let package_path = package.replace('.', "/");
        write(
            &project.join("settings.gradle.kts"),
            settings_gradle(application).as_bytes(),
        )?;
        write(
            &project.join("gradle.properties"),
            b"org.gradle.jvmargs=-Xmx2g\nandroid.useAndroidX=false\nandroid.nonTransitiveRClass=true\n",
        )?;
        write(
            &project.join("app/build.gradle.kts"),
            app_gradle(application, &package).as_bytes(),
        )?;
        write(
            &project.join("app/src/main/AndroidManifest.xml"),
            android_manifest(application).as_bytes(),
        )?;
        write(
            &project.join("app/src/main/res/values/strings.xml"),
            strings_xml(application).as_bytes(),
        )?;
        write_host_source(
            &project.join(format!(
                "app/src/main/java/{package_path}/MainActivity.java"
            )),
            &java_source(application, &package),
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

fn app_gradle(application: &NativeApplication, package: &str) -> String {
    format!(
        r#"plugins {{
  id("com.android.application") version "{plugin}"
}}

android {{
  namespace = "{package}"
  compileSdk = {compile_sdk}

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
}}
"#,
        plugin = ANDROID_GRADLE_PLUGIN,
        package = package,
        compile_sdk = COMPILE_SDK,
        min_sdk = MIN_SDK,
        version = application.version,
    )
}

fn android_manifest(_application: &NativeApplication) -> String {
    String::from(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
  <application
      android:label="@string/app_name"
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
"#,
    )
}

fn strings_xml(application: &NativeApplication) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <resources>\n  <string name=\"app_name\">{name}</string>\n</resources>\n",
        name = xml_escape(&application.name),
    )
}

fn java_source(application: &NativeApplication, package: &str) -> String {
    JAVA_HOST
        .replace("__PACKAGE__", package)
        .replace(
            "__BUNDLE_ID__",
            &quoted_string_escape(&application.application_id),
        )
        .replace("__APP_NAME__", &quoted_string_escape(&application.name))
}

const JAVA_HOST: &str = r#"package __PACKAGE__;

import android.app.Activity;
import android.content.Intent;
import android.graphics.Color;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.text.Editable;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.View;
import android.view.ViewGroup;
import android.view.WindowInsetsController;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.ValueCallback;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.ByteArrayInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/** Generated Tachyon Native UI v1 host. */
public final class MainActivity extends Activity {

  private static final String BUNDLE_ID = "__BUNDLE_ID__";
  private static final String ASSET_ORIGIN = "https://appassets.tachyon.local/";
  private static final String HEIGHT_SCRIPT =
      "(() => { const top = document.body.getBoundingClientRect().top; let bottom = top;"
          + " for (const node of document.body.querySelectorAll('*')) {"
          + " const rect = node.getBoundingClientRect();"
          + " if (rect.width || rect.height) bottom = Math.max(bottom, rect.bottom); }"
          + " return Math.ceil(bottom - top); })()";
  private static final int MAX_DEPTH = 64;
  private static final int MAX_STATE_BYTES = 4096;

  private final Map<String, String> state = new HashMap<>();
  private final Map<String, List<TextView>> outputs = new HashMap<>();
  private LinearLayout container;
  private JSONObject index;
  private String route = "/";
  private String lifecycle = "created";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    record("controller.created");

    ScrollView scroller = new ScrollView(this);
    container = new LinearLayout(this);
    container.setOrientation(LinearLayout.VERTICAL);
    container.setPadding(0, statusBarInset(), 0, 0);
    scroller.addView(container);
    setContentView(scroller);
    configureSystemBars();

    try {
      index = new JSONObject(readAsset("NativeIndex.json"));
      openRoute(index.optString("entry_route", "/"));
    } catch (Exception error) {
      showRouteFailure();
    }

    lifecycle = "mounted";
    record("controller.mounted");
  }

  @Override
  protected void onResume() {
    super.onResume();
    lifecycle = "active";
    record("controller.active");
  }

  @Override
  protected void onPause() {
    super.onPause();
    lifecycle = "suspended";
    record("controller.suspended");
  }

  @Override
  protected void onDestroy() {
    super.onDestroy();
    lifecycle = "destroyed";
    record("controller.destroyed");
  }

  private String readAsset(String name) throws Exception {
    try (InputStream stream = getAssets().open(name)) {
      ByteArrayOutputStream buffer = new ByteArrayOutputStream();
      byte[] chunk = new byte[8192];
      int read;
      while ((read = stream.read(chunk)) != -1) {
        buffer.write(chunk, 0, read);
      }
      return buffer.toString(StandardCharsets.UTF_8.name());
    }
  }

  private int statusBarInset() {
    int identifier = getResources().getIdentifier("status_bar_height", "dimen", "android");
    return identifier > 0 ? getResources().getDimensionPixelSize(identifier) : 0;
  }

  private void configureSystemBars() {
    getWindow().setStatusBarColor(Color.WHITE);
    getWindow().setNavigationBarColor(Color.WHITE);
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      WindowInsetsController controller = getWindow().getInsetsController();
      if (controller != null) {
        int appearance = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
            | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
        controller.setSystemBarsAppearance(appearance, appearance);
      }
    } else {
      getWindow().getDecorView().setSystemUiVisibility(
          View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR);
    }
  }

  private void openRoute(String requested) throws Exception {
    if (index == null || requested == null || !requested.startsWith("/")) {
      throw new IllegalStateException("route");
    }
    JSONObject selected = null;
    JSONArray routes = index.optJSONArray("routes");
    for (int position = 0; routes != null && position < routes.length(); position += 1) {
      JSONObject entry = routes.getJSONObject(position);
      if (requested.equals(entry.optString("route"))) {
        selected = entry;
        break;
      }
    }
    if (selected == null) {
      for (int position = 0; routes != null && position < routes.length(); position += 1) {
        JSONObject entry = routes.getJSONObject(position);
        if (routeMatches(entry.optString("route"), requested)) {
          selected = entry;
          break;
        }
      }
    }
    if (selected == null) {
      throw new IllegalStateException("route");
    }

    state.clear();
    outputs.clear();
    String pattern = selected.optString("route");
    JSONObject initial = index.optJSONObject("initial_state");
    if (initial != null && initial.has(pattern)) {
      JSONObject entries = initial.getJSONObject(pattern);
      for (java.util.Iterator<String> keys = entries.keys(); keys.hasNext(); ) {
        String key = keys.next();
        state.put(key, entries.optString(key, ""));
      }
    }
    JSONObject view = new JSONObject(readAsset("NativeUI/" + selected.optString("document")));
    if (view.optInt("contract_version") != 1 || !"android".equals(view.optString("target"))) {
      throw new IllegalStateException("contract");
    }
    container.removeAllViews();
    route = requested;
    View built = buildNode(view.getJSONObject("root"), 0);
    if (built != null) {
      container.addView(built);
    }
    record("route.opened");
  }

  private static boolean routeMatches(String pattern, String candidate) {
    String[] expected = pattern.split("/", -1);
    String[] actual = candidate.split("/", -1);
    if (expected.length != actual.length) return false;
    for (int position = 0; position < expected.length; position += 1) {
      if (expected[position].startsWith("_")) {
        if (actual[position].isEmpty()) return false;
      } else if (!expected[position].equals(actual[position])) {
        return false;
      }
    }
    return true;
  }

  private void showRouteFailure() {
    container.removeAllViews();
    TextView failure = new TextView(this);
    failure.setText("Unable to load native application resources.");
    failure.setTextColor(Color.RED);
    container.addView(failure);
    record("route.failed");
  }

  private static String nodeText(JSONObject node) {
    if ("text".equals(node.optString("kind"))) {
      return node.optString("value", "");
    }
    StringBuilder text = new StringBuilder();
    JSONArray children = node.optJSONArray("children");
    for (int index = 0; children != null && index < children.length(); index += 1) {
      JSONObject child = children.optJSONObject(index);
      if (child != null) {
        text.append(nodeText(child));
      }
    }
    return text.toString();
  }

  private static String property(JSONObject node, String group, String name) {
    JSONObject values = node.optJSONObject(group);
    return values == null ? null : values.optString(name, null);
  }

  private void setState(String binding, String value) {
    String bounded = value.length() > MAX_STATE_BYTES ? value.substring(0, MAX_STATE_BYTES) : value;
    state.put(binding, bounded);
    List<TextView> bound = outputs.get(binding);
    if (bound == null) {
      return;
    }
    for (TextView label : bound) {
      Object tag = label.getTag();
      String prefix = tag instanceof String ? (String) tag : "";
      label.setText(prefix + bounded);
    }
  }

  private void dispatch(String action) {
    if ("destroyed".equals(lifecycle) || action == null) {
      return;
    }
    int separator = action.indexOf(':');
    if (separator < 0) {
      return;
    }
    String verb = action.substring(0, separator);
    String key = action.substring(separator + 1);
    String current = state.get(key);
    if (current == null) {
      return;
    }
    if ("increment".equals(verb)) {
      try {
        setState(key, Long.toString(Long.parseLong(current) + 1));
        record("state.increment");
      } catch (NumberFormatException ignored) {
        // Non-numeric increment state is rejected at compile time.
      }
    } else if ("toggle".equals(verb)) {
      setState(key, "true".equals(current) ? "false" : "true");
      record("state.toggle");
    }
  }

  private LinearLayout column(int spacing) {
    LinearLayout layout = new LinearLayout(this);
    layout.setOrientation(LinearLayout.VERTICAL);
    layout.setDividerPadding(spacing);
    layout.setLayoutParams(
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    return layout;
  }

  private void appendChildren(LinearLayout container, JSONObject node, int depth) {
    JSONArray children = node.optJSONArray("children");
    for (int index = 0; children != null && index < children.length(); index += 1) {
      JSONObject child = children.optJSONObject(index);
      View view = child == null ? null : buildNode(child, depth + 1);
      if (view != null) {
        container.addView(view);
      }
    }
  }

  private View buildNode(JSONObject node, int depth) {
    if (depth > MAX_DEPTH) {
      return null;
    }
    String kind = node.optString("kind");
    if ("text".equals(kind)) {
      String value = node.optString("value", "");
      if (value.trim().isEmpty()) {
        return null;
      }
      TextView label = new TextView(this);
      label.setText(value);
      return label;
    }
    if ("web_surface".equals(kind)) {
      return buildWebSurface(node);
    }

    String adapter = node.optString("adapter", "");
    String identifier = node.optString("id", "");
    String accessibleLabel = property(node, "accessibility", "label");
    String binding = property(node, "properties", "binding");
    String action = property(node, "properties", "action");
    String text = nodeText(node);
    View widget;

    if (adapter.equals("layout.app_bar")) {
      LinearLayout bar = new LinearLayout(this);
      bar.setOrientation(LinearLayout.HORIZONTAL);
      bar.setPadding(32, 32, 32, 32);
      appendChildren(bar, node, depth);
      widget = bar;
    } else if (adapter.startsWith("layout.")) {
      LinearLayout layout = column(24);
      JSONArray children = node.optJSONArray("children");
      boolean containsOnlyWebSurface =
          children != null
              && children.length() == 1
              && "web_surface".equals(children.optJSONObject(0).optString("kind"));
      if ("main".equals(property(node, "accessibility", "role"))
          && !containsOnlyWebSurface) {
        layout.setPadding(0, 24, 0, 24);
      }
      appendChildren(layout, node, depth);
      widget = layout;
    } else if (adapter.startsWith("text.heading")) {
      TextView heading = new TextView(this);
      heading.setText(text);
      heading.setTypeface(Typeface.DEFAULT_BOLD);
      heading.setTextSize(
          TypedValue.COMPLEX_UNIT_SP, adapter.equals("text.heading1") ? 28f : 22f);
      heading.setAccessibilityHeading(true);
      widget = heading;
    } else if (adapter.equals("content.text")) {
      TextView label = new TextView(this);
      label.setText(text);
      widget = label;
    } else if (adapter.equals("control.button")) {
      Button button = new Button(this);
      button.setText(text);
      final String dispatched = action;
      button.setOnClickListener(view -> dispatch(dispatched));
      widget = button;
    } else if (adapter.equals("control.text_field")) {
      EditText field = new EditText(this);
      String placeholder = property(node, "properties", "placeholder");
      if (placeholder != null) {
        field.setHint(placeholder);
      }
      if (binding != null) {
        String initial = state.get(binding);
        if (initial != null) {
          field.setText(initial);
        }
        final String bound = binding;
        field.addTextChangedListener(
            new TextWatcher() {
              @Override
              public void beforeTextChanged(CharSequence s, int start, int count, int after) {}

              @Override
              public void onTextChanged(CharSequence s, int start, int before, int count) {}

              @Override
              public void afterTextChanged(Editable value) {
                state.put(bound, value.toString());
                record("state.input");
              }
            });
      }
      widget = field;
    } else if (adapter.equals("content.output") && binding != null) {
      LinearLayout group = column(8);
      if (accessibleLabel != null) {
        TextView caption = new TextView(this);
        caption.setText(accessibleLabel);
        caption.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f);
        group.addView(caption);
      }
      TextView output = new TextView(this);
      String value = state.get(binding);
      String prefix = property(node, "properties", "prefix");
      prefix = prefix == null ? "" : prefix;
      output.setTag(prefix);
      output.setText(prefix + (value == null ? "" : value));
      output.setTextSize(TypedValue.COMPLEX_UNIT_SP, 22f);
      group.addView(output);
      List<TextView> bound = outputs.get(binding);
      if (bound == null) {
        bound = new ArrayList<>();
        outputs.put(binding, bound);
      }
      bound.add(output);
      widget = group;
    } else if (adapter.equals("control.disclosure")) {
      LinearLayout group = column(8);
      final LinearLayout content = column(8);
      appendChildren(content, node, depth);
      String summary = property(node, "properties", "label");
      Button toggle = new Button(this);
      toggle.setText(summary == null ? "Details" : summary);
      final String key = binding != null ? binding : identifier;
      content.setVisibility("true".equals(state.get(key)) ? View.VISIBLE : View.GONE);
      toggle.setOnClickListener(
          view -> {
            boolean expanded = content.getVisibility() == View.VISIBLE;
            content.setVisibility(expanded ? View.GONE : View.VISIBLE);
            state.put(key, expanded ? "false" : "true");
            record("state.disclosure");
          });
      group.addView(toggle);
      group.addView(content);
      widget = group;
    } else if (adapter.equals("navigation.link")) {
      Button link = new Button(this);
      link.setText(text);
      final String destination = property(node, "properties", "href");
      link.setOnClickListener(
          view -> {
            try {
              openRoute(destination);
            } catch (Exception error) {
              showRouteFailure();
            }
          });
      widget = link;
    } else if (adapter.equals("content.image")) {
      TextView placeholder = new TextView(this);
      placeholder.setText(accessibleLabel == null ? "Image" : accessibleLabel);
      widget = placeholder;
    } else if (adapter.equals("content.divider")) {
      View divider = new View(this);
      divider.setLayoutParams(new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 2));
      divider.setBackgroundColor(Color.GRAY);
      widget = divider;
    } else {
      LinearLayout group = column(8);
      appendChildren(group, node, depth);
      widget = group;
    }

    if (accessibleLabel != null && !accessibleLabel.isEmpty()) {
      widget.setContentDescription(accessibleLabel);
    }
    if (!identifier.isEmpty()) {
      widget.setTag(identifier);
    }
    if ("true".equals(property(node, "properties", "aria-hidden"))) {
      widget.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS);
    }
    return widget;
  }

  private View buildWebSurface(JSONObject node) {
    final String source = node.optString("source", "");
    final String location = node.optString("location", "");
    WebView view = new WebView(this);
    WebSettings settings = view.getSettings();
    settings.setJavaScriptEnabled("local_bundle".equals(source));
    settings.setAllowFileAccess(false);
    settings.setAllowContentAccess(false);
    settings.setDomStorageEnabled("local_bundle".equals(source));
    settings.setGeolocationEnabled(false);
    settings.setCacheMode(WebSettings.LOAD_NO_CACHE);
    // A fallback subtree is as tall as its document. A fixed height clipped
    // whatever rendered past it, and the native host has no scroll of its own
    // to reveal the rest, so the document reports its height once it settles.
    view.setLayoutParams(
        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 1));

    final Uri declared = Uri.parse(location);
    view.setWebViewClient(
        new WebViewClient() {
          @Override
          public void onPageFinished(WebView web, String url) {
            measure(web);
          }

          @Override
          public boolean shouldOverrideUrlLoading(WebView web, WebResourceRequest request) {
            Uri target = request.getUrl();
            if ("local_bundle".equals(source)) {
              if (isAssetUrl(target)) {
                String path = target.getPath();
                if (request.isForMainFrame()
                    && path != null
                    && !path.startsWith("/WebSurfaces/")
                    && !path.startsWith("/WebBundle/")) {
                  runOnUiThread(
                      () -> {
                        try {
                          openRoute(path);
                        } catch (Exception error) {
                          showRouteFailure();
                        }
                      });
                  return true;
                }
                return false;
              }
              openExternal(target);
              return true;
            }
            boolean allowed =
                "https".equals(target.getScheme())
                    && declared.getHost() != null
                    && declared.getHost().equals(target.getHost())
                    && (declared.getPort() == -1 ? 443 : declared.getPort())
                        == (target.getPort() == -1 ? 443 : target.getPort());
            return !allowed;
          }

          @Override
          public WebResourceResponse shouldInterceptRequest(
              WebView web, WebResourceRequest request) {
            if (!"local_bundle".equals(source)) {
              return null;
            }
            return assetResponse(request.getUrl());
          }
        });

    if ("local_bundle".equals(source)) {
      view.loadUrl(ASSET_ORIGIN + location + "?tachyon-route=" + Uri.encode(route));
    } else if (location.startsWith("https://")) {
      view.loadUrl(location);
    }
    String label = property(node, "accessibility", "label");
    if (label != null) {
      view.setContentDescription(label);
    }
    final float[] touchOrigin = new float[2];
    final int touchSlop = android.view.ViewConfiguration.get(this).getScaledTouchSlop();
    view.setOnTouchListener(
        (target, event) -> {
          switch (event.getActionMasked()) {
            case android.view.MotionEvent.ACTION_DOWN:
              touchOrigin[0] = event.getX();
              touchOrigin[1] = event.getY();
              target.getParent().requestDisallowInterceptTouchEvent(true);
              break;
            case android.view.MotionEvent.ACTION_MOVE:
              if (Math.abs(event.getX() - touchOrigin[0]) > touchSlop
                  || Math.abs(event.getY() - touchOrigin[1]) > touchSlop) {
                target.getParent().requestDisallowInterceptTouchEvent(false);
              }
              break;
            case android.view.MotionEvent.ACTION_UP:
              target.getParent().requestDisallowInterceptTouchEvent(false);
              target.postDelayed(() -> measure(view), 100);
              break;
            case android.view.MotionEvent.ACTION_CANCEL:
              target.getParent().requestDisallowInterceptTouchEvent(false);
              break;
            default:
              break;
          }
          return false;
        });
    record("websurface.attached");
    return view;
  }

  private static boolean isAssetUrl(Uri target) {
    return "https".equals(target.getScheme())
        && "appassets.tachyon.local".equals(target.getHost());
  }

  private WebResourceResponse assetResponse(Uri target) {
    if (!isAssetUrl(target)) {
      return emptyAssetResponse();
    }
    String path = target.getPath();
    if (path == null || path.length() < 2 || path.contains("..")) {
      return emptyAssetResponse();
    }
    String name = path.substring(1);
    try {
      return new WebResourceResponse(assetMimeType(name), "UTF-8", getAssets().open(name));
    } catch (Exception error) {
      try {
        String bundled = "WebBundle/" + name;
        return new WebResourceResponse(
            assetMimeType(bundled), "UTF-8", getAssets().open(bundled));
      } catch (Exception ignored) {
        return emptyAssetResponse();
      }
    }
  }

  private void openExternal(Uri target) {
    String scheme = target.getScheme();
    if (!"https".equals(scheme) && !"http".equals(scheme)) return;
    try {
      startActivity(new Intent(Intent.ACTION_VIEW, target));
    } catch (Exception ignored) {
      // A device without a registered browser keeps the native route open.
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
    if (path.endsWith(".wasm")) return "application/wasm";
    return "application/octet-stream";
  }

  // Asks one loaded document how tall it is and gives the view that height.
  private void measure(final WebView view) {
    final float density = view.getResources().getDisplayMetrics().density;
    view.evaluateJavascript(
        HEIGHT_SCRIPT,
        new ValueCallback<String>() {
          @Override
          public void onReceiveValue(String value) {
            int measured;
            try {
              measured = Math.round(Float.parseFloat(value) * density);
            } catch (NumberFormatException error) {
              return;
            }
            if (measured <= 0) {
              return;
            }
            ViewGroup.LayoutParams params = view.getLayoutParams();
            if (params != null && params.height != measured) {
              params.height = measured;
              view.setLayoutParams(params);
            }
          }
        });
  }

  private void record(String event) {
    String allowed = event.length() > 128 ? event.substring(0, 128) : event;
    java.io.File directory = new java.io.File(getFilesDir(), "tachyon");
    if (!directory.exists() && !directory.mkdirs()) {
      return;
    }
    java.io.File log = new java.io.File(directory, BUNDLE_ID + ".jsonl");
    String line = "{\"event\":\"" + allowed + "\",\"route\":\"" + route + "\"}\n";
    try (FileOutputStream stream = new FileOutputStream(log, true)) {
      stream.write(line.getBytes(StandardCharsets.UTF_8));
    } catch (Exception ignored) {
      // Lifecycle logging never fails an application launch.
    }
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::{
        ANDROID_GRADLE_PLUGIN, MIN_SDK, android_manifest, app_gradle, java_package, java_source,
        strings_xml,
    };
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
    fn java_packages_are_valid_for_hyphenated_numeric_and_reserved_segments() {
        assert_eq!(
            java_package("dev.tachyon.native-catalog"),
            "dev.tachyon.native_catalog"
        );
        assert_eq!(java_package("dev.9lives.app"), "dev._9lives.app");
        assert_eq!(java_package("dev.class.app"), "dev._class.app");
    }

    #[test]
    fn generated_host_uses_platform_views_lifecycle_and_no_bridge() {
        let source = java_source(&application(), "dev.tachyon.native_catalog");
        assert!(source.contains("package dev.tachyon.native_catalog;"));
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains("setJavaScriptEnabled(\"local_bundle\".equals(source))"));
        assert!(source.contains("https://appassets.tachyon.local/"));
        assert!(source.contains("shouldInterceptRequest"));
        assert!(source.contains("requestDisallowInterceptTouchEvent(true)"));
        assert!(source.contains("getScaledTouchSlop"));
        assert!(source.contains("APPEARANCE_LIGHT_STATUS_BARS"));
        assert!(source.contains("SYSTEM_UI_FLAG_LIGHT_STATUS_BAR"));
        assert!(source.contains("openRoute(path)"));
        assert!(source.contains("settings.setAllowFileAccess(false)"));
        assert!(source.contains("declared.getPort() == -1 ? 443 : declared.getPort()"));
        assert!(source.contains("target.getPort() == -1 ? 443 : target.getPort()"));
        assert!(source.contains("setContentDescription"));
        assert!(source.contains(r#""android".equals(view.optString("target"))"#));
        assert!(!source.contains("addJavascriptInterface"));
    }

    #[test]
    fn gradle_and_manifest_pin_the_supported_android_surface() {
        let gradle = app_gradle(&application(), "dev.tachyon.native_catalog");
        assert!(gradle.contains(ANDROID_GRADLE_PLUGIN));
        assert!(gradle.contains(&format!("minSdk = {MIN_SDK}")));
        assert!(gradle.contains("VERSION_17"));

        let manifest = android_manifest(&application());
        assert!(manifest.contains("android:usesCleartextTraffic=\"false\""));
        assert!(manifest.contains("Theme.Material.Light.NoActionBar"));
        assert!(manifest.contains("android.intent.category.LAUNCHER"));
        assert!(strings_xml(&application()).contains("Native Catalog"));
    }
}
