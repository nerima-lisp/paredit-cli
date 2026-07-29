//! Common Lisp identical-`if`-branch detection: an `(if test then else)`
//! whose `then` and `else` branches are structurally identical. Whatever the
//! test evaluates to, the `if` yields the same code either way, so the test —
//! and often a whole tangle of surrounding logic — is dead. This is the
//! Common Lisp analog of the well-known `if_same_then_else` lint, almost
//! always a copy-paste error where one branch was never edited to differ.
//!
//! Like `self-assignment`, an `if` can appear
//! anywhere in a body, so this report walks the whole expression tree and
//! reuses the same reader-aware structural comparison from
//! [`paredit_core_syntax::expression_equality`].
//!
//! Scope: Common Lisp only, and only the two-armed `(if test then else)`
//! shape — a one-armed `(if test then)` has no second branch to compare, and
//! a malformed `if` with extra trailing forms is left alone rather than
//! guessed at.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct IdenticalIfBranchItem {
    /// The span of the whole `(if test then else)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    pub branch: String,
}

impl Finding for IdenticalIfBranchItem {
    /// The rule's own name rather than the repeated branch: a branch is an
    /// arbitrary expression, so there is no closed set of `&'static str` names
    /// to draw a kind from. The branch itself stays a JSON field and a column.
    fn kind(&self) -> &'static str {
        "identical-if-branches"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("branch={}", self.branch)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("branch", json!(self.branch))]
    }

    /// The same sentence the `identical-if-branches` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("if branches are identical: {}", self.branch)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    source: &str,
    if_form_count: &mut usize,
    identical: &mut Vec<IdenticalIfBranchItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("if")) {
        return;
    }
    // A two-armed `if` has exactly four children: `if`, test, then, else.
    if view.children.len() != 4 {
        return;
    }
    *if_form_count += 1;
    let then_branch = &view.children[2];
    let else_branch = &view.children[3];
    if expressions_structurally_equal(then_branch, else_branch) {
        identical.push(IdenticalIfBranchItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            branch: render_expression(then_branch),
        });
    }
}

/// Collects every `if` whose two branches are structurally identical in one
/// file, with the number of two-armed `if` forms scanned as the denominator
/// beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no `if` here repeats itself" for Common
/// Lisp and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_identical_if_branch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IdenticalIfBranchItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("if_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut if_form_count = 0;
    let mut identical = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, source, &mut if_form_count, &mut identical);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        identical,
        vec![("if_form_count", json!(if_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<IdenticalIfBranchItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_identical_if_branch_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build identical if branch report")
    }

    /// The `(if_form_count, identical)` pair the report is built from.
    fn branches(input: &str) -> (u64, Vec<IdenticalIfBranchItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "if_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("if_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_an_if_with_identical_branches() {
        let (if_form_count, identical) = branches("(if test (foo x) (foo x))");
        assert_eq!(if_form_count, 1);
        assert_eq!(identical.len(), 1);
        assert_eq!(identical[0].branch, "(foo x)");
    }

    #[test]
    fn does_not_flag_an_if_with_different_branches() {
        let (if_form_count, identical) = branches("(if test (foo x) (bar x))");
        assert_eq!(if_form_count, 1);
        assert!(identical.is_empty());
    }

    #[test]
    fn does_not_flag_a_one_armed_if() {
        let (if_form_count, identical) = branches("(if test (foo x))");
        assert_eq!(if_form_count, 0);
        assert!(identical.is_empty());
    }

    #[test]
    fn folds_symbol_case_between_branches() {
        let (_, identical) = branches("(if test result RESULT)");
        assert_eq!(identical.len(), 1);
    }

    #[test]
    fn finds_an_if_nested_in_a_function_body() {
        let (if_form_count, identical) = branches("(defun f (test x) (if test (g x) (g x)))");
        assert_eq!(if_form_count, 1);
        assert_eq!(identical.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(if test a a)").expect("parse input");
        let report =
            build_identical_if_branch_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build identical if branch report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if test a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_branch() {
        let report = report("(defun f (test x)\n  (if test (g x) (g x)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "identical-if-branches");
        assert_eq!(finding.json_fields(), vec![("branch", json!("(g x)"))]);
        assert_eq!(finding.text_columns(), vec!["branch=(g x)".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_two_armed_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if test a a)\n(if test a b)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
