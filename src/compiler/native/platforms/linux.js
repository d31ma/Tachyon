// @ts-check
import path from 'path';
import { mkdir, writeFile } from 'fs/promises';
import PlatformGenerator from '../platform-generator.js';

/**
 * Generates a buildable Linux host project using GTK and WebKitGTK.
 *
 * Output layout:
 *   <outputRoot>/
 *     Resources/                 # copied Tac assets
 *     src/
 *       main.c
 *     CMakeLists.txt
 *     build.sh
 *     README.md
 *     tachyon.host.json
 */
export default class LinuxGenerator extends PlatformGenerator {
    async generateProjectFiles() {
        const srcDir = path.join(this.outputRoot, 'src');
        await mkdir(srcDir, { recursive: true });

        await writeFile(path.join(srcDir, 'main.c'), this.cSource());
        await writeFile(path.join(this.outputRoot, 'CMakeLists.txt'), this.cmakeLists());
        await this.writeExecutable('build.sh', this.buildScript());
    }

    cSource() {
        const appName = this.appName.replace(/"/g, '\\"');
        const bridgeScript = this.getBridgeScript().replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n');
        const allowedCapabilityExpression = this.nativeCapabilities
            .map(({ capability }) => `strcmp(capability, ${JSON.stringify(capability)}) == 0`)
            .join(' || ') || '0';
        return `#include <gtk/gtk.h>
#include <webkit2/webkit2.h>
#include <dirent.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/select.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define APP_NAME "${appName}"

static char* get_resource_dir() {
    static char path[PATH_MAX];
    ssize_t len = readlink("/proc/self/exe", path, sizeof(path) - 1);
    if (len == -1) {
        strcpy(path, ".");
        return path;
    }
    path[len] = '\\0';
    char* last_slash = strrchr(path, '/');
    if (last_slash) *last_slash = '\\0';
    return path;
}

static char* get_index_path() {
    char* dir = get_resource_dir();
    char* result = malloc(PATH_MAX);
    snprintf(result, PATH_MAX, "file://%s/Resources/index.html", dir);
    return result;
}

static char* json_escape(const char* input) {
    if (!input) input = "";
    size_t len = strlen(input);
    char* out = malloc((len * 2) + 1);
    size_t j = 0;
    for (size_t i = 0; i < len; i++) {
        if (input[i] == '\\\\' || input[i] == '"') out[j++] = '\\\\';
        if (input[i] == '\\n') {
            out[j++] = '\\\\';
            out[j++] = 'n';
        } else {
            out[j++] = input[i];
        }
    }
    out[j] = '\\0';
    return out;
}

static char* extract_json_string(const char* json, const char* key) {
    char pattern[128];
    snprintf(pattern, sizeof(pattern), "\\"%s\\"", key);
    const char* pos = strstr(json, pattern);
    if (!pos) return NULL;
    pos = strchr(pos + strlen(pattern), ':');
    if (!pos) return NULL;
    pos++;
    while (*pos == ' ' || *pos == '\\t' || *pos == '\\n') pos++;
    if (*pos != '"') return NULL;
    pos++;
    char* out = malloc(strlen(pos) + 1);
    size_t j = 0;
    while (*pos && *pos != '"') {
        if (*pos == '\\\\' && pos[1]) {
            pos++;
            out[j++] = *pos == 'n' ? '\\n' : *pos;
        } else {
            out[j++] = *pos;
        }
        pos++;
    }
    out[j] = '\\0';
    return out;
}

static char* extract_json_value(const char* json, const char* key) {
    char pattern[128];
    snprintf(pattern, sizeof(pattern), "\\"%s\\"", key);
    const char* pos = strstr(json, pattern);
    if (!pos) return NULL;
    pos = strchr(pos + strlen(pattern), ':');
    if (!pos) return NULL;
    pos++;
    while (*pos == ' ' || *pos == '\\t' || *pos == '\\n') pos++;

    const char* start = pos;
    if (*pos == '"') {
        pos++;
        while (*pos) {
            if (*pos == '\\\\' && pos[1]) { pos += 2; continue; }
            if (*pos == '"') { pos++; break; }
            pos++;
        }
    } else if (*pos == '{') {
        int depth = 0;
        while (*pos) {
            if (*pos == '"') {
                pos++;
                while (*pos) {
                    if (*pos == '\\\\' && pos[1]) { pos += 2; continue; }
                    if (*pos == '"') { pos++; break; }
                    pos++;
                }
                continue;
            }
            if (*pos == '{') depth++;
            if (*pos == '}') { depth--; pos++; if (depth == 0) break; }
            else pos++;
        }
    } else if (*pos == '[') {
        int depth = 0;
        while (*pos) {
            if (*pos == '"') {
                pos++;
                while (*pos) {
                    if (*pos == '\\\\' && pos[1]) { pos += 2; continue; }
                    if (*pos == '"') { pos++; break; }
                    pos++;
                }
                continue;
            }
            if (*pos == '[') depth++;
            if (*pos == ']') { depth--; pos++; if (depth == 0) break; }
            else pos++;
        }
    } else {
        while (*pos && *pos != ',' && *pos != '}' && *pos != ']' && !isspace((unsigned char)*pos)) pos++;
    }

    size_t len = (size_t)(pos - start);
    char* out = malloc(len + 1);
    memcpy(out, start, len);
    out[len] = '\\0';
    return out;
}

static int extract_json_int(const char* json, const char* key) {
    char pattern[128];
    snprintf(pattern, sizeof(pattern), "\\"%s\\"", key);
    const char* pos = strstr(json, pattern);
    if (!pos) return 0;
    pos = strchr(pos + strlen(pattern), ':');
    if (!pos) return 0;
    return atoi(pos + 1);
}

static char* read_text_file(const char* file_path) {
    FILE* file = fopen(file_path, "rb");
    if (!file) return NULL;
    fseek(file, 0, SEEK_END);
    long size = ftell(file);
    fseek(file, 0, SEEK_SET);
    char* buffer = malloc((size_t)size + 1);
    size_t read = fread(buffer, 1, (size_t)size, file);
    buffer[read] = '\\0';
    fclose(file);
    return buffer;
}

static int write_text_file(const char* file_path, const char* text) {
    FILE* file = fopen(file_path, "wb");
    if (!file) return -1;
    fwrite(text, 1, strlen(text), file);
    fclose(file);
    return 0;
}

static char** extract_json_string_array(const char* json, const char* key, int* count) {
    *count = 0;
    char pattern[128];
    snprintf(pattern, sizeof(pattern), "\\"%s\\"", key);
    const char* pos = strstr(json, pattern);
    if (!pos) return NULL;
    pos = strchr(pos + strlen(pattern), ':');
    if (!pos) return NULL;
    pos++;
    while (*pos == ' ' || *pos == '\\t' || *pos == '\\n') pos++;
    if (*pos != '[') return NULL;
    pos++;
    int capacity = 4;
    char** values = calloc((size_t)capacity, sizeof(char*));
    while (*pos && *pos != ']') {
        while (*pos == ' ' || *pos == '\\t' || *pos == '\\n' || *pos == ',') pos++;
        if (*pos == ']') break;
        if (*pos != '"') break;
        pos++;
        char* out = malloc(strlen(pos) + 1);
        size_t j = 0;
        while (*pos && *pos != '"') {
            if (*pos == '\\\\' && pos[1]) {
                pos++;
                out[j++] = *pos == 'n' ? '\\n' : *pos;
            } else {
                out[j++] = *pos;
            }
            pos++;
        }
        out[j] = '\\0';
        if (*pos == '"') pos++;
        if (*count >= capacity) {
            capacity *= 2;
            values = realloc(values, (size_t)capacity * sizeof(char*));
        }
        values[*count] = out;
        *count += 1;
    }
    return values;
}

static void free_string_array(char** values, int count) {
    if (!values) return;
    for (int i = 0; i < count; i++) free(values[i]);
    free(values);
}

static char* run_shell_command_json(const char* command, char** args, int arg_count, const char* cwd) {
    int stdout_pipe[2];
    int stderr_pipe[2];
    if (pipe(stdout_pipe) != 0 || pipe(stderr_pipe) != 0) return NULL;
    pid_t pid = fork();
    if (pid == 0) {
        close(stdout_pipe[0]);
        close(stderr_pipe[0]);
        dup2(stdout_pipe[1], STDOUT_FILENO);
        dup2(stderr_pipe[1], STDERR_FILENO);
        close(stdout_pipe[1]);
        close(stderr_pipe[1]);
        if (cwd && strlen(cwd) > 0) chdir(cwd);
        char** argv = calloc((size_t)arg_count + 2, sizeof(char*));
        argv[0] = (char*)command;
        for (int i = 0; i < arg_count; i++) argv[i + 1] = args[i];
        argv[arg_count + 1] = NULL;
        execvp(command, argv);
        _exit(127);
    }
    close(stdout_pipe[1]);
    close(stderr_pipe[1]);
    GString* stdout_text = g_string_new("");
    GString* stderr_text = g_string_new("");
    int stdout_open = 1;
    int stderr_open = 1;
    while (stdout_open || stderr_open) {
        fd_set read_fds;
        FD_ZERO(&read_fds);
        int max_fd = -1;
        if (stdout_open) {
            FD_SET(stdout_pipe[0], &read_fds);
            if (stdout_pipe[0] > max_fd) max_fd = stdout_pipe[0];
        }
        if (stderr_open) {
            FD_SET(stderr_pipe[0], &read_fds);
            if (stderr_pipe[0] > max_fd) max_fd = stderr_pipe[0];
        }
        if (select(max_fd + 1, &read_fds, NULL, NULL, NULL) <= 0) break;
        if (stdout_open && FD_ISSET(stdout_pipe[0], &read_fds)) {
            char buffer[512];
            ssize_t read_count = read(stdout_pipe[0], buffer, sizeof(buffer));
            if (read_count > 0) g_string_append_len(stdout_text, buffer, read_count);
            else stdout_open = 0;
        }
        if (stderr_open && FD_ISSET(stderr_pipe[0], &read_fds)) {
            char buffer[512];
            ssize_t read_count = read(stderr_pipe[0], buffer, sizeof(buffer));
            if (read_count > 0) g_string_append_len(stderr_text, buffer, read_count);
            else stderr_open = 0;
        }
    }
    close(stdout_pipe[0]);
    close(stderr_pipe[0]);
    int status = 0;
    waitpid(pid, &status, 0);
    char* escaped_command = json_escape(command);
    char* escaped_cwd = json_escape(cwd ? cwd : "");
    char* escaped_stdout = json_escape(stdout_text->str);
    char* escaped_stderr = json_escape(stderr_text->str);
    GString* args_json = g_string_new("[");
    for (int i = 0; i < arg_count; i++) {
        char* escaped_arg = json_escape(args[i]);
        g_string_append_printf(args_json, "%s\\"%s\\"", i == 0 ? "" : ",", escaped_arg);
        free(escaped_arg);
    }
    g_string_append(args_json, "]");
    char* value = g_strdup_printf("{\\"command\\":\\"%s\\",\\"args\\":%s,\\"cwd\\":\\"%s\\",\\"exitCode\\":%d,\\"stdout\\":\\"%s\\",\\"stderr\\":\\"%s\\"}", escaped_command, args_json->str, escaped_cwd, WIFEXITED(status) ? WEXITSTATUS(status) : -1, escaped_stdout, escaped_stderr);
    free(escaped_command);
    free(escaped_cwd);
    free(escaped_stdout);
    free(escaped_stderr);
    g_string_free(args_json, TRUE);
    g_string_free(stdout_text, TRUE);
    g_string_free(stderr_text, TRUE);
    return value;
}

static void handle_native_message(WebKitWebView* web_view, const char* message) {
    int id = extract_json_int(message, "id");
    char* capability = extract_json_string(message, "capability");
    if (!capability) {
        send_response(web_view, id, 0, NULL, "Missing native capability");
        return;
    }
    if (!(${allowedCapabilityExpression})) {
        send_response(web_view, id, 0, NULL, "Native capability is not enabled");
        free(capability);
        return;
    }
    if (strcmp(capability, "app.info") == 0) {
        char* value = g_strdup_printf("{\\"name\\":\\"%s\\",\\"runtime\\":\\"linux-webkitgtk\\"}", APP_NAME);
        send_response(web_view, id, 1, value, NULL);
        g_free(value);
    } else if (strcmp(capability, "clipboard.readText") == 0) {
        GtkClipboard* clipboard = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
        gchar* text = gtk_clipboard_wait_for_text(clipboard);
        char* escaped = json_escape(text ? text : "");
        char* value = g_strdup_printf("\\\"%s\\\"", escaped);
        send_response(web_view, id, 1, value, NULL);
        g_free(text);
        free(escaped);
        g_free(value);
    } else if (strcmp(capability, "clipboard.writeText") == 0) {
        char* text = extract_json_string(message, "text");
        GtkClipboard* clipboard = gtk_clipboard_get(GDK_SELECTION_CLIPBOARD);
        gtk_clipboard_set_text(clipboard, text ? text : "", -1);
        send_response(web_view, id, 1, "{\\\"written\\\":true}", NULL);
        free(text);
    } else if (strcmp(capability, "openUrl") == 0) {
        char* url = extract_json_string(message, "url");
        GError* error = NULL;
        gboolean safe_url = url && (strncmp(url, "http://", 7) == 0 || strncmp(url, "https://", 8) == 0);
        gboolean opened = safe_url && gtk_show_uri_on_window(NULL, url, GDK_CURRENT_TIME, &error);
        if (!opened) {
            send_response(web_view, id, 0, NULL, error ? error->message : safe_url ? "Unable to open the external URL" : "openUrl requires an http(s) URL");
        } else {
            send_response(web_view, id, 1, "{\\\"opened\\\":true}", NULL);
        }
        if (error) g_error_free(error);
        free(url);
    } else if (strcmp(capability, "fs.readText") == 0) {
        char* file_path = extract_json_string(message, "path");
        char* text = file_path ? read_text_file(file_path) : NULL;
        if (!file_path || !text) {
            send_response(web_view, id, 0, NULL, "Unable to read file");
        } else {
            char* escaped_path = json_escape(file_path);
            char* escaped_text = json_escape(text);
            char* value = g_strdup_printf("{\\"path\\":\\"%s\\",\\"text\\":\\"%s\\"}", escaped_path, escaped_text);
            send_response(web_view, id, 1, value, NULL);
            free(escaped_path);
            free(escaped_text);
            g_free(value);
        }
        free(file_path);
        free(text);
    } else if (strcmp(capability, "fs.writeText") == 0) {
        char* file_path = extract_json_string(message, "path");
        char* text = extract_json_string(message, "text");
        if (!file_path || write_text_file(file_path, text ? text : "") != 0) {
            send_response(web_view, id, 0, NULL, "Unable to write file");
        } else {
            char* escaped_path = json_escape(file_path);
            char* value = g_strdup_printf("{\\"path\\":\\"%s\\",\\"bytes\\":%zu,\\"written\\":true}", escaped_path, strlen(text ? text : ""));
            send_response(web_view, id, 1, value, NULL);
            free(escaped_path);
            g_free(value);
        }
        free(file_path);
        free(text);
    } else if (strcmp(capability, "fs.readDir") == 0) {
        char* dir_path = extract_json_string(message, "path");
        DIR* dir = dir_path ? opendir(dir_path) : NULL;
        if (!dir) {
            send_response(web_view, id, 0, NULL, "Unable to read directory");
        } else {
            GString* entries = g_string_new("[");
            struct dirent* entry;
            int first = 1;
            while ((entry = readdir(dir)) != NULL) {
                if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) continue;
                char* escaped_name = json_escape(entry->d_name);
                g_string_append_printf(entries, "%s{\\"name\\":\\"%s\\",\\"type\\":\\"%s\\"}", first ? "" : ",", escaped_name, entry->d_type == DT_DIR ? "directory" : "file");
                first = 0;
                free(escaped_name);
            }
            closedir(dir);
            g_string_append(entries, "]");
            char* escaped_path = json_escape(dir_path);
            char* value = g_strdup_printf("{\\"path\\":\\"%s\\",\\"entries\\":%s}", escaped_path, entries->str);
            send_response(web_view, id, 1, value, NULL);
            free(escaped_path);
            g_string_free(entries, TRUE);
            g_free(value);
        }
        free(dir_path);
    } else if (strcmp(capability, "shell.exec") == 0) {
        char* command = extract_json_string(message, "command");
        if (!command) {
            send_response(web_view, id, 0, NULL, "Missing shell command");
        } else {
            int arg_count = 0;
            char** args = extract_json_string_array(message, "args", &arg_count);
            char* cwd = extract_json_string(message, "cwd");
            char* value = run_shell_command_json(command, args, arg_count, cwd);
            if (!value) {
                send_response(web_view, id, 0, NULL, "Unable to execute command");
            } else {
                send_response(web_view, id, 1, value, NULL);
                g_free(value);
            }
            free_string_array(args, arg_count);
            free(cwd);
            free(command);
        }
    } else {
        send_response(web_view, id, 0, NULL, "Unsupported native capability");
    }
    free(capability);
}

static void on_message(WebKitUserContentManager* manager,
                       WebKitJavascriptResult* result,
                       gpointer user_data) {
    (void)manager;
    JSCValue* value = webkit_javascript_result_get_js_value(result);
    gchar* message = jsc_value_to_string(value);
    handle_native_message(WEBKIT_WEB_VIEW(user_data), message);
    g_free(message);
}

int main(int argc, char* argv[]) {
    gtk_init(&argc, &argv);

    GtkWidget* window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), APP_NAME);
    gtk_window_set_default_size(GTK_WINDOW(window), 1280, 800);
    char icon_path[PATH_MAX];
    snprintf(icon_path, PATH_MAX, "%s/Resources/TachyonIcon.png", get_resource_dir());
    gtk_window_set_icon_from_file(GTK_WINDOW(window), icon_path, NULL);
    g_signal_connect(window, "destroy", G_CALLBACK(gtk_main_quit), NULL);

    WebKitWebView* webView = WEBKIT_WEB_VIEW(webkit_web_view_new());
    gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(webView));

    WebKitUserContentManager* contentManager = webkit_web_view_get_user_content_manager(webView);
    WebKitUserScript* script = webkit_user_script_new(
        "${bridgeScript}",
        WEBKIT_USER_CONTENT_INJECT_TOP_FRAME,
        WEBKIT_USER_SCRIPT_INJECT_AT_DOCUMENT_START,
        NULL, NULL);
    webkit_user_content_manager_add_script(contentManager, script);
    g_signal_connect(contentManager, "script-message-received::tachyon", G_CALLBACK(on_message), webView);
    webkit_user_content_manager_register_script_message_handler(contentManager, "tachyon");

    char* indexPath = get_index_path();
    webkit_web_view_load_uri(webView, indexPath);
    free(indexPath);

    gtk_widget_show_all(window);
    gtk_main();

    return 0;
}
`;
    }

    cmakeLists() {
        return `cmake_minimum_required(VERSION 3.16)
project(${this.appName} C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(PkgConfig REQUIRED)
pkg_check_modules(GTK3 REQUIRED gtk+-3.0)
pkg_check_modules(WEBKIT2 REQUIRED webkit2gtk-4.1)

add_executable(\${PROJECT_NAME} src/main.c)

target_include_directories(\${PROJECT_NAME} PRIVATE
    \${GTK3_INCLUDE_DIRS}
    \${WEBKIT2_INCLUDE_DIRS}
)

target_compile_options(\${PROJECT_NAME} PRIVATE
    \${GTK3_CFLAGS_OTHER}
    \${WEBKIT2_CFLAGS_OTHER}
)

target_link_libraries(\${PROJECT_NAME} PRIVATE
    \${GTK3_LIBRARIES}
    \${WEBKIT2_LIBRARIES}
)

set_target_properties(\${PROJECT_NAME} PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY \${CMAKE_BINARY_DIR}/\${PROJECT_NAME}
)

add_custom_command(TARGET \${PROJECT_NAME} POST_BUILD
    COMMAND \${CMAKE_COMMAND} -E copy_directory
    \${CMAKE_SOURCE_DIR}/Resources $<TARGET_FILE_DIR:\${PROJECT_NAME}>/Resources
)
`;
    }

    buildScript() {
        return `#!/bin/sh
set -e

APP_NAME="${this.appName}"
OUTPUT_ROOT="$(cd "$(dirname "$0")" && pwd)"

echo "Building $APP_NAME (Linux WebKitGTK host)..."

mkdir -p "$OUTPUT_ROOT/build"
cd "$OUTPUT_ROOT/build"
cmake ..
cmake --build .

echo "Built: $OUTPUT_ROOT/build/$APP_NAME/$APP_NAME"
echo "Run with: $OUTPUT_ROOT/build/$APP_NAME/$APP_NAME"
`;
    }

    buildReadme() {
        return `# ${this.appName} — Linux native host

This folder contains a buildable WebKitGTK host for the Tac frontend.

## Prerequisites

- GTK 3 development headers
- WebKitGTK 4.1 development headers
- CMake and a C compiler

On Debian/Ubuntu:

\`\`\`sh
sudo apt-get install libgtk-3-dev libwebkit2gtk-4.1-dev cmake gcc
\`\`\`

On Fedora:

\`\`\`sh
sudo dnf install gtk3-devel webkit2gtk4.1-devel cmake gcc
\`\`\`

## Build

\`\`\`sh
sh build.sh
\`\`\`

The executable is written to \`build/${this.appName}/${this.appName}\`.

## Run

\`\`\`sh
./build/${this.appName}/${this.appName}
\`\`\`

## Architecture

- Static Tac assets live in \`Resources/\`.
- \`src/main.c\` creates a GTK window hosting \`WebKitWebView\`.
- \`window.__tcNativeBridge__\` exposes a minimal JS↔native message contract.
`;
    }
}
