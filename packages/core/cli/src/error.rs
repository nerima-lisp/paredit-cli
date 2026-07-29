//! Typed failures for the CLI's I/O conventions.
//!
//! Section 9.2, last of the core packages. This is the one the spec warned
//! about (§9.2.1): *do not mechanically turn every `.context(...)` into a
//! variant*, because most of the context here is a path and an operation name
//! attached to an [`std::io::Error`], and expanding those into variants gives
//! one variant per call site and explains nothing.
//!
//! So the split is between context and decision:
//!
//! - [`CliError::Io`] carries the context string, exactly as `.context()`
//!   produced it, with the [`std::io::Error`] as its source. Ten sites.
//! - [`IoRefusal`] is where the CLI *decided* not to do something a caller
//!   asked for: an input over the size limit, a write target that is a symlink
//!   or not a regular file, overlapping rewrite spans, output that does not
//!   reparse. These are policy, not I/O, and they are the ones a caller might
//!   want to handle rather than report.
//! - [`ArgumentError`] is a usage problem, resolvable by changing the command
//!   line and by nothing else.
//! - [`CleanupFailure`] is the one this conversion actually clarified — see
//!   its own documentation.
//!
//! Messages are reproduced exactly.

use std::string::FromUtf8Error;

use thiserror::Error;

use paredit_core_syntax::sexpr::{ParseError, SexprError};
use paredit_core_workspace::workspace::WorkspaceError;

/// The CLI declined to read or write something, as policy rather than because
/// the filesystem objected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IoRefusal {
    #[error("refusing to read {description}: input exceeds {limit} bytes")]
    InputTooLarge { description: String, limit: u64 },

    #[error("refusing to write symlink {path}")]
    WriteSymlink { path: String },

    #[error("refusing to write non-regular file {path}")]
    WriteNonRegularFile { path: String },

    #[error("refusing overlapping rewrite spans")]
    OverlappingRewriteSpans,

    #[error("rewrite span is outside input or not UTF-8 aligned")]
    RewriteSpanOutOfBounds,

    #[error("refusing to write: rewritten source does not reparse")]
    RewriteDoesNotReparse,

    #[error("duplicate write target {path}")]
    DuplicateWriteTarget { path: String },

    #[error("write target has no file name: {path}")]
    WriteTargetHasNoFileName { path: String },

    #[error("refusing to write {path}: rewritten source does not reparse")]
    NamedRewriteDoesNotReparse { path: String },

    // --- the target is not what it was when we read it ---
    //
    // These six are the write path's time-of-check-to-time-of-use guards.
    // Grouped here rather than in their own type because a caller's response
    // is the same for all of them: re-read and retry, or stop.
    #[error("refusing replaced target {path}")]
    ReplacedTarget { path: String },

    #[error("refusing to replace hard-linked target {path}")]
    HardLinkedTarget { path: String },

    #[error("refusing target replaced since parsing {path}")]
    TargetReplacedSinceParsing { path: String },

    #[error("refusing target changed since parsing {path}")]
    TargetChangedSinceParsing { path: String },

    #[error("refusing target removed since parsing {path}")]
    TargetRemovedSinceParsing { path: String },

    #[error("refusing writable parent directory {path}")]
    WritableParentDirectory { path: String },

    /// `--dry-run` reached a write the argument layer could not turn into a
    /// preview, because the command does not spell writing as `--write`.
    #[error(
        "refusing to write: --dry-run is in force. \
         Drop it, or use the command's own preview flag (--diff on an edit, \
         --fix --diff on lint)"
    )]
    DryRun,
}

/// The path a write was aimed at cannot serve as a target.
///
/// Distinct from [`IoRefusal`] because these are not policy decisions: the
/// path is structurally unusable, and no permission or retry changes that.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WriteTargetError {
    #[error("write target parent is not a directory: {path}")]
    ParentNotADirectory { path: String },

    #[error("anchored target name does not match display path {path}")]
    AnchoredNameMismatch { path: String },

    #[error("stable filesystem identity is unavailable for retained parent directory {path}")]
    ParentIdentityUnavailable { path: String },
}

/// The command line does not describe a runnable request.
///
/// Separate from every other kind because the fix is always the same shape —
/// change the arguments — and because these are raised before any I/O happens.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ArgumentError {
    #[error(
        "no input: pass --file <path> or pipe source into stdin \
         (refusing to wait on an interactive terminal)"
    )]
    NoInput,

    #[error("target required: pass --path or --at")]
    TargetRequired,

    #[error("pass only one of --path or --at")]
    TargetAmbiguous,

    #[error("--write requires --file")]
    WriteRequiresFile,
}

/// A write failed, and then undoing it failed too.
///
/// This is the variant the conversion earned. With `anyhow` the recovery
/// failure was layered on with `.context()`, which makes it the *outermost*
/// message — so the text a user saw led with "rollback/cleanup also failed"
/// and buried the write error that caused it as a cause. Reversing that would
/// change output, so the rendering is unchanged; what changes is that the two
/// failures are now separately reachable instead of being one flattened
/// string, and the type says out loud that the working tree may be in a
/// partially-written state.
#[derive(Debug, Error)]
#[error("{summary}")]
pub struct CleanupFailure {
    /// The recovery failure, rendered as `.context()` rendered it.
    pub summary: String,
    /// The failure that triggered the recovery attempt.
    #[source]
    pub cause: Box<CliError>,
}

