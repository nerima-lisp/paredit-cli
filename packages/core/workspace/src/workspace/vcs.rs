//! Repository boundaries and git-derived file sets.
//!
//! Two features live here because they need the same one fact — where the
//! enclosing repository starts.
//!
//! **Repository boundaries** matter to any run that spans more than one
//! checkout. A monorepo of independent repositories, or a `--since` run over
//! several roots, has to keep each checkout's `.gitignore` and each checkout's
//! diff to itself; a single global answer would be wrong for every root but
//! one.
//!
//! **`--since <ref>`** answers "what changed" by asking git rather than by
//! stat-ing the tree. CI is the case that pays for it: a lint run over a
//! thousand-file project spends nearly all of its time on files no commit in
//! the pull request touched.
//!
//! Git is invoked as a subprocess. There is no way around that short of
//! reimplementing packfile and index reading, and a wrong answer about which
//! files changed silently under-reports findings — which is the one failure
//! mode a CI gate must not have. The subprocess is run with a fixed argument
//! vector (never a shell), with `--` terminating options, and with a ref that
//! has been validated as a commit first, so a ref named `--upload-pack=...`
//! cannot become a flag.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::error::{WorkspaceError, WorkspaceLimit};

/// The most paths `--since` will accept from git before refusing.
///
/// A diff against an unrelated root can name every file in the repository, and
/// the caller has already been promised a bounded input set.
const MAX_CHANGED_PATHS: usize = 200_000;

/// The most bytes of git output that will be buffered.
const MAX_GIT_OUTPUT_BYTES: usize = 32 * 1024 * 1024;

/// Where a repository begins.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RepositoryRoot {
    /// The working-tree root, i.e. the directory holding `.git`.
    pub path: PathBuf,
}

/// Finds the repository containing `start`, if any.
///
/// Walks up looking for `.git`, which may be a directory (an ordinary clone) or
/// a file (a linked worktree or a submodule). The walk stops at the filesystem
/// root; it deliberately does not stop at a mount point, because a checkout
/// mounted from elsewhere is still a checkout.
#[must_use]
pub fn find_repository_root(start: &Path) -> Option<RepositoryRoot> {
    let mut current = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };
    while let Some(directory) = current {
        if is_repository_root(directory) {
            return Some(RepositoryRoot {
                path: directory.to_path_buf(),
            });
        }
        current = directory.parent();
    }
    None
}

/// Whether `directory` is itself a repository root.
#[must_use]
pub fn is_repository_root(directory: &Path) -> bool {
    let git = directory.join(".git");
    std::fs::symlink_metadata(&git).is_ok_and(|metadata| {
        let file_type = metadata.file_type();
        file_type.is_dir() || file_type.is_file()
    })
}

/// The git directory backing `repository`, resolving the linked-worktree case.
///
/// In an ordinary clone `.git` is a directory and this is it. In a linked
/// worktree or a submodule it is a *file* holding `gitdir: <path>`, and the
/// difference is not cosmetic: `<root>/.git/info/exclude` then fails with
/// `ENOTDIR` rather than `NotFound`, which is how a "file is simply absent"
/// code path turns into a hard error for every worktree user.
#[must_use]
pub fn git_directory(repository: &Path) -> Option<PathBuf> {
    let git = repository.join(".git");
    let metadata = std::fs::symlink_metadata(&git).ok()?;
    if metadata.is_dir() {
        return Some(git);
    }
    if !metadata.is_file() || metadata.len() > MAX_GITDIR_FILE_BYTES {
        return None;
    }
    let contents = std::fs::read_to_string(&git).ok()?;
    let target = contents.lines().find_map(|line| {
        line.trim()
            .strip_prefix("gitdir:")
            .map(|value| PathBuf::from(value.trim()))
    })?;
    Some(if target.is_absolute() {
        target
    } else {
        repository.join(target)
    })
}

/// The `info/exclude` files that apply to `repository`, most general first.
///
/// A linked worktree has two: the shared one in the common directory, which
/// every worktree of the clone sees, and its own. Git reads both, and a tool
/// that reads only the first would ignore a rule the user wrote for exactly
/// this checkout.
#[must_use]
pub fn git_info_exclude_files(repository: &Path) -> Vec<PathBuf> {
    let Some(git_directory) = git_directory(repository) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    let common = git_directory.join("commondir");
    if let Ok(contents) = std::fs::read_to_string(&common) {
        let relative = PathBuf::from(contents.trim());
        let resolved = if relative.is_absolute() {
            relative
        } else {
            git_directory.join(relative)
        };
        files.push(lexically_normalize(&resolved.join("info").join("exclude")));
    }
    files.push(lexically_normalize(
        &git_directory.join("info").join("exclude"),
    ));
    files.dedup();
    files
}

