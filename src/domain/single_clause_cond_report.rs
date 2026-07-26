//! Common Lisp single-clause-`cond` detection: a `cond` with exactly one
//! clause that has a test and a body. `cond` walks its clauses in order,
//! evaluating each test until one is non-nil and then running that clause's
//! body; with a single clause there is nothing to fall through to, so
//! `(cond (test a b))` is exactly `(when test a b)` — same test, same body,
//! same nil-on-failure result. The `when` states "run this body only when the
//! test holds" directly, without the reader scanning for further clauses.
//!
//! Only the narrow, provably-equivalent shape is flagged:
//!
//!   - exactly one clause (a two-or-more-clause `cond` is a real branch),
//!   - the clause is a parenthesized list holding a test **and** at least one
//!     body form (a test-only `(cond (test))` returns the *test value*, which
//!     `when` cannot express, so it is left alone), and
//!   - the test is not the literal `t`/`otherwise` catch-all (that shape is a
//!     plain `(progn …)`, a different rewrite), nor a reader-conditional
//!     operand (build-dependent).
//!
//! The fix rewrites `(cond (test body…))` as `(when test body…)` by wrapping
//! the clause interior verbatim, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteOffset, ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// clause containing one has no settled shape.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether `view` is the bare `t`/`otherwise` catch-all test (no reader
/// prefixes). Such a single clause is a `progn`, not a `when`.
fn is_catch_all_test(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| {
            text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("otherwise")
        })
}

#[derive(Debug, Clone)]
pub struct SingleClauseCondItem {
    pub path: PathBuf,
    /// The span of the whole `(cond (…))` form.
    pub span: ByteSpan,
    /// The span of the clause interior (`test body…`, parens excluded), which
    /// the fix wraps as `(when …)`.
    pub clause_inner_span: ByteSpan,
}

#[derive(Debug)]
pub struct SingleClauseCondSummary {
    pub cond_form_count: usize,
    pub violations: Vec<SingleClauseCondItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SingleClauseCondPolicyOptions {
    fail_on_violation: bool,
}

impl SingleClauseCondPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct SingleClauseCondPolicy {
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
    violations: &mut Vec<SingleClauseCondItem>,
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
    // Need a test and at least one body form; a test-only clause returns the
    // test value, which `when` cannot reproduce.
    if clause.children.len() < 2 {
        return;
    }
    let test = &clause.children[0];
    if is_catch_all_test(test) {
        return;
    }
    if clause.children.iter().any(is_reader_conditional) {
        return;
    }

    // Strip the clause's own parentheses: `(test body…)` -> `test body…`.
    let inner_start = clause.span.start().get() + 1;
    let inner_end = clause.span.end().get().saturating_sub(1);
    if inner_end <= inner_start {
        return;
    }
    let clause_inner_span = ByteSpan::new(ByteOffset::new(inner_start), ByteOffset::new(inner_end));

    violations.push(SingleClauseCondItem {
        path: path.to_path_buf(),
        span: view.span,
        clause_inner_span,
    });
}

/// Collects every single-clause `cond` across a whole file, along with the
/// total number of `cond` forms scanned.
pub fn collect_single_clause_conds(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SingleClauseCondItem>)> {
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

pub fn summarize_single_clause_conds(
    cond_form_count: usize,
    violations: Vec<SingleClauseCondItem>,
) -> SingleClauseCondSummary {
    SingleClauseCondSummary {
        cond_form_count,
        violations,
    }
}

pub fn evaluate_single_clause_cond_policy(
    options: SingleClauseCondPolicyOptions,
    summary: &SingleClauseCondSummary,
) -> SingleClauseCondPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SingleClauseCondPolicy {
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

    fn conds(input: &str) -> (usize, Vec<SingleClauseCondItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_single_clause_conds(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect single-clause conds")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_clause_with_body() {
        let source = "(cond ((> x 0) (f x)))";
        let (count, violations) = conds(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].clause_inner_span),
            "(> x 0) (f x)"
        );
    }

    #[test]
    fn flags_symbol_test() {
        let source = "(cond (ready (go)))";
        let (_, violations) = conds(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].clause_inner_span), "ready (go)");
    }

    #[test]
    fn preserves_multi_form_body_source() {
        let source = "(cond (test a b c))";
        let (_, violations) = conds(source);
        assert_eq!(slice(source, violations[0].clause_inner_span), "test a b c");
    }

    #[test]
    fn does_not_flag_two_clauses() {
        let (count, violations) = conds("(cond (a 1) (b 2))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_catch_all_t() {
        let (_, violations) = conds("(cond (t (default)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_otherwise() {
        let (_, violations) = conds("(cond (otherwise x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_test_only_clause() {
        // (cond (test)) returns the test value; when cannot express that.
        let (_, violations) = conds("(cond ((compute)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_cond() {
        let (count, violations) = conds("(cond)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_paren_clause() {
        let (_, violations) = conds("(cond foo)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_reader_conditional_body() {
        let (_, violations) = conds("(cond (test #+sbcl (sb-thing) other))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = conds("(COND (test body))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_single_clause_cond() {
        let (_, violations) = conds("(defun f (x) (cond ((plusp x) (g x))))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(cond (test body))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_single_clause_conds(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect single-clause conds");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = conds("(cond (test body))");
        let summary = summarize_single_clause_conds(count, items);

        let quiet =
            evaluate_single_clause_cond_policy(SingleClauseCondPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_single_clause_cond_policy(SingleClauseCondPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
