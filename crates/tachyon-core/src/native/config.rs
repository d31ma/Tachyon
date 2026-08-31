use crate::Failure;
use crate::external_command::{ToolError, run as supervise_tool};
use crate::failure::{diagnostic, source_span};
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Config file names, in the order they are accepted.
///
/// Configuration is a module the project already knows how to write, not a
/// separate data dialect: the same file that carries the build hooks carries
/// the application's identity, and it can compute those values rather than
/// repeat them.
const CONFIG_NAMES: [&str; 3] = ["tac.config.js", "tac.config.mjs", "tac.config.ts"];
const MAX_CONFIG_BYTES: u64 = 1_024 * 1_024;

/// Reads the configuration module and writes one named export as JSON.
///
/// Evaluating it needs the project's own runtime, because the module is the
/// developer's code and may import from the project.
const CONFIG_RUNNER: &str = r"import { pathToFileURL } from 'node:url'
const source = process.env.TAC_CONFIG
if (!source) throw new Error('missing Tachyon config path')
const name = process.env.TAC_CONFIG_EXPORT
const loaded = await import(`${pathToFileURL(source).href}?tachyon=${process.env.TAC_CONFIG_NONCE}`)
const resolved = loaded[name] ?? loaded.default?.[name]
const value = typeof resolved === 'function' ? await resolved() : resolved
if (value === undefined) process.stdout.write('null')
else process.stdout.write(JSON.stringify(value))
";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeApplication {
    pub(super) name: String,
    pub(super) executable_name: String,
    pub(super) application_id: String,
    pub(super) version: String,
    pub(super) entry_route: String,
    /// The raster icons the manifest declared, largest edge first.
    ///
    /// The list rather than one choice, because each platform wants a
    /// different size: macOS builds every `.icns` slice from the largest,
    /// Windows cannot embed a PNG wider than 256 in an `.ico`, and Android
    /// scales one bucket down.
    pub(super) icons: Vec<(u32, String)>,
    /// The window this application opens with, and what a page may do to it.
    pub(super) window: WindowConfiguration,
}

/// The window a native target opens, declared once in `manifest.json`.
///
/// A web app manifest already says what an application is called, what its
/// icon is and where it starts. What it has never had is a window size, so
/// that lives beside those under a `tachyon` member — a manifest carrying
/// unknown members is still a valid manifest, and a browser ignores it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct WindowConfiguration {
    /// Logical width the window opens at.
    #[serde(default = "default_width")]
    pub(super) width: u32,
    /// Logical height the window opens at.
    #[serde(default = "default_height")]
    pub(super) height: u32,
    /// Smallest width the window may be resized to.
    #[serde(default)]
    pub(super) min_width: Option<u32>,
    /// Smallest height the window may be resized to.
    #[serde(default)]
    pub(super) min_height: Option<u32>,
    /// What a browser companion is permitted to do to the window.
    ///
    /// Empty by default. Electron hands a privileged process the window and
    /// lets it do anything; Tauri gates each call behind a permission in a
    /// separate capabilities dialect. This is the same default-deny, declared
    /// in the file the application already has to ship.
    #[serde(default)]
    pub(super) controls: Vec<String>,
}

const fn default_width() -> u32 {
    1024
}

const fn default_height() -> u32 {
    768
}

impl Default for WindowConfiguration {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            min_width: None,
            min_height: None,
            controls: Vec::new(),
        }
    }
}

impl WindowConfiguration {
    /// Every control a page may be granted.
    pub(super) const KNOWN: [&'static str; 6] = [
        "close",
        "fullscreen",
        "maximize",
        "minimize",
        "resize",
        "title",
    ];

    /// Whether one control was granted.
    pub(super) fn grants(&self, control: &str) -> bool {
        self.controls.iter().any(|value| value == control)
    }
}

/// The manifest a project ships for the web platform, if it has one.
///
/// `manifest.json` at the project root, which is where a PWA already puts it.
/// Absent is not an error for a web build — a page without a manifest is a
/// page — but a native target needs a name, an identifier and a window, and
/// this is where they are declared.
pub(super) fn read_manifest(project_root: &Path) -> Result<Option<WebAppManifest>, Failure> {
    let path = project_root.join(MANIFEST_NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(config_failure(&format!(
                "Cannot inspect {MANIFEST_NAME}: {error}"
            )));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(config_failure(&format!(
            "{MANIFEST_NAME} must be a regular, non-symlinked file."
        )));
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return Err(config_failure(&format!(
            "{MANIFEST_NAME} exceeds the 1 MiB limit."
        )));
    }
    let source = fs::read_to_string(&path)
        .map_err(|error| config_failure(&format!("Cannot read {MANIFEST_NAME}: {error}")))?;
    let manifest: WebAppManifest = serde_json::from_str(crate::without_bom(&source))
        .map_err(|error| config_failure(&format!("{MANIFEST_NAME} is invalid: {error}")))?;
    validate_manifest(&manifest)?;
    if let Some(tachyon) = &manifest.tachyon
        && let Some(window) = &tachyon.window
    {
        for control in &window.controls {
            if !WindowConfiguration::KNOWN.contains(&control.as_str()) {
                return Err(config_failure(&format!(
                    "'{control}' is not a window control. Grant any of: {}.",
                    WindowConfiguration::KNOWN.join(", ")
                )));
            }
        }
    }
    Ok(Some(manifest))
}

