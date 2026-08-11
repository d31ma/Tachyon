//! Phase 6 migration analysis.
//!
//! Scans a project written against the legacy JavaScript implementation and
//! classifies everything it finds against the surface the Rust implementation
//! actually supports. The analysis never modifies the project and never runs
//! project code.

use crate::Failure;
use crate::failure::{diagnostic, source_span};
use crate::handler::Interpreters;
use crate::template::{ComponentRegistry, TemplateFrontend};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Largest source file the analysis will read.
const MAX_SOURCE_BYTES: u64 = 4 * 1_024 * 1_024;
/// Largest number of findings one report will carry.
const MAX_FINDINGS: usize = 10_000;
/// Directories that are never project sources.
const IGNORED_DIRECTORIES: [&str; 7] = [
    ".git",
    ".tachyon",
    "dist",
    "dist-bin",
    "node_modules",
    "target",
    "__pycache__",
];

/// How one discovered artifact relates to the Rust implementation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    /// The Rust implementation handles this with the same authored source.
    Supported,
    /// The Rust implementation handles this, but authored source must change.
    Changed,
    /// The Rust implementation has no equivalent behavior.
    Unsupported,
}

impl MigrationStatus {
    /// Returns the stable lowercase label used in reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Changed => "changed",
            Self::Unsupported => "unsupported",
        }
    }
}

/// One classified artifact or construct.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MigrationFinding {
    /// Portable project-relative path.
    pub source: String,
    /// Stable feature name.
    pub feature: String,
    /// Classification against the Rust implementation.
    pub status: MigrationStatus,
    /// Why the classification applies.
    pub detail: String,
    /// The action a maintainer must take, when one is required.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// A complete migration analysis of one project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MigrationReport {
    /// Contract major version.
    pub contract_version: u8,
    /// Number of findings by classification.
    pub supported: usize,
    /// Findings requiring an authored-source change.
    pub changed: usize,
    /// Findings with no Rust equivalent.
    pub unsupported: usize,
    /// Every finding, ordered by source then feature.
    pub findings: Vec<MigrationFinding>,
}

impl MigrationReport {
    /// Returns whether the project can build against the Rust implementation
    /// without losing behavior.
    #[must_use]
    pub const fn is_clean(&self) -> bool {
        self.unsupported == 0 && self.changed == 0
    }

    /// Returns the failure a strict migration check reports, if any.
    #[must_use]
    pub fn unsupported_failure(&self) -> Option<Failure> {
        (self.unsupported > 0).then(|| {
            Failure::one(diagnostic(
                1702,
                format!(
                    "{} construct(s) have no equivalent in this implementation.",
                    self.unsupported
                ),
                Some(String::from(
                    "Resolve each unsupported finding, or pass --allow-unsupported \
                     to report without failing.",
                )),
                None,
            ))
        })
    }

    /// Renders the human-readable report.
    #[must_use]
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut text = String::new();
        for finding in &self.findings {
            let _ = writeln!(
                text,
                "{:<11} {:<28} {}",
                finding.status.label(),
                finding.feature,
                finding.source
            );
            let _ = writeln!(text, "            {}", finding.detail);
            if let Some(action) = &finding.action {
                let _ = writeln!(text, "            action: {action}");
            }
        }
        let _ = writeln!(
            text,
            "\n{} supported, {} changed, {} unsupported",
            self.supported, self.changed, self.unsupported
        );
        text
    }
}

/// Analyzes a project for migration to the Rust implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct MigrationAnalysis;

