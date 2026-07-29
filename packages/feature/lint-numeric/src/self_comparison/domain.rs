//! Common Lisp self-comparison detection: a comparison call whose two
//! operands are structurally identical — `(eq x x)`, `(< a a)`,
//! `(equal (f y) (f y))`. An equality predicate applied to a value and
//! itself is trivially true, and an ordering predicate is trivially false,
//! so the test is dead: almost always one operand was meant to differ. This
//! is the Common Lisp analog of the `eq_op` lint.
//!
//! Covered operators are the equality predicates (`eq`, `eql`, `equal`,
//! `equalp`, `string=`, `char=`) and the strict/loose ordering predicates
//! (`<`, `>`, `<=`, `>=`, `string<`, `string>`, `char<`, `char>`). The
//! numeric `=` and `/=` are deliberately excluded: `(/= x x)` is a real (if
//! rare) idiom for detecting a floating-point NaN, where a value genuinely
//! is not `=` to itself, and flagging it would be a false positive.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`] and the reader-aware
//! structural comparison from [`paredit_core_syntax::expression_equality`], so
//! `(eq x X)` counts (symbols fold case) while `(eq x y)` does not.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

const COMPARISON_HEADS: [&str; 14] = [
    "eq", "eql", "equal", "equalp", "string=", "char=", "<", ">", "<=", ">=", "string<", "string>",
    "char<", "char>",
];

#[derive(Debug, Clone)]
pub struct SelfComparisonItem {
    pub span: ByteSpan,
    /// The 1-based line the comparison starts on.
    pub line: usize,
    pub operator: String,
    pub operand: String,
}

impl Finding for SelfComparisonItem {
    /// The rule's own name rather than the operator.
    ///
    /// Half of the fourteen heads this rule matches are punctuation (`<`, `>=`,
    /// `char<`), which a rule id or a `grep` pattern reads badly, and the
    /// operator is not a closed set of `&'static str` here anyway — it is the
    /// source spelling. It stays a field instead.
    fn kind(&self) -> &'static str {
        "self-comparison"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("op={}", self.operator),
            format!("operand={}", self.operand),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("operand", json!(self.operand)),
        ]
    }

    /// The same sentence the `self-comparison` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} compares operand {} with itself",
            self.operator, self.operand
        )
    }
}

pub fn examine_comparison(
    view: &ExpressionView,
    source: &str,
    comparison_form_count: &mut usize,
    violations: &mut Vec<SelfComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !COMPARISON_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *comparison_form_count += 1;

    // Report the first pair of structurally-equal operands (after the
    // operator); a call is one violation, not one per repeated operand.
    let operands: Vec<&ExpressionView> = view.children.iter().skip(1).collect();
    for anchor in 0..operands.len() {
        for candidate in (anchor + 1)..operands.len() {
            if expressions_structurally_equal(operands[anchor], operands[candidate]) {
                violations.push(SelfComparisonItem {
                    span: view.span,
                    line: line_of(source, view.span.start().get()),
                    operator: head.to_owned(),
                    operand: render_expression(operands[anchor]),
                });
                return;
            }
        }
    }
}

/// Collects every comparison call with two structurally-equal operands in one
/// file, with the number of comparison calls scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no comparison repeats an operand" for
/// Common Lisp and "nothing was looked for" for Fennel, and the two read
/// identically without the flag.
pub fn build_self_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SelfComparisonItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("comparison_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, source, &mut comparison_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("comparison_form_count", json!(comparison_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SelfComparisonItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_self_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build self comparison report")
    }

    /// The `(comparison_form_count, violations)` pair the report is built from.
    fn comparisons(input: &str) -> (u64, Vec<SelfComparisonItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "comparison_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("comparison_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_eq_of_a_variable_with_itself() {
        let (comparison_form_count, violations) = comparisons("(eq x x)");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
        assert_eq!(violations[0].operand, "x");
    }

    #[test]
    fn flags_an_ordering_predicate_of_identical_operands() {
        let (_, violations) = comparisons("(< (score a) (score a))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operand, "(score a)");
    }

    #[test]
    fn folds_symbol_case_between_operands() {
        let (_, violations) = comparisons("(equal foo FOO)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_operands() {
        let (comparison_form_count, violations) = comparisons("(eq x y)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_numeric_equality_used_as_a_nan_check() {
        // `(/= x x)` and `(= x x)` are excluded because of the NaN idiom.
        let (comparison_form_count, violations) = comparisons("(and (= x x) (/= y y))");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) = comparisons("(defun f (x) (when (eql x x) 1))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(eq x x)").expect("parse input");
        let report = build_self_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build self comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eq x y)").dialect_modelled);
    }

    /// The kind is the rule's name, not the operator: half the heads this rule
    /// matches are punctuation. The operator stays a field.
    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_operand() {
        let report = report("(defun f (x)\n  (when (eql x x) 1))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "self-comparison");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("eql")), ("operand", json!("x"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op=eql".to_owned(), "operand=x".to_owned()]
        );
    }

    #[test]
    fn a_punctuation_operator_still_reports_the_rule_name_as_its_kind() {
        let report = report("(< (rank a) (rank a))");
        let finding = &report.findings[0];
        assert_eq!(finding.kind(), "self-comparison");
        assert_eq!(finding.operator, "<");
    }

    #[test]
    fn the_summary_counts_every_comparison_scanned_not_only_the_flagged_ones() {
        let report = report("(eq x x)\n(eq a b)\n(eql y y)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
