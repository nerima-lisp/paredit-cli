#[cfg(unix)]
use std::cell::Cell;
#[cfg(unix)]
use std::collections::BTreeSet;
#[cfg(unix)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs;
use std::io::{self, ErrorKind, IsTerminal, Read};
#[cfg(unix)]
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
#[cfg(any(unix, test))]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

use super::{DialectArg, SourceInput};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ParseError, SyntaxTree};
use crate::infrastructure::fs_identity::FilesystemIdentity;

#[cfg(any(unix, test))]
static STAGED_WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);
#[cfg(unix)]
const UNIQUE_SIBLING_ATTEMPTS: usize = 128;
#[cfg(unix)]
const CLEANUP_QUARANTINE_NAME: &str = ".paredit.cleanup";
#[cfg(unix)]
const CLEANUP_QUARANTINE_MODE: u32 = 0o700;
pub(crate) const MAX_SOURCE_INPUT_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(all(test, unix))]
type BeforeExistingFileOpenHook = (PathBuf, Box<dyn FnOnce()>);

#[cfg(all(test, unix))]
type BeforeExistingTargetReplaceHook = (PathBuf, Box<dyn FnOnce()>);

#[cfg(all(test, unix))]
type BeforeExchangeRestoreHook = (PathBuf, Box<dyn FnOnce()>);

#[cfg(all(test, unix))]
type BeforeArtifactCleanupHook = (PathBuf, Box<dyn FnOnce()>);

#[cfg(all(test, unix))]
type AfterArtifactValidationHook = (PathBuf, Box<dyn FnOnce()>);

#[cfg(all(test, unix))]
thread_local! {
    static BEFORE_EXISTING_FILE_OPEN_HOOK: std::cell::RefCell<Option<BeforeExistingFileOpenHook>> =
        const { std::cell::RefCell::new(None) };
    static BEFORE_EXISTING_TARGET_REPLACE_HOOK: std::cell::RefCell<Option<BeforeExistingTargetReplaceHook>> =
        const { std::cell::RefCell::new(None) };
    static BEFORE_EXCHANGE_RESTORE_HOOK: std::cell::RefCell<Option<BeforeExchangeRestoreHook>> =
        const { std::cell::RefCell::new(None) };
    static BEFORE_ARTIFACT_CLEANUP_HOOK: std::cell::RefCell<Option<BeforeArtifactCleanupHook>> =
        const { std::cell::RefCell::new(None) };
    static AFTER_ARTIFACT_VALIDATION_HOOK: std::cell::RefCell<Option<AfterArtifactValidationHook>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(all(test, unix))]
#[derive(Debug)]
struct BeforeExistingFileOpenHookGuard;

#[cfg(all(test, unix))]
impl Drop for BeforeExistingFileOpenHookGuard {
    fn drop(&mut self) {
        BEFORE_EXISTING_FILE_OPEN_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(all(test, unix))]
#[derive(Debug)]
struct BeforeExistingTargetReplaceHookGuard;

#[cfg(all(test, unix))]
impl Drop for BeforeExistingTargetReplaceHookGuard {
    fn drop(&mut self) {
        BEFORE_EXISTING_TARGET_REPLACE_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(all(test, unix))]
#[derive(Debug)]
struct BeforeExchangeRestoreHookGuard;

#[cfg(all(test, unix))]
#[derive(Debug)]
struct BeforeArtifactCleanupHookGuard;

#[cfg(all(test, unix))]
#[derive(Debug)]
struct AfterArtifactValidationHookGuard;

#[cfg(all(test, unix))]
impl Drop for BeforeExchangeRestoreHookGuard {
    fn drop(&mut self) {
        BEFORE_EXCHANGE_RESTORE_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(all(test, unix))]
impl Drop for BeforeArtifactCleanupHookGuard {
    fn drop(&mut self) {
        BEFORE_ARTIFACT_CLEANUP_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(all(test, unix))]
impl Drop for AfterArtifactValidationHookGuard {
    fn drop(&mut self) {
        AFTER_ARTIFACT_VALIDATION_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(all(test, unix))]
fn install_before_existing_file_open_hook(
    path: PathBuf,
    action: impl FnOnce() + 'static,
) -> BeforeExistingFileOpenHookGuard {
    BEFORE_EXISTING_FILE_OPEN_HOOK.with(|slot| {
        assert!(
            slot.borrow_mut()
                .replace((path, Box::new(action)))
                .is_none(),
            "only one existing-file open hook may be installed per thread"
        );
    });
    BeforeExistingFileOpenHookGuard
}

#[cfg(all(test, unix))]
fn run_before_existing_file_open_hook(path: &FsPath) {
    let hook = BEFORE_EXISTING_FILE_OPEN_HOOK.with(|slot| slot.borrow_mut().take());
    let Some((expected_path, action)) = hook else {
        return;
    };
    if expected_path == path {
        action();
    } else {
        BEFORE_EXISTING_FILE_OPEN_HOOK.with(|slot| {
            assert!(slot.borrow_mut().replace((expected_path, action)).is_none());
        });
    }
}

#[cfg(all(test, unix))]
fn install_before_existing_target_replace_hook(
    path: PathBuf,
    action: impl FnOnce() + 'static,
) -> BeforeExistingTargetReplaceHookGuard {
    BEFORE_EXISTING_TARGET_REPLACE_HOOK.with(|slot| {
        assert!(
            slot.borrow_mut()
                .replace((path, Box::new(action)))
                .is_none(),
            "only one existing-target replace hook may be installed per thread"
        );
    });
    BeforeExistingTargetReplaceHookGuard
}

#[cfg(all(test, unix))]
fn run_before_existing_target_replace_hook(path: &FsPath) {
    let hook = BEFORE_EXISTING_TARGET_REPLACE_HOOK.with(|slot| slot.borrow_mut().take());
    let Some((expected_path, action)) = hook else {
        return;
    };
    if expected_path == path {
        action();
    } else {
        BEFORE_EXISTING_TARGET_REPLACE_HOOK.with(|slot| {
            assert!(slot.borrow_mut().replace((expected_path, action)).is_none());
        });
    }
}

#[cfg(all(test, unix))]
fn install_before_exchange_restore_hook(
    path: PathBuf,
    action: impl FnOnce() + 'static,
) -> BeforeExchangeRestoreHookGuard {
    BEFORE_EXCHANGE_RESTORE_HOOK.with(|slot| {
        assert!(
            slot.borrow_mut()
                .replace((path, Box::new(action)))
                .is_none(),
            "only one exchange-restore hook may be installed per thread"
        );
    });
    BeforeExchangeRestoreHookGuard
}

#[cfg(all(test, unix))]
fn run_before_exchange_restore_hook(path: &FsPath) {
    let hook = BEFORE_EXCHANGE_RESTORE_HOOK.with(|slot| slot.borrow_mut().take());
    let Some((expected_path, action)) = hook else {
        return;
    };
    if expected_path == path {
        action();
    } else {
        BEFORE_EXCHANGE_RESTORE_HOOK.with(|slot| {
            assert!(slot.borrow_mut().replace((expected_path, action)).is_none());
        });
    }
}

#[cfg(all(test, unix))]
fn install_before_artifact_cleanup_hook(
    path: PathBuf,
    action: impl FnOnce() + 'static,
) -> BeforeArtifactCleanupHookGuard {
    BEFORE_ARTIFACT_CLEANUP_HOOK.with(|slot| {
        assert!(
            slot.borrow_mut()
                .replace((path, Box::new(action)))
                .is_none(),
            "only one artifact cleanup hook may be installed per thread"
        );
    });
    BeforeArtifactCleanupHookGuard
}

#[cfg(all(test, unix))]
fn run_before_artifact_cleanup_hook(path: &FsPath) {
    let hook = BEFORE_ARTIFACT_CLEANUP_HOOK.with(|slot| slot.borrow_mut().take());
    let Some((expected_path, action)) = hook else {
        return;
    };
    if expected_path == path {
        action();
    } else {
        BEFORE_ARTIFACT_CLEANUP_HOOK.with(|slot| {
            assert!(slot.borrow_mut().replace((expected_path, action)).is_none());
        });
    }
}

#[cfg(all(test, unix))]
fn install_after_artifact_validation_hook(
    path: PathBuf,
    action: impl FnOnce() + 'static,
) -> AfterArtifactValidationHookGuard {
    AFTER_ARTIFACT_VALIDATION_HOOK.with(|slot| {
        assert!(
            slot.borrow_mut()
                .replace((path, Box::new(action)))
                .is_none(),
            "only one post-validation hook may be installed per thread"
        );
    });
    AfterArtifactValidationHookGuard
}

#[cfg(all(test, unix))]
fn run_after_artifact_validation_hook(path: &FsPath) {
    let hook = AFTER_ARTIFACT_VALIDATION_HOOK.with(|slot| slot.borrow_mut().take());
    let Some((expected_path, action)) = hook else {
        return;
    };
    if expected_path == path {
        action();
    } else {
        AFTER_ARTIFACT_VALIDATION_HOOK.with(|slot| {
            assert!(slot.borrow_mut().replace((expected_path, action)).is_none());
        });
    }
}

pub(crate) fn read_text_with_limit(
    reader: impl Read,
    limit: u64,
    description: &str,
) -> Result<String> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read {description}"))?;
    if bytes.len() as u64 > limit {
        anyhow::bail!("refusing to read {description}: input exceeds {limit} bytes");
    }
    String::from_utf8(bytes).with_context(|| format!("{description} is not valid UTF-8"))
}

pub(crate) fn read_text_file_with_limit(path: &FsPath, limit: u64) -> Result<String> {
    let file = open_regular_input_file(path)
        .with_context(|| format!("failed to open or inspect {}", path.display()))?;
    read_text_with_limit(file, limit, &path.display().to_string())
}

pub(crate) fn read_text_file_with_expected_target(
    path: &FsPath,
    limit: u64,
) -> Result<(String, ExpectedWriteTarget)> {
    let file = open_regular_input_file(path)
        .with_context(|| format!("failed to open or inspect {}", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("failed to inspect open input {}", path.display()))?;
    let text = read_text_with_limit(file, limit, &path.display().to_string())?;
    let expected = ExpectedWriteTarget::from_metadata_and_content(&metadata, &text)?;
    Ok((text, expected))
}

fn open_regular_input_file(path: &FsPath) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .map_err(|error| describe_refused_input_link(path, error))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("refusing non-regular input file {}", path.display()),
        ));
    }
    Ok(file)
}

/// Restates the `O_NOFOLLOW` refusal in terms of the policy that produced it.
///
/// Opening a symlinked path with `O_NOFOLLOW` fails as `ELOOP`, which surfaces
/// as "too many levels of symbolic links" and reads like a link cycle. The
/// real reason is that paredit only ever opens the regular file it was pointed
/// at, so the caller — not the tool — decides which file a link stands for.
fn describe_refused_input_link(path: &FsPath, error: io::Error) -> io::Error {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return io::Error::new(
            ErrorKind::InvalidInput,
            format!(
                "refusing to read symlink {} (pass the path it resolves to)",
                path.display()
            ),
        );
    }
    error
}

pub(crate) fn read_input(file: Option<PathBuf>) -> Result<SourceInput> {
    match file {
        Some(path) => {
            let text = read_text_file_with_limit(&path, MAX_SOURCE_INPUT_BYTES)?;
            Ok(SourceInput {
                text,
                file: Some(path),
            })
        }
        None => {
            let mut stdin = io::stdin();
            if stdin.is_terminal() {
                anyhow::bail!(
                    "no input: pass --file <path> or pipe source into stdin \
                     (refusing to wait on an interactive terminal)"
                );
            }
            let text = read_text_with_limit(&mut stdin, MAX_SOURCE_INPUT_BYTES, "stdin")?;
            Ok(SourceInput { text, file: None })
        }
    }
}

pub(crate) fn read_input_and_dialect(
    file: Option<PathBuf>,
    explicit: Option<DialectArg>,
) -> Result<(SourceInput, Dialect)> {
    let input = read_input(file)?;
    let dialect = super::detect_dialect(&input, explicit);
    Ok((input, dialect))
}

pub(crate) fn read_input_dialect_and_tree(
    file: Option<PathBuf>,
    explicit: Option<DialectArg>,
) -> Result<(SourceInput, Dialect, SyntaxTree)> {
    let (input, dialect) = read_input_and_dialect(file, explicit)?;
    let tree = parse_document(&input, dialect)?;
    Ok((input, dialect, tree))
}

/// Parses a source document with its resolved dialect, naming the input and
/// the error's line/column in the context. The underlying [`ParseError`] keeps
/// the raw byte offset, which feeds directly into `--at`.
pub(crate) fn parse_document(input: &SourceInput, dialect: Dialect) -> Result<SyntaxTree> {
    SyntaxTree::parse_with_dialect(&input.text, dialect).map_err(|error| {
        let location = parse_error_line_column(&input.text, &error);
        let source = match input.file.as_deref() {
            Some(path) => path.display().to_string(),
            None => "stdin".to_owned(),
        };
        anyhow::Error::new(error).context(format!("failed to parse {source} ({location})"))
    })
}

fn parse_error_line_column(text: &str, error: &ParseError) -> String {
    let position = match error {
        ParseError::UnexpectedClose { position, .. }
        | ParseError::MismatchedClose { position, .. }
        | ParseError::ResourceLimitExceeded { position, .. }
        | ParseError::UnsupportedReaderDispatch { position, .. } => *position,
        ParseError::UnclosedList(position)
        | ParseError::UnterminatedString(position)
        | ParseError::UnterminatedBlockComment(position)
        | ParseError::UnterminatedSymbol(position)
        | ParseError::DanglingSingleEscape(position)
        | ParseError::MissingReaderForm(position) => *position,
    };
    let clamped = position.min(text.len());
    let line = text[..clamped]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = match text[..clamped].rfind('\n') {
        Some(newline) => clamped - newline,
        None => clamped + 1,
    };
    format!("line {line}, column {column}")
}

