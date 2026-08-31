use super::android::AndroidHostGenerator;
use super::config::NativeApplication;
use super::host::GeneratedHost;
use super::ios::IosHostGenerator;
use super::linux::LinuxHostGenerator;
use super::macos::MacOsHostGenerator;
use super::routes::NativeRouteIndex;
use super::windows::WindowsHostGenerator;
use crate::compiler::{publish, resolve_output_path};
use crate::failure::diagnostic;
use crate::{BuildOptions, Failure, Project, ProjectDiscovery, WebCompiler};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tachyon_contracts::{
    ArtifactContractVersions, ArtifactManifest, ArtifactOutput, ArtifactTarget, ArtifactToolchain,
    NativeTarget,
};

/// Options for one Phase 5 native application build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBuildOptions {
    /// Project-relative base output directory. Artifacts publish below the target directory.
    pub output_directory: PathBuf,
    /// Native platform receiving the generated application.
    pub target: NativeTarget,
    /// Whether the generated host is compiled into an installable artifact.
    pub package: bool,
}

impl Default for NativeBuildOptions {
    fn default() -> Self {
        Self {
            output_directory: PathBuf::from("dist"),
            target: NativeTarget::Macos,
            package: true,
        }
    }
}

/// Returns the stable published directory name for one native target.
#[must_use]
pub const fn native_target_directory(target: NativeTarget) -> &'static str {
    match target {
        NativeTarget::Linux => "linux",
        NativeTarget::Macos => "macos",
        NativeTarget::Windows => "windows",
        NativeTarget::Android => "android",
        NativeTarget::Ios => "ios",
    }
}

/// Returns the Artifact Manifest v1 operating system, architecture, and ABI.
fn artifact_target(target: NativeTarget) -> ArtifactTarget {
    let host_architecture = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let (os, architecture, abi) = match target {
        NativeTarget::Linux => ("linux", host_architecture, "webkitgtk"),
        NativeTarget::Macos => ("macos", host_architecture, "wkwebview"),
        NativeTarget::Windows => ("windows", "x86_64", "webview2"),
        NativeTarget::Android => ("android", "universal", "android-webview"),
        NativeTarget::Ios => ("ios", host_architecture, "wkwebview-simulator"),
    };
    ArtifactTarget {
        os: String::from(os),
        architecture: String::from(architecture),
        abi: String::from(abi),
    }
}

/// Evidence returned by a successful native application build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeBuildResult {
    output_directory: PathBuf,
    application_bundle: PathBuf,
    route_count: usize,
    sha256: String,
}

impl NativeBuildResult {
    /// Returns the published target output directory.
    #[must_use]
    pub fn output_directory(&self) -> &Path {
        &self.output_directory
    }

    /// Returns the published application bundle, executable, or package.
    #[must_use]
    pub fn application_bundle(&self) -> &Path {
        &self.application_bundle
    }

    /// Returns the number of packaged page routes.
    #[must_use]
    pub const fn route_count(&self) -> usize {
        self.route_count
    }

    /// Returns the digest over the complete published artifact.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

/// Compiles resolved Tachyon routes into a native application.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeCompiler;

impl NativeCompiler {
    /// Builds a native application using a current-thread async runtime.
    ///
    /// # Errors
    ///
    /// Returns diagnostics when planning, Swift compilation, signing, or
    /// atomic publication fails.
    pub fn build(
        project_root: impl AsRef<Path>,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure> {
        let project = ProjectDiscovery::discover(project_root)?;
        Self::build_project(&project, options)
    }

    /// Builds a native application from one immutable discovery snapshot.
    ///
    /// # Errors
    ///
    /// Returns deterministic planning, generation, or publication diagnostics.
    pub fn build_project(
        project: &Project,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                Failure::one(diagnostic(
                    1605,
                    format!("Cannot start the native build runtime: {error}"),
                    None,
                    None,
                ))
            })?;
        runtime.block_on(Self::build_project_async(project, options))
    }

