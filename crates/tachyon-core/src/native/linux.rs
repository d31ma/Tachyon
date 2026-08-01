//! Linux GTK4 host generation.
//!
//! The generated host renders Native UI v1 through GTK4 widgets and isolates
//! unsupported subtrees in ephemeral `WebKitGTK` surfaces without a bridge.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, c_string_escape, first_line, native_tool_failure, run_tool, stage_application,
    write, write_host_source, xml_escape,
};
use super::planner::{NativeRouteIndex, PlannedNativeRoute};
use crate::Failure;
use std::path::{Path, PathBuf};

/// `pkg-config` modules required by the generated GTK4 host.
const PKG_CONFIG_MODULES: [&str; 3] = ["gtk4", "webkitgtk-6.0", "json-glib-1.0"];

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LinuxHostGenerator;

impl LinuxHostGenerator {
    pub(super) async fn generate(
        application: &NativeApplication,
        routes: &[PlannedNativeRoute],
        index: &NativeRouteIndex,
        web_bundle: &Path,
        stage: &Path,
        package: bool,
    ) -> Result<GeneratedHost, Failure> {
        let bundle = stage.join(&application.executable_name);
        let resources = bundle.join("resources");
        stage_application(application, routes, index, web_bundle, stage, &resources)?;

        let source_path = stage.join("project").join("tachyon_host.c");
        write_host_source(&source_path, &c_source(application))?;
        write(
            &bundle.join(format!("{}.desktop", application.application_id)),
            desktop_entry(application).as_bytes(),
        )?;
        if !package {
            return Ok(GeneratedHost {
                application_bundle: PathBuf::from("project/tachyon_host.c"),
                toolchain_name: String::from("source"),
                toolchain_version: String::from("not-packaged"),
            });
        }

        let executable = bundle.join("bin").join(&application.executable_name);
        let compiler_version = compile_c(&source_path, &executable).await?;
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
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Exec=bin/{executable}\n\
         Terminal=false\n\
         Categories=Utility;\n\
         X-Tachyon-Version={version}\n",
        name = xml_escape(&application.name),
        executable = application.executable_name,
        version = application.version,
    )
}