fn validate_manifest(manifest: &WebAppManifest) -> Result<(), Failure> {
    for name in [&manifest.name, &manifest.short_name].into_iter().flatten() {
        if name.trim().is_empty()
            || name.chars().count() > 64
            || name.chars().any(char::is_control)
            || executable_name(name).is_empty()
        {
            return Err(config_failure(
                "Manifest names must contain 1 to 64 printable characters and an ASCII letter or digit.",
            ));
        }
    }
    if manifest
        .start_url
        .as_ref()
        .is_some_and(|route| !valid_route(route))
    {
        return Err(config_failure(
            "Manifest start_url must be a canonical local route.",
        ));
    }
    for icon in &manifest.icons {
        let source = icon.src.trim_start_matches('/');
        if source.is_empty()
            || icon.src.starts_with("//")
            || source.contains(['\\', ':', '?', '#', '%'])
            || source
                .split('/')
                .any(|segment| matches!(segment, "" | "." | ".."))
        {
            return Err(config_failure(
                "Manifest icons must identify contained local bundle paths.",
            ));
        }
    }
    let Some(tachyon) = &manifest.tachyon else {
        return Ok(());
    };
    if tachyon
        .id
        .as_ref()
        .is_some_and(|id| !valid_application_id(id))
    {
        return Err(config_failure(
            "Manifest tachyon.id must be a bounded lowercase reverse-DNS identifier.",
        ));
    }
    if tachyon.version.as_ref().is_some_and(|version| {
        version.is_empty()
            || version.len() > 64
            || !version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    }) {
        return Err(config_failure(
            "Manifest tachyon.version must be a bounded portable version.",
        ));
    }
    if let Some(window) = &tachyon.window
        && (!(64..=16_384).contains(&window.width)
            || !(64..=16_384).contains(&window.height)
            || window.min_width.is_some_and(|value| value > window.width)
            || window.min_height.is_some_and(|value| value > window.height)
            || window.controls.len() > WindowConfiguration::KNOWN.len()
            || window
                .controls
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != window.controls.len())
    {
        return Err(config_failure(
            "Manifest window dimensions or controls exceed their supported bounds.",
        ));
    }
    Ok(())
}

/// The edge length an icon declares, or zero when it declares none.
fn icon_edge(icon: &ManifestIcon) -> u32 {
    icon.sizes
        .as_deref()
        .and_then(|sizes| sizes.split_whitespace().next())
        .and_then(|size| size.split(['x', 'X']).next())
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
}

/// Whether a source names an SVG, however it was cased.
fn is_svg(source: &str) -> bool {
    std::path::Path::new(source)
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("svg"))
}

/// The file a project declares its identity in.
pub(crate) const MANIFEST_NAME: &str = "manifest.json";

/// The name the manifest is published under, which is the media type's own.
pub(crate) const MANIFEST_OUTPUT: &str = "manifest.webmanifest";