    /// Builds and atomically publishes a native application.
    ///
    /// # Errors
    ///
    /// Returns deterministic native diagnostics and preserves the previous
    /// native output on every failure.
    pub async fn build_async(
        project_root: impl AsRef<Path>,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure> {
        let project = ProjectDiscovery::discover(project_root)?;
        Self::build_project_async(&project, options).await
    }

    /// Asynchronously builds a native application from one immutable discovery snapshot.
    ///
    /// # Errors
    ///
    /// Returns deterministic planning, generation, or publication diagnostics.
    pub async fn build_project_async(
        project: &Project,
        options: &NativeBuildOptions,
    ) -> Result<NativeBuildResult, Failure> {
        let application =
            NativeApplication::discover_from_snapshot(project.snapshot_root(), project.root())
                .await?;
        let base_output = resolve_output_path(project.root(), &options.output_directory)?;
        if !project
            .route_graph()
            .routes()
            .iter()
            .any(|route| route.route() == application.entry_route && route.source_path().is_some())
        {
            return Err(Failure::one(diagnostic(
                1601,
                format!(
                    "Native entry route '{}' does not identify a page.",
                    application.entry_route
                ),
                Some(String::from(
                    "Point the tac.config.js application entryRoute at a discovered page route.",
                )),
                None,
            )));
        }

        let (temporary_web, companions) = build_web_bundle(project, options.target).await?;
        let index =
            NativeRouteIndex::build(project.route_graph().routes(), &application.entry_route)?;

        let destination = base_output.join(native_target_directory(options.target));
        let parent = destination.parent().unwrap_or(project.root());
        fs::create_dir_all(parent).map_err(|error| native_io_failure(parent, &error))?;
        let stage = tempfile::Builder::new()
            .prefix(".tachyon-native-build-")
            .tempdir_in(parent)
            .map_err(|error| native_io_failure(parent, &error))?;
        write_host_descriptor(
            stage.path(),
            &application,
            options.target,
            &index,
            &companions,
        )?;
        let generated = generate_host(
            options.target,
            &application,
            &index,
            &companions,
            temporary_web.path(),
            stage.path(),
            options.package,
        )
        .await?;
        let outputs = collect_outputs(stage.path(), "artifact-manifest.json")?;
        let manifest = artifact_manifest(options.target, &generated, outputs);
        let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            Failure::one(diagnostic(
                1605,
                format!("Cannot serialize Artifact Manifest v1: {error}"),
                None,
                None,
            ))
        })?;
        manifest_bytes.push(b'\n');
        fs::write(stage.path().join("artifact-manifest.json"), manifest_bytes)
            .map_err(|error| native_io_failure(stage.path(), &error))?;
        let digest = digest_tree(stage.path())?;
        publish(stage, &destination).map_err(|error| native_io_failure(&destination, &error))?;

        Ok(NativeBuildResult {
            output_directory: destination.clone(),
            application_bundle: destination.join(generated.application_bundle),
            route_count: index.routes.len(),
            sha256: digest,
        })
    }
}

/// Writes the descriptor that says what this bundle is and what it can do.
fn write_host_descriptor(
    stage: &Path,
    application: &NativeApplication,
    target: NativeTarget,
    index: &NativeRouteIndex,
    companions: &[super::registry::NativeCompanionInput],
) -> Result<(), Failure> {
    let host = serde_json::json!({
        "schemaVersion": 3,
        "target": native_target_directory(target),
        "appName": application.name,
        "appId": application.application_id,
        "version": application.version,
        // The application's own bundle is what the host shows, so there is no
        // second rendering to describe and no adapter table to advertise.
        "renderMode": "bundle",
        "entryRoute": index.entry_route,
        "entryDocument": index.entry_document,
        "platformApiVersion": 1,
        "bridgeVersion": 3,
        "companions": companions.iter().map(|item| serde_json::json!({
            "route": item.route, "language": item.language.label(),
        })).collect::<Vec<_>>(),
        "windowControls": application.window.controls,
        // The host implements one channel and no vocabulary: a page asks its
        // compiled companion for a member. A tray, a window or a notification
        // is the platform's, and a companion written in the platform's own
        // language already has it.
        "hostCapabilities": ["companion.invoke"],
    });
    write_pretty_json(&stage.join("tachyon.host.json"), &host)
}

fn write_pretty_json(path: &Path, value: &serde_json::Value) -> Result<(), Failure> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| {
        Failure::one(diagnostic(
            1605,
            format!("Cannot serialize native compatibility artifact: {error}"),
            None,
            None,
        ))
    })?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| native_io_failure(path, &error))
}

