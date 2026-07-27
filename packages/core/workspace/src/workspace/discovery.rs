use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::error::{WorkspaceError, WorkspaceLimit, WorkspaceRefusal};
use cap_std::ambient_authority;
use cap_std::fs::Dir;
#[cfg(unix)]
use cap_std::fs::{OpenOptions, OpenOptionsExt};

use super::filters::{is_generated_workspace_path, is_hidden_workspace_path};
use super::types::{WorkspaceDiscovery, WorkspaceDiscoveryOptions, WorkspaceLimits};
use crate::fs_identity::FilesystemIdentity;
use paredit_core_syntax::dialect::Dialect;

pub(super) const READ_CHUNK_BYTES: usize = 64 * 1024;
const MAX_EXCLUDE_PATHS: usize = 4_096;
const MAX_EXCLUDE_COMPONENTS: usize = 65_536;

#[derive(Default)]
struct ExcludeNode {
    children: HashMap<OsString, usize>,
    excludes_subtree: bool,
}

pub(super) struct ExcludeIndex {
    nodes: Vec<ExcludeNode>,
    current_dir: PathBuf,
}

struct ScanRoot {
    lexical: PathBuf,
    canonical: PathBuf,
}

struct ResolvedRoot {
    lexical: PathBuf,
    canonical: PathBuf,
    is_symlink: bool,
    can_cover_descendants: bool,
}

impl ScanRoot {
    fn new(path: &Path, canonical: &Path, current_dir: &Path) -> Self {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            current_dir.join(path)
        };
        Self {
            lexical: lexically_normalize(&absolute),
            canonical: canonical.to_path_buf(),
        }
    }

    fn canonical_candidate(&self, path: &Path, current_dir: &Path) -> Option<PathBuf> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            current_dir.join(path)
        };
        let normalized = lexically_normalize(&absolute);
        let relative = normalized.strip_prefix(&self.lexical).ok()?;
        Some(lexically_normalize(&self.canonical.join(relative)))
    }
}

impl ExcludeIndex {
    pub(super) fn new(excludes: &[PathBuf]) -> std::result::Result<Self, WorkspaceError> {
        if excludes.len() > MAX_EXCLUDE_PATHS {
            return Err(WorkspaceLimit::ExcludePaths {
                actual: excludes.len(),
                maximum: MAX_EXCLUDE_PATHS,
            }
            .into());
        }

        let current_dir = std::env::current_dir()
            .map_err(WorkspaceError::io("failed to resolve current directory"))?;
        let mut index = Self {
            nodes: vec![ExcludeNode::default()],
            current_dir,
        };
        for exclude in excludes {
            let absolute = if exclude.is_absolute() {
                exclude.clone()
            } else {
                index.current_dir.join(exclude)
            };
            index.insert(&normalize_path(&absolute))?;
        }
        Ok(index)
    }

    fn insert(&mut self, path: &Path) -> std::result::Result<(), WorkspaceError> {
        let mut node_index = 0;
        for component in path.components() {
            if self.nodes[node_index].excludes_subtree {
                return Ok(());
            }
            let component = component.as_os_str().to_os_string();
            let child_index = if let Some(index) = self.nodes[node_index].children.get(&component) {
                *index
            } else {
                if self.nodes.len() >= MAX_EXCLUDE_COMPONENTS {
                    return Err(WorkspaceLimit::ExcludeComponents {
                        maximum: MAX_EXCLUDE_COMPONENTS,
                    }
                    .into());
                }
                let index = self.nodes.len();
                self.nodes.push(ExcludeNode::default());
                self.nodes[node_index].children.insert(component, index);
                index
            };
            node_index = child_index;
        }
        self.nodes[node_index].excludes_subtree = true;
        self.nodes[node_index].children.clear();
        Ok(())
    }