pub(crate) fn read_file_or_empty(path: &FsPath) -> Result<(SourceInput, bool)> {
    match open_regular_input_file(path) {
        Ok(file) => Ok((
            SourceInput {
                text: read_text_with_limit(
                    file,
                    MAX_SOURCE_INPUT_BYTES,
                    &path.display().to_string(),
                )?,
                file: Some(path.to_path_buf()),
            },
            true,
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok((
            SourceInput {
                text: String::new(),
                file: Some(path.to_path_buf()),
            },
            false,
        )),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

pub(crate) fn write_files_with_rollback<I>(files: I) -> Result<()>
where
    I: IntoIterator<Item = (PathBuf, String)>,
{
    write_files_with_rollback_inner(
        files
            .into_iter()
            .map(|(path, content)| (path, content, None))
            .collect(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpectedWriteTarget {
    identity: FileIdentity,
    digest: [u8; 32],
}

impl ExpectedWriteTarget {
    pub(crate) fn from_metadata_and_content(
        metadata: &fs::Metadata,
        content: &str,
    ) -> io::Result<Self> {
        Ok(Self {
            identity: file_identity(metadata)?,
            digest: *blake3::hash(content.as_bytes()).as_bytes(),
        })
    }
}

pub(crate) fn write_files_with_rollback_expected<I>(files: I) -> Result<()>
where
    I: IntoIterator<Item = (PathBuf, String, ExpectedWriteTarget)>,
{
    write_files_with_rollback_inner(
        files
            .into_iter()
            .map(|(path, content, expected)| (path, content, Some(expected)))
            .collect(),
    )
}

pub(crate) struct AnchoredExpectedWrite {
    pub(crate) display_path: PathBuf,
    pub(crate) parent_dir: Arc<cap_std::fs::Dir>,
    pub(crate) file_name: OsString,
    pub(crate) content: String,
    pub(crate) expected: ExpectedWriteTarget,
}

pub(crate) fn write_files_with_rollback_expected_anchored(
    files: Vec<AnchoredExpectedWrite>,
) -> Result<()> {
    #[cfg(unix)]
    {
        validate_write_inputs(files.iter().map(|file| (&file.display_path, &file.content)))?;
        stage_and_apply(files, |file| {
            let parent = anchored_directory_from_retained(
                &file.display_path,
                &file.parent_dir,
                &file.file_name,
            )?;
            stage_write_target_expected_inner(
                file.display_path,
                file.content,
                Some(file.expected),
                parent,
                file.file_name,
            )
        })
    }
    #[cfg(not(unix))]
    {
        for AnchoredExpectedWrite {
            display_path,
            parent_dir,
            file_name,
            content,
            expected,
        } in files
        {
            let _ = (display_path, parent_dir, file_name, content, expected);
        }
        Err(io::Error::new(
            ErrorKind::Unsupported,
            "capability-anchored writes are unsupported on this platform",
        )
        .into())
    }
}

fn write_files_with_rollback_inner(
    files: Vec<(PathBuf, String, Option<ExpectedWriteTarget>)>,
) -> Result<()> {
    #[cfg(unix)]
    validate_write_inputs(files.iter().map(|(path, content, _)| (path, content)))?;
    write_files_transactionally(files)
}

fn write_files_transactionally(
    files: Vec<(PathBuf, String, Option<ExpectedWriteTarget>)>,
) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = files;
        Err(io::Error::new(
            ErrorKind::Unsupported,
            "transactional writes are unsupported on this platform",
        )
        .into())
    }
    #[cfg(unix)]
    {
        stage_and_apply(files, |(path, content, expected)| {
            stage_write_target_expected(path, content, expected)
        })
    }
}

#[cfg(unix)]
fn validate_write_inputs<'a>(
    files: impl IntoIterator<Item = (&'a PathBuf, &'a String)>,
) -> Result<()> {
    let files = files.into_iter().collect::<Vec<_>>();
    reject_duplicate_write_targets(files.iter().map(|(path, _)| *path))?;
    // Central invariant: paredit never persists an unbalanced document, no
    // matter which command produced the rewrite.
    for (path, content) in files {
        SyntaxTree::parse(content).with_context(|| {
            format!(
                "refusing to write {}: rewritten source does not reparse",
                path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(unix)]
fn stage_and_apply<T>(
    files: Vec<T>,
    mut stage: impl FnMut(T) -> Result<StagedWriteTarget>,
) -> Result<()> {
    let mut staged = Vec::with_capacity(files.len());
    for file in files {
        match stage(file) {
            Ok(target) => staged.push(target),
            Err(error) => {
                let cleanup_errors = cleanup_unapplied_writes(&staged);
                if cleanup_errors.is_empty() {
                    return Err(error);
                }
                return Err(error.context(format!(
                    "staging cleanup also failed: {}",
                    cleanup_errors.join("; ")
                )));
            }
        }
    }
    apply_staged_writes(staged)
}

#[cfg(unix)]
fn apply_staged_writes(staged: Vec<StagedWriteTarget>) -> Result<()> {
    for (index, target) in staged.iter().enumerate() {
        if let Err(error) = apply_staged_write(target) {
            let cleanup_errors = rollback_failed_write(&staged, index);
            let apply_error = anyhow::Error::new(error)
                .context(format!("failed to write {}", target.path.display()));
            if cleanup_errors.is_empty() {
                return Err(apply_error);
            }
            return Err(apply_error.context(format!(
                "rollback/cleanup also failed: {}",
                cleanup_errors.join("; ")
            )));
        }
    }

    let mut cleanup_errors = Vec::new();
    for target in staged {
        if target.existed {
            remove_backup_for_cleanup(&target, &mut cleanup_errors);
        }
    }

    if !cleanup_errors.is_empty() {
        anyhow::bail!(
            "writes committed successfully, but backup cleanup failed: {}",
            cleanup_errors.join("; ")
        );
    }

    Ok(())
}

pub(crate) fn write_file_with_rollback(path: PathBuf, content: String) -> Result<()> {
    write_files_with_rollback([(path, content)])
}

pub(crate) fn write_artifact_with_rollback(path: PathBuf, content: String) -> Result<()> {
    write_files_transactionally(vec![(path, content, None)])
}

#[cfg(unix)]
struct StagedWriteTarget {
    path: PathBuf,
    staged_path: PathBuf,
    backup_path: PathBuf,
    #[cfg(unix)]
    parent: AnchoredDirectory,
    #[cfg(unix)]
    target_name: OsString,
    #[cfg(unix)]
    staged_name: OsString,
    #[cfg(unix)]
    backup_name: OsString,
    existed: bool,
    original_identity: Option<FileIdentity>,
    original_digest: Option<[u8; 32]>,
    #[cfg(unix)]
    original_security: Option<SecurityMetadata>,
    staged_identity: FileIdentity,
    staged_digest: [u8; 32],
    backup_identity: Option<FileIdentity>,
    backup_digest: Option<[u8; 32]>,
    #[cfg(unix)]
    publication_state_uncertain: Cell<bool>,
}

#[cfg(unix)]
#[derive(Clone, Copy)]
struct ExpectedFileSnapshot {
    identity: FileIdentity,
    digest: [u8; 32],
    description: &'static str,
}

#[cfg(unix)]
struct ReplaceFileRequest<'a> {
    parent: &'a AnchoredDirectory,
    source_name: &'a OsStr,
    target_name: &'a OsStr,
    source_path: &'a FsPath,
    target_path: &'a FsPath,
    publication_state_uncertain: Option<&'a Cell<bool>>,
}

#[cfg(unix)]
enum ReplaceTargetExpectation<'a> {
    Missing,
    Existing {
        snapshot: ExpectedFileSnapshot,
        security: &'a SecurityMetadata,
    },
}

#[cfg(unix)]
struct AnchoredDirectory {
    handle: cap_std::fs::Dir,
    display_path: PathBuf,
    identity: FileIdentity,
}

#[cfg(unix)]
struct CreatedSibling {
    path: PathBuf,
    name: OsString,
    file: fs::File,
}

#[cfg(unix)]
struct QuarantinedFile {
    directory: AnchoredDirectory,
    name: OsString,
    path: PathBuf,
}

#[cfg(unix)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct SecurityMetadata {
    uid: u32,
    gid: u32,
    mode: u32,
    xattrs: Vec<(OsString, Vec<u8>)>,
    #[cfg(target_os = "macos")]
    acl: Option<super::macos_acl::SerializedAcl>,
}

type FileIdentity = FilesystemIdentity;

#[cfg(all(test, unix))]
fn stage_write_target(path: PathBuf, content: String) -> Result<StagedWriteTarget> {
    stage_write_target_expected(path, content, None)
}

#[cfg(unix)]
fn stage_write_target_expected(
    path: PathBuf,
    content: String,
    expected_original: Option<ExpectedWriteTarget>,
) -> Result<StagedWriteTarget> {
    #[cfg(unix)]
    {
        let (parent, target_name) = open_anchored_parent(&path)?;
        stage_write_target_expected_inner(path, content, expected_original, parent, target_name)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, content, expected_original);
        Err(io::Error::new(
            ErrorKind::Unsupported,
            "transactional writes are unsupported on this platform",
        )
        .into())
    }
}

#[cfg(unix)]
fn stage_write_target_expected_inner(
    path: PathBuf,
    content: String,
    expected_original: Option<ExpectedWriteTarget>,
    #[cfg(unix)] parent: AnchoredDirectory,
    #[cfg(unix)] target_name: OsString,
) -> Result<StagedWriteTarget> {
    let staged_digest = *blake3::hash(content.as_bytes()).as_bytes();
    let metadata = match target_symlink_metadata(
        #[cfg(unix)]
        &parent,
        #[cfg(unix)]
        &target_name,
        &path,
    ) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!("refusing to write symlink {}", path.display());
        }
        Ok(metadata) if !metadata.file_type().is_file() => {
            anyhow::bail!("refusing to write non-regular file {}", path.display());
        }
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("failed to stat {}", path.display()));
        }
    };
    let existed = metadata.is_some();
    let mut original = if let Some(expected) = metadata.as_ref() {
        #[cfg(test)]
        run_before_existing_file_open_hook(&path);
        let mut file = open_existing_file_at(
            #[cfg(unix)]
            &parent,
            #[cfg(unix)]
            &target_name,
            &path,
        )
        .with_context(|| format!("failed to open existing target {}", path.display()))?;
        let opened_metadata = file
            .metadata()
            .with_context(|| format!("failed to inspect open target {}", path.display()))?;
        anyhow::ensure!(
            opened_metadata.file_type().is_file(),
            "refusing to write non-regular file {}",
            path.display()
        );
        let path_identity = snapshot_identity(expected)?;
        let opened_identity = file_identity(&opened_metadata)?;
        anyhow::ensure!(
            path_identity == opened_identity,
            "refusing replaced target {}",
            path.display()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            anyhow::ensure!(
                opened_metadata.nlink() == 1,
                "refusing to replace hard-linked target {}",
                path.display()
            );
        }
        if let Some(expected_original) = expected_original {
            anyhow::ensure!(
                opened_identity == expected_original.identity,
                "refusing target replaced since parsing {}",
                path.display()
            );
            let digest = digest_reader(&mut file)
                .with_context(|| format!("failed to verify parsed target {}", path.display()))?;
            anyhow::ensure!(
                digest == expected_original.digest,
                "refusing target changed since parsing {}",
                path.display()
            );
            file.seek(SeekFrom::Start(0))
                .with_context(|| format!("failed to rewind parsed target {}", path.display()))?;
        }
        Some(file)
    } else {
        anyhow::ensure!(
            expected_original.is_none(),
            "refusing target removed since parsing {}",
            path.display()
        );
        None
    };
    let permissions = original
        .as_ref()
        .map(|file| file.metadata().map(|metadata| metadata.permissions()))
        .transpose()
        .with_context(|| format!("failed to inspect open target {}", path.display()))?;
    #[cfg(unix)]
    let original_security = original
        .as_ref()
        .map(read_security_metadata)
        .transpose()
        .with_context(|| format!("failed to read security metadata for {}", path.display()))?;

    let CreatedSibling {
        path: staged_path,
        name: staged_name,
        file: mut staged_file,
    } = create_unique_sibling_file(
        #[cfg(unix)]
        &parent,
        &path,
        "tmp",
    )
    .with_context(|| format!("failed to create staging file for {}", path.display()))?;
    if let Err(error) = staged_file.write_all(content.as_bytes()) {
        let error =
            anyhow::Error::new(error).context(format!("failed to stage {}", staged_path.display()));
        return Err(with_open_artifact_cleanup(
            error,
            &parent,
            &staged_name,
            &staged_path,
            "staging file",
            staged_file,
        ));
    }

    if let Some(permissions) = permissions.as_ref() {
        if let Err(error) = staged_file.set_permissions(permissions.clone()) {
            let error = anyhow::Error::new(error).context(format!(
                "failed to copy permissions to {}",
                staged_path.display()
            ));
            return Err(with_open_artifact_cleanup(
                error,
                &parent,
                &staged_name,
                &staged_path,
                "staging file",
                staged_file,
            ));
        }
    }
    #[cfg(unix)]
    if let Some(security) = original_security.as_ref() {
        if let Err(error) = apply_security_metadata(&staged_file, permissions.as_ref(), security) {
            let error = anyhow::Error::new(error).context(format!(
                "failed to preserve security metadata on {}",
                staged_path.display()
            ));
            return Err(with_open_artifact_cleanup(
                error,
                &parent,
                &staged_name,
                &staged_path,
                "staging file",
                staged_file,
            ));
        }
    }
    if let Err(error) = staged_file.sync_all() {
        let error =
            anyhow::Error::new(error).context(format!("failed to sync {}", staged_path.display()));
        return Err(with_open_artifact_cleanup(
            error,
            &parent,
            &staged_name,
            &staged_path,
            "staging file",
            staged_file,
        ));
    }
    let staged_identity = match staged_file.metadata() {
        Ok(metadata) => match file_identity(&metadata) {
            Ok(identity) => identity,
            Err(error) => {
                let error = anyhow::Error::new(error).context(format!(
                    "stable filesystem identity is unavailable for {}",
                    staged_path.display()
                ));
                return Err(with_open_artifact_cleanup(
                    error,
                    &parent,
                    &staged_name,
                    &staged_path,
                    "staging file",
                    staged_file,
                ));
            }
        },
        Err(error) => {
            let error = anyhow::Error::new(error).context(format!(
                "failed to inspect staging file {}",
                staged_path.display()
            ));
            return Err(with_open_artifact_cleanup(
                error,
                &parent,
                &staged_name,
                &staged_path,
                "staging file",
                staged_file,
            ));
        }
    };
    drop(staged_file);

    let (backup_path, backup_name, backup_identity, original_digest, backup_digest) =
        if let Some(original) = original.as_mut() {
            match create_unique_backup_copy(
                #[cfg(unix)]
                &parent,
                &path,
                original,
                permissions.as_ref(),
                #[cfg(unix)]
                original_security.as_ref(),
            ) {
                Ok((backup_path, backup_name, identity, digest)) => (
                    backup_path,
                    backup_name,
                    Some(identity),
                    Some(digest),
                    Some(digest),
                ),
                Err(error) => {
                    let error = anyhow::Error::new(error)
                        .context(format!("failed to create backup for {}", path.display()));
                    return Err(with_snapshot_cleanup(
                        error,
                        &parent,
                        &staged_name,
                        &staged_path,
                        staged_identity,
                        staged_digest,
                        "staging file",
                    ));
                }
            }
        } else {
            let backup_path = unique_sibling_path(&path, "bak");
            let backup_name = backup_path
                .file_name()
                .expect("sibling path always has a file name")
                .to_os_string();
            (backup_path, backup_name, None, None, None)
        };

    #[cfg(not(unix))]
    let _ = backup_name;

    let target = StagedWriteTarget {
        path,
        staged_path,
        backup_path,
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        target_name,
        #[cfg(unix)]
        staged_name,
        #[cfg(unix)]
        backup_name,
        existed,
        original_identity: metadata.as_ref().map(snapshot_identity).transpose()?,
        original_digest,
        #[cfg(unix)]
        original_security,
        staged_identity,
        staged_digest,
        backup_identity,
        backup_digest,
        #[cfg(unix)]
        publication_state_uncertain: Cell::new(false),
    };
    if let Err(error) = sync_parent_directory(
        #[cfg(unix)]
        &target.parent,
        &target.path,
    ) {
        let mut cleanup_errors = Vec::new();
        cleanup_unapplied_write(&target, &mut cleanup_errors);
        let sync_error = anyhow::Error::new(error).context(format!(
            "failed to sync staged write for {}",
            target.path.display()
        ));
        if cleanup_errors.is_empty() {
            return Err(sync_error);
        }
        return Err(sync_error.context(format!(
            "staging cleanup also failed: {}",
            cleanup_errors.join("; ")
        )));
    }

    Ok(target)
}

#[cfg(unix)]
fn original_target_snapshot(
    target: &StagedWriteTarget,
) -> io::Result<Option<ExpectedFileSnapshot>> {
    if !target.existed {
        return Ok(None);
    }
    let identity = target.original_identity.ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("missing original identity for {}", target.path.display()),
        )
    })?;
    let digest = target.original_digest.ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidData,
            format!("missing original digest for {}", target.path.display()),
        )
    })?;
    Ok(Some(ExpectedFileSnapshot {
        identity,
        digest,
        description: "target",
    }))
}

