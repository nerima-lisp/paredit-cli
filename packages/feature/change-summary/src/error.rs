//! Why a change summary could not be produced.
//!
//! Section 9.2. One refusal: the command diffs two documents structurally, so
//! both have to parse. Which of the two did not is deliberately left to
//! `inspect check` rather than guessed at here — the message says so, and
//! saying it is the whole value of the error.

use thiserror::Error;

use paredit_core_cli::CliError;
use paredit_core_cli::diagnosis::ErrorCode;
use paredit_core_cli::error::FeatureRefusal;

/// One of the two documents is not a balanced S-expression document.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "cannot compare {before} with {after}: one of them is not a balanced S-expression document. \
     Run `paredit inspect check` on each to find out which."
)]
pub struct DocumentsNotComparable {
    pub before: String,
    pub after: String,
}

impl From<DocumentsNotComparable> for CliError {
    fn from(error: DocumentsNotComparable) -> Self {
        Self::Feature(FeatureRefusal::new(ErrorCode::InputUnparsable, &error))
    }
}

impl From<DocumentsNotComparable> for paredit_core_cli::CommandFailure {
    fn from(error: DocumentsNotComparable) -> Self {
        Self::Error(error.into())
    }
}