fn c_source(application: &NativeApplication) -> String {
    C_HOST
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

const C_HOST: &str = r#"#include <gtk/gtk.h>
#include <json-glib/json-glib.h>
#include <webkit/webkit.h>

#include <limits.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define TACHYON_APP_NAME "__APP_NAME__"
#define TACHYON_BUNDLE_ID "__BUNDLE_ID__"
#define TACHYON_GTK_APP_ID "__GTK_APP_ID__"
#define TACHYON_MAX_DEPTH 64
#define TACHYON_MAX_STATE_BYTES 4096

typedef struct {
  GHashTable *state;      /* char* -> char* */
  GHashTable *outputs;    /* char* -> GPtrArray of GtkLabel* */
  gchar *resource_root;
  gchar *route;
  const gchar *lifecycle;
} TachyonModel;

typedef struct {
  TachyonModel *model;
  gchar *action;
} TachyonAction;

typedef struct {
  TachyonModel *model;
  gchar *binding;
} TachyonBinding;

typedef struct {
  gchar *source;
  gchar *location;
} TachyonSurfacePolicy;

static void tachyon_record(TachyonModel *model, const char *event) {
  const gchar *state_home = g_get_user_state_dir();
  gchar *directory = g_build_filename(state_home, "tachyon", NULL);
  g_mkdir_with_parents(directory, 0700);
  gchar *path = g_build_filename(directory, TACHYON_BUNDLE_ID ".jsonl", NULL);
  gchar *line = g_strdup_printf("{\"event\":\"%.128s\",\"route\":\"%.256s\"}\n", event,
                                model->route ? model->route : "/");
  FILE *handle = fopen(path, "ae");
  if (handle != NULL) {
    fputs(line, handle);
    fclose(handle);
  }
  g_free(line);
  g_free(path);
  g_free(directory);
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

static JsonNode *tachyon_load_json(const gchar *path) {
  JsonParser *parser = json_parser_new();
  GError *error = NULL;
  if (!json_parser_load_from_file(parser, path, &error)) {
    if (error != NULL) {
      g_error_free(error);
    }
    g_object_unref(parser);
    return NULL;
  }
  JsonNode *root = json_node_copy(json_parser_get_root(parser));
  g_object_unref(parser);
  return root;
}

static const gchar *tachyon_member_string(JsonObject *object, const gchar *name) {
  if (object == NULL || !json_object_has_member(object, name)) {
    return NULL;
  }
  JsonNode *node = json_object_get_member(object, name);
  if (JSON_NODE_HOLDS_VALUE(node) && json_node_get_value_type(node) == G_TYPE_STRING) {
    return json_node_get_string(node);
  }
  return NULL;
}

static const gchar *tachyon_nested_string(JsonObject *object, const gchar *group,
                                          const gchar *name) {
  if (object == NULL || !json_object_has_member(object, group)) {
    return NULL;
  }
  JsonNode *node = json_object_get_member(object, group);
  if (!JSON_NODE_HOLDS_OBJECT(node)) {
    return NULL;
  }
  return tachyon_member_string(json_node_get_object(node), name);
}

static void tachyon_set_label(GtkWidget *widget, const gchar *label) {
  if (label == NULL || *label == '\0') {
    return;
  }
  gtk_accessible_update_property(GTK_ACCESSIBLE(widget), GTK_ACCESSIBLE_PROPERTY_LABEL, label, -1);
}

/* GTK4 treats a plain container as presentational, so an accessible name set
   on it never reaches the accessibility bus. Wrapping the container in a
   borderless frame gives it the group role that can carry the name. */
static GtkWidget *tachyon_named_group(GtkWidget *child, const gchar *label) {
  if (label == NULL || *label == '\0') {
    return child;
  }
  GtkWidget *frame = gtk_frame_new(NULL);
  gtk_widget_add_css_class(frame, "flat");
  gtk_frame_set_child(GTK_FRAME(frame), child);
  tachyon_set_label(frame, label);
  return frame;
}

static gchar *tachyon_node_text(JsonObject *node) {
  const gchar *kind = tachyon_member_string(node, "kind");
  if (kind != NULL && g_strcmp0(kind, "text") == 0) {
    const gchar *value = tachyon_member_string(node, "value");
    return g_strdup(value != NULL ? value : "");
  }
  GString *text = g_string_new(NULL);
  if (node != NULL && json_object_has_member(node, "children")) {
    JsonArray *children = json_object_get_array_member(node, "children");
    guint count = json_array_get_length(children);
    for (guint index = 0; index < count; index += 1) {
      JsonObject *child = json_array_get_object_element(children, index);
      gchar *child_text = tachyon_node_text(child);
      g_string_append(text, child_text);
      g_free(child_text);
    }
  }
  return g_string_free(text, FALSE);
}

static void tachyon_render_binding(TachyonModel *model, const gchar *binding) {
  GPtrArray *labels = g_hash_table_lookup(model->outputs, binding);
  if (labels == NULL) {
    return;
  }
  const gchar *value = g_hash_table_lookup(model->state, binding);
  for (guint index = 0; index < labels->len; index += 1) {
    gtk_label_set_text(GTK_LABEL(g_ptr_array_index(labels, index)), value ? value : "");
  }
}

static void tachyon_set_state(TachyonModel *model, const gchar *binding, const gchar *value) {
  g_hash_table_replace(model->state, g_strdup(binding),
                       g_strndup(value, TACHYON_MAX_STATE_BYTES));
  tachyon_render_binding(model, binding);
}

static void tachyon_action_free(gpointer data, GClosure *closure) {
  (void)closure;
  TachyonAction *action = data;
  g_free(action->action);
  g_free(action);
}

static void tachyon_binding_free(gpointer data, GClosure *closure) {
  (void)closure;
  TachyonBinding *binding = data;
  g_free(binding->binding);
  g_free(binding);
}

static void tachyon_on_click(GtkButton *button, gpointer data) {
  (void)button;
  TachyonAction *action = data;
  TachyonModel *model = action->model;
  if (g_strcmp0(model->lifecycle, "destroyed") == 0 || action->action == NULL) {
    return;
  }
  gchar **parts = g_strsplit(action->action, ":", 2);
  if (parts[0] == NULL || parts[1] == NULL) {
    g_strfreev(parts);
    return;
  }
  const gchar *current = g_hash_table_lookup(model->state, parts[1]);
  if (current == NULL) {
    g_strfreev(parts);
    return;
  }
  if (g_strcmp0(parts[0], "increment") == 0) {
    gchar *end = NULL;
    gint64 value = g_ascii_strtoll(current, &end, 10);
    if (end != NULL && *end == '\0') {
      gchar *next = g_strdup_printf("%" G_GINT64_FORMAT, value + 1);
      tachyon_set_state(model, parts[1], next);
      g_free(next);
      tachyon_record(model, "state.increment");
    }
  } else if (g_strcmp0(parts[0], "toggle") == 0) {
    tachyon_set_state(model, parts[1], g_strcmp0(current, "true") == 0 ? "false" : "true");
    tachyon_record(model, "state.toggle");
  }
  g_strfreev(parts);
}

static void tachyon_on_input(GtkEditable *editable, gpointer data) {
  TachyonBinding *binding = data;
  const gchar *text = gtk_editable_get_text(editable);
  tachyon_set_state(binding->model, binding->binding, text != NULL ? text : "");
  tachyon_record(binding->model, "state.input");
}

static void tachyon_on_expanded(GObject *expander, GParamSpec *spec, gpointer data) {
  (void)spec;
  TachyonBinding *binding = data;
  gboolean expanded = gtk_expander_get_expanded(GTK_EXPANDER(expander));
  tachyon_set_state(binding->model, binding->binding, expanded ? "true" : "false");
  tachyon_record(binding->model, "state.disclosure");
}

static void tachyon_surface_policy_free(gpointer data, GClosure *closure) {
  (void)closure;
  TachyonSurfacePolicy *policy = data;
  g_free(policy->source);
  g_free(policy->location);
  g_free(policy);
}

static gboolean tachyon_decide_policy(WebKitWebView *view, WebKitPolicyDecision *decision,
                                      WebKitPolicyDecisionType type, gpointer data) {
  (void)view;
  if (type != WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION) {
    webkit_policy_decision_ignore(decision);
    return TRUE;
  }
  TachyonSurfacePolicy *policy = data;
  WebKitNavigationAction *action =
      webkit_navigation_policy_decision_get_navigation_action(WEBKIT_NAVIGATION_POLICY_DECISION(decision));
  const gchar *uri = webkit_uri_request_get_uri(webkit_navigation_action_get_request(action));
  if (uri == NULL) {
    webkit_policy_decision_ignore(decision);
    return TRUE;
  }
  gboolean allowed = FALSE;
  if (g_strcmp0(policy->source, "local_bundle") == 0) {
    allowed = g_str_has_prefix(uri, "file://");
  } else if (g_strcmp0(policy->source, "remote_url") == 0 && policy->location != NULL) {
    GUri *declared = g_uri_parse(policy->location, G_URI_FLAGS_NONE, NULL);
    GUri *candidate = g_uri_parse(uri, G_URI_FLAGS_NONE, NULL);
    if (declared != NULL && candidate != NULL) {
      allowed = g_strcmp0(g_uri_get_scheme(candidate), "https") == 0 &&
                g_strcmp0(g_uri_get_host(candidate), g_uri_get_host(declared)) == 0;
    }
    if (declared != NULL) {
      g_uri_unref(declared);
    }
    if (candidate != NULL) {
      g_uri_unref(candidate);
    }
  }
  if (allowed) {
    webkit_policy_decision_use(decision);
  } else {
    webkit_policy_decision_ignore(decision);
  }
  return TRUE;
}

static GtkWidget *tachyon_build_node(TachyonModel *model, JsonObject *node, guint depth);

static void tachyon_append_children(TachyonModel *model, GtkWidget *container, JsonObject *node,
                                    guint depth) {
  if (node == NULL || !json_object_has_member(node, "children")) {
    return;
  }
  JsonArray *children = json_object_get_array_member(node, "children");
  guint count = json_array_get_length(children);
  for (guint index = 0; index < count; index += 1) {
    JsonObject *child = json_array_get_object_element(children, index);
    GtkWidget *widget = tachyon_build_node(model, child, depth + 1);
    if (widget != NULL) {
      gtk_box_append(GTK_BOX(container), widget);
    }
  }
}

static void tachyon_surface_measured(GObject *source, GAsyncResult *result, gpointer data) {
  (void)data;
  JSCValue *value =
      webkit_web_view_evaluate_javascript_finish(WEBKIT_WEB_VIEW(source), result, NULL);
  if (value == NULL) {
    return;
  }
  if (jsc_value_is_number(value)) {
    gint height = (gint)jsc_value_to_double(value);
    if (height > 0) {
      gtk_widget_set_size_request(GTK_WIDGET(source), -1, height);
    }
  }
  g_object_unref(value);
}

/* A fallback subtree is as tall as its document. A fixed height clipped
 * whatever rendered past it, and the window has no scroll of its own to reveal
 * the rest, so the document reports its height once it has settled. A surface
 * that cannot run script keeps the default height. */
static void tachyon_surface_loaded(WebKitWebView *view, WebKitLoadEvent event, gpointer data) {
  (void)data;
  if (event != WEBKIT_LOAD_FINISHED) {
    return;
  }
  webkit_web_view_evaluate_javascript(view, "document.documentElement.scrollHeight", -1, NULL,
                                      NULL, NULL, tachyon_surface_measured, NULL);
}

static GtkWidget *tachyon_build_web_surface(TachyonModel *model, JsonObject *node) {
  TachyonSurfacePolicy *policy = g_new0(TachyonSurfacePolicy, 1);
  policy->source = g_strdup(tachyon_member_string(node, "source"));
  policy->location = g_strdup(tachyon_member_string(node, "location"));

  WebKitNetworkSession *session = webkit_network_session_new_ephemeral();
  GtkWidget *view = g_object_new(WEBKIT_TYPE_WEB_VIEW, "network-session", session, NULL);
  g_object_unref(session);

  WebKitSettings *settings = webkit_web_view_get_settings(WEBKIT_WEB_VIEW(view));
  webkit_settings_set_enable_javascript(settings,
                                        g_strcmp0(policy->source, "local_bundle") == 0);
  webkit_settings_set_enable_developer_extras(settings, FALSE);
  g_signal_connect_data(view, "decide-policy", G_CALLBACK(tachyon_decide_policy), policy,
                        tachyon_surface_policy_free, 0);

  if (g_strcmp0(policy->source, "local_bundle") == 0 && policy->location != NULL) {
    gchar *path = g_build_filename(model->resource_root, policy->location, NULL);
    gchar *uri = g_filename_to_uri(path, NULL, NULL);
    if (uri != NULL) {
      webkit_web_view_load_uri(WEBKIT_WEB_VIEW(view), uri);
      g_free(uri);
    }
    g_free(path);
  } else if (policy->location != NULL) {
    webkit_web_view_load_uri(WEBKIT_WEB_VIEW(view), policy->location);
  }

  gtk_widget_set_size_request(view, -1, 180);
  gtk_widget_set_vexpand(view, FALSE);
  g_signal_connect(view, "load-changed", G_CALLBACK(tachyon_surface_loaded), NULL);
  tachyon_record(model, "websurface.attached");
  return tachyon_named_group(view, tachyon_nested_string(node, "accessibility", "label"));
}

static GtkWidget *tachyon_build_node(TachyonModel *model, JsonObject *node, guint depth) {
  if (node == NULL || depth > TACHYON_MAX_DEPTH) {
    return NULL;
  }
  const gchar *kind = tachyon_member_string(node, "kind");
  if (g_strcmp0(kind, "text") == 0) {
    const gchar *value = tachyon_member_string(node, "value");
    if (value == NULL || *value == '\0') {
      return NULL;
    }
    GtkWidget *label = gtk_label_new(value);
    gtk_label_set_xalign(GTK_LABEL(label), 0.0f);
    gtk_label_set_wrap(GTK_LABEL(label), TRUE);
    return label;
  }
  if (g_strcmp0(kind, "web_surface") == 0) {
    return tachyon_build_web_surface(model, node);
  }

  const gchar *adapter = tachyon_member_string(node, "adapter");
  const gchar *identifier = tachyon_member_string(node, "id");
  const gchar *label_text = tachyon_nested_string(node, "accessibility", "label");
  const gchar *binding = tachyon_nested_string(node, "properties", "binding");
  const gchar *action = tachyon_nested_string(node, "properties", "action");
  gchar *text = tachyon_node_text(node);
  GtkWidget *widget = NULL;

  if (adapter == NULL) {
    adapter = "";
  }
  if (g_strcmp0(adapter, "layout.app_bar") == 0) {
    widget = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 12);
    gtk_widget_set_margin_top(widget, 16);
    gtk_widget_set_margin_bottom(widget, 16);
    gtk_widget_set_margin_start(widget, 16);
    gtk_widget_set_margin_end(widget, 16);
    tachyon_append_children(model, widget, node, depth);
  } else if (g_strcmp0(adapter, "layout.column") == 0 ||
             g_strcmp0(adapter, "layout.list") == 0 ||
             g_strcmp0(adapter, "layout.list_item") == 0) {
    const gchar *role = tachyon_nested_string(node, "accessibility", "role");
    gboolean is_main = g_strcmp0(role, "main") == 0;
    widget = gtk_box_new(GTK_ORIENTATION_VERTICAL, is_main ? 16 : 8);
    if (is_main) {
      gtk_widget_set_margin_top(widget, 24);
      gtk_widget_set_margin_bottom(widget, 24);
      gtk_widget_set_margin_start(widget, 24);
      gtk_widget_set_margin_end(widget, 24);
    }
    gtk_widget_set_halign(widget, GTK_ALIGN_FILL);
    tachyon_append_children(model, widget, node, depth);
  } else if (g_str_has_prefix(adapter, "text.heading")) {
    widget = gtk_label_new(text);
    gtk_label_set_xalign(GTK_LABEL(widget), 0.0f);
    gtk_label_set_wrap(GTK_LABEL(widget), TRUE);
    gtk_widget_add_css_class(widget, g_strcmp0(adapter, "text.heading1") == 0 ? "title-1"
                                                                             : "title-2");
    gtk_accessible_update_property(GTK_ACCESSIBLE(widget), GTK_ACCESSIBLE_PROPERTY_LABEL,
                                   text, -1);
  } else if (g_strcmp0(adapter, "content.text") == 0) {
    widget = gtk_label_new(text);
    gtk_label_set_xalign(GTK_LABEL(widget), 0.0f);
    gtk_label_set_wrap(GTK_LABEL(widget), TRUE);
  } else if (g_strcmp0(adapter, "control.button") == 0) {
    /* A button constructed with an intrinsic label derives its accessible
       name from that label and ignores an explicit one. Supplying the text as
       a hidden child keeps the declared accessible name authoritative. */
    widget = gtk_button_new();
    GtkWidget *caption = gtk_label_new(text);
    gtk_accessible_update_state(GTK_ACCESSIBLE(caption), GTK_ACCESSIBLE_STATE_HIDDEN, TRUE, -1);
    gtk_button_set_child(GTK_BUTTON(widget), caption);
    gtk_widget_set_halign(widget, GTK_ALIGN_START);
    TachyonAction *payload = g_new0(TachyonAction, 1);
    payload->model = model;
    payload->action = g_strdup(action);
    g_signal_connect_data(widget, "clicked", G_CALLBACK(tachyon_on_click), payload,
                          tachyon_action_free, 0);
    tachyon_set_label(widget, label_text != NULL ? label_text : text);
  } else if (g_strcmp0(adapter, "control.text_field") == 0) {
    widget = gtk_entry_new();
    const gchar *placeholder = tachyon_nested_string(node, "properties", "placeholder");
    if (placeholder != NULL) {
      gtk_entry_set_placeholder_text(GTK_ENTRY(widget), placeholder);
    }
    if (binding != NULL) {
      const gchar *initial = g_hash_table_lookup(model->state, binding);
      if (initial != NULL) {
        gtk_editable_set_text(GTK_EDITABLE(widget), initial);
      }
      TachyonBinding *payload = g_new0(TachyonBinding, 1);
      payload->model = model;
      payload->binding = g_strdup(binding);
      g_signal_connect_data(widget, "changed", G_CALLBACK(tachyon_on_input), payload,
                            tachyon_binding_free, 0);
    }
    tachyon_set_label(widget, label_text != NULL ? label_text : placeholder);
  } else if (g_strcmp0(adapter, "content.output") == 0 && binding != NULL) {
    widget = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
    if (label_text != NULL) {
      GtkWidget *caption = gtk_label_new(label_text);
      gtk_label_set_xalign(GTK_LABEL(caption), 0.0f);
      gtk_widget_add_css_class(caption, "caption");
      gtk_box_append(GTK_BOX(widget), caption);
    }
    const gchar *value = g_hash_table_lookup(model->state, binding);
    GtkWidget *output = gtk_label_new(value != NULL ? value : "");
    gtk_label_set_xalign(GTK_LABEL(output), 0.0f);
    gtk_widget_add_css_class(output, "title-2");
    gtk_box_append(GTK_BOX(widget), output);
    GPtrArray *labels = g_hash_table_lookup(model->outputs, binding);
    if (labels == NULL) {
      labels = g_ptr_array_new();
      g_hash_table_replace(model->outputs, g_strdup(binding), labels);
    }
    g_ptr_array_add(labels, output);
    tachyon_set_label(widget, label_text);
  } else if (g_strcmp0(adapter, "control.disclosure") == 0) {
    const gchar *summary = tachyon_nested_string(node, "properties", "label");
    widget = gtk_expander_new(summary != NULL ? summary : "Details");
    GtkWidget *content = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    tachyon_append_children(model, content, node, depth);
    gtk_expander_set_child(GTK_EXPANDER(widget), content);
    const gchar *key = binding != NULL ? binding : identifier;
    if (key != NULL) {
      TachyonBinding *payload = g_new0(TachyonBinding, 1);
      payload->model = model;
      payload->binding = g_strdup(key);
      g_signal_connect_data(widget, "notify::expanded", G_CALLBACK(tachyon_on_expanded), payload,
                            tachyon_binding_free, 0);
    }
    tachyon_set_label(widget, label_text != NULL ? label_text : summary);
  } else if (g_strcmp0(adapter, "navigation.link") == 0) {
    widget = gtk_button_new_with_label(text);
    gtk_widget_add_css_class(widget, "link");
    gtk_widget_set_halign(widget, GTK_ALIGN_START);
    tachyon_set_label(widget, label_text != NULL ? label_text : text);
  } else if (g_strcmp0(adapter, "content.image") == 0) {
    widget = gtk_image_new_from_icon_name("image-x-generic-symbolic");
    gtk_image_set_pixel_size(GTK_IMAGE(widget), 48);
    gtk_widget_set_halign(widget, GTK_ALIGN_START);
    tachyon_set_label(widget, label_text != NULL ? label_text : "Image");
  } else if (g_strcmp0(adapter, "content.divider") == 0) {
    widget = gtk_separator_new(GTK_ORIENTATION_HORIZONTAL);
  } else {
    widget = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    tachyon_append_children(model, widget, node, depth);
  }

  if (identifier != NULL) {
    gtk_widget_set_name(widget, identifier);
  }
  tachyon_set_label(widget, label_text);
  if (g_str_has_prefix(adapter, "layout.")) {
    widget = tachyon_named_group(widget, label_text);
  }
  const gchar *hidden = tachyon_nested_string(node, "properties", "aria-hidden");
  if (g_strcmp0(hidden, "true") == 0) {
    gtk_accessible_update_state(GTK_ACCESSIBLE(widget), GTK_ACCESSIBLE_STATE_HIDDEN, TRUE, -1);
  }
  g_free(text);
  return widget;
}

static void tachyon_load_state(TachyonModel *model, JsonObject *index, const gchar *route) {
  if (!json_object_has_member(index, "initial_state")) {
    return;
  }
  JsonObject *all = json_object_get_object_member(index, "initial_state");
  if (!json_object_has_member(all, route)) {
    return;
  }
  JsonObject *entries = json_object_get_object_member(all, route);
  GList *keys = json_object_get_members(entries);
  for (GList *item = keys; item != NULL; item = item->next) {
    const gchar *key = item->data;
    const gchar *value = tachyon_member_string(entries, key);
    if (value != NULL) {
      g_hash_table_replace(model->state, g_strdup(key), g_strdup(value));
    }
  }
  g_list_free(keys);
}

static const gchar *tachyon_document_for_route(JsonObject *index, const gchar *route) {
  JsonArray *routes = json_object_get_array_member(index, "routes");
  guint count = json_array_get_length(routes);
  for (guint position = 0; position < count; position += 1) {
    JsonObject *entry = json_array_get_object_element(routes, position);
    if (g_strcmp0(tachyon_member_string(entry, "route"), route) == 0) {
      return tachyon_member_string(entry, "document");
    }
  }
  return NULL;
}

static void tachyon_activate(GtkApplication *app, gpointer data) {
  TachyonModel *model = data;
  GtkWidget *window = gtk_application_window_new(app);
  gtk_window_set_title(GTK_WINDOW(window), TACHYON_APP_NAME);
  gtk_window_set_default_size(GTK_WINDOW(window), 420, 780);

  GtkWidget *scroller = gtk_scrolled_window_new();
  gtk_widget_set_hexpand(scroller, TRUE);
  gtk_widget_set_vexpand(scroller, TRUE);

  gchar *index_path = g_build_filename(model->resource_root, "NativeIndex.json", NULL);
  JsonNode *index_node = tachyon_load_json(index_path);
  g_free(index_path);

  GtkWidget *content = NULL;
  if (index_node != NULL && JSON_NODE_HOLDS_OBJECT(index_node)) {
    JsonObject *index = json_node_get_object(index_node);
    const gchar *entry_route = tachyon_member_string(index, "entry_route");
    if (entry_route == NULL) {
      entry_route = "/";
    }
    g_free(model->route);
    model->route = g_strdup(entry_route);
    tachyon_load_state(model, index, entry_route);
    const gchar *document = tachyon_document_for_route(index, entry_route);
    if (document != NULL) {
      gchar *document_path =
          g_build_filename(model->resource_root, "NativeUI", document, NULL);
      JsonNode *view = tachyon_load_json(document_path);
      g_free(document_path);
      if (view != NULL && JSON_NODE_HOLDS_OBJECT(view)) {
        JsonObject *view_object = json_node_get_object(view);
        const gchar *target = tachyon_member_string(view_object, "target");
        gint64 version = json_object_has_member(view_object, "contract_version")
                             ? json_object_get_int_member(view_object, "contract_version")
                             : 0;
        if (version == 1 && g_strcmp0(target, "linux") == 0) {
          content = tachyon_build_node(model, json_object_get_object_member(view_object, "root"), 0);
        }
      }
      if (view != NULL) {
        json_node_free(view);
      }
    }
  }
  if (index_node != NULL) {
    json_node_free(index_node);
  }

  if (content == NULL) {
    content = gtk_label_new("Unable to load native application resources.");
    tachyon_record(model, "route.failed");
  } else {
    tachyon_record(model, "route.opened");
  }
  gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroller), content);
  gtk_window_set_child(GTK_WINDOW(window), scroller);

  model->lifecycle = "mounted";
  tachyon_record(model, "controller.mounted");
  gtk_window_present(GTK_WINDOW(window));
  model->lifecycle = "active";
  tachyon_record(model, "controller.active");
}

