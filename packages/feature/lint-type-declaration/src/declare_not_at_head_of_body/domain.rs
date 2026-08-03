//! A `(declare …)` that is not in the declaration section of the body it sits
//! in.
//!
//! # What CLHS says
//!
//! CLHS 3.3.4 ("Declaration Scope") and the `declare` special operator's own
//! page restrict declaration expressions to the *beginning* of the bodies of the
//! forms that accept them; CLHS 3.4.11 fixes the one exception, a documentation
//! string that may sit among them. A `declare` anywhere else "is not a
//! declaration and may be a violation" — the standard's phrasing for undefined
//! behaviour.
//!
//! # What SBCL 2.6.0 does
//!
//! It is a full `ERROR`, not a warning and not silence:
//!
//! ```text
//! $ sbcl --eval '(compile-file "p1.lisp")'
//! ; in: DEFUN BAD-1
//! ;     (DECLARE (FIXNUM X))
//! ; caught ERROR:
//! ;   There is no function named DECLARE.  References to DECLARE in some
//! ;   contexts (like starts of blocks) are unevaluated expressions, but here
//! ;   the expression is being evaluated, which invokes undefined behaviour.
//! ```
//!
//! on both
//!
//! ```lisp
//! (defun bad-1 (x) (print x) (declare (fixnum x)) (+ x 1))
//! (defun bad-1b (x) (let ((y (* x 2))) (print y) (declare (fixnum y)) y))
//! ```
//!
//! and it stays silent on every correct placement — declaration first,
//! docstring then declaration, and a trailing string that is a return value
//! rather than a docstring. That `ERROR` is why this rule is
//! [`Severity::Error`] and [`RuleCategory::Malformed`]: the form does not mean
//! what it says, and the declaration it looks like is simply not in effect.
//!
//! [`Severity::Error`]: paredit_core_lint_engine::model::Severity::Error
//! [`RuleCategory::Malformed`]: paredit_core_lint_engine::model::RuleCategory::Malformed

use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};

use crate::support::{body_start_of, declaration_section_end, form_shape_is_opaque, is_declare};

/// One misplaced declaration.
#[derive(Debug, Clone, Copy)]
pub struct MisplacedDeclare {
    /// The `(declare …)` form itself.
    pub span: ByteSpan,
    /// How many body forms precede it, so the message can say what displaced it.
    pub preceding_forms: usize,
}

impl MisplacedDeclare {
    #[must_use]
    pub fn message(&self) -> String {
        let forms = self.preceding_forms;
        let plural = if forms == 1 { "form" } else { "forms" };
        format!(
            "this (declare ...) comes after {forms} body {plural}, so it is not a declaration \
             at all but a call to the undefined function DECLARE; move it to the head of the \
             body, before the first form"
        )
    }
}

