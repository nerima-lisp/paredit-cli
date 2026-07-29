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

use super::cache::CachedDiscovery;
use super::filters::{is_generated_workspace_path, is_hidden_workspace_path};
use super::ignore::{IgnoreCache, IgnoreStack};
use super::types::{
    DirectoryFingerprint, WorkspaceDiscovery, WorkspaceDiscoveryOptions, WorkspaceLimits,
    WorkspaceRootCapability,
};
use super::vcs::{find_repository_root, is_repository_root};
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

/// Opens one directory capability per canonical root.
///
/// Every read this package performs goes through one of these rather than
/// through an ambient path, which is what bounds a run to the roots it was
/// given. Factored out of the walk so a cache hit can re-open them without
/// re-walking: the capabilities are the one part of a discovery that cannot
/// be serialised, and re-opening them is O(roots) where the walk is O(tree).
fn open_root_capabilities(
    canonical_roots: &[PathBuf],
) -> std::result::Result<Vec<WorkspaceRootCapability>, WorkspaceError> {
    canonical_roots
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
        .collect()
}

/// Rebuilds a full discovery from a cache entry, without walking the tree.
///
/// A cache hit restores the file list and the counters, but a
/// [`WorkspaceDiscovery`] also owns the directory capabilities every read goes
/// through, and those cannot be serialised. They are re-opened here — which
/// also re-checks that each root is still the directory it was — so a cached
/// result supports `read_file` exactly as a fresh one does. Without this the
/// cache would only serve commands that never read a file, which is the one
/// command for which scanning was already cheap.
pub fn rehydrate_cached_discovery(
    options: &WorkspaceDiscoveryOptions,
    cached: &CachedDiscovery,
) -> std::result::Result<WorkspaceDiscovery, WorkspaceError> {
    // Canonicalised and deduplicated exactly as the walk does, so a hit and a
    // miss resolve `read_file` against the same capability set.
    let mut canonical_roots = options
        .roots
        .iter()
        .map(|root| {
            fs::canonicalize(root).map_err(|source| WorkspaceError::Io {
                context: format!("failed to resolve workspace root {}", root.display()),
                source,
            })
        })
        .collect::<std::result::Result<Vec<_>, WorkspaceError>>()?;
    canonical_roots.sort();
    canonical_roots.dedup();
    let root_dirs = open_root_capabilities(&canonical_roots)?;
    Ok(WorkspaceDiscovery {
        files: cached.files.clone(),
        canonical_files: cached.canonical_files.iter().cloned().collect(),
        skipped_unknown_count: cached.skipped_count("unknown"),
        skipped_hidden_count: cached.skipped_count("hidden"),
        skipped_generated_count: cached.skipped_count("generated"),
        skipped_symlink_count: cached.skipped_count("symlink"),
        skipped_excluded_count: cached.skipped_count("excluded"),
        canonical_roots,
        root_dirs,
        visited_entry_count: cached.visited_entry_count,
        ..WorkspaceDiscovery::default()
    })
}

pub fn discover_workspace_files(
    options: &WorkspaceDiscoveryOptions,
) -> std::result::Result<WorkspaceDiscovery, WorkspaceError> {
    discover_workspace_files_with_limits(options, WorkspaceLimits::default())
}

/// Discovers over a root list that is itself an already-selected file set.
///
/// The ordinary root limit (1,024) bounds a hand-written command line, where
/// more than a few roots is a mistake. `--since`, `--paths-from` and
/// `--from-git` are not hand-written: they hand over the exact file list some
/// other tool produced, which for a large refactoring commit is thousands of
/// entries. Those lists are already bounded at their own source, and the file
/// limit still applies to the result, so the only bound this relaxes is the one
/// that was measuring the wrong thing.
pub fn discover_workspace_files_from_list(
    options: &WorkspaceDiscoveryOptions,
) -> std::result::Result<WorkspaceDiscovery, WorkspaceError> {
    let default = WorkspaceLimits::default();
    discover_workspace_files_with_limits(
        options,
        WorkspaceLimits {
            max_roots: default.max_files,
            ..default
        },
    )
}

