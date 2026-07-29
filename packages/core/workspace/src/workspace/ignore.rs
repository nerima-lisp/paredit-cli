//! Per-directory ignore files, stacked the way git stacks them.
//!
//! Before this existed, workspace discovery skipped generated directories from
//! a hard-coded list (`target`, `node_modules`, …) and nothing else. Every
//! project that keeps vendored sources, fixtures or generated Lisp outside
//! those names had to repeat itself with `--exclude` on every invocation, even
//! though the answer was already written down in `.gitignore`.
//!
//! Three files feed the stack, in increasing precedence within one directory:
//!
//! 1. `.git/info/exclude`, only at a repository root;
//! 2. `.gitignore`;
//! 3. `.pareditignore`, so a project can exclude something from analysis
//!    without excluding it from version control — generated Lisp that is
//!    deliberately committed is the case that needs this.
//!
//! Precedence across directories is the deeper file wins, and within one file
//! the last matching pattern wins. A repository boundary cuts the stack: a
//! `.gitignore` in an outer checkout does not govern a nested repository, the
//! same way git does not read across that line.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::error::{WorkspaceError, WorkspaceLimit};
use super::glob::{GlobDecision, GlobSet};
use super::vcs::git_info_exclude_files;

/// The largest ignore file that will be read.
///
/// An ignore file comes from inside the tree being scanned, so it is untrusted
/// input like any source file, and it is parsed eagerly on entering a
/// directory. A megabyte is four orders of magnitude above every real one.
const MAX_IGNORE_FILE_BYTES: u64 = 1024 * 1024;

/// The ignore file names this tool reads, in increasing precedence.
pub const GITIGNORE_FILE_NAME: &str = ".gitignore";
/// The tool-specific ignore file, which outranks `.gitignore` at the same level.
pub const PAREDITIGNORE_FILE_NAME: &str = ".pareditignore";

/// Which ignore files discovery should consult.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IgnoreOptions {
    /// Read `.gitignore` files and `.git/info/exclude`.
    pub respect_gitignore: bool,
    /// Read `.pareditignore` files.
    pub respect_pareditignore: bool,
}

impl Default for IgnoreOptions {
    /// Both on.
    ///
    /// A tool that walks a source tree and reports on what it finds is far more
    /// often wrong for having read a build artifact than for having skipped a
    /// file the project told git to forget about. Callers that want the old
    /// behaviour turn both off explicitly.
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            respect_pareditignore: true,
        }
    }
}

impl IgnoreOptions {
    /// The default, narrowed by the `PAREDIT_NO_*_IGNORE` environment variables.
    ///
    /// Most commands take explicit paths rather than roots and so carry no
    /// `--no-ignore` flag, yet a directory argument still expands through a
    /// walk that honours ignore files. Without an escape hatch those commands
    /// would have no way at all to look at a generated file, which is a worse
    /// trap than the one respecting `.gitignore` avoids. The variable is also
    /// the right shape for CI, where the adjustment is per-run rather than
    /// per-invocation.
    ///
    /// `PAREDIT_NO_IGNORE` disables both files, `PAREDIT_NO_GITIGNORE` and
    /// `PAREDIT_NO_PAREDITIGNORE` one each. A variable set to `0`, `false` or
    /// the empty string is treated as unset, so a CI system that exports every
    /// variable it knows about cannot switch this on by accident.
    #[must_use]
    pub fn from_environment() -> Self {
        let all = environment_flag("PAREDIT_NO_IGNORE");
        Self {
            respect_gitignore: !(all || environment_flag("PAREDIT_NO_GITIGNORE")),
            respect_pareditignore: !(all || environment_flag("PAREDIT_NO_PAREDITIGNORE")),
        }
    }

    /// Neither file is read.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            respect_gitignore: false,
            respect_pareditignore: false,
        }
    }

    /// Whether any ignore file would be read at all.
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        self.respect_gitignore || self.respect_pareditignore
    }
}

fn environment_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| {
        let value = value.to_string_lossy().to_ascii_lowercase();
        !matches!(value.trim(), "" | "0" | "false" | "no" | "off")
    })
}

/// Where one layer's patterns came from, for reporting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoreSource {
    /// `.git/info/exclude` at a repository root.
    GitInfoExclude,
    /// A `.gitignore` file.
    GitIgnore,
    /// A `.pareditignore` file.
    PareditIgnore,
    /// Patterns supplied on the command line.
    CommandLine,
}