#[cfg(unix)]
fn apply_staged_write(target: &StagedWriteTarget) -> io::Result<()> {
    #[cfg(unix)]
    validate_parent_anchor(&target.parent)?;
    validate_target_unchanged(target)?;
    validate_file_snapshot(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.staged_name,
        &target.staged_path,
        target.staged_identity,
        target.staged_digest,
        "staging file",
    )?;
    if let (Some(identity), Some(digest)) = (target.backup_identity, target.backup_digest) {
        validate_file_snapshot(
            #[cfg(unix)]
            &target.parent,
            #[cfg(unix)]
            &target.backup_name,
            &target.backup_path,
            identity,
            digest,
            "backup",
        )?;
    }
    #[cfg(test)]
    if target.existed {
        run_before_existing_target_replace_hook(&target.path);
    }
    #[cfg(unix)]
    {
        let expectation = match original_target_snapshot(target)? {
            Some(snapshot) => ReplaceTargetExpectation::Existing {
                snapshot,
                security: target.original_security.as_ref().ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        format!("missing original security for {}", target.path.display()),
                    )
                })?,
            },
            None => ReplaceTargetExpectation::Missing,
        };
        replace_file(
            ReplaceFileRequest {
                parent: &target.parent,
                source_name: &target.staged_name,
                target_name: &target.target_name,
                source_path: &target.staged_path,
                target_path: &target.path,
                publication_state_uncertain: Some(&target.publication_state_uncertain),
            },
            expectation,
        )?;
    }
    #[cfg(not(unix))]
    replace_file(&target.staged_path, &target.path)?;
    finish_applied_write(target)
}

#[cfg(unix)]
fn finish_applied_write(target: &StagedWriteTarget) -> io::Result<()> {
    let sync_result = sync_parent_directory(
        #[cfg(unix)]
        &target.parent,
        &target.path,
    );
    // Check the user-visible pathname after the anchored mutation as well as
    // before it. The held dirfd remains authoritative for any rollback.
    #[cfg(unix)]
    let parent_result = validate_parent_anchor(&target.parent);

    sync_result?;
    #[cfg(unix)]
    parent_result?;
    Ok(())
}

#[cfg(unix)]
fn rollback_failed_write(targets: &[StagedWriteTarget], failed_index: usize) -> Vec<String> {
    let mut errors = Vec::new();

    for target in targets[..failed_index].iter().rev() {
        rollback_applied_write(target, &mut errors);
    }
    rollback_failed_target(&targets[failed_index], &mut errors);
    for target in &targets[failed_index + 1..] {
        cleanup_unapplied_write(target, &mut errors);
    }

    errors
}

#[cfg(unix)]
fn rollback_applied_write(target: &StagedWriteTarget, errors: &mut Vec<String>) {
    if target.existed {
        if let Err(error) = validate_file_snapshot(
            #[cfg(unix)]
            &target.parent,
            #[cfg(unix)]
            &target.target_name,
            &target.path,
            target.staged_identity,
            target.staged_digest,
            "applied target",
        ) {
            errors.push(error.to_string());
            return;
        }
        let Some(backup_identity) = target.backup_identity else {
            errors.push(format!(
                "missing backup identity for {}",
                target.path.display()
            ));
            return;
        };
        let Some(backup_digest) = target.backup_digest else {
            errors.push(format!(
                "missing backup digest for {}",
                target.path.display()
            ));
            return;
        };
        if let Err(error) = validate_file_snapshot(
            #[cfg(unix)]
            &target.parent,
            #[cfg(unix)]
            &target.backup_name,
            &target.backup_path,
            backup_identity,
            backup_digest,
            "backup",
        ) {
            errors.push(error.to_string());
            return;
        }
        match target_symlink_metadata(
            #[cfg(unix)]
            &target.parent,
            #[cfg(unix)]
            &target.backup_name,
            &target.backup_path,
        ) {
            Ok(_) => {
                #[cfg(unix)]
                let original_security = match target.original_security.as_ref() {
                    Some(security) => security,
                    None => {
                        errors.push(format!(
                            "missing original security for {}",
                            target.path.display()
                        ));
                        return;
                    }
                };
                #[cfg(unix)]
                let replace_result = replace_file(
                    ReplaceFileRequest {
                        parent: &target.parent,
                        source_name: &target.backup_name,
                        target_name: &target.target_name,
                        source_path: &target.backup_path,
                        target_path: &target.path,
                        publication_state_uncertain: None,
                    },
                    ReplaceTargetExpectation::Existing {
                        snapshot: ExpectedFileSnapshot {
                            identity: target.staged_identity,
                            digest: target.staged_digest,
                            description: "applied target",
                        },
                        security: original_security,
                    },
                );
                #[cfg(not(unix))]
                let replace_result = replace_file(&target.backup_path, &target.path);
                if let Err(error) = replace_result {
                    errors.push(format!(
                        "failed to restore backup {} to {}: {error}",
                        target.backup_path.display(),
                        target.path.display()
                    ));
                } else if let Err(error) = sync_parent_directory(
                    #[cfg(unix)]
                    &target.parent,
                    &target.path,
                ) {
                    errors.push(format!(
                        "failed to sync restored target {}: {error}",
                        target.path.display()
                    ));
                }
            }
            Err(error) => errors.push(format!(
                "failed to inspect backup {}: {error}",
                target.backup_path.display()
            )),
        }
    } else {
        remove_applied_new_target(target, errors);
    }
}

#[cfg(unix)]
fn remove_applied_new_target(target: &StagedWriteTarget, errors: &mut Vec<String>) {
    remove_file_snapshot_for_cleanup(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.target_name,
        &target.path,
        target.staged_identity,
        target.staged_digest,
        "newly published target",
        errors,
    );
}

#[cfg(unix)]
fn rollback_failed_target(target: &StagedWriteTarget, errors: &mut Vec<String>) {
    #[cfg(unix)]
    if target.publication_state_uncertain.get() {
        errors.push(format!(
            "rollback skipped for {} because publication or recovery-artifact state is uncertain; target and transaction artifacts were preserved",
            target.path.display()
        ));
        return;
    }

    remove_file_snapshot_for_cleanup(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.staged_name,
        &target.staged_path,
        target.staged_identity,
        target.staged_digest,
        "staging file",
        errors,
    );

    if !target.existed {
        remove_applied_new_target(target, errors);
        return;
    }
    let target_exists_for_restore = match target_symlink_metadata(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.target_name,
        &target.path,
    ) {
        Ok(metadata)
            if target.original_identity.is_some_and(|identity| {
                snapshot_identity(&metadata).is_ok_and(|actual| actual == identity)
            }) =>
        {
            remove_backup_for_cleanup(target, errors);
            return;
        }
        Ok(metadata)
            if metadata.file_type().is_file()
                && snapshot_identity(&metadata)
                    .is_ok_and(|actual| actual == target.staged_identity) =>
        {
            if let Err(error) = validate_file_snapshot(
                #[cfg(unix)]
                &target.parent,
                #[cfg(unix)]
                &target.target_name,
                &target.path,
                target.staged_identity,
                target.staged_digest,
                "applied target",
            ) {
                errors.push(error.to_string());
                return;
            }
            true
        }
        Ok(_) => {
            errors.push(format!(
                "refusing to overwrite concurrently replaced target {} during rollback",
                target.path.display()
            ));
            return;
        }
        Err(error) if error.kind() == ErrorKind::NotFound => false,
        Err(error) => {
            errors.push(format!(
                "failed to inspect target {} during rollback: {error}",
                target.path.display()
            ));
            return;
        }
    };
    let Some(backup_identity) = target.backup_identity else {
        errors.push(format!(
            "missing backup identity for {}",
            target.path.display()
        ));
        return;
    };
    let Some(backup_digest) = target.backup_digest else {
        errors.push(format!(
            "missing backup digest for {}",
            target.path.display()
        ));
        return;
    };
    if let Err(error) = validate_file_snapshot(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.backup_name,
        &target.backup_path,
        backup_identity,
        backup_digest,
        "backup",
    ) {
        errors.push(error.to_string());
        return;
    }
    match target_symlink_metadata(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.backup_name,
        &target.backup_path,
    ) {
        Ok(_) => {
            #[cfg(unix)]
            let expectation = if target_exists_for_restore {
                ReplaceTargetExpectation::Existing {
                    snapshot: ExpectedFileSnapshot {
                        identity: target.staged_identity,
                        digest: target.staged_digest,
                        description: "applied target",
                    },
                    security: match target.original_security.as_ref() {
                        Some(security) => security,
                        None => {
                            errors.push(format!(
                                "missing original security for {}",
                                target.path.display()
                            ));
                            return;
                        }
                    },
                }
            } else {
                ReplaceTargetExpectation::Missing
            };
            #[cfg(unix)]
            let replace_result = replace_file(
                ReplaceFileRequest {
                    parent: &target.parent,
                    source_name: &target.backup_name,
                    target_name: &target.target_name,
                    source_path: &target.backup_path,
                    target_path: &target.path,
                    publication_state_uncertain: None,
                },
                expectation,
            );
            #[cfg(not(unix))]
            let replace_result = replace_file(&target.backup_path, &target.path);
            if let Err(error) = replace_result {
                errors.push(format!(
                    "failed to restore backup {} to {}: {error}",
                    target.backup_path.display(),
                    target.path.display()
                ));
            } else if let Err(error) = sync_parent_directory(
                #[cfg(unix)]
                &target.parent,
                &target.path,
            ) {
                errors.push(format!(
                    "failed to sync restored target {}: {error}",
                    target.path.display()
                ));
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => errors.push(format!(
            "failed to inspect backup {}: {error}",
            target.backup_path.display()
        )),
    }
}

#[cfg(unix)]
fn cleanup_unapplied_write(target: &StagedWriteTarget, errors: &mut Vec<String>) {
    remove_file_snapshot_for_cleanup(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.staged_name,
        &target.staged_path,
        target.staged_identity,
        target.staged_digest,
        "staging file",
        errors,
    );
    if target.existed {
        remove_backup_for_cleanup(target, errors);
    }
}

#[cfg(unix)]
fn remove_backup_for_cleanup(target: &StagedWriteTarget, errors: &mut Vec<String>) {
    let (Some(identity), Some(digest)) = (target.backup_identity, target.backup_digest) else {
        errors.push(format!(
            "missing backup snapshot for {}",
            target.backup_path.display()
        ));
        return;
    };
    remove_file_snapshot_for_cleanup(
        #[cfg(unix)]
        &target.parent,
        #[cfg(unix)]
        &target.backup_name,
        &target.backup_path,
        identity,
        digest,
        "backup",
        errors,
    );
}

#[cfg(unix)]
fn remove_file_snapshot_for_cleanup(
    #[cfg(unix)] parent: &AnchoredDirectory,
    #[cfg(unix)] name: &OsStr,
    path: &FsPath,
    expected_identity: FileIdentity,
    expected_digest: [u8; 32],
    description: &str,
    errors: &mut Vec<String>,
) {
    match target_symlink_metadata(
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        name,
        path,
    ) {
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(error) => {
            errors.push(format!(
                "failed to inspect {description} {} before cleanup: {error}",
                path.display()
            ));
            return;
        }
        Ok(_) => {}
    }
    if let Err(error) = validate_file_snapshot(
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        name,
        path,
        expected_identity,
        expected_digest,
        description,
    ) {
        errors.push(error.to_string());
        return;
    }
    #[cfg(test)]
    run_before_artifact_cleanup_hook(path);
    if let Err(error) = validate_file_snapshot(
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        name,
        path,
        expected_identity,
        expected_digest,
        description,
    ) {
        errors.push(format!(
            "refusing to remove {description} {} after it changed before cleanup: {error}",
            path.display()
        ));
        return;
    }
    #[cfg(test)]
    run_after_artifact_validation_hook(path);

    let quarantined = match move_file_to_cleanup_quarantine(parent, name, path, description) {
        Ok(quarantined) => quarantined,
        Err(error) => {
            errors.push(error.to_string());
            return;
        }
    };
    if let Err(error) = validate_file_snapshot(
        &quarantined.directory,
        &quarantined.name,
        &quarantined.path,
        expected_identity,
        expected_digest,
        description,
    ) {
        errors.push(format!(
            "refusing to remove quarantined {description}; the public entry {} was moved to retained recovery path {} but no longer matches the validated snapshot: {error}",
            path.display(),
            quarantined.path.display()
        ));
        return;
    }
    remove_file_for_cleanup(parent, &quarantined, description, errors);
}

#[cfg(unix)]
fn remove_file_for_cleanup(
    parent: &AnchoredDirectory,
    quarantined: &QuarantinedFile,
    description: &str,
    errors: &mut Vec<String>,
) {
    match remove_quarantined_file(parent, quarantined) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => errors.push(format!(
            "quarantined {description} disappeared before cleanup {}; no replacement was removed",
            quarantined.path.display()
        )),
        Err(error) => errors.push(format!(
            "failed to remove quarantined {description} {}; recovery artifact was retained when possible: {error}",
            quarantined.path.display()
        )),
    }
}

#[cfg(unix)]
fn with_snapshot_cleanup(
    error: anyhow::Error,
    parent: &AnchoredDirectory,
    name: &OsStr,
    path: &FsPath,
    expected_identity: FileIdentity,
    expected_digest: [u8; 32],
    description: &str,
) -> anyhow::Error {
    let mut cleanup_errors = Vec::new();
    remove_file_snapshot_for_cleanup(
        parent,
        name,
        path,
        expected_identity,
        expected_digest,
        description,
        &mut cleanup_errors,
    );
    if cleanup_errors.is_empty() {
        error
    } else {
        error.context(format!(
            "staging cleanup also failed: {}",
            cleanup_errors.join("; ")
        ))
    }
}

#[cfg(unix)]
fn with_open_artifact_cleanup(
    error: anyhow::Error,
    parent: &AnchoredDirectory,
    name: &OsStr,
    path: &FsPath,
    description: &str,
    mut file: fs::File,
) -> anyhow::Error {
    let snapshot = snapshot_open_artifact_for_cleanup(&mut file);
    drop(file);
    match snapshot {
        Ok((identity, digest)) => {
            with_snapshot_cleanup(error, parent, name, path, identity, digest, description)
        }
        Err(snapshot_error) => with_partial_cleanup(error, path, description, &snapshot_error),
    }
}

#[cfg(unix)]
fn with_partial_cleanup(
    error: anyhow::Error,
    path: &FsPath,
    description: &str,
    snapshot_error: &io::Error,
) -> anyhow::Error {
    error.context(format!(
        "retained partial {description} {} because stable cleanup identity could not be established: {snapshot_error}",
        path.display()
    ))
}

#[cfg(unix)]
fn reject_duplicate_write_targets<'a>(files: impl IntoIterator<Item = &'a PathBuf>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for path in files {
        let identity = write_target_identity(path);
        if !seen.insert(identity) {
            anyhow::bail!("duplicate write target {}", path.display());
        }
    }
    Ok(())
}

/// Returns the directory that contains `path`.
///
/// `FsPath::parent` yields `Some("")` — not `None` — for a bare relative file
/// name such as `source.lisp`, and the empty path cannot be opened or
/// canonicalized. Both the empty and the absent parent denote the current
/// directory, so they collapse onto `.` here.
#[cfg(unix)]
fn write_target_parent(path: &FsPath) -> &FsPath {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => FsPath::new("."),
    }
}

#[cfg(unix)]
fn write_target_identity(path: &FsPath) -> PathBuf {
    if let Ok(identity) = fs::canonicalize(path) {
        return identity;
    }

    let Some(file_name) = path.file_name() else {
        return path.to_path_buf();
    };
    fs::canonicalize(write_target_parent(path))
        .ok()
        .map_or_else(|| path.to_path_buf(), |parent| parent.join(file_name))
}

#[cfg(unix)]
fn cleanup_unapplied_writes(targets: &[StagedWriteTarget]) -> Vec<String> {
    let mut errors = Vec::new();
    for target in targets {
        cleanup_unapplied_write(target, &mut errors);
    }
    errors
}

#[cfg(unix)]
fn open_anchored_parent(path: &FsPath) -> Result<(AnchoredDirectory, OsString)> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let target_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("write target has no file name: {}", path.display()))?
        .to_os_string();
    let display_path = write_target_parent(path).to_path_buf();
    let handle = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&display_path)
        .with_context(|| format!("failed to open parent directory {}", display_path.display()))?;
    let metadata = handle.metadata().with_context(|| {
        format!(
            "failed to inspect parent directory {}",
            display_path.display()
        )
    })?;
    anyhow::ensure!(
        metadata.is_dir(),
        "write target parent is not a directory: {}",
        display_path.display()
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "refusing writable parent directory {}",
        display_path.display()
    );
    let identity = file_identity(&metadata)?;
    Ok((
        AnchoredDirectory {
            handle: cap_std::fs::Dir::from_std_file(handle),
            display_path,
            identity,
        },
        target_name,
    ))
}

