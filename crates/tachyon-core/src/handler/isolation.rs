use super::readiness::YonLanguage;
use crate::Failure;
use crate::failure::diagnostic;
use crate::handler::HandlerLanguage;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

const ISOLATION_ENV: &str = "YON_ISOLATION";
const DRIVER_ENV: &str = "YON_FIRECRACKER_DRIVER";
const POOL_ENV: &str = "YON_FIRECRACKER_POOL";
const VCPUS_ENV: &str = "YON_FIRECRACKER_VCPUS";
const MEMORY_ENV: &str = "YON_FIRECRACKER_MEMORY_MIB";
const EGRESS_ENV: &str = "YON_FIRECRACKER_EGRESS";

const DEFAULT_POOL: &str = "default";
const DEFAULT_VCPUS: u8 = 1;
const DEFAULT_MEMORY_MIB: u32 = 256;
const MIN_MEMORY_MIB: u32 = 128;
const MAX_MEMORY_MIB: u32 = 32_768;
const MAX_VCPUS: u8 = 32;

/// Deployment-selected operating-system isolation for Yon handlers.
///
/// Applications cannot select this policy from project files. Operators set
/// it through the process environment before Tachyon starts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum YonIsolationPolicy {
    /// Execute one directly supervised child process per request.
    #[default]
    Process,
    /// Delegate execution to a Firecracker control program.
    Firecracker(FirecrackerIsolation),
}

impl YonIsolationPolicy {
    /// Reads the complete isolation policy from environment variables.
    ///
    /// # Errors
    ///
    /// Returns `TY2010` when the mode or one of its values is invalid,
    /// incomplete, unsafe, or cannot be represented as Unicode.
    pub fn from_environment() -> Result<Self, Failure> {
        Self::from_lookup(|name| std::env::var_os(name))
    }

    pub(crate) fn uses_direct_handler_protocol(&self) -> bool {
        matches!(self, Self::Process)
    }

    pub(crate) fn firecracker(&self) -> Option<&FirecrackerIsolation> {
        match self {
            Self::Process => None,
            Self::Firecracker(policy) => Some(policy),
        }
    }

    fn from_lookup<F>(lookup: F) -> Result<Self, Failure>
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let firecracker_only = [DRIVER_ENV, POOL_ENV, VCPUS_ENV, MEMORY_ENV, EGRESS_ENV];
        let configured_firecracker_values = firecracker_only
            .iter()
            .copied()
            .filter(|name| lookup(name).is_some())
            .collect::<Vec<_>>();
        let mode =
            optional_unicode(&lookup, ISOLATION_ENV)?.unwrap_or_else(|| String::from("process"));
        match mode.as_str() {
            "process" if !configured_firecracker_values.is_empty() => {
                Err(configuration_failure(format!(
                    "{} require {ISOLATION_ENV}=firecracker and cannot be used with process isolation.",
                    configured_firecracker_values.join(", ")
                )))
            }
            "process" => Ok(Self::Process),
            "firecracker" => FirecrackerIsolation::from_lookup(&lookup).map(Self::Firecracker),
            _ => Err(configuration_failure(format!(
                "{ISOLATION_ENV} must be 'process' or 'firecracker', not '{mode}'."
            ))),
        }
    }
}

/// Validated settings passed to the Firecracker control program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirecrackerIsolation {
    driver: PathBuf,
    pool: String,
    vcpus: u8,
    memory_mib: u32,
}

