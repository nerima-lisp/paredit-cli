//! Common Lisp single-argument-comparison detection: a numeric comparison
//! (`<`, `>`, `<=`, `>=`, `=`, `/=`) called with exactly one argument. These
//! operators are variadic and defined to return `t` for a single argument, so
//! `(< x)`, `(= x)`, `(/= x)` and friends are *vacuously true* no matter what
//! `x` is — the comparison does nothing. In practice this is a missing operand
//! (`(< x y)` mistyped as `(< x)`), which no compiler flags because the call is
//! technically legal.
//!
//! Only these six variadic numeric comparisons are considered. The fixed-arity
//! equality tests (`eq`/`eql`/`equal`/`equalp`) are handled by the
//! `equality-arity` rule instead, since for them a single argument is an arity
//! error rather than a vacuous truth. A lone reader conditional (`#+`/`#-`) as
//! the sole argument is exempt: it may expand to zero or more arguments.
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

/// The variadic numeric comparison operators for which a single argument is
/// vacuously true. Matched exactly (these symbols have no alphabetic case).
const COMPARISONS: [&str; 6] = ["<", ">", "<=", ">=", "=", "/="];

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom does not represent one argument.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SingleArgComparisonItem {
    /// The span of the whole `(< x)`-style form.
    pub span: ByteSpan,
    /// The comparison operator (`<`, `>`, `<=`, `>=`, `=`, or `/=`).
    pub operator: &'static str,
}

/// `severity` is left at the envelope's default `Warning` even though this
/// rule's `RuleMeta` calls it an `Error`. The two scales are not the same one:
/// the lint suite's is how loudly a rule speaks inside an aggregated run, and
/// the envelope's is a rung in the SARIF/Code-Climate vocabulary shared by
/// every report in this workspace. Nothing else in that campaign grades itself
/// above `Warning`, and a lone `error` here would read as a ranking rather than
/// as this rule's opinion of itself.
impl Finding for SingleArgComparisonItem {
    /// The rule's own name.
    ///
    /// All six operators are punctuation (`<`, `>=`, `/=`), which makes a poor
    /// `grep` selector and a worse SARIF rule id, and there is only one kind of
    /// finding here anyway. The operator stays a reported field instead.
    fn kind(&self) -> &'static str {
        "single-arg-comparison"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("operator={}", self.operator)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `single-arg-comparison` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} has a single argument; the comparison is always true (missing an operand?)",
            self.operator
        )
    }
}

pub fn examine_comparison(
    view: &ExpressionView,
    comparison_form_count: &mut usize,
    violations: &mut Vec<SingleArgComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = COMPARISONS.iter().copied().find(|op| *op == head) else {
        return;
    };
    *comparison_form_count += 1;

    // children[0] is the operator; a single argument means exactly two children.
    if view.children.len() != 2 || is_reader_conditional(&view.children[1]) {
        return;
    }
    violations.push(SingleArgComparisonItem {
        span: view.span,
        operator,
    });
}

/// Collects every single-argument numeric comparison in one file, with the
/// number of comparison forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every comparison here has its operands"
/// for Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_single_arg_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SingleArgComparisonItem>> {
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

    fn report(input: &str) -> FileFindings<SingleArgComparisonItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_single_arg_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build single-arg comparison report")
    }

    /// The `(comparison_form_count, violations)` pair the report is built from.
    fn comparisons(input: &str) -> (u64, Vec<SingleArgComparisonItem>) {
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
    fn flags_single_arg_less_than() {
        let (count, violations) = comparisons("(when (< x) (go))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "<");
    }

    #[test]
    fn flags_every_comparison_operator() {
        for op in ["<", ">", "<=", ">=", "=", "/="] {
            let (_, violations) = comparisons(&format!("({op} x)"));
            assert_eq!(violations.len(), 1, "operator {op} should be flagged");
            assert_eq!(violations[0].operator, op);
        }
    }

    #[test]
    fn flags_a_single_complex_argument() {
        let (_, violations) = comparisons("(= (length xs))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "=");
    }

    #[test]
    fn does_not_flag_two_arguments() {
        let (count, violations) = comparisons("(< x y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_many_arguments() {
        let (_, violations) = comparisons("(<= a b c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equality_predicates() {
        // eq/eql/equal are the equality-arity rule's territory, not this one.
        let (count, violations) = comparisons("(eql x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional() {
        let (_, violations) = comparisons("(< #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_comparison_heads() {
        let (count, violations) = comparisons("(max x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_single_arg_comparison() {
        let (_, violations) = comparisons("(and ready (> total))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, ">");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(< x)", Dialect::Clojure).expect("parse");
        let report =
            build_single_arg_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build single-arg comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(< x y)").dialect_modelled);
    }

    /// The operator is punctuation, so it stays a column and a JSON field
    /// rather than becoming the `kind`.
    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun f (x)\n  (when (/= x) (go)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "single-arg-comparison");
        assert_eq!(finding.json_fields(), vec![("operator", json!("/="))]);
        assert_eq!(finding.text_columns(), vec!["operator=/=".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_comparison_scanned_not_only_the_flagged_ones() {
        let report = report("(< x)\n(< x y)\n(<= a b c)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