/// Resolves `.` and `..` textually, without touching the filesystem.
///
/// A `commondir` file holds a relative path like `../..`, and leaving it
/// unresolved would put `.git/worktrees/name/../../info/exclude` in a report
/// that a human is meant to read.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

/// The largest `.git` pointer file that will be read.
const MAX_GITDIR_FILE_BYTES: u64 = 64 * 1024;

/// Which changed paths a `--since` run should collect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SinceOptions {
    /// Also list files git does not track yet, honouring `.gitignore`.
    ///
    /// On by default: a pull request that adds a file has not committed it in
    /// the working tree the developer is running the command in, and a
    /// `--since` that silently skipped new files would be a trap.
    pub include_untracked: bool,
}

impl Default for SinceOptions {
    fn default() -> Self {
        Self {
            include_untracked: true,
        }
    }
}

/// Why a `--since` request could not be answered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitRefusal {
    /// No `.git` was found above the requested root.
    NotARepository { path: PathBuf },
    /// The ref text could not be a ref, or could be mistaken for an option.
    InvalidRef { reference: String },
    /// Git ran but did not recognise the ref.
    UnknownRef { reference: String, message: String },
    /// The `git` executable could not be run at all.
    GitUnavailable { message: String },
    /// Git exited non-zero for something other than an unknown ref.
    GitFailed { message: String },
}

impl std::fmt::Display for GitRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotARepository { path } => write!(
                formatter,
                "--since requires a git repository, and none contains {}",
                path.display()
            ),
            Self::InvalidRef { reference } => {
                write!(
                    formatter,
                    "--since received an invalid git ref: {reference}"
                )
            }
            Self::UnknownRef { reference, message } => write!(
                formatter,
                "--since could not resolve the git ref {reference}: {message}"
            ),
            Self::GitUnavailable { message } => {
                write!(formatter, "--since requires the git executable: {message}")
            }
            Self::GitFailed { message } => write!(formatter, "git failed: {message}"),
        }
    }
}

impl std::error::Error for GitRefusal {}

impl From<GitRefusal> for WorkspaceError {
    fn from(refusal: GitRefusal) -> Self {
        Self::Io {
            context: refusal.to_string(),
            source: std::io::Error::other(refusal.to_string()),
        }
    }
}

/// Validates a ref before it is handed to git.
///
/// The check that matters is the leading `-`: git's argument parser would read
/// such a ref as an option, and `--upload-pack=` and friends turn that into
/// command execution. A ref is also rejected if it carries a NUL or a newline,
/// neither of which can appear in one and both of which would confuse the
/// `-z` framing of the output.
fn validate_ref(reference: &str) -> Result<(), GitRefusal> {
    let invalid = reference.is_empty()
        || reference.starts_with('-')
        || reference
            .chars()
            .any(|character| character == '\0' || character == '\n' || character.is_control());
    if invalid {
        return Err(GitRefusal::InvalidRef {
            reference: reference.to_owned(),
        });
    }
    Ok(())
}

fn run_git(repository: &Path, arguments: &[&str]) -> Result<std::process::Output, GitRefusal> {
    Command::new("git")
        .arg("-C")
        .arg(repository)
        // A concurrent `git status` must not be blocked by, or block, a
        // read-only query issued from a lint run.
        .arg("--no-optional-locks")
        // Without this git renders a non-ASCII path as C-style escapes, which
        // would arrive here as a filename that does not exist.
        .arg("-c")
        .arg("core.quotepath=false")
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| GitRefusal::GitUnavailable {
            message: error.to_string(),
        })
}

