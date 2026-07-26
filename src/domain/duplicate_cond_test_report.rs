//! Common Lisp duplicate-`cond`-test detection: a `cond` form with two
//! clauses whose test expressions are structurally identical. `cond`
//! evaluates clause tests top to bottom and takes the first that is true, so
//! a later clause repeating an earlier test can never run — dead code, and
//! the analog of the well-known `ifs_same_cond` lint.
//!
//! Unlike [`crate::domain::duplicate_case_key_report`], whose clause heads are
//! `eql` literal keys, a `cond` test is an arbitrary expression, so this
//! report compares tests with the reader-aware structural equality from
//! [`crate::domain::expression_equality`] — `(= x 1)` and `(= X 1)` are the
//! same test, `(foo)` and `(bar)` are not. It walks the whole expression
//! tree, since a `cond` can appear anywhere in a body.
//!
//! A repeated catch-all (`t` or `otherwise`) is reported too: a second `t`
//! clause is exactly as unreachable as any other repeated test.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::{expressions_structurally_equal, render_expression};
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, is_paren_list, list_head};

#[derive(Debug, Clone)]
pub struct DuplicateCondTestItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub test: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateCondTestSummary {
    pub cond_form_count: usize,
    pub duplicates: Vec<DuplicateCondTestItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateCondTestPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateCondTestPolicyOptions {
    pub fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateCondTestPolicy {
    pub fail_on_duplicate: bool,
    pub cond_form_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_cond(
    view: &ExpressionView,
    path: &Path,
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
                path: path.to_path_buf(),
                span: view.span,
                test: render_expression(tests[anchor]),
                occurrence_count,
            });
        }
    }
}

/// Collects every duplicated `cond` test across a whole file, along with the
/// total number of `cond` forms scanned.
pub fn collect_duplicate_cond_tests(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateCondTestItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut cond_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cond(subview, path, &mut cond_form_count, &mut duplicates);
        });
    }
    Ok((cond_form_count, duplicates))
}

pub fn summarize_duplicate_cond_tests(
    cond_form_count: usize,
    duplicates: Vec<DuplicateCondTestItem>,
) -> DuplicateCondTestSummary {
    DuplicateCondTestSummary {
        cond_form_count,
        duplicates,
    }
}

pub fn evaluate_duplicate_cond_test_policy(
    options: DuplicateCondTestPolicyOptions,
    summary: &DuplicateCondTestSummary,
) -> DuplicateCondTestPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateCondTestPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        cond_form_count: summary.cond_form_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateCondTestItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_duplicate_cond_tests(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate cond tests")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(cond ((foo) 1) ((foo) 2))").expect("parse input");
        let (cond_form_count, duplicates) =
            collect_duplicate_cond_tests(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate cond tests");
        assert_eq!(cond_form_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (cond_form_count, items) = duplicates("(cond ((foo) 1) ((foo) 2))");
        let summary = summarize_duplicate_cond_tests(cond_form_count, items);

        let quiet = evaluate_duplicate_cond_test_policy(
            DuplicateCondTestPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict = evaluate_duplicate_cond_test_policy(
            DuplicateCondTestPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