async fn generate_host(
    target: NativeTarget,
    application: &NativeApplication,
    index: &NativeRouteIndex,
    companions: &[super::registry::NativeCompanionInput],
    web_bundle: &Path,
    stage: &Path,
    package: bool,
) -> Result<GeneratedHost, Failure> {
    match target {
        NativeTarget::Macos => {
            MacOsHostGenerator::generate(application, index, companions, web_bundle, stage, package)
                .await
        }
        NativeTarget::Ios => {
            IosHostGenerator::generate(application, index, companions, web_bundle, stage, package)
                .await
        }
        NativeTarget::Linux => {
            LinuxHostGenerator::generate(application, index, companions, web_bundle, stage, package)
                .await
        }
        NativeTarget::Windows => {
            WindowsHostGenerator::generate(
                application,
                index,
                companions,
                web_bundle,
                stage,
                package,
            )
            .await
        }
        NativeTarget::Android => {
            AndroidHostGenerator::generate(
                application,
                index,
                companions,
                web_bundle,
                stage,
                package,
            )
            .await
        }
    }
}

/// Builds the application's web bundle and collects the companions this
/// target compiles natively.
async fn build_web_bundle(
    project: &crate::Project,
    target: NativeTarget,
) -> Result<
    (
        tempfile::TempDir,
        Vec<super::registry::NativeCompanionInput>,
    ),
    Failure,
> {
    let temporary_web = tempfile::Builder::new()
        .prefix(".tachyon-native-web-")
        .tempdir_in(project.root())
        .map_err(|error| native_io_failure(project.root(), &error))?;
    let temporary_name = temporary_web
        .path()
        .file_name()
        .map(PathBuf::from)
        .ok_or_else(|| {
            Failure::one(diagnostic(
                1605,
                "Native staging directory has no name.",
                None,
                None,
            ))
        })?;
    WebCompiler::build_project_for_native(
        project,
        &BuildOptions {
            output_directory: temporary_name,
            incremental: false,
        },
        target,
    )
    .await?;

    let companions = native_companions(project, target)?;
    Ok((temporary_web, companions))
}

/// The companions this target compiles, once the matrix has been enforced.
///
/// A language is skipped when it belongs to another target, because a project
/// is expected to carry one companion per platform beside the same page. It is
/// an error only when a page declares companions and *none* of them reaches
/// this target: an application written solely in Swift and built for Android
/// has no behaviour on Android, and saying so at build time is the whole point
/// of the matrix.
fn native_companions(
    project: &crate::Project,
    target: NativeTarget,
) -> Result<Vec<super::registry::NativeCompanionInput>, Failure> {
    use crate::project::CompanionKind;

    let mut compiled = Vec::new();
    for route in project.route_graph().routes() {
        let mut declared = Vec::new();
        let mut browser = false;
        for companion in route.companions() {
            match companion.kind {
                CompanionKind::Native(language) => {
                    declared.push((
                        language,
                        project.snapshot_root().join(&companion.source_path),
                    ));
                }
                // A JavaScript or TypeScript companion runs on every target,
                // because every host is a web view.
                CompanionKind::ClientModule | CompanionKind::TypeScriptModule => browser = true,
                CompanionKind::Style => {}
            }
        }
        let languages = declared
            .iter()
            .map(|(language, _)| *language)
            .collect::<Vec<_>>();
        // Exactly one compiled companion answers a route on a target, even
        // when several reach it. Compiling the others produced a library the
        // host never calls — and demanded their toolchains to do it.
        let chosen = crate::project::NativeCompanion::most_specific(&languages, target);
        if let Some(chosen) = chosen {
            if let Some((_, source)) = declared.iter().find(|(language, _)| *language == chosen) {
                if compiled.len() >= 128 {
                    return Err(Failure::one(diagnostic(
                        1010,
                        "Native application exceeds the 128 companion-route limit.",
                        None,
                        None,
                    )));
                }
                compiled.push(super::registry::NativeCompanionInput {
                    language: chosen,
                    source: source.clone(),
                    route: String::from(route.route()),
                });
            }
        } else if !languages.is_empty() && !browser {
            return Err(unreachable_companion(route, &languages, target));
        }
    }
    Ok(compiled)
}

fn unreachable_companion(
    route: &crate::RouteNode,
    declared: &[crate::project::NativeCompanion],
    target: NativeTarget,
) -> Failure {
    crate::project::unreachable_companion(
        route.route(),
        declared,
        native_target_directory(target),
        Some(target),
    )
}

