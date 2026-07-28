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
