use crate::Failure;
use crate::failure::{diagnostic, source_span};
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::Path;

const CONFIG_PATH: &str = "tachyon.json";
const MAX_CONFIG_BYTES: u64 = 64 * 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeApplication {
    pub(super) name: String,
    pub(super) executable_name: String,
    pub(super) application_id: String,
    pub(super) version: String,
    pub(super) entry_route: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeConfiguration {
    application: ApplicationConfiguration,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationConfiguration {
    name: String,
    id: String,
    version: String,
    entry_route: String,
}

impl NativeApplication {
    pub(super) fn discover(project_root: &Path) -> Result<Self, Failure> {
        let path = project_root.join(CONFIG_PATH);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(config_failure(
                        "tachyon.json must be a regular, non-symlinked file.",
                    ));
                }
                if metadata.len() > MAX_CONFIG_BYTES {
                    return Err(config_failure("tachyon.json exceeds the 64 KiB limit."));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::defaults(project_root));
            }
            Err(error) => {
                return Err(config_failure(&format!(
                    "Cannot inspect tachyon.json: {error}"
                )));
            }
        }
        let bytes = fs::read(&path)
            .map_err(|error| config_failure(&format!("Cannot read tachyon.json: {error}")))?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| config_failure("tachyon.json must be valid UTF-8."))?;
        let configuration: NativeConfiguration = serde_json::from_str(source)
            .map_err(|error| config_failure(&format!("tachyon.json is invalid: {error}")))?;
        Self::validate(configuration.application)
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

fn config_failure(message: &str) -> Failure {
    Failure::one(diagnostic(
        1601,
        message,
        Some(String::from(
            "Use the strict Phase 4 tachyon.json application contract.",
        )),
        source_span(CONFIG_PATH, 0, CONFIG_PATH.len()),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::NativeApplication;
    use std::fs;

    #[test]
    fn defaults_and_strict_configuration_are_portable() {
        let root = tempfile::tempdir().expect("root");
        let defaults = NativeApplication::discover(root.path()).expect("defaults");
        assert!(defaults.application_id.starts_with("ma.del.tachyon."));
        assert_eq!(defaults.entry_route, "/");

        fs::write(
            root.path().join("tachyon.json"),
            r#"{"application":{"name":"Catalog App","id":"dev.example.catalog","version":"1.2.3","entry_route":"/products"}}"#,
        )
        .expect("configuration");
        let configured = NativeApplication::discover(root.path()).expect("configured");
        assert_eq!(configured.executable_name, "CatalogApp");
        assert_eq!(configured.application_id, "dev.example.catalog");
    }

    #[test]
    fn malformed_configuration_fails_closed() {
        for source in [
            "{}",
            r#"{"application":{"name":"","id":"dev.ok","version":"1","entry_route":"/"}}"#,
            r#"{"application":{"name":"App","id":"INVALID","version":"1","entry_route":"/"}}"#,
            r#"{"application":{"name":"App","id":"dev.ok","version":"bad version","entry_route":"/"}}"#,
            r#"{"application":{"name":"App","id":"dev.ok","version":"1","entry_route":"../bad"}}"#,
            r#"{"application":{"name":"App","id":"dev.ok","version":"1","entry_route":"/","extra":true}}"#,
        ] {
            let root = tempfile::tempdir().expect("root");
            fs::write(root.path().join("tachyon.json"), source).expect("configuration");
            assert!(
                NativeApplication::discover(root.path()).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn oversized_configuration_fails_before_parsing() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("tachyon.json"), vec![b' '; 64 * 1_024 + 1])
            .expect("oversized configuration");
        let error = NativeApplication::discover(root.path()).expect_err("size limit");
        assert!(error.to_string().contains("TY1601"));
        assert!(error.to_string().contains("64 KiB"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_configuration_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let target = root.path().join("application.json");
        fs::write(
            &target,
            r#"{"application":{"name":"App","id":"dev.example.app","version":"1","entry_route":"/"}}"#,
        )
        .expect("target");
        symlink(&target, root.path().join("tachyon.json")).expect("symlink");
        let error = NativeApplication::discover(root.path()).expect_err("symlink rejection");
        assert!(error.to_string().contains("TY1601"));
        assert!(error.to_string().contains("non-symlinked"));
    }
}
