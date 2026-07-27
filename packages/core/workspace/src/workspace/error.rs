//! Typed failures for workspace discovery.
//!
//! Section 9.2 of the migration spec: `anyhow::Error` is type-erased, so a
//! caller can only inspect a failure by reading its message. That matters here
//! more than anywhere else in the tree, because the two things discovery can
//! refuse to do are not alike.
//!
//! A **limit** means the request was too large: narrow `--include`, raise a
//! bound, or split the run. A **refusal** means something about the filesystem
//! was unsafe — a path outside the canonical roots, a file that was replaced
//! while being opened, a symlink where a regular file was expected. The first
//! is a usage problem and the second is closer to a security event, and they
//! deserve different exit codes. Today that distinction exists only in the
//! wording of a string.
//!
//! Messages are reproduced exactly. Section 9.2's goal is type-level
//! distinction, not better wording, and the CLI's string assertions and the
//! `inspect capabilities` golden both depend on the current text.

use std::path::PathBuf;

use thiserror::Error;

/// A bound on the size of a discovery request was exceeded.
///
/// Every variant is recoverable by asking for less.
#[derive(Debug, Error)]
pub enum WorkspaceLimit {
    #[error("workspace exclude path limit exceeded: {actual} paths exceeds maximum {maximum}")]
    ExcludePaths { actual: usize, maximum: usize },

    #[error("workspace exclude index component limit exceeded: maximum is {maximum}")]
    ExcludeComponents { maximum: usize },

    #[error(
        "workspace total read limit exceeded while reading {path}: {current} + {bytes} exceeds maximum {maximum}"
    )]
    TotalRead {
        path: PathBuf,
        current: u64,
        bytes: u64,
        maximum: u64,
    },

    #[error("workspace root input limit exceeded: {actual} roots exceeds maximum {maximum}")]
    Roots { actual: usize, maximum: usize },

    #[error("workspace entry limit exceeded while scanning {path}: maximum is {maximum}")]
    Entries { path: PathBuf, maximum: usize },

    #[error("workspace file limit exceeded: maximum is {maximum}")]
    Files { maximum: usize },

    #[error(
        "workspace file size limit exceeded for {path}: {actual} bytes exceeds maximum {maximum}"
    )]
    FileSize {
        path: PathBuf,
        actual: u64,
        maximum: u64,
    },

    #[error("workspace file size limit exceeded while reading {path}: maximum is {maximum}")]
    ReadSize { path: PathBuf, maximum: u64 },

    #[error("workspace total byte limit exceeded: {actual} bytes exceeds maximum {maximum}")]
    TotalBytes { actual: u64, maximum: u64 },
}

/// Discovery declined to touch something, for safety rather than for size.
///
/// These are the checks that make a capability-scoped traversal meaningful: a
/// path that escapes its root, or a file that changed identity between being
/// selected and being opened, must not be read.
#[derive(Debug, Error)]
pub enum WorkspaceRefusal {
    #[error("refusing workspace file outside canonical roots: {path}")]
    OutsideRoots { path: PathBuf },

    #[error("refusing non-regular workspace file: {path}")]
    NonRegularFile { path: PathBuf },

    #[error("refusing replaced or non-regular workspace file: {path}")]
    ReplacedOrNonRegular { path: PathBuf },

    #[error("refusing workspace file not selected during discovery: {path}")]
    NotSelected { path: PathBuf },

    #[error("refusing workspace file replaced while opening: {path}")]
    ReplacedWhileOpening { path: PathBuf },

    #[error("workspace file identity differs between ambient path and root capability: {path}")]
    IdentityMismatch { path: PathBuf },

    #[error("workspace root changed while opening capability: {path}")]
    RootChanged { path: PathBuf },

    #[error("workspace root identity changed after capability open: {path}")]
    RootIdentityChanged { path: PathBuf },

    #[error("workspace file changed while reading: {path}")]
    ChangedWhileReading { path: PathBuf },
}

/// Anything workspace discovery can fail with.
///
/// The three variants are the three things a caller can do about a failure:
/// ask for less, stop because the filesystem is not what it was, or report an
/// I/O error. `Io` carries its context string so the rendered chain matches
/// what `anyhow`'s `.context()` produced.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Limit(#[from] WorkspaceLimit),

    #[error(transparent)]
    Refused(#[from] WorkspaceRefusal),

    /// A precondition that is not an I/O failure: an identity the platform did
    /// not report, an arithmetic overflow in a bound, a path that resolved
    /// outside its capability. Each is a `&'static str` because there is
    /// nothing per-call to say - the message is the whole content.
    #[error("{0}")]
    Unavailable(&'static str),

    #[error("{context}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl WorkspaceError {
    /// The deepest error in the chain, as `anyhow::Error::root_cause` returned.
    ///
    /// Callers that only want to report a failure still need this, and losing
    /// it was the one thing that broke when the type erasure went away - which
    /// is itself the finding: a caller reaching for `root_cause().to_string()`
    /// is re-deriving from a string what it could now match on.
    #[must_use]
    pub fn root_cause(&self) -> &(dyn std::error::Error + 'static) {
        let mut cause: &(dyn std::error::Error + 'static) = self;
        while let Some(source) = cause.source() {
            cause = source;
        }
        cause
    }

    /// Attaches the same context string `anyhow`'s `.context()` would have.
    pub fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        move |source| Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// The result type every fallible discovery entry point returns.
pub type WorkspaceResult<T> = std::result::Result<T, WorkspaceError>;
