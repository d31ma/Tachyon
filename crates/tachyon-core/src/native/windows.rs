//! Windows Win32 host generation.
//!
//! Unlike the Apple, Linux, and Android hosts, the Windows host lowers Native
//! UI v1 into a generated control table at build time instead of parsing JSON
//! at run time. The published `NativeUI/*.json` artifacts remain the contract;
//! the table is a deterministic compile-time projection of them.
//!
//! Embedded `WebSurface` rendering is deferred behind a `WebView2` viability
//! gate. Until then a surface renders an accessible placeholder that names the
//! fallback reason and opens its content in the default browser on request.

use super::config::NativeApplication;
use super::host::{
    GeneratedHost, c_string_escape, first_line, native_tool_failure, run_tool, stage_application,
    write, write_host_source,
};
use super::planner::{NativeRouteIndex, PlannedNativeRoute};
use crate::Failure;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use tachyon_contracts::NativeNode;

/// Maximum number of generated controls in one Windows host.
const MAX_ITEMS: usize = 4_096;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WindowsHostGenerator;

impl WindowsHostGenerator {
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

        let entry = routes
            .iter()
            .find(|route| route.route == application.entry_route)
            .ok_or_else(|| {
                native_tool_failure(1601, "Native entry route has no planned Windows view.")
            })?;
        let source_path = stage.join("project").join("tachyon_host.c");
        write_host_source(&source_path, &c_source(application, entry)?)?;
        // Windows reads a side-by-side manifest only as `<executable>.manifest`
        // beside the executable itself. A copy at the bundle root is never
        // consulted, so the process would run without the Common-Controls v6
        // activation context and every standard control would fall back to the
        // generic window provider — reaching assistive technology as a plain
        // pane rather than as a button, output, or heading.
        write(
            &bundle
                .join("bin")
                .join(format!("{}.exe.manifest", application.executable_name)),
            application_manifest(application).as_bytes(),
        )?;
        if !package {
            return Ok(GeneratedHost {
                application_bundle: PathBuf::from("project/tachyon_host.c"),
                toolchain_name: String::from("source"),
                toolchain_version: String::from("not-packaged"),
            });
        }

        let executable = bundle
            .join("bin")
            .join(format!("{}.exe", application.executable_name));
        let compiler_version = compile_c(&source_path, &executable).await?;
        Ok(GeneratedHost {
            application_bundle: PathBuf::from(&application.executable_name),
            toolchain_name: String::from("mingw-w64-gcc"),
            toolchain_version: compiler_version,
        })
    }
}

/// Returns the C compiler that produces a Windows executable on this machine.
const fn windows_compiler() -> &'static str {
    if cfg!(target_os = "windows") {
        "gcc"
    } else {
        "x86_64-w64-mingw32-gcc"
    }
}

async fn compile_c(source: &Path, executable: &Path) -> Result<String, Failure> {
    if let Some(parent) = executable.parent() {
        super::host::native_io(std::fs::create_dir_all(parent), parent)?;
    }
    let compiler = windows_compiler();
    let version = first_line(
        &run_tool(compiler, &["--version"]).await?,
        "mingw-w64 unknown",
    );
    let source = compiler_path(source, "Host source")?;
    let executable = compiler_path(executable, "Application")?;
    run_tool(
        compiler,
        &[
            "-std=c17",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-municode",
            "-mwindows",
            &source,
            "-o",
            &executable,
            "-lcomctl32",
            "-lshell32",
            "-luser32",
            "-lgdi32",
        ],
    )
    .await?;
    Ok(version)
}

/// MinGW interprets backslashes in extended Windows paths as escapes. GCC
/// accepts forward slashes on Windows, which also keeps Unix cross-builds
/// unchanged.
fn compiler_path(path: &Path, label: &str) -> Result<String, Failure> {
    let value = path
        .to_str()
        .ok_or_else(|| native_tool_failure(1605, &format!("{label} path is not valid Unicode.")))?;
    if let Some(value) = value.strip_prefix(r"\\?\UNC\") {
        Ok(format!("//{}", value.replace('\\', "/")))
    } else {
        Ok(value
            .strip_prefix(r"\\?\")
            .unwrap_or(value)
            .replace('\\', "/"))
    }
}

fn application_manifest(application: &NativeApplication) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="{identifier}" version="1.0.0.0"/>
  <dependency><dependentAssembly><assemblyIdentity
    type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
    processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>
"#,
        identifier = application.application_id.replace('.', "-"),
    )
}

