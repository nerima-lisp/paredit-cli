//! The shared flag set for every command that takes a workspace.
//!
//! Before this module, each workspace command declared `--include-unknown`,
//! `--include-hidden`, `--include-generated` and `--max-depth` for itself, in
//! its own `Args` struct, with its own doc comments. Five copies of four flags
//! is survivable; five copies of *fourteen* is not, and the fifth copy is where
//! the default quietly differs from the other four.
//!
//! So the flags live here once and are `#[command(flatten)]`ed in. That also
//! makes the answer to "which input sources does this command support" a fact
//! about one type rather than a survey.
//!
//! The flags fall into two groups that behave differently:
//!
//! * **Selectors** — `--since`, `--paths-from`, `--from-git`, `--from-manifest`
//!   — replace the directory walk with a list somebody else computed.
//! * **Filters** — `--include`, `--exclude-glob`, `--no-gitignore`,
//!   `--follow-symlinks`, and the older include flags — narrow whatever the
//!   selector produced.
//!
//! Filters always apply, whichever selector ran. That is what makes
//! `--since main --include '**/*.lisp'` mean what it looks like.

use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};

use paredit_core_workspace::workspace::{
    GlobSet, IgnoreOptions, ManifestKind, ManifestSource, PathListSeparator, SinceOptions,
    SymlinkPolicy, WorkspaceDiscoveryOptions, changed_paths_since,
    elisp_package_manifests_in_directory, extract_tar_path, find_repository_root,
    manifests_in_directory, parse_manifest, parse_manifest_as, read_path_list,
    read_path_list_from_stdin, tracked_paths,
};

use crate::error::{ArgumentError, CliError, CliResult};

/// How entries in a `--paths-from` list are separated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum PathListSeparatorArg {
    /// One path per line, whitespace-trimmed.
    #[default]
    Newline,
    /// NUL-separated, as `git ls-files -z` and `find -print0` produce.
    Nul,
}

impl From<PathListSeparatorArg> for PathListSeparator {
    fn from(value: PathListSeparatorArg) -> Self {
        match value {
            PathListSeparatorArg::Newline => Self::Newline,
            PathListSeparatorArg::Nul => Self::Nul,
        }
    }
}

/// Every input flag a workspace command accepts.
#[derive(Debug, Args)]
pub struct WorkspaceInputArgs {
    /// Include files whose extension does not identify a known Lisp dialect.
    #[arg(long)]
    pub include_unknown: bool,
    /// Include hidden directories and files.
    #[arg(long)]
    pub include_hidden: bool,
    /// Include generated or dependency directories such as target and node_modules.
    #[arg(long)]
    pub include_generated: bool,
    /// Maximum directory recursion depth from each root directory.
    #[arg(long)]
    pub max_depth: Option<usize>,
    /// Skip this exact path and everything under it. Repeatable.
    #[arg(long, value_name = "PATH")]
    pub exclude: Vec<PathBuf>,
    /// Keep only paths matching this gitignore-style pattern. Repeatable.
    #[arg(long, value_name = "GLOB")]
    pub include: Vec<String>,
    /// Skip paths matching this gitignore-style pattern, overriding --include. Repeatable.
    #[arg(long = "exclude-glob", value_name = "GLOB")]
    pub exclude_glob: Vec<String>,
    /// Ignore both .gitignore and .pareditignore files.
    #[arg(long)]
    pub no_ignore: bool,
    /// Ignore .gitignore files and .git/info/exclude.
    #[arg(long)]
    pub no_gitignore: bool,
    /// Ignore .pareditignore files.
    #[arg(long)]
    pub no_pareditignore: bool,
    /// Traverse symlinks whose target resolves inside the scanned roots.
    #[arg(long)]
    pub follow_symlinks: bool,
    /// Analyse only files that differ from this git ref.
    #[arg(long, value_name = "GIT_REF")]
    pub since: Option<String>,
    /// With --since, leave out files git does not track yet.
    #[arg(long)]
    pub since_skip_untracked: bool,
    /// Analyse only the files git tracks, instead of walking the directory.
    #[arg(long)]
    pub from_git: bool,
    /// Take the file set from project manifests (.asd, deps.edn, project.clj).
    #[arg(long)]
    pub from_manifest: bool,
    /// Read the file list from this file, or from stdin when given "-".
    #[arg(long, value_name = "FILE")]
    pub paths_from: Option<PathBuf>,
    /// Analyse an uncompressed tar archive, or stdin when given "-".
    ///
    /// Compression and transport are the shell's: pipe `gzip -dc` or `curl`
    /// into this rather than teaching the tool to speak HTTP.
    #[arg(long, value_name = "ARCHIVE", requires = "extract_to")]
    pub from_archive: Option<PathBuf>,
    /// Where --from-archive unpacks. Existing files are never overwritten.
    #[arg(long, value_name = "DIR")]
    pub extract_to: Option<PathBuf>,
    /// How --paths-from separates entries.
    #[arg(long, value_enum, default_value_t = PathListSeparatorArg::Newline)]
    pub paths_from_separator: PathListSeparatorArg,
}

