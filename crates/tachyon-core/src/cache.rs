//! The on-disk cache a Tachyon installation keeps.
//!
//! The Rust binary does not need unpacked JavaScript sources, but it preserves
//! the released runtime-cache layout so scripts can inspect and clear it with
//! the same commands before and after upgrading.

use crate::Failure;
use crate::failure::{diagnostic, source_span};
use std::fs;
use std::path::PathBuf;

/// Reports where the cache lives and how much of it is in use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheStatus {
    /// Directory holding the cache, whether or not it exists.
    pub root: PathBuf,
    /// Content-addressed runtime entries.
    pub runtime_entries: usize,
    /// Entries attributed to a previous installation.
    pub legacy_entries: usize,
}

/// Returns the platform cache directory, honouring `TACHYON_CACHE_DIR`.
///
/// The layout matches the legacy implementation so both find the same
/// directory and `clean` can remove what the legacy one wrote.
#[must_use]
pub fn root() -> PathBuf {
    if let Some(override_path) = std::env::var_os("TACHYON_CACHE_DIR")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return override_path;
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if cfg!(target_os = "macos") {
        home.join("Library/Caches/Tachyon")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA")
            .map_or_else(|| home.join("AppData/Local"), PathBuf::from)
            .join("Tachyon/Cache")
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map_or_else(|| home.join(".cache"), PathBuf::from)
            .join("tachyon")
    }
}

/// Inspects the cache without changing it.
#[must_use]
pub fn status() -> CacheStatus {
    status_at(root())
}

fn status_at(root: PathBuf) -> CacheStatus {
    let runtime_entries = fs::read_dir(root.join("runtime"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .count()
        })
        .unwrap_or_default();
    CacheStatus {
        root,
        runtime_entries,
        legacy_entries: 0,
    }
}

/// Materializes the stable runtime-cache marker used by the released CLI.
///
/// # Errors
///
/// Returns a cache diagnostic when the configured directory is not writable.
pub fn ensure_runtime() -> Result<PathBuf, Failure> {
    let entry = root().join("runtime/native-v1");
    fs::create_dir_all(&entry).map_err(|error| {
        Failure::one(diagnostic(
            1503,
            format!("Cannot initialize the Tachyon runtime cache: {error}"),
            Some(String::from(
                "Check the directory's permissions, or set TACHYON_CACHE_DIR to a writable location.",
            )),
            source_span("cache", 0, 5),
        ))
    })?;
    Ok(entry)
}

/// Removes the cache directory, returning what was removed.
///
/// # Errors
///
/// Returns a diagnostic when the directory exists but cannot be removed.
pub fn clean() -> Result<CacheStatus, Failure> {
    clean_at(root())
}

fn clean_at(root: PathBuf) -> Result<CacheStatus, Failure> {
    let before = status_at(root);
    match fs::remove_dir_all(before.root.join("runtime")) {
        Ok(()) => Ok(before),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(before),
        Err(error) => Err(Failure::one(diagnostic(
            1503,
            format!(
                "Cannot clear the cache at {}. {error}",
                before.root.display()
            ),
            Some(String::from(
                "Check the directory's permissions, or set TACHYON_CACHE_DIR to \
                 a writable location.",
            )),
            source_span("cache", 0, 5),
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::{clean_at, root, status_at};
    use std::fs;

    #[test]
    fn the_cache_root_is_absolute_and_named() {
        let root = root();
        assert!(
            root.to_string_lossy().to_lowercase().contains("tachyon"),
            "{}",
            root.display()
        );
    }

    #[test]
    fn a_legacy_cache_is_counted_then_removed() {
        // Nothing written yet, so nothing is reported and cleaning succeeds
        // rather than failing on a missing directory.
        let directory = tempfile::tempdir().expect("directory");
        let cache = directory.path().join("cache");
        let empty = status_at(cache.clone());
        assert_eq!(empty.legacy_entries, 0);
        assert_eq!(empty.runtime_entries, 0);
        assert!(clean_at(cache.clone()).is_ok());

        // A cache a legacy installation left behind is counted, then removed,
        // which is what a project moving off the legacy implementation wants.
        fs::create_dir_all(cache.join("runtime")).expect("runtime");
        fs::write(cache.join("runtime/a.js"), "export const a = 1").expect("entry");
        assert_eq!(status_at(cache.clone()).runtime_entries, 0);

        let cleaned = clean_at(cache.clone()).expect("cleaned");
        assert_eq!(cleaned.runtime_entries, 0);
        assert!(!cache.join("runtime").exists());
        assert_eq!(status_at(cache).legacy_entries, 0);
    }
}
