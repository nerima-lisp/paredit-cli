//! Common Lisp duplicate-`cond`-test detection: a `cond` form with two
//! clauses whose test expressions are structurally identical. `cond`
//! evaluates clause tests top to bottom and takes the first that is true, so
//! a later clause repeating an earlier test can never run — dead code, and
//! the analog of the well-known `ifs_same_cond` lint.
//!
//! Unlike [`crate::duplicate_case_keys::domain`], whose clause heads are
//! `eql` literal keys, a `cond` test is an arbitrary expression, so this
//! report compares tests with the reader-aware structural equality from
//! [`paredit_core_syntax::expression_equality`] — `(= x 1)` and `(= X 1)` are the
//! same test, `(foo)` and `(bar)` are not. It walks the whole expression
//! tree, since a `cond` can appear anywhere in a body.
//!
//! A repeated catch-all (`t` or `otherwise`) is reported too: a second `t`
//! clause is exactly as unreachable as any other repeated test.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct DuplicateCondTestItem {
    /// The span of the whole `cond` form the repeat was found in.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    pub test: String,
    pub occurrence_count: usize,
}

impl Finding for DuplicateCondTestItem {
    /// The rule's own name rather than the repeated test: a `cond` test is an
    /// arbitrary expression, so there is no closed set of `&'static str` names
    /// to draw a kind from. The test itself stays a JSON field and a column.
    fn kind(&self) -> &'static str {
        "duplicate-cond-tests"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("test={}", self.test),
            format!("count={}", self.occurrence_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("test", json!(self.test)),
            ("occurrence_count", json!(self.occurrence_count)),
        ]
    }

    /// The same sentence the `duplicate-cond-tests` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "cond repeats test {} ({}×)",
            self.test, self.occurrence_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_cond(
    view: &ExpressionView,
    source: &str,
    cond_form_count: &mut usize,
    duplicates: &mut Vec<DuplicateCondTestItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("cond")) {
        return;
    }
    *cond_form_count += 1;

    // Each clause is `(test . body)`; a non-list or empty clause has no test
    // to compare.
    let tests: Vec<&ExpressionView> = view
        .children
        .iter()
        .skip(1)
        .filter(|clause| is_paren_list(clause))
        .filter_map(|clause| clause.children.first())
        .collect();

    // Pairwise grouping by structural equality — clause counts are small, so
    // the quadratic scan is cheaper than canonicalizing every test.
    let mut grouped = vec![false; tests.len()];
    for anchor in 0..tests.len() {
        if grouped[anchor] {
            continue;
        }
        let mut occurrence_count = 1;
        for candidate in (anchor + 1)..tests.len() {
            if !grouped[candidate]
                && expressions_structurally_equal(tests[anchor], tests[candidate])
            {
                grouped[candidate] = true;
                occurrence_count += 1;
            }
        }
        if occurrence_count >= 2 {
            duplicates.push(DuplicateCondTestItem {
                span: view.span,
                line: line_of(source, view.span.start().get()),
                test: render_expression(tests[anchor]),
                occurrence_count,
            });
        }
    }
}

/// Collects every duplicated `cond` test in one file, with the number of `cond`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no repeated test here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_duplicate_cond_test_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateCondTestItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("cond_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut cond_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cond(subview, source, &mut cond_form_count, &mut duplicates);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        duplicates,
        vec![("cond_form_count", json!(cond_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateCondTestItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_duplicate_cond_test_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build duplicate cond test report")
    }

    /// The `(cond_form_count, duplicates)` pair the report is built from.
    fn duplicates(input: &str) -> (u64, Vec<DuplicateCondTestItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "cond_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("cond_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_repeated_test_expression() {
        let (cond_form_count, duplicates) = duplicates("(cond ((foo) 1) ((bar) 2) ((foo) 3))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].test, "(foo)");
        assert_eq!(duplicates[0].occurrence_count, 2);
    }

    #[test]
    fn folds_symbol_case_inside_the_test() {
        let (_, duplicates) = duplicates("(cond ((= x 1) 1) ((= X 1) 2))");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_tests() {
        let (cond_form_count, duplicates) = duplicates("(cond ((foo) 1) ((bar) 2) (t 3))");
        assert_eq!(cond_form_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn flags_a_repeated_catch_all() {
        let (_, duplicates) = duplicates("(cond ((foo) 1) (t 2) (t 3))");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].test, "t");
    }

    #[test]
    fn finds_a_cond_nested_in_a_function_body() {
        let (cond_form_count, duplicates) = duplicates("(defun f (x) (cond ((p x) 1) ((p x) 2)))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(duplicates.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(cond ((foo) 1) ((foo) 2))").expect("parse input");
        let report =
            build_duplicate_cond_test_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build duplicate cond test report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("cond_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(cond ((foo) 1) ((bar) 2))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_test_and_its_count() {
        let report = report("(defun f ()\n  (cond ((foo) 1) ((foo) 2)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "duplicate-cond-tests");
        assert_eq!(
            finding.json_fields(),
            vec![("test", json!("(foo)")), ("occurrence_count", json!(2))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["test=(foo)".to_owned(), "count=2".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_cond_scanned_not_only_the_flagged_ones() {
        let report = report("(cond ((foo) 1) ((foo) 2))\n(cond ((a) 1) ((b) 2))\n");
        assert_eq!(report.summary, vec![("cond_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
