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

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};

#[derive(Debug, Clone)]
pub struct IdenticalIfBranchItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub branch: String,
}

#[derive(Debug)]
pub struct IdenticalIfBranchSummary {
    pub if_form_count: usize,
    pub identical: Vec<IdenticalIfBranchItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct IdenticalIfBranchPolicyOptions {
    fail_on_identical: bool,
}

impl IdenticalIfBranchPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_identical: bool) -> Self {
        Self { fail_on_identical }
    }

    #[must_use]
    pub const fn fail_on_identical(self) -> bool {
        self.fail_on_identical
    }
}

#[derive(Debug)]
pub struct IdenticalIfBranchPolicy {
    pub fail_on_identical: bool,
    pub if_form_count: usize,
    pub identical_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    path: &Path,
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
            path: path.to_path_buf(),
            span: view.span,
            branch: render_expression(then_branch),
        });
    }
}

/// Collects every `if` whose two branches are structurally identical across a
/// whole file, along with the total number of two-armed `if` forms scanned.
pub fn collect_identical_if_branches(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<IdenticalIfBranchItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut if_form_count = 0;
    let mut identical = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, path, &mut if_form_count, &mut identical);
        });
    }
    Ok((if_form_count, identical))
}

#[must_use]
pub const fn summarize_identical_if_branches(
    if_form_count: usize,
    identical: Vec<IdenticalIfBranchItem>,
) -> IdenticalIfBranchSummary {
    IdenticalIfBranchSummary {
        if_form_count,
        identical,
    }
}

#[must_use]
pub fn evaluate_identical_if_branch_policy(
    options: IdenticalIfBranchPolicyOptions,
    summary: &IdenticalIfBranchSummary,
) -> IdenticalIfBranchPolicy {
    let identical_count = summary.identical.len();
    let mut violations = Vec::new();
    if options.fail_on_identical() && identical_count > 0 {
        violations.push(format!("identical_count {identical_count} exceeds 0"));
    }

    IdenticalIfBranchPolicy {
        fail_on_identical: options.fail_on_identical(),
        if_form_count: summary.if_form_count,
        identical_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branches(input: &str) -> (usize, Vec<IdenticalIfBranchItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_identical_if_branches(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect identical if branches")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(if test a a)").expect("parse input");
        let (if_form_count, identical) =
            collect_identical_if_branches(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect identical if branches");
        assert_eq!(if_form_count, 0);
        assert!(identical.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (if_form_count, items) = branches("(if test a a)");
        let summary = summarize_identical_if_branches(if_form_count, items);

        let quiet = evaluate_identical_if_branch_policy(
            IdenticalIfBranchPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.identical_count, 1);

        let strict = evaluate_identical_if_branch_policy(
            IdenticalIfBranchPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