/// Resolves `reference` to a commit inside `repository`.
///
/// Done before any diff so that a typo is reported as a bad ref rather than as
/// an empty change set, which would otherwise read as "nothing changed" and
/// let a CI gate pass without examining anything.
pub fn resolve_commit(repository: &Path, reference: &str) -> Result<String, GitRefusal> {
    validate_ref(reference)?;
    let peeled = format!("{reference}^{{commit}}");
    let output = run_git(repository, &["rev-parse", "--verify", "--quiet", &peeled])?;
    if !output.status.success() {
        return Err(GitRefusal::UnknownRef {
            reference: reference.to_owned(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Lists the paths that differ between `reference` and the working tree.
///
/// Paths are returned absolute, deleted files are dropped (there is nothing to
/// analyse), and a rename reports only its destination. Untracked files are
/// appended when [`SinceOptions::include_untracked`] is set, using git's own
/// `--exclude-standard`, so they are filtered by exactly the `.gitignore` rules
/// git would apply.
pub fn changed_paths_since(
    repository: &Path,
    reference: &str,
    options: SinceOptions,
) -> Result<Vec<PathBuf>, WorkspaceError> {
    let commit = resolve_commit(repository, reference)?;

    let mut paths = Vec::new();
    let diff = run_git(
        repository,
        &[
            "diff",
            // difftastic and friends are frequently configured as the external
            // diff driver. `--name-only` does not need one, and letting it run
            // would fork a renderer per file for output that is thrown away.
            "--no-ext-diff",
            "--name-only",
            "-z",
            // Lowercase excludes: a deleted path has nothing left to parse.
            "--diff-filter=d",
            &commit,
            "--",
        ],
    )?;
    if !diff.status.success() {
        return Err(GitRefusal::GitFailed {
            message: String::from_utf8_lossy(&diff.stderr).trim().to_owned(),
        }
        .into());
    }
    push_nul_separated(repository, &diff.stdout, &mut paths)?;

    if options.include_untracked {
        let untracked = run_git(
            repository,
            &["ls-files", "--others", "--exclude-standard", "-z", "--"],
        )?;
        if !untracked.status.success() {
            return Err(GitRefusal::GitFailed {
                message: String::from_utf8_lossy(&untracked.stderr).trim().to_owned(),
            }
            .into());
        }
        push_nul_separated(repository, &untracked.stdout, &mut paths)?;
    }

    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Lists every path git tracks in `repository`.
///
/// This is the enumeration `--from-git` uses: it is both faster than walking a
/// large tree and exactly the set a developer thinks of as "the project",
/// since it is already filtered by every ignore rule git knows about.
pub fn tracked_paths(repository: &Path) -> Result<Vec<PathBuf>, WorkspaceError> {
    let output = run_git(repository, &["ls-files", "--cached", "-z", "--"])?;
    if !output.status.success() {
        return Err(GitRefusal::GitFailed {
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        }
        .into());
    }
    let mut paths = Vec::new();
    push_nul_separated(repository, &output.stdout, &mut paths)?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn push_nul_separated(
    repository: &Path,
    output: &[u8],
    paths: &mut Vec<PathBuf>,
) -> Result<(), WorkspaceError> {
    if output.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(WorkspaceLimit::TotalBytes {
            actual: output.len() as u64,
            maximum: MAX_GIT_OUTPUT_BYTES as u64,
        }
        .into());
    }

    for entry in output.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        if paths.len() >= MAX_CHANGED_PATHS {
            return Err(WorkspaceLimit::Files {
                maximum: MAX_CHANGED_PATHS,
            }
            .into());
        }
        let relative = decode_path(entry)?;
        paths.push(repository.join(relative));
    }
    Ok(())
}

#[cfg(unix)]
fn decode_path(bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    Ok(PathBuf::from(OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn decode_path(bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
    // Windows paths are UTF-16 underneath and git emits UTF-8 for them, so a
    // byte sequence that is not valid UTF-8 is not a path this platform has.
    std::str::from_utf8(bytes)
        .map(PathBuf::from)
        .map_err(|_| WorkspaceError::Unavailable("git reported a non-UTF-8 path"))
}

#[cfg(test)]
mod tests {
    use super::{GitRefusal, validate_ref};

    #[test]
    fn a_ref_that_looks_like_an_option_is_refused() {
        assert_eq!(
            validate_ref("--upload-pack=touch /tmp/pwned"),
            Err(GitRefusal::InvalidRef {
                reference: "--upload-pack=touch /tmp/pwned".to_owned(),
            })
        );
        assert_eq!(
            validate_ref("-HEAD"),
            Err(GitRefusal::InvalidRef {
                reference: "-HEAD".to_owned(),
            })
        );
    }

    #[test]
    fn a_ref_with_a_control_character_is_refused() {
        assert!(validate_ref("HEAD\nmain").is_err());
        assert!(validate_ref("HEAD\0").is_err());
        assert!(validate_ref("").is_err());
    }

    #[test]
    fn ordinary_refs_pass() {
        assert!(validate_ref("HEAD").is_ok());
        assert!(validate_ref("origin/main").is_ok());
        assert!(validate_ref("v1.2.1").is_ok());
        assert!(validate_ref("HEAD~3").is_ok());
        assert!(validate_ref("abc1234").is_ok());
    }
}
