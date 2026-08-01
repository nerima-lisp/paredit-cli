//! The operators this layer is allowed to evaluate.
//!
//! A whitelist, never a heuristic. Everything here is a standard operator with
//! no side effects, no dependence on the environment, and a result this layer
//! can compute exactly in `i128`. An operator that is merely *probably* pure
//! does not belong: a wrong `Known` is worse than no answer at all, because
//! rules downstream trust `Known` absolutely.

use paredit_core_syntax::dialect::Dialect;

/// A pure operation on constants that can be evaluated at analysis time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldableOperation {
    /// `+`, `-`, `*` over integers.
    Add,
    Subtract,
    Multiply,
    /// `/` — folded only when the division is exact. Common Lisp's `/` yields
    /// a rational otherwise, which this layer does not model.
    Divide,
    /// `1+`, `1-`.
    Increment,
    Decrement,
    Min,
    Max,
    Abs,
    /// `=`, `<`, `>`, `<=`, `>=` — yield a boolean.
    NumericEqual,
    Less,
    Greater,
    LessOrEqual,
    GreaterOrEqual,
    /// `zerop`, `plusp`, `minusp`, `evenp`, `oddp` — yield a boolean.
    Zerop,
    Plusp,
    Minusp,
    Evenp,
    Oddp,
    /// `not`, `null` — yield a boolean from any value's truthiness.
    Not,
}

/// A special form whose value is one of its subforms, chosen by a test this
/// layer may be able to evaluate.
///
/// These are not functions: their arguments are not all evaluated, which is
/// exactly why folding them is useful — the branch not taken need not be
/// constant, or even analysable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldableControlForm {
    /// `(if test then else)`.
    If,
    /// `(when test body…)` — the value is the last body form, or `nil`.
    When,
    /// `(unless test body…)`.
    Unless,
    /// `(and forms…)` — the first false form, or the last.
    And,
    /// `(or forms…)` — the first true form, or the last.
    Or,
}

const COMMON_LISP_OPERATIONS: [(&str, FoldableOperation); 21] = [
    ("+", FoldableOperation::Add),
    ("-", FoldableOperation::Subtract),
    ("*", FoldableOperation::Multiply),
    ("/", FoldableOperation::Divide),
    ("1+", FoldableOperation::Increment),
    ("1-", FoldableOperation::Decrement),
    ("min", FoldableOperation::Min),
    ("max", FoldableOperation::Max),
    ("abs", FoldableOperation::Abs),
    ("=", FoldableOperation::NumericEqual),
    ("<", FoldableOperation::Less),
    (">", FoldableOperation::Greater),
    ("<=", FoldableOperation::LessOrEqual),
    (">=", FoldableOperation::GreaterOrEqual),
    ("zerop", FoldableOperation::Zerop),
    ("plusp", FoldableOperation::Plusp),
    ("minusp", FoldableOperation::Minusp),
    ("evenp", FoldableOperation::Evenp),
    ("oddp", FoldableOperation::Oddp),
    ("not", FoldableOperation::Not),
    ("null", FoldableOperation::Not),
];

/// `if`/`when`/`unless`/`and`/`or`, spelled and behaving identically in
/// Common Lisp and Emacs Lisp — both are Lisp-1s with `nil` as the one false
/// value and the same short-circuit `and`/`or` — so one table serves both,
/// compared case-sensitively for Emacs Lisp and case-insensitively for
/// Common Lisp the same way [`foldable_operation`] does.
const SHARED_CONTROL_FORMS: [(&str, FoldableControlForm); 5] = [
    ("if", FoldableControlForm::If),
    ("when", FoldableControlForm::When),
    ("unless", FoldableControlForm::Unless),
    ("and", FoldableControlForm::And),
    ("or", FoldableControlForm::Or),
];

/// Emacs Lisp's own arithmetic and comparison primitives that fold
/// identically to their Common Lisp counterparts above.
///
/// Deliberately a subset, not a copy: `plusp`, `minusp`, `evenp`, and `oddp`
/// are Common Lisp names Emacs Lisp does not have without the `cl-` prefix
/// (`cl-plusp`, …), and this layer only ever compares the bare head text —
/// guessing that a file loaded `cl-lib`'s old unprefixed compatibility
/// aliases would be exactly the kind of probably-pure guess the module doc
/// above rules out.
const EMACS_LISP_OPERATIONS: [(&str, FoldableOperation); 17] = [
    ("+", FoldableOperation::Add),
    ("-", FoldableOperation::Subtract),
    ("*", FoldableOperation::Multiply),
    ("/", FoldableOperation::Divide),
    ("1+", FoldableOperation::Increment),
    ("1-", FoldableOperation::Decrement),
    ("min", FoldableOperation::Min),
    ("max", FoldableOperation::Max),
    ("abs", FoldableOperation::Abs),
    ("=", FoldableOperation::NumericEqual),
    ("<", FoldableOperation::Less),
    (">", FoldableOperation::Greater),
    ("<=", FoldableOperation::LessOrEqual),
    (">=", FoldableOperation::GreaterOrEqual),
    ("zerop", FoldableOperation::Zerop),
    ("not", FoldableOperation::Not),
    ("null", FoldableOperation::Not),
];

