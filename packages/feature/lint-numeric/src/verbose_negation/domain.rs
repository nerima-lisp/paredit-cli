//! Common Lisp verbose-negation detection: negation written the long way, which
//! the unary `-` expresses directly — `(- 0 x)` (zero minus x), `(* x -1)` and
//! `(* -1 x)` (times minus one) are all exactly `(- x)`. The unary form states
//! "negate this" without a spurious constant operand.
//!
//! Each shape is an exact rewrite that evaluates `x` once:
//!
//!   - `(- 0 X)` → `(- X)`   (0 − X = −X; only the *leading* zero, so this does
//!     not overlap `identity-arithmetic`'s trailing `(- x 0)`).
//!   - `(* X -1)` / `(* -1 X)` → `(- X)`   (`*` commutes).
//!
//! Only the bare *integer* literals `0` and `-1` count. A float spelling like
//! `-1.0` would coerce the result type (`(* 2 -1.0)` is a float while `(- 2)` is
//! an integer), so it is left alone, as is a reader-conditional operand.
//!
//! The fix rewrites the form as `(- X)`, copying `X` from source, so the rule is
//! auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// Whether `view` is the bare integer literal `literal` (no reader prefixes).
fn is_int_literal(view: &ExpressionView, literal: &str) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some(literal)
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct VerboseNegationItem {
    /// The span of the whole `(- 0 x)`/`(* x -1)` form.
    pub span: ByteSpan,
    /// The span of the operand `X` that is being negated.
    ///
    pub operand_span: ByteSpan,
}

impl Finding for VerboseNegationItem {
    /// The rule's own name.
    ///
    /// This report has nothing else to offer: the three shapes it matches
    /// (`(- 0 X)`, `(* X -1)`, `(* -1 X)`) are one smell with one rewrite, and
    /// the head that produced a finding is punctuation the item never retained.
    fn kind(&self) -> &'static str {
        "verbose-negation"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the leading `kind`: the old text row carried only the
    /// path and the offset, both of which the envelope prints itself.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// Nothing beyond the envelope's own `kind`/`line`/`span`, which is exactly
    /// what the old JSON published.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn message(&self) -> String {
        "negation written the long way; use (- x)".to_owned()
    }
}

pub fn examine_form(
    view: &ExpressionView,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<VerboseNegationItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !matches!(head, "-" | "*") {
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

    // `(- 0 X)` negates X; only the leading zero (trailing is identity-arithmetic).
    // `(* X -1)` / `(* -1 X)` negate the other operand.
    let operand = if head == "-" {
        if is_int_literal(left, "0") {
            right
        } else {
            return;
        }
    } else if is_int_literal(left, "-1") {
        right
    } else if is_int_literal(right, "-1") {
        left
    } else {
        return;
    };

    violations.push(VerboseNegationItem {
        span: view.span,
        operand_span: operand.span,
    });
}

/// Collects every verbose negation in one file, with the number of `-`/`*`
/// forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_verbose_negation_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<VerboseNegationItem>> {
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

    fn report(input: &str) -> FileFindings<VerboseNegationItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_verbose_negation_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build verbose negation report")
    }

    fn negations(input: &str) -> (u64, Vec<VerboseNegationItem>) {
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
    fn zero_minus_x_negates() {
        let source = "(- 0 count)";
        let (count, violations) = negations(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].operand_span), "count");
    }

    #[test]
    fn times_minus_one_negates() {
        let source = "(* delta -1)";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operand_span), "delta");
    }

    #[test]
    fn minus_one_times_negates_commuted() {
        let source = "(* -1 delta)";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operand_span), "delta");
    }

    #[test]
    fn preserves_compound_operand_source() {
        let source = "(- 0 (compute x))";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operand_span), "(compute x)");
    }

    #[test]
    fn does_not_flag_trailing_zero() {
        // (- x 0) is identity-arithmetic's job, not negation.
        let (_, violations) = negations("(- x 0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_minus_one() {
        // (* x -1.0) would coerce the result type.
        let (_, violations) = negations("(* x -1.0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_constants() {
        assert!(negations("(- 5 x)").1.is_empty());
        assert!(negations("(* x -2)").1.is_empty());
    }

    #[test]
    fn does_not_flag_three_operands() {
        assert!(negations("(- 0 x y)").1.is_empty());
        assert!(negations("(* -1 x y)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_negation() {
        let (_, violations) = negations("(setf d (- 0 delta))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(- 0 x)", Dialect::Clojure).expect("parse");
        let report = build_verbose_negation_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build verbose negation report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(- x 1)").dialect_modelled);
    }

    /// This finding has no fields of its own, so `message` is the only prose an
    /// interop consumer gets.
    #[test]
    fn a_finding_carries_its_line_and_leans_on_its_message() {
        let report = report("(defun negate (x)\n  (- 0 x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "verbose-negation");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.message(),
            "negation written the long way; use (- x)"
        );
    }

    #[test]
    fn the_summary_counts_every_arithmetic_form_scanned_not_only_the_flagged_ones() {
        let report = report("(- 0 x)\n(- x 0)\n(* a b)\n");
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
