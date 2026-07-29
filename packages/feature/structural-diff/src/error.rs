//! Why a structural patch refuses to apply.
//!
//! Section 9.2. One refusal, and it is the one that matters most in this
//! package: a structural patch composes replacements taken from *another*
//! file, so nothing guarantees the result is still balanced. A change whose
//! "after" side ends mid-form would produce a document this tool could not
//! read back.
//!
//! It reached the boundary as `internal.unclassified` before this type
//! existed, which is close to the worst possible reading: the tool declining
//! to corrupt a file, reported as the tool being broken.

use thiserror::Error;

use paredit_core_cli::CliError;
use paredit_core_cli::diagnosis::ErrorCode;
use paredit_core_cli::error::FeatureRefusal;

/// The patch was computed, and applying it would not leave a readable file.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "refusing to patch {path}: applying the change would produce source that no longer \
     parses ({reason})"
)]
pub struct PatchWouldNotReparse {
    pub path: String,
    pub reason: String,
}

impl From<PatchWouldNotReparse> for CliError {
    fn from(error: PatchWouldNotReparse) -> Self {
        // The same code the CLI's own write path uses when a rewrite fails to
        // reparse. A caller that already handles that one needs no new case.
        Self::Feature(FeatureRefusal::new(
            ErrorCode::RefusalRewriteDoesNotReparse,
            &error,
        ))
    }
}

impl From<PatchWouldNotReparse> for paredit_core_cli::CommandFailure {
    fn from(error: PatchWouldNotReparse) -> Self {
        Self::Error(error.into())
    }
}