/// One generated Win32 control descriptor.
#[derive(Clone, Debug, Default)]
struct Item {
    kind: &'static str,
    text: String,
    label: String,
    identifier: String,
    binding: String,
    action: String,
    location: String,
    indent: u32,
}

/// Lowers one planned route into a deterministic flat control table.
fn lower(node: &NativeNode, indent: u32, items: &mut Vec<Item>) -> Result<(), Failure> {
    if items.len() >= MAX_ITEMS {
        return Err(native_tool_failure(
            1605,
            "Windows host exceeds the limit of 4,096 generated controls.",
        ));
    }
    match node {
        NativeNode::Text { value } => {
            if !value.trim().is_empty() {
                items.push(Item {
                    kind: "TACHYON_TEXT",
                    text: value.clone(),
                    indent,
                    ..Item::default()
                });
            }
        }
        NativeNode::WebSurface {
            id,
            location,
            reason,
            accessibility,
            ..
        } => items.push(Item {
            kind: "TACHYON_SURFACE",
            text: reason.clone(),
            label: accessibility
                .as_ref()
                .and_then(|value| value.label.clone())
                .unwrap_or_else(|| String::from("Web content")),
            identifier: id.clone().unwrap_or_default(),
            location: location.clone(),
            indent,
            ..Item::default()
        }),
        NativeNode::NativeElement {
            id,
            adapter,
            properties,
            accessibility,
            children,
            ..
        } => {
            let text = element_text(node);
            let label = accessibility
                .as_ref()
                .and_then(|value| value.label.clone())
                .unwrap_or_else(|| text.clone());
            let property = |name: &str| {
                properties
                    .get(name)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned()
            };
            let base = Item {
                text: text.clone(),
                label,
                identifier: id.clone().unwrap_or_default(),
                binding: property("binding"),
                action: property("action"),
                indent,
                ..Item::default()
            };
            let (kind, descend) = match adapter.as_str() {
                "text.heading1" | "text.heading2" => ("TACHYON_HEADING", false),
                "text.heading3" | "text.heading4" | "text.heading5" | "text.heading6" => {
                    ("TACHYON_SUBHEADING", false)
                }
                "control.button" => ("TACHYON_BUTTON", false),
                "control.text_field" => ("TACHYON_FIELD", false),
                "content.output" if !base.binding.is_empty() => ("TACHYON_OUTPUT", false),
                "content.text" | "content.output" => ("TACHYON_TEXT", false),
                "control.disclosure" => ("TACHYON_DISCLOSURE", true),
                "navigation.link" => ("TACHYON_LINK", false),
                "content.image" => ("TACHYON_IMAGE", false),
                "content.divider" => ("TACHYON_DIVIDER", false),
                _ => ("", true),
            };
            let child_indent = if kind.is_empty() {
                items
                    .last()
                    .map_or(indent, |_| indent.saturating_add(u32::from(indent < 8)))
            } else {
                items.push(Item { kind, ..base });
                indent.saturating_add(1)
            };
            if descend || kind.is_empty() {
                for child in children {
                    lower(child, child_indent, items)?;
                }
            }
        }
    }
    Ok(())
}

/// Returns the concatenated visible text of one Native UI node.
fn element_text(node: &NativeNode) -> String {
    match node {
        NativeNode::Text { value } => value.clone(),
        NativeNode::WebSurface { .. } => String::new(),
        NativeNode::NativeElement { children, .. } => children.iter().map(element_text).collect(),
    }
}

