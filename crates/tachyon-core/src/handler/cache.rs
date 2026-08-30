//! Capability-confined handler build cache.
//!
//! The project path is ambient only while the cache directory handle is
//! acquired. Every subsequent lookup and mutation is relative to that handle,
//! and every directory component is opened without following symlinks.

use cap_fs_ext::{DirExt as _, FollowSymlinks, MetadataExt as _, OpenOptionsFollowExt as _};
#[cfg(test)]
use cap_std::ambient_authority;
use cap_std::fs::{Dir, Metadata, OpenOptions};
use sha2::{Digest as _, Sha256};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};
#[cfg(unix)]
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

const MAX_CACHE_ENTRIES: usize = 256;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_LOCK_METADATA_BYTES: usize = 1024;
// Compiler processes are killed after one minute. Ten minutes leaves ample
// room for process-group cleanup and cache publication while still bounding a
// lock whose PID has been reused by an unrelated process.
const MAX_LOCK_LEASE: Duration = Duration::from_mins(10);

/// An already-open capability for `.tachyon/handlers`.
#[derive(Clone, Debug)]
pub(super) struct CacheDirectory {
    directory: Arc<Dir>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EntryIdentity {
    device: u64,
    inode: u64,
}

/// Removes a temporary cache entry only while its original identity remains.
#[derive(Debug)]
pub(super) struct OwnedCacheEntry {
    cache: CacheDirectory,
    relative: PathBuf,
    identity: EntryIdentity,
    published: bool,
}

/// A cache lock whose ownership is bound to both identity and random token.
#[derive(Debug)]
pub(super) struct CacheLock {
    cache: CacheDirectory,
    relative: PathBuf,
    token: String,
    identity: EntryIdentity,
}

#[derive(Debug)]
struct CacheEntry {
    relative: PathBuf,
    name: String,
    metadata: Metadata,
}

#[derive(Debug)]
struct PrunableEntry {
    relative: PathBuf,
    name: String,
    modified: cap_std::time::SystemTime,
    usage: (usize, u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LockSnapshot {
    identity: EntryIdentity,
    fingerprint: [u8; 32],
    owner: Option<LockOwner>,
    modified: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LockOwner {
    pid: u32,
    created: u64,
    token: String,
}

impl CacheDirectory {
    /// Opens the project-owned cache through non-following directory handles.
    #[cfg(all(test, unix))]
    pub(super) fn open(project_root: &Path) -> std::io::Result<Self> {
        let project = Dir::open_ambient_dir(project_root, ambient_authority())?;
        Self::open_project(&project)
    }

    /// Opens the cache below an already-retained project capability.
    pub(super) fn open_project(project: &Dir) -> std::io::Result<Self> {
        create_directory_if_missing(project, Path::new(".tachyon"))?;
        let tachyon = project.open_dir_nofollow(".tachyon")?;
        create_directory_if_missing(&tachyon, Path::new("handlers"))?;
        let handlers = tachyon.open_dir_nofollow("handlers")?;
        Ok(Self {
            directory: Arc::new(handlers),
        })
    }

    #[cfg(test)]
    pub(super) fn open_test_root(path: &Path) -> std::io::Result<Self> {
        Dir::open_ambient_dir(path, ambient_authority()).map(|directory| Self {
            directory: Arc::new(directory),
        })
    }

    pub(super) fn metadata(&self, relative: &Path) -> std::io::Result<Metadata> {
        let (parent, name) = self.parent_and_name(relative)?;
        parent.symlink_metadata(name)
    }

    pub(super) fn read(&self, relative: &Path) -> std::io::Result<Vec<u8>> {
        let mut file = self.open_regular_file(relative)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    pub(super) fn read_to_string(&self, relative: &Path) -> std::io::Result<String> {
        String::from_utf8(self.read(relative)?).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.utf8_error())
        })
    }

    pub(super) fn is_valid_file(&self, relative: &Path) -> bool {
        self.metadata(relative).is_ok_and(|metadata| {
            metadata.is_file() && !metadata.is_symlink() && metadata.len() > 0
        })
    }

    pub(super) fn create_owned_directory(
        &self,
        relative: &Path,
    ) -> std::io::Result<OwnedCacheEntry> {
        let (parent, name) = self.parent_and_name(relative)?;
        parent.create_dir(&name)?;
        let identity = entry_identity(&parent.symlink_metadata(&name)?);
        Ok(OwnedCacheEntry {
            cache: self.clone(),
            relative: relative.to_path_buf(),
            identity,
            published: false,
        })
    }

    #[cfg(test)]
    pub(super) fn adopt_file(&self, relative: &Path) -> std::io::Result<OwnedCacheEntry> {
        let identity = self.identity(relative)?;
        Ok(OwnedCacheEntry {
            cache: self.clone(),
            relative: relative.to_path_buf(),
            identity,
            published: false,
        })
    }

    #[cfg(test)]
    pub(super) fn adopt_directory(&self, relative: &Path) -> std::io::Result<OwnedCacheEntry> {
        let identity = self.identity(relative)?;
        Ok(OwnedCacheEntry {
            cache: self.clone(),
            relative: relative.to_path_buf(),
            identity,
            published: false,
        })
    }

    #[cfg(test)]
    pub(super) fn write_owned_for_test(
        &self,
        relative: &Path,
        bytes: &[u8],
    ) -> std::io::Result<OwnedCacheEntry> {
        self.write_owned_file(relative, bytes)
    }

    pub(super) fn create_directory(&self, relative: &Path) -> std::io::Result<()> {
        let (parent, name) = self.parent_and_name(relative)?;
        parent.create_dir(&name)
    }

    pub(super) fn stage_bytes(&self, final_path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        match self.metadata(final_path) {
            Ok(metadata) if metadata.is_symlink() || !metadata.is_file() => {
                return Err(invalid_data("cache child is a symlink or unsupported type"));
            }
            Ok(_) if self.read(final_path)? == bytes => return Ok(()),
            Ok(_) => {
                return Err(invalid_data(
                    "cached stage does not match its content identity",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let temporary = sibling_temporary(final_path, "stage")?;
        let guard = self.write_owned_file(&temporary, bytes)?;
        match self.hard_link(&temporary, final_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = self.metadata(final_path)?;
                if metadata.is_symlink()
                    || !metadata.is_file()
                    || self.read(final_path)?.as_slice() != bytes
                {
                    return Err(invalid_data(
                        "competing cache stage is not the expected file",
                    ));
                }
            }
            Err(error) => return Err(error),
        }
        drop(guard);
        Ok(())
    }

    /// Copies an owned compiler output into a content-addressed cache file.
    pub(super) fn publish_file(&self, source: &Path, final_path: &Path) -> std::io::Result<()> {
        let source_metadata = fs::symlink_metadata(source)?;
        if source_metadata.file_type().is_symlink()
            || !source_metadata.is_file()
            || source_metadata.len() == 0
        {
            return Err(invalid_data(
                "compiler output is not a non-empty regular file",
            ));
        }
        let temporary = sibling_temporary(final_path, "publish")?;
        let guard = self.copy_owned_file(source, &temporary)?;
        self.hard_link(&temporary, final_path)?;
        drop(guard);
        Ok(())
    }

    /// Copies a trusted cache file into an independently owned runtime path.
    pub(super) fn copy_file_out(&self, relative: &Path, destination: &Path) -> std::io::Result<()> {
        let mut source = self.open_regular_file(relative)?;
        let metadata = source.metadata()?;
        if metadata.len() == 0 {
            return Err(invalid_data("cached artifact is empty"));
        }
        let mut destination_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        std::io::copy(&mut source, &mut destination_file)?;
        destination_file.flush()?;
        destination_file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(
                destination,
                fs::Permissions::from_mode(cap_std::fs::MetadataExt::mode(&metadata)),
            )?;
        }
        Ok(())
    }

    pub(super) fn remove(&self, relative: &Path) -> std::io::Result<()> {
        let metadata = self.metadata(relative)?;
        if metadata.is_symlink() {
            return Err(invalid_data("cache traversal encountered a symlink"));
        }
        if metadata.is_file() {
            let (parent, name) = self.parent_and_name(relative)?;
            return parent.remove_file(name);
        }
        if !metadata.is_dir() {
            return Err(invalid_data(
                "cache traversal encountered an unsupported entry",
            ));
        }
        for entry in self.entries(relative)? {
            self.remove(&entry.relative)?;
        }
        let (parent, name) = self.parent_and_name(relative)?;
        parent.remove_dir(name)
    }

    pub(super) fn path_usage(&self, relative: &Path) -> std::io::Result<(usize, u64)> {
        let metadata = self.metadata(relative)?;
        if metadata.is_symlink() {
            return Err(invalid_data("cache accounting encountered a symlink"));
        }
        if metadata.is_file() {
            return Ok((1, metadata.len()));
        }
        if !metadata.is_dir() {
            return Err(invalid_data(
                "cache accounting encountered an unsupported entry",
            ));
        }
        let mut entries = 1_usize;
        let mut bytes = 0_u64;
        for child in self.entries(relative)? {
            let (child_entries, child_bytes) = self.path_usage(&child.relative)?;
            entries = entries
                .checked_add(child_entries)
                .ok_or_else(|| std::io::Error::other("handler cache entry accounting overflow"))?;
            bytes = bytes
                .checked_add(child_bytes)
                .ok_or_else(|| std::io::Error::other("handler cache byte accounting overflow"))?;
        }
        Ok((entries, bytes))
    }

    pub(super) fn prune(&self) -> std::io::Result<()> {
        self.prune_with_limits(MAX_CACHE_ENTRIES, MAX_CACHE_BYTES)
    }

    pub(super) fn acquire_lock(
        &self,
        relative: &Path,
        wait: Duration,
    ) -> std::io::Result<CacheLock> {
        self.acquire_lock_with(relative, wait, |file, pid, created, token| {
            writeln!(file, "{pid} {created} {token}")?;
            file.flush()?;
            file.sync_all()
        })
    }

    pub(super) fn acquire_lock_with<F>(
        &self,
        relative: &Path,
        wait: Duration,
        initialize: F,
    ) -> std::io::Result<CacheLock>
    where
        F: Fn(&mut cap_std::fs::File, u32, u64, &str) -> std::io::Result<()>,
    {
        self.acquire_lock_with_runtime(relative, wait, initialize, &unix_time, &process_is_live)
    }

    fn acquire_lock_with_runtime<F, N, L>(
        &self,
        relative: &Path,
        wait: Duration,
        initialize: F,
        now: &N,
        process_live: &L,
    ) -> std::io::Result<CacheLock>
    where
        F: Fn(&mut cap_std::fs::File, u32, u64, &str) -> std::io::Result<()>,
        N: Fn() -> Duration,
        L: Fn(u32) -> bool,
    {
        let deadline = Instant::now() + wait;
        loop {
            match self.create_new_file(relative) {
                Ok(mut file) => {
                    let identity = entry_identity(&file.metadata()?);
                    let created = now().as_secs();
                    let token = format!("{}-{}", std::process::id(), now().as_nanos());
                    let mut guard = OwnedCacheEntry {
                        cache: self.clone(),
                        relative: relative.to_path_buf(),
                        identity,
                        published: false,
                    };
                    initialize(&mut file, std::process::id(), created, &token)?;
                    guard.publish();
                    return Ok(CacheLock {
                        cache: self.clone(),
                        relative: relative.to_path_buf(),
                        token,
                        identity,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if self.recover_lock_with(relative, now, process_live)? {
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "Timed out waiting for handler cache lock",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
    }

    #[cfg(test)]
    pub(super) fn prune_with_limits(
        &self,
        max_entries: usize,
        max_bytes: u64,
    ) -> std::io::Result<()> {
        self.prune_with(
            max_entries,
            max_bytes,
            CacheDirectory::remove,
            CacheDirectory::path_usage,
        )
    }

    #[cfg(not(test))]
    fn prune_with_limits(&self, max_entries: usize, max_bytes: u64) -> std::io::Result<()> {
        self.prune_with(
            max_entries,
            max_bytes,
            CacheDirectory::remove,
            CacheDirectory::path_usage,
        )
    }

    #[cfg(test)]
    pub(super) fn prune_with<F, M>(
        &self,
        max_entries: usize,
        max_bytes: u64,
        remove: F,
        measure: M,
    ) -> std::io::Result<()>
    where
        F: Fn(&Self, &Path) -> std::io::Result<()>,
        M: Fn(&Self, &Path) -> std::io::Result<(usize, u64)>,
    {
        self.prune_entries(max_entries, max_bytes, remove, measure)
    }

    #[cfg(not(test))]
    fn prune_with<F, M>(
        &self,
        max_entries: usize,
        max_bytes: u64,
        remove: F,
        measure: M,
    ) -> std::io::Result<()>
    where
        F: Fn(&Self, &Path) -> std::io::Result<()>,
        M: Fn(&Self, &Path) -> std::io::Result<(usize, u64)>,
    {
        self.prune_entries(max_entries, max_bytes, remove, measure)
    }

    fn prune_entries<F, M>(
        &self,
        max_entries: usize,
        max_bytes: u64,
        remove: F,
        measure: M,
    ) -> std::io::Result<()>
    where
        F: Fn(&Self, &Path) -> std::io::Result<()>,
        M: Fn(&Self, &Path) -> std::io::Result<(usize, u64)>,
    {
        self.prune_entries_with_runtime(
            max_entries,
            max_bytes,
            remove,
            measure,
            &unix_time,
            &process_is_live,
        )
    }

    fn prune_entries_with_runtime<F, M, N, L>(
        &self,
        max_entries: usize,
        max_bytes: u64,
        remove: F,
        measure: M,
        now: &N,
        process_live: &L,
    ) -> std::io::Result<()>
    where
        F: Fn(&Self, &Path) -> std::io::Result<()>,
        M: Fn(&Self, &Path) -> std::io::Result<(usize, u64)>,
        N: Fn() -> Duration,
        L: Fn(u32) -> bool,
    {
        self.scavenge_locks_with(now, process_live)?;
        let mut entries = self.prunable_entries(&measure)?;
        entries
            .sort_by(|left, right| (left.modified, &left.name).cmp(&(right.modified, &right.name)));
        let mut total_bytes = checked_sum_bytes(entries.iter().map(|entry| entry.usage.1))?;
        let mut remaining = checked_sum_entries(entries.iter().map(|entry| entry.usage.0))?;
        for entry in entries {
            if remaining <= max_entries && total_bytes <= max_bytes {
                break;
            }
            remove(self, &entry.relative)?;
            total_bytes = total_bytes.saturating_sub(entry.usage.1);
            remaining = remaining.saturating_sub(entry.usage.0);
        }

        let remaining_entries = self.prunable_entries(&measure)?;
        let final_bytes = checked_sum_bytes(remaining_entries.iter().map(|entry| entry.usage.1))?;
        let final_count = checked_sum_entries(remaining_entries.iter().map(|entry| entry.usage.0))?;
        if final_count > max_entries || final_bytes > max_bytes {
            return Err(std::io::Error::other(
                "handler cache pruning did not achieve its entry and byte limits",
            ));
        }
        Ok(())
    }

    fn scavenge_locks_with<N, L>(&self, now: &N, process_live: &L) -> std::io::Result<()>
    where
        N: Fn() -> Duration,
        L: Fn(u32) -> bool,
    {
        let root = Path::new("");
        for entry in self.entries(root)? {
            if entry.name != ".prune.lock" && entry.name.strip_suffix(".lock").is_some() {
                self.recover_lock_with(&entry.relative, now, process_live)?;
            }
        }
        Ok(())
    }

    fn prunable_entries<M>(&self, measure: &M) -> std::io::Result<Vec<PrunableEntry>>
    where
        M: Fn(&Self, &Path) -> std::io::Result<(usize, u64)>,
    {
        let root = Path::new("");
        let discovered = self.entries(root)?;
        let mut active = Vec::new();
        for entry in &discovered {
            if let Some(digest) = entry.name.strip_suffix(".lock")
                && digest != ".prune"
            {
                active.push(String::from(digest));
            }
        }
        let mut entries = Vec::new();
        for entry in discovered {
            if Path::new(&entry.name)
                .extension()
                .is_some_and(|extension| extension == "lock")
                || active
                    .iter()
                    .any(|digest| entry.name.trim_start_matches('.').starts_with(digest))
            {
                continue;
            }
            if entry.metadata.is_symlink() {
                return Err(invalid_data("cache pruning encountered a symlink"));
            }
            let modified = entry.metadata.modified()?;
            let usage = measure(self, &entry.relative)?;
            entries.push(PrunableEntry {
                relative: entry.relative,
                name: entry.name,
                modified,
                usage,
            });
        }
        Ok(entries)
    }

    fn entries(&self, relative: &Path) -> std::io::Result<Vec<CacheEntry>> {
        let directory = if relative.as_os_str().is_empty() {
            self.directory.try_clone()?
        } else {
            self.open_directory(relative)?
        };
        let mut entries = Vec::new();
        for entry in directory.entries()? {
            let entry = entry?;
            let name = entry.file_name();
            let name_text = name
                .to_str()
                .ok_or_else(|| invalid_data("cache entry name is not UTF-8"))?
                .to_string();
            let child = if relative.as_os_str().is_empty() {
                PathBuf::from(&name)
            } else {
                relative.join(&name)
            };
            let metadata = self.metadata(&child)?;
            entries.push(CacheEntry {
                relative: child,
                name: name_text,
                metadata,
            });
        }
        Ok(entries)
    }

    fn open_regular_file(&self, relative: &Path) -> std::io::Result<cap_std::fs::File> {
        let (parent, name) = self.parent_and_name(relative)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = parent.open_with(name, &options)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(invalid_data("cache child is not a regular file"));
        }
        Ok(file)
    }

    fn create_new_file(&self, relative: &Path) -> std::io::Result<cap_std::fs::File> {
        let (parent, name) = self.parent_and_name(relative)?;
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        parent.open_with(name, &options)
    }

    fn write_owned_file(&self, relative: &Path, bytes: &[u8]) -> std::io::Result<OwnedCacheEntry> {
        let mut file = self.create_new_file(relative)?;
        let identity = entry_identity(&file.metadata()?);
        let guard = OwnedCacheEntry {
            cache: self.clone(),
            relative: relative.to_path_buf(),
            identity,
            published: false,
        };
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        Ok(guard)
    }

    fn copy_owned_file(&self, source: &Path, relative: &Path) -> std::io::Result<OwnedCacheEntry> {
        let mut source_file = fs::File::open(source)?;
        let mut destination = self.create_new_file(relative)?;
        let identity = entry_identity(&destination.metadata()?);
        let guard = OwnedCacheEntry {
            cache: self.clone(),
            relative: relative.to_path_buf(),
            identity,
            published: false,
        };
        std::io::copy(&mut source_file, &mut destination)?;
        destination.flush()?;
        destination.sync_all()?;
        #[cfg(unix)]
        {
            use cap_std::fs::PermissionsExt as _;
            use std::os::unix::fs::PermissionsExt as _;
            destination.set_permissions(cap_std::fs::Permissions::from_mode(
                fs::metadata(source)?.permissions().mode(),
            ))?;
        }
        Ok(guard)
    }

    fn hard_link(&self, source: &Path, destination: &Path) -> std::io::Result<()> {
        let (source_parent, source_name) = self.parent_and_name(source)?;
        let (destination_parent, destination_name) = self.parent_and_name(destination)?;
        source_parent.hard_link(source_name, &destination_parent, destination_name)
    }

    fn rename(&self, source: &Path, destination: &Path) -> std::io::Result<()> {
        let (source_parent, source_name) = self.parent_and_name(source)?;
        let (destination_parent, destination_name) = self.parent_and_name(destination)?;
        source_parent.rename(source_name, &destination_parent, destination_name)
    }

    fn inspect_lock(&self, relative: &Path) -> std::io::Result<LockSnapshot> {
        let mut file = self.open_regular_file(relative)?;
        let metadata = file.metadata()?;
        let identity = entry_identity(&metadata);
        let modified = metadata
            .modified()?
            .into_std()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let mut parser_bytes = Vec::with_capacity(MAX_LOCK_METADATA_BYTES + 1);
        std::io::Read::by_ref(&mut file)
            .take((MAX_LOCK_METADATA_BYTES + 1) as u64)
            .read_to_end(&mut parser_bytes)?;
        let overflowed = parser_bytes.len() > MAX_LOCK_METADATA_BYTES;
        if overflowed {
            parser_bytes.truncate(MAX_LOCK_METADATA_BYTES);
        }
        let mut hasher = Sha256::new();
        hasher.update(identity.device.to_le_bytes());
        hasher.update(identity.inode.to_le_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(modified.as_secs().to_le_bytes());
        hasher.update(modified.subsec_nanos().to_le_bytes());
        hasher.update([u8::from(overflowed)]);
        hasher.update(&parser_bytes);
        let fingerprint = hasher.finalize().into();
        let contents = if overflowed {
            ""
        } else {
            std::str::from_utf8(&parser_bytes).unwrap_or_default()
        };
        let mut fields = contents.split_whitespace();
        let owner = match (fields.next(), fields.next(), fields.next(), fields.next()) {
            (Some(pid), Some(created), Some(token), None) => pid
                .parse::<u32>()
                .ok()
                .zip(created.parse::<u64>().ok())
                .map(|(pid, created)| LockOwner {
                    pid,
                    created,
                    token: token.to_string(),
                }),
            _ => None,
        };
        Ok(LockSnapshot {
            identity,
            fingerprint,
            owner,
            modified,
        })
    }

    fn lock_is_reclaimable<L>(snapshot: &LockSnapshot, now: Duration, process_live: &L) -> bool
    where
        L: Fn(u32) -> bool,
    {
        if snapshot.modified > now {
            return true;
        }
        snapshot.owner.as_ref().map_or_else(
            || now.saturating_sub(snapshot.modified) >= MAX_LOCK_LEASE,
            |owner| {
                if !process_live(owner.pid) {
                    return true;
                }
                let created = Duration::from_secs(owner.created);
                let lease_origin = if created <= now {
                    created
                } else {
                    snapshot.modified
                };
                now.saturating_sub(lease_origin) >= MAX_LOCK_LEASE
            },
        )
    }

    fn recover_lock_with<N, L>(
        &self,
        relative: &Path,
        now: &N,
        process_live: &L,
    ) -> std::io::Result<bool>
    where
        N: Fn() -> Duration,
        L: Fn(u32) -> bool,
    {
        let before = match self.inspect_lock(relative) {
            Ok(snapshot) => snapshot,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error),
        };
        if !Self::lock_is_reclaimable(&before, now(), process_live) {
            return Ok(false);
        }

        let quarantine = sibling_temporary(relative, "recover")?;
        match self.rename(relative, &quarantine) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error),
        }
        let after = self.inspect_lock(&quarantine)?;
        if after.identity == before.identity
            && after.fingerprint == before.fingerprint
            && Self::lock_is_reclaimable(&after, now(), process_live)
        {
            let current = self.inspect_lock(&quarantine)?;
            if current.identity == after.identity && current.fingerprint == after.fingerprint {
                self.remove(&quarantine)?;
                return Ok(true);
            }
        }

        // Never rename over a replacement lock. A hard link restores the
        // quarantined inode only when the original name is still vacant.
        if self.hard_link(&quarantine, relative).is_ok()
            && self
                .inspect_lock(&quarantine)
                .is_ok_and(|current| current.identity == after.identity)
        {
            self.remove(&quarantine)?;
        }
        Ok(false)
    }

    fn identity(&self, relative: &Path) -> std::io::Result<EntryIdentity> {
        self.metadata(relative)
            .map(|metadata| entry_identity(&metadata))
    }

    fn open_directory(&self, relative: &Path) -> std::io::Result<Dir> {
        let mut directory = self.directory.try_clone()?;
        for component in relative_components(relative)? {
            directory = directory.open_dir_nofollow(component)?;
        }
        Ok(directory)
    }

    fn parent_and_name(&self, relative: &Path) -> std::io::Result<(Dir, OsString)> {
        let mut components = relative_components(relative)?;
        let name = components
            .pop()
            .ok_or_else(|| invalid_data("cache operation requires a child path"))?;
        let mut directory = self.directory.try_clone()?;
        for component in components {
            directory = directory.open_dir_nofollow(component)?;
        }
        Ok((directory, name))
    }
}

impl OwnedCacheEntry {
    pub(super) fn publish(&mut self) {
        self.published = true;
    }
}

impl Drop for OwnedCacheEntry {
    fn drop(&mut self) {
        if self.published || self.cache.identity(&self.relative).ok() != Some(self.identity) {
            return;
        }
        let _removed = self.cache.remove(&self.relative);
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        if self
            .cache
            .inspect_lock(&self.relative)
            .is_ok_and(|snapshot| {
                snapshot.identity == self.identity
                    && snapshot
                        .owner
                        .is_some_and(|owner| owner.token == self.token)
            })
        {
            let _removed = self.cache.remove(&self.relative);
        }
    }
}

fn create_directory_if_missing(parent: &Dir, child: &Path) -> std::io::Result<()> {
    match parent.create_dir(child) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

fn relative_components(path: &Path) -> std::io::Result<Vec<OsString>> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value.to_os_string()),
            _ => Err(invalid_data("cache paths must be relative and regular")),
        })
        .collect()
}

fn sibling_temporary(path: &Path, purpose: &str) -> std::io::Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| invalid_data("cache child name is not UTF-8"))?;
    let temporary = format!(
        ".{name}.{purpose}.{}.{}.tmp",
        std::process::id(),
        unix_time().as_nanos()
    );
    Ok(path.parent().map_or_else(
        || PathBuf::from(&temporary),
        |parent| parent.join(&temporary),
    ))
}

fn entry_identity(metadata: &Metadata) -> EntryIdentity {
    EntryIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

fn checked_sum_bytes(values: impl Iterator<Item = u64>) -> std::io::Result<u64> {
    let mut total = 0_u64;
    for value in values {
        total = total
            .checked_add(value)
            .ok_or_else(|| std::io::Error::other("handler cache byte accounting overflow"))?;
    }
    Ok(total)
}

fn checked_sum_entries(values: impl Iterator<Item = usize>) -> std::io::Result<usize> {
    let mut total = 0_usize;
    for value in values {
        total = total
            .checked_add(value)
            .ok_or_else(|| std::io::Error::other("handler cache entry accounting overflow"))?;
    }
    Ok(total)
}

fn invalid_data(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

fn unix_time() -> Duration {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
}

#[cfg(unix)]
fn process_is_live(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn process_is_live(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use std::cell::Cell;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    fn seed(cache: &CacheDirectory, relative: &str, bytes: &[u8]) {
        let mut entry = cache
            .write_owned_file(Path::new(relative), bytes)
            .expect("seed cache entry");
        entry.publish();
    }

    fn set_modified(root: &Path, relative: &str, modified: Duration) {
        fs::OpenOptions::new()
            .write(true)
            .open(root.join(relative))
            .expect("open cache entry for timestamp")
            .set_modified(SystemTime::UNIX_EPOCH + modified)
            .expect("set cache entry timestamp");
    }

    fn assert_future_lock_digest_is_reclaimed(
        cache: &CacheDirectory,
        now: Duration,
        process_live: impl Fn(u32) -> bool,
    ) {
        seed(cache, ".future.publish.tmp", b"temporary");
        seed(cache, "future.bin", b"artifact");
        cache
            .prune_entries_with_runtime(
                0,
                0,
                CacheDirectory::remove,
                CacheDirectory::path_usage,
                &|| now,
                &process_live,
            )
            .expect("future lock and associated state are reclaimable");
        assert!(cache.metadata(Path::new("future.lock")).is_err());
        assert!(cache.metadata(Path::new(".future.publish.tmp")).is_err());
        assert!(cache.metadata(Path::new("future.bin")).is_err());
        assert!(
            cache
                .prunable_entries(&CacheDirectory::path_usage)
                .expect("postcondition entries")
                .is_empty()
        );
    }

    #[cfg(unix)]
    #[test]
    fn opened_cache_capability_survives_ambient_root_swap() {
        let project = tempfile::tempdir().expect("project");
        let cache = CacheDirectory::open(project.path()).expect("open cache capability");
        let ambient = project.path().join(".tachyon/handlers");
        fs::write(ambient.join("stale"), b"obsolete").expect("seed prune candidate");

        let owned = project.path().join(".tachyon/handlers-owned");
        fs::rename(&ambient, &owned).expect("move opened cache directory");
        let outside = tempfile::tempdir().expect("outside directory");
        let sentinel = outside.path().join("sentinel");
        fs::write(&sentinel, b"outside").expect("outside sentinel");
        symlink(outside.path(), &ambient).expect("replace ambient cache with symlink");

        let lock = cache
            .acquire_lock(Path::new("capability.lock"), Duration::from_secs(1))
            .expect("lock through opened directory");
        assert!(owned.join("capability.lock").is_file());
        assert!(!outside.path().join("capability.lock").exists());
        drop(lock);

        cache
            .prune_with_limits(0, 0)
            .expect("prune through opened directory");
        assert!(!owned.join("stale").exists());

        cache
            .stage_bytes(Path::new("stage.bin"), b"staged")
            .expect("stage through opened directory");
        let compiler = tempfile::tempdir().expect("compiler workspace");
        let output = compiler.path().join("artifact");
        fs::write(&output, b"compiled").expect("compiler output");
        cache
            .publish_file(&output, Path::new("artifact.bin"))
            .expect("publish through opened directory");

        assert!(cache.is_valid_file(Path::new("stage.bin")));
        assert_eq!(
            cache
                .read(Path::new("artifact.bin"))
                .expect("discover artifact"),
            b"compiled"
        );
        assert_eq!(fs::read(&sentinel).expect("read sentinel"), b"outside");
        assert_eq!(
            fs::read_dir(outside.path())
                .expect("outside listing")
                .count(),
            1
        );
        assert_eq!(
            fs::read(owned.join("stage.bin")).expect("owned stage"),
            b"staged"
        );
        assert_eq!(
            fs::read(owned.join("artifact.bin")).expect("owned artifact"),
            b"compiled"
        );
    }

    #[test]
    fn pruning_scavenges_a_fresh_dead_lock_for_an_abandoned_digest() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "old-digest.lock", b"41 999 dead-token\n");
        seed(&cache, "old-digest.bin", b"abandoned artifact");

        cache
            .prune_entries_with_runtime(
                0,
                0,
                CacheDirectory::remove,
                CacheDirectory::path_usage,
                &|| Duration::from_secs(1_000),
                &|pid| {
                    assert_eq!(pid, 41);
                    false
                },
            )
            .expect("scavenge crashed build after source digest changed");

        assert!(cache.metadata(Path::new("old-digest.lock")).is_err());
        assert!(cache.metadata(Path::new("old-digest.bin")).is_err());
    }

    #[test]
    fn fresh_dead_lock_is_recovered_before_the_acquisition_deadline() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "fresh-dead.lock", b"47 999 dead-token\n");
        let now = || Duration::from_secs(1_000);

        let recovered = cache
            .acquire_lock_with_runtime(
                Path::new("fresh-dead.lock"),
                Duration::from_secs(1),
                |file, pid, created, token| writeln!(file, "{pid} {created} {token}"),
                &now,
                &|pid| {
                    assert_eq!(pid, 47);
                    false
                },
            )
            .expect("fresh dead process recovers immediately");
        assert_ne!(
            cache
                .read(Path::new("fresh-dead.lock"))
                .expect("replacement lock"),
            b"47 999 dead-token\n"
        );
        drop(recovered);
    }

