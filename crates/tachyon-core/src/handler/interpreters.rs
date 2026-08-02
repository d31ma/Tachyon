//! Interpreter registration for handlers in any language.
//!
//! The legacy implementation ships a bespoke runner per language, so adding a
//! language means writing and maintaining a new adapter in that language. This
//! module takes the other route: one protocol simple enough that a handler in
//! any language satisfies it directly, and a `.tachyonrc` that says how to run
//! the file.
//!
//! A direct handler reads one JSON request object from standard input until
//! end of file and writes one JSON response object to standard output. There
//! is no framing to implement, because one process serves exactly one request.

use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Configuration file naming interpreters for handler extensions.
pub(crate) const CONFIG_PATH: &str = ".tachyonrc";
/// Largest configuration file the loader will read.
const MAX_CONFIG_BYTES: u64 = 64 * 1_024;
/// Largest number of interpreter registrations accepted.
const MAX_INTERPRETERS: usize = 64;
/// Largest number of arguments accepted in one interpreter command.
const MAX_COMMAND_PARTS: usize = 16;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfiguration {
    #[serde(default)]
    interpreters: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    workers: BTreeMap<String, RawWorker>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorker {
    /// How often the worker runs, in seconds.
    every_seconds: u64,
}

/// Interpreter commands registered for handler extensions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Interpreters {
    commands: BTreeMap<String, Vec<String>>,
}

impl Interpreters {
    /// Loads `.tachyonrc` from a project root, if present.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the file is unreadable, oversized, malformed,
    /// or declares an interpreter that is not a bounded command.
    pub fn discover(project_root: &Path) -> Result<Self, Failure> {
        let path = project_root.join(CONFIG_PATH);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(config_failure(&format!("Cannot inspect: {error}"))),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(config_failure("Must be a regular, non-symlinked file."));
        }
        if metadata.len() > MAX_CONFIG_BYTES {
            return Err(config_failure("Exceeds the 64 KiB limit."));
        }
        let bytes =
            fs::read(&path).map_err(|error| config_failure(&format!("Cannot read: {error}")))?;
        let source =
            std::str::from_utf8(&bytes).map_err(|_| config_failure("Must be valid UTF-8."))?;
        let raw: RawConfiguration = serde_json::from_str(source)
            .map_err(|error| config_failure(&format!("Is not valid JSON: {error}")))?;
        Self::validate(raw.interpreters)
    }

    fn validate(raw: BTreeMap<String, Vec<String>>) -> Result<Self, Failure> {
        if raw.len() > MAX_INTERPRETERS {
            return Err(config_failure("Declares more than 64 interpreters."));
        }
        let mut commands = BTreeMap::new();
        for (extension, command) in raw {
            let normalized = extension.trim_start_matches('.').to_ascii_lowercase();
            if normalized.is_empty()
                || normalized.len() > 16
                || !normalized.bytes().all(|byte| byte.is_ascii_alphanumeric())
            {
                return Err(config_failure(&format!(
                    "Interpreter key '{extension}' is not a bounded file extension."
                )));
            }
            if command.is_empty() || command.len() > MAX_COMMAND_PARTS {
                return Err(config_failure(&format!(
                    "Interpreter for '{extension}' must be 1 to 16 command parts."
                )));
            }
            if command
                .iter()
                .any(|part| part.is_empty() || part.chars().any(char::is_control))
            {
                return Err(config_failure(&format!(
                    "Interpreter for '{extension}' has an empty or control-bearing argument."
                )));
            }
            commands.insert(normalized, command);
        }
        Ok(Self { commands })
    }

    /// Returns the interpreter command registered for a file extension.
    #[must_use]
    pub fn command(&self, extension: &str) -> Option<&[String]> {
        self.commands
            .get(&extension.to_ascii_lowercase())
            .map(Vec::as_slice)
    }

    /// Returns every registration, as an extension and its command.
    pub fn commands(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.commands
            .iter()
            .map(|(extension, command)| (extension.as_str(), command.as_slice()))
    }

    /// Returns whether any interpreter is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Largest number of scheduled workers accepted.
const MAX_WORKERS: usize = 64;
/// Shortest interval a worker may declare.
const MIN_WORKER_SECONDS: u64 = 1;
/// Longest interval a worker may declare, one day.
const MAX_WORKER_SECONDS: u64 = 86_400;

/// Workers scheduled by `.tachyonrc`.
///
/// A worker is a handler invoked on a schedule instead of by a request, so it
/// reuses the same protocol, supervision, and bounds. Nothing worker-specific
/// exists beyond the interval.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Workers {
    schedules: BTreeMap<String, u64>,
}