    fn contains_scanned(&self, path: &Path, root: &ScanRoot) -> bool {
        root.canonical_candidate(path, &self.current_dir)
            .is_some_and(|candidate| self.contains_normalized_counted(&candidate).0)
    }

    #[cfg(test)]
    fn contains_counted(&self, path: &Path) -> (bool, usize) {
        if self.nodes.len() == 1 && self.nodes[0].children.is_empty() {
            return (false, 0);
        }
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.current_dir.join(path)
        };
        let normalized = lexically_normalize(&absolute);
        self.contains_normalized_counted(&normalized)
    }

    fn contains_normalized_counted(&self, normalized: &Path) -> (bool, usize) {
        let mut node_index = 0;
        let mut visited = 0;
        for component in normalized.components() {
            visited += 1;
            let Some(child_index) = self.nodes[node_index].children.get(component.as_os_str())
            else {
                return (false, visited);
            };
            node_index = *child_index;
            if self.nodes[node_index].excludes_subtree {
                return (true, visited);
            }
        }
        (self.nodes[node_index].excludes_subtree, visited)
    }

    #[cfg(test)]
    pub(super) fn match_steps(&self, path: &Path) -> (bool, usize) {
        self.contains_counted(path)
    }
}

struct ReadReservation<'a> {
    total: &'a AtomicU64,
    reserved: u64,
    committed: bool,
}

impl<'a> ReadReservation<'a> {
    const fn new(total: &'a AtomicU64) -> Self {
        Self {
            total,
            reserved: 0,
            committed: false,
        }
    }

    fn reserve(
        &mut self,
        bytes: u64,
        maximum: u64,
        path: &Path,
    ) -> std::result::Result<(), WorkspaceError> {
        self.total
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(bytes).filter(|total| *total <= maximum)
            })
            .map_err(|current| WorkspaceLimit::TotalRead {
                path: path.to_path_buf(),
                current,
                bytes,
                maximum,
            })?;
        self.reserved = self
            .reserved
            .checked_add(bytes)
            .expect("reservation cannot exceed the bounded total read count");
        Ok(())
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for ReadReservation<'_> {
    fn drop(&mut self) {
        if !self.committed && self.reserved != 0 {
            self.total.fetch_sub(self.reserved, Ordering::AcqRel);
        }
    }
}

pub(super) fn read_bounded<R: Read>(
    mut reader: R,
    path: &Path,
    limits: WorkspaceLimits,
    read_bytes: &AtomicU64,
) -> std::result::Result<Vec<u8>, WorkspaceError> {
    let initial_capacity = usize::try_from(limits.max_file_bytes)
        .unwrap_or(usize::MAX)
        .min(READ_CHUNK_BYTES);
    let mut content = Vec::with_capacity(initial_capacity);
    let mut chunk = [0_u8; READ_CHUNK_BYTES];
    let mut reservation = ReadReservation::new(read_bytes);

    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|source| WorkspaceError::Io {
                context: format!("failed to read {}", path.display()),
                source,
            })?;
        if count == 0 {
            break;
        }

        let next_len = content
            .len()
            .checked_add(count)
            .ok_or(WorkspaceError::Unavailable("workspace file size overflow"))?;
        if u64::try_from(next_len).unwrap_or(u64::MAX) > limits.max_file_bytes {
            return Err(WorkspaceLimit::ReadSize {
                path: path.to_path_buf(),
                maximum: limits.max_file_bytes,
            }
            .into());
        }

        let bytes = u64::try_from(count)
            .map_err(|_| WorkspaceError::Unavailable("workspace read chunk size overflow"))?;
        reservation.reserve(bytes, limits.max_total_bytes, path)?;
        content
            .try_reserve(count)
            .map_err(|_| WorkspaceError::Unavailable("failed to allocate workspace file buffer"))?;
        content.extend_from_slice(&chunk[..count]);
    }

    reservation.commit();
    Ok(content)
}

