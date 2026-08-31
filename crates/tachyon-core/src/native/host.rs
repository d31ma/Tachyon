//! Platform-neutral staging shared by every Phase 5 native host generator.
//!
//! Each platform generator receives a fully staged resource tree and owns only
//! its host source, bundle layout, and toolchain invocation.

use super::routes::NativeRouteIndex;
use crate::Failure;
use crate::external_command::run as supervise_tool;
use crate::failure::diagnostic;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Upper bound on any generated host source file.
pub(super) const MAX_HOST_SOURCE_BYTES: usize = 4 * 1_024 * 1_024;
const MAX_TOOL_OUTPUT_BYTES: usize = 64 * 1_024;
/// Injected into the hosted bundle before its own scripts run.
///
/// Two jobs, and deliberately no more. It tells the page it is running
/// natively, so an application can style or behave differently without
/// sniffing a user agent. And it carries the channel between page and
/// compiled companion, in both directions.
///
/// There is no capability vocabulary here any more. A tray, a window, a
/// notification — those are the platform's, and a companion written in the
/// platform's own language already has them. A fixed list of verbs in the
/// middle could only ever be the subset Tachyon had got round to
/// implementing, in three hosts, for every platform at once.
///
/// The publish stub queues rather than delivers. A companion may publish from
/// a thread of its own at any moment, including before the page's modules have
/// run, and this file is what runs first: without the queue that value is lost
/// to a race nothing in the application can see.
pub(super) const NATIVE_SHIM: &str = r"globalThis.__tachyonCompanionQueue = []
globalThis.__tachyonCompanionPublish = (signal) => {
  const queue = globalThis.__tachyonCompanionQueue
  if (queue.length === 128) queue.shift()
  queue.push(signal)
}
globalThis.__tachyonNativeHostCall = (capability, payload) =>
  globalThis.webkit?.messageHandlers?.tachyon
    ? globalThis.webkit.messageHandlers.tachyon.postMessage({ capability, payload })
    : globalThis.__tachyonHostPost?.(capability, payload)
globalThis.tachyonWindow = Object.fromEntries(
  __WINDOW_CONTROLS__.map((name) => [
    name,
    (payload = {}) => globalThis.__tachyonNativeHostCall('window.' + name, JSON.stringify(payload)),
  ]),
)
addEventListener('DOMContentLoaded', () => {
  if (document.body) document.body.dataset.platform = 'native'
})
";

/// Builds the shim for one application, with the controls it was granted.
///
/// The list is baked in rather than probed, so `tachyonWindow.minimize` is
/// either a function or `undefined` — a page can ask what it has without
/// calling anything and being refused. Electron hands a privileged process
/// the whole window; Tauri gates each call behind a permission in a separate
/// capabilities file. This is the same default-deny, declared in the manifest
/// the application already ships.
pub(super) fn native_shim(window: &super::config::WindowConfiguration) -> String {
    let granted = window
        .controls
        .iter()
        .map(|control| format!("'{control}'"))
        .collect::<Vec<_>>()
        .join(", ");
    NATIVE_SHIM.replace("__WINDOW_CONTROLS__", &format!("[{granted}]"))
}

/// Evidence returned by one platform host generator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct GeneratedHost {
    /// Stage-relative application bundle, executable, or package.
    pub(super) application_bundle: PathBuf,
    /// Platform toolchain name recorded in Artifact Manifest v1.
    pub(super) toolchain_name: String,
    /// Platform toolchain version recorded in Artifact Manifest v1.
    pub(super) toolchain_version: String,
}

/// Writes the platform-neutral resource tree used by every native host.
///
/// The stage receives inspectable top-level copies and the platform bundle
/// receives the canonical `NativeIndex.json` and `WebBundle/` names.
pub(super) fn stage_application(
    index: &NativeRouteIndex,
    web_bundle: &Path,
    stage: &Path,
    resources: &Path,
) -> Result<(), Failure> {
    for directory in [&stage.join("web"), &resources.to_path_buf()] {
        native_io(fs::create_dir_all(directory), directory)?;
    }

    let index_bytes = pretty_json(index, "Native route index")?;
    write(&stage.join("native-index.json"), &index_bytes)?;
    write(&resources.join("NativeIndex.json"), &index_bytes)?;

    // The application's own web bundle is what the host loads. It is the same
    // bundle the browser gets, which is the point: one build, one rendering,
    // and no per-platform approximation of it.
    copy_tree(web_bundle, &stage.join("web"))?;
    copy_tree(web_bundle, &resources.join("WebBundle"))?;
    // Android's asset packager drops dot-prefixed directories, so the
    // generated runtime is published under a visible alias on every platform.
    let hidden_runtime = web_bundle.join(".tachyon");
    if hidden_runtime.is_dir() {
        copy_tree(&hidden_runtime, &stage.join("web/tachyon-runtime"))?;
        copy_tree(
            &hidden_runtime,
            &resources.join("WebBundle/tachyon-runtime"),
        )?;
    }

    Ok(())
}

/// Writes one generated host source file after enforcing the size budget.
pub(super) fn write_host_source(path: &Path, source: &str) -> Result<(), Failure> {
    if source.len() > MAX_HOST_SOURCE_BYTES {
        return Err(native_tool_failure(
            1605,
            "Generated native host exceeds the 4 MiB limit.",
        ));
    }
    write(path, source.as_bytes())
}