static void tachyon_shutdown(GApplication *app, gpointer data) {
  (void)app;
  TachyonModel *model = data;
  model->lifecycle = "destroyed";
  tachyon_record(model, "controller.destroyed");
}

int main(int argc, char **argv) {
  TachyonModel model = {0};
  model.state = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_free);
  model.outputs = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                        (GDestroyNotify)g_ptr_array_unref);
  model.resource_root = tachyon_resource_root();
  model.route = g_strdup("/");
  model.lifecycle = "created";
  tachyon_record(&model, "controller.created");

  GtkApplication *app = gtk_application_new(TACHYON_GTK_APP_ID, G_APPLICATION_DEFAULT_FLAGS);
  g_signal_connect(app, "activate", G_CALLBACK(tachyon_activate), &model);
  g_signal_connect(app, "shutdown", G_CALLBACK(tachyon_shutdown), &model);
  int status = g_application_run(G_APPLICATION(app), argc, argv);
  g_object_unref(app);

  g_hash_table_destroy(model.outputs);
  g_hash_table_destroy(model.state);
  g_free(model.resource_root);
  g_free(model.route);
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
        }
    }

    #[test]
    fn generated_host_covers_adapters_lifecycle_and_isolated_surfaces() {
        let source = c_source(&application());
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains("webkit_network_session_new_ephemeral"));
        assert!(source.contains("gtk_accessible_update_property"));
        assert!(source.contains("control.button"));
        assert!(source.contains("control.disclosure"));
        assert!(source.contains(r#"g_strcmp0(target, "linux")"#));
        assert!(!source.contains("webkit_user_content_manager_register_script_message_handler"));
    }

    #[test]
    fn gtk_application_ids_avoid_hyphenated_bus_names() {
        assert_eq!(
            gtk_application_id("dev.tachyon.native-catalog"),
            "dev.tachyon.native_catalog"
        );
        assert_eq!(gtk_application_id("dev.example.app"), "dev.example.app");
    }

    #[test]
    fn desktop_entry_points_at_the_generated_executable() {
        let entry = desktop_entry(&application());
        assert!(entry.contains("Exec=bin/NativeCatalog"));
        assert!(entry.contains("Name=Native Catalog"));
        assert!(entry.contains("X-Tachyon-Version=1.0.0"));
    }
}