/// The head elements a document gets from the project's manifest.
///
/// One declaration, three uses: the favicon a browser tab shows, the manifest
/// an install prompt reads, and the icon a native build compiles into its
/// bundle. They were the same artwork all along; this is the site saying so
/// once instead of three times.
///
/// # Errors
///
/// Returns a diagnostic when the manifest cannot be read or parsed.
pub(crate) fn manifest_head(project_root: &Path) -> Result<String, Failure> {
    use crate::compiler::html_attribute_escape;
    use std::fmt::Write as _;

    let Some(manifest) = read_manifest(project_root)? else {
        return Ok(String::new());
    };
    let mut head = format!(r#"<link rel="manifest" href="/{MANIFEST_OUTPUT}">"#);
    // An SVG scales to every tab and every density, so it is preferred where
    // it exists; a PNG is listed beside it for anything that cannot read one.
    for icon in &manifest.icons {
        let kind = if is_svg(&icon.src) {
            r#" type="image/svg+xml""#
        } else {
            ""
        };
        let sizes = icon.sizes.as_deref().map_or_else(String::new, |value| {
            format!(r#" sizes="{}""#, html_attribute_escape(value))
        });
        let _ = write!(
            head,
            r#"<link rel="icon" href="{}"{kind}{sizes}>"#,
            html_attribute_escape(&icon.src)
        );
    }
    if let Some(largest) = manifest.largest_icon() {
        let _ = write!(
            head,
            r#"<link rel="apple-touch-icon" href="{}">"#,
            html_attribute_escape(largest)
        );
    }
    Ok(head)
}

/// The subset of a web app manifest a native build reads.
#[derive(Debug, Deserialize)]
pub(super) struct WebAppManifest {
    name: Option<String>,
    short_name: Option<String>,
    start_url: Option<String>,
    #[serde(default)]
    icons: Vec<ManifestIcon>,
    #[serde(default)]
    tachyon: Option<TachyonManifest>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ManifestIcon {
    src: String,
    #[serde(default)]
    sizes: Option<String>,
}

impl WebAppManifest {
    /// The largest raster icon the manifest declares.
    ///
    /// Raster, because every platform's icon pipeline wants pixels: macOS
    /// builds an `.icns` from a PNG, and an SVG would have to be rasterised by
    /// something Tachyon does not ship. A manifest that lists only an SVG gets
    /// no native icon, which is the honest outcome rather than a blurry one.
    pub(super) fn largest_icon(&self) -> Option<&str> {
        self.icons
            .iter()
            .filter(|icon| !is_svg(&icon.src))
            .max_by_key(|icon| icon_edge(icon))
            .map(|icon| icon.src.as_str())
    }

    /// Every raster icon declared, largest edge first.
    pub(super) fn raster_icons(&self) -> Vec<(u32, String)> {
        let mut icons = self
            .icons
            .iter()
            .filter(|icon| !is_svg(&icon.src))
            .map(|icon| (icon_edge(icon), icon.src.clone()))
            .collect::<Vec<_>>();
        icons.sort_by_key(|(edge, _)| std::cmp::Reverse(*edge));
        icons
    }
}

/// What a manifest carries that the web platform has no field for.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TachyonManifest {
    /// Reverse-DNS bundle identifier. The manifest's own `id` is a URL, which
    /// is not what a platform installer wants.
    id: Option<String>,
    version: Option<String>,
    #[serde(default)]
    window: Option<WindowConfiguration>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationConfiguration {
    name: String,
    id: String,
    version: String,
    /// Named for the spelling an author writes in a module, with the snake
    /// case the former JSON contract used still accepted.
    #[serde(rename = "entryRoute", alias = "entry_route")]
    entry_route: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyNativeConfiguration {
    application: ApplicationConfiguration,
}

impl NativeApplication {
    /// Reads the application block from the project's configuration module.
    ///
    /// A project without one, or with one that exports no `application`, keeps
    /// the derived defaults: a name is not required to build.
    #[cfg(test)]
    pub(super) async fn discover(project_root: &Path) -> Result<Self, Failure> {
        Self::discover_from_snapshot(project_root, project_root).await
    }

    pub(super) async fn discover_from_snapshot(
        project_root: &Path,
        authored_root: &Path,
    ) -> Result<Self, Failure> {
        let manifest = read_manifest(project_root)?;
        let Some(path) = config_module_path(project_root)? else {
            let application = Self::legacy_or_defaults(project_root, authored_root)?;
            return Ok(application.with_manifest(manifest.as_ref()));
        };
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("tac.config.js");
        let source = evaluate_config(project_root, &path, name).await?;
        let trimmed = source.trim();
        if trimmed.is_empty() || trimmed == "null" {
            return Ok(Self::legacy_or_defaults(project_root, authored_root)?
                .with_manifest(manifest.as_ref()));
        }
        let application: ApplicationConfiguration = serde_json::from_str(trimmed)
            .map_err(|error| config_failure(&format!("{name} application is invalid: {error}")))?;
        Ok(Self::validate(application)?.with_manifest(manifest.as_ref()))
    }

    fn legacy_or_defaults(project_root: &Path, authored_root: &Path) -> Result<Self, Failure> {
        let legacy = project_root.join("tachyon.json");
        match fs::symlink_metadata(&legacy) {
            Ok(metadata) => {
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || metadata.len() > 65536
                {
                    return Err(config_failure(
                        "tachyon.json must be a regular non-symlinked file within 64 KiB.",
                    ));
                }
                let bytes = fs::read(&legacy).map_err(|error| {
                    config_failure(&format!("Cannot read tachyon.json: {error}"))
                })?;
                let config: LegacyNativeConfiguration =
                    serde_json::from_slice(&bytes).map_err(|error| {
                        config_failure(&format!("tachyon.json is invalid: {error}"))
                    })?;
                Self::validate(config.application)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok(Self::defaults(authored_root))
            }
            Err(error) => Err(config_failure(&format!(
                "Cannot inspect tachyon.json: {error}"
            ))),
        }
    }

    /// Overlays what `manifest.json` declares, which wins where both speak.
    ///
    /// The manifest is the application's identity to the web platform already
    /// — its name, its icon, where it starts. A native build reads the same
    /// file rather than a second copy of the same facts.
    fn with_manifest(mut self, manifest: Option<&WebAppManifest>) -> Self {
        let Some(manifest) = manifest else {
            return self;
        };
        if let Some(name) = manifest
            .name
            .clone()
            .or_else(|| manifest.short_name.clone())
            && !name.trim().is_empty()
        {
            self.executable_name = executable_name(&name);
            self.name = name;
        }
        if let Some(start) = &manifest.start_url
            && start.starts_with('/')
        {
            self.entry_route = String::from(start.trim_end_matches('/'));
            if self.entry_route.is_empty() {
                self.entry_route = String::from("/");
            }
        }
        if let Some(tachyon) = &manifest.tachyon {
            if let Some(id) = &tachyon.id
                && !id.trim().is_empty()
            {
                self.application_id.clone_from(id);
            }
            if let Some(version) = &tachyon.version
                && !version.trim().is_empty()
            {
                self.version.clone_from(version);
            }
            if let Some(window) = &tachyon.window {
                self.window.clone_from(window);
            }
        }
        self.icons = manifest.raster_icons();
        self
    }

    /// The largest raster icon declared, if any.
    pub(super) fn largest_icon(&self) -> Option<&str> {
        self.icons.first().map(|(_, src)| src.as_str())
    }

    /// The largest raster icon no wider than `edge`.
    pub(super) fn icon_within(&self, edge: u32) -> Option<&str> {
        self.icons
            .iter()
            .find(|(size, _)| *size <= edge)
            .map(|(_, src)| src.as_str())
    }

    fn defaults(project_root: &Path) -> Self {
        let candidate = project_root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("TachyonApp");
        let executable_name = executable_name(candidate);
        let identifier = identifier_segment(&executable_name);
        Self {
            name: executable_name.clone(),
            executable_name,
            application_id: format!("ma.del.tachyon.{identifier}"),
            version: String::from("1.0.0"),
            entry_route: String::from("/"),
            icons: Vec::new(),
            window: WindowConfiguration::default(),
        }
    }

    fn validate(value: ApplicationConfiguration) -> Result<Self, Failure> {
        let name = value.name.trim();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(config_failure(
                "Application name must contain 1 to 64 printable characters.",
            ));
        }
        let executable_name = executable_name(name);
        if executable_name.is_empty() {
            return Err(config_failure(
                "Application name must contain an ASCII letter or digit.",
            ));
        }
        if !valid_application_id(&value.id) {
            return Err(config_failure(
                "Application id must be a bounded lowercase reverse-DNS identifier.",
            ));
        }
        if value.version.is_empty()
            || value.version.len() > 64
            || !value
                .version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        {
            return Err(config_failure(
                "Application version must be a bounded portable version string.",
            ));
        }
        if !valid_route(&value.entry_route) {
            return Err(config_failure(
                "Application entry_route must be a canonical absolute route.",
            ));
        }
        Ok(Self {
            name: String::from(name),
            executable_name,
            application_id: value.id,
            version: value.version,
            entry_route: value.entry_route,
            icons: Vec::new(),
            window: WindowConfiguration::default(),
        })
    }
}

fn executable_name(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(64)
        .collect()
}

fn identifier_segment(value: &str) -> String {
    let value = value
        .bytes()
        .map(|byte| {
            let byte = byte.to_ascii_lowercase();
            if byte.is_ascii_alphanumeric() {
                char::from(byte)
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = value.trim_matches('-');
    if trimmed.is_empty() {
        String::from("app")
    } else {
        String::from(trimmed)
    }
}

fn valid_application_id(value: &str) -> bool {
    value.len() <= 255
        && value.split('.').count() >= 2
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || (byte == b'-' && !segment.starts_with('-') && !segment.ends_with('-'))
                })
        })
}

fn valid_route(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 2_048
        && !value.contains('\\')
        && !value.contains("//")
        && !value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
}

/// Resolves the configuration module a project uses, if it has one.
///
/// Every consumer resolves through here, so the application block, the
/// `postBundle` hook, and the rebuild fingerprint always agree on which file
/// is the project's configuration.
pub(crate) fn config_module_path(project_root: &Path) -> Result<Option<PathBuf>, Failure> {
    for name in CONFIG_NAMES {
        let path = project_root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(config_failure(&format!(
                        "{name} must be a regular, non-symlinked file."
                    )));
                }
                if metadata.len() > MAX_CONFIG_BYTES {
                    return Err(config_failure(&format!("{name} exceeds the 1 MiB limit.")));
                }
                return Ok(Some(path));
            }
            // Absent simply means the project uses one of the other names,
            // or none at all.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(config_failure(&format!("Cannot inspect {name}: {error}")));
            }
        }
    }
    Ok(None)
}

