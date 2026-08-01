use crate::Failure;
use crate::failure::{diagnostic, source_span};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_HANDLER_SOURCE_BYTES: u64 = 1024 * 1024;

/// A Phase 2 Yon handler language.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandlerLanguage {
    /// ECMAScript module executed by Node.js.
    JavaScript,
    /// Python module executed by `CPython`.
    Python,
    /// A handler in any language that speaks the direct protocol itself.
    ///
    /// The process reads one JSON request from standard input and writes one
    /// JSON response to standard output, so no per-language adapter exists.
    Direct,
}

impl HandlerLanguage {
    /// Returns the stable adapter identifier recorded in manifests.
    #[must_use]
    pub const fn adapter(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript.v1",
            Self::Python => "python.v1",
            Self::Direct => "direct.v1",
        }
    }

    /// Returns the public language identifier.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Direct => "direct",
        }
    }
}

/// A validated, project-contained Yon handler source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandlerSource {
    project_root: PathBuf,
    relative_path: String,
    absolute_path: PathBuf,
    language: HandlerLanguage,
    interpreter: Vec<String>,
}

impl HandlerSource {
    /// Discovers and validates one project-relative `yon.js` or `yon.py`.
    ///
    /// # Errors
    ///
    /// Returns stable diagnostics for missing, unsafe, oversized, non-UTF-8,
    /// NUL-containing, or unsupported handler source.
    pub fn discover(
        project_root: impl AsRef<Path>,
        relative_source: impl AsRef<Path>,
    ) -> Result<Self, Failure> {
        let project_root = canonical_root(project_root.as_ref())?;
        let relative_source = validate_relative_source(relative_source.as_ref())?;
        let portable = portable(relative_source);
        let (language, interpreter) = language(relative_source, &portable, &project_root)?;
        assert_regular_path(&project_root, relative_source, &portable)?;
        let absolute_path = project_root.join(relative_source);
        let metadata = source_io(fs::metadata(&absolute_path), &portable)?;
        if !metadata.is_file() {
            return Err(invalid_source(
                &portable,
                "Handler source is not a regular file.",
            ));
        }
        if metadata.len() > MAX_HANDLER_SOURCE_BYTES {
            return Err(invalid_source(
                &portable,
                "Handler source exceeds the 1 MiB Phase 2 limit.",
            ));
        }
        let bytes = source_io(fs::read(&absolute_path), &portable)?;
        if bytes.contains(&0) {
            return Err(invalid_source(
                &portable,
                "Handler source contains a NUL byte.",
            ));
        }
        if std::str::from_utf8(&bytes).is_err() {
            return Err(invalid_source(
                &portable,
                "Handler source must be valid UTF-8.",
            ));
        }
        Ok(Self {
            project_root,
            relative_path: portable,
            absolute_path,
            language,
            interpreter,
        })
    }

    /// Returns the canonical project root.
    #[must_use]
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Returns the portable project-relative source path.
    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    /// Returns the canonical absolute source path.
    #[must_use]
    pub fn absolute_path(&self) -> &Path {
        &self.absolute_path
    }

    /// Returns the selected language adapter.
    #[must_use]
    pub const fn language(&self) -> HandlerLanguage {
        self.language
    }

    /// Returns the interpreter command for a direct handler.
    ///
    /// Empty when the handler file is itself executable, or when the language
    /// has a built-in adapter.
    #[must_use]
    pub fn interpreter(&self) -> &[String] {
        &self.interpreter
    }
}

fn canonical_root(root: &Path) -> Result<PathBuf, Failure> {
    let canonical = fs::canonicalize(root).map_err(|error| {
        Failure::one(diagnostic(
            2001,
            format!(
                "Cannot open handler project root '{}': {error}",
                root.display()
            ),
            Some(String::from("Pass an existing readable project directory.")),
            None,
        ))
    })?;
    if !canonical.is_dir() {
        return Err(Failure::one(diagnostic(
            2001,
            format!(
                "Handler project root '{}' is not a directory.",
                canonical.display()
            ),
            None,
            None,
        )));
    }
    Ok(canonical)
}

fn validate_relative_source(source: &Path) -> Result<&Path, Failure> {
    if source.as_os_str().is_empty()
        || source.is_absolute()
        || source.components().any(
            |component| !matches!(component, Component::Normal(value) if value.to_str().is_some()),
        )
        || !(source.starts_with("server/routes")
            || source.starts_with("server/workers")
            || is_middleware(source))
    {
        return Err(Failure::one(diagnostic(
            2002,
            format!(
                "Handler source '{}' is not a safe project-relative route path.",
                source.display()
            ),
            Some(String::from(
                "Use server/routes/**, server/workers/**, or a root middleware source.",
            )),
            None,
        )));
    }
    Ok(source)
}

/// Resolves the adapter for one handler file name.
///
/// JavaScript and Python have built-in adapters. Any other extension is served
/// by the direct protocol, provided `.tachyonrc` registers an interpreter for
/// it or the file is executable. That is what makes a new language a
/// configuration change rather than a new adapter.
/// Returns whether a project-relative path is the root middleware source.
fn is_middleware(source: &Path) -> bool {
    source
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
        && source
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("middleware."))
}