fn c_source(
    application: &NativeApplication,
    route: &PlannedNativeRoute,
) -> Result<String, Failure> {
    let mut items = Vec::new();
    lower(&route.native_ui.root, 0, &mut items)?;

    let mut table = String::new();
    for item in &items {
        let _ = writeln!(
            table,
            "  {{{kind}, \"{text}\", \"{label}\", \"{identifier}\", \"{binding}\", \"{action}\", \"{location}\", {indent}}},",
            kind = item.kind,
            text = c_string_escape(&item.text),
            label = c_string_escape(&item.label),
            identifier = c_string_escape(&item.identifier),
            binding = c_string_escape(&item.binding),
            action = c_string_escape(&item.action),
            location = c_string_escape(&item.location),
            indent = item.indent,
        );
    }
    if table.is_empty() {
        table.push_str("  {TACHYON_TEXT, \"\", \"\", \"\", \"\", \"\", \"\", 0},\n");
    }

    let mut state = String::new();
    for (key, value) in &route.initial_state {
        let _ = writeln!(
            state,
            "  {{\"{key}\", \"{value}\"}},",
            key = c_string_escape(key),
            value = c_string_escape(value),
        );
    }
    if state.is_empty() {
        state.push_str("  {NULL, NULL},\n");
    }

    Ok(C_HOST
        .replace("__APP_NAME__", &c_string_escape(&application.name))
        .replace(
            "__BUNDLE_ID__",
            &c_string_escape(&application.application_id),
        )
        .replace("__ROUTE__", &c_string_escape(&route.route))
        .replace("__ITEMS__", table.trim_end())
        .replace("__STATE__", state.trim_end()))
}

const C_HOST: &str = r#"#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#define WIN32_LEAN_AND_MEAN

/* windows.h must precede the other platform headers; they depend on its types. */
#include <windows.h>

#include <commctrl.h>
#include <shellapi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TACHYON_APP_NAME "__APP_NAME__"
#define TACHYON_BUNDLE_ID "__BUNDLE_ID__"
#define TACHYON_ROUTE "__ROUTE__"
#define TACHYON_MAX_STATE_BYTES 4096
#define TACHYON_FIRST_CONTROL 1000

enum TachyonKind {
  TACHYON_TEXT = 0,
  TACHYON_HEADING,
  TACHYON_SUBHEADING,
  TACHYON_BUTTON,
  TACHYON_FIELD,
  TACHYON_OUTPUT,
  TACHYON_DISCLOSURE,
  TACHYON_LINK,
  TACHYON_IMAGE,
  TACHYON_DIVIDER,
  TACHYON_SURFACE
};

typedef struct {
  int kind;
  const char *text;
  const char *label;
  const char *id;
  const char *binding;
  const char *action;
  const char *location;
  unsigned indent;
} TachyonItem;

typedef struct {
  const char *key;
  const char *value;
} TachyonStateEntry;

static const TachyonItem TACHYON_ITEMS[] = {
__ITEMS__
};

static TachyonStateEntry TACHYON_STATE[] = {
__STATE__
};

static const size_t TACHYON_ITEM_COUNT = sizeof(TACHYON_ITEMS) / sizeof(TACHYON_ITEMS[0]);
static const size_t TACHYON_STATE_COUNT = sizeof(TACHYON_STATE) / sizeof(TACHYON_STATE[0]);

static char g_state_values[64][TACHYON_MAX_STATE_BYTES];
static HWND g_controls[512];
static size_t g_control_count = 0;
static const char *g_lifecycle = "created";

static void tachyon_record(const char *event) {
  char base[MAX_PATH];
  DWORD length = GetEnvironmentVariableA("LOCALAPPDATA", base, MAX_PATH);
  if (length == 0 || length >= MAX_PATH) {
    return;
  }
  /* Destinations are sized above the largest possible expansion so the
     composition is provably never truncated. */
  char directory[MAX_PATH + 16];
  snprintf(directory, sizeof(directory), "%s\\Tachyon", base);
  CreateDirectoryA(directory, NULL);
  char path[MAX_PATH + 320];
  snprintf(path, sizeof(path), "%s\\%s.jsonl", directory, TACHYON_BUNDLE_ID);
  FILE *handle = fopen(path, "a");
  if (handle == NULL) {
    return;
  }
  fprintf(handle, "{\"event\":\"%.128s\",\"route\":\"%.256s\"}\n", event, TACHYON_ROUTE);
  fclose(handle);
}

static wchar_t *tachyon_widen(const char *value) {
  int length = MultiByteToWideChar(CP_UTF8, 0, value, -1, NULL, 0);
  if (length <= 0) {
    return NULL;
  }
  wchar_t *wide = (wchar_t *)calloc((size_t)length, sizeof(wchar_t));
  if (wide == NULL) {
    return NULL;
  }
  MultiByteToWideChar(CP_UTF8, 0, value, -1, wide, length);
  return wide;
}

