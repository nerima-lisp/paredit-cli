//! Common Lisp unmatched-`return-from` detection: a `(return-from name …)`
//! whose `name` no lexically enclosing form establishes as a block.
//!
//! `return-from` is *lexical* (CLHS 5.2): the block it names must be
//! established by a form that textually encloses it, by `block`, by the
//! implicit block a `defun`/`defmacro`/`defmethod` wraps around its body, by
//! the implicit `nil` block of `do`/`dolist`/`dotimes`/`loop`/`prog`, or by an
//! `flet`/`labels`/`macrolet` binding's own implicit block. A name none of
//! those provides is a `control-error` in every conforming implementation —
//! this is not a style complaint.
//!
//! # Why this reports so little
//!
//! The one thing this analysis cannot see is a macro expansion.
//! `(with-retry (return-from retry 1))` is a perfectly good program if
//! `with-retry` expands to `(block retry …)`, and nothing in the file says
//! whether it does. So the walk outward stops — reporting nothing — the moment
//! it meets a head that is not a standard Common Lisp operator
//! (`BlockScope::Unknown`). A false negative on every file that wraps its
//! exits in a project macro is the deliberate price of never flagging one.
//!
//! Two more deliberate false negatives:
//!
//! - An `flet`/`labels`/`macrolet` binding named `foo` is read as providing
//!   block `foo` to the *whole* form, though CLHS scopes it to that binding's
//!   own body. `(flet ((foo () 1)) (return-from foo 2))` is therefore missed.
//! - `(return-from nil …)` is checked like any other name, against the
//!   implicit `nil` block. That is the one shape this rule shares with
//!   `return-outside-implicit-nil-block`, which owns the `(return …)`
//!   spelling; the two are disjoint by head.
//!
//! Scope: Common Lisp only. `return-from` is not an operator in the other nine
//! dialects.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{BlockScope, block_scope, plain_name, with_lexical_chain};

#[derive(Debug, Clone)]
pub struct ReturnFromUnmatchedBlockItem {
    /// The span of the whole `(return-from …)` form.
    pub span: ByteSpan,
    /// The block name it names, normalized (lowercased, package-unqualified).
    pub block_name: String,
}

impl Finding for ReturnFromUnmatchedBlockItem {
    fn kind(&self) -> &'static str {
        "return-from-unmatched-block"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("block={}", self.block_name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("block", json!(self.block_name))]
    }

    fn message(&self) -> String {
        message_for(&self.block_name)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(block_name: &str) -> String {
    format!(
        "return-from names the block `{block_name}`, which no enclosing form establishes; \
         a block name is lexical"
    )
}

/// What the walk outward concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    /// An enclosing form establishes the named block.
    Established,
    /// Nothing enclosing it does, and every enclosing form was one this
    /// analysis can read.
    Unestablished,
    /// The question could not be answered: the form is quoted data, or a head
    /// this analysis cannot read stands between it and the top level.
    Unknown,
}

