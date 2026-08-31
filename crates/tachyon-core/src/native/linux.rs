//! Linux GTK4 host generation.
//!
//! A GTK4 window around one `WebKitGTK` view showing the application's own
//! bundle; see `native/routes.rs` for why it is no longer a tree of widgets.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, c_string_escape, first_line, native_tool_failure, run_tool, stage_application,
    write, write_host_source, xml_escape,
};
use super::routes::NativeRouteIndex;
use crate::Failure;
use std::path::{Path, PathBuf};

/// `pkg-config` modules required by the generated GTK4 host.
/// `JavaScriptCore` ships with the `WebKitGTK` the host already links, so the
/// engine the controller runs in is the platform's rather than one Tachyon
/// ships. See ADR 0017.
const PKG_CONFIG_MODULES: [&str; 5] = [
    "gtk4",
    "webkitgtk-6.0",
    "javascriptcoregtk-6.0",
    "gmodule-2.0",
    "gio-unix-2.0",
];

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LinuxHostGenerator;

impl LinuxHostGenerator {
    pub(super) async fn generate(
        application: &NativeApplication,
        index: &NativeRouteIndex,
        companions: &[super::registry::NativeCompanionInput],
        web_bundle: &Path,
        stage: &Path,
        package: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle = stage.join(&application.executable_name);
        let resources = bundle.join("resources");
        stage_application(index, web_bundle, stage, &resources)?;

        let rust_companion = super::rust::stage(companions, stage, &application.application_id)?;
        let source_path = stage.join("project").join("tachyon_host.c");
        write_host_source(&source_path, &c_source(application, index))?;
        write(
            &bundle.join(format!("{}.desktop", application.application_id)),
            desktop_entry(application).as_bytes(),
        )?;
        stage_icon(application, web_bundle, &bundle)?;
        if !package {
            return Ok(GeneratedHost {
                application_bundle: PathBuf::from("project/tachyon_host.c"),
                toolchain_name: String::from("source"),
                toolchain_version: String::from("not-packaged"),
            });
        }

        let executable = bundle.join("bin").join(&application.executable_name);
        let compiler_version = compile_c(&source_path, &executable).await?;
        // Loaded beside the executable rather than linked, so a GTK host built
        // without a companion needs no rebuild to gain one.
        if let Some(source) = &rust_companion {
            super::rust::compile(
                source,
                super::rust::Linkage::Shared,
                None,
                &bundle.join("bin").join("libtachyoncompanion.so"),
            )
            .await?;
        }
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(&application.executable_name),
            toolchain_name: String::from("cc"),
            toolchain_version: compiler_version,
        })
    }
}

async fn compile_c(source: &Path, executable: &Path) -> Result<String, Failure> {
    if !cfg!(target_os = "linux") {
        return Err(native_tool_failure(
            1605,
            "The Linux host requires a Linux build machine with GTK4 and WebKitGTK.",
        ));
    }
    if let Some(parent) = executable.parent() {
        super::host::native_io(std::fs::create_dir_all(parent), parent)?;
    }
    let version = first_line(&run_tool("cc", &["--version"]).await?, "cc unknown");
    let mut flags = Vec::new();
    for stage in ["--cflags", "--libs"] {
        let mut arguments = vec![stage];
        arguments.extend_from_slice(&PKG_CONFIG_MODULES);
        let output = run_tool("pkg-config", &arguments).await?;
        flags.extend(output.split_whitespace().map(String::from));
    }
    let source = source
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Host source path is not valid Unicode."))?;
    let executable = executable
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, "Application path is not valid Unicode."))?;
    // gnu17 rather than c17: the host uses POSIX `readlink` and the glibc
    // `fopen` close-on-exec mode, and GLib headers assume GNU extensions.
    let mut arguments = vec!["-std=gnu17", "-O2", "-Wall", "-Wextra", "-Werror", source];
    arguments.extend(flags.iter().map(String::as_str));
    arguments.extend_from_slice(&["-o", executable]);
    run_tool("cc", &arguments).await?;
    Ok(version)
}