static int tachyon_state_index(const char *key) {
  if (key == NULL || *key == '\0') {
    return -1;
  }
  for (size_t index = 0; index < TACHYON_STATE_COUNT && index < 64; index += 1) {
    if (TACHYON_STATE[index].key != NULL && strcmp(TACHYON_STATE[index].key, key) == 0) {
      return (int)index;
    }
  }
  return -1;
}

static const char *tachyon_state_get(const char *key) {
  int index = tachyon_state_index(key);
  return index < 0 ? "" : g_state_values[index];
}

static void tachyon_state_set(const char *key, const char *value) {
  int index = tachyon_state_index(key);
  if (index < 0) {
    return;
  }
  snprintf(g_state_values[index], TACHYON_MAX_STATE_BYTES, "%s", value);
  for (size_t position = 0; position < TACHYON_ITEM_COUNT && position < g_control_count;
       position += 1) {
    const TachyonItem *item = &TACHYON_ITEMS[position];
    if (item->kind != TACHYON_OUTPUT || strcmp(item->binding, key) != 0) {
      continue;
    }
    wchar_t *wide = tachyon_widen(g_state_values[index]);
    if (wide != NULL) {
      SetWindowTextW(g_controls[position], wide);
      free(wide);
    }
  }
}

static void tachyon_dispatch(const char *action) {
  if (strcmp(g_lifecycle, "destroyed") == 0 || action == NULL || *action == '\0') {
    return;
  }
  const char *separator = strchr(action, ':');
  if (separator == NULL) {
    return;
  }
  size_t verb_length = (size_t)(separator - action);
  const char *key = separator + 1;
  int index = tachyon_state_index(key);
  if (index < 0) {
    return;
  }
  if (verb_length == 9 && strncmp(action, "increment", 9) == 0) {
    char *end = NULL;
    long long value = strtoll(g_state_values[index], &end, 10);
    if (end != NULL && *end == '\0') {
      char next[TACHYON_MAX_STATE_BYTES];
      snprintf(next, sizeof(next), "%lld", value + 1);
      tachyon_state_set(key, next);
      tachyon_record("state.increment");
    }
  } else if (verb_length == 6 && strncmp(action, "toggle", 6) == 0) {
    tachyon_state_set(key, strcmp(g_state_values[index], "true") == 0 ? "false" : "true");
    tachyon_record("state.toggle");
  }
}

static void tachyon_open_surface(const char *location) {
  if (location == NULL || strncmp(location, "https://", 8) != 0) {
    return;
  }
  wchar_t *wide = tachyon_widen(location);
  if (wide == NULL) {
    return;
  }
  ShellExecuteW(NULL, L"open", wide, NULL, NULL, SW_SHOWNORMAL);
  free(wide);
  tachyon_record("websurface.externalized");
}

static void tachyon_create_controls(HWND parent, HINSTANCE instance) {
  HFONT font = (HFONT)GetStockObject(DEFAULT_GUI_FONT);
  int y = 16;
  for (size_t index = 0; index < TACHYON_ITEM_COUNT && index < 512; index += 1) {
    const TachyonItem *item = &TACHYON_ITEMS[index];
    const char *display = item->kind == TACHYON_OUTPUT ? tachyon_state_get(item->binding)
                                                       : item->text;
    if (item->kind == TACHYON_FIELD) {
      display = tachyon_state_get(item->binding);
    }
    wchar_t *wide = tachyon_widen(display != NULL ? display : "");
    if (wide == NULL) {
      continue;
    }
    int x = 16 + (int)(item->indent > 8 ? 8 : item->indent) * 12;
    int width = 560 - x;
    int height = 24;
    const wchar_t *class_name = L"STATIC";
    DWORD style = WS_CHILD | WS_VISIBLE;

    switch (item->kind) {
      case TACHYON_BUTTON:
      case TACHYON_LINK:
      case TACHYON_DISCLOSURE:
        class_name = L"BUTTON";
        style |= BS_PUSHBUTTON | WS_TABSTOP;
        width = 220;
        height = 30;
        break;
      case TACHYON_FIELD:
        class_name = L"EDIT";
        style |= WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL;
        height = 26;
        break;
      case TACHYON_HEADING:
        height = 34;
        break;
      case TACHYON_DIVIDER:
        style |= SS_ETCHEDHORZ;
        height = 2;
        break;
      case TACHYON_SURFACE:
        style |= SS_SUNKEN;
        height = 56;
        break;
      default:
        break;
    }

    HWND control = CreateWindowExW(0, class_name, wide, style, x, y, width, height, parent,
                                   (HMENU)(UINT_PTR)(TACHYON_FIRST_CONTROL + index), instance,
                                   NULL);
    free(wide);
    if (control == NULL) {
      continue;
    }
    SendMessageW(control, WM_SETFONT, (WPARAM)font, TRUE);

    /* Win32 derives a control's accessible name from its window text, so a
       control with no visible text of its own carries the declared name. */
    if (item->kind == TACHYON_SURFACE || item->kind == TACHYON_IMAGE) {
      wchar_t *label = tachyon_widen(*item->label != '\0' ? item->label : item->text);
      if (label != NULL) {
        SetWindowTextW(control, label);
        free(label);
      }
    }
    g_controls[index] = control;
    g_control_count = index + 1;
    y += height + 10;
  }
  tachyon_record("controller.mounted");
}