/// Whether the block named by a `return-from` at `span` is established.
fn resolve(tree: &SyntaxTree, span: ByteSpan, name: &str) -> Resolution {
    with_lexical_chain(tree, span, |chain| {
        if chain.unevaluated {
            return Resolution::Unknown;
        }
        for index in chain.ancestors_inward() {
            match block_scope(&chain.nodes, index) {
                BlockScope::Named(established) if established == name => {
                    return Resolution::Established;
                }
                BlockScope::LocalFunctions(names) if names.iter().any(|each| each == name) => {
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
///
/// The tree is needed because the answer is about the node's *ancestors*, and
/// a matched node carries no parent pointer. Only the one enclosing top-level
/// form is ever materialized — see `crate::support::with_lexical_chain`.
pub fn examine_return_from(
    tree: &SyntaxTree,
    view: &ExpressionView,
    return_from_form_count: &mut usize,
    violations: &mut Vec<ReturnFromUnmatchedBlockItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "return-from")) {
        return;
    }
    *return_from_form_count += 1;

    // `(return-from name)` and `(return-from name result)` are the only two
    // shapes CLHS defines; anything else is malformed, which is a different
    // subject and not one to guess a block name out of.
    if view.children.len() < 2 || view.children.len() > 3 {
        return;
    }
    let Some(name) = view.children.get(1).and_then(plain_name) else {
        return;
    };

    if resolve(tree, view.span, &name) == Resolution::Unestablished {
        violations.push(ReturnFromUnmatchedBlockItem {
            span: view.span,
            block_name: name,
        });
    }
}

/// Collects every unmatched `return-from` in one file, with the number of
/// `return-from` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every return-from here resolves" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_return_from_unmatched_block_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ReturnFromUnmatchedBlockItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("return_from_form_count", json!(0))],
        ));
    }

    let mut return_from_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_return_from(tree, subview, &mut return_from_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("return_from_form_count", json!(return_from_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ReturnFromUnmatchedBlockItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_return_from_unmatched_block_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn names(input: &str) -> Vec<String> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.block_name)
            .collect()
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_return_from_naming_no_enclosing_block() {
        assert_eq!(names("(defun f () (return-from g 1))"), vec!["g"]);
    }

    #[test]
    fn flags_a_return_from_at_top_level() {
        assert_eq!(names("(return-from nowhere)"), vec!["nowhere"]);
    }

    /// The block a sibling `block` establishes does not enclose this one.
    #[test]
    fn flags_a_return_from_naming_a_sibling_block() {
        assert_eq!(
            names("(defun f () (block a 1) (return-from a 2))"),
            vec!["a"]
        );
    }

    #[test]
    fn flags_a_return_from_naming_a_block_that_only_encloses_a_sibling() {
        assert_eq!(
            names("(defun f ()\n  (when t (block inner 1))\n  (return-from inner 2))"),
            vec!["inner"]
        );
    }

    /// CLHS 6.1.1.4: `named` replaces the loop's `nil` block, so `nil` is not
    /// established here.
    #[test]
    fn flags_a_return_from_nil_inside_a_named_loop() {
        assert_eq!(
            names("(loop named outer do (return-from nil 1))"),
            vec!["nil"]
        );
    }

    // -- near-miss negatives ------------------------------------------------

    #[test]
    fn does_not_flag_a_return_from_the_enclosing_defun() {
        assert!(names("(defun f () (return-from f 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_return_from_an_enclosing_block() {
        assert!(names("(block a (return-from a 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_return_from_through_transparent_forms() {
        assert!(
            names("(defun f (l)\n  (dolist (x l)\n    (when (foo x)\n      (return-from f x))))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_return_from_nil_inside_an_iteration_macro() {
        for source in [
            "(dolist (x l) (return-from nil 1))",
            "(dotimes (i 3) (return-from nil 1))",
            "(do () (t) (return-from nil 1))",
            "(loop do (return-from nil 1))",
            "(prog () (return-from nil 1))",
        ] {
            assert!(names(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_return_from_a_named_loop() {
        assert!(names("(loop named outer do (return-from outer 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_return_from_a_setf_function() {
        assert!(names("(defun (setf value) (v o) (return-from value v))").is_empty());
    }

    #[test]
    fn does_not_flag_a_return_from_a_local_function_name() {
        assert!(names("(labels ((walk (n) (return-from walk n))) (walk 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_return_from_inside_a_lambda_under_its_own_defun() {
        assert!(names("(defun f (l) (mapcar (lambda (x) (return-from f x)) l))").is_empty());
    }

    /// The whole point of the `Unknown` stop: a project macro may expand to a
    /// `block`, and this file cannot see that it does.
    #[test]
    fn does_not_flag_a_return_from_under_an_unknown_macro() {
        assert!(names("(with-retry (return-from retry 1))").is_empty());
        assert!(names("(defun f () (with-open-thing (return-from g 1)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_return_from() {
        assert!(names("(return-from)").is_empty());
        assert!(names("(return-from a 1 2)").is_empty());
        assert!(names("(return-from (compute-name) 1)").is_empty());
        assert!(names("(return-from 'a 1)").is_empty());
    }

    #[test]
    fn case_folds_and_ignores_the_package_qualifier() {
        assert!(names("(defun f () (CL:RETURN-FROM F 1))").is_empty());
        assert_eq!(names("(defun f () (RETURN-FROM G 1))"), vec!["g"]);
    }

    // -- the five quote shapes ---------------------------------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert!(names("'(return-from nowhere 1)").is_empty());
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert!(names("(quote (return-from nowhere 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert!(names("'(a ,(return-from nowhere 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_macro_template() {
        assert!(names("(defmacro m () `(return-from nowhere 1))").is_empty());
    }

    /// An unquote escapes back to code, so this one *is* evaluated — and the
    /// `defmacro` block is named `m`, not `nowhere`.
    #[test]
    fn flags_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(
            names("(defmacro m () `(progn ,(return-from nowhere 1)))"),
            vec!["nowhere"]
        );
    }

    /// The same shape under an unknown head reports nothing, and for a reason
    /// that is *not* the quote state: `(a …)` may be a macro that establishes
    /// the block. Pinned beside the test above so a change to either the quote
    /// handling or the `Unknown` stop cannot be mistaken for the other.
    #[test]
    fn an_unquoted_form_under_an_unknown_head_is_still_unknown() {
        assert!(names("(defmacro m () `(a ,(return-from nowhere 1)))").is_empty());
    }

    // -- strings ------------------------------------------------------------

    #[test]
    fn does_not_flag_a_return_from_inside_a_string_literal() {
        assert!(names("(defun f () \"(return-from nowhere 1)\")").is_empty());
    }

    // -- report shape -------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(return-from g 1)", Dialect::Clojure).expect("parse");
        let report =
            build_return_from_unmatched_block_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("return_from_form_count", json!(0))]);
    }

    #[test]
    fn the_summary_counts_every_return_from_scanned_not_only_the_flagged_ones() {
        let report = report("(defun f () (return-from f 1) (return-from g 2))");
        assert_eq!(report.summary, vec![("return_from_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_block_name() {
        let report = report("(defun f ()\n  (return-from g 1))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "return-from-unmatched-block");
        assert_eq!(finding.json_fields(), vec![("block", json!("g"))]);
        assert_eq!(finding.text_columns(), vec!["block=g".to_owned()]);
        assert!(finding.message().contains("`g`"));
    }
}