fn language(
    source: &Path,
    portable: &str,
    project_root: &Path,
) -> Result<(HandlerLanguage, Vec<String>), Failure> {
    let name = source.file_name().and_then(|name| name.to_str());
    match name {
        Some("yon.js" | "middleware.js") => {
            return Ok((HandlerLanguage::JavaScript, Vec::new()));
        }
        Some("yon.py" | "middleware.py") => return Ok((HandlerLanguage::Python, Vec::new())),
        _ => {}
    }
    // Any other source is served by the direct protocol, keyed on its file
    // extension. That covers yon.<ext> routes, middleware.<ext>, and a worker
    // under server/workers with any name at all.
    let extension = name
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
        .filter(|extension| {
            !extension.is_empty() && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        });
    let Some(extension) = extension else {
        return Err(no_adapter(portable, "has no file extension to resolve"));
    };

    let interpreters = super::interpreters::Interpreters::discover(project_root)?;
    if let Some(command) = interpreters.command(extension) {
        return Ok((HandlerLanguage::Direct, command.to_vec()));
    }
    if is_executable(&project_root.join(source)) {
        return Ok((HandlerLanguage::Direct, Vec::new()));
    }
    Err(no_adapter(
        portable,
        "has no registered interpreter and is not executable",
    ))
}

/// Returns whether a path is an executable regular file.
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

fn no_adapter(portable: &str, reason: &str) -> Failure {
    Failure::one(diagnostic(
        2003,
        format!("Handler source '{portable}' {reason}."),
        Some(String::from(
            "Use yon.js or yon.py, register an interpreter for the extension in \
             .tachyonrc, or make the handler executable. A direct handler reads \
             one JSON request from stdin and writes one JSON response to stdout.",
        )),
        source_span(portable, 0, portable.len()),
    ))
}

fn assert_regular_path(root: &Path, relative: &Path, portable: &str) -> Result<(), Failure> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(invalid_source(portable, "Handler path is not regular."));
        };
        current.push(segment);
        let metadata = source_io(fs::symlink_metadata(&current), portable)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid_source(
                portable,
                "Handler source paths cannot contain symlinks.",
            ));
        }
    }
    Ok(())
}

fn invalid_source(path: &str, message: &str) -> Failure {
    Failure::one(diagnostic(
        2004,
        format!("{message} Source: '{path}'."),
        Some(String::from(
            "Use a regular, UTF-8, project-contained source no larger than 1 MiB.",
        )),
        source_span(path, 0, path.len()),
    ))
}

fn source_io<T>(result: std::io::Result<T>, path: &str) -> Result<T, Failure> {
    result.map_err(|error| {
        Failure::one(diagnostic(
            2001,
            format!("Cannot inspect handler source '{path}': {error}"),
            None,
            source_span(path, 0, path.len()),
        ))
    })
}

fn portable(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{HandlerLanguage, HandlerSource};
    use std::fs;
    use std::path::Path;

    fn write(root: &Path, relative: &str, bytes: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, bytes).expect("source");
    }

    #[test]
    fn valid_sources_select_stable_adapters() {
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            "server/routes/a/yon.js",
            b"export class Handler {}",
        );
        write(
            root.path(),
            "server/routes/b/yon.py",
            b"class Handler: pass",
        );
        let javascript =
            HandlerSource::discover(root.path(), "server/routes/a/yon.js").expect("JavaScript");
        let python =
            HandlerSource::discover(root.path(), "server/routes/b/yon.py").expect("Python");
        assert_eq!(javascript.language(), HandlerLanguage::JavaScript);
        assert_eq!(javascript.language().name(), "javascript");
        assert_eq!(javascript.language().adapter(), "javascript.v1");
        assert_eq!(python.language(), HandlerLanguage::Python);
        assert!(javascript.absolute_path().is_absolute());
        assert_eq!(python.relative_path(), "server/routes/b/yon.py");
    }

    #[test]
    fn unsafe_unsupported_and_malformed_sources_fail_closed() {
        let root = tempfile::tempdir().expect("project");
        write(root.path(), "server/routes/yon.rb", b"class Handler; end");
        // `nul` is a reserved device name on Windows, so the directory
        // holding the NUL-byte source cannot be named after it.
        write(
            root.path(),
            "server/routes/nulbyte/yon.py",
            b"class\0 Handler",
        );
        write(root.path(), "server/routes/bytes/yon.js", &[0xff, 0xfe]);
        write(
            root.path(),
            "server/routes/large/yon.py",
            &vec![b'x'; 1024 * 1024 + 1],
        );
        for (path, code) in [
            ("../yon.py", "TY2002"),
            ("client/pages/yon.py", "TY2002"),
            ("server/routes/yon.rb", "TY2003"),
            ("server/routes/missing/yon.py", "TY2001"),
            ("server/routes/nulbyte/yon.py", "TY2004"),
            ("server/routes/bytes/yon.js", "TY2004"),
            ("server/routes/large/yon.py", "TY2004"),
        ] {
            let failure = HandlerSource::discover(root.path(), path).expect_err(path);
            assert!(failure.to_string().contains(code), "{path}: {failure}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_handler_paths_are_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("project");
        write(root.path(), "outside.py", b"class Handler: pass");
        fs::create_dir_all(root.path().join("server/routes")).expect("routes");
        symlink(
            root.path().join("outside.py"),
            root.path().join("server/routes/yon.py"),
        )
        .expect("symlink");
        let failure =
            HandlerSource::discover(root.path(), "server/routes/yon.py").expect_err("symlink");
        assert!(failure.to_string().contains("TY2004"));
    }
}
