//! An opt-in cache for the *result* of discovery.
//!
//! Discovery is not the expensive part of a lint run — parsing is — so this is
//! not a general speed-up and is deliberately not on by default. It exists for
//! the case where the walk genuinely dominates: a tree with tens of thousands
//! of entries, scanned repeatedly by an editor or an agent that runs several
//! commands in a row over the same roots.
//!
//! The design constraint is that a stale hit is far worse than a miss. A tool
//! whose entire value is not silently doing the wrong thing must not report a
//! clean result over a file set that no longer exists. So:
//!
//! * the key covers every option that can change the answer, and a format
//!   version that is bumped whenever the entry shape changes;
//! * validation re-stats every directory the walk listed, comparing both its
//!   mtime and its entry count — a coarse filesystem timestamp can hide a
//!   same-second rename, and the count catches it;
//! * every ignore file that contributed patterns is stamped by mtime and
//!   length, because editing a `.gitignore` changes the result without
//!   changing any directory's mtime;
//! * anything unexpected — a missing directory, an unparsable entry, a version
//!   mismatch, an I/O error — is a miss, never an error.
//!
//! Writes go through a temporary file and a rename, so two concurrent runs
//! cannot leave a half-written entry behind.
//!
//! One consequence of validating by directory identity is worth knowing: a
//! cache directory placed *inside* a scanned root invalidates its own first
//! entry, because creating it changes the root's mtime and entry count after
//! the walk recorded them. It converges on the next run — the directory then
//! already exists and only the entry file inside it changes — but a cache kept
//! outside the roots avoids the wasted run entirely.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use serde_json::{Value, json};

use super::error::WorkspaceError;
use super::types::{WorkspaceDiscovery, WorkspaceDiscoveryOptions};

/// Bumped whenever a cached entry's shape or the meaning of a field changes.
///
/// An older paredit must not read a newer entry and vice versa; the version is
/// part of both the file contents and the key, so a mismatch is a plain miss.
const CACHE_FORMAT_VERSION: u32 = 1;

/// The largest cache entry that will be read back.
const MAX_CACHE_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

/// A directory holding cached discovery results.
#[derive(Clone, Debug)]
pub struct DiscoveryCache {
    directory: PathBuf,
}

/// Whether a lookup hit, and why it missed if it did not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheOutcome {
    /// The entry was reused.
    Hit,
    /// No entry existed for this key.
    Missing,
    /// An entry existed but the tree had changed under it.
    Stale,
    /// An entry existed but could not be read or understood.
    Unusable,
}

impl CacheOutcome {
    /// A stable identifier for JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Missing => "missing",
            Self::Stale => "stale",
            Self::Unusable => "unusable",
        }
    }

    /// Whether the cached answer was used.
    #[must_use]
    pub const fn is_hit(self) -> bool {
        matches!(self, Self::Hit)
    }
}

/// What a cache hit restores.
#[derive(Clone, Debug)]
pub struct CachedDiscovery {
    /// The selected files, in the order discovery produced them.
    pub files: Vec<PathBuf>,
    /// Their canonical paths, so a hit need not re-canonicalise every file.
    pub canonical_files: Vec<PathBuf>,
    /// Files grouped by the repository containing them.
    pub repositories: BTreeMap<PathBuf, Vec<PathBuf>>,
    /// Files no repository contains.
    pub files_outside_repositories: Vec<PathBuf>,
    /// The ignore files that governed the result.
    pub ignore_files_read: Vec<PathBuf>,
    /// The skip counters, in the order [`skip_counter_names`] gives.
    pub skipped: [usize; 9],
    /// How many directory entries the recorded walk looked at.
    pub visited_entry_count: usize,
}

impl CachedDiscovery {
    /// One skip counter by name, or `0` for a name this entry does not carry.
    ///
    /// By name rather than by index so a counter added to the middle of
    /// [`skip_counter_names`] cannot silently shift every later reading.
    #[must_use]
    pub fn skipped_count(&self, name: &str) -> usize {
        skip_counter_names()
            .iter()
            .position(|counter| *counter == name)
            .and_then(|index| self.skipped.get(index).copied())
            .unwrap_or(0)
    }
}

/// The skip counters a cache entry carries, in a fixed order.
///
/// Named rather than positional in the stored JSON: a counter added in the
/// middle of the list would otherwise silently shift every later value, and the
/// format version alone would not catch a shift that still parses.
#[must_use]
pub const fn skip_counter_names() -> [&'static str; 9] {
    [
        "unknown",
        "hidden",
        "generated",
        "symlink",
        "excluded",
        "ignored",
        "glob",
        "symlink_escaped",
        "symlink_cycle",
    ]
}

