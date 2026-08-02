//! Emacs Lisp truncating-division detection: a `/` over integer literals whose
//! quotient collapses a non-zero numerator to `0`.
//!
//! # Why this rule is Emacs Lisp only
//!
//! In Common Lisp there is nothing here to find. CLHS says of `/`: "If each
//! argument is either an integer or a ratio, and the result is not an integer,
//! then it is a ratio." `(/ 1 3)` is the exact rational `1/3`, `(/ 5 2)` is
//! `5/2`, and no precision is lost at all — SBCL confirms both. A
//! "division-result-precision-loss" rule aimed at Common Lisp would report
//! correct code on every hit, because CL is not C.
//!
//! Emacs Lisp is the dialect where the defect is real. The GNU Emacs Lisp
//! Reference Manual, Arithmetic Operations: "If all the arguments are integers,
//! the result is an integer, obtained by rounding the quotient towards zero
//! after each division." Its own examples include `(/ 5 2)` ⇒ `2` and
//! `(/ -17 6)` ⇒ `-2`. Verified against Emacs 31.0.91: `(/ 1 3)` ⇒ `0`.
//!
//! Rounding happens **after each division**, not once at the end, which is why
//! this walks the operands as a left fold: `(/ 25 3 2)` ⇒ `4` in the manual's
//! own example, and `(/ 10 3 2)` ⇒ `1` rather than the `5/3` an exact evaluation
//! would give.
//!
//! # Limits, on purpose
//!
//! - **Only a quotient that collapses to zero.** `(/ 100 3)` ⇒ `33` is very
//!   plausibly deliberate integer division, and reporting it would be a
//!   nuisance on correct code. `(/ 1 3)` ⇒ `0` discards the value *entirely*,
//!   which almost nobody writes on purpose. This is a deliberate false-negative
//!   trade: the rule reports the unambiguous case and stays quiet on the
//!   arguable one.
//! - **Only integer literals.** `(/ x 3)` may be dividing a float and nothing at
//!   this layer can tell; a single float operand anywhere makes the whole
//!   expression float division (`(/ 5 2.0)` ⇒ `2.5`) and is correct.
//! - **Two or more operands.** Single-argument `/` is deliberately skipped: its
//!   meaning *changed* in Emacs 25.1, where `etc/NEWS.25` records "'(/ N)' is
//!   now equivalent to '(/ 1 N)' rather than to '(/ N 1)'". A rule that reported
//!   it would be reporting a different defect on either side of that boundary.
//! - **A zero divisor is skipped**, being a different defect (and an error at
//!   run time) rather than a precision one.
//!
//! Report-only: whether the author wanted `(/ 1.0 3)` or `(float (/ 1 3))` or
//! genuinely wanted `0` is not something a rewrite can decide.
//!
//! Scope: Emacs Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

use crate::support::integer_literal_value;

#[derive(Debug, Clone)]
pub struct DivisionPrecisionLossItem {
    /// The span of the whole `(/ …)` form.
    pub span: ByteSpan,
    /// The numerator whose value the division discards, as written.
    pub numerator: String,
    /// The divisor that discards it, as written.
    pub divisor: String,
}