impl Default for WorkspaceInputArgs {
    fn default() -> Self {
        Self {
            include_unknown: false,
            include_hidden: false,
            include_generated: false,
            max_depth: None,
            exclude: Vec::new(),
            include: Vec::new(),
            exclude_glob: Vec::new(),
            no_ignore: false,
            no_gitignore: false,
            no_pareditignore: false,
            follow_symlinks: false,
            since: None,
            since_skip_untracked: false,
            from_git: false,
            from_manifest: false,
            paths_from: None,
            from_archive: None,
            extract_to: None,
            paths_from_separator: PathListSeparatorArg::Newline,
        }
    }
}

/// Which selector produced the file set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InputSelector {
    /// The roots were walked.
    #[default]
    Walk,
    /// `--since <ref>`.
    GitSince,
    /// `--from-git`.
    GitTracked,
    /// `--from-manifest`.
    Manifest,
    /// `--paths-from`.
    PathList,
    /// `--from-archive`.
    Archive,
}

impl InputSelector {
    /// A stable identifier for JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Walk => "walk",
            Self::GitSince => "git-since",
            Self::GitTracked => "git-tracked",
            Self::Manifest => "manifest",
            Self::PathList => "path-list",
            Self::Archive => "archive",
        }
    }
}

/// The input set a command should analyse, and how it was arrived at.
#[derive(Debug)]
pub struct ResolvedWorkspaceInput {
    /// Options to pass to discovery. `roots` already reflects the selector.
    pub options: WorkspaceDiscoveryOptions,
    /// Which selector ran.
    pub selector: InputSelector,
    /// Whether `roots` is a pre-selected file list rather than the user's roots.
    pub from_list: bool,
    /// The repositories the roots resolved to, deduplicated.
    pub repositories: Vec<PathBuf>,
    /// Manifests read by `--from-manifest`, for reporting.
    pub manifests: Vec<ManifestSource>,
    /// How many paths the selector produced before filtering.
    pub selected_path_count: usize,
}

impl WorkspaceInputArgs {
    /// The ignore-file policy these flags describe.
    #[must_use]
    pub fn ignore_options(&self) -> IgnoreOptions {
        let environment = IgnoreOptions::from_environment();
        IgnoreOptions {
            respect_gitignore: environment.respect_gitignore
                && !(self.no_ignore || self.no_gitignore),
            respect_pareditignore: environment.respect_pareditignore
                && !(self.no_ignore || self.no_pareditignore),
        }
    }

    /// The symlink policy these flags describe.
    #[must_use]
    pub const fn symlink_policy(&self) -> SymlinkPolicy {
        if self.follow_symlinks {
            SymlinkPolicy::Follow
        } else {
            SymlinkPolicy::Skip
        }
    }

    /// Compiles the `--include` patterns.
    pub fn include_globs(&self) -> CliResult<GlobSet> {
        compile_globs(&self.include, "--include")
    }