/// Discovers with caller-supplied bounds.
///
/// Public since `--max-file-bytes` and its siblings: a CI container with a
/// 512 MB budget has to be able to say so, and the defaults are sized for a
/// workstation.
pub fn discover_workspace_files_with_limits(
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
    discovery.root_dirs = open_root_capabilities(&discovery.canonical_roots)?;

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
    let mut ignore_cache = IgnoreCache::new();
    for (root, scan_root) in &scan_roots {
        let mut context = TraversalContext::new(root, scan_root, &excludes.current_dir);
        context.prime_ignore_stack(options, &mut discovery, &mut ignore_cache)?;
        collect_workspace_files(
            root,
            scan_root,
            0,
            options,
            &excludes,
            &mut context,
            &mut ignore_cache,
            &mut discovery,
            &mut seen,
        )?;
    }

    discovery.files.sort();
    for files in discovery.repositories.values_mut() {
        files.sort();
    }
    discovery.files_outside_repositories.sort();
    // A file-list run primes the ignore stack once per root, so one `.gitignore`
    // is legitimately visited thousands of times. The report wants the set of
    // files that governed the result, not how often each was consulted.
    discovery.ignore_files_read.sort();
    discovery.ignore_files_read.dedup();
    Ok(discovery)
}

/// A directory's identity at the moment it was listed.
struct DirectoryStamp {
    path: PathBuf,
    modified: Option<std::time::SystemTime>,
}

/// The state a traversal carries down one scan root.
///
/// Everything here is per-root rather than per-run: two roots in different
/// checkouts get different ignore stacks and different repository answers, and
/// sharing either between them is how a monorepo run starts reporting one
/// project's rules against another's files.
struct TraversalContext {
    /// The absolute, lexically normalised scan root, for glob matching.
    root_absolute: PathBuf,
    current_dir: PathBuf,
    ignore: IgnoreStack,
    /// Canonical directories on the current ancestor chain, for symlink loops.
    ancestors: Vec<PathBuf>,
    /// The repository containing the directory currently being walked.
    repository: Option<PathBuf>,
}

impl TraversalContext {
    fn new(root: &Path, scan_root: &ScanRoot, current_dir: &Path) -> Self {
        let _ = root;
        Self {
            root_absolute: scan_root.lexical.clone(),
            current_dir: current_dir.to_path_buf(),
            ignore: IgnoreStack::new(),
            ancestors: Vec::new(),
            repository: None,
        }
    }

    fn absolute(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            lexically_normalize(path)
        } else {
            lexically_normalize(&self.current_dir.join(path))
        }
    }

    /// Loads the ignore files between the enclosing repository root and the
    /// scan root.
    ///
    /// Without this, `paredit inspect lint src/` inside a repository would
    /// ignore the repository's own top-level `.gitignore`, and the same command
    /// would select a different file set depending on which directory it was
    /// pointed at — the kind of inconsistency that gets blamed on the analysis
    /// rather than on the traversal.
    fn prime_ignore_stack(
        &mut self,
        options: &WorkspaceDiscoveryOptions,
        discovery: &mut WorkspaceDiscovery,
        cache: &mut IgnoreCache,
    ) -> std::result::Result<(), WorkspaceError> {
        let start = if self.root_absolute.is_dir() {
            self.root_absolute.clone()
        } else {
            self.root_absolute
                .parent()
                .map_or_else(|| self.root_absolute.clone(), Path::to_path_buf)
        };
        self.repository = find_repository_root(&start).map(|root| root.path);

        if !options.ignore.is_enabled() {
            return Ok(());
        }

        let Some(repository) = self.repository.clone() else {
            return Ok(());
        };
        // Ancestors from the repository root down to, but not including, the
        // directory the traversal itself will enter first.
        let mut chain = Vec::new();
        let mut current = start.parent();
        while let Some(directory) = current {
            // Stop at the repository, and never step above it: a `.gitignore`
            // in a parent directory that happens to sit above the checkout is
            // not this project's, and git would not read it either.
            if !directory.starts_with(&repository) {
                break;
            }
            chain.push(directory.to_path_buf());
            if directory == repository {
                break;
            }
            current = directory.parent();
        }
        chain.reverse();

        for directory in chain {
            let is_root = directory == repository;
            let before = self.ignore.depth();
            self.ignore
                .enter_directory(&directory, options.ignore, is_root, cache)?;
            record_ignore_files(&self.ignore, before, discovery);
        }
        Ok(())
    }
}

fn record_ignore_files(stack: &IgnoreStack, before: usize, discovery: &mut WorkspaceDiscovery) {
    for layer in stack.layers().iter().skip(before) {
        if let Some(origin) = layer.origin() {
            discovery.ignore_files_read.push(origin.to_path_buf());
        }
    }
}

