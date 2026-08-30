//! Why a project-wide analysis fails.
//!
//! Section 9.2. Almost nothing here can fail: these reports walk a parsed tree
//! and count things. What failures exist come from *below* — a path that does
//! not resolve, or one of the two packages this one reads through
//! (`paredit-feature-package` for dependency graphs,
//! `paredit-feature-remove-unused` for definition inventories).
//!
//! So [`ProjectAnalysisError`] is mostly a union, and that is the honest
//! shape: this package's own contribution to the failure set is nearly empty,
//! and saying so is more useful than inventing variants it never raises.
//!
//! [`WorkspaceAnalysisError`] is the exception, and it exists for a reason
//! specific to workspace-scale reports: it names *which file* and *which pass*
//! gave up. Over a thousand-file run, "something failed" is not actionable and
//! "failed to score complexity in src/foo.lisp" is.

use thiserror::Error;

use paredit_core_syntax::sexpr::SexprError;

/// One file's analysis could not complete.
#[derive(Debug, Error)]
pub enum WorkspaceAnalysisError {
    #[error("failed to analyze {path}")]
    Definitions {
        path: String,
        #[source]
        source: Box<ProjectAnalysisError>,
    },

    #[error("failed to collect calls in {path}")]
    Calls {
        path: String,
        #[source]
        source: Box<ProjectAnalysisError>,
    },

    #[error("failed to score complexity in {path}")]
    Complexity {
        path: String,
        #[source]
        source: Box<ProjectAnalysisError>,
    },
}

/// Anything a project-wide analysis can fail with.
#[derive(Debug, Error)]
pub enum ProjectAnalysisError {
    #[error(transparent)]
    Selection(#[from] SexprError),

    #[error(transparent)]
    Package(#[from] paredit_feature_package::PackageRefactorError),

    #[error(transparent)]
    Definitions(#[from] paredit_feature_remove_unused::RemoveUnusedError),

    #[error(transparent)]
    Workspace(#[from] WorkspaceAnalysisError),

    // `analyze_files` reads and parses each file itself, ahead of the
    // per-file analysis this crate contributes; this is that read/parse
    // failure crossing into the closure's own error type so a single
    // unreadable file among many is data for the report rather than an
    // error `analyze_files` has no vocabulary to carry.
    #[error(transparent)]
    Io(#[from] paredit_core_cli::CliError),
}

/// The result type the project-analysis passes return.
pub type ProjectAnalysisResult<T> = std::result::Result<T, ProjectAnalysisError>;

// States which documented error code each project-analysis refusal earns.
//
// Three of the four arms delegate: this package composes `package`,
// `remove-unused` and the workspace layer rather than refusing on its own, so
// re-deciding here would be a second answer to a question already answered.
paredit_core_cli::impl_classified_refusal!(ProjectAnalysisError, |error| match error {
    ProjectAnalysisError::Selection(sexpr) =>
        paredit_core_cli::diagnosis::code_for_sexpr_error(sexpr),
    ProjectAnalysisError::Package(package) => paredit_feature_package::error::code_of(package),
    ProjectAnalysisError::Definitions(definitions) =>
        paredit_feature_remove_unused::error::code_of(definitions),
    ProjectAnalysisError::Workspace(_) => paredit_core_cli::diagnosis::ErrorCode::RefusalWorkspace,
    ProjectAnalysisError::Io(io) => paredit_core_cli::diagnosis::code_for_cli_error(io),
});

// A workspace analysis failure names the file it stopped on; the failure
// itself is the file not parsing.
paredit_core_cli::impl_classified_refusal!(WorkspaceAnalysisError, |_error| {
    paredit_core_cli::diagnosis::ErrorCode::InputUnparsable
});
