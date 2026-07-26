//! Common Lisp `cond`-single-`t`-clause detection: a `cond` with exactly one
//! clause whose test is the literal `t` and which has a body — `(cond (t a b))`.
//! Since `t` always holds, the `cond` unconditionally runs that clause's body
//! and there is nothing else to fall through to, so the form is exactly
//! `(progn a b)` — same body, same last-form return value.
//!
//! This is the `t`-clause complement of
//! [`crate::domain::single_clause_cond_report`], which rewrites a single
//! *non-`t`* clause `(cond (test body…))` to `(when test body…)` and
//! deliberately leaves the `t` case to this rule.
//!
//! Only the provably-unconditional shape is flagged:
//!
//!   - exactly one clause (a multi-clause `cond` whose *last* clause is `(t …)`
//!     is the idiomatic else and is a real branch — left alone),
//!   - the clause is a parenthesized list whose test is the bare literal `t`
//!     (no reader prefix); `otherwise` is *not* special in `cond` (only in
//!     `case`/`typecase`), so it is not treated as a catch-all, and
//!   - the clause has at least one body form; a test-only `(cond (t))` returns
//!     the test value `t`, which `progn` cannot express, so it is left alone.
//!
//! The fix rewrites `(cond (t body…))` as `(progn body…)`, copying the body
//! forms verbatim, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// Whether `view` is the bare literal `t` (no reader prefixes).
fn is_literal_t(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// clause containing one has no settled shape.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct CondTClauseItem {
    pub path: PathBuf,
    /// The span of the whole `(cond (t body…))` form.
    pub span: ByteSpan,
    /// The span of the clause body (`body…`, test and parens excluded), spliced
    /// into `(progn …)`.
    pub body_span: ByteSpan,
}

#[derive(Debug)]
pub struct CondTClauseSummary {
    pub cond_form_count: usize,
    pub violations: Vec<CondTClauseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CondTClausePolicyOptions {
    fail_on_violation: bool,
}

impl CondTClausePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct CondTClausePolicy {
    pub fail_on_violation: bool,
    pub cond_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_cond(
    view: &ExpressionView,
    path: &Path,
    cond_form_count: &mut usize,
    violations: &mut Vec<CondTClauseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("cond") {
        return;
    }
    *cond_form_count += 1;

    // children: [cond, clause] — require exactly one clause.
    if view.children.len() != 2 {
        return;
    }
    let clause = &view.children[1];
    if !is_paren_list(clause) {
        return;
    }
    // Need the literal `t` test and at least one body form.
    if clause.children.len() < 2 {
        return;
    }
    if !is_literal_t(&clause.children[0]) {
        return;
    }
    if clause.children.iter().any(is_reader_conditional) {
        return;
    }

    // Body span runs from the first body form through the last, dropping the
    // `t` test and the clause parentheses.
    let body_start = clause.children[1].span.start();
    let body_end = clause.children[clause.children.len() - 1].span.end();

    violations.push(CondTClauseItem {
        path: path.to_path_buf(),
        span: view.span,
        body_span: ByteSpan::new(body_start, body_end),
    });
}

/// Collects every single-`t`-clause `cond` across a whole file, along with the
/// total number of `cond` forms scanned.
pub fn collect_cond_t_clauses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<CondTClauseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut cond_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cond(subview, path, &mut cond_form_count, &mut violations);
        });
    }
    Ok((cond_form_count, violations))
}

pub fn summarize_cond_t_clauses(
    cond_form_count: usize,
    violations: Vec<CondTClauseItem>,
) -> CondTClauseSummary {
    CondTClauseSummary {
        cond_form_count,
        violations,
    }
}

pub fn evaluate_cond_t_clause_policy(
    options: CondTClausePolicyOptions,
    summary: &CondTClauseSummary,
) -> CondTClausePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CondTClausePolicy {
        fail_on_violation: options.fail_on_violation(),
        cond_form_count: summary.cond_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conds(input: &str) -> (usize, Vec<CondTClauseItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_cond_t_clauses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect cond-t-clause")
    }

    fn body<'a>(source: &'a str, item: &CondTClauseItem) -> &'a str {
        &source[item.body_span.start().get()..item.body_span.end().get()]
    }

    #[test]
    fn flags_single_t_clause_with_body() {
        let source = "(cond (t a b))";
        let (count, violations) = conds(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(body(source, &violations[0]), "a b");
    }

    #[test]
    fn preserves_compound_body_forms() {
        let source = "(cond (t (setup) (run x)))";
        let (_, violations) = conds(source);
        assert_eq!(body(source, &violations[0]), "(setup) (run x)");
    }

    #[test]
    fn does_not_flag_a_test_only_clause() {
        // (cond (t)) returns the test value t, which progn cannot express.
        let (_, violations) = conds("(cond (t))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_t_test() {
        let (_, violations) = conds("(cond (ready a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_clause_cond() {
        // A trailing (t …) else in a real branch is idiomatic, not a violation.
        let (count, violations) = conds("(cond (p a) (t b))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_test() {
        let (_, violations) = conds("(COND (T a))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_cond() {
        let (_, violations) = conds("(defun f () (cond (t (go))))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(cond (t a))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_cond_t_clauses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect cond-t-clause");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = conds("(cond (t a))");
        let summary = summarize_cond_t_clauses(count, items);

        let quiet = evaluate_cond_t_clause_policy(CondTClausePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_cond_t_clause_policy(CondTClausePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