pub fn discover_workspace_files(
    options: &WorkspaceDiscoveryOptions,
) -> std::result::Result<WorkspaceDiscovery, WorkspaceError> {
    discover_workspace_files_with_limits(options, WorkspaceLimits::default())
}

pub(super) fn discover_workspace_files_with_limits(
    options: &WorkspaceDiscoveryOptions,
    limits: WorkspaceLimits,
) -> std::result::Result<WorkspaceDiscovery, WorkspaceError> {
    if options.roots.len() > limits.max_roots {
        return Err(WorkspaceLimit::Roots {
            actual: options.roots.len(),
            maximum: limits.max_roots,
        }
        .into());
    }

    let mut discovery = WorkspaceDiscovery {
        limits,
        ..WorkspaceDiscovery::default()
    };
    let mut seen = BTreeSet::new();
    let excludes = ExcludeIndex::new(&options.exclude)?;

    let mut resolved_roots = options
        .roots
        .iter()
        .map(|root| {
            let canonical = fs::canonicalize(root).map_err(|source| WorkspaceError::Io {
                context: format!("failed to resolve workspace root {}", root.display()),
                source,
            })?;
            let metadata = fs::symlink_metadata(root).map_err(|source| WorkspaceError::Io {
                context: format!("failed to inspect {}", root.display()),
                source,
            })?;
            Ok(ResolvedRoot {
                lexical: root.clone(),
                canonical,
                is_symlink: metadata.file_type().is_symlink(),
                can_cover_descendants: metadata.is_dir(),
            })
        })
        .collect::<std::result::Result<Vec<_>, WorkspaceError>>()?;
    resolved_roots.sort_by(|left, right| {
        left.canonical
            .cmp(&right.canonical)
            .then_with(|| left.is_symlink.cmp(&right.is_symlink))
            .then_with(|| left.lexical.cmp(&right.lexical))
    });
    resolved_roots.dedup_by(|left, right| left.canonical == right.canonical);

    discovery.canonical_roots = resolved_roots
        .iter()
        .map(|root| root.canonical.clone())
        .collect();
    discovery.root_dirs = discovery
        .canonical_roots
        .iter()
        .map(|root| {
            let capability_base = if root.is_file() {
                root.parent().ok_or_else(|| WorkspaceError::Io {
                    context: format!("workspace file root has no parent: {}", root.display()),
                    source: std::io::Error::other(""),
                })?
            } else {
                root.as_path()
            };
            let ambient_metadata =
                fs::metadata(capability_base).map_err(|source| WorkspaceError::Io {
                    context: format!(
                        "failed to inspect ambient workspace root {}",
                        capability_base.display()
                    ),
                    source,
                })?;
            let dir =
                Dir::open_ambient_dir(capability_base, ambient_authority()).map_err(|source| {
                    WorkspaceError::Io {
                        context: format!("failed to open workspace root {}", root.display()),
                        source,
                    }
                })?;
            let capability_metadata = dir.dir_metadata().map_err(|source| WorkspaceError::Io {
                context: format!(
                    "failed to inspect workspace root capability {}",
                    capability_base.display()
                ),
                source,
            })?;
            let identity = FilesystemIdentity::from_cap(&capability_metadata).ok_or(
                WorkspaceError::Unavailable("workspace root capability identity is unavailable"),
            )?;
            let root_is_unchanged = ambient_metadata.is_dir()
                && FilesystemIdentity::from_std(&ambient_metadata) == Some(identity);
            if !root_is_unchanged {
                return Err(WorkspaceRefusal::RootChanged {
                    path: capability_base.to_path_buf(),
                }
                .into());
            }
            Ok((
                root.clone(),
                capability_base.to_path_buf(),
                std::sync::Arc::new(dir),
                identity,
            ))
        })
        .collect::<std::result::Result<Vec<_>, WorkspaceError>>()?;

    let covering_directories = resolved_roots
        .iter()
        .filter(|root| root.can_cover_descendants)
        .map(|root| root.canonical.clone())
        .collect::<BTreeSet<_>>();
    let scan_roots = resolved_roots
        .iter()
        .filter(|root| {
            options.max_depth.is_some()
                || !root
                    .canonical
                    .ancestors()
                    .skip(1)
                    .any(|ancestor| covering_directories.contains(ancestor))
        })
        .map(|root| {
            (
                root.lexical.clone(),
                ScanRoot::new(&root.lexical, &root.canonical, &excludes.current_dir),
            )
        })
        .collect::<Vec<_>>();
    for (root, scan_root) in &scan_roots {
        collect_workspace_files(
            root,
            scan_root,
            0,
            options,
            &excludes,
            &mut discovery,
            &mut seen,
        )?;
    }

    discovery.files.sort();
    Ok(discovery)
}

