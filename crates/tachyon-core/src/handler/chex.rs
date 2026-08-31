//! Bounded CHEX validation of captured request schemas.

use crate::Failure;
use crate::external_command::{ToolOutput, run};
use crate::failure::diagnostic;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::{Instant, timeout_at};

const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_VERDICT_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const VALIDATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChexVerdict {
    Valid,
    Invalid,
}

/// Server-owned executable selection and concurrency budget.
#[derive(Clone, Debug)]
pub(crate) struct ChexValidator {
    program: PathBuf,
    permits: Arc<Semaphore>,
}

impl ChexValidator {
    pub(crate) fn from_environment() -> Self {
        Self {
            program: std::env::var_os("TAC_CHEX_BINARY")
                .map_or_else(|| PathBuf::from("chex"), PathBuf::from),
            permits: Arc::new(Semaphore::new(16)),
        }
    }

    async fn invoke(
        &self,
        schema: &Path,
        body: &[u8],
        deadline: Instant,
    ) -> Result<ChexVerdict, Failure> {
        let _permit = timeout_at(deadline, self.permits.acquire())
            .await
            .map_err(|_| chex_failure())?
            .map_err(|_| chex_failure())?;
        // Owned, mode-0600 input is never an authored path or command argument.
        // Its lifetime includes process-group settlement and pipe drainage.
        let mut input = tempfile::NamedTempFile::new().map_err(|_| chex_failure())?;
        input.write_all(body).map_err(|_| chex_failure())?;
        let mut argument = std::ffi::OsString::from("@");
        argument.push(input.path());
        let mut command = tokio::process::Command::new(&self.program);
        command
            .arg("validate")
            .arg(schema)
            .arg(argument)
            .current_dir(schema.parent().ok_or_else(chex_failure)?)
            .stdin(Stdio::null())
            .env_clear();
        // Windows process startup and executable lookup need only this baseline.
        for name in ["PATH", "SystemRoot", "WINDIR", "PATHEXT"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(chex_failure());
        }
        let output = run(&mut command, remaining, MAX_VERDICT_BYTES + 1)
            .await
            .map_err(|_| chex_failure())?;
        parse_verdict(&output)
    }
}

/// Immutable private schema storage, never under an application-owned cache.
#[derive(Clone, Debug)]
pub(crate) struct ChexSchema {
    directory: Arc<tempfile::TempDir>,
    fields: Vec<String>,
    validator: ChexValidator,
}

impl ChexSchema {
    pub(crate) fn stage(
        validator: &ChexValidator,
        schema: &serde_json::Value,
    ) -> Result<Self, Failure> {
        let fields = schema.as_object().ok_or_else(chex_failure)?;
        if fields.is_empty() {
            return Err(chex_failure());
        }
        let directory = tempfile::tempdir().map_err(|_| chex_failure())?;
        let bytes = serde_json::to_vec(schema).map_err(|_| chex_failure())?;
        std::fs::write(directory.path().join("request.schema.json"), bytes)
            .map_err(|_| chex_failure())?;
        // CHEX definition-checks the complete schema before validating data.
        // Optionalizing only root names gives {} a guaranteed successful data
        // shape without duplicating regex semantics or parsing error messages.
        let probe: serde_json::Map<String, serde_json::Value> = fields
            .iter()
            .map(|(name, value)| (format!("{}?", name.trim_end_matches('?')), value.clone()))
            .collect();
        if probe.len() != fields.len() {
            return Err(chex_failure());
        }
        std::fs::write(
            directory.path().join("definition.schema.json"),
            serde_json::to_vec(&probe).map_err(|_| chex_failure())?,
        )
        .map_err(|_| chex_failure())?;
        Ok(Self {
            directory: Arc::new(directory),
            fields: fields
                .keys()
                .map(|name| name.trim_end_matches('?').to_owned())
                .collect(),
            validator: validator.clone(),
        })
    }

    pub(crate) fn field_names(&self) -> &[String] {
        &self.fields
    }