impl MigrationAnalysis {
    /// Classifies every artifact in a project against the Rust surface.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the project root cannot be read or when the
    /// project exceeds the analysis budget.
    pub fn check(project_root: impl AsRef<Path>) -> Result<MigrationReport, Failure> {
        let root = project_root.as_ref();
        let metadata = fs::metadata(root).map_err(|error| {
            migration_failure(&format!(
                "Cannot inspect project '{}': {error}",
                root.display()
            ))
        })?;
        if !metadata.is_dir() {
            return Err(migration_failure(
                "The migration target must be a directory.",
            ));
        }

        // A handler in another language is only a divergence when nothing
        // says how to run it, so the registrations are read up front. A
        // malformed .tachyonrc is reported as registering nothing rather than
        // failing the check, since reporting is the whole point of the command.
        let interpreters = Interpreters::discover(root).unwrap_or_default();
        let mut findings = Vec::new();
        let mut signals = ProjectSignals::default();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let mut entries = fs::read_dir(&directory)
                .and_then(Iterator::collect::<Result<Vec<_>, _>>)
                .map_err(|error| {
                    migration_failure(&format!("Cannot read '{}': {error}", directory.display()))
                })?;
            entries.sort_by_key(fs::DirEntry::file_name);
            for entry in entries {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let entry_metadata = fs::symlink_metadata(&path).map_err(|error| {
                    migration_failure(&format!("Cannot inspect '{}': {error}", path.display()))
                })?;
                if entry_metadata.file_type().is_symlink() {
                    continue;
                }
                if entry_metadata.is_dir() {
                    if !IGNORED_DIRECTORIES.contains(&name.as_str()) && !name.starts_with('.') {
                        pending.push(path);
                    }
                    continue;
                }
                if entry_metadata.len() > MAX_SOURCE_BYTES {
                    continue;
                }
                let relative = portable_path(path.strip_prefix(root).unwrap_or(&path));
                classify_file(
                    root,
                    &relative,
                    &name,
                    &interpreters,
                    &mut signals,
                    &mut findings,
                )?;
                if findings.len() > MAX_FINDINGS {
                    return Err(migration_failure(
                        "Project exceeds the limit of 10,000 migration findings.",
                    ));
                }
            }
        }

        findings.extend(compile_views(root));
        findings.extend(signals.findings());
        findings.sort_by(|left, right| {
            left.source
                .cmp(&right.source)
                .then_with(|| left.feature.cmp(&right.feature))
        });
        findings.dedup();
        Ok(MigrationReport {
            contract_version: 1,
            supported: count(&findings, MigrationStatus::Supported),
            changed: count(&findings, MigrationStatus::Changed),
            unsupported: count(&findings, MigrationStatus::Unsupported),
            findings,
        })
    }
}

fn count(findings: &[MigrationFinding], status: MigrationStatus) -> usize {
    findings
        .iter()
        .filter(|finding| finding.status == status)
        .count()
}

/// Classifies one file and, for view sources, the constructs inside it.
/// Evidence, gathered while scanning, for divergences no single file names.
#[derive(Default)]
struct ProjectSignals {
    /// A source names the generated specification or its docs client.
    openapi: bool,
    /// A source reaches for telemetry.
    telemetry: bool,
    /// The project ships a client module, so it has state to lose.
    client_module: bool,
}

impl ProjectSignals {
    /// Records what one source reveals about the project as a whole.
    fn observe(&mut self, source: &str) {
        self.openapi |= source.contains("openapi.json") || source.contains("/api-docs");
        self.telemetry |= source.contains("telemetry") || source.contains("Telemetry");
    }

    /// Reports divergences that belong to the project rather than to a file.
    ///
    /// Some of what a legacy project relies on is never written down in one
    /// place: the server simply provided it. Each is reported only where the
    /// project shows it actually reaches that surface. Reporting them against
    /// every project would make the command cry wolf, and a check nothing can
    /// pass is a check every project learns to run with `--allow-unsupported`.
    ///
    /// This cannot see a consumer outside the project calling `/openapi.json`.
    /// `docs/PARITY_LEDGER.md` records that case.
    fn findings(&self) -> Vec<MigrationFinding> {
        let note = |feature: &str, status, detail: &str, action: &str| MigrationFinding {
            source: String::from("(project)"),
            feature: String::from(feature),
            status,
            detail: String::from(detail),
            action: Some(String::from(action)),
        };
        let mut project = Vec::new();
        if self.openapi {
            project.push(note(
                "server.openapi",
                MigrationStatus::Unsupported,
                "This project names the specification the legacy server generates, but \
                 /openapi.json and /api-docs do not exist here.",
                "Generate a specification outside Tachyon, or keep the legacy server \
                 for those endpoints.",
            ));
        }
        if self.telemetry {
            project.push(note(
                "server.telemetry",
                MigrationStatus::Unsupported,
                "This project reaches for telemetry, and no OpenTelemetry spans are \
                 emitted here.",
                "Instrument outside Tachyon if emitted spans are relied upon.",
            ));
        }
        if self.client_module {
            project.push(note(
                "client.navigation",
                MigrationStatus::Changed,
                "Tac renders each page in the client, while route navigation remains a \
                 real cross-document navigation with view transitions and prefetching, \
                 so in-memory state does not survive it.",
                "Hold state that must outlive a navigation in storage, a shared worker, \
                 or on the server.",
            ));
        }
        project
    }
}

