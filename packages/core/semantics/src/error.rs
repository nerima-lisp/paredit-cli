//! Typed failures for the semantic analyses.
//!
//! Section 9.2. Everything this package can refuse is the same refusal in
//! different words: *this is not a binding form I can read*. A `let` whose
//! bindings are a vector in a dialect that spells them as pairs, a binding
//! pair with three elements, a destructuring pattern that binds nothing.
//!
//! That matters to a caller, because a semantic analysis failing is not the
//! same as a semantic analysis finding nothing. The binding table is built
//! from source that has already parsed, so a malformed binding form means the
//! dialect tables disagree with the document — a rule that reads such a
//! failure as "no bindings here" reports a bug in working code, which is the
//! rule the architecture guide states as "a fact is recorded only when it is
//! provable".
//!
//! Messages are reproduced exactly; §9.2's goal is type-level distinction.

use thiserror::Error;

/// A binding form that the lexical-scope pass cannot read.
///
/// The variants name *which* shape assumption failed, so a caller can tell a
/// dialect mismatch (the form is well-formed, just not for this dialect) from
/// a malformed form (no dialect would accept it).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BindingFormError {
    // --- the dialect tables and the document disagree ---
    #[error("unknown binding form delimiter")]
    UnknownDelimiter,

    #[error("dialect expects vector let bindings: [name value ...]")]
    ExpectedVectorBindings,

    #[error("dialect expects list-pair let bindings: ((name value) ...)")]
    ExpectedListPairBindings,

    // --- no dialect would accept this shape ---
    #[error("vector let binding form must contain name/value pairs")]
    VectorBindingsNotPaired,

    #[error("let binding must be a name, (name), or (name value)")]
    BindingNotANameOrPair,

    #[error("bare let binding must contain one binding name")]
    BareBindingNotSingle,

    #[error("let binding pair must be (name) or (name value)")]
    BindingPairWrongArity,

    #[error("let binding pattern must contain at least one binding name")]
    PatternBindsNothing,
}

/// A one-based binding index that is not one-based.
///
/// Its own type rather than a [`BindingFormError`] variant because it comes
/// from a CLI argument (`--binding-index`) rather than from a document.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("binding index must be greater than zero")]
pub struct BindingIndexError;

/// The result type the binding-form readers return.
pub type BindingFormResult<T> = std::result::Result<T, BindingFormError>;

#[cfg(test)]
mod tests {
    use super::*;

    /// The two failure families a caller treats differently: one says the
    /// document is fine but written for another dialect, the other says no
    /// dialect would accept it. Before §9.2 both were the same `anyhow::Error`.
    #[test]
    fn dialect_mismatches_are_distinguishable_from_malformed_forms() {
        let dialect_mismatch = [
            BindingFormError::ExpectedVectorBindings,
            BindingFormError::ExpectedListPairBindings,
        ];
        let malformed = [
            BindingFormError::VectorBindingsNotPaired,
            BindingFormError::BindingNotANameOrPair,
            BindingFormError::BareBindingNotSingle,
            BindingFormError::BindingPairWrongArity,
            BindingFormError::PatternBindsNothing,
        ];
        for error in &dialect_mismatch {
            assert!(error.to_string().starts_with("dialect expects "));
            assert!(!malformed.contains(error));
        }
        for error in &malformed {
            assert!(!dialect_mismatch.contains(error));
        }
    }

    /// A binding index comes from `--binding-index`, not from a document, so it
    /// is not a `BindingFormError` variant.
    #[test]
    fn a_binding_index_failure_is_its_own_type() {
        assert_eq!(
            BindingIndexError.to_string(),
            "binding index must be greater than zero"
        );
    }
}