static LRESULT CALLBACK tachyon_window_procedure(HWND window, UINT message, WPARAM wparam,
                                                 LPARAM lparam) {
  switch (message) {
    case WM_COMMAND: {
      size_t index = (size_t)LOWORD(wparam) - TACHYON_FIRST_CONTROL;
      if (index >= TACHYON_ITEM_COUNT) {
        break;
      }
      const TachyonItem *item = &TACHYON_ITEMS[index];
      if (HIWORD(wparam) == BN_CLICKED) {
        if (item->kind == TACHYON_SURFACE) {
          tachyon_open_surface(item->location);
        } else {
          tachyon_dispatch(item->action);
        }
        return 0;
      }
      if (HIWORD(wparam) == EN_CHANGE && item->kind == TACHYON_FIELD) {
        wchar_t buffer[TACHYON_MAX_STATE_BYTES];
        GetWindowTextW((HWND)lparam, buffer, TACHYON_MAX_STATE_BYTES);
        char narrow[TACHYON_MAX_STATE_BYTES];
        WideCharToMultiByte(CP_UTF8, 0, buffer, -1, narrow, TACHYON_MAX_STATE_BYTES, NULL, NULL);
        int state = tachyon_state_index(item->binding);
        if (state >= 0) {
          snprintf(g_state_values[state], TACHYON_MAX_STATE_BYTES, "%s", narrow);
          tachyon_record("state.input");
        }
        return 0;
      }
      break;
    }
    case WM_ACTIVATE:
      g_lifecycle = LOWORD(wparam) == WA_INACTIVE ? "suspended" : "active";
      tachyon_record(LOWORD(wparam) == WA_INACTIVE ? "controller.suspended"
                                                   : "controller.active");
      return 0;
    case WM_DESTROY:
      g_lifecycle = "destroyed";
      tachyon_record("controller.destroyed");
      PostQuitMessage(0);
      return 0;
    default:
      break;
  }
  return DefWindowProcW(window, message, wparam, lparam);
}