    #[test]
    fn live_lock_is_not_stolen_within_a_legitimate_transaction() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "live.lock", b"42 1 live-token\n");
        set_modified(root.path(), "live.lock", Duration::from_secs(1));
        let now = || Duration::from_mins(1);
        let live = |pid| pid == 42;

        let failure = cache
            .acquire_lock_with_runtime(
                Path::new("live.lock"),
                Duration::ZERO,
                |_, _, _, _| Ok(()),
                &now,
                &live,
            )
            .expect_err("live transaction must retain its lock");
        assert_eq!(failure.kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(
            cache.read(Path::new("live.lock")).expect("live lock"),
            b"42 1 live-token\n"
        );
        assert!(MAX_LOCK_LEASE > Duration::from_mins(1));
    }

    #[test]
    fn hard_lease_bounds_a_lock_whose_pid_has_been_reused() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "reused.lock", b"43 100 reused-token\n");
        let now = || Duration::from_secs(100) + MAX_LOCK_LEASE;

        let recovered = cache
            .acquire_lock_with_runtime(
                Path::new("reused.lock"),
                Duration::from_millis(20),
                |file, pid, created, token| writeln!(file, "{pid} {created} {token}"),
                &now,
                &|pid| {
                    assert_eq!(pid, 43);
                    true
                },
            )
            .expect("hard lease recovers PID-reuse lock");
        assert_ne!(
            cache.read(Path::new("reused.lock")).expect("new lock"),
            b"43 100 reused-token\n"
        );
        drop(recovered);
    }

    #[test]
    fn future_owner_timestamp_falls_back_to_the_metadata_lease() {
        let snapshot = LockSnapshot {
            identity: EntryIdentity {
                device: 1,
                inode: 2,
            },
            fingerprint: [0; 32],
            owner: Some(LockOwner {
                pid: 43,
                created: 10_000,
                token: "future-token".to_string(),
            }),
            modified: Duration::from_secs(100),
        };
        let live = |pid| {
            assert_eq!(pid, 43);
            true
        };

        assert!(!CacheDirectory::lock_is_reclaimable(
            &snapshot,
            (Duration::from_secs(100) + MAX_LOCK_LEASE)
                .checked_sub(Duration::from_nanos(1))
                .expect("positive lease"),
            &live,
        ));
        assert!(CacheDirectory::lock_is_reclaimable(
            &snapshot,
            Duration::from_secs(100) + MAX_LOCK_LEASE,
            &live,
        ));
    }

    #[test]
    fn malformed_lock_with_future_mtime_cannot_escape_quota() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "future.lock", b"malformed");
        set_modified(root.path(), "future.lock", Duration::from_secs(1_000));

        assert_future_lock_digest_is_reclaimed(&cache, Duration::from_secs(100), |_| true);
    }

    #[test]
    fn live_future_owner_and_future_mtime_cannot_escape_quota() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "future.lock", b"43 10000 future-token\n");
        set_modified(root.path(), "future.lock", Duration::from_secs(1_000));

        assert_future_lock_digest_is_reclaimed(&cache, Duration::from_secs(100), |pid| {
            assert_eq!(pid, 43);
            true
        });
    }

    #[test]
    fn oversized_sparse_lock_is_inspected_and_recovered_with_bounded_io() {
        let root = tempfile::tempdir().expect("cache");
        let path = root.path().join("oversized.lock");
        let file = fs::File::create(&path).expect("create sparse lock");
        file.set_len(1_u64 << 40).expect("make sparse lock");
        drop(file);
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        let now = || Duration::from_secs(10_000_000_000);

        let snapshot = cache
            .inspect_lock(Path::new("oversized.lock"))
            .expect("bounded inspection");
        assert!(snapshot.owner.is_none(), "oversized metadata is malformed");
        let recovered = cache
            .acquire_lock_with_runtime(
                Path::new("oversized.lock"),
                Duration::from_millis(20),
                |file, pid, created, token| writeln!(file, "{pid} {created} {token}"),
                &now,
                &|_| true,
            )
            .expect("expired oversized lock is recovered");
        assert!(
            cache
                .metadata(Path::new("oversized.lock"))
                .expect("replacement metadata")
                .len()
                < MAX_LOCK_METADATA_BYTES as u64
        );
        drop(recovered);
    }

    #[test]
    fn oversized_sparse_lock_with_future_mtime_cannot_escape_quota() {
        let root = tempfile::tempdir().expect("cache");
        let path = root.path().join("future.lock");
        let file = fs::File::create(&path).expect("create sparse lock");
        file.set_len(1_u64 << 40).expect("make sparse lock");
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000))
            .expect("set hostile future mtime");
        drop(file);
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");

        assert_future_lock_digest_is_reclaimed(&cache, Duration::from_secs(100), |_| true);
    }

    #[test]
    fn abandoned_temporaries_are_accounted_and_quota_is_a_postcondition() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "dead.lock", b"44 499 dead-token\n");
        seed(&cache, ".dead.publish.tmp", b"temporary");
        seed(&cache, "dead.bin", b"artifact");
        seed(&cache, "survivor", b"ok");
        let now = || Duration::from_secs(500);
        let dead = |_| false;

        cache
            .prune_entries_with_runtime(
                1,
                2,
                CacheDirectory::remove,
                CacheDirectory::path_usage,
                &now,
                &dead,
            )
            .expect("enforce quota after scavenging");
        assert!(cache.metadata(Path::new("dead.lock")).is_err());
        assert!(cache.metadata(Path::new(".dead.publish.tmp")).is_err());
        assert!(cache.metadata(Path::new("dead.bin")).is_err());
        let remaining = cache
            .prunable_entries(&CacheDirectory::path_usage)
            .expect("remaining entries");
        assert!(
            checked_sum_entries(remaining.iter().map(|entry| entry.usage.0))
                .expect("entry accounting")
                <= 1
        );
        assert!(
            checked_sum_bytes(remaining.iter().map(|entry| entry.usage.1))
                .expect("byte accounting")
                <= 2
        );
    }

    #[test]
    fn pruning_rejects_a_false_success_that_does_not_achieve_quota() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "entry", b"too large");
        let failure = cache
            .prune_entries_with_runtime(
                0,
                0,
                |_, _| Ok(()),
                CacheDirectory::path_usage,
                &|| Duration::from_secs(1),
                &|_| false,
            )
            .expect_err("reported removal did not achieve quota");
        assert!(failure.to_string().contains("did not achieve"));
    }

    #[test]
    fn quarantine_recovery_never_deletes_a_replacement_lock() {
        let root = tempfile::tempdir().expect("cache");
        let cache = CacheDirectory::open_test_root(root.path()).expect("cache capability");
        seed(&cache, "replace.lock", b"45 1 old-token\n");
        set_modified(root.path(), "replace.lock", Duration::from_secs(1));
        let observations = Cell::new(0_u8);
        let live = |pid| {
            assert_eq!(pid, 45);
            observations.set(observations.get() + 1);
            if observations.get() == 2 {
                let quarantine = fs::read_dir(root.path())
                    .expect("cache listing")
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.file_name()
                            .and_then(OsStr::to_str)
                            .is_some_and(|name| name.contains(".recover."))
                    })
                    .expect("quarantined lock");
                fs::remove_file(&quarantine).expect("replace quarantine");
                fs::write(&quarantine, b"46 999 replacement-token\n").expect("replacement lock");
            }
            false
        };

        assert!(
            !cache
                .recover_lock_with(Path::new("replace.lock"), &|| Duration::from_secs(2), &live,)
                .expect("safe recovery")
        );
        assert_eq!(
            cache
                .read(Path::new("replace.lock"))
                .expect("replacement survives"),
            b"46 999 replacement-token\n"
        );
    }
}
