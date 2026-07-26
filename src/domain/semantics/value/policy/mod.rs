//! Dialect knowledge the value services consult but do not own.

mod foldable_heads;
mod literal_syntax;

pub use foldable_heads::{
    FoldableControlForm, FoldableOperation, foldable_control_form, foldable_operation,
};
pub use literal_syntax::{folds_symbol_case, named_constant};