fn classify_file(
    root: &Path,
    relative: &str,
    name: &str,
    interpreters: &Interpreters,
    signals: &mut ProjectSignals,
    findings: &mut Vec<MigrationFinding>,
) -> Result<(), Failure> {
    let Some(mut finding) = classify_name(relative, name, interpreters) else {
        return Ok(());
    };
    if finding.feature == "companion.polyglot"
        && finding.status == MigrationStatus::Changed
        && name.rsplit_once('.').is_some_and(|(_, extension)| {
            root.join(relative)
                .with_file_name(format!("tachyon-wasm.{extension}"))
                .is_file()
        })
    {
        finding.status = MigrationStatus::Supported;
        finding.detail = String::from(
            "The legacy tac.<language> companion is paired with a tachyon-wasm.<language> \
             source for the real compiler, so the same project builds under both implementations.",
        );
        finding.action = None;
    }
    findings.push(finding);
    let read = |what: &str| {
        fs::read_to_string(root.join(relative)).map_err(|error| {
            migration_failure(&format!("Cannot read {what} '{relative}': {error}"))
        })
    };
    if name == "tac.html" {
        let source = read("view")?;
        signals.observe(&source);
        classify_view(relative, &source, findings);
    }
    if matches!(name, "tac.js" | "tac.ts") {
        signals.observe(&read("client module")?);
        signals.client_module = true;
    }
    // Handlers and middleware are where a legacy project reaches for the
    // server-provided surfaces, so they are read for evidence even though
    // their classification does not depend on their contents.
    if matches!(
        name,
        "yon.js" | "yon.py" | "yon.ts" | "middleware.js" | "middleware.ts"
    ) {
        signals.observe(&read("handler")?);
    }
    Ok(())
}

/// Reports whether a file is a handler in a language with no built-in adapter.
fn is_other_language_handler(name: &str) -> bool {
    name.starts_with("yon.")
        && !matches!(
            Path::new(name)
                .extension()
                .and_then(std::ffi::OsStr::to_str),
            Some("html" | "css")
        )
}

/// Classifies a handler in any language by whether it can actually be run.
///
/// A handler in any language runs under the direct protocol, so the only
/// question is whether this project says how to run this one.
fn classify_other_language_handler(
    relative: &str,
    name: &str,
    interpreters: &Interpreters,
) -> MigrationFinding {
    let extension = name.trim_start_matches("yon.");
    let (status, detail, action) = if interpreters.command(extension).is_some() {
        (
            MigrationStatus::Supported,
            "Runs under the direct protocol using the interpreter registered in \
             .tachyonrc. The handler reads one JSON request from standard input and \
             writes one JSON response to standard output.",
            None,
        )
    } else {
        (
            MigrationStatus::Changed,
            "A handler in any language is supported, but this project does not say \
             how to run this one.",
            Some(
                "Register the extension in .tachyonrc, or make the file executable \
                 so it can run directly.",
            ),
        )
    };
    MigrationFinding {
        source: String::from(relative),
        feature: String::from("handler.other_language"),
        status,
        detail: String::from(detail),
        action: action.map(String::from),
    }
}

fn classify_route_schema(relative: &str) -> MigrationFinding {
    MigrationFinding {
        source: String::from(relative),
        feature: String::from("server.route_schema"),
        status: MigrationStatus::Unsupported,
        detail: String::from(
            "The Rust runtime does not discover or enforce legacy OPTIONS.schema.json request and response schemas.",
        ),
        action: Some(String::from(
            "Validate the request and response in the handler or at the deployment boundary, or keep the legacy server for this route contract.",
        )),
    }
}