impl IgnoreSource {
    /// A stable identifier for JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GitInfoExclude => "git-info-exclude",
            Self::GitIgnore => "gitignore",
            Self::PareditIgnore => "pareditignore",
            Self::CommandLine => "command-line",
        }
    }
}

/// One ignore file's patterns, together with the directory they resolve against.
#[derive(Clone, Debug)]
pub struct IgnoreLayer {
    base: PathBuf,
    origin: Option<PathBuf>,
    source: IgnoreSource,
    set: GlobSet,
    /// Set on the layer that sits at a repository root, so entering a nested
    /// repository can cut everything above it.
    repository_root: bool,
}

impl IgnoreLayer {
    /// Builds a layer from patterns given on the command line.
    ///
    /// `base` is the root the patterns are relative to, which makes
    /// `--exclude-glob 'src/*.lisp'` mean the same thing as writing that line
    /// in a `.gitignore` at the root.
    #[must_use]
    pub const fn from_command_line(base: PathBuf, set: GlobSet) -> Self {
        Self {
            base,
            origin: None,
            source: IgnoreSource::CommandLine,
            set,
            repository_root: false,
        }
    }

    /// The directory this layer's patterns resolve against.
    #[must_use]
    pub fn base(&self) -> &Path {
        &self.base
    }

    /// The file the patterns were read from, if any.
    #[must_use]
    pub fn origin(&self) -> Option<&Path> {
        self.origin.as_deref()
    }

    /// Which kind of ignore file this layer came from.
    #[must_use]
    pub const fn source(&self) -> IgnoreSource {
        self.source
    }

    /// How many patterns the layer holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// Whether the layer holds no patterns.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

/// The ignore layers in scope at one point in a traversal.
///
/// Layers are pushed on entering a directory and dropped on leaving it, so the
/// stack is always ordered outermost first.
///
/// A repository boundary does *not* drop the layers above it. It moves
/// `barrier`, the index lookups start from, and the walk restores the old value
/// on the way back out. Truncating instead is the obvious implementation and
/// the wrong one: a depth-first walk that meets a nested checkout before its
/// parent's own files would delete the parent's rules permanently, and every
/// sibling visited afterwards would be scanned unfiltered.
#[derive(Clone, Debug, Default)]
pub struct IgnoreStack {
    layers: Vec<IgnoreLayer>,
    barrier: usize,
}

/// Ignore files already parsed during one discovery run.
///
/// A walk enters each directory once, so a cache buys nothing there. A
/// `--since` run does not walk: it hands discovery a list of several thousand
/// files, each of which is its own root, and each root primes the stack from
/// its repository down. Without this, one commit's worth of changed files
/// re-reads and re-compiles the same handful of `.gitignore` files thousands of
/// times.
#[derive(Debug, Default)]
pub struct IgnoreCache {
    directories: HashMap<PathBuf, Vec<IgnoreLayer>>,
}

impl IgnoreCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many directories have been parsed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.directories.len()
    }

    /// Whether nothing has been parsed yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.directories.is_empty()
    }
}

/// What [`IgnoreStack::enter_directory`] must be given back on the way out.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IgnoreScope {
    depth: usize,
    barrier: usize,
}

/// Why a path was ignored, for the skip accounting discovery reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IgnoreMatch {
    /// The file the deciding pattern came from, if it came from a file.
    pub origin: Option<PathBuf>,
    /// Which kind of ignore file decided.
    pub source: IgnoreSource,
}

