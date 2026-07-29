//! Common Lisp negated-comparison detection: a `not`/`null` wrapping a
//! two-argument numeric comparison, which has an exact complementary operator —
//! `(not (= a b))` is `(/= a b)`, `(not (< a b))` is `(>= a b)`, `(null (> a b))`
//! is `(<= a b)`. Over Common Lisp's total order on reals every comparison has a
//! complement, so the negation collapses to the opposite operator, evaluating
//! each operand exactly once and stating the intent directly.
//!
//! Complement table (for exactly two operands):
//!
//!   - `=`  ↔ `/=`
//!   - `<`  ↔ `>=`
//!   - `>`  ↔ `<=`
//!
//! Only the two-operand shape is flagged. `(not (= a b c))` is *not* `(/= a b c)`
//! — a three-argument `=` tests all-equal while `/=` tests pairwise-distinct — so
//! a comparison with any operand count other than two is left alone, as is a
//! reader-conditional operand (build-dependent arity).
//!
//! The fix rewrites the whole `(not (OP a b))` form as `(COMPLEMENT a b)`,
//! copying the operands' source verbatim, so the rule is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The complementary operator for a two-argument numeric comparison, or `None`
/// for a non-comparison head.
fn complement_operator(op: &str) -> Option<&'static str> {
    match op {
        "=" => Some("/="),
        "/=" => Some("="),
        "<" => Some(">="),
        ">" => Some("<="),
        "<=" => Some(">"),
        ">=" => Some("<"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// comparison containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NegatedComparisonItem {
    /// The span of the whole `(not (OP a b))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The complementary operator to substitute (`/=`, `>=`, …).
    pub complement: &'static str,
    /// The span covering the two operands (`a b`), reused verbatim in the fix.
    ///
    /// The rewrite's input, not the report's: the lint rule slices it to build
    /// the replacement, and the command has never printed it.
    pub operands_span: ByteSpan,
}

impl Finding for NegatedComparisonItem {
    /// The rule's own name, not the complement.
    ///
    /// A `kind` leads every text row and names the SARIF rule; `/=` and `>=`
    /// are punctuation that would make both unreadable and, in the CSV and TSV
    /// outputs, ambiguous. The complement stays a field on the finding.
    fn kind(&self) -> &'static str {
        "negated-comparison"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("complement={}", self.complement)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("complement", json!(self.complement))]
    }

    /// The same sentence the `negated-comparison` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "negated comparison has a complement operator; use {}",
            self.complement
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_negation(
    view: &ExpressionView,
    source: &str,
    negation_form_count: &mut usize,
    violations: &mut Vec<NegatedComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("not") && !head.eq_ignore_ascii_case("null") {
        return;
    }
    // A negation is (not X) / (null X): exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    *negation_form_count += 1;

    let inner = &view.children[1];
    if !is_paren_list(inner) {
        return;
    }
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    let Some(complement) = complement_operator(inner_head) else {
        return;
    };
    // The comparison must have exactly two operands for the complement to hold.
    if inner.children.len() != 3 {
        return;
    }
    if is_reader_conditional(&inner.children[1]) || is_reader_conditional(&inner.children[2]) {
        return;
    }

    let operands_span = ByteSpan::new(inner.children[1].span.start(), inner.children[2].span.end());
    violations.push(NegatedComparisonItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        complement,
        operands_span,
    });
}

/// Collects every negated two-argument comparison in one file, with the number
/// of `not`/`null` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no negated comparison here" for Common
/// Lisp and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_negated_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NegatedComparisonItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("negation_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut negation_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_negation(subview, source, &mut negation_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("negation_form_count", json!(negation_form_count))],
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

    fn report(input: &str) -> FileFindings<NegatedComparisonItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_negated_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build negated comparison report")
    }

    /// The `(negation_form_count, violations)` pair the report is built from.
    fn negations(input: &str) -> (u64, Vec<NegatedComparisonItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "negation_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("negation_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn not_equal_becomes_slash_equal() {
        let source = "(not (= a b))";
        let (count, violations) = negations(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].complement, "/=");
        assert_eq!(slice(source, violations[0].operands_span), "a b");
    }

    #[test]
    fn not_less_becomes_ge() {
        let (_, violations) = negations("(not (< x y))");
        assert_eq!(violations[0].complement, ">=");
    }

    #[test]
    fn not_greater_becomes_le() {
        let (_, violations) = negations("(not (> x y))");
        assert_eq!(violations[0].complement, "<=");
    }

    #[test]
    fn not_le_becomes_greater() {
        let (_, violations) = negations("(not (<= x y))");
        assert_eq!(violations[0].complement, ">");
    }

    #[test]
    fn not_ne_becomes_equal() {
        let (_, violations) = negations("(not (/= x y))");
        assert_eq!(violations[0].complement, "=");
    }

    #[test]
    fn null_head_is_also_a_negation() {
        let (_, violations) = negations("(null (>= p q))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].complement, "<");
    }

    #[test]
    fn preserves_compound_operand_source() {
        let source = "(not (= (length xs) 0))";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operands_span), "(length xs) 0");
    }

    #[test]
    fn does_not_flag_three_operand_comparison() {
        // (not (= a b c)) is not (/= a b c).
        let (_, violations) = negations("(not (= a b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_comparison_inner() {
        let (_, violations) = negations("(not (evenp x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_not_of_symbol() {
        let (count, violations) = negations("(not flag)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_negated_comparison() {
        let (_, violations) = negations("(when (not (= a b)) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].complement, "/=");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(not (= a b))", Dialect::Clojure).expect("parse");
        let report = build_negated_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build negated comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("negation_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(not flag)").dialect_modelled);
    }

    /// The complement is a `json_fields` entry rather than the `kind`, so no
    /// punctuation operator ever reaches a text row's leading column.
    #[test]
    fn a_finding_carries_its_line_and_its_complement() {
        let report = report("(defun distinct? (a b)\n  (not (= a b)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "negated-comparison");
        assert_eq!(finding.json_fields(), vec![("complement", json!("/="))]);
        assert_eq!(finding.text_columns(), vec!["complement=/=".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_negation_scanned_not_only_the_flagged_ones() {
        let report = report("(not (= a b))\n(not flag)\n");
        assert_eq!(report.summary, vec![("negation_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