/// Returns a valid `GApplication` identifier for a reverse-DNS application id.
fn gtk_application_id(application_id: &str) -> String {
    application_id.replace('-', "_")
}

fn desktop_entry(application: &NativeApplication) -> String {
    // A desktop entry takes a path or a themed name; the manifest's icon is
    // staged beside the binary, so the path is what it gets.
    let icon = application
        .largest_icon()
        .map_or_else(String::new, |_| String::from("Icon=share/icon.png\n"));
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Exec=bin/{executable}\n\
         {icon}\
         Terminal=false\n\
         Categories=Utility;\n\
         X-Tachyon-Version={version}\n",
        name = xml_escape(&application.name),
        executable = application.executable_name,
        version = application.version,
    )
}

/// Copies the manifest's icon to the path the desktop entry names.
fn stage_icon(
    application: &NativeApplication,
    web_bundle: &Path,
    bundle: &Path,
) -> Result<(), Failure> {
    let Some(source) = application.largest_icon() else {
        return Ok(());
    };
    let origin = web_bundle.join(source.trim_start_matches('/'));
    if !origin.is_file() {
        return Ok(());
    }
    let destination = bundle.join("share").join("icon.png");
    if let Some(parent) = destination.parent() {
        super::host::native_io(std::fs::create_dir_all(parent), parent)?;
    }
    super::host::native_io(
        std::fs::copy(&origin, &destination).map(|_| ()),
        &destination,
    )
}

fn c_source(application: &NativeApplication, index: &NativeRouteIndex) -> String {
    C_HOST
        .replace(
            "__LOCAL_BUNDLE_HELPERS__",
            &super::routes::c_local_bundle(index, &[], "tachyon://app"),
        )
        .replace("__ENTRY_ROUTE__", &c_string_escape(&index.entry_route))
        .replace(
            "__NATIVE_SHIM__",
            &c_string_escape(&super::host::native_shim(&application.window)),
        )
        .replace("__APP_NAME__", &c_string_escape(&application.name))
        .replace(
            "__BUNDLE_ID__",
            &c_string_escape(&application.application_id),
        )
        .replace(
            "__GTK_APP_ID__",
            &c_string_escape(&gtk_application_id(&application.application_id)),
        )
}

/// The generated host source, for the capability-drift test.
///
/// The dispatch arms live in this string rather than in Rust, so the only way
/// to assert that a host implements what the bundle advertises is to read it.
#[cfg(test)]
pub(super) const fn host_source() -> &'static str {
    C_HOST
}

const C_HOST: &str = r#"#include <gtk/gtk.h>
#include <gio/gio.h>
#include <gmodule.h>
#include <libsoup/soup.h>
#include <jsc/jsc.h>
#include <webkit/webkit.h>

#include <limits.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <gio/gunixinputstream.h>

__LOCAL_BUNDLE_HELPERS__

/* A GTK4 window around one WebKit view showing the application's own bundle.
   This used to build GTK widgets from a lowered Native UI tree; see
   native/routes.rs for why it does not any more. */

#define TACHYON_APP_NAME "__APP_NAME__"
#define TACHYON_BUNDLE_ID "__BUNDLE_ID__"
#define TACHYON_ENTRY_ROUTE "__ENTRY_ROUTE__"

static const gchar *TACHYON_NATIVE_SHIM = "__NATIVE_SHIM__";

static gchar *g_route = NULL;
/* Held for the one thing that has no view to hand: a companion publishing on
   its own schedule rather than answering the page. */
static WebKitWebView *g_view = NULL;
static int g_bundle_fd = -1;
static gint g_pending_publishes = 0;
static void tachyon_record(const gchar *event);

static gboolean tachyon_trusted_view(WebKitWebView *view) {
  char path[TACHYON_PATH_LIMIT];
  return view != NULL && tachyon_local_path(webkit_web_view_get_uri(view), path, sizeof(path)) &&
         tachyon_document_route(path) != NULL;
}