impl DiscoveryCache {
    /// Opens (without creating) a cache rooted at `directory`.
    #[must_use]
    pub const fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// The directory entries are stored in.
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The content-addressed file name for one set of options.
    ///
    /// Everything that can change which files are selected goes into the hash.
    /// Anything left out would let two different requests share one entry,
    /// which is the failure this whole module exists to avoid.
    #[must_use]
    pub fn key(&self, options: &WorkspaceDiscoveryOptions) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&CACHE_FORMAT_VERSION.to_le_bytes());
        for root in &options.roots {
            // Canonical where possible: `.` and an absolute path to the same
            // directory must not be two entries with two different answers.
            let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            hasher.update(canonical.as_os_str().as_encoded_bytes());
            hasher.update(b"\0");
        }
        hasher.update(b"|flags|");
        for flag in [
            options.include_unknown,
            options.include_hidden,
            options.include_generated,
            options.ignore.respect_gitignore,
            options.ignore.respect_pareditignore,
            options.symlinks.follows(),
        ] {
            hasher.update(&[u8::from(flag)]);
        }
        hasher.update(b"|depth|");
        hasher.update(&options.max_depth.unwrap_or(usize::MAX).to_le_bytes());
        hasher.update(b"|exclude|");
        for exclude in &options.exclude {
            hasher.update(exclude.as_os_str().as_encoded_bytes());
            hasher.update(b"\0");
        }
        hasher.update(b"|include-globs|");
        for pattern in options.include_globs.patterns() {
            hasher.update(pattern.source().as_bytes());
            hasher.update(b"\0");
        }
        hasher.update(b"|exclude-globs|");
        for pattern in options.exclude_globs.patterns() {
            hasher.update(pattern.source().as_bytes());
            hasher.update(b"\0");
        }
        format!("{}.json", hasher.finalize().to_hex())
    }

    fn entry_path(&self, options: &WorkspaceDiscoveryOptions) -> PathBuf {
        self.directory.join(self.key(options))
    }

    /// Returns the cached result for `options`, if it is still valid.
    ///
    /// Never fails: every problem is reported as a miss, because falling back
    /// to a fresh walk is always correct and erroring out never is.
    #[must_use]
    pub fn lookup(
        &self,
        options: &WorkspaceDiscoveryOptions,
    ) -> (CacheOutcome, Option<CachedDiscovery>) {
        let path = self.entry_path(options);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => return (CacheOutcome::Missing, None),
        };
        if !metadata.is_file() || metadata.len() > MAX_CACHE_ENTRY_BYTES {
            return (CacheOutcome::Unusable, None);
        }
        let Ok(contents) = fs::read_to_string(&path) else {
            return (CacheOutcome::Unusable, None);
        };
        let Ok(entry) = serde_json::from_str::<Value>(&contents) else {
            return (CacheOutcome::Unusable, None);
        };
        if entry["format_version"].as_u64() != Some(u64::from(CACHE_FORMAT_VERSION)) {
            return (CacheOutcome::Unusable, None);
        }
        if !directories_are_unchanged(&entry) || !ignore_files_are_unchanged(&entry) {
            return (CacheOutcome::Stale, None);
        }
        match read_entry(&entry) {
            Some(cached) => (CacheOutcome::Hit, Some(cached)),
            None => (CacheOutcome::Unusable, None),
        }
    }

    /// Stores `discovery` under the key for `options`.
    ///
    /// Writes to a sibling temporary file and renames it into place, so a
    /// concurrent reader sees either the old entry or the new one.
    pub fn store(
        &self,
        options: &WorkspaceDiscoveryOptions,
        discovery: &WorkspaceDiscovery,
    ) -> Result<(), WorkspaceError> {
        fs::create_dir_all(&self.directory).map_err(|source| WorkspaceError::Io {
            context: format!("failed to create {}", self.directory.display()),
            source,
        })?;

        let entry = write_entry(discovery);
        let rendered = serde_json::to_string(&entry).map_err(|error| WorkspaceError::Io {
            context: "failed to encode the discovery cache entry".to_owned(),
            source: std::io::Error::other(error.to_string()),
        })?;

        let final_path = self.entry_path(options);
        // The temporary name carries the process id so two concurrent runs
        // never write the same scratch file.
        let temporary =
            self.directory
                .join(format!(".{}.{}.tmp", self.key(options), std::process::id()));
        fs::write(&temporary, rendered).map_err(|source| WorkspaceError::Io {
            context: format!("failed to write {}", temporary.display()),
            source,
        })?;
        fs::rename(&temporary, &final_path).map_err(|source| {
            let _ = fs::remove_file(&temporary);
            WorkspaceError::Io {
                context: format!("failed to install {}", final_path.display()),
                source,
            }
        })?;
        Ok(())
    }

    /// Removes every entry, returning how many were deleted.
    pub fn clear(&self) -> Result<usize, WorkspaceError> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(source) => {
                return Err(WorkspaceError::Io {
                    context: format!("failed to list {}", self.directory.display()),
                    source,
                });
            }
        };
        let mut removed = 0;
        for entry in entries {
            let entry = entry.map_err(|source| WorkspaceError::Io {
                context: format!("failed to list {}", self.directory.display()),
                source,
            })?;
            let path = entry.path();
            let is_entry = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension == "json" || extension == "tmp");
            if is_entry && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn write_entry(discovery: &WorkspaceDiscovery) -> Value {
    let mut skipped = serde_json::Map::new();
    for (name, value) in skip_counter_names()
        .into_iter()
        .zip(discovery.skip_counters())
    {
        skipped.insert(name.to_owned(), json!(value));
    }

    json!({
        "format_version": CACHE_FORMAT_VERSION,
        "files": paths(discovery.files()),
        "canonical_files": discovery.canonical_file_paths(),
        "repositories": discovery
            .repositories()
            .iter()
            .map(|(repository, files)| json!({
                "path": repository.display().to_string(),
                "files": paths(files),
            }))
            .collect::<Vec<_>>(),
        "files_outside_repositories": paths(discovery.files_outside_repositories()),
        "ignore_files": discovery
            .ignore_files_read()
            .iter()
            .map(|path| json!({
                "path": path.display().to_string(),
                "stamp": file_stamp(path),
            }))
            .collect::<Vec<_>>(),
        "directories": discovery
            .directory_fingerprints()
            .iter()
            .map(|fingerprint| json!({
                "path": fingerprint.path.display().to_string(),
                "modified_nanos": fingerprint.modified_nanos.map(|nanos| nanos.to_string()),
                "entry_count": fingerprint.entry_count,
            }))
            .collect::<Vec<_>>(),
        "visited_entry_count": discovery.visited_entry_count(),
        "skipped": Value::Object(skipped),
    })
}