/// Anything the CLI's I/O layer can fail with.
#[derive(Debug, Error)]
pub enum CliError {
    /// An [`std::io::Error`] with the operation and path that produced it.
    ///
    /// A `String` rather than a variant per call site: the context is the
    /// whole content, there are ten of them, and one variant each would name
    /// the call site rather than the failure.
    #[error("{context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// An [`std::io::Error`] reaching a caller with no context attached.
    ///
    /// The helpers that return a bare `io::Result` - identity snapshots,
    /// digests - were already reaching `anyhow` this way, and the rendered
    /// message was the `io::Error`'s own. `transparent` keeps that exactly.
    #[error(transparent)]
    Bare(#[from] std::io::Error),

    #[error(transparent)]
    Refused(#[from] IoRefusal),

    /// The syntax layer refused a selection or an edit the CLI passed through.
    ///
    /// Kept whole rather than flattened, so a caller can still tell a shape
    /// refusal from a stale selection - see `paredit_core_syntax::sexpr::error`.
    #[error(transparent)]
    Sexpr(#[from] SexprError),

    /// Workspace discovery declined or hit a limit.
    ///
    /// Also kept whole: `WorkspaceError` already separates a size limit from a
    /// safety refusal, and flattening it here would undo that.
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),

    #[error(transparent)]
    Argument(#[from] ArgumentError),

    #[error("{description} is not valid UTF-8")]
    NotUtf8 {
        description: String,
        #[source]
        source: FromUtf8Error,
    },

    #[error("failed to parse {origin} ({location})")]
    Parse {
        origin: String,
        location: String,
        #[source]
        source: ParseError,
    },

    #[error(transparent)]
    CleanupAlsoFailed(#[from] CleanupFailure),

    #[error(transparent)]
    WriteTarget(#[from] WriteTargetError),

    /// The writes landed; only tidying up after them did not.
    ///
    /// Its own variant because it is the one failure here that a caller must
    /// *not* treat as "the operation did not happen". The files are written.
    #[error("writes committed successfully, but backup cleanup failed: {details}")]
    BackupCleanupAfterCommit { details: String },
}

impl CliError {
    /// Attaches the same context string `anyhow`'s `.context()` would have.
    pub fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        move |source| Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Wraps this failure in the recovery failure that followed it.
    #[must_use]
    pub fn with_cleanup_failure(self, summary: impl Into<String>) -> Self {
        Self::CleanupAlsoFailed(CleanupFailure {
            summary: summary.into(),
            cause: Box::new(self),
        })
    }

    /// The message chain, joined the way `anyhow`'s `{:#}` joins it.
    ///
    /// `thiserror` renders only the outermost message; `anyhow` walks
    /// `source()` and joins with `": "`. The CLI still sees the `anyhow`
    /// rendering, because `main` converts at the boundary — this exists so a
    /// caller inside the workspace can assert on the whole chain without
    /// going through that conversion.
    #[must_use]
    pub fn chain(&self) -> String {
        let mut rendered = self.to_string();
        let mut source = std::error::Error::source(self);
        while let Some(error) = source {
            rendered.push_str(": ");
            rendered.push_str(&error.to_string());
            source = error.source();
        }
        rendered
    }

    /// The deepest error in the chain, as `anyhow::Error::root_cause` returned.
    #[must_use]
    pub fn root_cause(&self) -> &(dyn std::error::Error + 'static) {
        let mut cause: &(dyn std::error::Error + 'static) = self;
        while let Some(source) = std::error::Error::source(cause) {
            cause = source;
        }
        cause
    }
}

/// The result type the CLI's I/O entry points return.
pub type CliResult<T> = std::result::Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// `chain()` must render what `anyhow`'s `{:#}` rendered, because the
    /// tests that assert on write-path failures were written against it — and
    /// because that is still what a user sees, `main` converting at the edge.
    #[test]
    fn chain_flattens_the_source_chain_like_anyhow() {
        let error = CliError::Io {
            context: "failed to open or inspect /tmp/x".to_owned(),
            source: std::io::Error::other("refusing non-regular input file /tmp/x"),
        };

        assert_eq!(error.to_string(), "failed to open or inspect /tmp/x");
        assert_eq!(
            error.chain(),
            "failed to open or inspect /tmp/x: refusing non-regular input file /tmp/x"
        );
        assert_eq!(
            error.root_cause().to_string(),
            "refusing non-regular input file /tmp/x"
        );
    }

    /// A recovery failure keeps the failure that caused it reachable, rather
    /// than flattening both into one string. The rendering is unchanged - the
    /// cleanup message still leads - but the two are now separate values.
    #[test]
    fn a_cleanup_failure_keeps_the_write_failure_it_followed() {
        let write_failure = CliError::Io {
            context: "failed to write /tmp/x".to_owned(),
            source: std::io::Error::other("disk full"),
        };
        let error = write_failure.with_cleanup_failure("rollback/cleanup also failed: denied");

        assert_eq!(error.to_string(), "rollback/cleanup also failed: denied");
        let CliError::CleanupAlsoFailed(failure) = &error else {
            panic!("expected a cleanup failure, got {error:?}");
        };
        assert_eq!(failure.cause.to_string(), "failed to write /tmp/x");
        assert_eq!(error.root_cause().to_string(), "disk full");
    }

    /// The one failure that must not be read as "nothing happened".
    #[test]
    fn a_post_commit_cleanup_failure_is_its_own_variant() {
        let error = CliError::BackupCleanupAfterCommit {
            details: "denied".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "writes committed successfully, but backup cleanup failed: denied"
        );
        assert!(!matches!(error, CliError::Refused(_)));
    }
}