impl FirecrackerIsolation {
    fn from_lookup<F>(lookup: &F) -> Result<Self, Failure>
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let driver = required_unicode(lookup, DRIVER_ENV).map(PathBuf::from)?;
        validate_driver(&driver)?;
        let pool =
            optional_unicode(lookup, POOL_ENV)?.unwrap_or_else(|| String::from(DEFAULT_POOL));
        if !valid_pool(&pool) {
            return Err(configuration_failure(format!(
                "{POOL_ENV} must contain 1 to 64 ASCII letters, digits, underscores, or hyphens."
            )));
        }
        let vcpus = parse_number(
            optional_unicode(lookup, VCPUS_ENV)?,
            VCPUS_ENV,
            DEFAULT_VCPUS,
            1,
            MAX_VCPUS,
        )?;
        let memory_mib = parse_number(
            optional_unicode(lookup, MEMORY_ENV)?,
            MEMORY_ENV,
            DEFAULT_MEMORY_MIB,
            MIN_MEMORY_MIB,
            MAX_MEMORY_MIB,
        )?;
        let egress = optional_unicode(lookup, EGRESS_ENV)?.unwrap_or_else(|| String::from("deny"));
        if egress != "deny" {
            return Err(configuration_failure(format!(
                "{EGRESS_ENV} currently supports only 'deny'."
            )));
        }
        Ok(Self {
            driver,
            pool,
            vcpus,
            memory_mib,
        })
    }

    pub(crate) fn driver(&self) -> &Path {
        &self.driver
    }

    pub(crate) fn append_arguments(&self, command: &mut tokio::process::Command) {
        command
            .arg("invoke")
            .arg("--protocol")
            .arg("handler-v1")
            .arg("--pool")
            .arg(&self.pool)
            .arg("--vcpus")
            .arg(self.vcpus.to_string())
            .arg("--memory-mib")
            .arg(self.memory_mib.to_string())
            .arg("--egress")
            .arg("deny");
    }
}

fn optional_unicode<F>(lookup: &F, name: &str) -> Result<Option<String>, Failure>
where
    F: Fn(&str) -> Option<OsString>,
{
    lookup(name)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| configuration_failure(format!("{name} must contain valid Unicode.")))
        })
        .transpose()
}

fn required_unicode<F>(lookup: &F, name: &str) -> Result<String, Failure>
where
    F: Fn(&str) -> Option<OsString>,
{
    optional_unicode(lookup, name)?.ok_or_else(|| {
        configuration_failure(format!(
            "{name} is required when {ISOLATION_ENV}=firecracker."
        ))
    })
}

fn parse_number<T>(
    value: Option<String>,
    name: &str,
    default: T,
    minimum: T,
    maximum: T,
) -> Result<T, Failure>
where
    T: Copy + Ord + std::str::FromStr,
{
    let Some(value) = value else {
        return Ok(default);
    };
    let parsed = value
        .parse::<T>()
        .map_err(|_| configuration_failure(format!("{name} must be a base-10 integer.")))?;
    if parsed < minimum || parsed > maximum {
        return Err(configuration_failure(format!(
            "{name} is outside its supported range."
        )));
    }
    Ok(parsed)
}