static int tachyon_open_resource(const char *relative) {
  if (g_bundle_fd < 0 || relative[0] == '\0') return -1;
  int current = dup(g_bundle_fd);
  gchar **parts = g_strsplit(relative, "/", -1);
  for (gsize index = 0; parts[index] != NULL && current >= 0; index++) {
    if (parts[index][0] == '\0' || strcmp(parts[index], ".") == 0 || strcmp(parts[index], "..") == 0) { close(current); current = -1; break; }
    int flags = O_RDONLY | O_CLOEXEC | O_NOFOLLOW;
    if (parts[index + 1] != NULL) flags |= O_DIRECTORY;
    int next = openat(current, parts[index], flags);
    close(current);
    current = next;
  }
  g_strfreev(parts);
  return current;
}

static const char *tachyon_content_type(const char *path) {
  const char *extension = strrchr(path, '.');
  if (extension == NULL) return "application/octet-stream";
  if (strcmp(extension, ".html") == 0) return "text/html; charset=utf-8";
  if (strcmp(extension, ".js") == 0 || strcmp(extension, ".mjs") == 0) return "text/javascript; charset=utf-8";
  if (strcmp(extension, ".css") == 0) return "text/css; charset=utf-8";
  if (strcmp(extension, ".json") == 0) return "application/json";
  if (strcmp(extension, ".wasm") == 0) return "application/wasm";
  if (strcmp(extension, ".svg") == 0) return "image/svg+xml";
  if (strcmp(extension, ".png") == 0) return "image/png";
  if (strcmp(extension, ".jpg") == 0 || strcmp(extension, ".jpeg") == 0) return "image/jpeg";
  if (strcmp(extension, ".woff2") == 0) return "font/woff2";
  return "application/octet-stream";
}

static void tachyon_scheme_request(WebKitURISchemeRequest *request, gpointer data) {
  (void)data;
  char path[TACHYON_PATH_LIMIT], relative[TACHYON_PATH_LIMIT];
  int descriptor = -1;
  if (tachyon_local_path(webkit_uri_scheme_request_get_uri(request), path, sizeof(path))) {
    descriptor = tachyon_open_resource(path + 1);
    if (descriptor >= 0) { struct stat info; if (fstat(descriptor, &info) != 0 || !S_ISREG(info.st_mode)) { close(descriptor); descriptor = -1; } }
    if (descriptor >= 0) g_strlcpy(relative, path + 1, sizeof(relative));
    else if (tachyon_bundle_path(path, relative, sizeof(relative))) descriptor = tachyon_open_resource(relative);
  }
  struct stat info;
  if (descriptor < 0 || fstat(descriptor, &info) != 0 || !S_ISREG(info.st_mode) || info.st_size < 0 || info.st_size > 16 * 1024 * 1024) {
    if (descriptor >= 0) close(descriptor);
    GError *error = g_error_new_literal(G_IO_ERROR, G_IO_ERROR_NOT_FOUND, "Local application resource is unavailable.");
    webkit_uri_scheme_request_finish_error(request, error); g_error_free(error); return;
  }
  GInputStream *stream = g_unix_input_stream_new(descriptor, TRUE);
  WebKitURISchemeResponse *response = webkit_uri_scheme_response_new(stream, info.st_size);
  webkit_uri_scheme_response_set_content_type(response, tachyon_content_type(relative));
  SoupMessageHeaders *headers = soup_message_headers_new(SOUP_MESSAGE_HEADERS_RESPONSE);
  soup_message_headers_append(headers, "Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data: https:; connect-src 'self' https:; frame-src 'none'; object-src 'none'; base-uri 'self'");
  soup_message_headers_append(headers, "X-Content-Type-Options", "nosniff");
  webkit_uri_scheme_response_set_http_headers(response, headers);
  webkit_uri_scheme_request_finish_with_response(request, response);
  /* set_http_headers takes ownership of headers. */
  g_object_unref(response); g_object_unref(stream);
}

