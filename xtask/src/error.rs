//! What the developer tooling can fail with.
//!
//! `xtask` is the one crate in this workspace that is a *program*, not a
//! library: it scaffolds a new command or lint rule and prints what it did.
//! That is the textbook place for `anyhow` — and it still went, because the
//! goal was to remove the dependency from the tree, not merely from the
//! shipped crates. A dependency that is still in `Cargo.lock` is one somebody
//! reaches for again.
//!
//! What replaces it is deliberately small. Nothing here branches on a failure;
//! `main` prints it and exits. So this is one enum with the two shapes the
//! tooling actually produces — an I/O failure with the path it was working on,
//! and a refusal to overwrite something — rather than a variant per call site.

use std::fmt;
use std::path::Path;

/// A scaffolding step did not complete.
#[derive(Debug)]
pub enum XtaskError {
    /// An OS failure, with what was being done when it happened.
    Io {
        context: String,
        source: std::io::Error,
    },
    /// The tooling declined: a name that is not valid, a file already present,
    /// a repository layout that is not what it expects.
    Refused(String),
}

impl XtaskError {
    /// Attaches the same context string `anyhow`'s `.context()` would have.
    pub fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        move |source| Self::Io {
            context: context.into(),
            source,
        }
    }

    /// Names the path as well, which most of these want.
    pub fn at(verb: &str, path: &Path) -> impl FnOnce(std::io::Error) -> Self {
        Self::io(format!("{verb} {}", path.display()))
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self::Refused(message.into())
    }
}

impl fmt::Display for XtaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The chain, joined the way `anyhow`'s `{:#}` joined it: `main`
            // prints this and the cause is the useful half.
            Self::Io { context, source } => write!(formatter, "{context}: {source}"),
            Self::Refused(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for XtaskError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Refused(_) => None,
        }
    }
}

impl From<std::io::Error> for XtaskError {
    fn from(source: std::io::Error) -> Self {
        Self::Io {
            context: "i/o failed".to_owned(),
            source,
        }
    }
}

pub type Result<T = ()> = std::result::Result<T, XtaskError>;