#[cfg(unix)]
fn anchored_directory_from_retained(
    display_path: &FsPath,
    parent_dir: &Arc<cap_std::fs::Dir>,
    file_name: &OsStr,
) -> Result<AnchoredDirectory> {
    use cap_std::fs::PermissionsExt;

    let expected_name = display_path.file_name().ok_or_else(|| {
        anyhow::anyhow!("write target has no file name: {}", display_path.display())
    })?;
    anyhow::ensure!(
        expected_name == file_name,
        "anchored target name does not match display path {}",
        display_path.display()
    );
    let parent_display_path = write_target_parent(display_path).to_path_buf();
    let metadata = parent_dir.dir_metadata().with_context(|| {
        format!(
            "failed to inspect retained parent directory {}",
            parent_display_path.display()
        )
    })?;
    anyhow::ensure!(
        metadata.is_dir(),
        "write target parent is not a directory: {}",
        parent_display_path.display()
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o022 == 0,
        "refusing writable parent directory {}",
        parent_display_path.display()
    );
    let identity = FilesystemIdentity::from_cap(&metadata).ok_or_else(|| {
        anyhow::anyhow!(
            "stable filesystem identity is unavailable for retained parent directory {}",
            parent_display_path.display()
        )
    })?;
    let parent = AnchoredDirectory {
        handle: parent_dir.try_clone().with_context(|| {
            format!(
                "failed to retain parent directory {}",
                parent_display_path.display()
            )
        })?,
        display_path: parent_display_path,
        identity,
    };
    validate_parent_anchor(&parent).with_context(|| {
        format!(
            "retained parent no longer matches {}",
            parent.display_path.display()
        )
    })?;
    Ok(parent)
}

#[cfg(unix)]
fn validate_parent_anchor(parent: &AnchoredDirectory) -> io::Result<()> {
    let metadata = fs::symlink_metadata(&parent.display_path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || file_identity(&metadata)? != parent.identity
    {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "refusing replaced parent directory {}",
                parent.display_path.display()
            ),
        ));
    }
    Ok(())
}

#[allow(unsafe_code)]
#[cfg(unix)]
fn open_cleanup_quarantine(parent: &AnchoredDirectory) -> io::Result<AnchoredDirectory> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd as _, FromRawFd as _};
    use std::os::unix::ffi::OsStrExt as _;

    validate_parent_anchor(parent)?;

    let name = CString::new(OsStr::new(CLEANUP_QUARANTINE_NAME).as_bytes()).map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "cleanup quarantine name contains NUL",
        )
    })?;
    let parent_fd = parent.handle.as_raw_fd();
    let created = match unsafe {
        libc::mkdirat(
            parent_fd,
            name.as_ptr(),
            CLEANUP_QUARANTINE_MODE as libc::mode_t,
        )
    } {
        0 => true,
        _ => {
            let error = io::Error::last_os_error();
            if error.kind() == ErrorKind::AlreadyExists {
                false
            } else {
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "failed to create cleanup quarantine under {}: {error}",
                        parent.display_path.display()
                    ),
                ));
            }
        }
    };

    let fd = unsafe {
        libc::openat(
            parent_fd,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!(
                "failed to open cleanup quarantine without following links under {}: {error}",
                parent.display_path.display()
            ),
        ));
    }
    let file = unsafe { fs::File::from_raw_fd(fd) };
    if created
        && unsafe { libc::fchmod(file.as_raw_fd(), CLEANUP_QUARANTINE_MODE as libc::mode_t) } != 0
    {
        let error = io::Error::last_os_error();
        return Err(io::Error::new(
            error.kind(),
            format!(
                "failed to set cleanup quarantine mode under {}: {error}",
                parent.display_path.display()
            ),
        ));
    }

    let handle = cap_std::fs::Dir::from_std_file(file);
    let metadata = handle.dir_metadata()?;
    let identity = FilesystemIdentity::from_cap(&metadata).ok_or_else(|| {
        io::Error::new(
            ErrorKind::Unsupported,
            "stable filesystem identity is unavailable for cleanup quarantine",
        )
    })?;
    let quarantine = AnchoredDirectory {
        handle,
        display_path: parent.display_path.join(CLEANUP_QUARANTINE_NAME),
        identity,
    };
    validate_cleanup_quarantine_anchor(parent, &quarantine)?;
    Ok(quarantine)
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn effective_user_id() -> u32 {
    // SAFETY: geteuid has no preconditions and does not dereference memory.
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn validate_cleanup_quarantine_anchor(
    parent: &AnchoredDirectory,
    quarantine: &AnchoredDirectory,
) -> io::Result<()> {
    use cap_std::fs::{MetadataExt as _, PermissionsExt as _};

    validate_parent_anchor(parent)?;
    let entry_metadata = parent
        .handle
        .symlink_metadata(OsStr::new(CLEANUP_QUARANTINE_NAME))?;
    let retained_metadata = quarantine.handle.dir_metadata()?;
    let entry_identity = FilesystemIdentity::from_cap(&entry_metadata);
    let retained_identity = FilesystemIdentity::from_cap(&retained_metadata);
    let entry_is_valid = !entry_metadata.file_type().is_symlink()
        && entry_metadata.is_dir()
        && entry_metadata.permissions().mode() & 0o7777 == CLEANUP_QUARANTINE_MODE
        && entry_identity == Some(quarantine.identity);
    let retained_is_valid = retained_metadata.is_dir()
        && retained_metadata.permissions().mode() & 0o7777 == CLEANUP_QUARANTINE_MODE
        && retained_identity == Some(quarantine.identity);
    if !entry_is_valid || !retained_is_valid {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "refusing changed cleanup quarantine {}",
                quarantine.display_path.display()
            ),
        ));
    }

    let effective_owner = effective_user_id();
    if entry_metadata.uid() != effective_owner || retained_metadata.uid() != effective_owner {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!(
                "refusing cleanup quarantine {} not owned by the effective user",
                quarantine.display_path.display()
            ),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn move_file_to_cleanup_quarantine(
    parent: &AnchoredDirectory,
    source_name: &OsStr,
    source_path: &FsPath,
    description: &str,
) -> io::Result<QuarantinedFile> {
    let quarantine = open_cleanup_quarantine(parent).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "could not open a trusted cleanup quarantine for {description} at public path {}; the public recovery entry was not moved or removed: {error}",
                source_path.display()
            ),
        )
    })?;

    for _ in 0..UNIQUE_SIBLING_ATTEMPTS {
        validate_cleanup_quarantine_anchor(parent, &quarantine)?;
        let sequence = STAGED_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let quarantine_name = OsString::from(format!("artifact-{}-{sequence}", std::process::id()));
        let quarantine_path = quarantine.display_path.join(&quarantine_name);
        match rename_directory_entry_between_directories_no_replace(
            parent,
            source_name,
            &quarantine,
            &quarantine_name,
        ) {
            Ok(()) => {
                if let Err(error) = validate_cleanup_quarantine_anchor(parent, &quarantine) {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "moved {description} out of public path {} but cleanup quarantine {} changed; recovery location is uncertain and no file was removed: {error}",
                            source_path.display(),
                            quarantine_path.display()
                        ),
                    ));
                }
                if let Err(error) = sync_parent_directory(parent, source_path) {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "moved {description} from public path {} to retained recovery path {} but failed to sync the public directory; no file was removed: {error}",
                            source_path.display(),
                            quarantine_path.display()
                        ),
                    ));
                }
                if let Err(error) = sync_parent_directory(&quarantine, &quarantine.display_path) {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "moved {description} from public path {} to retained recovery path {} but failed to sync the cleanup quarantine; no file was removed: {error}",
                            source_path.display(),
                            quarantine_path.display()
                        ),
                    ));
                }
                return Ok(QuarantinedFile {
                    directory: quarantine,
                    name: quarantine_name,
                    path: quarantine_path,
                });
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "failed to move {description} from public path {} into retained cleanup quarantine {}; public recovery entry was not removed: {error}",
                        source_path.display(),
                        quarantine.display_path.display()
                    ),
                ));
            }
        }
    }

    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        format!(
            "cleanup quarantine collision limit reached for {description} {}; public recovery entry was not removed",
            source_path.display()
        ),
    ))
}

#[cfg(unix)]
fn remove_quarantined_file(
    parent: &AnchoredDirectory,
    quarantined: &QuarantinedFile,
) -> io::Result<()> {
    validate_cleanup_quarantine_anchor(parent, &quarantined.directory)?;
    // Unix has no expected-inode unlink; mode 0700 cannot exclude an active same-UID process.
    quarantined
        .directory
        .handle
        .remove_file(&quarantined.name)?;
    sync_parent_directory(&quarantined.directory, &quarantined.path)
}

#[cfg(unix)]
fn target_symlink_metadata(
    parent: &AnchoredDirectory,
    name: &OsStr,
    _path: &FsPath,
) -> io::Result<cap_std::fs::Metadata> {
    parent.handle.symlink_metadata(name)
}

#[cfg(unix)]
fn snapshot_identity(metadata: &cap_std::fs::Metadata) -> io::Result<FileIdentity> {
    FilesystemIdentity::from_cap(metadata).ok_or_else(stable_identity_unavailable)
}

#[cfg(unix)]
fn create_unique_sibling_file(
    #[cfg(unix)] parent: &AnchoredDirectory,
    path: &FsPath,
    suffix: &str,
) -> io::Result<CreatedSibling> {
    for _ in 0..UNIQUE_SIBLING_ATTEMPTS {
        let candidate = unique_sibling_path(path, suffix);
        let name = candidate
            .file_name()
            .expect("sibling path always has a file name")
            .to_os_string();
        match open_exclusive_file_at(
            #[cfg(unix)]
            parent,
            #[cfg(unix)]
            &name,
            &candidate,
        ) {
            Ok(file) => {
                return Ok(CreatedSibling {
                    path: candidate,
                    name,
                    file,
                });
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        format!("could not allocate a unique sibling for {}", path.display()),
    ))
}

#[cfg(unix)]
fn open_exclusive_file_at(
    #[cfg(unix)] parent: &AnchoredDirectory,
    #[cfg(unix)] name: &OsStr,
    _path: &FsPath,
) -> io::Result<fs::File> {
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;

        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        parent
            .handle
            .open_with(name, &options)
            .map(cap_std::fs::File::into_std)
    }
    #[cfg(not(unix))]
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        options.open(_path)
    }
}

#[cfg(unix)]
fn open_existing_file_at(
    #[cfg(unix)] parent: &AnchoredDirectory,
    #[cfg(unix)] name: &OsStr,
    _path: &FsPath,
) -> io::Result<fs::File> {
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;

        let mut options = cap_std::fs::OpenOptions::new();
        options
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC);
        parent
            .handle
            .open_with(name, &options)
            .map(cap_std::fs::File::into_std)
    }
    #[cfg(not(unix))]
    {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        options.open(_path)
    }
}

#[cfg(all(test, unix))]
fn open_exclusive_file(path: &FsPath) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

#[cfg(all(test, unix))]
#[allow(dead_code)]
fn open_existing_file(path: &FsPath) -> io::Result<fs::File> {
    fs::File::open(path)
}

fn file_identity(metadata: &fs::Metadata) -> io::Result<FileIdentity> {
    FilesystemIdentity::from_std(metadata).ok_or_else(stable_identity_unavailable)
}

fn stable_identity_unavailable() -> io::Error {
    io::Error::new(
        ErrorKind::Unsupported,
        "stable filesystem identity is unavailable",
    )
}

#[cfg(unix)]
fn validate_target_unchanged(target: &StagedWriteTarget) -> io::Result<()> {
    match target.original_identity {
        Some(identity) => {
            let digest = target.original_digest.ok_or_else(|| {
                io::Error::new(ErrorKind::InvalidData, "missing original content digest")
            })?;
            let file = validate_file_snapshot(
                #[cfg(unix)]
                &target.parent,
                #[cfg(unix)]
                &target.target_name,
                &target.path,
                identity,
                digest,
                "target",
            )?;
            #[cfg(unix)]
            {
                let opened_metadata = file.metadata()?;
                use std::os::unix::fs::MetadataExt;
                if opened_metadata.nlink() != 1 {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        format!("refusing hard-linked target {}", target.path.display()),
                    ));
                }
                let current = read_security_metadata(&file)?;
                if target.original_security.as_ref() != Some(&current) {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "refusing target with changed security metadata {}",
                            target.path.display()
                        ),
                    ));
                }
            }
            Ok(())
        }
        None => match target_symlink_metadata(
            #[cfg(unix)]
            &target.parent,
            #[cfg(unix)]
            &target.target_name,
            &target.path,
        ) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(io::Error::new(
                ErrorKind::AlreadyExists,
                format!(
                    "refusing to overwrite target created after staging {}",
                    target.path.display()
                ),
            )),
            Err(error) => Err(error),
        },
    }
}

/// Extended-attribute support is filesystem-dependent: tmpfs without
/// `CONFIG_TMPFS_XATTR`, some network filesystems, and sandboxed build
/// environments reject `*listxattr`/`*getxattr` with `ENOTSUP`/`EOPNOTSUPP`
/// (or `ENOSYS`). Preserving security metadata is best-effort, so treat "this
/// filesystem has no xattr support" as "no extended attributes" instead of
/// failing every write.
#[cfg(unix)]
fn is_xattr_unsupported(error: &io::Error) -> bool {
    if error.kind() == ErrorKind::Unsupported {
        return true;
    }
    matches!(
        error.raw_os_error(),
        Some(code) if code == libc::ENOTSUP || code == libc::EOPNOTSUPP || code == libc::ENOSYS
    )
}

#[cfg(unix)]
fn list_xattr_names(file: &fs::File) -> io::Result<Vec<OsString>> {
    use xattr::FileExt;

    match file.list_xattr() {
        Ok(names) => Ok(names.collect()),
        Err(error) if is_xattr_unsupported(&error) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn read_security_metadata(file: &fs::File) -> io::Result<SecurityMetadata> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use xattr::FileExt;

    let metadata = file.metadata()?;
    let mut names = list_xattr_names(file)?;
    names.sort_unstable();
    let mut xattrs = Vec::with_capacity(names.len());
    for name in names {
        let value = file.get_xattr(&name)?.ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!("extended attribute {name:?} changed while being read"),
            )
        })?;
        xattrs.push((name, value));
    }
    Ok(SecurityMetadata {
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.permissions().mode() & 0o7777,
        xattrs,
        #[cfg(target_os = "macos")]
        acl: super::macos_acl::read_acl(file)?,
    })
}