int WINAPI wWinMain(HINSTANCE instance, HINSTANCE previous, PWSTR arguments, int show) {
  (void)previous;
  (void)arguments;
  tachyon_record("controller.created");
  for (size_t index = 0; index < TACHYON_STATE_COUNT && index < 64; index += 1) {
    if (TACHYON_STATE[index].value != NULL) {
      snprintf(g_state_values[index], TACHYON_MAX_STATE_BYTES, "%s",
               TACHYON_STATE[index].value);
    }
  }

  INITCOMMONCONTROLSEX controls = {sizeof(INITCOMMONCONTROLSEX), ICC_STANDARD_CLASSES};
  InitCommonControlsEx(&controls);

  WNDCLASSEXW window_class = {0};
  window_class.cbSize = sizeof(WNDCLASSEXW);
  window_class.lpfnWndProc = tachyon_window_procedure;
  window_class.hInstance = instance;
  window_class.hCursor = LoadCursorW(NULL, IDC_ARROW);
  window_class.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
  window_class.lpszClassName = L"TachyonNativeHost";
  if (RegisterClassExW(&window_class) == 0) {
    return 1;
  }

  wchar_t *title = tachyon_widen(TACHYON_APP_NAME);
  HWND window = CreateWindowExW(0, L"TachyonNativeHost", title != NULL ? title : L"Tachyon",
                                WS_OVERLAPPEDWINDOW | WS_VSCROLL, CW_USEDEFAULT, CW_USEDEFAULT,
                                620, 820, NULL, NULL, instance, NULL);
  free(title);
  if (window == NULL) {
    return 1;
  }

  tachyon_create_controls(window, instance);
  ShowWindow(window, show);
  UpdateWindow(window);

  MSG message;
  while (GetMessageW(&message, NULL, 0, 0) > 0) {
    if (!IsDialogMessageW(window, &message)) {
      TranslateMessage(&message);
      DispatchMessageW(&message);
    }
  }
  return 0;
}
"#;

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{application_manifest, c_source, compiler_path, lower, windows_compiler};
    use crate::native::config::NativeApplication;
    use crate::native::planner::NativePlanner;
    use tachyon_contracts::NativeTarget;

    fn application() -> NativeApplication {
        NativeApplication {
            name: String::from("Native Catalog"),
            executable_name: String::from("NativeCatalog"),
            application_id: String::from("dev.tachyon.native-catalog"),
            version: String::from("1.0.0"),
            entry_route: String::from("/"),
        }
    }

    fn route() -> crate::native::planner::PlannedNativeRoute {
        NativePlanner::plan(
            NativeTarget::Windows,
            "/",
            "client/pages/tac.html",
            r#"<main aria-label="Demo"><h1>Catalog</h1><button aria-label="Increase" data-tachyon-action="increment:count">Add</button><output aria-label="Count" data-tachyon-bind="count" data-tachyon-state="0">0</output><input aria-label="Name" data-tachyon-bind="name" data-tachyon-state=""><x-chart aria-label="Chart">Chart</x-chart></main>"#,
            "",
        )
        .expect("plan")
    }

    #[test]
    fn lowering_produces_a_deterministic_bounded_control_table() {
        let planned = route();
        let mut items = Vec::new();
        lower(&planned.native_ui.root, 0, &mut items).expect("lower");
        let kinds: Vec<&str> = items.iter().map(|item| item.kind).collect();
        assert!(kinds.contains(&"TACHYON_HEADING"));
        assert!(kinds.contains(&"TACHYON_BUTTON"));
        assert!(kinds.contains(&"TACHYON_OUTPUT"));
        assert!(kinds.contains(&"TACHYON_FIELD"));
        assert!(kinds.contains(&"TACHYON_SURFACE"));

        let mut repeated = Vec::new();
        lower(&planned.native_ui.root, 0, &mut repeated).expect("lower");
        let repeated_kinds: Vec<&str> = repeated.iter().map(|item| item.kind).collect();
        assert_eq!(kinds, repeated_kinds);
    }

    #[test]
    fn generated_host_embeds_state_actions_and_lifecycle() {
        let source = c_source(&application(), &route()).expect("source");
        assert!(source.contains("controller.created"));
        assert!(source.contains("controller.destroyed"));
        assert!(source.contains(r#"{"count", "0"}"#));
        assert!(source.contains("increment:count"));
        assert!(source.contains("TACHYON_SURFACE"));
        assert!(source.contains("ShellExecuteW"));
    }

    #[test]
    fn manifest_requests_common_controls_and_per_monitor_dpi() {
        let manifest = application_manifest(&application());
        assert!(manifest.contains("Microsoft.Windows.Common-Controls"));
        assert!(manifest.contains("PerMonitorV2"));
        assert!(manifest.contains("dev-tachyon-native-catalog"));
    }

    #[test]
    fn compiler_selection_follows_the_build_machine() {
        let compiler = windows_compiler();
        assert!(compiler == "gcc" || compiler == "x86_64-w64-mingw32-gcc");
    }

    #[test]
    fn mingw_paths_use_forward_slashes_on_every_build_machine() {
        let path = compiler_path(std::path::Path::new(r"\\?\D:\work\tachyon_host.c"), "Host")
            .expect("path");
        assert_eq!(path, "D:/work/tachyon_host.c");
    }
}
