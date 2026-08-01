use crate::Failure;
use crate::compiler::publish;
use crate::failure::diagnostic;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_ENV: &str = "YON_PORT=8000\n\
YON_HOST=127.0.0.1\n\
YON_HOSTNAME=127.0.0.1\n\
YON_DEV=true\n\
YON_LOG_LEVEL=info\n\
YON_LOG_FORMAT=pretty\n\
YON_TRUST_PROXY=\n\
YON_CONTENT_SECURITY_POLICY=default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'\n\
TAC_FORMAT=esm\n\
TAC_PUBLIC_ENV=\n\
YON_MAX_BODY_BYTES=1048576\n\
YON_HANDLER_TIMEOUT_MS=30000\n\
YON_RATE_LIMIT_MAX=\n\
YON_RATE_LIMIT_WINDOW_MS=\n\
YON_HMR_TOKEN=\n\
YON_HMR_MAX_CLIENTS=20\n\
YON_ENABLE_HSTS=false\n\
YON_SKIP_BUNDLE=false\n\
YON_OTEL_ENABLED=false\n\
YON_OTEL_ROOT=\n\
YON_OTEL_SERVICE_NAME=@d31ma/tachyon\n\
YON_OTEL_SERVICE_VERSION=\n\
YON_OTEL_CAPTURE_IP=false\n\
FYLO_ROOT=db\n\
FYLO_SCHEMA=db/schemas\n\
FYLO_INDEX_BACKEND=local-fs\n\
FYLO_ENCRYPTION_KEY=\n\
FYLO_CIPHER_SALT=\n\
YON_DATA_BROWSER_ENABLED=false\n\
YON_DATA_BROWSER_READONLY=true\n\
YON_DATA_BROWSER_REVEAL=false\n\
YON_CORS_ORIGIN=\n\
YON_PAGES_PATH=client/pages\n\
YON_COMPONENTS_PATH=client/components\n\
YON_ASSETS_PATH=client/shared/assets\n\
YON_ROUTES_PATH=server/routes\n\
YON_SHARED_SCRIPTS_PATH=client/shared/scripts\n\
YON_SHARED_STYLES_PATH=client/shared/styles\n\
YON_SHARED_DATA_PATH=client/shared/data\n";

/// Evidence returned after initializing a project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaffoldResult {
    root: PathBuf,
    app_name: String,
}

impl ScaffoldResult {
    /// Returns the initialized project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the validated application name.
    #[must_use]
    pub fn app_name(&self) -> &str {
        &self.app_name
    }
}

/// Creates minimal HTML-only Tachyon applications.
#[derive(Clone, Copy, Debug, Default)]
pub struct Scaffold;

impl Scaffold {
    /// Creates a project in a missing or empty target directory.
    ///
    /// # Errors
    ///
    /// Returns stable diagnostics when the target is unsafe or non-empty, the
    /// application name is invalid, or the complete scaffold cannot publish.
    pub fn create(
        target: impl AsRef<Path>,
        app_name: Option<&str>,
    ) -> Result<ScaffoldResult, Failure> {
        let target = absolute_target(target.as_ref())?;
        validate_target(&target)?;
        let app_name = validated_name(app_name, &target)?;
        let parent = target.parent().ok_or_else(|| {
            Failure::one(diagnostic(
                1401,
                format!(
                    "Initialization target '{}' has no parent.",
                    target.display()
                ),
                None,
                None,
            ))
        })?;
        scaffold_io(fs::create_dir_all(parent), &target)?;
        let stage = scaffold_io(
            tempfile::Builder::new()
                .prefix(".tachyon-init-")
                .tempdir_in(parent),
            &target,
        )?;
        for (relative, contents) in scaffold_files(&app_name) {
            let output = stage.path().join(relative);
            if let Some(directory) = output.parent() {
                scaffold_io(fs::create_dir_all(directory), &target)?;
            }
            scaffold_io(fs::write(output, contents), &target)?;
        }
        scaffold_io(publish(stage, &target), &target)?;
        let root = scaffold_io(fs::canonicalize(&target), &target)?;
        Ok(ScaffoldResult { root, app_name })
    }
}