static gboolean tachyon_decide_policy(WebKitWebView *view, WebKitPolicyDecision *decision, WebKitPolicyDecisionType type, gpointer data) {
  (void)data;
  if (type == WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION) { webkit_policy_decision_ignore(decision); return TRUE; }
  if (type != WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION) return FALSE;
  WebKitNavigationAction *action = webkit_navigation_policy_decision_get_navigation_action(WEBKIT_NAVIGATION_POLICY_DECISION(decision));
  const char *uri = webkit_uri_request_get_uri(webkit_navigation_action_get_request(action));
  char path[TACHYON_PATH_LIMIT];
  if (!tachyon_local_path(uri, path, sizeof(path)) || tachyon_document_route(path) == NULL) {
    webkit_policy_decision_ignore(decision); return TRUE;
  }
  const TachyonLocalRoute *route = tachyon_document_route(path);
  g_free(g_route); g_route = g_strdup(route->route);
  if (path[strlen(path) - 1] != '/' && strcmp(path + 1, route->document) != 0) {
    size_t boundary = strcspn(uri, "?#");
    if (boundary > TACHYON_PATH_LIMIT) { webkit_policy_decision_ignore(decision); return TRUE; }
    gchar *normalized = g_strdup_printf("%.*s/%s", (int)boundary, uri, uri + boundary);
    webkit_policy_decision_ignore(decision);
    webkit_web_view_load_uri(view, normalized); g_free(normalized); return TRUE;
  }
  return FALSE;
}

static gchar *tachyon_resource_root(void) {
  char buffer[PATH_MAX];
  ssize_t length = readlink("/proc/self/exe", buffer, sizeof(buffer) - 1);
  if (length <= 0) {
    return g_strdup("resources");
  }
  buffer[length] = '\0';
  gchar *bin_directory = g_path_get_dirname(buffer);
  gchar *bundle = g_path_get_dirname(bin_directory);
  gchar *resources = g_build_filename(bundle, "resources", NULL);
  g_free(bundle);
  g_free(bin_directory);
  return resources;
}

/* The native companion, loaded beside this executable and reached through two
   ordinary C entry points. Looked up rather than linked, so an application
   without one simply finds nothing. */
typedef const gchar *(*TachyonCompanionInvoke)(const gchar *);
typedef void (*TachyonCompanionFree)(const gchar *);
typedef void (*TachyonCompanionEmit)(const gchar *);
typedef void (*TachyonCompanionSetEmit)(TachyonCompanionEmit);

static TachyonCompanionInvoke g_companion_invoke = NULL;
static TachyonCompanionFree g_companion_free = NULL;

/* Delivers one publish on the main loop, where WebKit may be touched. */
static gboolean tachyon_deliver_publish(gpointer payload) {
  gchar *text = payload;
  if (tachyon_trusted_view(g_view)) {
    JSCContext *context = jsc_context_new();
    JSCValue *value = jsc_value_new_from_json(context, text);
    if (value != NULL) {
      gchar *json = jsc_value_to_json(value, 0);
      gchar *script = g_strdup_printf("globalThis.__tachyonCompanionPublish(%s)", json);
      webkit_web_view_evaluate_javascript(g_view, script, -1, NULL, NULL, NULL, NULL, NULL);
      g_free(script); g_free(json); g_object_unref(value);
    }
    g_object_unref(context);
  }
  g_free(text);
  g_atomic_int_add(&g_pending_publishes, -1);
  return G_SOURCE_REMOVE;
}

/* The sink handed to the companion.

   The other direction of the bridge: everything else is the page asking a
   question, and a companion watching the platform has no question to answer
   because nobody asked one.

   Copied and queued rather than used where it arrives: a companion may publish
   from a thread it started itself, the pointer is borrowed only for this call,
   and g_idle_add is the thread-safe way onto the loop that owns the view. */
static void tachyon_companion_emit(const gchar *payload) {
  if (payload == NULL || strnlen(payload, TACHYON_MESSAGE_LIMIT + 1) > TACHYON_MESSAGE_LIMIT) return;
  if (g_atomic_int_add(&g_pending_publishes, 1) >= 128) { g_atomic_int_add(&g_pending_publishes, -1); return; }
  g_idle_add(tachyon_deliver_publish, g_strdup(payload));
}