    /// Rejects missing, incompatible validators and invalid schemas at startup.
    pub(crate) async fn preflight(&self, deadline: Instant) -> Result<(), Failure> {
        let verdict = self
            .validator
            .invoke(
                &self.directory.path().join("definition.schema.json"),
                b"{}",
                deadline.min(Instant::now() + VALIDATION_TIMEOUT),
            )
            .await?;
        if verdict != ChexVerdict::Valid {
            return Err(chex_failure());
        }
        Ok(())
    }

    pub(crate) async fn validate_until(
        &self,
        body: &[u8],
        deadline: Instant,
    ) -> Result<ChexVerdict, Failure> {
        if body.len() > MAX_INPUT_BYTES {
            return Ok(ChexVerdict::Invalid);
        }
        if !serde_json::from_slice::<serde_json::Value>(body).is_ok_and(|value| value.is_object()) {
            return Ok(ChexVerdict::Invalid);
        }
        self.validator
            .invoke(
                &self.directory.path().join("request.schema.json"),
                body,
                deadline,
            )
            .await
    }
}

fn parse_verdict(output: &ToolOutput) -> Result<ChexVerdict, Failure> {
    if output.stdout.len() > MAX_VERDICT_BYTES || output.stderr.len() > MAX_VERDICT_BYTES {
        return Err(chex_failure());
    }
    let answer: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| chex_failure())?;
    if answer["protocolVersion"].as_u64() != Some(1) || answer["op"] != "validate" {
        return Err(chex_failure());
    }
    match (output.status.code(), answer["ok"].as_bool()) {
        (Some(0), Some(true)) => Ok(ChexVerdict::Valid),
        (Some(1), Some(false)) if answer["error"]["name"] == "ValidationError" => {
            Ok(ChexVerdict::Invalid)
        }
        _ => Err(chex_failure()),
    }
}

