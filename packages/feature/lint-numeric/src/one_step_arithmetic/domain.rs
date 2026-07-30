//! Common Lisp one-step-arithmetic detection: a two-argument `+`/`-` where one
//! operand is the literal `1`, which the language provides a unary shorthand
//! for. `(1+ n)` is defined as `(+ n 1)` and `(1- n)` as `(- n 1)`, so the
//! rewrite is exact (same value, same numeric-type contract) and the shorthand
//! states the unit step directly.
//!
//! The operator's algebra decides which shapes qualify. `+` is commutative, so
//! `(+ x 1)` and `(+ 1 x)` both become `(1+ x)`. `-` is not: only `(- x 1)`
//! becomes `(1- x)`, since `(- 1 x)` is `1 - x`, which has no unary shorthand.
//!
//! Only the bare integer literal `1` is matched. A float `1.0` is left alone:
//! `(+ x 1.0)` can coerce `x` to a float, so it is not `(1+ x)`. A `#x1`/prefixed
//! spelling, a three-argument form, and a reader-conditional operand are all
//! left alone.
//!
//! The fix rewrites the form as `(1+ x)` / `(1- x)`, copying the non-`1` operand
//! from its exact source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// Whether `view` is the bare integer `1` literal (no reader prefixes, so `#x1`
/// and a prefixed `,1` are excluded; `1.0` is a different spelling, excluded).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("1")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct OneStepArithmeticItem {
    /// The span of the whole `(+ x 1)` / `(- x 1)` form.
    pub span: ByteSpan,
    /// The span of the non-`1` operand `x`.
    ///
    /// The rewrite's input, not the report's: the lint rule copies it into the
    /// shorthand form, and the command never prints it.
    pub operand_span: ByteSpan,
    /// The suggested shorthand operator (`1+`/`1-`).
    pub shorthand: &'static str,
}

impl Finding for OneStepArithmeticItem {
    /// The shorthand the form should have used. `1+` and `1-` are Common Lisp
    /// function names, not the bare `+`/`-` operators, so they read as
    /// identifiers in a rule id or a `grep` — and they separate an increment
    /// from a decrement, which is the distinction a consumer cares about.
    fn kind(&self) -> &'static str {
        self.shorthand
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the leading `kind`: the old text row carried the
    /// shorthand and no other column, and that shorthand is now the kind.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("shorthand", json!(self.shorthand))]
    }

    /// The same sentence the `one-step-arithmetic` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("add/subtract of 1 has a shorthand; use {}", self.shorthand)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_form(
    view: &ExpressionView,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<OneStepArithmeticItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let is_plus = head == "+";
    let is_minus = head == "-";
    if !is_plus && !is_minus {
        return;
    }
    *arithmetic_form_count += 1;

    // children[0] is the operator; require exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let left = &view.children[1];
    let right = &view.children[2];
    if is_reader_conditional(left) || is_reader_conditional(right) {
        return;
    }

    // `+` takes the `1` on either side; `-` only as the subtrahend `(- x 1)`.
    let (operand, shorthand) = if is_plus {
        if is_one_literal(right) {
            (left, "1+")
        } else if is_one_literal(left) {
            (right, "1+")
        } else {
            return;
        }
    } else if is_one_literal(right) {
        (left, "1-")
    } else {
        return;
    };

    violations.push(OneStepArithmeticItem {
        span: view.span,
        operand_span: operand.span,
        shorthand,
    });
}

/// Collects every two-argument `+`/`-` of a literal `1` in one file, with the
/// number of `+`/`-` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no unit step here" for Common Lisp and
/// "nothing was looked for" for Fennel, and the two read identically without
/// the flag.
pub fn build_one_step_arithmetic_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<OneStepArithmeticItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("arithmetic_form_count", json!(0))],
        ));
    }

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, &mut arithmetic_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("arithmetic_form_count", json!(arithmetic_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<OneStepArithmeticItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_one_step_arithmetic_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build one-step arithmetic report")
    }

    /// The `(arithmetic_form_count, violations)` pair the report is built from.
    fn forms(input: &str) -> (u64, Vec<OneStepArithmeticItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "arithmetic_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("arithmetic_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_plus_one_on_the_right() {
        let source = "(+ n 1)";
        let (count, violations) = forms(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].shorthand, "1+");
        assert_eq!(slice(source, violations[0].operand_span), "n");
    }

    #[test]
    fn flags_plus_one_on_the_left() {
        let source = "(+ 1 (length xs))";
        let (_, violations) = forms(source);
        assert_eq!(violations[0].shorthand, "1+");
        assert_eq!(slice(source, violations[0].operand_span), "(length xs)");
    }

    #[test]
    fn flags_minus_one_on_the_right() {
        let source = "(- count 1)";
        let (_, violations) = forms(source);
        assert_eq!(violations[0].shorthand, "1-");
        assert_eq!(slice(source, violations[0].operand_span), "count");
    }

    #[test]
    fn does_not_flag_one_minus_x() {
        // (- 1 x) is 1 - x; no unary shorthand.
        let (count, violations) = forms("(- 1 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_one() {
        assert!(forms("(+ x 1.0)").1.is_empty());
        assert!(forms("(- x 1.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_one_literal() {
        let (count, violations) = forms("(+ x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_operands() {
        let (_, violations) = forms("(+ x 1 y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = forms("(setf i (+ i 1))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(+ n 1)", Dialect::Clojure).expect("parse");
        let report =
            build_one_step_arithmetic_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build one-step arithmetic report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(+ n 2)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_shorthand() {
        let report = report("(defun f (i)\n  (setf i (+ i 1)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "1+");
        assert_eq!(finding.json_fields(), vec![("shorthand", json!("1+"))]);
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_arithmetic_form_scanned_not_only_the_flagged_ones() {
        let report = report("(+ n 1)\n(+ n 2)\n(- n 1)\n");
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