static void tachyon_companion_load(void) {
  gchar *root = tachyon_resource_root();
  gchar *bundle = g_path_get_dirname(root);
  gchar *path = g_build_filename(bundle, "bin", "libtachyoncompanion.so", NULL);
  GModule *module = g_module_open(path, G_MODULE_BIND_LAZY);
  if (module != NULL) {
    g_module_symbol(module, "tac_native_invoke", (gpointer *)&g_companion_invoke);
    g_module_symbol(module, "tac_native_free", (gpointer *)&g_companion_free);
    /* Looked up rather than required: a companion built before this existed
       loads and answers questions, it just never publishes. */
    TachyonCompanionSetEmit set_emit = NULL;
    g_module_symbol(module, "tac_native_set_emit", (gpointer *)&set_emit);
    if (set_emit != NULL) {
      set_emit(tachyon_companion_emit);
    }
    if (g_companion_invoke != NULL && g_companion_free != NULL) {
      tachyon_record("companion.loaded");
    }
  }
  g_free(path);
  g_free(bundle);
  g_free(root);
}

static gchar *tachyon_companion_invoke(const gchar *request) {
  if (g_companion_invoke == NULL) {
    if (request != NULL && tachyon_payload_string_matches(request, "op", "init")) return g_strdup("{\"value\":{\"fields\":[],\"methods\":[]}}");
    return g_strdup("{\"error\":\"This application has no native companion.\"}");
  }
  const gchar *answer = g_companion_invoke(request != NULL ? request : "{}");
  gchar *copy = g_strdup(answer != NULL && strnlen(answer, TACHYON_MESSAGE_LIMIT + 1) <= TACHYON_MESSAGE_LIMIT ? answer : "{\"error\":\"Invalid native response.\"}");
  if (g_companion_free != NULL && answer != NULL) {
    g_companion_free(answer);
  }
  return copy;
}

static void tachyon_record(const gchar *event) {
  const gchar *state = g_get_user_state_dir();
  gchar *directory = g_build_filename(state, "tachyon", NULL);
  g_mkdir_with_parents(directory, 0700);
  gchar *path = g_build_filename(directory, TACHYON_BUNDLE_ID ".jsonl", NULL);
  gchar *entry = g_strdup_printf("{\"event\":\"%s\",\"route\":\"%s\"}\n", event,
                                 g_route != NULL ? g_route : TACHYON_ENTRY_ROUTE);
  FILE *handle = fopen(path, "a");
  if (handle != NULL) {
    fputs(entry, handle);
    fclose(handle);
  }
  g_free(entry);
  g_free(path);
  g_free(directory);
}

/* The capability is echoed into a JSON string, so anything that could close
   that string early is dropped rather than escaped. */
static gchar *tachyon_safe_name(const gchar *value) {
  GString *allowed = g_string_new(NULL);
  for (gsize index = 0; value[index] != '\0' && index < 64; index += 1) {
    gchar character = value[index];
    if (g_ascii_isalnum(character) || character == '.' || character == '_' ||
        character == '-') {
      g_string_append_c(allowed, character);
    }
  }
  if (allowed->len == 0) {
    g_string_append(allowed, "unnamed");
  }
  return g_string_free(allowed, FALSE);
}

/* One function is the whole native surface. The page calls it; the answer goes
   back as the promise WebKit resolves for the script message. */