fn chex_failure() -> Failure {
    Failure::one(diagnostic(
        2006,
        "CHEX could not validate the declared request schema.",
        Some(String::from(
            "Install CHEX on PATH or set TAC_CHEX_BINARY to a compatible executable; verify OPTIONS.schema.json. Validator execution and output are bounded; no requests bypass failed validation.",
        )),
        None,
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::{ChexSchema, ChexValidator, ChexVerdict, VALIDATION_TIMEOUT};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::time::Instant;

    #[tokio::test]
    async fn missing_validator_and_invalid_json_fail_closed_without_secrets() {
        let validator = ChexValidator {
            program: PathBuf::from("missing-secret-validator-executable"),
            permits: Arc::new(tokio::sync::Semaphore::new(1)),
        };
        let schema =
            ChexSchema::stage(&validator, &serde_json::json!({"name":"^.+$"})).expect("stage");
        let failure = schema
            .preflight(Instant::now() + VALIDATION_TIMEOUT)
            .await
            .expect_err("missing validator");
        assert!(!failure.to_string().contains("missing-secret"));
        assert!(failure.to_string().contains("TY2006"));
        for bytes in [b"not json".as_slice(), b"[]", b"null", b"", b"1"] {
            assert_eq!(
                schema
                    .validate_until(bytes, Instant::now() + VALIDATION_TIMEOUT)
                    .await
                    .expect("invalid data"),
                ChexVerdict::Invalid
            );
        }
        assert_eq!(
            schema
                .validate_until(
                    &vec![b' '; 1024 * 1024 + 1],
                    Instant::now() + VALIDATION_TIMEOUT
                )
                .await
                .expect("bounded input"),
            ChexVerdict::Invalid
        );
    }

    #[test]
    fn malformed_top_level_schemas_and_duplicate_optional_names_are_rejected() {
        let validator = ChexValidator::from_environment();
        for schema in [
            serde_json::json!({}),
            serde_json::json!([]),
            serde_json::json!(null),
            serde_json::json!({"name":"^.*$","name?":"^.*$"}),
        ] {
            assert!(ChexSchema::stage(&validator, &schema).is_err());
        }
    }

    #[cfg(unix)]
    fn fake_validator(source: &str) -> (tempfile::TempDir, ChexValidator) {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().expect("fake validator");
        let program = directory.path().join("validator");
        std::fs::write(&program, format!("#!/usr/bin/env python3\n{source}\n")).expect("program");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
            .expect("executable");
        (
            directory,
            ChexValidator {
                program,
                permits: Arc::new(tokio::sync::Semaphore::new(1)),
            },
        )
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn strict_typed_verdicts_do_not_echo_validator_output() {
        for (source, expected) in [
            (
                "print('{\"protocolVersion\":1,\"op\":\"validate\",\"ok\":true}')",
                Some(ChexVerdict::Valid),
            ),
            (
                "import sys\nprint('{\"protocolVersion\":1,\"op\":\"validate\",\"ok\":false,\"error\":{\"name\":\"ValidationError\",\"message\":\"PRIVATE_REQUEST_TOKEN\"}}')\nsys.exit(1)",
                Some(ChexVerdict::Invalid),
            ),
            ("print('{\"ok\":true}')", None),
            ("print('PRIVATE_REQUEST_TOKEN')", None),
            (
                "import sys\nprint('{\"protocolVersion\":1,\"op\":\"validate\",\"ok\":true}')\nsys.exit(1)",
                None,
            ),
            (
                "import sys\nprint('{\"protocolVersion\":1,\"op\":\"validate\",\"ok\":false,\"error\":{\"name\":\"SchemaLoadError\",\"message\":\"PRIVATE_REQUEST_TOKEN\"}}')\nsys.exit(1)",
                None,
            ),
            ("print('x' * (4 * 1024 * 1024 + 1))", None),
        ] {
            let (_directory, validator) = fake_validator(source);
            let schema =
                ChexSchema::stage(&validator, &serde_json::json!({"name":"^.+$"})).expect("stage");
            let result = schema
                .validate_until(br#"{"name":"Ada"}"#, Instant::now() + VALIDATION_TIMEOUT)
                .await;
            match expected {
                Some(verdict) => assert_eq!(result.expect("typed verdict"), verdict),
                None => assert!(
                    !result
                        .expect_err("fail closed")
                        .to_string()
                        .contains("PRIVATE_REQUEST_TOKEN")
                ),
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn deadline_includes_admission_and_reaps_descendants() {
        let (directory, validator) = fake_validator(
            "import subprocess,time,pathlib\nchild=subprocess.Popen(['sleep','30'])\npathlib.Path(__file__).with_suffix('.pid').write_text(str(child.pid))\ntime.sleep(30)",
        );
        let schema =
            ChexSchema::stage(&validator, &serde_json::json!({"name":"^.+$"})).expect("stage");
        let started = Instant::now();
        assert!(
            schema
                .validate_until(b"{}", started + std::time::Duration::from_secs(2))
                .await
                .is_err()
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        let pid = std::fs::read_to_string(directory.path().join("validator.pid"))
            .expect("descendant started");
        let alive = std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("probe")
            .success();
        assert!(!alive, "validator descendant survives deadline");
        let _permit = validator
            .permits
            .acquire()
            .await
            .expect("reserve admission");
        let started = Instant::now();
        assert!(
            schema
                .validate_until(b"{}", started + std::time::Duration::from_millis(40))
                .await
                .is_err()
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancelling_validation_kills_its_process_group() {
        let (directory, validator) = fake_validator(
            "import os,pathlib,time\npathlib.Path(__file__).with_suffix('.pid').write_text(str(os.getpid()))\ntime.sleep(30)",
        );
        let schema =
            ChexSchema::stage(&validator, &serde_json::json!({"name":"^.+$"})).expect("stage");
        let pid_file = directory.path().join("validator.pid");
        let mut invocation =
            Box::pin(schema.validate_until(b"{}", Instant::now() + VALIDATION_TIMEOUT));
        let observed = async {
            while !pid_file.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        };
        tokio::select! { _ = &mut invocation => panic!("expected active child"), () = observed => {} }
        // Drop the entire owned future, as server shutdown does.
        drop(invocation);
        let pid = std::fs::read_to_string(pid_file).expect("validator pid");
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let alive = std::process::Command::new("kill")
                    .args(["-0", pid.trim()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .expect("probe")
                    .success();
                if !alive {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("cancelled process reaped");
    }
}