/// Renders `path` relative to the scan root as `/`-separated text.
///
/// Returns `None` for a path outside the root or with a non-UTF-8 component:
/// in both cases there is no text a pattern could have been written against, so
/// treating it as unmatched keeps the file rather than dropping it invisibly.
fn relative_to_root(root: &Path, absolute: &Path) -> Option<String> {
    let relative = absolute.strip_prefix(root).ok()?;
    let mut rendered = String::new();
    for component in relative.components() {
        let text = component.as_os_str().to_str()?;
        if !rendered.is_empty() {
            rendered.push('/');
        }
        rendered.push_str(text);
    }
    (!rendered.is_empty()).then_some(rendered)
}

/// Whether the command-line globs reject `absolute`.
///
/// `--exclude-glob` always wins: a caller that writes both is narrowing, and
/// letting an `--include` re-admit an explicitly excluded path would make the
/// pair order-dependent.
fn rejected_by_globs(
    options: &WorkspaceDiscoveryOptions,
    context: &TraversalContext,
    absolute: &Path,
    is_directory: bool,
) -> bool {
    let Some(relative) = relative_to_root(&context.root_absolute, absolute) else {
        return false;
    };
    if options.exclude_globs.is_match(&relative, is_directory) {
        return true;
    }
    if options.include_globs.is_empty() {
        return false;
    }
    if is_directory {
        // A directory is kept whenever anything below it could still match;
        // `--include '**/*.lisp'` must not prune the directories that hold the
        // files it is asking for.
        return !options.include_globs.is_match(&relative, true)
            && !options.include_globs.could_match_descendant(&relative);
    }
    !options.include_globs.is_match(&relative, false)
}

#[expect(
    clippy::too_many_arguments,
    reason = "one traversal frame: path, root, depth, and the four collaborators it threads"
)]
fn collect_workspace_files(
    path: &Path,
    scan_root: &ScanRoot,
    depth: usize,
    options: &WorkspaceDiscoveryOptions,
    excludes: &ExcludeIndex,
    context: &mut TraversalContext,
    ignore_cache: &mut IgnoreCache,
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

    let mut followed_canonical = None;
    let metadata = if metadata.file_type().is_symlink() {
        if !options.symlinks.follows() {
            discovery.skipped_symlink_count += 1;
            return Ok(());
        }
        let Ok(canonical) = fs::canonicalize(path) else {
            // A broken link resolves to nothing; there is no target to read.
            discovery.skipped_symlink_count += 1;
            return Ok(());
        };
        if !discovery
            .canonical_roots
            .iter()
            .any(|root| canonical.starts_with(root))
        {
            discovery.skipped_symlink_escaped_count += 1;
            return Ok(());
        }
        if context.ancestors.contains(&canonical) {
            discovery.skipped_symlink_cycle_count += 1;
            return Ok(());
        }
        followed_canonical = Some(canonical);
        fs::metadata(path).map_err(|source| WorkspaceError::Io {
            context: format!("failed to inspect the target of {}", path.display()),
            source,
        })?
    } else {
        metadata
    };

    if metadata.is_dir() {
        let modified = metadata.modified().ok();
        collect_workspace_directory(
            path,
            scan_root,
            depth,
            options,
            excludes,
            context,
            ignore_cache,
            discovery,
            seen,
            followed_canonical,
            modified,
        )?;
        return Ok(());
    }

    if metadata.is_file() {
        // A followed symlink is recorded by its target: every later read goes
        // through a root capability and refuses a non-regular file, which a
        // link path is.
        let selected = followed_canonical.as_deref().unwrap_or(path);
        collect_workspace_file(selected, path, depth, options, context, discovery, seen)?;
    }

    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "one traversal frame: path, root, depth, and the collaborators it threads"
)]
fn collect_workspace_directory(
    path: &Path,
    scan_root: &ScanRoot,
    depth: usize,
    options: &WorkspaceDiscoveryOptions,
    excludes: &ExcludeIndex,
    context: &mut TraversalContext,
    ignore_cache: &mut IgnoreCache,
    discovery: &mut WorkspaceDiscovery,
    seen: &mut BTreeSet<PathBuf>,
    followed_canonical: Option<PathBuf>,
    modified: Option<std::time::SystemTime>,
) -> std::result::Result<(), WorkspaceError> {
    if !options.include_hidden && is_hidden_workspace_path(path) {
        discovery.skipped_hidden_count += 1;
        return Ok(());
    }

    if !options.include_generated && is_generated_workspace_path(path) {
        discovery.skipped_generated_count += 1;
        return Ok(());
    }

    let absolute = context.absolute(path);
    // Depth zero is the root the caller named. Git does not apply ignore rules
    // to a path given explicitly on the command line, and neither does this:
    // being told to scan a directory is a stronger signal than a pattern that
    // happens to cover it.
    if depth > 0 {
        if context.ignore.is_ignored(&absolute, true) {
            discovery.skipped_ignored_count += 1;
            return Ok(());
        }
        if rejected_by_globs(options, context, &absolute, true) {
            discovery.skipped_glob_count += 1;
            return Ok(());
        }
    }

    if options
        .max_depth
        .is_some_and(|max_depth| depth >= max_depth)
    {
        return Ok(());
    }

    let repository_boundary = is_repository_root(&absolute);
    let previous_repository = context.repository.clone();
    if repository_boundary {
        context.repository = Some(absolute.clone());
    }
    let layers_before = context.ignore.depth();
    let ignore_scope = context.ignore.enter_directory(
        &absolute,
        options.ignore,
        repository_boundary,
        ignore_cache,
    )?;
    record_ignore_files(&context.ignore, layers_before, discovery);
    // Loop detection compares canonical paths, so the entry pushed here has to
    // be canonical too. A lexical path never equals the canonicalised target of
    // a link — on a platform where `/tmp` is itself a symlink, every comparison
    // silently fails and the walk recurses until it runs out of stack.
    if options.symlinks.follows() {
        let canonical = match followed_canonical {
            Some(canonical) => Some(canonical),
            None => fs::canonicalize(path).ok(),
        };
        context
            .ancestors
            .push(canonical.unwrap_or_else(|| absolute.clone()));
    }

    let result = walk_directory_entries(
        path,
        scan_root,
        depth,
        options,
        excludes,
        context,
        ignore_cache,
        discovery,
        seen,
        DirectoryStamp {
            path: absolute.clone(),
            modified,
        },
    );

    if options.symlinks.follows() {
        context.ancestors.pop();
    }
    context.ignore.restore(ignore_scope);
    context.repository = previous_repository;
    result
}