fn read_entry(entry: &Value) -> Option<CachedDiscovery> {
    let mut repositories = BTreeMap::new();
    for repository in entry["repositories"].as_array()? {
        repositories.insert(
            PathBuf::from(repository["path"].as_str()?),
            read_paths(&repository["files"])?,
        );
    }
    let mut skipped = [0_usize; 9];
    for (index, name) in skip_counter_names().into_iter().enumerate() {
        skipped[index] = usize::try_from(entry["skipped"][name].as_u64()?).ok()?;
    }
    Some(CachedDiscovery {
        files: read_paths(&entry["files"])?,
        canonical_files: read_paths(&entry["canonical_files"])?,
        repositories,
        files_outside_repositories: read_paths(&entry["files_outside_repositories"])?,
        ignore_files_read: entry["ignore_files"]
            .as_array()?
            .iter()
            .map(|ignore| ignore["path"].as_str().map(PathBuf::from))
            .collect::<Option<Vec<_>>>()?,
        skipped,
        visited_entry_count: usize::try_from(entry["visited_entry_count"].as_u64()?).ok()?,
    })
}

/// Below this many entries, a thread costs more than the stats it would save.
///
/// Each item here is one `fs::metadata` call, or for a directory one
/// `fs::read_dir(...).count()` — cheap enough that thread setup can dominate
/// the work it was meant to parallelize. `benches/cache_dir.rs`'s
/// `cache-dir/warm/128` arm (8 directories, the previous threshold) measured
/// threading at that count as ~37% *slower* than the serial path; `warm/1024`
/// (64 directories) measured a ~28% win. This constant sits at the larger,
/// confirmed-safe end rather than guessing at the crossover between them.
const PARALLEL_VALIDATION_THRESHOLD: usize = 64;

fn directories_are_unchanged(entry: &Value) -> bool {
    let Some(directories) = entry["directories"].as_array() else {
        return false;
    };
    all_parallel(directories, directory_is_unchanged)
}

fn directory_is_unchanged(directory: &Value) -> bool {
    let Some(path) = directory["path"].as_str() else {
        return false;
    };
    let path = Path::new(path);
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_dir() {
        return false;
    }
    let recorded = directory["modified_nanos"].as_str();
    let current = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_nanos().to_string());
    if recorded.map(str::to_owned) != current {
        return false;
    }
    // The entry count is not redundant with the mtime. Many filesystems
    // record it at one-second granularity, so a file created and another
    // deleted within the same second leaves the timestamp untouched.
    let Some(recorded_count) = directory["entry_count"].as_u64() else {
        return false;
    };
    fs::read_dir(path).is_ok_and(|entries| entries.count() as u64 == recorded_count)
}

fn ignore_files_are_unchanged(entry: &Value) -> bool {
    let Some(ignore_files) = entry["ignore_files"].as_array() else {
        return false;
    };
    all_parallel(ignore_files, ignore_file_is_unchanged)
}