fn collect_workspace_files(
    path: &Path,
    scan_root: &ScanRoot,
    depth: usize,
    options: &WorkspaceDiscoveryOptions,
    excludes: &ExcludeIndex,
    discovery: &mut WorkspaceDiscovery,
    seen: &mut BTreeSet<PathBuf>,
) -> std::result::Result<(), WorkspaceError> {
    if excludes.contains_scanned(path, scan_root) {
        discovery.skipped_excluded_count += 1;
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path).map_err(|source| WorkspaceError::Io {
        context: format!("failed to inspect {}", path.display()),
        source,
    })?;

    if metadata.file_type().is_symlink() {
        discovery.skipped_symlink_count += 1;
        return Ok(());
    }

    if metadata.is_dir() {
        collect_workspace_directory(path, scan_root, depth, options, excludes, discovery, seen)?;
        return Ok(());
    }

    if metadata.is_file() {
        collect_workspace_file(path, options, discovery, seen)?;
    }

    Ok(())
}

fn collect_workspace_directory(
    path: &Path,
    scan_root: &ScanRoot,
    depth: usize,
    options: &WorkspaceDiscoveryOptions,
    excludes: &ExcludeIndex,
    discovery: &mut WorkspaceDiscovery,
    seen: &mut BTreeSet<PathBuf>,
) -> std::result::Result<(), WorkspaceError> {
    if !options.include_hidden && is_hidden_workspace_path(path) {
        discovery.skipped_hidden_count += 1;
        return Ok(());
    }

    if !options.include_generated && is_generated_workspace_path(path) {
        discovery.skipped_generated_count += 1;
        return Ok(());
    }

    if options
        .max_depth
        .is_some_and(|max_depth| depth >= max_depth)
    {
        return Ok(());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(path).map_err(|source| WorkspaceError::Io {
        context: format!("failed to list {}", path.display()),
        source,
    })? {
        discovery.visited_entry_count =
            discovery
                .visited_entry_count
                .checked_add(1)
                .ok_or(WorkspaceError::Unavailable(
                    "workspace entry count overflow",
                ))?;
        if discovery.visited_entry_count > discovery.limits.max_entries {
            return Err(WorkspaceLimit::Entries {
                path: path.to_path_buf(),
                maximum: discovery.limits.max_entries,
            }
            .into());
        }
        entries.push(
            entry
                .map_err(|source| WorkspaceError::Io {
                    context: format!("failed to list {}", path.display()),
                    source,
                })?
                .path(),
        );
    }
    entries.sort();

    for entry in entries {
        collect_workspace_files(
            &entry,
            scan_root,
            depth + 1,
            options,
            excludes,
            discovery,
            seen,
        )?;
    }

    Ok(())
}

fn collect_workspace_file(
    path: &Path,
    options: &WorkspaceDiscoveryOptions,
    discovery: &mut WorkspaceDiscovery,
    seen: &mut BTreeSet<PathBuf>,
) -> std::result::Result<(), WorkspaceError> {
    if !options.include_hidden && is_hidden_workspace_path(path) {
        discovery.skipped_hidden_count += 1;
        return Ok(());
    }

    let dialect = Dialect::detect(Some(path), None);
    if dialect == Dialect::Unknown && !options.include_unknown {
        discovery.skipped_unknown_count += 1;
        return Ok(());
    }

    let canonical = fs::canonicalize(path).map_err(|source| WorkspaceError::Io {
        context: format!("failed to resolve workspace file {}", path.display()),
        source,
    })?;
    if discovery.root_capability_for(&canonical).is_none() {
        return Err(WorkspaceRefusal::OutsideRoots {
            path: canonical.clone(),
        }
        .into());
    }
    let metadata = fs::symlink_metadata(&canonical).map_err(|source| WorkspaceError::Io {
        context: format!("failed to inspect {}", canonical.display()),
        source,
    })?;
    if !metadata.is_file() {
        return Err(WorkspaceRefusal::NonRegularFile {
            path: canonical.clone(),
        }
        .into());
    }
    if seen.insert(canonical.clone()) {
        if discovery.files.len() >= discovery.limits.max_files {
            return Err(WorkspaceLimit::Files {
                maximum: discovery.limits.max_files,
            }
            .into());
        }
        if metadata.len() > discovery.limits.max_file_bytes {
            return Err(WorkspaceLimit::FileSize {
                path: canonical.clone(),
                actual: metadata.len(),
                maximum: discovery.limits.max_file_bytes,
            }
            .into());
        }
        let total = discovery
            .discovered_bytes
            .checked_add(metadata.len())
            .ok_or(WorkspaceError::Unavailable("workspace byte count overflow"))?;
        if total > discovery.limits.max_total_bytes {
            return Err(WorkspaceLimit::TotalBytes {
                actual: total,
                maximum: discovery.limits.max_total_bytes,
            }
            .into());
        }
        discovery.discovered_bytes = total;
        discovery.canonical_files.insert(canonical);
        discovery.files.push(path.to_path_buf());
    }
    Ok(())
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn normalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        let normalized = lexically_normalize(path);
        fs::canonicalize(&normalized).unwrap_or(normalized)
    })
}

