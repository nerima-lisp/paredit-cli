//! Why a conditional conversion refuses to run.
//!
//! Section 9.2. The interesting part of this package's conversion is how
//! little of it is new: six of its eleven refusals are already
//! [`paredit_core_edit::EditRefusal`] variants — the dialect check, the
//! input/output parse checks, the comment check, the two shape checks — with
//! `operation` set to `"conditional conversion"`.
//!
//! That is the measurement from core/edit's README repeating one package out:
//! the refusal vocabulary is shared, and only the reasons that are genuinely
//! about *this* conversion need types of their own. Those are the five below,
//! each about the arity or shape of a specific `if`/`when`/`unless` form.

use thiserror::Error;

/// The selected form is not the conditional the requested conversion needs.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConditionalShapeError {
    #[error("convert-when-to-if requires a test")]
    WhenHasNoTest,

    #[error("convert-unless-to-if requires a test")]
    UnlessHasNoTest,

    #[error("convert-if-to-when requires (if test then [nil])")]
    IfIsNotWhenShaped,

    #[error("convert-if-to-when requires no else form or a literal nil else")]
    IfHasNonNilElse,

    #[error("convert-if-to-unless requires (if test nil else)")]
    IfIsNotUnlessShaped,
}

/// Anything a conditional conversion can refuse to do.
#[derive(Debug, PartialEq, Eq, Error)]
pub enum ConditionalConversionError {
    /// A refusal this package shares with every other structural edit.
    #[error(transparent)]
    Edit(#[from] paredit_core_edit::EditRefusal),

    #[error(transparent)]
    Shape(#[from] ConditionalShapeError),
}

// `From` does not chain: `DialectRefusal -> EditRefusal -> ConditionalConversionError`
// needs the middle step spelled out, or every call site would have to write
// `EditRefusal::from(x).into()`. These four are the sub-enums this package
// raises directly.
macro_rules! from_edit_refusal {
    ($($ty:ident),+ $(,)?) => {
        $(impl From<paredit_core_edit::$ty> for ConditionalConversionError {
            fn from(error: paredit_core_edit::$ty) -> Self {
                Self::Edit(error.into())
            }
        })+
    };
}

from_edit_refusal!(
    DialectRefusal,
    DocumentRefusal,
    ConservativeRefusal,
    ShapeRefusal,
);

impl From<paredit_core_syntax::sexpr::SexprError> for ConditionalConversionError {
    fn from(error: paredit_core_syntax::sexpr::SexprError) -> Self {
        Self::Edit(error.into())
    }
}

/// The result type the conditional conversion planners return.
pub type ConditionalConversionResult<T> = std::result::Result<T, ConditionalConversionError>;
