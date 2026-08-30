//! Scheduled-worker configuration.
//!
//! `.tachyonrc.workers` remains the bounded scheduling surface. Legacy
//! `.tachyonrc.interpreters` registration is rejected: Yon handlers are one of
//! the eight framework-owned languages, while other programs are reached from
//! an explicit `@Relay` delegate.

use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) const CONFIG_PATH: &str = ".tachyonrc";
const MAX_CONFIG_BYTES: u64 = 64 * 1_024;
const MAX_WORKERS: usize = 64;
const MIN_WORKER_SECONDS: u64 = 1;
const MAX_WORKER_SECONDS: u64 = 86_400;

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
    every_seconds: u64,
}

/// Workers scheduled by `.tachyonrc.workers`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Workers {
    schedules: BTreeMap<String, u64>,
}

impl Workers {
    /// Loads scheduled workers from a project root.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the configuration is unsafe or malformed,
    /// retains the removed `interpreters` field, or declares an invalid worker.
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
        Self::from_captured(Some(&bytes))
    }

    /// Parses worker schedules from bytes retained by project discovery.
    pub(crate) fn from_captured(bytes: Option<&[u8]>) -> Result<Self, Failure> {
        let Some(bytes) = bytes else {
            return Ok(Self::default());
        };
        if bytes.len() as u64 > MAX_CONFIG_BYTES {
            return Err(config_failure("Exceeds the 64 KiB limit."));
        }
        let source =
            std::str::from_utf8(bytes).map_err(|_| config_failure("Must be valid UTF-8."))?;
        let raw: RawConfiguration = serde_json::from_str(source)
            .map_err(|error| config_failure(&format!("Is not valid JSON: {error}")))?;
        if !raw.interpreters.is_empty() {
            return Err(config_failure(
                "The 'interpreters' field was removed. Move non-Yon execution behind @Relay.",
            ));
        }
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

    /// Returns each worker source path and interval in seconds.
    pub fn iter(&self) -> impl Iterator<Item = (&String, u64)> {
        self.schedules
            .iter()
            .map(|(path, seconds)| (path, *seconds))
    }

    /// Returns whether no worker is scheduled.
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
            "Keep only bounded worker schedules here. Yon runs its eight owned languages; \
             reach any other program from a @Delegate method carrying @Relay.",
        )),
        source_span(CONFIG_PATH, 0, CONFIG_PATH.len()),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::Workers;
    use std::fs;

    #[test]
    fn a_missing_configuration_schedules_nothing() {
        let root = tempfile::tempdir().expect("root");
        assert!(Workers::discover(root.path()).expect("none").is_empty());
    }

    #[test]
    fn interpreter_registration_is_rejected_with_relay_guidance() {
        let root = tempfile::tempdir().expect("project");
        fs::write(
            root.path().join(".tachyonrc"),
            r#"{"interpreters":{"rb":["ruby"]}}"#,
        )
        .expect("configuration");
        let error = Workers::discover(root.path()).expect_err("removed interpreter field");
        let rendered = error.to_string();
        assert!(rendered.contains("TY1502"), "{rendered}");
        assert!(rendered.contains("@Relay"), "{rendered}");
    }

    #[test]
    fn workers_are_scheduled_with_bounded_intervals() {
        let root = tempfile::tempdir().expect("root");
        fs::write(
            root.path().join(".tachyonrc"),
            r#"{"workers":{"server/workers/beat.py":{"every_seconds":30}}}"#,
        )
        .expect("configuration");
        let workers = Workers::discover(root.path()).expect("workers");
        assert_eq!(
            workers.iter().collect::<Vec<_>>(),
            vec![(&String::from("server/workers/beat.py"), 30)]
        );

        for source in [
            r#"{"workers":{"beat.py":{"every_seconds":30}}}"#,
            r#"{"workers":{"server/workers/../x.py":{"every_seconds":30}}}"#,
            r#"{"workers":{"server/workers/beat.py":{"every_seconds":0}}}"#,
            r#"{"workers":{"server/workers/beat.py":{"every_seconds":86401}}}"#,
        ] {
            let root = tempfile::tempdir().expect("root");
            fs::write(root.path().join(".tachyonrc"), source).expect("configuration");
            assert!(Workers::discover(root.path()).is_err(), "{source}");
        }
    }

    #[test]
    fn malformed_configurations_fail_closed() {
        for source in [r#"{"unknown":true}"#, "not json"] {
            let root = tempfile::tempdir().expect("root");
            fs::write(root.path().join(".tachyonrc"), source).expect("configuration");
            let error = Workers::discover(root.path()).expect_err(source);
            assert!(error.to_string().contains("TY1502"), "{source}: {error}");
        }
    }
}