fn artifact_manifest(
    target: NativeTarget,
    generated: &GeneratedHost,
    outputs: Vec<ArtifactOutput>,
) -> ArtifactManifest {
    ArtifactManifest {
        contract_version: 1,
        release_version: String::from(tachyon_contracts::PRODUCT_VERSION),
        commit: source_revision(),
        source_date_epoch: std::env::var("SOURCE_DATE_EPOCH")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        target: artifact_target(target),
        toolchains: vec![
            ArtifactToolchain {
                name: String::from("rust"),
                version: String::from("1.97.1"),
            },
            ArtifactToolchain {
                name: generated.toolchain_name.clone(),
                version: generated.toolchain_version.clone(),
            },
        ],
        contracts: ArtifactContractVersions::default(),
        outputs,
    }
}

fn source_revision() -> String {
    std::env::var("GITHUB_SHA")
        .ok()
        .filter(|value| {
            matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .unwrap_or_else(|| "0".repeat(40))
}

fn collect_outputs(root: &Path, excluded: &str) -> Result<Vec<ArtifactOutput>, Failure> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)
            .and_then(Iterator::collect::<Result<Vec<_>, _>>)
            .map_err(|error| native_io_failure(&directory, &error))?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| native_io_failure(&path, &error))?;
            if metadata.file_type().is_symlink() {
                return Err(Failure::one(diagnostic(
                    1605,
                    "Native output contains a symlink.",
                    None,
                    None,
                )));
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                let relative = portable_path(path.strip_prefix(root).unwrap_or(&path));
                if relative != excluded {
                    let bytes =
                        fs::read(&path).map_err(|error| native_io_failure(&path, &error))?;
                    files.push(ArtifactOutput {
                        path: relative,
                        sha256: sha256_bytes(&bytes),
                        size: metadata.len(),
                    });
                }
            }
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn digest_tree(root: &Path) -> Result<String, Failure> {
    let outputs = collect_outputs(root, "\0")?;
    let mut hasher = Sha256::new();
    for output in outputs {
        hasher.update(output.path.as_bytes());
        hasher.update([0]);
        hasher.update(output.sha256.as_bytes());
        hasher.update([0]);
    }
    Ok(hex_digest(hasher.finalize()))
}

fn sha256_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    hex_digest(hasher.finalize())
}

