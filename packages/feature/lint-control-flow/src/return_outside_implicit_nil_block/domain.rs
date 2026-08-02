//! Common Lisp orphaned-`return` detection: a `(return …)` with no enclosing
//! form to return *from*.
//!
//! `(return x)` is defined as `(return-from nil x)` (CLHS `return`), so it
//! needs a lexically enclosing block named `nil`. Exactly these forms
//! establish one, and the list is closed — CLHS says so in as many words for
//! each:
//!
//! - `(block nil …)`, spelled out.
//! - `do`, `do*`, `dolist`, `dotimes`, `prog`, `prog*`.
//! - `do-symbols`, `do-external-symbols`, `do-all-symbols`.
//! - `loop` — **unless** it carries a `named` clause. CLHS 6.1.1.4 gives a
//!   `named` loop a block of *that* name instead of `nil`, so
//!   `(loop named outer do (return 1))` is an error and
//!   `(loop do (return 1))` is not. Reading every `loop` as establishing `nil`
//!   would miss the first; reading none as establishing it would flag every
//!   ordinary loop, which is the single most common `return` in Common Lisp.
//!
//! What is *not* on the list matters as much: `defun`, `defmethod`, `lambda`,
//! `flet` bindings, `handler-case`, `restart-case`, `case`, `cond`, `when`,
//! `let` and `mapcar` establish no `nil` block. `(defun f () (return 1))` is a
//! `control-error`, not a shorthand for returning from `f`, which is the
//! confusion this rule exists to catch.
//!
//! # Why this reports so little
//!
//! The walk outward stops — reporting nothing — at any head that is not a
//! standard Common Lisp operator, because a project macro may expand to a
//! `loop` or a `block nil` and this file cannot see that it does. Preferring
//! a false negative to a false positive is deliberate.
//!
//! # Relationship to the neighbouring rules
//!
//! `explicit-nil-return` matches the same head but asks a disjoint question —
//! whether the *result operand* is a redundant literal `nil` — so
//! `(return nil)` written outside any block earns one finding from each, which
//! is two true statements about it. `return-from-unmatched-block` owns the
//! `(return-from nil …)` spelling; this rule owns `(return …)`.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{BlockScope, block_scope, with_lexical_chain};

#[derive(Debug, Clone)]
pub struct ReturnOutsideImplicitNilBlockItem {
    /// The span of the whole `(return …)` form.
    pub span: ByteSpan,
}

impl Finding for ReturnOutsideImplicitNilBlockItem {
    fn kind(&self) -> &'static str {
        "return-outside-implicit-nil-block"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn message(&self) -> String {
        MESSAGE.to_owned()
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
pub const MESSAGE: &str = "return exits the implicit block named nil, which no enclosing form establishes; \
     only do/dolist/dotimes/loop/prog and an explicit (block nil …) do";

/// The block `(return …)` names. `return` *is* `return-from nil`.
const IMPLICIT_BLOCK: &str = "nil";

/// What the walk outward concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    Established,
    Unestablished,
    Unknown,
}

fn resolve(tree: &SyntaxTree, span: ByteSpan) -> Resolution {
    with_lexical_chain(tree, span, |chain| {
        if chain.unevaluated {
            return Resolution::Unknown;
        }
        for index in chain.ancestors_inward() {
            match block_scope(&chain.nodes, index) {
                BlockScope::Named(established) if established == IMPLICIT_BLOCK => {
                    return Resolution::Established;
                }
                BlockScope::Unknown => return Resolution::Unknown,
                _ => {}
            }
        }
        Resolution::Unestablished
    })
    .unwrap_or(Resolution::Unknown)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_return(
    tree: &SyntaxTree,
    view: &ExpressionView,
    return_form_count: &mut usize,
    violations: &mut Vec<ReturnOutsideImplicitNilBlockItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "return")) {
        return;
    }
    *return_form_count += 1;

    // `(return)` and `(return result)` are the only two shapes CLHS defines.
    if view.children.len() > 2 {
        return;
    }

    if resolve(tree, view.span) == Resolution::Unestablished {
        violations.push(ReturnOutsideImplicitNilBlockItem { span: view.span });
    }
}