fn absolute_target(target: &Path) -> Result<PathBuf, Failure> {
    if target.as_os_str().is_empty() {
        return Err(Failure::one(diagnostic(
            1401,
            "Initialization target cannot be empty.",
            None,
            None,
        )));
    }
    if target.is_absolute() {
        Ok(target.to_path_buf())
    } else {
        match std::env::current_dir() {
            Ok(current) => Ok(current.join(target)),
            Err(error) => Err(Failure::one(diagnostic(
                1401,
                format!("Cannot resolve initialization target: {error}"),
                None,
                None,
            ))),
        }
    }
}

fn validate_target(target: &Path) -> Result<(), Failure> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(scaffold_error(target, &error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Failure::one(diagnostic(
            1401,
            format!(
                "Initialization target '{}' must be a regular directory.",
                target.display()
            ),
            None,
            None,
        )));
    }
    let mut entries = scaffold_io(fs::read_dir(target), target)?;
    if scaffold_io(entries.next().transpose(), target)?.is_some() {
        return Err(Failure::one(diagnostic(
            1402,
            format!("Initialization target '{}' is not empty.", target.display()),
            Some(String::from(
                "Choose a new or empty directory; Tachyon never overwrites existing projects.",
            )),
            None,
        )));
    }
    Ok(())
}

fn validated_name(provided: Option<&str>, target: &Path) -> Result<String, Failure> {
    let fallback = target
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Tachyon App");
    let name = provided.unwrap_or(fallback).trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(Failure::one(diagnostic(
            1403,
            "Application name must contain 1 to 100 printable characters.",
            None,
            None,
        )));
    }
    Ok(String::from(name))
}