/// Classifies one file by its conventional name.
fn classify_name(
    relative: &str,
    name: &str,
    interpreters: &Interpreters,
) -> Option<MigrationFinding> {
    let finding = |feature: &str, status, detail: &str, action: Option<&str>| {
        Some(MigrationFinding {
            source: String::from(relative),
            feature: String::from(feature),
            status,
            detail: String::from(detail),
            action: action.map(String::from),
        })
    };
    match name {
        "tac.html" => finding(
            "view.tac",
            MigrationStatus::Supported,
            "Tac view sources are discovered and compiled.",
            None,
        ),
        "yon.html" => finding(
            "view.yon",
            MigrationStatus::Unsupported,
            "Yon is REST-only and does not compile HTML templates.",
            Some(
                "Move application views to client/pages/**/tac.html, or return an explicit text/html response from a yon.* handler.",
            ),
        ),
        "yon.js" | "yon.py" => finding(
            "handler.supervised",
            MigrationStatus::Supported,
            "JavaScript and Python handlers run under Handler Protocol v1.",
            None,
        ),
        _ if is_other_language_handler(name) => Some(classify_other_language_handler(
            relative,
            name,
            interpreters,
        )),
        "tac.js" | "tac.ts" => finding(
            "companion.controller",
            MigrationStatus::Supported,
            "Emitted as a client module. TypeScript is compiled by the \
             TypeScript compiler, which must be version 6 or newer.",
            None,
        ),
        // A component companion in one of these languages is compiled to
        // WebAssembly by the language's own compiler, so the language is
        // supported and the file is not: a legacy companion was written for a
        // subset transpiler and declares nothing about its members.
        "tac.rs" | "tac.kt" | "tac.swift" | "tac.cs" | "tac.dart"
            if relative.contains("components/") =>
        {
            finding(
                "companion.polyglot",
                MigrationStatus::Changed,
                "A component companion in this language is compiled to WebAssembly \
                 by the language's own compiler, not by a subset transpiler, so it \
                 must be the language as the compiler defines it.",
                Some(
                    "Rewrite the companion as ordinary code in its language and \
                     declare the members the island may reach in tac. See ADR 0011.",
                ),
            )
        }
        "tac.py" | "tac.rs" | "tac.kt" | "tac.swift" | "tac.cs" | "tac.dart" => finding(
            "companion.polyglot",
            MigrationStatus::Unsupported,
            "A page renders before any companion runs, so a page companion is \
             JavaScript or TypeScript. Python has no browser target at all.",
            Some("Move the logic into a Yon handler or an island component."),
        ),
        "tac.css" | "yon.css" => finding(
            "companion.style",
            MigrationStatus::Supported,
            "Colocated stylesheets are emitted beside the route and linked from \
             the document.",
            None,
        ),
        "middleware.js" | "middleware.ts" => finding(
            "server.middleware",
            MigrationStatus::Supported,
            "Root middleware is consulted before and after every request, and may be \
             written in any language the project runs. Returning 204 continues; any \
             other status answers the request.",
            None,
        ),
        "OPTIONS.schema.json" => Some(classify_route_schema(relative)),
        "tachyon.json" => finding(
            "config.application",
            MigrationStatus::Supported,
            "The strict application contract is read for native builds.",
            None,
        ),
        ".tachyonrc" => finding(
            "config.interpreters",
            MigrationStatus::Supported,
            "Registers an interpreter per handler extension, and the interval for \
             each background worker.",
            None,
        ),
        _ => classify_by_location(relative, name),
    }
}

/// Classifies files that matter because of where they live.
fn classify_by_location(relative: &str, name: &str) -> Option<MigrationFinding> {
    let finding = |feature: &str, detail: &str, action: &str| {
        Some(MigrationFinding {
            source: String::from(relative),
            feature: String::from(feature),
            status: MigrationStatus::Unsupported,
            detail: String::from(detail),
            action: Some(String::from(action)),
        })
    };
    let supported = |feature: &str, detail: &str| {
        Some(MigrationFinding {
            source: String::from(relative),
            feature: String::from(feature),
            status: MigrationStatus::Supported,
            detail: String::from(detail),
            action: None,
        })
    };
    if name == "openapi.json" || relative.contains("api-docs") {
        return finding(
            "server.openapi",
            "The legacy /openapi.json and /api-docs endpoints are out of scope for \
             this implementation.",
            "Keep the legacy server for those endpoints, or generate a \
             specification outside Tachyon.",
        );
    }
    if relative.starts_with("server/workers/") {
        return supported(
            "server.worker",
            "A worker is a handler invoked on a schedule, so it may be written in any \
             language. Declare its interval under \"workers\" in .tachyonrc.",
        );
    }
    if relative.starts_with("server/repositories/") || relative.starts_with("server/services/") {
        // These are ordinary modules imported by handlers. They only matter
        // because handler adapters cannot import application dependencies.
        return finding(
            "handler.dependency",
            "A handler runs as its own process and does not import application \
             modules.",
            "Inline the logic into the handler.",
        );
    }
    None
}