/// Collects every orphaned `return` in one file, with the number of `return`
/// forms scanned as the denominator beside them.
pub fn build_return_outside_implicit_nil_block_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ReturnOutsideImplicitNilBlockItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("return_form_count", json!(0))],
        ));
    }

    let mut return_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_return(tree, subview, &mut return_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("return_form_count", json!(return_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ReturnOutsideImplicitNilBlockItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_return_outside_implicit_nil_block_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn count(input: &str) -> usize {
        report(input).findings.len()
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_return_directly_inside_a_defun() {
        assert_eq!(count("(defun f () (return 1))"), 1);
    }

    #[test]
    fn flags_a_return_at_top_level() {
        assert_eq!(count("(return)"), 1);
    }

    #[test]
    fn flags_a_return_inside_a_lambda() {
        assert_eq!(count("(defun f (l) (mapcar (lambda (x) (return x)) l))"), 1);
    }

    /// The trap CLHS 6.1.1.4 sets: `named` replaces the `nil` block.
    #[test]
    fn flags_a_return_inside_a_named_loop() {
        assert_eq!(count("(loop named outer do (return 1))"), 1);
    }

    #[test]
    fn flags_a_return_inside_a_let_under_a_defun() {
        assert_eq!(count("(defun f () (let ((x 1)) (return x)))"), 1);
    }

    /// A `block` with a *name* is not a block named nil.
    #[test]
    fn flags_a_return_inside_a_named_block() {
        assert_eq!(count("(block outer (return 1))"), 1);
    }

    // -- near-miss negatives ------------------------------------------------

    #[test]
    fn does_not_flag_a_return_inside_any_iteration_macro() {
        for source in [
            "(dolist (x l) (return 1))",
            "(dotimes (i 3) (return 1))",
            "(do ((i 0 (1+ i))) ((= i 3)) (return 1))",
            "(do* ((i 0 (1+ i))) ((= i 3)) (return 1))",
            "(prog () (return 1))",
            "(prog* () (return 1))",
            "(do-symbols (s) (return 1))",
            "(do-external-symbols (s) (return 1))",
            "(do-all-symbols (s) (return 1))",
        ] {
            assert_eq!(count(source), 0, "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_return_inside_a_plain_loop() {
        assert_eq!(count("(loop for x in l do (return x))"), 0);
        assert_eq!(count("(loop (return 1))"), 0);
    }

    #[test]
    fn does_not_flag_a_return_inside_an_explicit_nil_block() {
        assert_eq!(count("(block nil (return 1))"), 0);
    }

    /// The shape this rule exists for, written correctly: the loop is what the
    /// return exits, not the defun.
    #[test]
    fn does_not_flag_a_return_nested_deep_inside_a_loop() {
        assert_eq!(
            count("(defun f (l)\n  (dolist (x l)\n    (when (foo x)\n      (return x))))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_return_in_a_cond_clause_inside_a_loop() {
        assert_eq!(
            count("(dolist (x l) (cond ((foo x) (return x)) (t nil)))"),
            0
        );
    }

    #[test]
    fn does_not_flag_a_return_under_an_unknown_macro() {
        assert_eq!(count("(with-my-loop (return 1))"), 0);
        assert_eq!(count("(defun f () (with-my-loop (return 1)))"), 0);
    }

    #[test]
    fn does_not_flag_a_malformed_return() {
        assert_eq!(count("(return 1 2)"), 0);
    }

    #[test]
    fn does_not_flag_return_from() {
        assert_eq!(count("(defun f () (return-from f 1))"), 0);
        assert_eq!(
            report("(defun f () (return-from f 1))").summary[0].1,
            json!(0)
        );
    }

    #[test]
    fn case_folds_and_ignores_the_package_qualifier() {
        assert_eq!(count("(defun f () (CL:RETURN 1))"), 1);
        assert_eq!(count("(dolist (x l) (CL:RETURN 1))"), 0);
    }

    // -- the five quote shapes ---------------------------------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert_eq!(count("'(return 1)"), 0);
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert_eq!(count("(quote (return 1))"), 0);
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert_eq!(count("'(a ,(return 1))"), 0);
    }

    #[test]
    fn does_not_flag_a_quasiquoted_macro_template() {
        assert_eq!(count("(defmacro m () `(return 1))"), 0);
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(count("(defmacro m () `(progn ,(return 1)))"), 1);
    }

    /// The same shape under an unknown head reports nothing, and for a reason
    /// that is *not* the quote state: `(a …)` may be a macro that expands to a
    /// `loop`. Pinned beside the test above so a change to either the quote
    /// handling or the `Unknown` stop cannot be mistaken for the other.
    #[test]
    fn an_unquoted_form_under_an_unknown_head_is_still_unknown() {
        assert_eq!(count("(defmacro m () `(a ,(return 1)))"), 0);
    }

    // -- strings ------------------------------------------------------------

    #[test]
    fn does_not_flag_a_return_inside_a_string_literal() {
        assert_eq!(count("(defun f () \"(return 1)\")"), 0);
    }

    // -- report shape -------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(return 1)", Dialect::Clojure).expect("parse");
        let report = build_return_outside_implicit_nil_block_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("return_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_return_scanned_not_only_the_flagged_ones() {
        let report = report("(defun f () (return 1))\n(dolist (x l) (return 2))\n");
        assert_eq!(report.summary, vec![("return_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_kind() {
        let report = report("(defun f ()\n  (return 1))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "return-outside-implicit-nil-block");
        assert!(finding.json_fields().is_empty());
        assert!(finding.message().contains("nil"));
    }
}
