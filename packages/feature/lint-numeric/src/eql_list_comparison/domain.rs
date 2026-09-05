//! Common Lisp `eq`/`eql`-on-a-quoted-list detection: a call to `eq` or
//! `eql` with a quoted, non-empty list literal argument — `(eq x '(1 2))`,
//! `(eql (path node) '(:a :b))`. `eq` and `eql` both test object identity for
//! conses, and two list literals are distinct objects (a fresh cons on each
//! read/evaluation), so such a comparison is essentially always false
//! regardless of the lists' contents — a classic bug where `equal` was meant.
//!
//! A quoted *symbol* (`'foo`) is fine: symbols are interned, so `(eq x 'foo)`
//! is a correct and common idiom, and is not flagged. The empty quoted list
//! `'()` is `nil` — also a symbol, also `eq`-comparable — and is likewise
//! left alone. Only a quoted, non-empty list triggers the report, whether
//! written with the reader prefix (`'(...)`) or the explicit `(quote (...))`
//! form.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`] and the display rendering
//! from [`paredit_core_syntax::expression_equality`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

const EQ_HEADS: [&str; 2] = ["eq", "eql"];

/// Whether `view` is a quoted, non-empty list — either `'(a b)` (a paren
/// list carrying a `Quote` reader prefix) or `(quote (a b))` (an explicit
/// quote of a non-empty list). An empty quoted list (`'()` / `(quote ())`)
/// is `nil` and is not a list literal for this purpose.
fn is_quoted_list_literal(view: &ExpressionView) -> bool {
    if is_paren_list(view)
        && view.reader_prefixes.contains(&ReaderPrefix::Quote)
        && !view.children.is_empty()
    {
        return true;
    }

    if list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote")) {
        if let Some(quoted) = view.children.get(1) {
            return is_paren_list(quoted) && !quoted.children.is_empty();
        }
    }

    false
}

#[derive(Debug, Clone)]
pub struct EqlListComparisonItem {
    pub span: ByteSpan,
    /// The operator exactly as it was written, so its source casing survives.
    pub operator: String,
    pub literal: String,
}

impl Finding for EqlListComparisonItem {
    /// Which of the two identity predicates was used, normalized to the
    /// spelling this rule matched on.
    ///
    /// They are two different mistakes to make — `eq` on a list is wrong for
    /// the same reason `eql` is, but a codebase mid-way through replacing one
    /// with the other cares which it is looking at — and a consumer filtering
    /// on one of them is asking a real question. `operator` keeps the source
    /// casing that this discards.
    fn kind(&self) -> &'static str {
        EQ_HEADS
            .iter()
            .find(|name| self.operator.eq_ignore_ascii_case(name))
            .copied()
            .unwrap_or("eql-list-comparison")
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("op={}", self.operator),
            format!("literal={}", self.literal),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("literal", json!(self.literal)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} compares against quoted list literal {}",
            self.operator, self.literal
        )
    }
}

pub fn examine_comparison(
    view: &ExpressionView,
    comparison_form_count: &mut usize,
    violations: &mut Vec<EqlListComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EQ_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *comparison_form_count += 1;

    // Report the first quoted-list argument (after the operator); a call with
    // two such literals is still one bug, not two.
    if let Some(literal) = view
        .children
        .iter()
        .skip(1)
        .find(|argument| is_quoted_list_literal(argument))
    {
        violations.push(EqlListComparisonItem {
            span: view.span,
            operator: head.to_owned(),
            literal: render_expression(literal),
        });
    }
}

/// Collects every `eq`/`eql` call with a quoted-list-literal argument in one
/// file, with the number of `eq`/`eql` calls scanned as the denominator beside
/// them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_eql_list_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EqlListComparisonItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("comparison_form_count", json!(0))],
        ));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, &mut comparison_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("comparison_form_count", json!(comparison_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EqlListComparisonItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_eql_list_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eql list comparison report")
    }

    fn comparisons(input: &str) -> (u64, Vec<EqlListComparisonItem>) {
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
    fn flags_eql_against_a_quoted_list() {
        let (comparison_form_count, violations) = comparisons("(eql x '(1 2))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eql");
        assert_eq!(violations[0].literal, "(1 2)");
    }

    #[test]
    fn flags_eq_against_an_explicit_quote_form() {
        let (_, violations) = comparisons("(eq (path n) (quote (:a :b)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn does_not_flag_a_quoted_symbol() {
        let (comparison_form_count, violations) = comparisons("(eq x 'foo)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_empty_quoted_list_which_is_nil() {
        let (_, violations) = comparisons("(eq x '())");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equal_against_a_quoted_list() {
        let (comparison_form_count, violations) = comparisons("(equal x '(1 2))");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) =
            comparisons("(defun f (x) (when (eql x '(a)) 1))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(eq x '(1 2))").expect("parse input");
        let report =
            build_eql_list_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build eql list comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eq x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_literal() {
        let report = report("(defun f (x)\n  (eql x '(1 2)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eql");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("eql")), ("literal", json!("(1 2)"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op=eql".to_owned(), "literal=(1 2)".to_owned()]
        );
    }

    /// The `kind` normalizes; the `operator` field does not, so the source
    /// spelling is still recoverable from the report.
    #[test]
    fn a_shouted_operator_normalizes_only_in_the_kind() {
        let (_, violations) = comparisons("(EQL x '(1 2))");
        assert_eq!(violations[0].kind(), "eql");
        assert_eq!(violations[0].operator, "EQL");
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(eql x '(1 2))\n(eq y 'foo)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