#[cfg(unix)]
fn apply_security_metadata(
    destination: &fs::File,
    permissions: Option<&fs::Permissions>,
    expected: &SecurityMetadata,
) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    use xattr::FileExt;

    if let Some(permissions) = permissions {
        destination.set_permissions(permissions.clone())?;
    }
    let metadata = destination.metadata()?;
    if metadata.uid() != expected.uid || metadata.gid() != expected.gid {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!(
                "cannot preserve owner/group (expected {}:{}, got {}:{})",
                expected.uid,
                expected.gid,
                metadata.uid(),
                metadata.gid()
            ),
        ));
    }

    let expected_names = expected
        .xattrs
        .iter()
        .map(|(name, _)| name)
        .collect::<BTreeSet<_>>();
    for name in list_xattr_names(destination)? {
        if !expected_names.contains(&name) {
            destination.remove_xattr(&name)?;
        }
    }
    for (name, value) in &expected.xattrs {
        destination.set_xattr(name, value)?;
    }
    #[cfg(target_os = "macos")]
    super::macos_acl::write_acl(destination, expected.acl.as_ref())?;

    let actual = read_security_metadata(destination)?;
    if &actual != expected {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "security metadata did not round-trip exactly",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_path_identity(
    #[cfg(unix)] parent: &AnchoredDirectory,
    #[cfg(unix)] name: &OsStr,
    path: &FsPath,
    expected: FileIdentity,
    description: &str,
) -> io::Result<()> {
    let metadata = target_symlink_metadata(
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        name,
        path,
    )?;
    if metadata.file_type().is_symlink()
        || !metadata.file_type().is_file()
        || snapshot_identity(&metadata)? != expected
    {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("refusing replaced {description} {}", path.display()),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_file_snapshot(
    #[cfg(unix)] parent: &AnchoredDirectory,
    #[cfg(unix)] name: &OsStr,
    path: &FsPath,
    expected_identity: FileIdentity,
    expected_digest: [u8; 32],
    description: &str,
) -> io::Result<fs::File> {
    validate_path_identity(
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        name,
        path,
        expected_identity,
        description,
    )?;
    #[cfg(test)]
    run_before_existing_file_open_hook(path);
    let mut file = open_existing_file_at(
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        name,
        path,
    )?;
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() || file_identity(&metadata)? != expected_identity {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("refusing replaced {description} {}", path.display()),
        ));
    }
    let actual_digest = digest_reader(&mut file)?;
    if actual_digest != expected_digest {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "refusing {description} with changed content {}",
                path.display()
            ),
        ));
    }
    validate_path_identity(
        #[cfg(unix)]
        parent,
        #[cfg(unix)]
        name,
        path,
        expected_identity,
        description,
    )?;
    if file_identity(&file.metadata()?)? != expected_identity {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("refusing replaced {description} {}", path.display()),
        ));
    }
    Ok(file)
}

#[cfg(unix)]
fn digest_reader(reader: &mut impl Read) -> io::Result<[u8; 32]> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(*hasher.finalize().as_bytes()),
            Ok(read) => {
                hasher.update(&buffer[..read]);
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
fn snapshot_open_artifact_for_cleanup(file: &mut fs::File) -> io::Result<(FileIdentity, [u8; 32])> {
    let identity = file_identity(&file.metadata()?)?;
    file.seek(SeekFrom::Start(0))?;
    let digest = digest_reader(file)?;
    Ok((identity, digest))
}

#[cfg(unix)]
fn create_unique_backup_copy(
    #[cfg(unix)] parent: &AnchoredDirectory,
    path: &FsPath,
    original: &mut fs::File,
    permissions: Option<&fs::Permissions>,
    #[cfg(unix)] security: Option<&SecurityMetadata>,
) -> io::Result<(PathBuf, OsString, FileIdentity, [u8; 32])> {
    for _ in 0..UNIQUE_SIBLING_ATTEMPTS {
        let candidate = unique_sibling_path(path, "bak");
        let name = candidate
            .file_name()
            .expect("sibling path always has a file name")
            .to_os_string();
        match open_exclusive_file_at(
            #[cfg(unix)]
            parent,
            #[cfg(unix)]
            &name,
            &candidate,
        ) {
            Ok(mut backup) => {
                let result = (|| -> io::Result<[u8; 32]> {
                    original.seek(SeekFrom::Start(0))?;
                    let mut hasher = blake3::Hasher::new();
                    let mut buffer = [0_u8; 64 * 1024];
                    loop {
                        let read = original.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        hasher.update(&buffer[..read]);
                        backup.write_all(&buffer[..read])?;
                    }
                    if let Some(permissions) = permissions {
                        backup.set_permissions(permissions.clone())?;
                    }
                    #[cfg(unix)]
                    if let Some(security) = security {
                        apply_security_metadata(&backup, permissions, security)?;
                    }
                    backup.sync_all()?;
                    Ok(*hasher.finalize().as_bytes())
                })();
                let digest = match result {
                    Ok(digest) => digest,
                    Err(error) => {
                        let snapshot = snapshot_open_artifact_for_cleanup(&mut backup);
                        drop(backup);
                        return match snapshot {
                            Ok((identity, digest)) => {
                                let mut cleanup_errors = Vec::new();
                                remove_file_snapshot_for_cleanup(
                                    parent,
                                    &name,
                                    &candidate,
                                    identity,
                                    digest,
                                    "partial backup",
                                    &mut cleanup_errors,
                                );
                                if cleanup_errors.is_empty() {
                                    Err(error)
                                } else {
                                    Err(io::Error::new(
                                        error.kind(),
                                        format!(
                                            "{error}; staging cleanup also failed: {}",
                                            cleanup_errors.join("; ")
                                        ),
                                    ))
                                }
                            }
                            Err(snapshot_error) => Err(io::Error::new(
                                error.kind(),
                                format!(
                                    "{error}; retained partial backup {} because stable cleanup identity could not be established: {snapshot_error}",
                                    candidate.display()
                                ),
                            )),
                        };
                    }
                };
                let identity = match backup
                    .metadata()
                    .and_then(|metadata| file_identity(&metadata))
                {
                    Ok(identity) => identity,
                    Err(error) => {
                        drop(backup);
                        return Err(io::Error::new(
                            error.kind(),
                            format!(
                                "{error}; retained backup {} because stable cleanup identity could not be established",
                                candidate.display()
                            ),
                        ));
                    }
                };
                return Ok((candidate, name, identity, digest));
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        format!("could not allocate a unique backup for {}", path.display()),
    ))
}

#[cfg(unix)]
fn replace_file(
    request: ReplaceFileRequest<'_>,
    expectation: ReplaceTargetExpectation<'_>,
) -> io::Result<()> {
    let ReplaceFileRequest {
        parent,
        source_name,
        target_name,
        source_path,
        target_path,
        publication_state_uncertain,
    } = request;
    if let ReplaceTargetExpectation::Existing { snapshot, security } = expectation {
        exchange_directory_entries(parent, source_name, target_name)?;
        if let Err(validation_error) =
            validate_displaced_target(parent, source_name, source_path, snapshot, security)
        {
            #[cfg(test)]
            run_before_exchange_restore_hook(target_path);
            return match exchange_directory_entries(parent, source_name, target_name) {
                Ok(()) => Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "target changed during atomic publication {}: {validation_error}",
                        target_path.display()
                    ),
                )),
                Err(restore_error) => {
                    if let Some(state) = publication_state_uncertain {
                        state.set(true);
                    }
                    Err(io::Error::other(format!(
                        "target changed during atomic publication {}; failed to restore exchange: {restore_error}; validation error: {validation_error}",
                        target_path.display()
                    )))
                }
            };
        }
        #[cfg(test)]
        run_before_artifact_cleanup_hook(source_path);
        if let Err(validation_error) =
            validate_displaced_target(parent, source_name, source_path, snapshot, security)
        {
            if let Some(state) = publication_state_uncertain {
                state.set(true);
            }
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "published target {} retained and recovery artifact {} retained after the recovery artifact changed before cleanup: {validation_error}",
                    target_path.display(),
                    source_path.display()
                ),
            ));
        }
        #[cfg(test)]
        run_after_artifact_validation_hook(source_path);

        let quarantined = match move_file_to_cleanup_quarantine(
            parent,
            source_name,
            source_path,
            "recovery artifact",
        ) {
            Ok(quarantined) => quarantined,
            Err(error) => {
                if let Some(state) = publication_state_uncertain {
                    state.set(true);
                }
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "published target {} retained but recovery artifact cleanup could not be secured: {error}",
                        target_path.display()
                    ),
                ));
            }
        };
        if let Err(validation_error) = validate_displaced_target(
            &quarantined.directory,
            &quarantined.name,
            &quarantined.path,
            snapshot,
            security,
        ) {
            if let Some(state) = publication_state_uncertain {
                state.set(true);
            }
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "published target {} retained; public recovery artifact {} moved to retained quarantine path {} but was not removed because it changed at the final cleanup boundary: {validation_error}",
                    target_path.display(),
                    source_path.display(),
                    quarantined.path.display()
                ),
            ));
        }
        if let Err(error) = remove_quarantined_file(parent, &quarantined) {
            if let Some(state) = publication_state_uncertain {
                state.set(true);
            }
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "published target {} retained; recovery artifact moved from public path {} to quarantine path {} but cleanup failed: {error}",
                    target_path.display(),
                    source_path.display(),
                    quarantined.path.display()
                ),
            ));
        }
        return Ok(());
    }

    rename_directory_entry_no_replace(parent, source_name, target_name)
}

#[cfg(unix)]
fn validate_displaced_target(
    parent: &AnchoredDirectory,
    name: &OsStr,
    path: &FsPath,
    expected: ExpectedFileSnapshot,
    expected_security: &SecurityMetadata,
) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;

    let file = validate_file_snapshot(
        parent,
        name,
        path,
        expected.identity,
        expected.digest,
        expected.description,
    )?;
    if file.metadata()?.nlink() != 1 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "refusing multiply-linked {} {}",
                expected.description,
                path.display()
            ),
        ));
    }
    if read_security_metadata(&file)? != *expected_security {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "refusing {} with changed security metadata {}",
                expected.description,
                path.display()
            ),
        ));
    }
    Ok(())
}

#[allow(unsafe_code)]
#[cfg(unix)]
fn exchange_directory_entries(
    parent: &AnchoredDirectory,
    source_name: &OsStr,
    target_name: &OsStr,
) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::ffi::CString;
        use std::os::fd::AsRawFd as _;
        use std::os::unix::ffi::OsStrExt as _;

        let source_name = CString::new(source_name.as_bytes()).map_err(|_| {
            io::Error::new(ErrorKind::InvalidInput, "source file name contains NUL")
        })?;
        let target_name = CString::new(target_name.as_bytes()).map_err(|_| {
            io::Error::new(ErrorKind::InvalidInput, "target file name contains NUL")
        })?;
        let directory_fd = parent.handle.as_raw_fd();
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                directory_fd,
                source_name.as_ptr(),
                directory_fd,
                target_name.as_ptr(),
                libc::RENAME_EXCHANGE,
            )
        };
        #[cfg(target_os = "macos")]
        let result = unsafe {
            libc::renameatx_np(
                directory_fd,
                source_name.as_ptr(),
                directory_fd,
                target_name.as_ptr(),
                libc::RENAME_SWAP,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (parent, source_name, target_name);
        Err(io::Error::new(
            ErrorKind::Unsupported,
            "atomic exchange is unsupported on this Unix platform",
        ))
    }
}

#[cfg(unix)]
fn rename_directory_entry_no_replace(
    parent: &AnchoredDirectory,
    source_name: &OsStr,
    target_name: &OsStr,
) -> io::Result<()> {
    rename_directory_entry_between_directories_no_replace(parent, source_name, parent, target_name)
}

#[allow(unsafe_code)]
#[cfg(unix)]
fn rename_directory_entry_between_directories_no_replace(
    source_parent: &AnchoredDirectory,
    source_name: &OsStr,
    target_parent: &AnchoredDirectory,
    target_name: &OsStr,
) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::ffi::CString;
        use std::os::fd::AsRawFd as _;
        use std::os::unix::ffi::OsStrExt as _;

        let source_name = CString::new(source_name.as_bytes()).map_err(|_| {
            io::Error::new(ErrorKind::InvalidInput, "source file name contains NUL")
        })?;
        let target_name = CString::new(target_name.as_bytes()).map_err(|_| {
            io::Error::new(ErrorKind::InvalidInput, "target file name contains NUL")
        })?;
        let source_directory_fd = source_parent.handle.as_raw_fd();
        let target_directory_fd = target_parent.handle.as_raw_fd();
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                source_directory_fd,
                source_name.as_ptr(),
                target_directory_fd,
                target_name.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result = unsafe {
            libc::renameatx_np(
                source_directory_fd,
                source_name.as_ptr(),
                target_directory_fd,
                target_name.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (source_parent, source_name, target_parent, target_name);
        Err(io::Error::new(
            ErrorKind::Unsupported,
            "atomic no-replace rename is unsupported on this Unix platform",
        ))
    }
}

/// Syncing a directory is a best-effort durability step, not a correctness
/// requirement: the rename it follows has already taken effect. Some
/// filesystems and directory descriptors reject `fsync` outright — Linux
/// `O_PATH` handles and sandboxed tmpfs return EBADF/EINVAL, and filesystems
/// without durable metadata return ENOTSUP/ENOSYS. Treat that class as success
/// so an otherwise-complete write is not aborted for lack of a durability hint.
#[cfg(unix)]
fn is_dir_sync_unsupported(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code)
            if code == libc::EBADF
                || code == libc::EINVAL
                || code == libc::ENOTSUP
                || code == libc::EOPNOTSUPP
                || code == libc::ENOSYS
    )
}