    /// Compiles the `--exclude-glob` patterns.
    pub fn exclude_globs(&self) -> CliResult<GlobSet> {
        compile_globs(&self.exclude_glob, "--exclude-glob")
    }

    /// Turns the flags plus `roots` into everything discovery needs.
    ///
    /// At most one selector may be given. Combining `--since` with
    /// `--paths-from` has no defensible meaning — one of the two lists would
    /// silently win — so it is rejected rather than resolved by precedence.
    pub fn resolve(&self, roots: &[PathBuf]) -> CliResult<ResolvedWorkspaceInput> {
        let selectors = usize::from(self.since.is_some())
            + usize::from(self.from_git)
            + usize::from(self.from_manifest)
            + usize::from(self.from_archive.is_some())
            + usize::from(self.paths_from.is_some());
        if selectors > 1 {
            return Err(ArgumentError::ConflictingInputSelectors.into());
        }

        let base = WorkspaceDiscoveryOptions {
            roots: roots.to_vec(),
            include_unknown: self.include_unknown,
            include_hidden: self.include_hidden,
            include_generated: self.include_generated,
            max_depth: self.max_depth,
            exclude: self.exclude.clone(),
            ignore: self.ignore_options(),
            include_globs: self.include_globs()?,
            exclude_globs: self.exclude_globs()?,
            symlinks: self.symlink_policy(),
        };

        let repositories = repositories_for(roots);
        let mut resolved = ResolvedWorkspaceInput {
            options: base,
            selector: InputSelector::Walk,
            from_list: false,
            repositories,
            manifests: Vec::new(),
            selected_path_count: 0,
        };

        if let Some(archive) = self.from_archive.as_deref() {
            let destination = self
                .extract_to
                .as_deref()
                .ok_or(ArgumentError::ArchiveRequiresDestination)?;
            let extracted = extract_tar_path(archive, destination)?;
            // The archive's own files are the input set, not whatever else the
            // destination happens to hold. Narrowing to the roots on top of
            // that lets `--extract-to /tmp/p /tmp/p/src` mean what it looks
            // like.
            let paths = restrict_to_roots(extracted.files, roots);
            if paths.is_empty() {
                return Err(ArgumentError::EmptyArchive.into());
            }
            resolved.selector = InputSelector::Archive;
            resolved.selected_path_count = paths.len();
            resolved.options.roots = paths;
            resolved.from_list = true;
            return Ok(resolved);
        }

        if let Some(reference) = self.since.as_deref() {
            let paths = self.changed_paths(&resolved.repositories, reference, roots)?;
            resolved.selector = InputSelector::GitSince;
            resolved.selected_path_count = paths.len();
            resolved.options.roots = paths;
            resolved.from_list = true;
            return Ok(resolved);
        }

        if self.from_git {
            let mut paths = Vec::new();
            for repository in &resolved.repositories {
                paths.extend(tracked_paths(repository)?);
            }
            let paths = restrict_to_roots(paths, roots);
            resolved.selector = InputSelector::GitTracked;
            resolved.selected_path_count = paths.len();
            resolved.options.roots = paths;
            resolved.from_list = true;
            return Ok(resolved);
        }

        if self.from_manifest {
            let (manifests, paths) = manifest_roots(roots)?;
            if paths.is_empty() {
                return Err(ArgumentError::NoManifestFound.into());
            }
            resolved.selector = InputSelector::Manifest;
            resolved.selected_path_count = paths.len();
            resolved.manifests = manifests;
            resolved.options.roots = paths;
            resolved.from_list = true;
            return Ok(resolved);
        }

        if let Some(list) = self.paths_from.as_deref() {
            let separator = PathListSeparator::from(self.paths_from_separator);
            let paths = if list == std::path::Path::new("-") {
                read_path_list_from_stdin(separator)?
            } else {
                let file = std::fs::File::open(list).map_err(|source| CliError::Io {
                    context: format!("failed to open {}", list.display()),
                    source,
                })?;
                read_path_list(file, separator)?
            };
            // A path list that names something outside the roots is a mistake
            // worth catching quietly rather than a licence to read it: the
            // roots are what this run was authorised for.
            let paths = restrict_to_roots(paths, roots);
            if paths.is_empty() {
                return Err(ArgumentError::EmptyPathList.into());
            }
            resolved.selector = InputSelector::PathList;
            resolved.selected_path_count = paths.len();
            resolved.options.roots = paths;
            resolved.from_list = true;
            return Ok(resolved);
        }

        Ok(resolved)
    }

