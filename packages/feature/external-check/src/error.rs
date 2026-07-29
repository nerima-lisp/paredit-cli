//! Why running an external Lisp implementation did not produce a report.
//!
//! Section 9.2. This package is unusual in the workspace: everything else
//! fails because of the *source*, and this one fails because of a **program on
//! the caller's machine** — a missing `sbcl`, a heap exhaustion, a compile
//! that ran past its budget. Those are environment failures, and they were
//! reaching the boundary as `internal.unclassified`, which told a caller the
//! refactoring tool was defective when the honest answer was "install the
//! implementation" or "raise `--compile-timeout-ms`".
//!
//! The distinction the type makes is the one the command's own comment already
//! insisted on: an implementation that ran and said something unreadable must
//! never be reported as "no findings", because a caller gating a refactor on
//! this command would read a failed check as a passed one.

use std::path::PathBuf;

use thiserror::Error;

use paredit_core_cli::CliError;
use paredit_core_cli::diagnosis::ErrorCode;
use paredit_core_cli::error::FeatureRefusal;
use paredit_core_safety::external::ExternalError;

/// The external implementation did not give this command a usable answer.
///
/// Not `Clone`/`PartialEq`: two variants carry another error as their source.
#[derive(Debug, Error)]
pub enum ExternalCheckError {
    /// The implementation ran and produced something that is not diagnostics.
    ///
    /// A missing binary (the shell's 127), a heap exhaustion, a `--script`
    /// that was not there. The transcript is carried whole because it is the
    /// only evidence of what actually happened.
    #[error(
        "{implementation} produced no readable diagnostics for {path} (exit {exit}): {transcript}"
    )]
    NoReadableDiagnostics {
        implementation: &'static str,
        path: String,
        exit: String,
        transcript: String,
    },

    /// The compile ran past `--compile-timeout-ms`.
    ///
    /// Its own variant, not folded into [`Self::NoReadableDiagnostics`],
    /// because nothing is wrong: the same command with a larger budget would
    /// answer. That is the difference between "fix your environment" and "wait
    /// longer", and it is worth an error code each.
    #[error("{implementation} exceeded the {budget_ms}ms budget compiling {path}")]
    CompileTimedOut {
        implementation: &'static str,
        budget_ms: u64,
        path: String,
    },

    /// The implementation could not be started at all.
    ///
    /// [`ExternalError`] already names the command and carries the OS error;
    /// what it cannot know is which file this command was working on, which is
    /// the only thing this variant adds.
    #[error("failed to run {implementation} over {path}")]
    RunFailed {
        implementation: &'static str,
        path: String,
        #[source]
        source: ExternalError,
    },

    /// A saved baseline file is not a baseline this version can read.
    #[error("baseline {path} is not usable: {reason}")]
    BaselineUnusable { path: PathBuf, reason: String },

    /// `--save-baseline` named a path that could not be written.
    #[error("failed to write baseline {path}")]
    BaselineWriteFailed {
        path: String,
        #[source]
        source: Box<CliError>,
    },
}

impl ExternalCheckError {
    const fn code(&self) -> ErrorCode {
        match self {
            // "the tool you asked me to run is not usable here"
            Self::NoReadableDiagnostics { .. } => ErrorCode::EnvironmentUnavailable,
            Self::CompileTimedOut { .. } => ErrorCode::EnvironmentTimeout,
            Self::RunFailed { .. } => ErrorCode::EnvironmentUnavailable,
            // The file was read; its contents are the problem.
            Self::BaselineUnusable { .. } => ErrorCode::InputUnparsable,
            Self::BaselineWriteFailed { .. } => ErrorCode::EnvironmentIo,
        }
    }
}

impl From<ExternalCheckError> for CliError {
    fn from(error: ExternalCheckError) -> Self {
        Self::Feature(FeatureRefusal::new(error.code(), &error))
    }
}

impl From<ExternalCheckError> for paredit_core_cli::CommandFailure {
    fn from(error: ExternalCheckError) -> Self {
        Self::Error(error.into())
    }
}