fn validate_workspace_root_identity(
    capability_base: &Path,
    root_dir: &Dir,
    expected_identity: FilesystemIdentity,
) -> std::result::Result<(), WorkspaceError> {
    let ambient_metadata = fs::metadata(capability_base).map_err(|source| WorkspaceError::Io {
        context: format!(
            "failed to inspect ambient workspace root {}",
            capability_base.display()
        ),
        source,
    })?;
    let capability_metadata = root_dir
        .dir_metadata()
        .map_err(|source| WorkspaceError::Io {
            context: format!(
                "failed to inspect workspace root capability {}",
                capability_base.display()
            ),
            source,
        })?;
    let identity_is_unchanged = ambient_metadata.is_dir()
        && FilesystemIdentity::from_std(&ambient_metadata) == Some(expected_identity)
        && FilesystemIdentity::from_cap(&capability_metadata) == Some(expected_identity);
    if !identity_is_unchanged {
        return Err(WorkspaceRefusal::RootIdentityChanged {
            path: capability_base.to_path_buf(),
        }
        .into());
    }
    Ok(())
}

impl WorkspaceDiscovery {
    pub fn read_file(&self, path: &Path) -> std::result::Result<Vec<u8>, WorkspaceError> {
        let metadata = fs::symlink_metadata(path).map_err(|source| WorkspaceError::Io {
            context: format!("failed to inspect {}", path.display()),
            source,
        })?;
        let is_regular_file = metadata.is_file() && !metadata.file_type().is_symlink();
        if !is_regular_file {
            return Err(WorkspaceRefusal::ReplacedOrNonRegular {
                path: path.to_path_buf(),
            }
            .into());
        }

        let canonical = fs::canonicalize(path).map_err(|source| WorkspaceError::Io {
            context: format!("failed to resolve workspace file {}", path.display()),
            source,
        })?;
        let (_, capability_base, root_dir, root_identity) = self
            .root_capability_for(&canonical)
            .ok_or_else(|| WorkspaceError::Io {
                context: format!(
                    "refusing workspace file outside canonical roots: {}",
                    canonical.display()
                ),
                source: std::io::Error::other(""),
            })?;
        if !self.contains_canonical_file(&canonical) {
            return Err(WorkspaceRefusal::NotSelected {
                path: canonical.clone(),
            }
            .into());
        }
        validate_workspace_root_identity(capability_base, root_dir, *root_identity)?;
        let relative = canonical.strip_prefix(capability_base).map_err(|_| {
            WorkspaceError::Unavailable("workspace file is outside selected root capability")
        })?;
        let pre_open_metadata =
            root_dir
                .symlink_metadata(relative)
                .map_err(|source| WorkspaceError::Io {
                    context: format!("failed to inspect {}", canonical.display()),
                    source,
                })?;
        let is_regular_file =
            pre_open_metadata.is_file() && !pre_open_metadata.file_type().is_symlink();
        if !is_regular_file {
            return Err(WorkspaceRefusal::ReplacedOrNonRegular {
                path: canonical.clone(),
            }
            .into());
        }
        let ambient_identity = FilesystemIdentity::from_std(&metadata).ok_or(
            WorkspaceError::Unavailable("ambient workspace file identity is unavailable"),
        )?;
        let pre_open_identity = FilesystemIdentity::from_cap(&pre_open_metadata).ok_or(
            WorkspaceError::Unavailable("workspace file capability identity is unavailable"),
        )?;
        if ambient_identity != pre_open_identity {
            return Err(WorkspaceRefusal::IdentityMismatch {
                path: canonical.clone(),
            }
            .into());
        }
        #[cfg(unix)]
        let cap_file = {
            let mut options = OpenOptions::new();
            options
                .read(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
            root_dir.open_with(relative, &options)
        };
        #[cfg(not(unix))]
        let cap_file = root_dir.open(relative);
        let cap_file = cap_file.map_err(|source| WorkspaceError::Io {
            context: format!("failed to open {}", canonical.display()),
            source,
        })?;
        let opened_metadata = cap_file.metadata().map_err(|source| WorkspaceError::Io {
            context: format!("failed to inspect open file {}", canonical.display()),
            source,
        })?;
        if !opened_metadata.is_file() {
            return Err(WorkspaceRefusal::NonRegularFile {
                path: canonical.clone(),
            }
            .into());
        }
        let opened_identity = FilesystemIdentity::from_cap(&opened_metadata).ok_or(
            WorkspaceError::Unavailable("open workspace file identity is unavailable"),
        )?;
        if pre_open_identity != opened_identity {
            return Err(WorkspaceRefusal::ReplacedWhileOpening {
                path: canonical.clone(),
            }
            .into());
        }
        let content = read_bounded(
            cap_file.into_std(),
            &canonical,
            self.limits,
            self.read_bytes.as_ref(),
        )?;
        validate_workspace_root_identity(capability_base, root_dir, *root_identity)?;
        let current_ambient_metadata =
            fs::symlink_metadata(&canonical).map_err(|source| WorkspaceError::Io {
                context: format!("failed to re-inspect {}", canonical.display()),
                source,
            })?;
        let file_is_unchanged = current_ambient_metadata.is_file()
            && !current_ambient_metadata.file_type().is_symlink()
            && FilesystemIdentity::from_std(&current_ambient_metadata) == Some(opened_identity);
        if !file_is_unchanged {
            return Err(WorkspaceRefusal::ChangedWhileReading {
                path: canonical.to_path_buf(),
            }
            .into());
        }
        validate_workspace_root_identity(capability_base, root_dir, *root_identity)?;
        Ok(content)
    }
}