impl Finding for DivisionPrecisionLossItem {
    /// One shape, one kind. Every finding here is the same defect: an integer
    /// quotient truncated to zero.
    fn kind(&self) -> &'static str {
        "truncated-to-zero"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("numerator={}", self.numerator),
            format!("divisor={}", self.divisor),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("numerator", json!(self.numerator)),
            ("divisor", json!(self.divisor)),
        ]
    }

    /// The same sentence the `division-result-precision-loss` lint rule writes,
    /// so a SARIF or JUnit consumer reading both sees one finding described one
    /// way.
    fn message(&self) -> String {
        format!(
            "integer division of {} by {} truncates towards zero and yields 0, discarding the \
             value entirely; make an operand a float to get a fractional result",
            self.numerator, self.divisor
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Work is bounded to the matched form's own direct operands.
pub fn examine(
    view: &ExpressionView,
    division_form_count: &mut usize,
    violations: &mut Vec<DivisionPrecisionLossItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if head != "/" {
        return;
    }
    *division_form_count += 1;

    // `(/ n)` changed meaning in Emacs 25.1; it is deliberately out of scope.
    //
    // The fold below would reach no divisor for a one-operand call and so would
    // report nothing anyway — mutation-verified — but the version boundary is a
    // reason to skip it, not a coincidence to rely on.
    if view.children.len() < 3 {
        return;
    }

    // The left fold the manual describes: round towards zero after each
    // division. Every operand must be a plain integer literal, or nothing can
    // be computed and the form is skipped.
    let mut operands = view.children.iter().skip(1);
    let Some(accumulator) = operands.next().and_then(literal_operand) else {
        return;
    };
    let (mut value, mut written) = accumulator;

    for operand in operands {
        let Some((divisor, divisor_text)) = literal_operand(operand) else {
            return;
        };
        // A zero divisor is a different defect, and an error at run time.
        //
        // Stated rather than left to `checked_div`, which also returns `None`
        // here — mutation-verified, so removing this line changes no verdict.
        // The explicit test is what makes "division by zero is out of scope"
        // readable instead of an accident of the arithmetic below.
        if divisor == 0 {
            return;
        }
        let Some(quotient) = value.checked_div(divisor) else {
            return;
        };
        if value != 0 && quotient == 0 {
            violations.push(DivisionPrecisionLossItem {
                span: view.span,
                numerator: written.to_owned(),
                divisor: divisor_text.to_owned(),
            });
            return;
        }
        value = quotient;
        written = divisor_text;
    }
}

/// An operand's integer value and its source text, or `None` when it is not a
/// plain integer literal this can compute with.
fn literal_operand(view: &ExpressionView) -> Option<(i64, &str)> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    integer_literal_value(text).map(|value| (value, text))
}

/// Collects every value-discarding integer division in one file, with the
/// number of `/` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no truncating division here" for Emacs
/// Lisp and "nothing was looked for" for Common Lisp — where `(/ 1 3)` is the
/// exact ratio `1/3` and there is nothing to find at all.
pub fn build_division_result_precision_loss_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DivisionPrecisionLossItem>> {
    if dialect != Dialect::EmacsLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("division_form_count", json!(0))],
        ));
    }

    let mut division_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut division_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("division_form_count", json!(division_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DivisionPrecisionLossItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::EmacsLisp).expect("parse input");
        build_division_result_precision_loss_report(Path::new("test.el"), Dialect::EmacsLisp, &tree)
            .expect("build report")
    }

    /// The `(division_form_count, violations)` pair the report is built from.
    fn divisions(input: &str) -> (u64, Vec<DivisionPrecisionLossItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "division_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("division_form_count in the summary");
        (count, report.findings)
    }

    // -- positive ------------------------------------------------------------

    /// Emacs 31.0.91: `(/ 1 3)` => 0.
    #[test]
    fn flags_a_quotient_that_collapses_to_zero() {
        let (count, violations) = divisions("(/ 1 3)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].numerator, "1");
        assert_eq!(violations[0].divisor, "3");
    }

    #[test]
    fn flags_other_collapsing_quotients() {
        for source in ["(/ 2 5)", "(/ 3 10)", "(/ -1 3)", "(/ 1 100)"] {
            assert_eq!(divisions(source).1.len(), 1, "{source}");
        }
    }

    /// The manual's "rounding towards zero after each division" is a left fold,
    /// so a later step can collapse a value an earlier one left intact.
    #[test]
    fn flags_a_collapse_at_a_later_fold_step() {
        // (/ 10 4 5): 10/4 => 2, then 2/5 => 0.
        let (_, violations) = divisions("(/ 10 4 5)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].divisor, "5");
    }

    #[test]
    fn finds_a_division_nested_in_a_definition() {
        assert_eq!(divisions("(defun ratio () (* 100 (/ 1 3)))").1.len(), 1);
    }

    // -- near-miss negatives -------------------------------------------------

    /// Emacs: `(/ 6 3)` => 2 exactly. Nothing is lost.
    #[test]
    fn does_not_flag_an_exact_division() {
        for source in ["(/ 6 3)", "(/ 10 2)", "(/ 100 4)", "(/ 0 3)"] {
            assert!(divisions(source).1.is_empty(), "{source}");
        }
    }

    /// The deliberate false negative: `(/ 100 3)` => 33 is plausibly intended
    /// integer division, so it is left alone even though it truncates.
    #[test]
    fn does_not_flag_a_truncation_that_keeps_a_non_zero_quotient() {
        for source in ["(/ 100 3)", "(/ 5 2)", "(/ -17 6)", "(/ 25 3 2)"] {
            assert!(divisions(source).1.is_empty(), "{source}");
        }
    }

    /// Emacs: `(/ 5 2.0)` => 2.5. A float operand makes it float division.
    #[test]
    fn does_not_flag_a_division_with_a_float_operand() {
        for source in ["(/ 1 3.0)", "(/ 1.0 3)", "(/ 1.0 3.0)"] {
            assert!(divisions(source).1.is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_division_with_a_non_literal_operand() {
        for source in ["(/ x 3)", "(/ 1 n)", "(/ (f) 3)", "(/ 1 (g))"] {
            assert!(divisions(source).1.is_empty(), "{source}");
        }
    }

    /// Its meaning changed in Emacs 25.1, so it is out of scope on purpose.
    #[test]
    fn does_not_flag_a_single_argument_division() {
        let (count, violations) = divisions("(/ 3)");
        assert_eq!(count, 1, "the form is still counted");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_zero_divisor() {
        assert!(divisions("(/ 1 0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_division_head() {
        let (count, violations) = divisions("(* 1 3)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// A string is one atom, so its contents are never operands.
    #[test]
    fn does_not_flag_a_division_spelling_inside_a_string_literal() {
        assert!(divisions("(message \"(/ 1 3)\")").1.is_empty());
        assert!(divisions("(/ \"1\" \"3\")").1.is_empty());
    }

    /// Emacs Lisp has no `#+` reader conditional at all — the parser refuses
    /// `#` dispatch outright — so the prefix that reaches this guard in elisp is
    /// a quote. `'1` is a quoted datum, not an operand whose value is known.
    #[test]
    fn does_not_flag_a_prefixed_operand() {
        assert!(divisions("(/ '1 3)").1.is_empty());
        assert!(divisions("(/ 1 '3)").1.is_empty());
    }

    // -- report plumbing -----------------------------------------------------

    /// Common Lisp is exactly the dialect where this defect does not exist:
    /// `(/ 1 3)` is the ratio 1/3.
    #[test]
    fn common_lisp_is_reported_as_unmodelled_because_its_division_is_exact() {
        let tree = SyntaxTree::parse_with_dialect("(/ 1 3)", Dialect::CommonLisp).expect("parse");
        let report = build_division_result_precision_loss_report(
            Path::new("app.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("division_form_count", json!(0))]);
    }

    #[test]
    fn an_emacs_lisp_file_is_reported_as_modelled() {
        assert!(report("(/ 6 3)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_operands() {
        let report = report("(defun third ()\n  (/ 1 3))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "truncated-to-zero");
        assert_eq!(
            finding.json_fields(),
            vec![("numerator", json!("1")), ("divisor", json!("3"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["numerator=1".to_owned(), "divisor=3".to_owned()]
        );
        assert!(finding.message().contains("truncates towards zero"));
    }

    #[test]
    fn the_summary_counts_every_division_form_scanned_not_only_the_flagged_ones() {
        let report = report("(/ 1 3)\n(/ 6 3)\n(/ x 2)\n");
        assert_eq!(report.summary, vec![("division_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