impl IgnoreStack {
    /// An empty stack, which ignores nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            layers: Vec::new(),
            barrier: 0,
        }
    }

    /// Appends a layer.
    pub fn push(&mut self, layer: IgnoreLayer) {
        self.layers.push(layer);
    }

    /// Undoes everything [`Self::enter_directory`] did for one directory.
    pub fn restore(&mut self, scope: IgnoreScope) {
        self.layers.truncate(scope.depth);
        self.barrier = scope.barrier;
    }

    /// How many layers are in scope.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.layers.len()
    }

    /// Whether no layer holds any pattern.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.iter().all(IgnoreLayer::is_empty)
    }

    /// The layers in scope, outermost first.
    #[must_use]
    pub fn layers(&self) -> &[IgnoreLayer] {
        &self.layers
    }

    /// Reads the ignore files that `directory` contributes and pushes them.
    ///
    /// Returns the stack depth before the push, which the caller restores with
    /// [`Self::truncate`] on the way back out.
    ///
    /// `is_repository_root` makes this directory cut the stack for gitignore
    /// purposes: a nested checkout is governed by its own files only. That is
    /// what git does, and it is what makes a monorepo of independent
    /// checkouts — the F9 case — behave the way each checkout expects.
    pub fn enter_directory(
        &mut self,
        directory: &Path,
        options: IgnoreOptions,
        is_repository_root: bool,
        cache: &mut IgnoreCache,
    ) -> Result<IgnoreScope, WorkspaceError> {
        let scope = IgnoreScope {
            depth: self.layers.len(),
            barrier: self.barrier,
        };
        if is_repository_root && options.respect_gitignore {
            // Everything read from a file above this point stops applying.
            // Command-line layers are not a repository's business and stay in
            // scope, which is why the lookup keeps them regardless of barrier.
            self.barrier = self.layers.len();
        }

        if !options.is_enabled() {
            return Ok(scope);
        }

        if let Some(cached) = cache.directories.get(directory) {
            self.layers.extend(cached.iter().cloned());
            return Ok(scope);
        }

        let mut parsed = Vec::new();
        if options.respect_gitignore {
            if is_repository_root {
                for exclude in git_info_exclude_files(directory) {
                    push_file(
                        &mut parsed,
                        directory,
                        &exclude,
                        IgnoreSource::GitInfoExclude,
                        true,
                    )?;
                }
            }
            let gitignore = directory.join(GITIGNORE_FILE_NAME);
            push_file(
                &mut parsed,
                directory,
                &gitignore,
                IgnoreSource::GitIgnore,
                is_repository_root,
            )?;
        }
        if options.respect_pareditignore {
            let pareditignore = directory.join(PAREDITIGNORE_FILE_NAME);
            push_file(
                &mut parsed,
                directory,
                &pareditignore,
                IgnoreSource::PareditIgnore,
                is_repository_root,
            )?;
        }

        self.layers.extend(parsed.iter().cloned());
        cache.directories.insert(directory.to_path_buf(), parsed);
        Ok(scope)
    }

    /// Decides whether `path` is ignored.
    ///
    /// Deeper layers speak last, so a `!keep.lisp` in a subdirectory overrides
    /// a `*.lisp` written at the root — which is the whole point of the
    /// per-directory design and the thing a single flat pattern list cannot do.
    #[must_use]
    pub fn decide(&self, path: &Path, is_directory: bool) -> Option<IgnoreMatch> {
        let mut decided = None;
        for (index, layer) in self.layers.iter().enumerate() {
            if index < self.barrier && layer.source != IgnoreSource::CommandLine {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&layer.base) else {
                continue;
            };
            let Some(relative) = relative_to_slash(relative) else {
                continue;
            };
            match layer.set.decide(&relative, is_directory) {
                GlobDecision::Unmatched => {}
                GlobDecision::Matched => {
                    decided = Some(IgnoreMatch {
                        origin: layer.origin.clone(),
                        source: layer.source,
                    });
                }
                GlobDecision::Negated => decided = None,
            }
        }
        decided
    }

    /// Whether `path` is ignored.
    #[must_use]
    pub fn is_ignored(&self, path: &Path, is_directory: bool) -> bool {
        self.decide(path, is_directory).is_some()
    }

    /// Whether any layer marks a repository root.
    #[must_use]
    pub fn has_repository_root(&self) -> bool {
        self.layers.iter().any(|layer| layer.repository_root)
    }
}

fn push_file(
    into: &mut Vec<IgnoreLayer>,
    base: &Path,
    file: &Path,
    source: IgnoreSource,
    repository_root: bool,
) -> Result<(), WorkspaceError> {
    let Some(contents) = read_ignore_file(file)? else {
        return Ok(());
    };
    let set = GlobSet::parse_file(&contents).map_err(|error| WorkspaceError::Io {
        context: format!("failed to parse {}: {error}", file.display()),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()),
    })?;
    if set.is_empty() {
        return Ok(());
    }
    into.push(IgnoreLayer {
        base: base.to_path_buf(),
        origin: Some(file.to_path_buf()),
        source,
        set,
        repository_root,
    });
    Ok(())
}