static gboolean tachyon_on_host_call(WebKitUserContentManager *manager, JSCValue *value,
                                     WebKitScriptMessageReply *reply, gpointer data) {
  (void)manager;
  WebKitWebView *view = WEBKIT_WEB_VIEW(data);
  char path[TACHYON_PATH_LIMIT];
  if (!tachyon_trusted_view(view) || !tachyon_local_path(webkit_web_view_get_uri(view), path, sizeof(path))) {
    webkit_script_message_reply_return_error_message(reply, "Native calls require the local application document."); return TRUE;
  }
  gchar *capability = NULL;
  gchar *payload = NULL;
  if (jsc_value_is_object(value)) {
    JSCValue *name = jsc_value_object_get_property(value, "capability");
    JSCValue *body = jsc_value_object_get_property(value, "payload");
    if (jsc_value_is_string(name)) capability = jsc_value_to_string(name);
    if (jsc_value_is_string(body)) payload = jsc_value_to_string(body);
    g_object_unref(name);
    g_object_unref(body);
  }
  if (capability == NULL || payload == NULL || strlen(capability) > 64 || strlen(payload) > TACHYON_MESSAGE_LIMIT) {
    g_free(capability); g_free(payload);
    webkit_script_message_reply_return_error_message(reply, "Native message exceeds its bounds."); return TRUE;
  }

  gchar *answer = NULL;
  if (g_strcmp0(capability, "companion.invoke") == 0) {
    const TachyonLocalRoute *route = tachyon_document_route(path);
    answer = route != NULL && tachyon_payload_route_matches(payload, route->route)
      ? tachyon_companion_invoke(payload) : g_strdup("{\"error\":\"Native route mismatch.\"}");
  } else {
    gchar *safe = tachyon_safe_name(capability);
    answer = g_strdup_printf(
        "{\"ok\":false,\"error\":\"linux host answers companion.invoke, not '%s'\"}", safe);
    g_free(safe);
  }

  JSCValue *result = jsc_value_new_string(jsc_value_get_context(value), answer);
  webkit_script_message_reply_return_value(reply, result);
  g_object_unref(result);
  g_free(answer);
  g_free(capability);
  g_free(payload);
  return TRUE;
}

static void tachyon_on_load(WebKitWebView *view, WebKitLoadEvent event, gpointer data) {
  (void)view;
  (void)data;
  if (event == WEBKIT_LOAD_FINISHED) {
    tachyon_record("controller.mounted");
    tachyon_record("controller.active");
  }
}

static void tachyon_activate(GtkApplication *app, gpointer data) {
  (void)data;
  tachyon_record("controller.created");
  tachyon_companion_load();
  GtkWidget *window = gtk_application_window_new(app);
  gtk_window_set_title(GTK_WINDOW(window), TACHYON_APP_NAME);
  gtk_window_set_default_size(GTK_WINDOW(window), 1024, 768);

  gchar *resources = tachyon_resource_root();
  gchar *bundle = g_build_filename(resources, "WebBundle", NULL);
  g_bundle_fd = open(bundle, O_RDONLY | O_CLOEXEC | O_DIRECTORY | O_NOFOLLOW);
  g_free(resources); g_free(bundle);
  if (g_bundle_fd < 0) { tachyon_record("bridge.unavailable"); return; }
  WebKitWebContext *context = webkit_web_context_new();
  webkit_web_context_register_uri_scheme(context, "tachyon", tachyon_scheme_request, NULL, NULL);
  WebKitSecurityManager *security = webkit_web_context_get_security_manager(context);
  webkit_security_manager_register_uri_scheme_as_secure(security, "tachyon");
  webkit_security_manager_register_uri_scheme_as_cors_enabled(security, "tachyon");
  WebKitNetworkSession *session = webkit_network_session_new_ephemeral();
  WebKitUserContentManager *manager = webkit_user_content_manager_new();
  WebKitWebView *view = WEBKIT_WEB_VIEW(g_object_new(
      WEBKIT_TYPE_WEB_VIEW, "web-context", context, "network-session", session, "user-content-manager", manager, NULL));
  g_view = view;

  WebKitSettings *settings = webkit_web_view_get_settings(view);
  webkit_settings_set_enable_javascript(settings, TRUE);
  webkit_settings_set_enable_developer_extras(settings, FALSE);
  webkit_settings_set_allow_file_access_from_file_urls(settings, FALSE);
  webkit_settings_set_allow_universal_access_from_file_urls(settings, FALSE);

  /* Injected before the bundle's own scripts, the same shim every host uses,
     with the Linux half of the bridge appended: a script message out, and a
     resolver the host calls back into. */
  gchar *shim = g_strdup_printf(
      "if (location.protocol === 'tachyon:' && location.host === 'app' && window === window.top) {\n%s\n}\n",
      TACHYON_NATIVE_SHIM);
  WebKitUserScript *script = webkit_user_script_new(
      shim, WEBKIT_USER_CONTENT_INJECT_TOP_FRAME, WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
      NULL, NULL);
  webkit_user_content_manager_add_script(manager, script);
  webkit_user_script_unref(script);
  g_free(shim);

  webkit_user_content_manager_register_script_message_handler_with_reply(manager, "tachyon", NULL);
  g_signal_connect(manager, "script-message-with-reply-received::tachyon",
                   G_CALLBACK(tachyon_on_host_call), view);
  g_signal_connect(view, "load-changed", G_CALLBACK(tachyon_on_load), NULL);
  g_signal_connect(view, "decide-policy", G_CALLBACK(tachyon_decide_policy), NULL);

  gtk_window_set_child(GTK_WINDOW(window), GTK_WIDGET(view));
  gtk_widget_set_hexpand(GTK_WIDGET(view), TRUE);
  gtk_widget_set_vexpand(GTK_WIDGET(view), TRUE);
  gtk_widget_set_can_focus(GTK_WIDGET(view), TRUE);
  gtk_accessible_update_property(GTK_ACCESSIBLE(view), GTK_ACCESSIBLE_PROPERTY_LABEL,
                                 TACHYON_APP_NAME, -1);

  gchar *uri = g_strdup_printf("tachyon://app%s%s", TACHYON_ENTRY_ROUTE,
                              g_str_has_suffix(TACHYON_ENTRY_ROUTE, "/") ? "" : "/");
  webkit_web_view_load_uri(view, uri);
  tachyon_record("bridge.ready");
  g_free(uri);

  gtk_window_present(GTK_WINDOW(window));
}