/// Classifies constructs used inside one view source.
fn classify_view(relative: &str, source: &str, findings: &mut Vec<MigrationFinding>) {
    let mut push = |feature: &str, status, detail: &str, action: Option<&str>| {
        findings.push(MigrationFinding {
            source: String::from(relative),
            feature: String::from(feature),
            status,
            detail: String::from(detail),
            action: action.map(String::from),
        });
    };

    let mut controls = BTreeSet::new();
    for tag in ["if", "else", "for", "loop"] {
        if source.contains(&format!("<{tag}")) {
            controls.insert(tag);
        }
    }
    if !controls.is_empty() {
        push(
            "view.control_tags",
            MigrationStatus::Supported,
            "Control tags are validated by the compiler and rendered by the owning Tac client or Yon server runtime.",
            None,
        );
    }
    if source.contains('{') && source.contains('}') {
        push(
            "view.bindings",
            MigrationStatus::Supported,
            "Bounded binding expressions are evaluated with contextual escaping.",
            None,
        );
    }
    if source.contains("data-tac-on-") {
        push(
            "view.legacy_events",
            MigrationStatus::Changed,
            "Generated data-tac-on-* markers are replaced by on:<event> bindings in the Tac client render plan.",
            Some("Author an on:<event> binding and let the Tac renderer own dispatch."),
        );
    }
    if source.contains("tachyon-island") || source.contains("data-tachyon-island") {
        push(
            "view.islands",
            MigrationStatus::Changed,
            "Tac has no SSR islands; components are created and mounted entirely in the browser.",
            Some(
                "Use a registered Tac component tag; hydrate= is accepted only as a mount schedule.",
            ),
        );
    }
    if source.contains("<script") {
        push(
            "view.inline_script",
            MigrationStatus::Unsupported,
            "Inline scripts are not emitted into generated output.",
            Some(
                "Move behavior into a Tac companion module; keep only bounded literal page-state declarations inline.",
            ),
        );
    }
    if source.contains("<iframe") {
        push(
            "view.remote_frame",
            MigrationStatus::Changed,
            "An iframe becomes a bridge-free WebSurface in native builds and \
             requires an accessible name and an HTTPS source.",
            Some("Add aria-label and an https:// src."),
        );
    }
}

fn portable_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn migration_failure(message: &str) -> Failure {
    Failure::one(diagnostic(
        1701,
        message,
        Some(String::from(
            "Point `ty migrate check` at a readable Tachyon project directory.",
        )),
        source_span("tachyon.json", 0, 0),
    ))
}

/// Reports what the compiler itself refuses, by running it.
///
/// Classifying a project by file name and location can only describe the files
/// it recognises, never the constructs inside them. That is how this command
/// came to report a project as safe while `ty build` failed on it: a parallel
/// set of rules drifts from the compiler the moment either changes. So the
/// views are compiled here with the real parser, and its diagnostics become
/// findings verbatim.
fn compile_views(root: &Path) -> Vec<MigrationFinding> {
    // A component registry that will not load is itself a finding, not a
    // reason to report nothing about the views.
    let (names, mut findings) = match ComponentRegistry::discover(root) {
        Ok(registry) => (registry.names(), Vec::new()),
        Err(failure) => (
            BTreeSet::new(),
            failure.diagnostics().iter().map(compiler_finding).collect(),
        ),
    };

    for path in view_sources(root) {
        let relative = portable_path(path.strip_prefix(root).unwrap_or(&path));
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        if let Err(failure) = TemplateFrontend::compile(&source, &relative, &names) {
            findings.extend(failure.diagnostics().iter().map(compiler_finding));
        }
    }
    findings.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.detail.cmp(&right.detail))
    });
    findings.dedup();
    findings
}