/// Evaluates the configuration module and returns the JSON it printed.
///
/// The module is the project's own code, so it runs in the project's runtime
/// rather than a parser here. TypeScript is stripped by the runtime that
/// supports it: Bun reads it directly, and Node needs to be told.
async fn evaluate_config(project_root: &Path, path: &Path, name: &str) -> Result<String, Failure> {
    evaluate_export(project_root, path, name, "application").await
}

async fn evaluate_export(
    project_root: &Path,
    path: &Path,
    name: &str,
    export: &str,
) -> Result<String, Failure> {
    let typescript = path.extension().is_some_and(|value| value == "ts");
    let configured = std::env::var_os("TAC_JAVASCRIPT_RUNTIME").map(PathBuf::from);
    let programs = configured
        .into_iter()
        .chain([PathBuf::from("node"), PathBuf::from("bun")]);
    // A fresh query keeps a long-lived runtime from serving a cached module,
    // without reaching for a clock the build cannot reproduce.
    let nonce = format!("{}", std::process::id());
    for program in programs {
        let node = program.file_stem().is_some_and(|value| value == "node");
        let mut command = tokio::process::Command::new(&program);
        if typescript && node {
            command.arg("--experimental-strip-types");
        }
        command
            .args(["--input-type=module", "--eval", CONFIG_RUNNER])
            .current_dir(project_root)
            .env("TAC_CONFIG", path)
            .env("TAC_CONFIG_EXPORT", export)
            .env("TAC_CONFIG_NONCE", &nonce)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true);
        let output = supervise_tool(&mut command, Duration::from_secs(30), 1_048_576).await;
        let output = match output {
            Ok(output) => output,
            Err(ToolError::Spawn(error)) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(config_failure(&format!("Cannot start {name}: {error}")));
            }
        };
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr);
            return Err(config_failure(&format!(
                "{name} failed to evaluate: {}",
                detail.lines().last().unwrap_or("no detail").trim()
            )));
        }
        return String::from_utf8(output.stdout)
            .map_err(|_| config_failure(&format!("{name} printed invalid UTF-8.")));
    }
    Err(config_failure(&format!(
        "{name} needs a JavaScript runtime, but neither node nor bun is available."
    )))
}