fn validate_driver(driver: &Path) -> Result<(), Failure> {
    if !driver.is_absolute() {
        return Err(configuration_failure(format!(
            "{DRIVER_ENV} must be an absolute path."
        )));
    }
    let metadata = fs::symlink_metadata(driver)
        .map_err(|error| configuration_failure(format!("Cannot inspect {DRIVER_ENV}: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(configuration_failure(format!(
            "{DRIVER_ENV} must name a regular, non-symlinked file."
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o111 == 0 {
            return Err(configuration_failure(format!(
                "{DRIVER_ENV} is not executable."
            )));
        }
        if mode & 0o022 != 0 {
            return Err(configuration_failure(format!(
                "{DRIVER_ENV} must not be writable by its group or other users."
            )));
        }
    }
    Ok(())
}

fn valid_pool(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn configuration_failure(message: impl Into<String>) -> Failure {
    Failure::one(diagnostic(
        2010,
        message,
        Some(String::from(
            "Set a complete environment-only Yon isolation policy before starting Tachyon.",
        )),
        None,
    ))
}

pub(crate) fn apply_backend_environment(
    policy: &YonIsolationPolicy,
    command: &mut tokio::process::Command,
) {
    if matches!(policy, YonIsolationPolicy::Firecracker(_)) {
        command.env(ISOLATION_ENV, OsStr::new("firecracker"));
    }
}

pub(crate) fn validate_backend_language(
    policy: &YonIsolationPolicy,
    language: HandlerLanguage,
) -> Result<(), Failure> {
    if matches!(policy, YonIsolationPolicy::Firecracker(_))
        && !matches!(
            language,
            HandlerLanguage::JavaScript | HandlerLanguage::Python
        )
    {
        return Err(Failure::one(diagnostic(
            2010,
            format!(
                "Firecracker isolation cannot invoke a prepared {} handler.",
                language.name()
            ),
            Some(String::from(
                "The current Firecracker driver contract transfers only project-contained \
                 JavaScript and Python source. Use process isolation for TypeScript, Java, PHP, \
                 Kotlin, C#, or Rust until an artifact-transfer contract is implemented.",
            )),
            None,
        )));
    }
    Ok(())
}

pub(crate) fn validate_backend_yon_language(
    policy: &YonIsolationPolicy,
    language: YonLanguage,
) -> Result<(), Failure> {
    if matches!(policy, YonIsolationPolicy::Firecracker(_))
        && !matches!(language, YonLanguage::JavaScript | YonLanguage::Python)
    {
        return Err(Failure::one(diagnostic(
            2010,
            format!(
                "Firecracker isolation cannot invoke a prepared {} handler.",
                language.family()
            ),
            Some(String::from(
                "The current Firecracker driver contract transfers only project-contained \
                 JavaScript and Python source. Use process isolation for TypeScript, Java, PHP, \
                 Kotlin, C#, or Rust until an artifact-transfer contract is implemented.",
            )),
            None,
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{
        YonIsolationPolicy, configuration_failure, validate_backend_language,
        validate_backend_yon_language,
    };
    use crate::handler::{HandlerLanguage, YonLanguage};
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::time::Duration;

    fn parse(values: &[(&str, &str)]) -> Result<YonIsolationPolicy, crate::Failure> {
        let values = values
            .iter()
            .map(|(name, value)| (String::from(*name), OsString::from(*value)))
            .collect::<BTreeMap<_, _>>();
        YonIsolationPolicy::from_lookup(|name| values.get(name).cloned())
    }

    #[test]
    fn process_is_the_environment_default() {
        assert_eq!(
            parse(&[]).expect("default policy"),
            YonIsolationPolicy::Process
        );
        assert_eq!(
            parse(&[("YON_ISOLATION", "process")]).expect("process policy"),
            YonIsolationPolicy::Process
        );
    }

    #[test]
    fn invalid_or_partial_environment_policy_fails_closed() {
        for values in [
            vec![("YON_ISOLATION", "container")],
            vec![("YON_ISOLATION", "firecracker")],
            vec![("YON_FIRECRACKER_DRIVER", "/tmp/driver")],
            vec![
                ("YON_ISOLATION", "process"),
                ("YON_FIRECRACKER_POOL", "configured-but-disabled"),
            ],
            vec![
                ("YON_ISOLATION", "firecracker"),
                ("YON_FIRECRACKER_DRIVER", "relative/driver"),
            ],
        ] {
            assert!(
                parse(&values)
                    .expect_err("invalid policy")
                    .to_string()
                    .contains("TY2010")
            );
        }
        assert!(
            configuration_failure("invalid")
                .to_string()
                .contains("TY2010")
        );
    }

    #[test]
    fn firecracker_support_is_limited_to_source_languages() {
        let process = YonIsolationPolicy::Process;
        assert!(validate_backend_language(&process, HandlerLanguage::Direct).is_ok());

        // The driver value is irrelevant to this pure contract check.
        let firecracker = YonIsolationPolicy::Firecracker(super::FirecrackerIsolation {
            driver: std::path::PathBuf::from("driver"),
            pool: String::from("default"),
            vcpus: 1,
            memory_mib: 256,
        });
        for language in [HandlerLanguage::JavaScript, HandlerLanguage::Python] {
            assert!(validate_backend_language(&firecracker, language).is_ok());
            assert!(matches!(language.adapter(), "javascript.v1" | "python.v1"));
        }
        for language in [HandlerLanguage::TypeScript, HandlerLanguage::Direct] {
            let failure = validate_backend_language(&firecracker, language)
                .expect_err("prepared artifact must be refused");
            assert!(failure.to_string().contains("TY2010"), "{failure}");
        }
        for language in [YonLanguage::JavaScript, YonLanguage::Python] {
            assert!(validate_backend_yon_language(&firecracker, language).is_ok());
        }
        for language in [
            YonLanguage::TypeScript,
            YonLanguage::Java,
            YonLanguage::Php,
            YonLanguage::Kotlin,
            YonLanguage::CSharp,
            YonLanguage::Rust,
        ] {
            let failure = validate_backend_yon_language(&firecracker, language)
                .expect_err("unsupported deployment language must fail readiness");
            assert!(failure.to_string().contains("TY2010"), "{failure}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn firecracker_policy_is_bounded_and_deny_by_default() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("temporary directory");
        let driver = directory.path().join("driver");
        fs::write(&driver, "#!/bin/sh\nexit 0\n").expect("driver");
        fs::set_permissions(&driver, fs::Permissions::from_mode(0o700)).expect("permissions");
        let path = driver.to_string_lossy();
        let policy = parse(&[
            ("YON_ISOLATION", "firecracker"),
            ("YON_FIRECRACKER_DRIVER", &path),
            ("YON_FIRECRACKER_POOL", "tenant_a"),
            ("YON_FIRECRACKER_VCPUS", "2"),
            ("YON_FIRECRACKER_MEMORY_MIB", "512"),
        ])
        .expect("firecracker policy");
        assert!(policy.firecracker().is_some());

        for (name, value) in [
            ("YON_FIRECRACKER_POOL", "tenant/a"),
            ("YON_FIRECRACKER_VCPUS", "0"),
            ("YON_FIRECRACKER_VCPUS", "33"),
            ("YON_FIRECRACKER_MEMORY_MIB", "127"),
            ("YON_FIRECRACKER_MEMORY_MIB", "32769"),
            ("YON_FIRECRACKER_EGRESS", "allow"),
        ] {
            let values = [
                ("YON_ISOLATION", "firecracker"),
                ("YON_FIRECRACKER_DRIVER", path.as_ref()),
                (name, value),
            ];
            assert!(
                parse(&values)
                    .expect_err("bounded policy")
                    .to_string()
                    .contains("TY2010")
            );
        }

        fs::set_permissions(&driver, fs::Permissions::from_mode(0o722))
            .expect("unsafe permissions");
        assert!(
            parse(&[
                ("YON_ISOLATION", "firecracker"),
                ("YON_FIRECRACKER_DRIVER", path.as_ref()),
            ])
            .expect_err("writable driver")
            .to_string()
            .contains("TY2010")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn firecracker_driver_receives_framed_protocol_and_bounded_policy() {
        use crate::{
            HandlerCancellation, HandlerSource, HandlerSupervisor, HandlerSupervisorOptions,
        };
        use std::fmt::Write as _;
        use std::os::unix::fs::PermissionsExt;
        use tachyon_contracts::{HandlerBody, HandlerBodyEncoding, HandlerRequest, HttpMethod};

        let directory = tempfile::tempdir().expect("temporary directory");
        let route = directory.path().join("server/routes/example");
        fs::create_dir_all(&route).expect("route directory");
        let handler = route.join("yon.py");
        fs::write(
            &handler,
            "@Controller\nclass ExampleController:\n    pass\n",
        )
        .expect("source");
        let source = HandlerSource::discover(
            directory.path(),
            std::path::Path::new("server/routes/example/yon.py"),
        )
        .expect("handler source");
        let request = HandlerRequest::route(
            String::from("firecracker_test"),
            String::from("/example"),
            HttpMethod::Get,
        );
        let request_bytes = crate::handler::frame::request_frame(&request).expect("request frame");
        let body = HandlerBody {
            encoding: HandlerBodyEncoding::Utf8,
            data: String::from("isolated"),
        };
        let response = tachyon_contracts::HandlerResponse::success(
            String::from("firecracker_test"),
            200,
            tachyon_contracts::HandlerHeaders::new(),
            body,
        );
        let response_bytes = serde_json::to_vec(&response).expect("response JSON");
        let length = u32::try_from(response_bytes.len()).expect("bounded response");
        let prefix = length.to_be_bytes();
        let prefix = prefix.iter().fold(String::new(), |mut output, byte| {
            write!(output, "\\{byte:03o}").expect("write to string");
            output
        });
        let driver = directory.path().join("firecracker-driver");
        let script = format!(
            "#!/bin/sh\n\
             case \"$*\" in\n\
               *\"--protocol handler-v1\"*\"--pool tenant_a\"*\"--vcpus 2\"*\"--memory-mib 512\"*\"--egress deny\"*\"--source server/routes/example/yon.py\"*\"--adapter python.v1\"*) ;;\n\
               *) echo 'invalid driver arguments' >&2; exit 9 ;;\n\
             esac\n\
             [ \"$YON_ISOLATION\" = firecracker ] || exit 10\n\
             dd bs=1 count={} of=/dev/null 2>/dev/null\n\
             printf '{prefix}'\n\
             printf '%s' '{}'\n",
            request_bytes.len(),
            String::from_utf8(response_bytes).expect("UTF-8 response")
        );
        fs::write(&driver, script).expect("driver");
        fs::set_permissions(&driver, fs::Permissions::from_mode(0o700)).expect("permissions");
        let path = driver.to_string_lossy();
        let isolation = parse(&[
            ("YON_ISOLATION", "firecracker"),
            ("YON_FIRECRACKER_DRIVER", &path),
            ("YON_FIRECRACKER_POOL", "tenant_a"),
            ("YON_FIRECRACKER_VCPUS", "2"),
            ("YON_FIRECRACKER_MEMORY_MIB", "512"),
        ])
        .expect("isolation policy");
        let supervisor = HandlerSupervisor::new(HandlerSupervisorOptions {
            isolation,
            default_timeout: Duration::from_secs(2),
            ..HandlerSupervisorOptions::default()
        })
        .expect("supervisor");
        let received = supervisor
            .invoke(&source, &request, &HandlerCancellation::default())
            .await
            .expect("driver response");
        assert_eq!(received.status, 200);
        assert_eq!(received.body.expect("body").data, "isolated");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn firecracker_refuses_prepared_sources_without_starting_the_driver() {
        use crate::{
            HandlerCancellation, HandlerSource, HandlerSupervisor, HandlerSupervisorOptions,
        };
        use std::os::unix::fs::PermissionsExt;
        use tachyon_contracts::{HandlerRequest, HttpMethod};

        let directory = tempfile::tempdir().expect("temporary directory");
        let marker = directory.path().join("driver-invoked");
        let driver = directory.path().join("firecracker-driver");
        fs::write(
            &driver,
            format!(
                "#!/bin/sh\nprintf invoked > '{}'\nexit 99\n",
                marker.display()
            ),
        )
        .expect("driver");
        fs::set_permissions(&driver, fs::Permissions::from_mode(0o700)).expect("permissions");
        let path = driver.to_string_lossy();
        let isolation = parse(&[
            ("YON_ISOLATION", "firecracker"),
            ("YON_FIRECRACKER_DRIVER", &path),
        ])
        .expect("isolation policy");
        let supervisor = HandlerSupervisor::new(HandlerSupervisorOptions {
            isolation,
            ..HandlerSupervisorOptions::default()
        })
        .expect("supervisor");

        let cases = [
            (
                "server/routes/typescript/yon.ts",
                "@Controller\nexport class TypescriptController {}\n",
            ),
            (
                "server/routes/php/yon.php",
                "<?php\n#[Controller]\nclass PhpController {}\n",
            ),
        ];
        for (relative, contents) in cases {
            let source_path = directory.path().join(relative);
            fs::create_dir_all(source_path.parent().expect("route parent")).expect("route");
            fs::write(&source_path, contents).expect("source");
            let source = HandlerSource::discover(directory.path(), relative).expect("discovery");
            let request = HandlerRequest::route(
                format!("firecracker_refusal_{}", source.language().name()),
                "/unsupported",
                HttpMethod::Get,
            );
            let failure = supervisor
                .invoke(&source, &request, &HandlerCancellation::default())
                .await
                .expect_err("prepared source must be refused");
            assert!(failure.to_string().contains("TY2010"), "{failure}");
            assert!(
                !marker.exists(),
                "Firecracker driver was invoked for {relative}"
            );
        }
    }
}