fn hex_digest(value: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value.as_ref() {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn portable_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn native_io_failure(path: &Path, error: &io::Error) -> Failure {
    Failure::one(diagnostic(
        1605,
        format!("Native build cannot access '{}': {error}", path.display()),
        Some(String::from(
            "Check permissions and keep generated output paths regular and project-contained.",
        )),
        None,
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::super::host::NATIVE_SHIM;
    use super::{NativeBuildOptions, NativeCompiler, source_revision};
    use std::fs;

    /// The one channel a host implements, in every host.
    ///
    /// There used to be a vocabulary here — tray, window, quit, route — and a
    /// drift test comparing it against three host sources, because the
    /// advertised list and the implemented arms had already disagreed once. It
    /// is gone: what a page can ask a host for is that its companion answer,
    /// and everything else belongs to the companion's own SDK.
    #[test]
    fn every_host_implements_the_companion_channel_and_nothing_else() {
        assert!(NATIVE_SHIM.contains("__tachyonNativeHostCall"));
        // No capability vocabulary reaches the page: `tachyonHost.invoke`
        // and `tachyonHost.on` are gone, and `__tachyonHostPost` is only the
        // transport the companion channel travels over.
        assert!(!NATIVE_SHIM.contains("globalThis.tachyonHost"));
        assert!(!NATIVE_SHIM.contains("__tachyonNativeEmit"));
        for (platform, source) in [
            ("macos", super::super::macos::host_source()),
            ("windows", super::super::windows::host_source()),
            ("linux", super::super::linux::host_source()),
        ] {
            assert!(
                source.contains("companion.invoke"),
                "{platform} host cannot answer a companion call"
            );
            for gone in [
                "tray.set",
                "tray.remove",
                "window.show",
                "window.hide",
                "app.quit",
            ] {
                assert!(
                    !source.contains(&format!("\"{gone}\"")),
                    "{platform} host still implements '{gone}'"
                );
            }
        }
    }

    /// Every host relays what its companion publishes, and every companion
    /// language can publish.
    ///
    /// The channel runs both ways: the page asks the companion for a member,
    /// and the companion publishes when the platform tells it something rather
    /// than when the page asks. Five hosts and four preludes have to agree on
    /// one function name and one payload shape, and nothing makes them agree
    /// except this.
    #[test]
    fn every_host_relays_what_its_companion_publishes() {
        // The shim queues, because a companion may publish before the page's
        // modules have run and this is the file that runs first.
        assert!(NATIVE_SHIM.contains("__tachyonCompanionPublish"));
        assert!(NATIVE_SHIM.contains("__tachyonCompanionQueue"));
        for (platform, source) in [
            ("macos", super::super::macos::host_source()),
            ("ios", super::super::ios::host_source()),
            ("windows", super::super::windows::host_source()),
            ("linux", super::super::linux::host_source()),
            ("android", super::super::android::host_source()),
        ] {
            assert!(
                source.contains("__tachyonCompanionPublish"),
                "{platform} host never delivers a publish to the page"
            );
        }
        // The two hosts that load their companion rather than link it look the
        // sink up, so a companion built before this existed still loads.
        for (platform, source) in [
            ("windows", super::super::windows::host_source()),
            ("linux", super::super::linux::host_source()),
        ] {
            assert!(
                source.contains("tac_native_set_emit"),
                "{platform} host never installs its sink"
            );
        }
        for (language, prelude, publish) in [
            (
                "swift",
                super::super::macos::companion_prelude(),
                "func tacPublish",
            ),
            (
                "kotlin",
                super::super::android::companion_prelude(),
                "fun tacPublish",
            ),
            (
                "csharp",
                super::super::windows::companion_prelude(),
                "void Publish",
            ),
            (
                "rust",
                super::super::rust::companion_prelude(),
                "fn tac_publish",
            ),
        ] {
            assert!(
                prelude.contains(publish),
                "a {language} companion cannot publish"
            );
        }
        // Both C-ABI preludes export the installer the C hosts look up.
        for (language, prelude) in [
            ("csharp", super::super::windows::companion_prelude()),
            ("rust", super::super::rust::companion_prelude()),
        ] {
            assert!(
                prelude.contains("tac_native_set_emit"),
                "a {language} companion exports no sink installer"
            );
        }
    }

    #[test]
    fn fallback_revision_always_satisfies_the_manifest_shape() {
        assert!(matches!(source_revision().len(), 40 | 64));
        assert!(
            source_revision()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn in_process_native_build_covers_packaging_and_public_evidence() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            root.path().join("tac.config.js"),
            "export const application = { name: 'Coverage', id: 'dev.tachyon.coverage', \
             version: '1.0.0', entryRoute: '/' }\n",
        )
        .expect("configuration");
        fs::write(
            pages.join("tac.html"),
            r#"<main aria-label="Coverage"><h1>Coverage</h1><button aria-label="Add" data-tachyon-action="increment:count">Add</button><output aria-label="Count" data-tachyon-bind="count" data-tachyon-state="0">0</output><input aria-label="Name" data-tachyon-bind="name" data-tachyon-state=""><details aria-label="Details"><summary>Details</summary><p>Open</p></details><x-chart aria-label="Chart">Web</x-chart><iframe aria-label="Report" src="https://reports.example.test/path"></iframe></main>"#,
        )
        .expect("view");

        let result =
            NativeCompiler::build(root.path(), &NativeBuildOptions::default()).expect("build");
        assert!(result.output_directory().ends_with("dist/macos"));
        assert!(result.application_bundle().is_dir());
        assert_eq!(result.route_count(), 1);
        assert_eq!(result.sha256().len(), 64);
    }

    #[test]
    fn invalid_entry_route_fails_before_native_tool_execution() {
        let root = tempfile::tempdir().expect("project");
        let pages = root.path().join("client/pages");
        fs::create_dir_all(&pages).expect("pages");
        fs::write(
            root.path().join("tac.config.js"),
            "export const application = { name: 'Entry', id: 'dev.tachyon.entry', \
             version: '1', entryRoute: '/missing' }\n",
        )
        .expect("configuration");
        fs::write(pages.join("tac.html"), "<main>Home</main>").expect("view");
        let error = NativeCompiler::build(root.path(), &NativeBuildOptions::default())
            .expect_err("missing entry");
        assert!(error.to_string().contains("TY1601"));
    }
    #[cfg(unix)]
    #[test]
    fn public_native_build_from_project_reuses_the_owned_web_and_config_snapshot() {
        use std::os::unix::fs::symlink;

        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let workspace = tempfile::tempdir().expect("workspace");
        let authored = workspace.path().join("project");
        let write = |root: &std::path::Path, relative: &str, contents: &str| {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            fs::write(path, contents).expect("source");
        };
        write(
            &authored,
            "client/pages/tac.html",
            r#"<main><owned-card hydrate="load" /></main>"#,
        );
        write(
            &authored,
            "client/components/owned/card/tac.html",
            "<section>owned native component</section>",
        );
        write(
            &authored,
            "client/components/owned/card/tac.js",
            "export default class OwnedCard { static origin = 'owned-native-script' }\n",
        );
        write(&authored, "client/shared/origin.txt", "owned-native-shared");
        write(
            &authored,
            "client/pages/tac.swift",
            "final class Companion {\n let marker: String = \"owned-native-companion\"\n}\n",
        );
        write(&authored, "package.json", r#"{"type":"module"}"#);
        write(
            &authored,
            "tac.config.js",
            "import { writeFile } from 'node:fs/promises'\nexport async function postBundle({ targetRoots }) { await writeFile(`${targetRoots.web}/hook.txt`, 'owned-native-hook') }\n",
        );
        write(
            &authored,
            "tachyon.json",
            r#"{"application":{"name":"Owned Snapshot","id":"dev.tachyon.owned","version":"1.2.3","entry_route":"/"}}"#,
        );
        let project = crate::ProjectDiscovery::discover(&authored).expect("owned snapshot");

        let retained = workspace.path().join("retained");
        fs::rename(&authored, &retained).expect("move project");
        let planted = tempfile::tempdir().expect("planted");
        write(
            planted.path(),
            "client/pages/tac.html",
            "<main>planted native page</main>",
        );
        write(
            planted.path(),
            "client/components/owned/card/tac.html",
            "<section>planted native component</section>",
        );
        write(
            planted.path(),
            "client/components/owned/card/tac.js",
            "export default class Planted {}\n",
        );
        write(
            planted.path(),
            "client/shared/origin.txt",
            "planted-native-shared",
        );
        write(planted.path(), "package.json", r#"{"type":"module"}"#);
        write(
            planted.path(),
            "tac.config.js",
            "import { writeFile } from 'node:fs/promises'\nexport async function postBundle({ targetRoots }) { await writeFile(`${targetRoots.web}/hook.txt`, 'planted-native-hook') }\n",
        );
        write(
            planted.path(),
            "tachyon.json",
            r#"{"application":{"name":"Planted Canary","id":"dev.tachyon.planted","version":"9.9.9","entry_route":"/"}}"#,
        );
        symlink(planted.path(), &authored).expect("ambient replacement");

        let result = NativeCompiler::build_project(
            &project,
            &NativeBuildOptions {
                output_directory: workspace.path().join("native-output"),
                target: tachyon_contracts::NativeTarget::Macos,
                package: false,
            },
        )
        .expect("snapshot native build");
        assert_native_snapshot_output(result.output_directory());
    }

    #[cfg(unix)]
    fn assert_native_snapshot_output(output: &std::path::Path) {
        let web = output.join("web");
        let index = fs::read_to_string(web.join("index.html")).expect("web index");
        let component = fs::read_to_string(web.join(".tachyon/components/owned-card.js"))
            .expect("component script");
        let plist = fs::read_to_string(output.join("OwnedSnapshot.app/Contents/Info.plist"))
            .expect("native config output");
        assert!(index.contains("owned native component"), "{index}");
        assert!(!index.contains("planted"), "{index}");
        assert!(component.contains("owned-native-script"), "{component}");
        assert_eq!(
            fs::read_to_string(web.join("shared/origin.txt")).expect("shared"),
            "owned-native-shared"
        );
        assert_eq!(
            fs::read_to_string(web.join("hook.txt")).expect("hook"),
            "owned-native-hook"
        );
        assert!(plist.contains("Owned Snapshot"), "{plist}");
        assert!(!plist.contains("Planted Canary"), "{plist}");
        let companion = fs::read_to_string(output.join("project/TachyonCompanion.swift"))
            .expect("captured companion");
        assert!(companion.contains("owned-native-companion"));
    }
}