fn scaffold_files(app_name: &str) -> BTreeMap<&'static str, String> {
    let package_name = package_name(app_name);
    let javascript_name =
        serde_json::to_string(app_name).unwrap_or_else(|_| String::from("\"Tachyon App\""));
    BTreeMap::from([
        (
            ".gitignore",
            String::from("node_modules\ndist\n.env\n.DS_Store\n"),
        ),
        (".env.example", String::from(DEFAULT_ENV)),
        (".env.test", String::from(DEFAULT_ENV)),
        (
            "package.json",
            format!(
                "{{\n  \"name\": \"{package_name}\",\n  \"private\": true,\n  \"type\": \"module\",\n  \"scripts\": {{\n    \"serve\": \"ty serve\",\n    \"bundle\": \"ty bundle\",\n    \"preview\": \"ty preview --watch\",\n    \"test\": \"bun test\"\n  }}\n}}\n"
            ),
        ),
        (
            "jsconfig.json",
            String::from(
                "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"allowJs\": true,\n    \"checkJs\": true,\n    \"target\": \"ESNext\",\n    \"module\": \"NodeNext\",\n    \"moduleResolution\": \"NodeNext\",\n    \"types\": [\n      \"bun-types\",\n      \"@types/node\"\n    ]\n  },\n  \"include\": [\n    \"tachyon-env.d.ts\",\n    \"client/**/*.js\",\n    \"client/**/*.ts\",\n    \"server/**/*.js\",\n    \"server/**/*.ts\"\n  ]\n}\n",
            ),
        ),
        ("tachyon-env.d.ts", application_globals()),
        (
            "README.md",
            format!(
                "# {app_name}\n\n## Install the CLI\n\n```bash\n\
                 curl -fsSL https://tachyon.del.ma/install.sh | sh\n```\n\n\
                 This shows a staged Tachyon progress bar while installing the `ty` binary plus\n\
                 the `fylo`, `chex`, and `ttid` binaries it drives at runtime.\n\n\
                 ## Commands\n\n```bash\nty bundle    # build the client + native artifacts\n\
                 ty preview   # preview the built bundle\nty serve     # run the dev/prod server\n```\n\n\
                 The bundled output is written to `dist/`. `ty serve` detects whether the app has `client/`, `server/`, or both and serves the matching frontend, backend, or full-stack runtime.\n"
            ),
        ),
        (
            "client/pages/tac.html",
            String::from(
                "<div class=\"shell\">\n  <div class=\"brand\">\n    <strong>Tachyon</strong>\n    <nav>\n      <a href=\"/\">Home</a>\n    </nav>\n  </div>\n\n  <slot />\n\n  <hero />\n</div>\n",
            ),
        ),
        (
            "client/pages/tac.js",
            format!("document.title = {javascript_name}\n"),
        ),
        (
            "client/pages/tac.css",
            String::from(
                "body { margin: 0; font-family: \"IBM Plex Sans\", ui-sans-serif, system-ui, sans-serif; background: #0f172a; color: #e2e8f0; }\n.shell { max-width: 72rem; margin: 0 auto; padding: 2rem 1.25rem 4rem; }\n.brand { display: flex; align-items: center; justify-content: space-between; margin-bottom: 2rem; }\n.brand a { color: inherit; text-decoration: none; }\n",
            ),
        ),
        (
            "client/components/hero/tac.html",
            String::from(
                "<section class=\"hero\">\n  <h1>Build your next Bun app with Tachyon.</h1>\n  <p>File-system routes, reactive Tac pages, static export, and preview tooling are already wired in.</p>\n</section>\n",
            ),
        ),
        (
            "client/components/hero/tac.css",
            String::from(
                ".hero { padding: 2rem; border-radius: 1.5rem; background: linear-gradient(135deg, #1d4ed8, #0f766e); }\n.hero h1 { margin: 0 0 0.75rem; font-size: clamp(2rem, 6vw, 4rem); }\n.hero p { margin: 0; max-width: 42rem; line-height: 1.6; }\n",
            ),
        ),
        (
            "client/shared/scripts/imports.js",
            String::from(
                "import \"../styles/app.css\"\n\ndocument.documentElement.setAttribute('data-theme', 'light')\n",
            ),
        ),
        (
            "client/shared/styles/app.css",
            String::from(":root {\n  color-scheme: dark;\n}\n"),
        ),
        ("client/shared/assets/.gitkeep", String::new()),
        ("client/shared/data/.gitkeep", String::new()),
        (
            "server/routes/yon.js",
            String::from(
                "export class Handler {\n  static async GET() {\n    return { ok: true, framework: 'Tachyon' }\n  }\n}\n",
            ),
        ),
        ("server/data/.gitkeep", String::new()),
        ("server/deps/.gitkeep", String::new()),
        ("db/schemas/.gitkeep", String::new()),
        ("db/.collections/.gitkeep", String::new()),
        (
            "db/README.md",
            String::from(
                "# db/\n\nThis folder is the default FYLO root for the application. Tachyon drives the\n`fylo` binary against it (installed alongside `ty`).\n\n## Structure\n\n```\ndb/\n├── schemas/       # Versioned schemas consumed by FYLO strict validation\n└── .collections/  # FYLO document store, managed exclusively by the FYLO binary\n```\n\n## schemas/\n\nPlace versioned JSON schemas here for FYLO strict validation:\n\n```\ndb/schemas/<collection>/\n|-- manifest.json\n|-- history/\n|   `-- v1.json\n`-- rules.json        # optional RLS rules\n```\n\nWhen schemas declare `$encrypted` fields, FYLO will use AES-GCM encryption for\nthose values. The manifest's `current` field selects the head schema version.\n\n## .collections/\n\n**Do not modify the contents of this directory by hand.**\n\nDocument shards, prefix indexes, event journals, locks, and WORM history are\ncreated and managed exclusively by the `fylo` binary. Manual edits to\nthis directory can corrupt storage state and cause data loss.\n\nTo rebuild the index from document files:\n\n```bash\nfylo.admin rebuild <collection> --root db\n```\n\n## Overriding the root\n\nTo use a different FYLO root or schema directory, set:\n\n```env\nFYLO_ROOT=/path/to/custom/root\nFYLO_SCHEMA=/path/to/custom/schemas\nFYLO_INDEX_BACKEND=local-fs\n```\n",
            ),
        ),
    ])
}