static void tachyon_shutdown(GApplication *app, gpointer data) {
  (void)app;
  (void)data;
  tachyon_record("controller.destroyed");
  g_view = NULL;
  if (g_bundle_fd >= 0) { close(g_bundle_fd); g_bundle_fd = -1; }
}

int main(int argc, char **argv) {
  g_route = g_strdup(TACHYON_ENTRY_ROUTE);
  GtkApplication *app = gtk_application_new("__GTK_APP_ID__", G_APPLICATION_DEFAULT_FLAGS);
  g_signal_connect(app, "activate", G_CALLBACK(tachyon_activate), NULL);
  g_signal_connect(app, "shutdown", G_CALLBACK(tachyon_shutdown), NULL);
  int status = g_application_run(G_APPLICATION(app), argc, argv);
  g_object_unref(app);
  g_free(g_route);
  return status;
}
"#;

#[cfg(test)]
mod tests {
    use super::{c_source, desktop_entry, gtk_application_id};
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
    fn the_host_is_a_gtk_window_around_one_web_view() {
        let source = c_source(&application(), &index());
        assert!(source.contains("gtk_application_window_new"));
        assert!(source.contains("webkit_web_view_load_uri"));
        assert!(source.contains("webkit_user_content_manager_register_script_message_handler"));
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        // Nothing rebuilds the view out of GTK widgets any more.
        assert!(!source.contains("gtk_label_new"));
        assert!(!source.contains("tachyon_build_node"));
        assert!(source.contains("webkit_web_context_register_uri_scheme"));
        assert!(source.contains("register_script_message_handler_with_reply"));
        assert!(source.contains("frame-src 'none'"));
        assert!(source.contains("O_NOFOLLOW"));
        assert!(source.contains("tachyon_payload_route_matches"));
        assert!(!source.contains("file://%s"));
        assert!(!source.contains("set_allow_universal_access_from_file_urls(settings, TRUE)"));
    }

    #[test]
    fn a_gtk_application_id_and_desktop_entry_are_valid() {
        assert_eq!(
            gtk_application_id("dev.tachyon.native-catalog"),
            "dev.tachyon.native_catalog"
        );
        let entry = desktop_entry(&application());
        assert!(entry.contains("Exec=bin/NativeCatalog"));
        assert!(entry.contains("Type=Application"));
    }
}