/// The pure operation `head` names, if `dialect` has one.
///
/// Common Lisp and Emacs Lisp only. Other dialects reach this layer through
/// the same skeleton but with an empty table, so they yield `Unknown`
/// everywhere rather than borrowing semantics that may not hold.
#[must_use]
pub fn foldable_operation(dialect: Dialect, head: &str) -> Option<FoldableOperation> {
    match dialect {
        Dialect::CommonLisp => COMMON_LISP_OPERATIONS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(head))
            .map(|(_, operation)| *operation),
        // Emacs Lisp reads symbols case-sensitively: folding `ZEROP` and
        // `zerop` together the Common Lisp way would treat two distinct
        // symbols as one operator.
        Dialect::EmacsLisp => EMACS_LISP_OPERATIONS
            .iter()
            .find(|(name, _)| *name == head)
            .map(|(_, operation)| *operation),
        _ => None,
    }
}

/// The branching form `head` names, if `dialect` has one.
#[must_use]
pub fn foldable_control_form(dialect: Dialect, head: &str) -> Option<FoldableControlForm> {
    match dialect {
        Dialect::CommonLisp => SHARED_CONTROL_FORMS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(head))
            .map(|(_, form)| *form),
        Dialect::EmacsLisp => SHARED_CONTROL_FORMS
            .iter()
            .find(|(name, _)| *name == head)
            .map(|(_, form)| *form),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(
            foldable_operation(Dialect::CommonLisp, "ZEROP"),
            Some(FoldableOperation::Zerop)
        );
        assert_eq!(
            foldable_control_form(Dialect::CommonLisp, "IF"),
            Some(FoldableControlForm::If)
        );
    }

    #[test]
    fn null_and_not_fold_identically() {
        assert_eq!(
            foldable_operation(Dialect::CommonLisp, "null"),
            foldable_operation(Dialect::CommonLisp, "not")
        );
    }

    #[test]
    fn an_impure_or_unmodelled_operator_is_absent() {
        for head in ["random", "read", "gensym", "expt", "mod", "sqrt", "list"] {
            assert_eq!(
                foldable_operation(Dialect::CommonLisp, head),
                None,
                "{head} must not be foldable"
            );
        }
    }

    #[test]
    fn other_dialects_fold_nothing() {
        assert_eq!(foldable_operation(Dialect::Scheme, "+"), None);
        assert_eq!(foldable_control_form(Dialect::Clojure, "if"), None);
    }

    #[test]
    fn emacs_lisp_folds_its_own_arithmetic_and_control_forms() {
        assert_eq!(
            foldable_operation(Dialect::EmacsLisp, "+"),
            Some(FoldableOperation::Add)
        );
        assert_eq!(
            foldable_operation(Dialect::EmacsLisp, "zerop"),
            Some(FoldableOperation::Zerop)
        );
        assert_eq!(
            foldable_control_form(Dialect::EmacsLisp, "when"),
            Some(FoldableControlForm::When)
        );
    }

    #[test]
    fn emacs_lisp_lookup_is_case_sensitive() {
        // Emacs Lisp has no reader-level case folding: `ZEROP` and `zerop`
        // are two different symbols, unlike Common Lisp's `ZEROP`/`zerop`.
        assert_eq!(foldable_operation(Dialect::EmacsLisp, "ZEROP"), None);
        assert_eq!(foldable_control_form(Dialect::EmacsLisp, "IF"), None);
    }

    #[test]
    fn emacs_lisp_does_not_borrow_common_lisps_unprefixed_cl_lib_predicates() {
        for head in ["plusp", "minusp", "evenp", "oddp"] {
            assert_eq!(
                foldable_operation(Dialect::EmacsLisp, head),
                None,
                "{head} is only `cl-{head}` in Emacs Lisp"
            );
        }
    }
}
