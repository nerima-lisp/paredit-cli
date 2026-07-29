use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use cap_std::fs::Dir;

use crate::fs_identity::FilesystemIdentity;

use super::glob::GlobSet;
use super::ignore::IgnoreOptions;

/// A directory's identity at the moment discovery listed it.
///
/// Kept so a later run can ask "is this still the same directory" without
/// walking it again. Both fields matter: many filesystems record mtime at
/// one-second granularity, and the entry count catches a same-second change
/// the timestamp cannot see.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryFingerprint {
    /// The absolute, lexically normalised directory path.
    pub path: PathBuf,
    /// Nanoseconds since the epoch, or `None` where the platform has no mtime.
    pub modified_nanos: Option<u128>,
    /// How many entries `read_dir` returned.
    pub entry_count: usize,
}

/// What a symlink encountered during traversal should do.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SymlinkPolicy {
    /// Count it and move on.
    ///
    /// The historical behaviour and still the default. A symlink is the one
    /// filesystem object that can make a bounded traversal unbounded, and the
    /// tool's whole safety model rests on every read going through a directory
    /// capability opened from a canonical root.
    #[default]
    Skip,
    /// Traverse it, provided its target resolves inside the canonical roots.
    ///
    /// The restriction is not a shortcut, it is the point: a followed symlink
    /// whose target escapes the roots would be read through a capability that
    /// was never opened for it. Such a link is counted separately (see
    /// [`WorkspaceDiscovery::skipped_symlink_escaped_count`]) so the answer is
    /// "add that directory as a root", not silence.
    Follow,
}

impl SymlinkPolicy {
    /// A stable identifier for JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skip => "skip",
            Self::Follow => "follow",
        }
    }

    /// Whether traversal descends through symlinks.
    #[must_use]
    pub const fn follows(self) -> bool {
        matches!(self, Self::Follow)
    }
}

/// Everything that decides which files a scan selects.
///
/// Build one with `..WorkspaceDiscoveryOptions::default()` rather than by
/// listing every field: this struct grows once per input feature, and a literal
/// that names all of them turns each addition into a mechanical edit of every
/// call site — which is exactly how a default ends up silently different at one
/// of them.
#[derive(Debug, Clone)]
pub struct WorkspaceDiscoveryOptions {
    /// Files or directories to scan.
    pub roots: Vec<PathBuf>,
    /// Keep files whose extension names no known dialect.
    pub include_unknown: bool,
    /// Keep dot-prefixed files and directories.
    pub include_hidden: bool,
    /// Keep build and dependency directories (`target`, `node_modules`, …).
    pub include_generated: bool,
    /// Directory recursion depth from each root.
    pub max_depth: Option<usize>,
    /// Exact paths to skip, matched by canonical path component.
    pub exclude: Vec<PathBuf>,
    /// Which ignore files to honour.
    pub ignore: IgnoreOptions,
    /// When non-empty, a file is kept only if it matches one of these.
    pub include_globs: GlobSet,
    /// Patterns whose matches are skipped, overriding `include_globs`.
    pub exclude_globs: GlobSet,
    /// What to do with a symlink.
    pub symlinks: SymlinkPolicy,
}

impl Default for WorkspaceDiscoveryOptions {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            include_unknown: false,
            include_hidden: false,
            include_generated: false,
            max_depth: None,
            exclude: Vec::new(),
            ignore: IgnoreOptions::default(),
            include_globs: GlobSet::new(),
            exclude_globs: GlobSet::new(),
            symlinks: SymlinkPolicy::Skip,
        }
    }
}