/// Renders a relative path as the `/`-separated text patterns are matched on.
///
/// Returns `None` for a path with a non-UTF-8 component. A pattern is text, so
/// it cannot describe such a name in the first place, and treating the path as
/// unmatched is the answer that keeps the file rather than dropping it for a
/// reason nobody can see.
fn relative_to_slash(relative: &Path) -> Option<String> {
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

fn read_ignore_file(file: &Path) -> Result<Option<String>, WorkspaceError> {
    let metadata = match fs::symlink_metadata(file) {
        Ok(metadata) => metadata,
        // `NotADirectory` is the linked-worktree case: `.git` is a pointer file
        // there, so `.git/info/exclude` is not a path at all rather than a path
        // to a missing file. Both mean "no patterns here".
        Err(error) if is_absent(&error) => return Ok(None),
        Err(source) => {
            return Err(WorkspaceError::Io {
                context: format!("failed to inspect {}", file.display()),
                source,
            });
        }
    };
    // A symlinked ignore file could point anywhere, including outside the
    // roots this run is allowed to read. Discovery refuses to follow symlinks
    // for source files; an ignore file gets the same treatment.
    if !metadata.is_file() {
        return Ok(None);
    }
    if metadata.len() > MAX_IGNORE_FILE_BYTES {
        return Err(WorkspaceLimit::FileSize {
            path: file.to_path_buf(),
            actual: metadata.len(),
            maximum: MAX_IGNORE_FILE_BYTES,
        }
        .into());
    }

    match fs::read(file) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(contents) => Ok(Some(contents)),
            // A non-UTF-8 ignore file has no patterns this matcher can honour.
            // Failing the whole run over it would be worse than reading none.
            Err(_) => Ok(None),
        },
        Err(error) if is_absent(&error) => Ok(None),
        Err(source) => Err(WorkspaceError::Io {
            context: format!("failed to read {}", file.display()),
            source,
        }),
    }
}

/// Whether the error means "there is nothing at this path".
fn is_absent(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{IgnoreLayer, IgnoreSource, IgnoreStack};
    use crate::workspace::glob::GlobSet;

    fn layer(base: &str, patterns: &str, source: IgnoreSource) -> IgnoreLayer {
        IgnoreLayer {
            base: PathBuf::from(base),
            origin: Some(PathBuf::from(base).join(".gitignore")),
            source,
            set: GlobSet::parse_file(patterns).expect("patterns compile"),
            repository_root: false,
        }
    }

    #[test]
    fn a_deeper_layer_can_reinclude_what_an_outer_one_dropped() {
        let mut stack = IgnoreStack::new();
        stack.push(layer("/repo", "*.lisp\n", IgnoreSource::GitIgnore));
        stack.push(layer("/repo/src", "!keep.lisp\n", IgnoreSource::GitIgnore));

        assert!(stack.is_ignored(&PathBuf::from("/repo/src/drop.lisp"), false));
        assert!(!stack.is_ignored(&PathBuf::from("/repo/src/keep.lisp"), false));
    }

    #[test]
    fn a_layer_does_not_apply_outside_its_own_directory() {
        let mut stack = IgnoreStack::new();
        stack.push(layer("/repo/src", "*.lisp\n", IgnoreSource::GitIgnore));

        assert!(stack.is_ignored(&PathBuf::from("/repo/src/a.lisp"), false));
        assert!(!stack.is_ignored(&PathBuf::from("/repo/lib/a.lisp"), false));
    }

    #[test]
    fn the_deciding_layer_is_reported() {
        let mut stack = IgnoreStack::new();
        stack.push(layer("/repo", "*.lisp\n", IgnoreSource::GitIgnore));
        stack.push(layer("/repo", "vendor/\n", IgnoreSource::PareditIgnore));

        let decision = stack
            .decide(&PathBuf::from("/repo/vendor"), true)
            .expect("directory is ignored");
        assert_eq!(decision.source, IgnoreSource::PareditIgnore);
    }

    #[test]
    fn command_line_layers_bind_to_the_root_they_were_given() {
        let mut stack = IgnoreStack::new();
        stack.push(IgnoreLayer::from_command_line(
            PathBuf::from("/repo"),
            GlobSet::parse_file("**/*.fasl\n").expect("patterns compile"),
        ));

        assert!(stack.is_ignored(&PathBuf::from("/repo/src/a.fasl"), false));
        assert!(!stack.is_ignored(&PathBuf::from("/repo/src/a.lisp"), false));
    }
}