fn ignore_file_is_unchanged(ignore: &Value) -> bool {
    let Some(path) = ignore["path"].as_str() else {
        return false;
    };
    ignore["stamp"].as_str() == file_stamp(Path::new(path)).as_deref()
}

/// Whether `predicate` holds for every element of `items`.
///
/// Below [`PARALLEL_VALIDATION_THRESHOLD`], this is `Iterator::all` with its
/// short-circuit-on-first-false intact. Above it, `items` is split into one
/// contiguous chunk per worker and each chunk is validated on its own thread;
/// `predicate` reads only its own argument (one `fs::metadata`, and for a
/// directory one `fs::read_dir(...).count()`), so no chunk touches another's
/// state. A parallel run may do marginally more stat work than a serial
/// short-circuit would when an early entry is already stale, since every
/// chunk still runs to completion — an accepted tradeoff for validating a
/// workspace with tens of thousands of entries on every cache-freshness
/// check.
fn all_parallel<T: Sync>(items: &[T], predicate: impl Fn(&T) -> bool + Sync) -> bool {
    if items.len() < PARALLEL_VALIDATION_THRESHOLD {
        return items.iter().all(&predicate);
    }
    let workers = paredit_core_safety::limits::effective_jobs_or_available()
        .get()
        .min(items.len())
        .max(1);
    if workers <= 1 {
        return items.iter().all(&predicate);
    }
    let chunk_size = items.len().div_ceil(workers);
    let predicate = &predicate;
    thread::scope(|scope| {
        let handles = items
            .chunks(chunk_size)
            .map(|chunk| scope.spawn(move || chunk.iter().all(predicate)))
            .collect::<Vec<_>>();
        handles.into_iter().all(|handle| match handle.join() {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        })
    })
}

/// A file's identity as `mtime:len`, or `None` when it is not there.
///
/// Used for ignore files, whose contents change the selected set without
/// changing any directory's mtime.
fn file_stamp(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!("{}:{}", modified.as_nanos(), metadata.len()))
}

fn paths(values: &[PathBuf]) -> Vec<String> {
    values
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

fn read_paths(value: &Value) -> Option<Vec<PathBuf>> {
    value
        .as_array()?
        .iter()
        .map(|entry| entry.as_str().map(PathBuf::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{CacheOutcome, DiscoveryCache};
    use crate::workspace::{WorkspaceDiscoveryOptions, discover_workspace_files, glob::GlobSet};

    fn temporary_directory(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time is after the epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "paredit-cache-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn a_second_lookup_hits_and_returns_the_same_files() {
        let root = temporary_directory("hit");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(root.join("a.lisp"), "(defun a () nil)").expect("write fixture");
        // Outside the root on purpose: a cache written inside the tree it
        // describes changes that tree, which is its own first invalidation.
        let cache = DiscoveryCache::new(temporary_directory("hit-cache"));
        let options = WorkspaceDiscoveryOptions {
            roots: vec![root.clone()],
            ..WorkspaceDiscoveryOptions::default()
        };

        assert_eq!(cache.lookup(&options).0, CacheOutcome::Missing);
        let discovery = discover_workspace_files(&options).expect("discovery");
        cache.store(&options, &discovery).expect("store");

        let (outcome, cached) = cache.lookup(&options);
        assert_eq!(outcome, CacheOutcome::Hit);
        assert_eq!(cached.expect("cached entry").files, discovery.files());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn adding_a_file_makes_the_entry_stale() {
        let root = temporary_directory("stale");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(root.join("a.lisp"), "(defun a () nil)").expect("write fixture");
        let cache = DiscoveryCache::new(temporary_directory("stale-cache"));
        let options = WorkspaceDiscoveryOptions {
            roots: vec![root.clone()],
            ..WorkspaceDiscoveryOptions::default()
        };
        let discovery = discover_workspace_files(&options).expect("discovery");
        cache.store(&options, &discovery).expect("store");

        std::fs::write(root.join("b.lisp"), "(defun b () nil)").expect("write fixture");
        assert_eq!(cache.lookup(&options).0, CacheOutcome::Stale);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_different_filter_uses_a_different_entry() {
        let root = temporary_directory("key");
        std::fs::create_dir_all(&root).expect("create root");
        let cache = DiscoveryCache::new(temporary_directory("key-cache"));
        let base = WorkspaceDiscoveryOptions {
            roots: vec![root.clone()],
            ..WorkspaceDiscoveryOptions::default()
        };
        let narrowed = WorkspaceDiscoveryOptions {
            include_globs: GlobSet::parse_file("src/**\n").expect("patterns compile"),
            ..base.clone()
        };

        assert_ne!(
            cache.key(&base),
            cache.key(&narrowed),
            "a filter that changes the answer must change the key"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