impl WorkspaceDiscoveryOptions {
    /// The options a caller wants when it has nothing to say but the roots.
    #[must_use]
    pub fn for_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots,
            ..Self::default()
        }
    }

    /// The pre-ignore-file behaviour: no `.gitignore`, no `.pareditignore`.
    ///
    /// Kept for callers that must reproduce a byte-identical earlier result,
    /// such as re-resolving the file set behind a stored refactoring manifest.
    #[must_use]
    pub const fn without_ignore_files(mut self) -> Self {
        self.ignore = IgnoreOptions::none();
        self
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceDiscovery {
    pub(super) files: Vec<PathBuf>,
    pub(super) canonical_files: BTreeSet<PathBuf>,
    pub(super) skipped_unknown_count: usize,
    pub(super) skipped_hidden_count: usize,
    pub(super) skipped_generated_count: usize,
    pub(super) skipped_symlink_count: usize,
    pub(super) skipped_excluded_count: usize,
    pub(super) skipped_ignored_count: usize,
    pub(super) skipped_glob_count: usize,
    pub(super) skipped_symlink_escaped_count: usize,
    pub(super) skipped_symlink_cycle_count: usize,
    pub(super) repositories: BTreeMap<PathBuf, Vec<PathBuf>>,
    pub(super) files_outside_repositories: Vec<PathBuf>,
    pub(super) ignore_files_read: Vec<PathBuf>,
    pub(super) directory_stamps: Vec<DirectoryFingerprint>,
    pub(super) canonical_roots: Vec<PathBuf>,
    pub(super) root_dirs: Vec<(PathBuf, PathBuf, Arc<Dir>, FilesystemIdentity)>,
    pub(super) visited_entry_count: usize,
    pub(super) discovered_bytes: u64,
    pub(super) read_bytes: Arc<AtomicU64>,
    pub(super) limits: WorkspaceLimits,
}

pub(super) type WorkspaceRootCapability = (PathBuf, PathBuf, Arc<Dir>, FilesystemIdentity);

/// The bounds one traversal will not exceed.
///
/// Public since a caller may lower them: a CI container with a 512 MB budget
/// needs to say so, and the defaults are sized for a workstation. Raising them
/// is possible in-process and deliberately not offered on the command line —
/// see `paredit_core_safety::limits`, which is where the ratchet lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceLimits {
    /// Bounds raw root inputs before filesystem resolution and canonical deduplication.
    pub max_roots: usize,
    pub max_entries: usize,
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for WorkspaceLimits {
    fn default() -> Self {
        Self {
            max_roots: 1_024,
            max_entries: 100_000,
            max_files: 50_000,
            max_file_bytes: 16 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
        }
    }
}

impl Default for WorkspaceDiscovery {
    fn default() -> Self {
        Self {
            files: Vec::new(),
            canonical_files: BTreeSet::new(),
            skipped_unknown_count: 0,
            skipped_hidden_count: 0,
            skipped_generated_count: 0,
            skipped_symlink_count: 0,
            skipped_excluded_count: 0,
            skipped_ignored_count: 0,
            skipped_glob_count: 0,
            skipped_symlink_escaped_count: 0,
            skipped_symlink_cycle_count: 0,
            repositories: BTreeMap::new(),
            files_outside_repositories: Vec::new(),
            ignore_files_read: Vec::new(),
            directory_stamps: Vec::new(),
            canonical_roots: Vec::new(),
            root_dirs: Vec::new(),
            visited_entry_count: 0,
            discovered_bytes: 0,
            read_bytes: Arc::new(AtomicU64::new(0)),
            limits: WorkspaceLimits::default(),
        }
    }
}

impl WorkspaceDiscovery {
    #[must_use]
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    #[must_use]
    pub fn into_files(self) -> Vec<PathBuf> {
        self.files
    }

    #[must_use]
    pub const fn skipped_unknown_count(&self) -> usize {
        self.skipped_unknown_count
    }

    #[must_use]
    pub const fn skipped_hidden_count(&self) -> usize {
        self.skipped_hidden_count
    }

    #[must_use]
    pub const fn skipped_generated_count(&self) -> usize {
        self.skipped_generated_count
    }

    #[must_use]
    pub const fn skipped_symlink_count(&self) -> usize {
        self.skipped_symlink_count
    }

    #[must_use]
    pub const fn skipped_excluded_count(&self) -> usize {
        self.skipped_excluded_count
    }

    /// How many paths an ignore file excluded.
    #[must_use]
    pub const fn skipped_ignored_count(&self) -> usize {
        self.skipped_ignored_count
    }

    /// How many paths an `--include` or `--exclude-glob` pattern excluded.
    #[must_use]
    pub const fn skipped_glob_count(&self) -> usize {
        self.skipped_glob_count
    }

    /// How many symlinks resolved outside every canonical root.
    ///
    /// Non-zero only under [`SymlinkPolicy::Follow`], and the number a user
    /// needs to see: it is the difference between "there was nothing there"
    /// and "what was there was not this run's to read".
    #[must_use]
    pub const fn skipped_symlink_escaped_count(&self) -> usize {
        self.skipped_symlink_escaped_count
    }

    /// How many symlinks pointed back into a directory already being walked.
    #[must_use]
    pub const fn skipped_symlink_cycle_count(&self) -> usize {
        self.skipped_symlink_cycle_count
    }

    /// The discovered files grouped by the repository that contains them.
    ///
    /// A run over several checkouts — a monorepo of independent repositories,
    /// or two roots in different clones — needs this to report per repository
    /// rather than as one undifferentiated list.
    #[must_use]
    pub const fn repositories(&self) -> &BTreeMap<PathBuf, Vec<PathBuf>> {
        &self.repositories
    }

    /// Discovered files that no repository contains.
    #[must_use]
    pub fn files_outside_repositories(&self) -> &[PathBuf] {
        &self.files_outside_repositories
    }

    /// The ignore files that contributed patterns, in the order they were read.
    #[must_use]
    pub fn ignore_files_read(&self) -> &[PathBuf] {
        &self.ignore_files_read
    }

    /// The canonical roots this scan was authorised for.
    #[must_use]
    pub fn canonical_roots(&self) -> &[PathBuf] {
        &self.canonical_roots
    }

    /// How many directory entries the traversal looked at.
    #[must_use]
    pub const fn visited_entry_count(&self) -> usize {
        self.visited_entry_count
    }

    /// The identity of every directory the traversal listed.
    #[must_use]
    pub fn directory_fingerprints(&self) -> &[DirectoryFingerprint] {
        &self.directory_stamps
    }

    /// The canonical path of every selected file.
    #[must_use]
    pub fn canonical_file_paths(&self) -> Vec<String> {
        self.canonical_files
            .iter()
            .map(|path| path.display().to_string())
            .collect()
    }

    /// The skip counters, in the order the cache records them.
    #[must_use]
    pub const fn skip_counters(&self) -> [usize; 9] {
        [
            self.skipped_unknown_count,
            self.skipped_hidden_count,
            self.skipped_generated_count,
            self.skipped_symlink_count,
            self.skipped_excluded_count,
            self.skipped_ignored_count,
            self.skipped_glob_count,
            self.skipped_symlink_escaped_count,
            self.skipped_symlink_cycle_count,
        ]
    }

    pub(super) fn contains_canonical_file(&self, path: &Path) -> bool {
        self.canonical_files.contains(path)
    }

    pub(super) fn root_capability_for(
        &self,
        canonical_path: &Path,
    ) -> Option<&WorkspaceRootCapability> {
        canonical_path.ancestors().find_map(|ancestor| {
            let index = self
                .canonical_roots
                .binary_search_by(|root| root.as_path().cmp(ancestor))
                .ok()?;
            self.root_dirs.get(index)
        })
    }
}