impl Workers {
    /// Loads scheduled workers from a project root.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the configuration is malformed or a worker
    /// declares an interval outside the supported range.
    pub fn discover(project_root: &Path) -> Result<Self, Failure> {
        let path = project_root.join(CONFIG_PATH);
        let Ok(bytes) = fs::read(&path) else {
            return Ok(Self::default());
        };
        let source =
            std::str::from_utf8(&bytes).map_err(|_| config_failure("Must be valid UTF-8."))?;
        let raw: RawConfiguration = serde_json::from_str(source)
            .map_err(|error| config_failure(&format!("Is not valid JSON: {error}")))?;
        if raw.workers.len() > MAX_WORKERS {
            return Err(config_failure("Declares more than 64 workers."));
        }
        let mut schedules = BTreeMap::new();
        for (source_path, worker) in raw.workers {
            if !source_path.starts_with("server/workers/") || source_path.contains("..") {
                return Err(config_failure(&format!(
                    "Worker '{source_path}' must be a path under server/workers/."
                )));
            }
            if !(MIN_WORKER_SECONDS..=MAX_WORKER_SECONDS).contains(&worker.every_seconds) {
                return Err(config_failure(&format!(
                    "Worker '{source_path}' must run every 1 to 86400 seconds."
                )));
            }
            schedules.insert(source_path, worker.every_seconds);
        }
        Ok(Self { schedules })
    }

    /// Returns each worker source path and its interval in seconds.
    pub fn iter(&self) -> impl Iterator<Item = (&String, u64)> {
        self.schedules
            .iter()
            .map(|(path, seconds)| (path, *seconds))
    }

    /// Returns whether any worker is scheduled.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.schedules.is_empty()
    }
}

fn config_failure(message: &str) -> Failure {
    Failure::one(diagnostic(
        1502,
        format!("{CONFIG_PATH} is invalid. {message}"),
        Some(String::from(
            "Declare interpreters as an object mapping an extension to a command, \
             such as rb to ruby.",
        )),
        source_span(CONFIG_PATH, 0, CONFIG_PATH.len()),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::Interpreters;
    use std::fs;

    #[test]
    fn a_missing_configuration_registers_nothing() {
        let root = tempfile::tempdir().expect("root");
        assert!(
            Interpreters::discover(root.path())
                .expect("none")
                .is_empty()
        );
    }

    #[test]
    fn interpreters_are_registered_by_extension() {
        let root = tempfile::tempdir().expect("root");
        fs::write(
            root.path().join(".tachyonrc"),
            r#"{"interpreters":{"rb":["ruby"],".GO":["go","run"]}}"#,
        )
        .expect("configuration");
        let interpreters = Interpreters::discover(root.path()).expect("registered");
        assert_eq!(
            interpreters.command("rb"),
            Some(["ruby".to_owned()].as_slice())
        );
        // A leading dot and letter case are normalised away.
        assert_eq!(
            interpreters.command("go"),
            Some(["go".to_owned(), "run".to_owned()].as_slice())
        );
        assert!(interpreters.command("rs").is_none());
    }

    #[test]
    fn workers_are_scheduled_with_bounded_intervals() {
        use super::Workers;

        let root = tempfile::tempdir().expect("root");
        fs::write(
            root.path().join(".tachyonrc"),
            r#"{"workers":{"server/workers/beat.rb":{"every_seconds":30}}}"#,
        )
        .expect("configuration");
        let workers = Workers::discover(root.path()).expect("workers");
        assert_eq!(
            workers.iter().collect::<Vec<_>>(),
            vec![(&String::from("server/workers/beat.rb"), 30)]
        );

        // A worker outside server/workers, or with an interval outside the
        // supported range, must be refused rather than silently ignored.
        for source in [
            r#"{"workers":{"beat.rb":{"every_seconds":30}}}"#,
            r#"{"workers":{"server/workers/../x.rb":{"every_seconds":30}}}"#,
            r#"{"workers":{"server/workers/beat.rb":{"every_seconds":0}}}"#,
            r#"{"workers":{"server/workers/beat.rb":{"every_seconds":86401}}}"#,
        ] {
            let root = tempfile::tempdir().expect("root");
            fs::write(root.path().join(".tachyonrc"), source).expect("configuration");
            assert!(Workers::discover(root.path()).is_err(), "{source}");
        }
    }

    #[test]
    fn malformed_configurations_fail_closed() {
        for source in [
            r#"{"interpreters":{"rb":[]}}"#,
            r#"{"interpreters":{"":["ruby"]}}"#,
            r#"{"interpreters":{"r b":["ruby"]}}"#,
            r#"{"interpreters":{"rb":[""]}}"#,
            r#"{"unknown":true}"#,
            "not json",
        ] {
            let root = tempfile::tempdir().expect("root");
            fs::write(root.path().join(".tachyonrc"), source).expect("configuration");
            let error = Interpreters::discover(root.path()).expect_err(source);
            assert!(error.to_string().contains("TY1502"), "{source}: {error}");
        }
    }
}
