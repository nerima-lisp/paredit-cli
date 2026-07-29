use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use cap_std::fs::Dir;

use crate::fs_identity::FilesystemIdentity;

#[derive(Debug, Clone)]
pub struct WorkspaceDiscoveryOptions {
    pub roots: Vec<PathBuf>,
    pub include_unknown: bool,
    pub include_hidden: bool,
    pub include_generated: bool,
    pub max_depth: Option<usize>,
    pub exclude: Vec<PathBuf>,
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