    fn changed_paths(
        &self,
        repositories: &[PathBuf],
        reference: &str,
        roots: &[PathBuf],
    ) -> CliResult<Vec<PathBuf>> {
        if repositories.is_empty() {
            return Err(ArgumentError::SinceRequiresRepository.into());
        }
        let options = SinceOptions {
            include_untracked: !self.since_skip_untracked,
        };
        let mut paths = Vec::new();
        for repository in repositories {
            paths.extend(changed_paths_since(repository, reference, options)?);
        }
        Ok(restrict_to_roots(paths, roots))
    }
}

fn compile_globs(patterns: &[String], flag: &'static str) -> CliResult<GlobSet> {
    let mut set = GlobSet::new();
    for pattern in patterns {
        set.push_line(pattern).map_err(|error| {
            CliError::from(ArgumentError::InvalidGlob {
                flag,
                pattern: pattern.clone(),
                reason: error.to_string(),
            })
        })?;
    }
    Ok(set)
}

/// The repositories containing `roots`, deduplicated and in a stable order.
fn repositories_for(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut repositories = roots
        .iter()
        .filter_map(|root| find_repository_root(root).map(|found| found.path))
        .collect::<Vec<_>>();
    repositories.sort();
    repositories.dedup();
    repositories
}

/// Drops paths that lie outside every root, and paths that no longer exist.
///
/// Both cases are ordinary rather than exceptional: `--since` against a wide
/// ref names files all over the repository when the command was pointed at one
/// subdirectory, and a rename reports a destination that a later commit
/// removed again.
fn restrict_to_roots(paths: Vec<PathBuf>, roots: &[PathBuf]) -> Vec<PathBuf> {
    let canonical_roots = roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .collect::<Vec<_>>();
    let mut kept = paths
        .into_iter()
        .filter_map(|path| {
            let canonical = std::fs::canonicalize(&path).ok()?;
            if !canonical.is_file() {
                return None;
            }
            canonical_roots
                .iter()
                .any(|root| canonical.starts_with(root))
                .then_some(path)
        })
        .collect::<Vec<_>>();
    kept.sort();
    kept.dedup();
    kept
}

/// Reads every manifest directly under `roots` and returns what they name.
fn manifest_roots(roots: &[PathBuf]) -> CliResult<(Vec<ManifestSource>, Vec<PathBuf>)> {
    let mut manifests = Vec::new();
    let mut paths = Vec::new();
    for root in roots {
        let directory = if root.is_dir() {
            root.clone()
        } else {
            // A manifest named directly is the most precise form of the flag.
            if let Some(kind) = ManifestKind::from_file_name(root) {
                if let Some(source) = parse_manifest_as(root, kind)? {
                    paths.extend(source.roots());
                    manifests.push(source);
                }
                continue;
            }
            root.parent()
                .map_or_else(|| root.clone(), Path::to_path_buf)
        };
        let mut found_named_manifest = false;
        for manifest in manifests_in_directory(&directory)? {
            if let Some(source) = parse_manifest(&manifest)? {
                found_named_manifest = true;
                paths.extend(source.roots());
                manifests.push(source);
            }
        }
        // An Emacs Lisp package declares itself in a header inside one of its
        // own sources, so there is no file name to look for. Reading every
        // top-level `.el` is only worth it when nothing else answered.
        if !found_named_manifest {
            for source in elisp_package_manifests_in_directory(&directory)? {
                paths.extend(source.roots());
                manifests.push(source);
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok((manifests, paths))
}