/// Every misplaced `declare` directly inside `view`'s body.
///
/// Only `view`'s **own** body: a nested `let` with its own misplaced declaration
/// is reported when the dispatcher reaches that `let`, not from here, so a
/// finding is never attributed to the wrong form and never reported twice.
///
/// Declines entirely when the body contains a reader conditional. `#+sbcl
/// (declare …)` reads as a single atom under this repository's dialect-aware
/// parse, so the position of everything after it is unknowable: the guarded form
/// may or may not be a declaration depending on the feature set, and either
/// answer would be invented.
#[must_use]
pub fn examine_body(view: &ExpressionView) -> Vec<MisplacedDeclare> {
    let Some(start) = body_start_of(view) else {
        return Vec::new();
    };
    if start >= view.children.len() || form_shape_is_opaque(view, start) {
        return Vec::new();
    }
    let section_end = declaration_section_end(view, start);
    view.children
        .iter()
        .enumerate()
        .skip(section_end)
        .filter(|(_, child)| is_declare(child))
        .map(|(index, child)| MisplacedDeclare {
            span: child.span,
            // Body forms between the end of the declaration section and this
            // declaration. Declarations and the docstring do not displace
            // anything, so the section itself contributes nothing.
            preceding_forms: index - section_end,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn findings(source: &str) -> Vec<MisplacedDeclare> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        examine_body(&tree.root_view().children[0])
    }

    #[test]
    fn flags_a_declare_after_a_body_form_in_a_defun() {
        let found = findings("(defun f (x) (print x) (declare (fixnum x)) (+ x 1))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].preceding_forms, 1);
    }

    #[test]
    fn flags_a_declare_after_a_body_form_in_a_let() {
        assert_eq!(
            findings("(let ((y 1)) (print y) (declare (fixnum y)) y)").len(),
            1
        );
    }

    #[test]
    fn flags_a_declare_in_a_lambda_and_a_locally() {
        assert_eq!(
            findings("(lambda (x) (print x) (declare (fixnum x)))").len(),
            1
        );
        assert_eq!(
            findings("(locally (print 1) (declare (optimize speed)))").len(),
            1
        );
    }

    #[test]
    fn flags_a_declare_past_a_defmethods_qualifier_and_lambda_list() {
        assert_eq!(
            findings("(defmethod area :before ((s square)) (print s) (declare (ignore s)))").len(),
            1
        );
    }

    // -- the correct placements ----------------------------------------------

    #[test]
    fn accepts_a_declaration_at_the_head_of_the_body() {
        assert!(findings("(defun f (x) (declare (fixnum x)) (+ x 1))").is_empty());
    }

    #[test]
    fn accepts_a_declaration_after_a_docstring() {
        assert!(findings("(defun f (x) \"doc\" (declare (fixnum x)) (+ x 1))").is_empty());
    }

    #[test]
    fn accepts_several_declarations_in_a_row() {
        assert!(
            findings("(defun f (x y) \"doc\" (declare (fixnum x)) (declare (ignore y)) x)")
                .is_empty()
        );
    }

    /// A string that is the last form is the return value, and a declaration
    /// before it is correctly placed.
    #[test]
    fn accepts_a_declaration_before_a_trailing_string_return_value() {
        assert!(findings("(defun f (x) (declare (fixnum x)) \"result\")").is_empty());
    }

    #[test]
    fn accepts_a_body_with_no_declaration_at_all() {
        assert!(findings("(defun f (x) (print x) (+ x 1))").is_empty());
    }

    /// The nested case is the enclosing form's finding, not this one's: a
    /// `defun` whose inner `let` is wrong reports nothing *here*, because the
    /// dispatcher visits the `let` separately.
    #[test]
    fn a_nested_forms_misplaced_declaration_is_not_this_forms_finding() {
        assert!(
            findings("(defun f (x) (let ((y 1)) (print y) (declare (fixnum y)) y))").is_empty()
        );
    }

    /// The reader-conditional guard. `#+sbcl (declare (fixnum x))` is one atom,
    /// so the `(declare …)` after it *looks* displaced by one form when it may
    /// in fact be the second declaration of a perfectly ordinary section.
    #[test]
    fn declines_a_body_containing_a_reader_conditional() {
        assert!(
            findings("(defun f (x) #+sbcl (declare (fixnum x)) (declare (ignore x)) 1)").is_empty()
        );
        assert!(findings("(defun f (x) #+sbcl (print x) (declare (fixnum x)) 1)").is_empty());
    }

    /// Reduced from `sbcl/src/compiler/globaldb.lisp`, which the corpus audit
    /// reported as a false positive. The *head* is a folded `#+` atom that still
    /// normalizes to `defmacro`, so the head index dispatches — and the two
    /// conditional atoms shift every later index, putting the lambda list where
    /// the first body form is counted.
    #[test]
    fn declines_a_form_whose_head_is_a_reader_conditional() {
        assert!(
            findings(
                "(#+sb-xc-host cl:defmacro\n #-sb-xc-host sb-xc:defmacro\n define-info-type \
                 ((category kind) &key default)\n  (declare (type keyword category kind))\n  \
                 (pick category))"
            )
            .is_empty()
        );
    }

    /// Reduced from `sbcl/src/code/save.lisp`, likewise a false positive found
    /// by the audit: the documentation string is built by `#.` at read time, so
    /// statically it is a `format` call sitting where a docstring belongs.
    #[test]
    fn declines_a_read_time_evaluated_docstring() {
        assert!(
            findings(
                "(defun save-lisp-and-die (name &key environment-name)\n  \
                 #.(format nil \"Save a core.~%\")\n  (declare (ignore environment-name))\n  \
                 (save name))"
            )
            .is_empty()
        );
    }

    #[test]
    fn declines_a_head_with_no_declaration_section() {
        assert!(findings("(if (print 1) (declare (fixnum x)) 2)").is_empty());
        assert!(findings("(progn (print 1) (declare (fixnum x)))").is_empty());
    }

    #[test]
    fn declines_a_form_too_short_to_have_a_body() {
        assert!(findings("(defun f (x))").is_empty());
        assert!(findings("(let ())").is_empty());
    }

    #[test]
    fn counts_the_forms_that_displaced_the_declaration() {
        let found = findings("(defun f (x) (print x) (print x) (declare (fixnum x)) 1)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].preceding_forms, 2);
    }

    #[test]
    fn flags_every_misplaced_declaration_in_one_body() {
        assert_eq!(
            findings("(defun f (x y) (print x) (declare (fixnum x)) (declare (ignore y)))").len(),
            2
        );
    }
}
