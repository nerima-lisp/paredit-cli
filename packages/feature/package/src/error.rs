//! Why a `defpackage` refactor refuses to run.
//!
//! Section 9.2. The refusals here divide cleanly into two, and the division is
//! useful because a caller can act on one and not the other:
//!
//! - [`DefpackageSelectionError`] — the command could not decide *which*
//!   `defpackage` to edit. Either none matched, or several did. The second is
//!   the interesting one: it is not a failure of the file, it is the command
//!   asking for `--package`, and a caller that can recognise it can prompt.
//! - [`DefpackageShapeError`] — the form was found, but part of it is written
//!   in a way this refactor does not read: an option that is not a direct
//!   list, an option head that is not an atom, an `:export` designator that is
//!   not a plain symbol. Every one of these is "the file is fine, the tool is
//!   conservative", and each carries the path of the part it stopped at.
//!
//! Messages are reproduced exactly.

use thiserror::Error;

use paredit_core_cli::CliError;
use paredit_core_cli::diagnosis::{ErrorCode, code_for_edit_refusal};
use paredit_core_cli::error::FeatureRefusal;
use paredit_core_edit::EditRefusal;
use paredit_core_syntax::sexpr::SexprError;

/// Which `defpackage` the command should edit could not be determined.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DefpackageSelectionError {
    #[error("no matching defpackage form found for {target}")]
    NoMatch { target: String },

    /// Not a defect: the command needs `--package` to disambiguate.
    #[error("multiple matching defpackage forms found; pass --package to choose one unambiguously")]
    Ambiguous,
}

/// Part of the `defpackage` form is written in a way this refactor will not
/// rewrite.
///
/// Each variant carries the path of the part it stopped at, so a caller can
/// point at it rather than making the user find it.
///
/// Not `Clone`, because `InspectFailed` carries a `SexprError`.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum DefpackageShapeError {
    #[error("cannot sort defpackage options at {path}; only direct option lists are supported")]
    SortOptionsNotDirectLists { path: String },

    #[error("cannot sort defpackage option at {path}; option head must be an atom")]
    SortOptionHeadNotAtom { path: String },

    #[error("cannot merge defpackage options at {path}; only direct option lists are supported")]
    MergeOptionsNotDirectLists { path: String },

    #[error("cannot merge defpackage option at {path}; option head must be an atom")]
    MergeOptionHeadNotAtom { path: String },

    #[error("cannot merge defpackage option at {path}; option payload must contain atoms only")]
    MergeOptionPayloadNotAtoms { path: String },

    #[error("cannot sort :export option at {path}; only atom symbol designators are supported")]
    ExportDesignatorNotAnAtom { path: String },

    #[error("failed to inspect package form at {path}")]
    InspectFailed {
        path: String,
        #[source]
        source: Box<PackageRefactorError>,
    },
}

/// Anything a package refactor can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum PackageRefactorError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] EditRefusal),

    #[error(transparent)]
    Selection(#[from] DefpackageSelectionError),

    #[error(transparent)]
    Shape(#[from] DefpackageShapeError),

    /// A reader conditional made the rewrite unsafe.
    #[error(transparent)]
    ReaderConditional(#[from] paredit_core_edit::mutation_safety::ReaderConditionalSafetyError),
}

// `From` does not chain.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for PackageRefactorError {
            fn from(error: paredit_core_edit::$ty) -> Self {
                Self::Edit(error.into())
            }
        })+
    };
}

from_edit_refusal!(DialectRefusal, DocumentRefusal);

impl From<SexprError> for PackageRefactorError {
    fn from(error: SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the package refactor planners return.
pub type PackageRefactorResult<T> = std::result::Result<T, PackageRefactorError>;

/// A package command could not plan or inspect one file.
///
/// The CLI layer's error, distinct from [`PackageRefactorError`] because it
/// adds the one thing the planner cannot know: *which file*. Six commands
/// spelled this as `.with_context(|| format!("failed to plan {op} for {path}"))`,
/// which is the shape §9.2.1 says to keep — the context is a path and an
/// operation name, and one variant per call site would name the call site
/// rather than the failure. So it is one variant with `operation` threaded,
/// exactly as `paredit-core-edit` does it.
#[derive(Debug, Error)]
pub enum PackageCommandError {
    #[error("failed to plan {operation} for {path}")]
    Plan {
        operation: &'static str,
        path: String,
        #[source]
        source: PackageRefactorError,
    },

    /// Its own variant only because the preposition differs (`in`, not `for`),
    /// and the messages are reproduced exactly.
    #[error("failed to inspect packages in {path}")]
    Inspect {
        path: String,
        #[source]
        source: PackageRefactorError,
    },
}

impl PackageCommandError {
    const fn source_refusal(&self) -> &PackageRefactorError {
        match self {
            Self::Plan { source, .. } | Self::Inspect { source, .. } => source,
        }
    }
}

/// States which documented error code a package refusal earns.
///
/// Before this existed the answer was `internal.unclassified` for every one of
/// them — "a defect in this tool" — because the refusal reached the boundary
/// inside an `anyhow::Error` and the classifier's `downcast` chain had nothing
/// to match. `refactor add-export --package nope` told an agent the tool was
/// broken when the honest answer was `selection.no-match`.
/// Which documented error code a package refusal earns.
///
/// Public because `paredit-feature-project-analysis` composes this package and has to answer the same question.
#[must_use]
pub const fn code_of(error: &PackageRefactorError) -> ErrorCode {
    match error {
        // Core already answers this for its own refusals; asking it rather
        // than re-deciding here is what keeps the two from drifting apart.
        PackageRefactorError::Edit(edit) => code_for_edit_refusal(edit),

        PackageRefactorError::Selection(DefpackageSelectionError::NoMatch { .. }) => {
            ErrorCode::SelectionNoMatch
        }
        // Not a defect and not a no-match: the file has several `defpackage`
        // forms and the command needs `--package` to say which. A caller that
        // reads this can prompt instead of giving up.
        PackageRefactorError::Selection(DefpackageSelectionError::Ambiguous) => {
            ErrorCode::SelectionAmbiguous
        }

        // "the form was found, but part of it is written in a way this
        // refactor does not read" — the file is fine, the tool is
        // conservative, and the action is to select or rewrite by hand.
        PackageRefactorError::Shape(_) | PackageRefactorError::ReaderConditional(_) => {
            ErrorCode::InputShapeRefused
        }
    }
}

impl From<PackageCommandError> for CliError {
    fn from(error: PackageCommandError) -> Self {
        let code = code_of(error.source_refusal());
        Self::Feature(FeatureRefusal::new(code, &error))
    }
}

/// The same classification without a file name attached.
///
/// The report commands walk several files and call the collectors directly,
/// so there is no single path worth naming and the planner's own message is
/// the whole message.
impl From<PackageRefactorError> for CliError {
    fn from(error: PackageRefactorError) -> Self {
        Self::Feature(FeatureRefusal::new(code_of(&error), &error))
    }
}

// `?` needs the one-step conversion at a command entry point, and `From` does
// not chain.
macro_rules! into_command_failure {
    ($($source:ty),+ $(,)?) => {
        $(impl From<$source> for paredit_core_cli::CommandFailure {
            fn from(error: $source) -> Self {
                Self::Error(CliError::from(error))
            }
        })+
    };
}

into_command_failure!(PackageCommandError, PackageRefactorError);
