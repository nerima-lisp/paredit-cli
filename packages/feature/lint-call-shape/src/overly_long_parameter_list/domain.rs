//! A definition whose *required* parameter list is longer than a threshold.
//!
//! A call to a function with many required parameters is unreadable at the call
//! site: `(render buffer 10 20 nil t 3 :fast)` says nothing about which number
//! is which, and the reader has to go and find the lambda list. Past some
//! number of them the usual remedies — `&key` parameters, a struct or a plist
//! for the ones that travel together, or splitting the function — are almost
//! always worth their cost.
//!
//! # What counts
//!
//! Only the *required* parameters: everything before the first lambda-list
//! keyword. `&optional`, `&rest`, `&key`, `&aux` and `&body` parameters are
//! deliberately not counted, because they are the very thing this rule suggests
//! using. A `defun` with three required parameters and nine `&key` ones is
//! exactly the shape a long positional list should become, and reporting it
//! would be reporting the fix.
//!
//! A `defmethod` specializer (`(x integer)`) and a `defmacro` destructuring
//! pattern (`(a b)`) each count as one, which is what they are to a caller.
//!
//! # What this rule does not attempt
//!
//! - It says nothing about *which* parameters to remove, and nothing about
//!   their types, names or order. There is no single correct rewrite, which is
//!   why it is report-only.
//! - It does not follow `&rest` into a function that redistributes it.
//! - The default threshold is deliberately generous. Six required parameters
//!   is a perfectly ordinary Common Lisp signature — the CLHS itself has
//!   plenty — so the default fires at eight, not at four. Lower it with
//!   `--rule-arg overly-long-parameter-list.max-required=N` on a codebase that
//!   has decided otherwise.
//! - Scope is Common Lisp only: the lambda-list keyword vocabulary this counts
//!   *against* (`&key`, `&aux`, `&allow-other-keys`) is the CLHS one, and a
//!   dialect with a different one would have its parameters counted wrongly.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in, unqualified};
use serde_json::{Value, json};

use crate::support::{
    definition_lambda_list, is_unevaluated_at, normalized_symbol, required_parameters,
};

/// How many required parameters a definition may carry by default.
///
/// Eight, not four. Six required parameters is an ordinary Common Lisp
/// signature, and the negative case a threshold has to survive is a correct
/// six-parameter `defun` — a rule that fires on one is worse than no rule.
pub const DEFAULT_MAX_REQUIRED: usize = 7;

/// The definition heads this rule reads. `defgeneric` is included because its
/// lambda list is the contract every method must match, so a long one is the
/// most expensive kind.
pub const DEFINITION_HEADS: &[&str] = &["defun", "defmacro", "defmethod", "defgeneric"];

/// One over-long lambda list.
#[derive(Debug, Clone)]
pub struct LongParameterListItem {
    /// The span of the lambda list itself, which is the thing to change.
    pub span: ByteSpan,
    /// The definition's head, for the message.
    pub form: String,
    /// The definition's name, or `""` when it is not a plain symbol.
    pub name: String,
    /// How many required parameters it declares.
    pub required_parameter_count: usize,
    /// The count this run allowed.
    pub threshold: usize,
}

impl Finding for LongParameterListItem {
    fn kind(&self) -> &'static str {
        "overly-long-parameter-list"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!(
            "required_parameter_count={}",
            self.required_parameter_count
        )]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("form", json!(self.form)),
            ("name", json!(self.name)),
            (
                "required_parameter_count",
                json!(self.required_parameter_count),
            ),
            ("threshold", json!(self.threshold)),
        ]
    }

    fn message(&self) -> String {
        message(
            &self.form,
            &self.name,
            self.required_parameter_count,
            self.threshold,
        )
    }
}