/// Runs one bounded native toolchain program without a shell.
pub(super) async fn run_tool(program: &str, arguments: &[&str]) -> Result<String, Failure> {
    run_tool_in(program, arguments, None).await
}

/// Runs one bounded native toolchain program in an explicit working directory.
pub(super) async fn run_tool_in(
    program: &str,
    arguments: &[&str],
    working_directory: Option<&Path>,
) -> Result<String, Failure> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
    }
    let output = supervise_tool(&mut command, Duration::from_mins(2), MAX_TOOL_OUTPUT_BYTES)
        .await
        .map_err(|error| {
            native_tool_failure(
                1605,
                &format!("Cannot run native tool '{program}': {error}"),
            )
        })?;
    let stdout = bounded_text(&output.stdout);
    let stderr = bounded_text(&output.stderr);
    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(native_tool_failure(
            1605,
            &format!(
                "Native tool '{}' failed with status {}: {}",
                program, output.status, detail
            ),
        ));
    }
    Ok(stdout)
}

/// Returns the first bounded line of a toolchain version banner.
pub(super) fn first_line(value: &str, fallback: &str) -> String {
    value
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(fallback)
        .trim()
        .chars()
        .take(64)
        .collect()
}

fn bounded_text(value: &[u8]) -> String {
    String::from_utf8_lossy(&value[..value.len().min(MAX_TOOL_OUTPUT_BYTES)]).into_owned()
}

/// Recursively copies a symlink-free tree in deterministic order.
pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<(), Failure> {
    native_io(fs::create_dir_all(destination), destination)?;
    let mut entries = native_io(
        fs::read_dir(source).and_then(Iterator::collect::<Result<Vec<_>, _>>),
        source,
    )?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = native_io(fs::symlink_metadata(&path), &path)?;
        if metadata.file_type().is_symlink() {
            return Err(native_tool_failure(
                1605,
                "Generated web bundle contains a symlink.",
            ));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&path, &target)?;
        } else if metadata.is_file() {
            native_io(fs::copy(&path, &target), &target)?;
        }
    }
    Ok(())
}

/// Writes one file, creating parent directories.
pub(super) fn write(path: &Path, contents: &[u8]) -> Result<(), Failure> {
    if let Some(parent) = path.parent() {
        native_io(fs::create_dir_all(parent), parent)?;
    }
    native_io(fs::write(path, contents), path)
}

/// Serializes one contract value as newline-terminated pretty JSON.
pub(super) fn pretty_json(value: &impl serde::Serialize, label: &str) -> Result<Vec<u8>, Failure> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        native_tool_failure(1605, &format!("Cannot serialize {label}: {error}"))
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Maps one filesystem error to a stable native diagnostic.
pub(super) fn native_io<T>(result: io::Result<T>, path: &Path) -> Result<T, Failure> {
    result.map_err(|error| {
        native_tool_failure(
            1605,
            &format!("Cannot write native artifact '{}': {error}", path.display()),
        )
    })
}

/// Builds a stable native toolchain or packaging diagnostic.
pub(super) fn native_tool_failure(number: u16, message: &str) -> Failure {
    Failure::one(diagnostic(
        number,
        message,
        Some(String::from(
            "Install the selected platform toolchain and keep the previous output intact.",
        )),
        None,
    ))
}

/// Escapes text embedded in generated XML documents.
pub(super) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Escapes text embedded in generated Swift and Kotlin string literals.
pub(super) fn quoted_string_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\u{2028}', "\\u{2028}")
        .replace('\u{2029}', "\\u{2029}")
}

/// Escapes text embedded in generated C string literals.
pub(super) fn c_string_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            value if (value as u32) < 0x20 => escaped.push(' '),
            value => escaped.push(value),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{NATIVE_SHIM, c_string_escape, first_line, quoted_string_escape, xml_escape};

    #[test]
    fn generated_literals_are_escaped_without_changing_plain_names() {
        assert_eq!(quoted_string_escape("Native Catalog"), "Native Catalog");
        assert_eq!(
            quoted_string_escape("A \"quoted\" \\\u{2028} name"),
            "A \\\"quoted\\\" \\\\\\u{2028} name"
        );
        assert_eq!(c_string_escape("plain"), "plain");
        assert_eq!(c_string_escape("a\"b\\c\nd\u{1}e"), "a\\\"b\\\\c\\nd e");
        assert_eq!(xml_escape("a&b<c>"), "a&amp;b&lt;c&gt;");
    }

    #[test]
    fn version_banners_are_bounded_and_never_empty() {
        assert_eq!(first_line("\n\n  gcc 15.1  \nmore", "fallback"), "gcc 15.1");
        assert_eq!(first_line("   ", "fallback"), "fallback");
        assert_eq!(first_line(&"x".repeat(200), "fallback").len(), 64);
    }

    #[test]
    fn the_shim_gives_the_page_one_bridge_and_says_where_it_is_running() {
        assert!(NATIVE_SHIM.contains("__tachyonNativeHostCall"));
        assert!(NATIVE_SHIM.contains("messageHandlers.tachyon"));
        assert!(NATIVE_SHIM.contains("__tachyonHostPost"));
        assert!(NATIVE_SHIM.contains("dataset.platform = 'native'"));
    }
}
