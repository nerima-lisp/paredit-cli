//! Why a refactor workflow refused to plan, apply, or undo.
//!
//! Section 9.2. This package is the workspace's most I/O-heavy: it reads a
//! manifest, opens a root capability, re-validates both against what it saw a
//! moment ago, writes, verifies, and can roll back. Most of its `.context()`
//! calls were the shape §9.2.1 says to keep — a path and an operation name on
//! an [`std::io::Error`] — and those became [`paredit_core_cli::CliError::Io`]
//! directly.
//!
//! Two things did not fit, and they are what this module is for.

use thiserror::Error;

use paredit_core_cli::CliError;
use paredit_core_cli::diagnosis::ErrorCode;

/// Context added to a failure that is already typed.
///
/// The handful of `.context()` calls whose source was a [`CliError`] rather
/// than an [`std::io::Error`] — "failed to read manifest {path}" wrapped
/// around a read that already refused for its own reason. `CliError::Io`
/// cannot hold them, because its source is an `io::Error`.
///
/// The classification is *delegated*, not re-decided: adding a sentence about
/// which file was being read does not change what kind of failure it is, and
/// deciding again here would be a second answer that could drift from the
/// first.
#[derive(Debug, Error)]
#[error("{context}")]
pub struct RefactorContext {
    pub context: String,
    #[source]
    pub source: Box<CliError>,
}

impl RefactorContext {
    /// Wraps `source` with the same sentence `.context()` would have added.
    ///
    /// Generic over the source so one helper covers every typed error this
    /// package puts context on — `io::Error`, `JournalError`, `ExternalError`,
    /// and `CliError` itself — instead of one helper per source type. That is
    /// what `.context()` gave for free and what the conversion had to rebuild.
    pub fn new<E: Into<CliError>>(context: impl Into<String>) -> impl FnOnce(E) -> Self {
        move |source| Self {
            context: context.into(),
            source: Box::new(source.into()),
        }
    }
}

paredit_core_cli::impl_classified_refusal!(RefactorContext, |error| {
    paredit_core_cli::diagnosis::code_for_cli_error(&error.source)
});

/// The manifest names something this run will not act on.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManifestError {
    #[error("{0}")]
    Malformed(String),

    /// A dialect string in the manifest is not one this build knows.
    ///
    /// Names the field as well as the value, because a manifest has one of
    /// these per file and "which one" is the first thing a reader needs. The
    /// message is reproduced exactly from the `.context()` it replaced.
    #[error("manifest field {field} has invalid dialect {dialect:?}")]
    UnsupportedDialect { field: String, dialect: String },
}

paredit_core_cli::impl_classified_refusal!(ManifestError, |error| match error {
    // The manifest is an artifact this tool reads; a malformed one is
    // unreadable input, not a defect in the tool.
    ManifestError::Malformed(_) => ErrorCode::InputUnparsable,
    ManifestError::UnsupportedDialect { .. } => ErrorCode::InputDialectUnsupported,
});