/// The one sentence both the report and the lint rule print.
#[must_use]
pub fn message(form: &str, name: &str, count: usize, threshold: usize) -> String {
    let subject = if name.is_empty() {
        form.to_owned()
    } else {
        format!("{form} {name}")
    };
    format!(
        "{subject} takes {count} required parameters, more than the {threshold} allowed; \
         `&key` parameters, or a structure for the ones that travel together, name them at the \
         call site"
    )
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// definition through the single dispatch pass instead of walking the tree
/// again.
pub fn examine_definition(
    tree: &SyntaxTree,
    view: &ExpressionView,
    max_required: usize,
    definition_count: &mut usize,
    violations: &mut Vec<LongParameterListItem>,
) {
    if !is_paren_list(view) {
        return;
    }
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, DEFINITION_HEADS) {
        return;
    }
    *definition_count += 1;

    let Some(lambda_list) = definition_lambda_list(view, head) else {
        return;
    };
    let required_parameter_count = required_parameters(lambda_list).len();
    if required_parameter_count <= max_required {
        return;
    }
    // Last, and only for a form that is otherwise reportable: the descent is
    // the one part of this rule that is not proportional to the matched node.
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    violations.push(LongParameterListItem {
        span: lambda_list.span,
        form: unqualified(head).to_ascii_lowercase(),
        name: view
            .children
            .get(1)
            .and_then(normalized_symbol)
            .unwrap_or_default(),
        required_parameter_count,
        threshold: max_required,
    });
}

/// Collects every over-long lambda list in one file, with the number of
/// definitions scanned as the denominator beside them.
pub fn build_overly_long_parameter_list_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LongParameterListItem>> {
    build_report_with_threshold(path, dialect, tree, DEFAULT_MAX_REQUIRED)
}