/// Collects every view source under a project, skipping generated directories.
fn view_sources(root: &Path) -> Vec<std::path::PathBuf> {
    let mut sources = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if !IGNORED_DIRECTORIES.contains(&name.as_str()) && !name.starts_with('.') {
                    pending.push(entry.path());
                }
            } else if name == "tac.html" {
                sources.push(entry.path());
            }
        }
    }
    sources.sort();
    sources
}

/// Turns one compiler diagnostic into a migration finding.
fn compiler_finding(diagnostic: &tachyon_diagnostics::Diagnostic) -> MigrationFinding {
    MigrationFinding {
        source: diagnostic
            .spans
            .first()
            .map_or_else(|| String::from("(project)"), |span| span.file.clone()),
        feature: format!("view.compile.{}", diagnostic.code),
        status: MigrationStatus::Unsupported,
        detail: diagnostic.message.clone(),
        action: diagnostic.help.clone(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{MigrationAnalysis, MigrationStatus};
    use std::fs;
    use std::path::Path;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        fs::write(path, contents).expect("source");
    }

    #[test]
    fn a_supported_project_reports_clean() {
        let root = tempfile::tempdir().expect("project");
        write(
            &root.path().join("client/pages/tac.html"),
            "<main aria-label=\"Home\"><h1>Home</h1></main>",
        );
        write(
            &root.path().join("server/routes/yon.js"),
            "export class Handler { static GET() { return {} } }",
        );
        let report = MigrationAnalysis::check(root.path()).expect("report");
        assert!(report.is_clean(), "{}", report.to_text());
        assert_eq!(report.unsupported, 0);
        assert!(report.supported >= 2);
    }

    #[test]
    fn a_view_the_compiler_refuses_is_reported_before_the_build_fails() {
        // This command once reported a project as safe while ty build failed
        // on it, because it classified file names and never read a view. It
        // runs the real parser now, so its findings cannot drift from the
        // compiler's.
        let root = tempfile::tempdir().expect("project");
        write(
            &root.path().join("client/pages/tac.html"),
            // Trailing syntax is still a parse error; assignment is not, since
            // an island can resolve it.
            "<main aria-label=\"T\"><p>{bad syntax}</p></main>",
        );

        let report = MigrationAnalysis::check(root.path()).expect("report");
        assert!(!report.is_clean(), "{}", report.to_text());
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.feature.starts_with("view.compile."))
            .unwrap_or_else(|| panic!("no compile finding in {}", report.to_text()));
        assert_eq!(finding.source, "client/pages/tac.html");
        assert!(finding.detail.contains("syntax"), "{finding:?}");

        // A view the compiler accepts produces no compile finding at all.
        let clean = tempfile::tempdir().expect("project");
        write(
            &clean.path().join("client/pages/tac.html"),
            "<main aria-label=\"T\"><h1>T</h1></main>",
        );
        let report = MigrationAnalysis::check(clean.path()).expect("report");
        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.feature.starts_with("view.compile.")),
            "{}",
            report.to_text()
        );
    }

    #[test]
    fn implemented_features_are_not_reported_as_blockers() {
        // Every one of these was reported as unsupported after it had been
        // implemented, which is worse than saying nothing: it sends a
        // developer back to the legacy server for features that work.
        let root = tempfile::tempdir().expect("project");
        write(
            &root.path().join("client/pages/tac.html"),
            "<main aria-label=\"Home\"><h1>Home</h1></main>",
        );
        write(&root.path().join("middleware.js"), "export default {}");
        write(
            &root.path().join("server/workers/job.js"),
            "export default {}",
        );
        write(
            &root.path().join(".tachyonrc"),
            r#"{"interpreters":{"rb":["ruby"]}}"#,
        );
        write(&root.path().join("server/routes/a/yon.rb"), "# handler");

        let report = MigrationAnalysis::check(root.path()).expect("report");
        assert!(report.is_clean(), "{}", report.to_text());
        for feature in [
            "server.middleware",
            "server.worker",
            "config.interpreters",
            "handler.other_language",
        ] {
            let found = report
                .findings
                .iter()
                .find(|finding| finding.feature == feature)
                .unwrap_or_else(|| panic!("{feature} missing from {}", report.to_text()));
            assert_eq!(
                found.status,
                MigrationStatus::Supported,
                "{feature}: {}",
                report.to_text()
            );
        }
    }

    #[test]
    fn a_handler_language_with_no_interpreter_is_reported_as_changed() {
        // The file is a valid handler; the project just never said how to run
        // it, so the action is to register it rather than to rewrite it.
        let root = tempfile::tempdir().expect("project");
        write(&root.path().join("client/pages/tac.html"), "<main>x</main>");
        write(&root.path().join("server/routes/b/yon.swift"), "// handler");

        let report = MigrationAnalysis::check(root.path()).expect("report");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.feature == "handler.other_language")
            .expect("finding");
        assert_eq!(finding.status, MigrationStatus::Changed);
        assert!(
            finding
                .action
                .as_deref()
                .is_some_and(|action| action.contains(".tachyonrc")),
            "{finding:?}"
        );
    }

    #[test]
    fn surfaces_the_legacy_server_provided_are_reported_only_where_reached() {
        // A project that never touched them must report clean, or the check
        // becomes one every project runs with --allow-unsupported.
        let quiet = tempfile::tempdir().expect("project");
        write(
            &quiet.path().join("client/pages/tac.html"),
            "<main>x</main>",
        );
        write(
            &quiet.path().join("server/routes/yon.js"),
            "export class Handler { static GET() { return {} } }",
        );
        let report = MigrationAnalysis::check(quiet.path()).expect("report");
        assert!(report.is_clean(), "{}", report.to_text());

        // One that names them is told, once, against the project itself.
        let loud = tempfile::tempdir().expect("project");
        write(&loud.path().join("client/pages/tac.html"), "<main>x</main>");
        write(
            &loud.path().join("server/routes/yon.js"),
            "import { telemetry } from 'x'\nexport class Handler {}\n// openapi.json",
        );
        let report = MigrationAnalysis::check(loud.path()).expect("report");
        let named: Vec<&str> = report
            .findings
            .iter()
            .filter(|finding| finding.source == "(project)")
            .map(|finding| finding.feature.as_str())
            .collect();
        assert!(named.contains(&"server.openapi"), "{named:?}");
        assert!(named.contains(&"server.telemetry"), "{named:?}");
    }

    #[test]
    fn unsupported_and_changed_constructs_are_named_with_an_action() {
        let root = tempfile::tempdir().expect("project");
        write(&root.path().join("client/pages/tac.html"), "<main>x</main>");
        write(
            &root.path().join("client/pages/tac.js"),
            "export default {}",
        );
        write(&root.path().join("client/pages/tac.rs"), "fn main() {}");
        write(&root.path().join("server/routes/yon.rs"), "fn main() {}");
        write(&root.path().join("middleware.js"), "export default {}");
        write(&root.path().join("server/workers/job.js"), "export {}");
        write(&root.path().join(".tachyonrc"), "{}");

        let report = MigrationAnalysis::check(root.path()).expect("report");
        assert!(!report.is_clean());
        let features: Vec<&str> = report
            .findings
            .iter()
            .map(|finding| finding.feature.as_str())
            .collect();
        for expected in [
            "companion.controller",
            "companion.polyglot",
            "handler.other_language",
            "server.middleware",
            "server.worker",
            "config.interpreters",
        ] {
            assert!(features.contains(&expected), "missing {expected}");
        }
        for finding in &report.findings {
            if finding.status == MigrationStatus::Supported {
                continue;
            }
            assert!(
                finding.action.is_some(),
                "{} has no action",
                finding.feature
            );
        }
    }

    #[test]
    fn view_constructs_are_classified_from_source() {
        let root = tempfile::tempdir().expect("project");
        write(
            &root.path().join("client/pages/tac.html"),
            r#"<main><if condition="ok"><p>{title}</p></if>
               <button data-tac-on-click="go">Go</button>
               <script>alert(1)</script>
               <iframe src="https://example.test"></iframe></main>"#,
        );
        let report = MigrationAnalysis::check(root.path()).expect("report");
        let by_feature = |name: &str| {
            report
                .findings
                .iter()
                .find(|finding| finding.feature == name)
                .unwrap_or_else(|| panic!("missing {name}"))
                .status
        };
        assert_eq!(by_feature("view.control_tags"), MigrationStatus::Supported);
        assert_eq!(by_feature("view.bindings"), MigrationStatus::Supported);
        assert_eq!(by_feature("view.legacy_events"), MigrationStatus::Changed);
        assert_eq!(
            by_feature("view.inline_script"),
            MigrationStatus::Unsupported
        );
        assert_eq!(by_feature("view.remote_frame"), MigrationStatus::Changed);
    }

    #[test]
    fn generated_and_vendored_directories_are_never_analyzed() {
        let root = tempfile::tempdir().expect("project");
        write(&root.path().join("client/pages/tac.html"), "<main>x</main>");
        write(&root.path().join("node_modules/pkg/tac.rs"), "fn main() {}");
        write(&root.path().join("dist/web/tac.js"), "export {}");
        write(&root.path().join("target/debug/middleware.js"), "export {}");
        let report = MigrationAnalysis::check(root.path()).expect("report");
        assert!(report.is_clean(), "{}", report.to_text());
    }

    #[test]
    fn companions_that_fail_the_build_are_never_reported_as_supported() {
        // Regression: a companion the compiler rejects must never be reported
        // as supported, which would tell a maintainer their project migrates
        // cleanly when it will not build. css, js, and ts are now emitted and
        // are asserted elsewhere; these remain unsupported.
        for companion in ["tac.py", "tac.rs", "tac.kt", "tac.swift"] {
            let root = tempfile::tempdir().expect("project");
            write(&root.path().join("client/pages/tac.html"), "<main>x</main>");
            write(&root.path().join("client/pages").join(companion), "x");
            let report = MigrationAnalysis::check(root.path()).expect("report");
            let finding = report
                .findings
                .iter()
                .find(|finding| finding.source.ends_with(companion))
                .unwrap_or_else(|| panic!("{companion} was not classified"));
            assert_eq!(
                finding.status,
                MigrationStatus::Unsupported,
                "{companion} is reported as {:?} but fails the build",
                finding.status
            );
            assert!(finding.action.is_some(), "{companion} carries no action");
        }
    }

    #[test]
    fn legacy_route_schemas_are_reported_as_unsupported() {
        let root = tempfile::tempdir().expect("project");
        write(
            &root.path().join("server/routes/users/OPTIONS.schema.json"),
            "{}",
        );
        let report = MigrationAnalysis::check(root.path()).expect("report");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.feature == "server.route_schema")
            .expect("route schema finding");
        assert_eq!(finding.status, MigrationStatus::Unsupported);
        assert!(
            finding
                .action
                .as_ref()
                .is_some_and(|action| action.contains("Validate"))
        );
    }

    #[test]
    fn a_component_companion_in_a_wasm_language_is_a_rewrite_rather_than_a_wall() {
        // The language is supported and the legacy file is not: it was written
        // for a subset transpiler, and the real compiler will reject it.
        for companion in ["tac.rs", "tac.kt", "tac.swift", "tac.cs", "tac.dart"] {
            let root = tempfile::tempdir().expect("project");
            let component = root.path().join("client/components/panel");
            write(&component.join("tac.html"), "<div>x</div>");
            write(&component.join(companion), "x");
            let report = MigrationAnalysis::check(root.path()).expect("report");
            let finding = report
                .findings
                .iter()
                .find(|finding| finding.source.ends_with(companion))
                .unwrap_or_else(|| panic!("{companion} was not classified"));
            assert_eq!(finding.status, MigrationStatus::Changed, "{companion}");
            assert!(
                finding
                    .action
                    .as_ref()
                    .is_some_and(|action| action.contains("declare")),
                "{companion} does not say what to rewrite"
            );
        }
    }

    #[test]
    fn a_real_compiler_sidecar_makes_a_legacy_polyglot_component_dual_buildable() {
        let root = tempfile::tempdir().expect("project");
        let component = root.path().join("client/components/panel");
        write(&component.join("tac.html"), "<div>x</div>");
        write(&component.join("tac.rs"), "struct Legacy;");
        write(
            &component.join("tachyon-wasm.rs"),
            "fn real_compiler_source() {}",
        );

        let report = MigrationAnalysis::check(root.path()).expect("report");
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.source.ends_with("tac.rs"))
            .expect("legacy companion finding");
        assert_eq!(finding.status, MigrationStatus::Supported);
        assert!(finding.action.is_none());
    }

    #[test]
    fn a_missing_project_fails_closed() {
        let error = MigrationAnalysis::check(Path::new("/nonexistent/tachyon/project"))
            .expect_err("missing project");
        assert!(error.to_string().contains("TY1701"));
    }
}
