//! A `(the TYPE EXPR)` whose `TYPE` provably excludes `EXPR`.
//!
//! # What CLHS says
//!
//! CLHS special operator `the`: the values yielded by the form "must be of the
//! types specified", and the consequences are undefined if they are not. Unlike
//! `check-type`, `the` is an *assertion the compiler may believe* rather than
//! one it must verify: at low safety SBCL will propagate the declared type and
//! generate code that cannot cope with the real one.
//!
//! # What SBCL 2.6.0 does
//!
//! A full `WARNING` on each of the four shapes, including both of the ones in
//! the original proposal:
//!
//! ```lisp
//! (defun bad-6a () (the null (list 1 2)))
//! (defun bad-6b () (the fixnum "s"))
//! (defun bad-6c () (the string 42))
//! (defun bad-6d () (the integer (list 1)))
//! ```
//!
//! ```text
//! ; caught WARNING:
//! ;   Derived type of (LIST 1 2) is (VALUES CONS &OPTIONAL),
//! ;   conflicting with its asserted type NULL.
//! ; caught WARNING:
//! ;   Constant "s" conflicts with its asserted type FIXNUM.
//! ```
//!
//! and it is silent on all eight correct uses tried alongside them, including
//! `(the (values integer integer) (floor 7 2))`, `(the null nil)`,
//! `(the (or null string) x)` and `(the fixnum (funcall f))`. This rule matches
//! that split: it fires only where the expression's type is *obvious* and the
//! declared type is one whose membership is fully modelled.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{list_head, symbol_is};

use crate::support::{LiteralKind, kind_text, literal_kind, type_excludes, type_text};

/// One impossible assertion.
#[derive(Debug, Clone)]
pub struct ImpossibleAssertion {
    /// The whole `(the …)` form.
    pub span: ByteSpan,
    pub declared: String,
    kind: LiteralKind,
}

impl ImpossibleAssertion {
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "this (the {} ...) asserts a type that cannot contain {}, which is what the \
             expression plainly is; at low safety the compiler is entitled to believe the \
             assertion and generate code for a value that never arrives",
            self.declared,
            kind_text(self.kind)
        )
    }
}

/// Reads one `(the TYPE EXPR)`.
///
/// A `the` with any other arity is malformed and is not this rule's subject.
#[must_use]
pub fn examine_the(view: &ExpressionView) -> Option<ImpossibleAssertion> {
    if !list_head(view).is_some_and(|head| symbol_is(head, "the")) {
        return None;
    }
    if view.children.len() != 3 {
        return None;
    }
    let type_spec = view.children.get(1)?;
    let expression = view.children.get(2)?;
    let kind = literal_kind(expression)?;
    if !type_excludes(type_spec, kind) {
        return None;
    }
    Some(ImpossibleAssertion {
        span: view.span,
        declared: type_text(type_spec),
        kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn finding(source: &str) -> Option<ImpossibleAssertion> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        examine_the(&tree.root_view().children[0])
    }

    #[test]
    fn flags_the_references_own_examples() {
        assert!(finding("(the null (list 1 2))").is_some());
        assert!(finding("(the fixnum \"s\")").is_some());
        assert!(finding("(the string 42)").is_some());
        assert!(finding("(the integer (list 1))").is_some());
    }

    #[test]
    fn names_the_declared_type_in_the_finding() {
        let found = finding("(the null (list 1 2))").expect("a finding");
        assert_eq!(found.declared, "null");
        assert!(found.message().contains("a non-empty list"));
    }

    #[test]
    fn flags_a_quoted_list_asserted_to_be_a_number() {
        assert!(finding("(the fixnum '(1 2))").is_some());
    }

    #[test]
    fn flags_a_character_asserted_to_be_a_string() {
        assert!(finding("(the string #\\a)").is_some());
    }

    // -- the correct uses ----------------------------------------------------

    /// Every one of these compiles without a peep from SBCL 2.6.0.
    #[test]
    fn accepts_assertions_that_hold() {
        for source in [
            "(the list (list 1 2))",
            "(the null nil)",
            "(the string \"s\")",
            "(the integer 42)",
            "(the fixnum 0)",
            "(the symbol nil)",
            "(the boolean t)",
            "(the number 1.5)",
            "(the real 1/2)",
        ] {
            assert!(finding(source).is_none(), "{source} is correct");
        }
    }

    /// An expression whose type is not obvious is not this rule's business.
    #[test]
    fn accepts_an_expression_whose_type_cannot_be_read() {
        assert!(finding("(the fixnum (funcall f))").is_none());
        assert!(finding("(the fixnum (+ x 1))").is_none());
        assert!(finding("(the string x)").is_none());
    }

    /// A compound type specifier is declined outright, `(values …)` included.
    #[test]
    fn accepts_a_compound_type_specifier() {
        assert!(finding("(the (values integer integer) (floor 7 2))").is_none());
        assert!(finding("(the (or null string) nil)").is_none());
        assert!(finding("(the (integer 0 10) \"s\")").is_none());
        assert!(finding("(the (simple-array double-float (*)) nil)").is_none());
    }

    #[test]
    fn accepts_an_unmodelled_type_name() {
        assert!(finding("(the my-widget 42)").is_none());
        assert!(finding("(the t 42)").is_none());
        assert!(finding("(the sequence \"s\")").is_none());
    }

    #[test]
    fn declines_a_malformed_the() {
        assert!(finding("(the fixnum)").is_none());
        assert!(finding("(the)").is_none());
        assert!(finding("(the fixnum \"s\" extra)").is_none());
    }

    #[test]
    fn declines_a_form_that_is_not_a_the() {
        assert!(finding("(list null (list 1 2))").is_none());
    }

    /// A string is a vector and a sequence; asserting either of those about a
    /// string is correct and must stay silent.
    #[test]
    fn accepts_a_string_asserted_to_be_a_wider_type() {
        assert!(finding("(the vector \"s\")").is_none());
        assert!(finding("(the sequence \"s\")").is_none());
        assert!(finding("(the array \"s\")").is_none());
    }
}