/// [`build_overly_long_parameter_list_report`] at a caller-chosen threshold.
pub fn build_report_with_threshold(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    max_required: usize,
) -> LintResult<FileFindings<LongParameterListItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("definition_count", json!(0))],
        ));
    }

    let mut definition_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        paredit_core_syntax::view_query::for_each_subview(&view, |subview| {
            examine_definition(
                tree,
                subview,
                max_required,
                &mut definition_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("definition_count", json!(definition_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<LongParameterListItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_overly_long_parameter_list_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build report")
    }

    fn counts(input: &str) -> Vec<usize> {
        report(input)
            .findings
            .iter()
            .map(|item| item.required_parameter_count)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_defun_with_eight_required_parameters() {
        assert_eq!(counts("(defun render (a b c d e f g h) nil)"), [8]);
    }

    #[test]
    fn flags_a_defmacro_a_defmethod_and_a_defgeneric() {
        assert_eq!(
            counts("(defmacro m (a b c d e f g h) nil)"),
            [8],
            "defmacro"
        );
        assert_eq!(
            counts("(defmethod m ((a t) (b t) c d e f g h) nil)"),
            [8],
            "defmethod"
        );
        assert_eq!(
            counts("(defgeneric g (a b c d e f g h))"),
            [8],
            "defgeneric"
        );
    }

    #[test]
    fn a_qualified_defmethod_is_read_past_its_qualifier() {
        assert_eq!(
            counts("(defmethod m :before ((a t) (b t) c d e f g h) nil)"),
            [8]
        );
    }

    #[test]
    fn the_reported_span_is_the_lambda_list() {
        let source = "(defun render (a b c d e f g h) nil)";
        let items = report(source).findings;
        assert_eq!(items[0].span.slice(source), "(a b c d e f g h)");
    }

    #[test]
    fn a_setf_function_name_does_not_confuse_the_lambda_list_position() {
        assert_eq!(counts("(defun (setf cell) (v a b c d e f g h) nil)"), [9]);
    }

    // -- near-miss negatives -------------------------------------------------

    /// The case a threshold has to survive: a genuinely fine six-parameter
    /// definition.
    #[test]
    fn a_six_parameter_definition_is_not_reported() {
        assert!(
            report("(defun draw-rectangle (x y width height color filled) nil)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn exactly_the_threshold_is_not_reported() {
        assert!(report("(defun f (a b c d e f g) nil)").findings.is_empty());
    }

    /// The shape this rule *suggests*, so reporting it would report the fix.
    #[test]
    fn keyword_and_optional_parameters_are_not_counted() {
        assert!(
            report(
                "(defun connect (host port &key timeout retries tls user password proxy keepalive) nil)"
            )
            .findings
            .is_empty()
        );
        assert!(
            report("(defun f (a &optional b c d e f g h i j) nil)")
                .findings
                .is_empty()
        );
        assert!(
            report("(defun f (a &rest more) nil)").findings.is_empty(),
            "&rest is not a parameter count"
        );
        assert!(
            report("(defmacro m (name &body body) nil)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn a_definition_with_no_parameters_is_not_reported() {
        assert!(report("(defun f () nil)").findings.is_empty());
    }

    #[test]
    fn a_malformed_definition_with_no_lambda_list_is_left_alone() {
        assert!(report("(defun f)").findings.is_empty());
    }

    #[test]
    fn a_head_that_is_not_a_definition_is_not_scanned() {
        let scanned = report("(list a b c d e f g h i)");
        assert!(scanned.findings.is_empty());
        assert_eq!(scanned.summary, vec![("definition_count", json!(0))]);
    }

    /// A realistic, correct file.
    #[test]
    fn idiomatic_code_is_silent() {
        let source = "(defpackage :app (:use :cl))\n(in-package :app)\n\n\
             (defun make-window (title width height &key resizable fullscreen decorated)\n  (declare (ignore resizable fullscreen decorated))\n  (list title width height))\n\n\
             (defmethod draw ((w window) (canvas canvas) x y)\n  (values x y))\n\n\
             (defmacro with-window ((var title) &body body)\n  `(let ((,var (make-window ,title 800 600))) ,@body))\n\n\
             (defgeneric resize (object width height))\n";
        assert!(report(source).findings.is_empty());
    }

    // -- the five quote shapes ----------------------------------------------

    #[test]
    fn a_hard_quoted_definition_is_data() {
        assert!(
            report("'(defun f (a b c d e f g h) nil)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn a_long_hand_quote_form_is_data() {
        assert!(
            report("(quote (defun f (a b c d e f g h) nil))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn a_quasiquoted_definition_without_an_unquote_is_data() {
        assert!(
            report("`(defun f (a b c d e f g h) nil)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn a_comma_inside_a_hard_quote_stays_data() {
        assert!(
            report("'(x ,(defun f (a b c d e f g h) nil))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn an_unquoted_definition_inside_a_quasiquote_is_code_again() {
        assert_eq!(counts("`(x ,(defun f (a b c d e f g h) nil))"), [8]);
    }

    #[test]
    fn a_definition_spelled_only_inside_a_string_is_never_a_form() {
        assert!(
            report("(format nil \"(defun f (a b c d e f g h) nil)\")")
                .findings
                .is_empty()
        );
    }

    // -- thresholds, dialects, denominators ----------------------------------

    #[test]
    fn the_threshold_moves_what_is_reported() {
        let tree = SyntaxTree::parse_with_dialect("(defun f (a b c d e) nil)", Dialect::CommonLisp)
            .expect("parse");
        let strict =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 4)
                .expect("report");
        assert_eq!(strict.findings.len(), 1);
        let lenient =
            build_report_with_threshold(Path::new("t.lisp"), Dialect::CommonLisp, &tree, 5)
                .expect("report");
        assert!(lenient.findings.is_empty());
    }

    #[test]
    fn a_dialect_this_rule_does_not_model_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defun f (a b c d e f g h) nil)", Dialect::EmacsLisp)
                .expect("parse");
        let report =
            build_overly_long_parameter_list_report(Path::new("t.el"), Dialect::EmacsLisp, &tree)
                .expect("report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_definition_scanned() {
        let report = report("(defun a (x) 1)\n(defun b (a b c d e f g h) 2)\n(defmacro c () 3)\n");
        assert_eq!(report.summary, vec![("definition_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_name_and_its_count() {
        let report = report("(in-package :app)\n(defun render (a b c d e f g h) nil)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "overly-long-parameter-list");
        assert_eq!(finding.name, "render");
        assert_eq!(finding.form, "defun");
        assert_eq!(
            finding.text_columns(),
            vec!["required_parameter_count=8".to_owned()]
        );
        assert!(finding.message().contains("defun render takes 8"));
    }
}
