//! A `(declaim …)` sitting where a `(declare …)` belongs.
//!
//! # What CLHS says
//!
//! `declaim` (CLHS macro `declaim`) makes a **global** proclamation and is a
//! top-level form; `declare` (CLHS symbol `declare`) attaches a declaration to
//! the form it is inside. They are not interchangeable, and the difference is
//! invisible at a glance: `(declaim (fixnum x))` inside a function does not
//! constrain the parameter `x`, it proclaims a global type for the *symbol* `x`,
//! affecting every special binding of that name in the image.
//!
//! # What SBCL 2.6.0 does
//!
//! It has a message for exactly this confusion:
//!
//! ```text
//! ; in: DEFUN F11B
//! ;     (DEFUN F11B (X) (DECLAIM (FIXNUM X)) (+ X 1))
//! ; caught STYLE-WARNING:
//! ;   DECLAIM where DECLARE was probably intended
//! ```
//!
//! on
//!
//! ```lisp
//! (defun f11b (x) (declaim (fixnum x)) (+ x 1))
//! ```
//!
//! The rule is scoped to the same position SBCL's own check is: a `declaim`
//! among a body's *leading* declarations. A `declaim` deeper in a body is a
//! deliberate runtime call to change global policy — unusual, but a different
//! thing, and not this rule's subject.

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

use crate::support::{
    body_start_of, is_declaim, is_declare, is_reader_conditional, is_string_literal,
};

/// One `declaim` in a declaration section.
#[derive(Debug, Clone, Copy)]
pub struct MisusedDeclaim {
    pub span: ByteSpan,
}

impl MisusedDeclaim {
    #[must_use]
    pub fn message(&self) -> String {
        "(declaim ...) here makes a global proclamation about these symbols rather than \
         declaring anything about this form's own bindings; DECLARE was probably intended"
            .to_owned()
    }
}

/// Every `declaim` among `view`'s leading declarations.
///
/// The scan walks the same prefix [`crate::support::declaration_section_end`]
/// does — declarations and at most one documentation string — but treats a
/// `declaim` as part of that prefix so that a second, correctly spelled
/// `declare` after it is still reached.
#[must_use]
pub fn examine_body(view: &ExpressionView) -> Vec<MisusedDeclaim> {
    let Some(start) = body_start_of(view) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut index = start;
    let mut seen_doc = false;
    while let Some(child) = view.children.get(index) {
        if is_reader_conditional(child) {
            // The prefix cannot be read past a form the reader folded away.
            break;
        }
        if is_declaim(child) {
            found.push(MisusedDeclaim { span: child.span });
        } else if !is_declare(child) {
            let is_doc = !seen_doc && is_string_literal(child) && index + 1 < view.children.len();
            if !is_doc {
                break;
            }
            seen_doc = true;
        }
        index += 1;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn findings(source: &str) -> Vec<MisusedDeclaim> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        examine_body(&tree.root_view().children[0])
    }

    #[test]
    fn flags_a_declaim_as_the_first_body_form() {
        assert_eq!(
            findings("(defun f (x) (declaim (fixnum x)) (+ x 1))").len(),
            1
        );
    }

    #[test]
    fn flags_a_declaim_after_a_correct_declaration() {
        assert_eq!(
            findings("(defun f (x y) (declare (ignore y)) (declaim (fixnum x)) x)").len(),
            1
        );
    }

    #[test]
    fn flags_a_declaim_after_a_docstring() {
        assert_eq!(
            findings("(defun f (x) \"doc\" (declaim (fixnum x)) (+ x 1))").len(),
            1
        );
    }

    #[test]
    fn flags_a_declaim_in_a_let_and_a_lambda() {
        assert_eq!(findings("(let ((x 1)) (declaim (fixnum x)) x)").len(), 1);
        assert_eq!(findings("(lambda (x) (declaim (fixnum x)) x)").len(), 1);
    }

    // -- what must stay silent -----------------------------------------------

    #[test]
    fn accepts_a_correctly_spelled_declaration() {
        assert!(findings("(defun f (x) (declare (fixnum x)) (+ x 1))").is_empty());
    }

    /// A `declaim` past the declaration section is a deliberate runtime call to
    /// change global policy, not the `declare`/`declaim` confusion.
    #[test]
    fn accepts_a_declaim_past_the_declaration_section() {
        assert!(findings("(defun f (x) (print x) (declaim (optimize speed)) x)").is_empty());
    }

    #[test]
    fn accepts_a_body_with_no_declaim() {
        assert!(findings("(defun f (x) (print x) (+ x 1))").is_empty());
    }

    #[test]
    fn declines_a_head_with_no_declaration_section() {
        assert!(findings("(progn (declaim (optimize speed)) 1)").is_empty());
        assert!(findings("(if a (declaim (optimize speed)) 1)").is_empty());
    }

    #[test]
    fn declines_a_body_whose_prefix_is_hidden_behind_a_reader_conditional() {
        assert!(
            findings("(defun f (x) #+sbcl (declare (fixnum x)) (declaim (fixnum x)) 1)").is_empty()
        );
    }

    #[test]
    fn declines_a_form_too_short_to_have_a_body() {
        assert!(findings("(defun f (x))").is_empty());
    }

    #[test]
    fn flags_each_declaim_in_one_section() {
        assert_eq!(
            findings("(defun f (x y) (declaim (fixnum x)) (declaim (fixnum y)) 1)").len(),
            2
        );
    }
}