#[expect(
    clippy::too_many_arguments,
    reason = "one traversal frame: path, root, depth, and the collaborators it threads"
)]
fn walk_directory_entries(
    path: &Path,
    scan_root: &ScanRoot,
    depth: usize,
    options: &WorkspaceDiscoveryOptions,
    excludes: &ExcludeIndex,
    context: &mut TraversalContext,
    ignore_cache: &mut IgnoreCache,
    discovery: &mut WorkspaceDiscovery,
    seen: &mut BTreeSet<PathBuf>,
    stamp: DirectoryStamp,
) -> std::result::Result<(), WorkspaceError> {
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
    // Recorded from metadata the traversal already read, so a run without a
    // cache pays nothing for it. A directory's mtime plus its entry count is
    // what tells a later run whether anything was added, removed or renamed
    // here — a file's *contents* changing does not affect which files exist.
    discovery.directory_stamps.push(DirectoryFingerprint {
        path: stamp.path,
        modified_nanos: stamp
            .modified
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_nanos()),
        entry_count: entries.len(),
    });

    for entry in entries {
        collect_workspace_files(
            &entry,
            scan_root,
            depth + 1,
            options,
            excludes,
            context,
            ignore_cache,
            discovery,
            seen,
        )?;
    }

    Ok(())
}

fn collect_workspace_file(
    path: &Path,
    traversal_path: &Path,
    depth: usize,
    options: &WorkspaceDiscoveryOptions,
    context: &mut TraversalContext,
    discovery: &mut WorkspaceDiscovery,
    seen: &mut BTreeSet<PathBuf>,
) -> std::result::Result<(), WorkspaceError> {
    if !options.include_hidden && is_hidden_workspace_path(path) {
        discovery.skipped_hidden_count += 1;
        return Ok(());
    }

    let absolute = context.absolute(traversal_path);
    if depth > 0 {
        if context.ignore.is_ignored(&absolute, false) {
            discovery.skipped_ignored_count += 1;
            return Ok(());
        }
        if rejected_by_globs(options, context, &absolute, false) {
            discovery.skipped_glob_count += 1;
            return Ok(());
        }
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
        match context.repository.clone() {
            Some(repository) => discovery
                .repositories
                .entry(repository)
                .or_default()
                .push(path.to_path_buf()),
            None => discovery
                .files_outside_repositories
                .push(path.to_path_buf()),
        }
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