fn package_name(app_name: &str) -> String {
    let mut package = String::new();
    let mut pending_dash = false;
    for character in app_name.trim().chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            if pending_dash && !package.is_empty() {
                package.push('-');
            }
            pending_dash = false;
            package.push(character);
        } else {
            pending_dash = true;
        }
    }
    let package = package.trim_matches('-');
    if package.is_empty() {
        String::from("tachyon-app")
    } else {
        String::from(package)
    }
}

fn application_globals() -> String {
    let mut globals =
        String::from("// Tachyon ambient globals — generated by 'ty init'. Do not edit.\n");
    globals.push_str(include_str!("../../../api/types/tachyon-env.d.ts"));
    globals
}

fn scaffold_error(target: &Path, error: &std::io::Error) -> Failure {
    Failure::one(diagnostic(
        1401,
        format!("Cannot initialize project '{}': {error}", target.display()),
        Some(String::from("Check target permissions and try again.")),
        None,
    ))
}

fn scaffold_io<T>(result: std::io::Result<T>, target: &Path) -> Result<T, Failure> {
    result.map_err(|error| scaffold_error(target, &error))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{Scaffold, absolute_target, scaffold_error, scaffold_io, validated_name};
    use std::fs;
    use std::io;
    use std::path::Path;

    #[test]
    fn scaffold_serializes_the_app_name_and_is_complete() {
        let root = tempfile::tempdir().expect("workspace");
        let target = root.path().join("app");
        let result = Scaffold::create(&target, Some("A & <B>")).expect("scaffold");
        let script = fs::read_to_string(target.join("client/pages/tac.js")).expect("script");
        assert!(script.contains("document.title = \"A & <B>\""));
        assert!(target.join("server/routes/yon.js").is_file());
        assert!(target.join("client/components/hero/tac.html").is_file());
        assert_eq!(result.app_name(), "A & <B>");
        assert_eq!(result.root(), fs::canonicalize(target).expect("canonical"));
    }

    #[test]
    fn existing_empty_targets_and_default_names_are_supported() {
        let root = tempfile::tempdir().expect("workspace");
        let target = root.path().join("default-name");
        fs::create_dir(&target).expect("empty target");
        let result = Scaffold::create(&target, None).expect("scaffold");
        assert_eq!(result.app_name(), "default-name");
        assert!(target.join("README.md").is_file());
    }

    #[test]
    fn invalid_targets_and_names_fail_before_writing() {
        let root = tempfile::tempdir().expect("workspace");
        let file = root.path().join("file");
        fs::write(&file, "occupied").expect("file");
        assert!(
            Scaffold::create(&file, None)
                .expect_err("file target")
                .to_string()
                .contains("TY1401")
        );

        for name in ["", "\n", &"x".repeat(101)] {
            let target = root.path().join(format!("invalid-{}", name.len()));
            assert!(
                Scaffold::create(&target, Some(name))
                    .expect_err("invalid name")
                    .to_string()
                    .contains("TY1403")
            );
            assert!(!target.exists());
        }

        assert!(absolute_target(Path::new("")).is_err());
        assert!(
            absolute_target(root.path())
                .expect("absolute")
                .is_absolute()
        );
        assert!(
            absolute_target(Path::new("relative"))
                .expect("relative")
                .is_absolute()
        );
        assert_eq!(
            validated_name(None, Path::new("/")).expect("fallback"),
            "Tachyon App"
        );

        let io_error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        assert!(
            scaffold_error(&file, &io_error)
                .to_string()
                .contains("TY1401")
        );
        assert!(scaffold_io::<()>(Err(io_error), &file).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_targets_are_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("workspace");
        let directory = root.path().join("directory");
        let link = root.path().join("link");
        fs::create_dir(&directory).expect("directory");
        symlink(&directory, &link).expect("symlink");
        assert!(
            Scaffold::create(&link, None)
                .expect_err("symlink")
                .to_string()
                .contains("TY1401")
        );
    }
}
