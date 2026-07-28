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

const COMMON_LISP_CONTROL: [(&str, FoldableControlForm); 5] = [
    ("if", FoldableControlForm::If),
    ("when", FoldableControlForm::When),
    ("unless", FoldableControlForm::Unless),
    ("and", FoldableControlForm::And),
    ("or", FoldableControlForm::Or),
];

/// The pure operation `head` names, if `dialect` has one.
///
/// Only Common Lisp for now. Other dialects reach this layer through the same
/// skeleton but with an empty table, so they yield `Unknown` everywhere rather
/// than borrowing CLHS semantics that may not hold.
#[must_use]
pub fn foldable_operation(dialect: Dialect, head: &str) -> Option<FoldableOperation> {
    if dialect != Dialect::CommonLisp {
        return None;
    }
    COMMON_LISP_OPERATIONS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(head))
        .map(|(_, operation)| *operation)
}

/// The branching form `head` names, if `dialect` has one.
#[must_use]
pub fn foldable_control_form(dialect: Dialect, head: &str) -> Option<FoldableControlForm> {
    if dialect != Dialect::CommonLisp {
        return None;
    }
    COMMON_LISP_CONTROL
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(head))
        .map(|(_, form)| *form)
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
}
