//! Common Lisp `eq`/`eql`-on-a-string detection: a call to `eq` or `eql`
//! with a string-literal argument, such as `(eql name "root")` or
//! `(eq (status x) "done")`. Two strings are `eql` only if they are the
//! *same object*, which distinct string literals never are, so such a test
//! is almost always false regardless of the string's contents — a classic
//! Common Lisp bug where `string=` or `equal` was intended.
//!
//! Character literals (`#\a`) are excluded: characters *are* reliably `eql`,
//! so `(eql c #\a)` is correct and common. Only string literals (`"..."`)
//! trigger the report.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`], since such a call can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

const EQ_HEADS: [&str; 2] = ["eq", "eql"];

fn is_string_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with('"'))
}

#[derive(Debug, Clone)]
pub struct EqlStringComparisonItem {
    pub span: ByteSpan,
    /// The operator exactly as it was written, so its source casing survives.
    pub operator: String,
    pub literal: String,
}

impl Finding for EqlStringComparisonItem {
    /// Which of the two identity predicates was used, normalized to the
    /// spelling this rule matched on.
    ///
    /// They are two different mistakes to make — `eq` on a string is wrong for
    /// the same reason `eql` is, but a codebase mid-way through replacing one
    /// with the other cares which it is looking at — and a consumer filtering
    /// on one of them is asking a real question. `operator` keeps the source
    /// casing that this discards.
    fn kind(&self) -> &'static str {
        EQ_HEADS
            .iter()
            .find(|name| self.operator.eq_ignore_ascii_case(name))
            .copied()
            .unwrap_or("eql-string-comparison")
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
            "{} compares against string literal {}",
            self.operator, self.literal
        )
    }
}

pub fn examine_comparison(
    view: &ExpressionView,
    comparison_form_count: &mut usize,
    violations: &mut Vec<EqlStringComparisonItem>,
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

    // A comparison needs both operands. `(eql "a_eql")` is a row in a data
    // table or a one-argument pattern in a match DSL such as trivia's
    // `(is-match #\a (eql #\a))`, and SBCL already warns that `eq` "is called
    // with one argument, but wants exactly two" — so whatever a one-argument
    // form is, it is not the bug this rule is named for. Counted in the
    // denominator either way: it *is* an `eq`/`eql` form that was scanned.
    if view.children.len() < 3 {
        return;
    }

    // Report the first string-literal argument (after the operator); a call
    // with two string literals is still one bug, not two.
    if let Some(literal) = view
        .children
        .iter()
        .skip(1)
        .find(|argument| is_string_literal(argument))
    {
        violations.push(EqlStringComparisonItem {
            span: view.span,
            operator: head.to_owned(),
            literal: atom_text(literal).unwrap_or_default().to_owned(),
        });
    }
}

/// Collects every `eq`/`eql` call with a string-literal argument in one file,
/// with the number of `eq`/`eql` calls scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_eql_string_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EqlStringComparisonItem>> {
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

    fn report(input: &str) -> FileFindings<EqlStringComparisonItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_eql_string_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eql string comparison report")
    }

    fn comparisons(input: &str) -> (u64, Vec<EqlStringComparisonItem>) {
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
    fn flags_eql_against_a_string_literal() {
        let (comparison_form_count, violations) = comparisons("(eql name \"root\")");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eql");
        assert_eq!(violations[0].literal, "\"root\"");
    }

    #[test]
    fn flags_eq_against_a_string_literal() {
        let (_, violations) = comparisons("(eq (status x) \"done\")");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn does_not_flag_eql_against_a_character_literal() {
        let (comparison_form_count, violations) = comparisons("(eql c #\\a)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_eql_between_two_symbols() {
        let (comparison_form_count, violations) = comparisons("(eql x y)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equal_or_string_equal() {
        let (comparison_form_count, violations) = comparisons("(equal x \"root\")");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) =
            comparisons("(defun f (x) (when (eq x \"y\") 1))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(eq x \"root\")").expect("parse input");
        let report =
            build_eql_string_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build eql string comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eql x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_literal() {
        let report = report("(defun f (name)\n  (eql name \"root\"))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eql");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("eql")), ("literal", json!("\"root\""))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op=eql".to_owned(), "literal=\"root\"".to_owned()]
        );
    }

    /// The `kind` normalizes; the `operator` field does not, so the source
    /// spelling is still recoverable from the report.
    #[test]
    fn a_shouted_operator_normalizes_only_in_the_kind() {
        let (_, violations) = comparisons("(EQ x \"root\")");
        assert_eq!(violations[0].kind(), "eq");
        assert_eq!(violations[0].operator, "EQ");
    }

    /// A one-argument form is not a comparison — trivia spells patterns that
    /// way, and SBCL warns that `eq` "wants exactly two" — but it *is* an
    /// `eq`/`eql` form that was scanned, so the denominator still counts it.
    #[test]
    fn a_one_argument_form_is_counted_but_not_flagged() {
        let (comparison_form_count, violations) = comparisons("(eql \"a_eql\")");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(eql x \"root\")\n(eq y z)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