fn config_failure(message: &str) -> Failure {
    Failure::one(diagnostic(
        1601,
        message,
        Some(String::from(
            "Export an `application` object from tac.config.js with name, id, version and entryRoute.",
        )),
        source_span(CONFIG_NAMES[0], 0, CONFIG_NAMES[0].len()),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::NativeApplication;
    use std::fs;

    fn config(root: &std::path::Path, body: &str) {
        fs::write(root.join("tac.config.js"), body).expect("configuration");
    }

    #[test]
    fn manifest_icon_attributes_remain_literal_when_parsed_as_html() {
        for (source, sizes) in [
            (
                "/icons/app.png",
                "16x16\"><script src=\"/shared/injected.js\"></script><link sizes=\"16x16",
            ),
            ("/icons/a&copy;.png", "16x16 &copy;"),
            (
                "/icons/a\"><script src=\"/shared/injected.js\"></script><link href=\"a.png",
                "16x16",
            ),
            ("/icons/<angle>'&copy;\".png", "16x16"),
        ] {
            let root = tempfile::tempdir().expect("project");
            let manifest = serde_json::json!({
                "icons": [{ "src": source, "sizes": sizes }],
            });
            fs::write(root.path().join("manifest.json"), manifest.to_string()).expect("manifest");
            let head = super::manifest_head(root.path()).expect("manifest head");
            let tags = html5gum::Tokenizer::new(head.as_str())
                .filter_map(|token| match token.expect("HTML token") {
                    html5gum::Token::StartTag(tag) => Some((
                        String::from_utf8(tag.name.to_vec()).expect("tag name"),
                        tag.attributes
                            .into_iter()
                            .map(|(name, value)| {
                                (
                                    String::from_utf8(name.to_vec()).expect("attribute name"),
                                    String::from_utf8(value.value.to_vec())
                                        .expect("attribute value"),
                                )
                            })
                            .collect::<std::collections::BTreeMap<_, _>>(),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                tags.len(),
                3,
                "metadata must produce only three links: {head}"
            );
            assert!(tags.iter().all(|(name, _)| name == "link"), "{head}");
            assert_eq!(tags[1].1.len(), 3, "no injected icon attributes: {head}");
            assert_eq!(tags[1].1["href"], source);
            assert_eq!(tags[1].1["sizes"], sizes);
            assert_eq!(
                tags[2].1.len(),
                2,
                "no injected Apple icon attributes: {head}"
            );
            assert_eq!(tags[2].1["href"], source);
        }
    }

    #[tokio::test]
    async fn document_resources_reject_foreign_and_ambiguous_paths() {
        let root = tempfile::tempdir().expect("project");
        for source in [
            "//example.invalid/module.js",
            "/\\example.invalid/module.js",
            "/\t/example.invalid/module.js",
            "/shared/../module.js",
            "/shared/%2e%2E/module.js",
            "/shared/%2fmodule.js",
            "/shared/%5Cmodule.js",
        ] {
            config(
                root.path(),
                &format!(
                    "export const scripts = [{}]; export const styles = scripts;",
                    serde_json::to_string(source).expect("source literal")
                ),
            );
            for export in ["scripts", "styles"] {
                assert!(
                    super::document_paths(root.path(), export).await.is_err(),
                    "{export} accepted an ambiguous project path: {source:?}"
                );
            }
        }
        let sources = vec![
            String::from(
                "/shared/app.js?v=1&next=https://example.invalid/a..b&encoded=%2f#section",
            ),
            String::from("/shared/name%20with%20spaces.js"),
        ];
        config(
            root.path(),
            &format!(
                "export const scripts = {}; export const styles = scripts;",
                serde_json::to_string(&sources).expect("source literals")
            ),
        );
        for export in ["scripts", "styles"] {
            assert_eq!(
                super::document_paths(root.path(), export)
                    .await
                    .expect("local resources preserve query and fragment data"),
                sources
            );
        }
    }

    #[tokio::test]
    async fn defaults_and_module_configuration_are_portable() {
        let root = tempfile::tempdir().expect("root");
        // A project without a configuration module still builds.
        let defaults = NativeApplication::discover(root.path())
            .await
            .expect("defaults");
        assert!(defaults.application_id.starts_with("ma.del.tachyon."));
        assert_eq!(defaults.entry_route, "/");

        // The module can compute what a data file could only repeat.
        config(
            root.path(),
            "const channel = 'catalog'\n\
             export const application = {\n\
               name: 'Catalog App',\n\
               id: `dev.example.${channel}`,\n\
               version: '1.2.3',\n\
               entryRoute: '/products',\n\
             }\n",
        );
        let configured = NativeApplication::discover(root.path())
            .await
            .expect("configured");
        assert_eq!(configured.executable_name, "CatalogApp");
        assert_eq!(configured.application_id, "dev.example.catalog");
        assert_eq!(configured.entry_route, "/products");
    }

    #[tokio::test]
    async fn an_application_function_is_awaited() {
        let root = tempfile::tempdir().expect("root");
        config(
            root.path(),
            "export const application = async () => ({\n\
               name: 'Async App', id: 'dev.example.async', version: '2', entryRoute: '/',\n\
             })\n",
        );
        let configured = NativeApplication::discover(root.path())
            .await
            .expect("configured");
        assert_eq!(configured.application_id, "dev.example.async");
    }

    #[tokio::test]
    async fn a_module_without_an_application_export_keeps_the_defaults() {
        let root = tempfile::tempdir().expect("root");
        // A project may use the module only for build hooks.
        config(root.path(), "export function postBundle() {}\n");
        let configured = NativeApplication::discover(root.path())
            .await
            .expect("configured");
        assert!(configured.application_id.starts_with("ma.del.tachyon."));
    }

    #[tokio::test]
    async fn malformed_configuration_fails_closed() {
        for body in [
            "export const application = {}",
            "export const application = { name: '', id: 'dev.ok', version: '1', entryRoute: '/' }",
            "export const application = { name: 'App', id: 'INVALID', version: '1', entryRoute: '/' }",
            "export const application = { name: 'App', id: 'dev.ok', version: 'bad version', entryRoute: '/' }",
            "export const application = { name: 'App', id: 'dev.ok', version: '1', entryRoute: '../bad' }",
            "export const application = { name: 'App', id: 'dev.ok', version: '1', entryRoute: '/', extra: true }",
            // A module that throws must fail the build, not fall back silently.
            "throw new Error('broken configuration')",
        ] {
            let root = tempfile::tempdir().expect("root");
            config(root.path(), body);
            assert!(
                NativeApplication::discover(root.path()).await.is_err(),
                "{body}"
            );
        }
    }

    #[tokio::test]
    async fn oversized_configuration_fails_before_evaluation() {
        let root = tempfile::tempdir().expect("root");
        fs::write(
            root.path().join("tac.config.js"),
            vec![b' '; 1_024 * 1_024 + 1],
        )
        .expect("oversized configuration");
        let error = NativeApplication::discover(root.path())
            .await
            .expect_err("size limit");
        assert!(error.to_string().contains("TY1601"));
        assert!(error.to_string().contains("1 MiB"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_configuration_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let target = root.path().join("application.js");
        fs::write(&target, "export const application = {}").expect("target");
        symlink(&target, root.path().join("tac.config.js")).expect("symlink");
        let error = NativeApplication::discover(root.path())
            .await
            .expect_err("symlink rejection");
        assert!(error.to_string().contains("TY1601"));
        assert!(error.to_string().contains("non-symlinked"));
    }
}

/// One declared cache policy for a URL pattern.
///
/// Caching used to be something a companion opted into per call, through
/// `this.tac.fetch`. That works, but it puts a framework API on the hot path
/// of every read and leaves a plain `fetch()` uncached — so the same request
/// behaves differently depending on which function issued it. Declaring the
/// policy by path moves the decision to where it belongs: the service worker
/// enforces it for every request the page makes, whoever made it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct CacheRule {
    /// Anchored regular expression the request path must match.
    pub(crate) pattern: String,
    /// One of `cache-first`, `network-first`, or `no-store`.
    pub(crate) policy: String,
}

#[derive(serde::Deserialize)]
struct RawCacheRule {
    path: String,
    policy: String,
}

/// What a page puts in the document head, per route.
///
/// A `tac.html` is a view rather than a page of HTML, so this is where the
/// title, the description and the social tags live. The configuration module
/// already knows the route list — it is what generates the sitemap — so it is
/// where a route's metadata belongs rather than repeated in every template.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct PageMetadata {
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) canonical: Option<String>,
    /// The social preview image, as an absolute URL.
    #[serde(default)]
    pub(crate) image: Option<String>,
    /// `og:site_name`, which is the same for every route of one site.
    #[serde(default)]
    pub(crate) site_name: Option<String>,
    /// The document language. `en` unless a route says otherwise.
    #[serde(default)]
    pub(crate) lang: Option<String>,
}

/// Reads the `scripts` export: browser modules every document loads.
///
/// A shared browser entry — the one that registers a component library, or
/// seeds a theme before first paint — belongs to the project rather than to
/// any one page. Declaring it here replaces a `tac.js` per route whose whole
/// body was an import of it.
pub(crate) async fn browser_scripts(project_root: &Path) -> Result<Vec<String>, Failure> {
    document_paths(project_root, "scripts").await
}

/// Reads the `styles` export: stylesheets every document links.
///
/// The same argument as `scripts`. A site's shared stylesheet was linked by a
/// `tac.css` per route whose whole body was `@import url(...)`.
pub(crate) async fn browser_styles(project_root: &Path) -> Result<Vec<String>, Failure> {
    document_paths(project_root, "styles").await
}

/// Reads one export naming project-absolute paths every document loads.
async fn document_paths(project_root: &Path, export: &str) -> Result<Vec<String>, Failure> {
    let Some(path) = config_module_path(project_root)? else {
        return Ok(Vec::new());
    };
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("tac.config.js");
    let raw = evaluate_export(project_root, &path, name, export).await?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    let declared: Vec<String> = serde_json::from_str(trimmed)
        .map_err(|error| config_failure(&format!("{name} {export} is invalid: {error}")))?;
    if declared.len() > 8 {
        return Err(config_failure(&format!(
            "{name} declares more than 8 entries in {export}."
        )));
    }
    for source in &declared {
        // A project-absolute path, so a document can load it from an HTTP
        // origin and from a native host's own scheme alike.
        if !local_document_path(source) {
            return Err(config_failure(&format!(
                "'{source}' in {export} must be a local absolute project path of at most 256 characters, without ambiguous separators, traversal, or control characters."
            )));
        }
    }
    Ok(declared)
}

fn local_document_path(source: &str) -> bool {
    // Query and fragment data cannot change the resource's origin or path.
    // Browsers normalize encoded dot segments and HTTP URL backslashes;
    // platform asset handlers may also decode encoded separators.
    let path = source.split(['?', '#']).next().unwrap_or(source);
    let normalized = path.to_ascii_lowercase().replace("%2e", ".");
    source.len() <= 256
        && !source.chars().any(char::is_control)
        && path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        && !normalized.contains("..")
        && !normalized.contains("%2f")
        && !normalized.contains("%5c")
}

/// Reads the `metadata` export: a map from route to what its head carries.
///
/// A route with no entry gets the document shell and nothing else, which is
/// what a project that has not asked for metadata should get.
pub(crate) async fn page_metadata(
    project_root: &Path,
) -> Result<std::collections::BTreeMap<String, PageMetadata>, Failure> {
    let Some(path) = config_module_path(project_root)? else {
        return Ok(std::collections::BTreeMap::new());
    };
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("tac.config.js");
    let raw = evaluate_export(project_root, &path, name, "metadata").await?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(std::collections::BTreeMap::new());
    }
    serde_json::from_str(trimmed)
        .map_err(|error| config_failure(&format!("{name} metadata is invalid: {error}")))
}

/// Reads the `cache` export, if the project declares one.
pub(crate) async fn cache_rules(project_root: &Path) -> Result<Vec<CacheRule>, Failure> {
    let Some(path) = config_module_path(project_root)? else {
        return Ok(Vec::new());
    };
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("tac.config.js");
    let raw = evaluate_export(project_root, &path, name, "cache").await?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(Vec::new());
    }
    let declared: Vec<RawCacheRule> = serde_json::from_str(trimmed)
        .map_err(|error| config_failure(&format!("{name} cache is invalid: {error}")))?;
    if declared.len() > 64 {
        return Err(config_failure(&format!(
            "{name} declares more than 64 cache rules."
        )));
    }
    declared.into_iter().map(cache_rule).collect()
}

/// Compiles one declared rule, so the worker needs no glob parser of its own.
fn cache_rule(raw: RawCacheRule) -> Result<CacheRule, Failure> {
    if !matches!(
        raw.policy.as_str(),
        "cache-first" | "network-first" | "no-store"
    ) {
        return Err(config_failure(&format!(
            "'{}' is not a cache policy. Use cache-first, network-first or no-store.",
            raw.policy
        )));
    }
    if !raw.path.starts_with('/') || raw.path.len() > 256 {
        return Err(config_failure(&format!(
            "Cache path '{}' must be an absolute path of at most 256 characters.",
            raw.path
        )));
    }
    // `*` is the only metacharacter, and it stops at a query string so one
    // rule cannot silently widen to another route's parameters.
    let mut pattern = String::from("^");
    for part in raw.path.split('*') {
        if pattern.len() > 1 {
            pattern.push_str("[^?]*");
        }
        for character in part.chars() {
            if character.is_ascii_alphanumeric() || matches!(character, '/' | '-' | '_' | '.') {
                if character == '.' {
                    pattern.push('\\');
                }
                pattern.push(character);
            } else {
                return Err(config_failure(&format!(
                    "Cache path '{}' may contain only letters, digits, '/', '-', '_', '.' and '*'.",
                    raw.path
                )));
            }
        }
    }
    pattern.push('$');
    Ok(CacheRule {
        pattern,
        policy: raw.policy,
    })
}

#[cfg(test)]
mod cache_tests {
    use super::{RawCacheRule, cache_rule};

    fn compile(path: &str, policy: &str) -> Result<String, String> {
        cache_rule(RawCacheRule {
            path: String::from(path),
            policy: String::from(policy),
        })
        .map(|rule| rule.pattern)
        .map_err(|failure| failure.to_string())
    }

    #[test]
    fn a_declared_path_compiles_to_an_anchored_expression() {
        assert_eq!(
            compile("/api/products", "cache-first"),
            Ok(String::from("^/api/products$"))
        );
        // A wildcard stops at the query string, so one rule cannot widen into
        // another route's parameters.
        assert_eq!(
            compile("/api/search/*", "network-first"),
            Ok(String::from("^/api/search/[^?]*$"))
        );
        // A dot is a literal in a path and a metacharacter in an expression.
        assert_eq!(
            compile("/api/v1.0/items", "no-store"),
            Ok(String::from(r"^/api/v1\.0/items$"))
        );
    }

    #[test]
    fn only_declared_policies_and_safe_paths_are_accepted() {
        assert!(compile("/api/products", "stale-while-revalidate").is_err());
        assert!(compile("api/products", "cache-first").is_err());
        // Anything that could change the meaning of the compiled expression.
        for path in ["/api/(a|b)", "/api/a+", "/api/^x", "/api/a$"] {
            assert!(compile(path, "cache-first").is_err(), "{path}");
        }
    }
}