#[cfg(unix)]
fn sync_parent_directory(parent: &AnchoredDirectory, _path: &FsPath) -> io::Result<()> {
    match parent.handle.try_clone()?.into_std_file().sync_all() {
        Ok(()) => Ok(()),
        Err(error) if is_dir_sync_unsupported(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn unique_sibling_path(path: &FsPath, suffix: &str) -> PathBuf {
    let counter = STAGED_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("paredit");
    path.with_file_name(format!(".{file_name}.paredit-{suffix}-{pid}-{counter}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn xattr_unsupported_errors_are_classified_as_such() {
        for code in [libc::ENOTSUP, libc::EOPNOTSUPP, libc::ENOSYS] {
            assert!(is_xattr_unsupported(&io::Error::from_raw_os_error(code)));
        }
        assert!(is_xattr_unsupported(&io::Error::from(
            ErrorKind::Unsupported
        )));
        assert!(!is_xattr_unsupported(&io::Error::from_raw_os_error(
            libc::EACCES
        )));
        assert!(!is_xattr_unsupported(&io::Error::from_raw_os_error(
            libc::EPERM
        )));
    }

    #[cfg(unix)]
    #[test]
    fn directory_sync_unsupported_errors_are_classified_as_such() {
        for code in [
            libc::EBADF,
            libc::EINVAL,
            libc::ENOTSUP,
            libc::EOPNOTSUPP,
            libc::ENOSYS,
        ] {
            assert!(is_dir_sync_unsupported(&io::Error::from_raw_os_error(code)));
        }
        assert!(!is_dir_sync_unsupported(&io::Error::from_raw_os_error(
            libc::EIO
        )));
        assert!(!is_dir_sync_unsupported(&io::Error::from_raw_os_error(
            libc::ENOSPC
        )));
    }

    #[cfg(not(unix))]
    #[test]
    fn transactional_writes_fail_before_creating_any_artifacts() {
        let directory = test_directory("unsupported-transactional-write");
        let existing = directory.join("existing.lisp");
        let new = directory.join("new.lisp");
        fs::write(&existing, "(old)").expect("write existing target");

        let error = write_files_with_rollback([
            (existing.clone(), "(replacement)".to_owned()),
            (new.clone(), "(new)".to_owned()),
        ])
        .expect_err("non-Unix transactional writes must be unsupported");

        assert_eq!(
            error.downcast_ref::<io::Error>().expect("I/O error").kind(),
            ErrorKind::Unsupported
        );
        assert_eq!(
            fs::read_to_string(&existing).expect("read existing target"),
            "(old)"
        );
        assert!(!new.exists());
        let entries = fs::read_dir(&directory)
            .expect("read test directory")
            .map(|entry| entry.expect("read directory entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, [OsString::from("existing.lisp")]);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn no_replace_publication_preserves_a_concurrently_created_target() {
        let directory = test_directory("no-replace-publication-conflict");
        let target_path = directory.join("new.lisp");
        let staged =
            stage_write_target(target_path.clone(), "(new)".to_owned()).expect("stage target");
        fs::write(&target_path, "(concurrent)").expect("publish concurrent target");

        let error = replace_file(
            ReplaceFileRequest {
                parent: &staged.parent,
                source_name: &staged.staged_name,
                target_name: &staged.target_name,
                source_path: &staged.staged_path,
                target_path: &staged.path,
                publication_state_uncertain: Some(&staged.publication_state_uncertain),
            },
            ReplaceTargetExpectation::Missing,
        )
        .expect_err("concurrent target must not be replaced");

        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read_to_string(&target_path).expect("read concurrent target"),
            "(concurrent)"
        );
        assert_eq!(
            fs::read_to_string(&staged.staged_path).expect("read retained staging file"),
            "(new)"
        );

        let mut cleanup_errors = Vec::new();
        cleanup_unapplied_write(&staged, &mut cleanup_errors);
        assert!(cleanup_errors.is_empty(), "{cleanup_errors:?}");

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn bounded_text_reader_rejects_oversized_input() {
        let error = read_text_with_limit(&b"12345"[..], 4, "test input")
            .expect_err("oversized input must be rejected");

        assert!(format!("{error:#}").contains("input exceeds 4 bytes"));
    }

    #[test]
    fn bounded_text_reader_rejects_invalid_utf8() {
        let error = read_text_with_limit(&[0xff][..], 4, "test input")
            .expect_err("invalid UTF-8 must be rejected");

        assert!(format!("{error:#}").contains("not valid UTF-8"));
    }

    #[cfg(unix)]
    #[test]
    fn bounded_text_file_reader_rejects_fifo_without_blocking() {
        let directory = test_directory("bounded-reader-fifo");
        let fifo = directory.join("source.lisp");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "mkfifo must succeed");

        let error = read_text_file_with_limit(&fifo, MAX_SOURCE_INPUT_BYTES)
            .expect_err("FIFO must be rejected without waiting for a writer");

        assert!(format!("{error:#}").contains("refusing non-regular input file"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn bounded_text_file_reader_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = test_directory("bounded-reader-symlink");
        let target = directory.join("target.lisp");
        let link = directory.join("source.lisp");
        fs::write(&target, "(target)").expect("write symlink target");
        symlink(&target, &link).expect("create source symlink");

        let error = open_regular_input_file(&link).expect_err("symlink input must be rejected");

        // The raw `O_NOFOLLOW` failure is ELOOP, which reads as a link cycle;
        // the caller must instead see why the link was refused.
        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert!(
            error.to_string().contains("refusing to read symlink"),
            "unexpected error: {error}"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read symlink target"),
            "(target)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn optional_file_reader_rejects_fifo_without_blocking() {
        let directory = test_directory("optional-reader-fifo");
        let fifo = directory.join("destination.lisp");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "mkfifo must succeed");

        let error = read_file_or_empty(&fifo)
            .expect_err("FIFO must be rejected without waiting for a writer");

        assert!(format!("{error:#}").contains("refusing non-regular input file"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_fifo_swapped_between_validation_and_open_without_blocking() {
        let directory = test_directory("staging-open-race-fifo");
        let target = directory.join("target.lisp");
        let preserved = directory.join("preserved-target.lisp");
        fs::write(&target, "(third-party)").expect("write target");

        let hook_target = target.clone();
        let hook_preserved = preserved.clone();
        let operation_target = target.clone();
        let result = completes_within(move || {
            let _guard =
                install_before_existing_file_open_hook(operation_target.clone(), move || {
                    replace_regular_file_with_fifo(&hook_target, &hook_preserved)
                });
            stage_write_target(operation_target, "(paredit)".to_owned())
                .map(|_| ())
                .map_err(|error| format!("{error:#}"))
        });

        assert!(result.is_err(), "FIFO replacement must be rejected");
        assert_fifo(&target);
        assert_eq!(
            fs::read_to_string(&preserved).expect("read preserved target"),
            "(third-party)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_fifo_swapped_between_validation_and_open_without_blocking() {
        let directory = test_directory("apply-open-race-fifo");
        let target = directory.join("target.lisp");
        let preserved = directory.join("preserved-target.lisp");
        fs::write(&target, "(third-party)").expect("write target");
        let staged =
            stage_write_target(target.clone(), "(paredit)".to_owned()).expect("stage target");

        let hook_target = target.clone();
        let hook_preserved = preserved.clone();
        let operation_target = target.clone();
        let result = completes_within(move || {
            let _guard = install_before_existing_file_open_hook(operation_target, move || {
                replace_regular_file_with_fifo(&hook_target, &hook_preserved)
            });
            apply_staged_writes(vec![staged]).map_err(|error| format!("{error:#}"))
        });

        assert!(result.is_err(), "FIFO replacement must be rejected");
        assert_fifo(&target);
        assert_eq!(
            fs::read_to_string(&preserved).expect("read preserved target"),
            "(third-party)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_cleanup_preserves_fifo_swapped_before_reopen_without_blocking() {
        let directory = test_directory("rollback-cleanup-open-race-fifo");
        let target = directory.join("target.lisp");
        fs::write(&target, "(original)").expect("write target");
        let staged =
            stage_write_target(target.clone(), "(third-party)".to_owned()).expect("stage target");
        let staged_path = staged.staged_path.clone();
        let preserved = directory.join("preserved-staging.lisp");

        let hook_staged = staged_path.clone();
        let hook_preserved = preserved.clone();
        let operation_staged = staged_path.clone();
        let errors = completes_within(move || {
            let _guard = install_before_existing_file_open_hook(operation_staged, move || {
                replace_regular_file_with_fifo(&hook_staged, &hook_preserved)
            });
            let mut errors = Vec::new();
            cleanup_unapplied_write(&staged, &mut errors);
            errors
        });

        assert!(!errors.is_empty(), "replacement must be reported");
        assert_fifo(&staged_path);
        assert_eq!(
            fs::read_to_string(&preserved).expect("read preserved staging data"),
            "(third-party)"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read original target"),
            "(original)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_preserves_a_file_replaced_after_snapshot_validation() {
        let directory = test_directory("cleanup-race-replaced-staging-file");
        let target = directory.join("target.lisp");
        fs::write(&target, "(original)").expect("write target");
        let staged =
            stage_write_target(target.clone(), "(staged)".to_owned()).expect("stage target");
        let staged_path = staged.staged_path.clone();
        let preserved = directory.join("preserved-staging.lisp");

        let hook_staged = staged_path.clone();
        let hook_preserved = preserved.clone();
        let _guard = install_before_artifact_cleanup_hook(staged_path.clone(), move || {
            fs::rename(&hook_staged, &hook_preserved).expect("preserve validated staging file");
            fs::write(&hook_staged, "third-party content").expect("replace validated staging file");
        });

        let mut errors = Vec::new();
        cleanup_unapplied_write(&staged, &mut errors);

        assert!(
            errors.join("; ").contains("changed before cleanup"),
            "{errors:?}"
        );
        assert_eq!(
            fs::read_to_string(&staged_path).expect("read preserved replacement"),
            "third-party content"
        );
        assert_eq!(
            fs::read_to_string(&preserved).expect("read preserved staging data"),
            "(staged)"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read original target"),
            "(original)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn expected_writer_rejects_same_content_on_a_different_inode() {
        let directory = test_directory("expected-target-replaced-inode");
        let target = directory.join("target.lisp");
        let preserved = directory.join("parsed-target.lisp");
        fs::write(&target, "(old)").expect("write parsed target");
        let (text, expected) = read_text_file_with_expected_target(&target, MAX_SOURCE_INPUT_BYTES)
            .expect("read expected target");
        fs::rename(&target, &preserved).expect("retain parsed inode");
        fs::write(&target, &text).expect("replace target with identical content");

        let error =
            write_files_with_rollback_expected([(target.clone(), "(new)".to_owned(), expected)])
                .expect_err("different inode must be rejected");

        assert!(format!("{error:#}").contains("replaced since parsing"));
        assert_eq!(
            fs::read_to_string(&target).expect("read replacement target"),
            "(old)"
        );
        assert_eq!(
            fs::read_to_string(&preserved).expect("read parsed target"),
            "(old)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn anchored_expected_writer_writes_through_retained_parent() {
        let directory = test_directory("anchored-expected-writer");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write parsed target");
        let (_, expected) = read_text_file_with_expected_target(&target, MAX_SOURCE_INPUT_BYTES)
            .expect("read expected target");
        let parent_dir = Arc::new(
            cap_std::fs::Dir::open_ambient_dir(&directory, cap_std::ambient_authority())
                .expect("retain parent directory"),
        );

        write_files_with_rollback_expected_anchored(vec![AnchoredExpectedWrite {
            display_path: target.clone(),
            parent_dir,
            file_name: OsString::from("target.lisp"),
            content: "(new)".to_owned(),
            expected,
        }])
        .expect("write through retained parent");

        assert_eq!(
            fs::read_to_string(&target).expect("read rewritten target"),
            "(new)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn anchored_expected_writer_rejects_replaced_ambient_parent() {
        let directory = test_directory("anchored-expected-writer-replaced-parent");
        let retained_directory = directory.with_extension("retained");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write parsed target");
        let (_, expected) = read_text_file_with_expected_target(&target, MAX_SOURCE_INPUT_BYTES)
            .expect("read expected target");
        let parent_dir = Arc::new(
            cap_std::fs::Dir::open_ambient_dir(&directory, cap_std::ambient_authority())
                .expect("retain parent directory"),
        );
        fs::rename(&directory, &retained_directory).expect("move retained parent");
        fs::create_dir(&directory).expect("create replacement parent");
        fs::write(&target, "(third-party)").expect("write replacement target");

        let error = write_files_with_rollback_expected_anchored(vec![AnchoredExpectedWrite {
            display_path: target.clone(),
            parent_dir,
            file_name: OsString::from("target.lisp"),
            content: "(new)".to_owned(),
            expected,
        }])
        .expect_err("replaced ambient parent must be rejected");

        assert!(format!("{error:#}").contains("refusing replaced parent directory"));
        assert_eq!(
            fs::read_to_string(retained_directory.join("target.lisp"))
                .expect("read retained target"),
            "(old)"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read replacement target"),
            "(third-party)"
        );
        fs::remove_dir_all(directory).expect("remove replacement directory");
        fs::remove_dir_all(retained_directory).expect("remove retained directory");
    }

    fn test_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "paredit-cli-{name}-{}-{}",
            std::process::id(),
            STAGED_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create test directory");
        path
    }

    #[cfg(unix)]
    fn completes_within<T: Send + 'static>(operation: impl FnOnce() -> T + Send + 'static) -> T {
        use std::sync::mpsc;
        use std::time::Duration;

        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = sender.send(operation());
        });
        let result = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("operation must complete without blocking");
        worker.join().expect("operation thread must not panic");
        result
    }

    #[cfg(unix)]
    fn replace_regular_file_with_fifo(path: &FsPath, preserved: &FsPath) {
        fs::rename(path, preserved).expect("preserve regular file");
        let status = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "mkfifo must succeed");
    }

    #[cfg(unix)]
    fn assert_fifo(path: &FsPath) {
        use std::os::unix::fs::FileTypeExt;

        assert!(
            fs::symlink_metadata(path)
                .expect("inspect FIFO")
                .file_type()
                .is_fifo(),
            "{} must remain a FIFO",
            path.display()
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[derive(Clone, Copy)]
    enum RecoveryArtifactReplacement {
        RegularFile,
        Fifo,
        Symlink,
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn assert_changed_recovery_artifact_is_preserved(
        test_name: &str,
        replacement: RecoveryArtifactReplacement,
    ) {
        use std::os::unix::fs::symlink;

        let directory = test_directory(test_name);
        let target = directory.join("target.lisp");
        let preserved_recovery = directory.join("preserved-recovery.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target");
        let staged_path = staged.staged_path.clone();
        let backup_path = staged.backup_path.clone();

        let hook_staged = staged_path.clone();
        let hook_preserved = preserved_recovery.clone();
        let _guard = install_before_artifact_cleanup_hook(staged_path.clone(), move || {
            fs::rename(&hook_staged, &hook_preserved)
                .expect("preserve displaced recovery artifact");
            match replacement {
                RecoveryArtifactReplacement::RegularFile => {
                    fs::write(&hook_staged, "third-party content")
                        .expect("replace recovery artifact with regular file");
                }
                RecoveryArtifactReplacement::Fifo => {
                    let status = std::process::Command::new("mkfifo")
                        .arg(&hook_staged)
                        .status()
                        .expect("run mkfifo");
                    assert!(status.success(), "mkfifo must succeed");
                }
                RecoveryArtifactReplacement::Symlink => {
                    symlink(&hook_preserved, &hook_staged)
                        .expect("replace recovery artifact with symlink");
                }
            }
        });

        let error = apply_staged_writes(vec![staged])
            .expect_err("changed recovery artifact must fail closed");
        let report = format!("{error:#}");

        assert!(report.contains("published target"), "{report}");
        assert!(report.contains("recovery artifact"), "{report}");
        assert!(report.contains("rollback skipped"), "{report}");
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(new)");
        assert_eq!(
            fs::read_to_string(&preserved_recovery).expect("read preserved recovery artifact"),
            "(old)"
        );
        assert_eq!(
            fs::read_to_string(&backup_path).expect("read retained backup"),
            "(old)"
        );
        match replacement {
            RecoveryArtifactReplacement::RegularFile => assert_eq!(
                fs::read_to_string(&staged_path).expect("read replacement regular file"),
                "third-party content"
            ),
            RecoveryArtifactReplacement::Fifo => assert_fifo(&staged_path),
            RecoveryArtifactReplacement::Symlink => {
                assert!(
                    fs::symlink_metadata(&staged_path)
                        .expect("inspect replacement symlink")
                        .file_type()
                        .is_symlink()
                );
                assert_eq!(
                    fs::read_link(&staged_path).expect("read replacement symlink"),
                    preserved_recovery
                );
            }
        }

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn assert_final_validation_replacement_is_quarantined(
        test_name: &str,
        replacement: RecoveryArtifactReplacement,
    ) {
        use std::os::unix::fs::symlink;

        let directory = test_directory(test_name);
        let target = directory.join("target.lisp");
        let preserved_recovery = directory.join("preserved-recovery.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target");
        let staged_path = staged.staged_path.clone();
        let backup_path = staged.backup_path.clone();

        let hook_staged = staged_path.clone();
        let hook_preserved = preserved_recovery.clone();
        let _guard = install_after_artifact_validation_hook(staged_path.clone(), move || {
            fs::rename(&hook_staged, &hook_preserved)
                .expect("preserve validated recovery artifact");
            match replacement {
                RecoveryArtifactReplacement::RegularFile => {
                    fs::write(&hook_staged, "third-party content")
                        .expect("replace recovery artifact with regular file");
                }
                RecoveryArtifactReplacement::Fifo => {
                    let status = std::process::Command::new("mkfifo")
                        .arg(&hook_staged)
                        .status()
                        .expect("run mkfifo");
                    assert!(status.success(), "mkfifo must succeed");
                }
                RecoveryArtifactReplacement::Symlink => {
                    symlink(&hook_preserved, &hook_staged)
                        .expect("replace recovery artifact with symlink");
                }
            }
        });

        let error = apply_staged_writes(vec![staged])
            .expect_err("final-boundary replacement must fail closed");
        let report = format!("{error:#}");
        let quarantine = directory.join(CLEANUP_QUARANTINE_NAME);
        let quarantined_entries = fs::read_dir(&quarantine)
            .expect("read cleanup quarantine")
            .map(|entry| entry.expect("read quarantined entry").path())
            .collect::<Vec<_>>();

        assert!(report.contains("published target"), "{report}");
        assert!(report.contains("retained quarantine path"), "{report}");
        assert!(report.contains("rollback skipped"), "{report}");
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(new)");
        assert_eq!(
            fs::read_to_string(&preserved_recovery).expect("read preserved recovery artifact"),
            "(old)"
        );
        assert_eq!(
            fs::read_to_string(&backup_path).expect("read retained backup"),
            "(old)"
        );
        assert!(!staged_path.exists());
        assert_eq!(
            quarantined_entries.len(),
            1,
            "exactly the mismatched artifact must be retained"
        );
        let quarantined = &quarantined_entries[0];
        match replacement {
            RecoveryArtifactReplacement::RegularFile => assert_eq!(
                fs::read_to_string(quarantined).expect("read quarantined regular file"),
                "third-party content"
            ),
            RecoveryArtifactReplacement::Fifo => assert_fifo(quarantined),
            RecoveryArtifactReplacement::Symlink => {
                assert!(
                    fs::symlink_metadata(quarantined)
                        .expect("inspect quarantined symlink")
                        .file_type()
                        .is_symlink()
                );
                assert_eq!(
                    fs::read_link(quarantined).expect("read quarantined symlink"),
                    preserved_recovery
                );
            }
        }

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn exclusive_staging_does_not_follow_an_existing_symlink() {
        use std::os::unix::fs::symlink;

        let directory = test_directory("staging-symlink");
        let victim = directory.join("victim.lisp");
        let staged = directory.join("staged.lisp");
        fs::write(&victim, "secret").expect("write victim");
        symlink(&victim, &staged).expect("create staged symlink");

        let error = open_exclusive_file(&staged).expect_err("existing symlink must be rejected");

        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&victim).expect("read victim"), "secret");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn exclusive_staging_is_created_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("staging-permissions");
        let staged = directory.join("staged.lisp");

        let file = open_exclusive_file(&staged).expect("create staged file");
        drop(file);

        let mode = fs::metadata(&staged)
            .expect("read staged metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_group_or_other_writable_parent_directory() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("writable-parent");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o777))
            .expect("make parent writable");
        let target = directory.join("target.lisp");

        let error = stage_write_target(target, "(new)".to_owned())
            .err()
            .expect("writable parent must be rejected");

        assert!(
            error
                .to_string()
                .contains("refusing writable parent directory")
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_replaced_parent_without_touching_replacement_files() {
        let directory = test_directory("replaced-parent");
        let moved_directory = directory.with_extension("moved");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target before replacing parent");
        fs::rename(&directory, &moved_directory).expect("move original parent");
        fs::create_dir(&directory).expect("create replacement parent");
        fs::write(&target, "(third-party)").expect("write replacement target");

        let error =
            apply_staged_writes(vec![staged]).expect_err("replaced parent must be rejected");

        assert!(format!("{error:#}").contains("replaced parent directory"));
        assert_eq!(
            fs::read_to_string(&target).expect("read replacement target"),
            "(third-party)"
        );
        fs::remove_dir_all(directory).expect("remove replacement directory");
        fs::remove_dir_all(moved_directory).expect("remove original directory");
    }

    #[cfg(unix)]
    #[test]
    fn post_apply_parent_replacement_is_rejected_and_rollback_retains_recovery_through_anchor() {
        let directory = test_directory("post-apply-replaced-parent");
        let moved_directory = directory.with_extension("moved");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target before replacing parent");
        replace_file(
            ReplaceFileRequest {
                parent: &staged.parent,
                source_name: &staged.staged_name,
                target_name: &staged.target_name,
                source_path: &staged.staged_path,
                target_path: &staged.path,
                publication_state_uncertain: Some(&staged.publication_state_uncertain),
            },
            ReplaceTargetExpectation::Existing {
                snapshot: original_target_snapshot(&staged)
                    .expect("original target snapshot")
                    .expect("existing target snapshot"),
                security: staged
                    .original_security
                    .as_ref()
                    .expect("original target security"),
            },
        )
        .expect("apply through anchored directory");
        fs::rename(&directory, &moved_directory).expect("move original parent after apply");
        fs::create_dir(&directory).expect("create replacement parent");
        fs::write(&target, "(third-party)").expect("write replacement target");

        let error = finish_applied_write(&staged).expect_err("replaced parent must be rejected");
        let mut rollback_errors = Vec::new();
        rollback_failed_target(&staged, &mut rollback_errors);

        assert!(error.to_string().contains("replaced parent directory"));
        assert_eq!(rollback_errors.len(), 1, "{rollback_errors:?}");
        let rollback_report = rollback_errors.join("; ");
        assert!(rollback_report.contains("published target"));
        assert!(rollback_report.contains("public recovery entry was not moved or removed"));
        assert!(rollback_report.contains("replaced parent directory"));
        assert_eq!(
            fs::read_to_string(moved_directory.join("target.lisp"))
                .expect("read restored anchored target"),
            "(old)"
        );
        assert_eq!(
            fs::read_to_string(moved_directory.join(&staged.backup_name))
                .expect("read retained recovery artifact"),
            "(new)"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read replacement target"),
            "(third-party)"
        );
        fs::remove_dir_all(directory).expect("remove replacement directory");
        fs::remove_dir_all(moved_directory).expect("remove original directory");
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_nonexistent_write_targets_are_rejected_after_parent_normalization() {
        let directory = test_directory("duplicate-targets");
        let direct = directory.join("new.lisp");
        let aliased = directory.join(".").join("new.lisp");
        let files = [(direct, "()".to_owned()), (aliased, "()".to_owned())];

        let error = reject_duplicate_write_targets(files.iter().map(|(path, _)| path))
            .expect_err("duplicate must be rejected");

        assert!(error.to_string().contains("duplicate write target"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_a_directory_target() {
        let directory = test_directory("directory-target");
        let target = directory.join("target.lisp");
        fs::create_dir(&target).expect("create target directory");

        let error = stage_write_target(target, "()".to_owned())
            .err()
            .expect("directory target must be rejected");

        assert!(error.to_string().contains("non-regular file"));
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_a_socket_target() {
        use std::os::unix::net::UnixListener;

        let directory = test_directory("socket-target");
        let target = directory.join("target.lisp");
        let listener = UnixListener::bind(&target).expect("bind target socket");

        let error = stage_write_target(target, "()".to_owned())
            .err()
            .expect("socket target must be rejected");

        assert!(error.to_string().contains("non-regular file"));
        drop(listener);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn staging_rejects_a_hard_linked_target() {
        let directory = test_directory("hard-linked-target");
        let target = directory.join("target.lisp");
        let alias = directory.join("alias.lisp");
        fs::write(&target, "(old)").expect("write target");
        fs::hard_link(&target, &alias).expect("create hard link");

        let error = stage_write_target(target, "(new)".to_owned())
            .err()
            .expect("hard-linked target must be rejected");

        assert!(error.to_string().contains("hard-linked target"));
        assert_eq!(fs::read_to_string(alias).expect("read alias"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn staging_creates_an_independent_backup_copy() {
        let directory = test_directory("independent-backup");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");

        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::write(&target, "(changed)").expect("modify original target");

        assert_eq!(
            fs::read_to_string(&staged.backup_path).expect("read independent backup"),
            "(old)"
        );
        cleanup_unapplied_write(&staged, &mut Vec::new());
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_a_target_replaced_after_staging() {
        let directory = test_directory("replaced-target");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::remove_file(&target).expect("remove original target");
        fs::write(&target, "(concurrent)").expect("write replacement target");

        let error = apply_staged_writes(vec![staged]).expect_err("replacement must be rejected");

        // The replacement is rejected either by filesystem-identity mismatch
        // ("replaced target") or, on filesystems that reuse the freed inode
        // number (e.g. tmpfs), by the content digest ("changed content").
        let report = format!("{error:#}");
        assert!(
            report.contains("replaced target") || report.contains("target with changed content"),
            "{report}"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read concurrent target"),
            "(concurrent)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_restores_target_replaced_after_final_validation() {
        let directory = test_directory("replacement-after-final-validation");
        let target = directory.join("target.lisp");
        let preserved = directory.join("preserved-target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target");

        let hook_target = target.clone();
        let hook_preserved = preserved.clone();
        let _guard = install_before_existing_target_replace_hook(target.clone(), move || {
            fs::rename(&hook_target, &hook_preserved).expect("preserve validated target");
            fs::write(&hook_target, "(concurrent)").expect("publish concurrent target");
        });

        let error = apply_staged_writes(vec![staged])
            .expect_err("concurrent replacement after final validation must be rejected");

        assert!(
            format!("{error:#}").contains("target changed during atomic publication"),
            "{error:#}"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read concurrent target"),
            "(concurrent)"
        );
        assert_eq!(
            fs::read_to_string(&preserved).expect("read preserved original target"),
            "(old)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_preserves_state_when_exchange_restoration_fails() {
        let directory = test_directory("exchange-restoration-failure");
        let target = directory.join("target.lisp");
        let preserved_original = directory.join("preserved-original.lisp");
        let preserved_concurrent = directory.join("preserved-concurrent.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target");
        let staged_path = staged.staged_path.clone();
        let backup_path = staged.backup_path.clone();

        let hook_target = target.clone();
        let hook_preserved_original = preserved_original.clone();
        let _replace_guard =
            install_before_existing_target_replace_hook(target.clone(), move || {
                fs::rename(&hook_target, &hook_preserved_original)
                    .expect("preserve validated target");
                fs::write(&hook_target, "(concurrent)").expect("publish concurrent target");
            });
        let hook_staged_path = staged_path.clone();
        let hook_preserved_concurrent = preserved_concurrent.clone();
        let _restore_guard = install_before_exchange_restore_hook(target.clone(), move || {
            fs::rename(&hook_staged_path, &hook_preserved_concurrent)
                .expect("make exchange restoration fail while preserving displaced target");
        });

        let error = apply_staged_writes(vec![staged])
            .expect_err("failed exchange restoration must fail closed");
        let report = format!("{error:#}");

        assert!(report.contains("failed to restore exchange"), "{report}");
        assert!(report.contains("rollback skipped"), "{report}");
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(new)");
        assert_eq!(
            fs::read_to_string(&preserved_original).expect("read preserved original"),
            "(old)"
        );
        assert_eq!(
            fs::read_to_string(&preserved_concurrent).expect("read preserved concurrent target"),
            "(concurrent)"
        );
        assert_eq!(
            fs::read_to_string(&backup_path).expect("read retained backup"),
            "(old)"
        );
        assert!(!staged_path.exists());
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_preserves_a_regular_file_replacing_the_recovery_artifact() {
        assert_changed_recovery_artifact_is_preserved(
            "post-publication-regular-recovery-replacement",
            RecoveryArtifactReplacement::RegularFile,
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_preserves_a_fifo_replacing_the_recovery_artifact() {
        assert_changed_recovery_artifact_is_preserved(
            "post-publication-fifo-recovery-replacement",
            RecoveryArtifactReplacement::Fifo,
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_preserves_a_symlink_replacing_the_recovery_artifact() {
        assert_changed_recovery_artifact_is_preserved(
            "post-publication-symlink-recovery-replacement",
            RecoveryArtifactReplacement::Symlink,
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_quarantines_a_regular_file_replaced_after_final_validation() {
        assert_final_validation_replacement_is_quarantined(
            "final-validation-regular-recovery-replacement",
            RecoveryArtifactReplacement::RegularFile,
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_quarantines_a_fifo_replaced_after_final_validation() {
        assert_final_validation_replacement_is_quarantined(
            "final-validation-fifo-recovery-replacement",
            RecoveryArtifactReplacement::Fifo,
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn apply_quarantines_a_symlink_replaced_after_final_validation() {
        assert_final_validation_replacement_is_quarantined(
            "final-validation-symlink-recovery-replacement",
            RecoveryArtifactReplacement::Symlink,
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn cleanup_removes_matching_artifacts_from_quarantine() {
        let directory = test_directory("cleanup-quarantine-success");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target");
        let staged_path = staged.staged_path.clone();
        let backup_path = staged.backup_path.clone();

        let mut errors = Vec::new();
        cleanup_unapplied_write(&staged, &mut errors);

        assert!(errors.is_empty(), "{errors:?}");
        assert!(!staged_path.exists());
        assert!(!backup_path.exists());
        assert_eq!(
            fs::read_dir(directory.join(CLEANUP_QUARANTINE_NAME))
                .expect("read cleanup quarantine")
                .count(),
            0,
            "successful cleanup must not retain artifact files"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read original target"),
            "(old)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn cleanup_quarantine_move_preserves_both_entries_on_collision() {
        let directory = test_directory("cleanup-quarantine-collision");
        let source = directory.join("source.lisp");
        fs::write(&source, "(source)").expect("write source");
        let (parent, source_name) = open_anchored_parent(&source).expect("open anchored parent");
        let quarantine = open_cleanup_quarantine(&parent).expect("open cleanup quarantine");
        let collision_name = OsStr::new("collision");
        let collision_path = quarantine.display_path.join(collision_name);
        fs::write(&collision_path, "(collision)").expect("write collision");

        let error = rename_directory_entry_between_directories_no_replace(
            &parent,
            &source_name,
            &quarantine,
            collision_name,
        )
        .expect_err("quarantine collision must fail closed");

        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read_to_string(&source).expect("read retained source"),
            "(source)"
        );
        assert_eq!(
            fs::read_to_string(&collision_path).expect("read retained collision"),
            "(collision)"
        );
        drop(quarantine);
        drop(parent);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn cleanup_quarantine_rejects_insecure_mode() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("cleanup-quarantine-mode");
        let quarantine_path = directory.join(CLEANUP_QUARANTINE_NAME);
        fs::create_dir(&quarantine_path).expect("create cleanup quarantine");
        fs::set_permissions(&quarantine_path, fs::Permissions::from_mode(0o755))
            .expect("set insecure quarantine mode");
        let (parent, _) =
            open_anchored_parent(&directory.join("target.lisp")).expect("open anchored parent");

        let error = open_cleanup_quarantine(&parent)
            .err()
            .expect("insecure quarantine mode must be rejected");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(
            fs::metadata(&quarantine_path)
                .expect("inspect cleanup quarantine")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        drop(parent);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn cleanup_quarantine_rejects_symlink() {
        use std::os::unix::fs::symlink;

        let directory = test_directory("cleanup-quarantine-symlink");
        let redirected = directory.join("redirected");
        let quarantine_path = directory.join(CLEANUP_QUARANTINE_NAME);
        fs::create_dir(&redirected).expect("create redirected directory");
        symlink(&redirected, &quarantine_path).expect("create quarantine symlink");
        let (parent, _) =
            open_anchored_parent(&directory.join("target.lisp")).expect("open anchored parent");

        open_cleanup_quarantine(&parent)
            .err()
            .expect("quarantine symlink must be rejected");

        assert!(
            fs::symlink_metadata(&quarantine_path)
                .expect("inspect quarantine symlink")
                .file_type()
                .is_symlink()
        );
        assert!(redirected.is_dir());
        drop(parent);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn cleanup_quarantine_rejects_public_identity_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("cleanup-quarantine-identity");
        let quarantine_path = directory.join(CLEANUP_QUARANTINE_NAME);
        let retained_path = directory.join("retained-cleanup-quarantine");
        let (parent, _) =
            open_anchored_parent(&directory.join("target.lisp")).expect("open anchored parent");
        let quarantine = open_cleanup_quarantine(&parent).expect("open cleanup quarantine");
        fs::rename(&quarantine_path, &retained_path).expect("retain opened quarantine");
        fs::create_dir(&quarantine_path).expect("replace public quarantine");
        fs::set_permissions(
            &quarantine_path,
            fs::Permissions::from_mode(CLEANUP_QUARANTINE_MODE),
        )
        .expect("set replacement quarantine mode");

        let error = validate_cleanup_quarantine_anchor(&parent, &quarantine)
            .expect_err("quarantine identity replacement must be rejected");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert!(retained_path.is_dir());
        assert!(quarantine_path.is_dir());
        drop(quarantine);
        drop(parent);
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_failure_restores_original_when_target_was_deleted() {
        let directory = test_directory("deleted-target-before-apply");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target");
        let staged_path = staged.staged_path.clone();
        let backup_path = staged.backup_path.clone();
        fs::remove_file(&target).expect("delete target after staging");

        let error = apply_staged_writes(vec![staged])
            .expect_err("deleted target must fail validation and roll back");
        let report = format!("{error:#}");

        assert!(!report.contains("rollback/cleanup also failed"), "{report}");
        assert_eq!(
            fs::read_to_string(&target).expect("read restored target"),
            "(old)"
        );
        assert!(!staged_path.exists());
        assert!(!backup_path.exists());
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_a_staging_file_replaced_after_creation() {
        let directory = test_directory("replaced-staging");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::remove_file(&staged.staged_path).expect("remove staging file");
        fs::write(&staged.staged_path, "(attacker)").expect("replace staging file");

        let error = apply_staged_writes(vec![staged]).expect_err("replacement must be rejected");

        let report = format!("{error:#}");
        assert!(
            report.contains("replaced staging file")
                || report.contains("staging file with changed content"),
            "{report}"
        );
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_a_backup_replaced_after_creation() {
        let directory = test_directory("replaced-backup");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::remove_file(&staged.backup_path).expect("remove backup file");
        fs::write(&staged.backup_path, "(attacker)").expect("replace backup file");

        let error = apply_staged_writes(vec![staged]).expect_err("replacement must be rejected");

        let report = format!("{error:#}");
        assert!(
            report.contains("replaced backup") || report.contains("backup with changed content"),
            "{report}"
        );
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_target_content_changed_in_place_after_staging() {
        let directory = test_directory("changed-target-content");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::write(&target, "(bad)").expect("change target in place");
        assert_eq!(
            file_identity(&fs::metadata(&target).expect("read changed target metadata"))
                .expect("read changed target identity"),
            staged.original_identity.expect("original identity")
        );

        let error = apply_staged_writes(vec![staged]).expect_err("content change must be rejected");

        assert!(format!("{error:#}").contains("target with changed content"));
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(bad)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_staging_content_changed_in_place() {
        let directory = test_directory("changed-staging-content");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::write(&staged.staged_path, "(bad)").expect("change staging file in place");
        assert_eq!(
            file_identity(&fs::metadata(&staged.staged_path).expect("read staging metadata"))
                .expect("read staging identity"),
            staged.staged_identity
        );

        let error = apply_staged_writes(vec![staged]).expect_err("content change must be rejected");

        assert!(format!("{error:#}").contains("staging file with changed content"));
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_backup_content_changed_in_place() {
        let directory = test_directory("changed-backup-content");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::write(&staged.backup_path, "(bad)").expect("change backup in place");
        assert_eq!(
            file_identity(&fs::metadata(&staged.backup_path).expect("read backup metadata"))
                .expect("read backup identity"),
            staged.backup_identity.expect("backup identity")
        );

        let error = apply_staged_writes(vec![staged]).expect_err("content change must be rejected");

        assert!(format!("{error:#}").contains("backup with changed content"));
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_rejects_applied_target_content_changed_in_place() {
        let directory = test_directory("changed-applied-content");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        apply_staged_write(&staged).expect("apply staged target");
        fs::write(&target, "(bad)").expect("change applied target in place");
        assert_eq!(
            file_identity(&fs::metadata(&target).expect("read applied target metadata"))
                .expect("read applied target identity"),
            staged.staged_identity
        );

        let mut errors = Vec::new();
        rollback_applied_write(&staged, &mut errors);

        assert!(
            errors
                .join("; ")
                .contains("applied target with changed content")
        );
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(bad)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_preserves_concurrently_replaced_new_target() {
        let directory = test_directory("replaced-new-applied-target");
        let target = directory.join("target.lisp");
        let staged =
            stage_write_target(target.clone(), "(new)".to_owned()).expect("stage new target");
        apply_staged_write(&staged).expect("apply staged target");
        fs::remove_file(&target).expect("remove applied target");
        fs::write(&target, "(concurrent)").expect("write concurrent replacement");

        let mut errors = Vec::new();
        rollback_applied_write(&staged, &mut errors);

        // Whether the replacement is caught as a distinct inode ("replaced") or,
        // on filesystems that reuse the freed inode number, as differing content
        // ("changed content"), the concurrently published file must be preserved.
        let report = errors.join("; ");
        assert!(
            report.contains("refusing replaced newly published target")
                || report.contains("newly published target with changed content"),
            "{report}"
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read concurrent target"),
            "(concurrent)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_preserves_concurrently_modified_new_target() {
        let directory = test_directory("modified-new-applied-target");
        let target = directory.join("target.lisp");
        let staged =
            stage_write_target(target.clone(), "(new)".to_owned()).expect("stage new target");
        apply_staged_write(&staged).expect("apply staged target");
        fs::write(&target, "(concurrent)").expect("modify applied target");
        assert_eq!(
            file_identity(&fs::metadata(&target).expect("read modified target metadata"))
                .expect("read modified target identity"),
            staged.staged_identity
        );

        let mut errors = Vec::new();
        rollback_applied_write(&staged, &mut errors);

        assert!(
            errors
                .join("; ")
                .contains("newly published target with changed content")
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read concurrent target"),
            "(concurrent)"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_preserves_deleted_new_target_pathname() {
        let directory = test_directory("deleted-new-applied-target");
        let target = directory.join("target.lisp");
        let staged =
            stage_write_target(target.clone(), "(new)".to_owned()).expect("stage new target");
        apply_staged_write(&staged).expect("apply staged target");
        fs::remove_file(&target).expect("remove applied target");

        let mut errors = Vec::new();
        rollback_applied_write(&staged, &mut errors);

        assert!(errors.is_empty(), "{errors:?}");
        assert!(!target.exists(), "rollback must not recreate the pathname");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_preserves_non_regular_new_target_pathname() {
        let directory = test_directory("non-regular-new-applied-target");
        let target = directory.join("target.lisp");
        let staged =
            stage_write_target(target.clone(), "(new)".to_owned()).expect("stage new target");
        apply_staged_write(&staged).expect("apply staged target");
        fs::remove_file(&target).expect("remove applied target");
        fs::create_dir(&target).expect("replace applied target with directory");

        let mut errors = Vec::new();
        rollback_applied_write(&staged, &mut errors);
        let report = errors.join("; ");

        assert!(report.contains("refusing replaced newly published target"));
        assert!(report.contains(&target.display().to_string()));
        assert!(target.is_dir(), "rollback must not modify the pathname");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn rollback_rejects_backup_content_changed_in_place() {
        let directory = test_directory("changed-rollback-backup");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        apply_staged_write(&staged).expect("apply staged target");
        fs::write(&staged.backup_path, "(bad)").expect("change backup in place");

        let mut errors = Vec::new();
        rollback_applied_write(&staged, &mut errors);

        assert!(errors.join("; ").contains("backup with changed content"));
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(new)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn committed_write_preserves_macos_acl() {
        use std::process::Command;

        let directory = test_directory("preserve-macos-acl");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let status = Command::new("/bin/chmod")
            .args(["+a", "everyone deny execute"])
            .arg(&target)
            .status()
            .expect("run chmod");
        assert!(status.success(), "set target ACL");
        let original_acl = super::super::macos_acl::read_acl(
            &open_existing_file(&target).expect("open original target"),
        )
        .expect("read original ACL")
        .expect("target ACL");

        let staged =
            stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target with ACL");
        for path in [&staged.staged_path, &staged.backup_path] {
            let acl = super::super::macos_acl::read_acl(
                &open_existing_file(path).expect("open transaction file"),
            )
            .expect("read transaction ACL")
            .expect("transaction ACL");
            assert_eq!(acl, original_acl);
        }
        apply_staged_writes(vec![staged]).expect("commit write");

        let committed_acl = super::super::macos_acl::read_acl(
            &open_existing_file(&target).expect("open committed target"),
        )
        .expect("read committed ACL")
        .expect("committed ACL");
        assert_eq!(committed_acl, original_acl);
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(new)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn write_acl_removes_existing_macos_acl() {
        use std::process::Command;

        let directory = test_directory("remove-macos-acl");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write target");
        let status = Command::new("/bin/chmod")
            .args(["+a", "everyone deny execute"])
            .arg(&target)
            .status()
            .expect("run chmod");
        assert!(status.success(), "set target ACL");
        let file = open_existing_file(&target).expect("open target");
        assert!(
            super::super::macos_acl::read_acl(&file)
                .expect("read original ACL")
                .is_some(),
            "target must have an ACL before removal"
        );

        super::super::macos_acl::write_acl(&file, None).expect("remove target ACL");

        assert_eq!(
            super::super::macos_acl::read_acl(&file).expect("read removed ACL"),
            None
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apply_rejects_macos_acl_changed_after_staging() {
        use std::process::Command;

        let directory = test_directory("changed-macos-acl");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let status = Command::new("/bin/chmod")
            .args(["+a", "everyone deny execute"])
            .arg(&target)
            .status()
            .expect("run chmod");
        assert!(status.success(), "set target ACL");
        let staged =
            stage_write_target(target.clone(), "(new)".to_owned()).expect("stage target with ACL");
        let status = Command::new("/bin/chmod")
            .arg("-N")
            .arg(&target)
            .status()
            .expect("run chmod");
        assert!(status.success(), "remove target ACL");

        let error = apply_staged_writes(vec![staged]).expect_err("ACL change must be rejected");

        assert!(format!("{error:#}").contains("target with changed security metadata"));
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn committed_write_preserves_target_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("preserve-permissions");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640))
            .expect("set original permissions");

        write_file_with_rollback(target.clone(), "(new)".to_owned()).expect("commit write");

        let mode = fs::metadata(&target)
            .expect("read target metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640);
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(new)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn committed_write_preserves_owner_group_and_extended_attributes() {
        use std::ffi::OsStr;
        use std::os::unix::fs::MetadataExt;
        use xattr::FileExt;

        let directory = test_directory("preserve-security-metadata");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&target)
            .expect("open target");
        #[cfg(target_os = "macos")]
        let attribute = OsStr::new("com.paredit.test");
        #[cfg(not(target_os = "macos"))]
        let attribute = OsStr::new("user.paredit.test");
        // Extended-attribute preservation can only be exercised where the
        // filesystem supports xattrs. Sandboxed CI tmpfs and some backends
        // reject `setxattr` with ENOTSUP; skip rather than fail there.
        match file.set_xattr(attribute, b"security-value") {
            Ok(()) => {}
            Err(error) if is_xattr_unsupported(&error) => return,
            Err(error) => panic!("set extended attribute: {error}"),
        }
        let before = file.metadata().expect("read metadata");
        drop(file);

        write_file_with_rollback(target.clone(), "(new)".to_owned()).expect("commit write");

        let file = fs::File::open(&target).expect("open committed target");
        let after = file.metadata().expect("read committed metadata");
        assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
        assert_eq!(
            file.get_xattr(attribute).expect("get extended attribute"),
            Some(b"security-value".to_vec())
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_rejects_security_metadata_changed_after_staging() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory("changed-security-metadata");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))
            .expect("change target permissions");

        let error =
            apply_staged_writes(vec![staged]).expect_err("metadata change must be rejected");

        assert!(format!("{error:#}").contains("changed security metadata"));
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn replaced_backup_is_rejected_before_commit_and_cleanup_failure_is_reported() {
        let directory = test_directory("committed-cleanup-error");
        let target = directory.join("target.lisp");
        fs::write(&target, "(old)").expect("write original target");
        let staged = stage_write_target(target.clone(), "(new)".to_owned())
            .expect("stage target with backup");
        fs::remove_file(&staged.backup_path).expect("remove backup file");
        fs::create_dir(&staged.backup_path).expect("block backup cleanup");
        let backup_path = staged.backup_path.display().to_string();

        let error = apply_staged_writes(vec![staged]).expect_err("replacement must be rejected");
        let report = format!("{error:#}");

        assert!(report.contains("replaced backup"));
        assert!(report.contains(&backup_path));
        assert_eq!(fs::read_to_string(&target).expect("read target"), "(old)");
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_failure_restores_applied_targets_and_cleans_unvisited_staging() {
        let directory = test_directory("apply-failure-cleanup");
        let paths = ["first.lisp", "second.lisp", "third.lisp"].map(|name| directory.join(name));
        for path in &paths {
            fs::write(path, "(old)").expect("write original target");
        }

        let staged = paths
            .iter()
            .cloned()
            .map(|path| stage_write_target(path, "(new)".to_owned()).expect("stage target"))
            .collect::<Vec<_>>();
        fs::remove_file(&staged[1].staged_path).expect("force second apply to fail");

        let error = apply_staged_writes(staged).expect_err("second apply must fail");

        assert!(format!("{error:#}").contains("failed to write"));
        for path in &paths {
            assert_eq!(
                fs::read_to_string(path).expect("read restored target"),
                "(old)"
            );
        }
        let artifacts = fs::read_dir(&directory)
            .expect("read test directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".paredit-"))
            .collect::<Vec<_>>();
        assert!(
            artifacts.is_empty(),
            "stale transaction artifacts: {artifacts:?}"
        );

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_failure_rolls_back_a_newly_created_target() {
        let directory = test_directory("apply-failure-new-target");
        let new_path = directory.join("new.lisp");
        let failed_path = directory.join("failed.lisp");
        fs::write(&failed_path, "(old)").expect("write failed target");
        let staged = vec![
            stage_write_target(new_path.clone(), "(new)".to_owned()).expect("stage new target"),
            stage_write_target(failed_path.clone(), "(new)".to_owned())
                .expect("stage failed target"),
        ];
        fs::remove_file(&staged[1].staged_path).expect("force second apply to fail");

        let error = apply_staged_writes(staged).expect_err("second apply must fail");
        let report = format!("{error:#}");

        assert!(report.contains(&format!("failed to write {}", failed_path.display())));
        assert!(!report.contains("rollback/cleanup also failed"));
        assert!(!new_path.exists());
        assert_eq!(
            fs::read_to_string(&failed_path).expect("read failed target"),
            "(old)"
        );

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn apply_failure_preserves_apply_error_and_aggregates_cleanup_errors() {
        let directory = test_directory("apply-failure-errors");
        let failed_path = directory.join("failed.lisp");
        let unvisited_path = directory.join("unvisited.lisp");
        fs::write(&failed_path, "(old)").expect("write failed target");
        fs::write(&unvisited_path, "(old)").expect("write unvisited target");

        let staged = vec![
            stage_write_target(failed_path.clone(), "(new)".to_owned())
                .expect("stage failed target"),
            stage_write_target(unvisited_path, "(new)".to_owned()).expect("stage unvisited target"),
        ];
        fs::remove_file(&staged[0].staged_path).expect("force first apply to fail");
        fs::remove_file(&staged[1].staged_path).expect("remove unvisited staging file");
        fs::create_dir(&staged[1].staged_path).expect("block staging cleanup");
        fs::remove_file(&staged[1].backup_path).expect("remove unvisited backup");
        fs::create_dir(&staged[1].backup_path).expect("block backup cleanup");
        let staged_path = staged[1].staged_path.display().to_string();
        let backup_path = staged[1].backup_path.display().to_string();

        let error = apply_staged_writes(staged).expect_err("first apply must fail");
        let report = format!("{error:#}");

        assert!(report.contains(&format!("failed to write {}", failed_path.display())));
        assert!(report.contains(&staged_path));
        assert!(report.contains(&backup_path));

        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_preserves_replaced_regular_staging_file() {
        let directory = test_directory("cleanup-replaced-staging-file");
        let failed_path = directory.join("failed.lisp");
        let unvisited_path = directory.join("unvisited.lisp");
        fs::write(&failed_path, "(old)").expect("write failed target");
        fs::write(&unvisited_path, "(old)").expect("write unvisited target");

        let staged = vec![
            stage_write_target(failed_path, "(new)".to_owned()).expect("stage failed target"),
            stage_write_target(unvisited_path, "(new)".to_owned()).expect("stage unvisited target"),
        ];
        fs::remove_file(&staged[0].staged_path).expect("force first apply to fail");
        fs::remove_file(&staged[1].staged_path).expect("remove unvisited staging file");
        fs::write(&staged[1].staged_path, "third-party content")
            .expect("replace unvisited staging file");
        let replaced_path = staged[1].staged_path.clone();

        let error = apply_staged_writes(staged).expect_err("first apply must fail");
        let report = format!("{error:#}");

        assert!(
            report.contains("refusing replaced staging file")
                || report.contains("staging file with changed content"),
            "{report}"
        );
        assert_eq!(
            fs::read_to_string(&replaced_path).expect("read preserved replacement"),
            "third-party content"
        );

        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
