//! Common Lisp constant-`when`/`unless`-test detection: a `when` or `unless`
//! whose test is the literal constant `t` or `nil`. Because `t` and `nil` are
//! constants that cannot be rebound, the branch is statically decided:
//!
//! - `(when t body…)` always runs its body, so it is `(progn body…)`.
//! - `(unless nil body…)` always runs its body, so it is `(progn body…)`.
//! - `(when nil body…)` never runs its body and yields `nil` — dead code.
//! - `(unless t body…)` never runs its body and yields `nil` — dead code.
//!
//! The "always runs" cases are rewritten to `progn` (splicing the head/test down
//! to `progn`), which preserves the last-form return value and evaluation order.
//! The "never runs" cases collapse to the literal `nil`; the discarded body is
//! never evaluated, so no side effects are lost. Both rewrites are exact, so the
//! rule is auto-fixable.
//!
//! Only the literal `t`/`nil` symbol is treated as constant; a truthy value like
//! `5` or a variable test is a legitimate condition and is left alone. Body forms
//! are preserved verbatim (always case) or discarded whole (never case), so a
//! reader-conditional in the body needs no special handling.
//!
//! This is the `when`/`unless` sibling of
//! [`crate::domain::constant_if_test_report`]. Reuses the shared whole-tree walk
//! from [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare literal `t` or `nil` (no reader prefixes); returns
/// which one so the caller can decide the branch.
fn constant_test(view: &ExpressionView) -> Option<bool> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    if text.eq_ignore_ascii_case("t") {
        Some(true)
    } else if text.eq_ignore_ascii_case("nil") {
        Some(false)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ConstantWhenTestItem {
    pub path: PathBuf,
    /// The span of the whole `(when TEST …)`/`(unless TEST …)` form.
    pub span: ByteSpan,
    /// The head operator, lowercased (`when` or `unless`).
    pub head: &'static str,
    /// The literal test, lowercased (`t` or `nil`).
    pub test: &'static str,
    /// Whether the body always runs (rewrite to `progn`) or never runs
    /// (collapse to `nil`).
    pub always_runs: bool,
    /// For the "always runs" rewrite: the span from the form's opening paren
    /// through the test atom, replaced wholesale with `(progn`.
    pub splice_span: ByteSpan,
}

#[derive(Debug)]
pub struct ConstantWhenTestSummary {
    pub when_form_count: usize,
    pub violations: Vec<ConstantWhenTestItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ConstantWhenTestPolicyOptions {
    fail_on_violation: bool,
}

impl ConstantWhenTestPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct ConstantWhenTestPolicy {
    pub fail_on_violation: bool,
    pub when_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_when(
    view: &ExpressionView,
    path: &Path,
    when_form_count: &mut usize,
    violations: &mut Vec<ConstantWhenTestItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let is_when = head.eq_ignore_ascii_case("when");
    let is_unless = head.eq_ignore_ascii_case("unless");
    if !is_when && !is_unless {
        return;
    }
    *when_form_count += 1;

    // children: [head, test, body…] — require at least the head and test.
    if view.children.len() < 2 {
        return;
    }
    let test = &view.children[1];
    let Some(is_true) = constant_test(test) else {
        return;
    };

    // `when` runs its body when the test is true; `unless` when it is false.
    let always_runs = is_when == is_true;
    let splice_span = ByteSpan::new(view.span.start(), test.span.end());

    violations.push(ConstantWhenTestItem {
        path: path.to_path_buf(),
        span: view.span,
        head: if is_when { "when" } else { "unless" },
        test: if is_true { "t" } else { "nil" },
        always_runs,
        splice_span,
    });
}

/// Collects every constant-test `when`/`unless` across a whole file, along with
/// the total number of `when`/`unless` forms scanned.
pub fn collect_constant_when_tests(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ConstantWhenTestItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut when_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_when(subview, path, &mut when_form_count, &mut violations)
        });
    }
    Ok((when_form_count, violations))
}

pub fn summarize_constant_when_tests(
    when_form_count: usize,
    violations: Vec<ConstantWhenTestItem>,
) -> ConstantWhenTestSummary {
    ConstantWhenTestSummary {
        when_form_count,
        violations,
    }
}

pub fn evaluate_constant_when_test_policy(
    options: ConstantWhenTestPolicyOptions,
    summary: &ConstantWhenTestSummary,
) -> ConstantWhenTestPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ConstantWhenTestPolicy {
        fail_on_violation: options.fail_on_violation(),
        when_form_count: summary.when_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whens(input: &str) -> (usize, Vec<ConstantWhenTestItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_constant_when_tests(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect constant when tests")
    }

    fn splice<'a>(source: &'a str, item: &ConstantWhenTestItem) -> &'a str {
        &source[item.splice_span.start().get()..item.splice_span.end().get()]
    }

    #[test]
    fn when_t_always_runs() {
        let source = "(when t a b)";
        let (count, violations) = whens(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].test, "t");
        assert!(violations[0].always_runs);
        assert_eq!(splice(source, &violations[0]), "(when t");
    }

    #[test]
    fn unless_nil_always_runs() {
        let source = "(unless nil a b)";
        let (_, violations) = whens(source);
        assert_eq!(violations[0].head, "unless");
        assert_eq!(violations[0].test, "nil");
        assert!(violations[0].always_runs);
        assert_eq!(splice(source, &violations[0]), "(unless nil");
    }

    #[test]
    fn when_nil_never_runs() {
        let source = "(when nil (side-effect))";
        let (_, violations) = whens(source);
        assert!(!violations[0].always_runs);
        assert_eq!(violations[0].test, "nil");
    }

    #[test]
    fn unless_t_never_runs() {
        let (_, violations) = whens("(unless t (go))");
        assert!(!violations[0].always_runs);
        assert_eq!(violations[0].head, "unless");
        assert_eq!(violations[0].test, "t");
    }

    #[test]
    fn empty_body_is_still_flagged() {
        let (_, violations) = whens("(when t)");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].always_runs);
    }

    #[test]
    fn does_not_flag_a_variable_test() {
        let (count, violations) = whens("(when ready a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_truthy_non_t_literal() {
        let (_, violations) = whens("(when 5 a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_test() {
        let (_, violations) = whens("(WHEN NIL a)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].test, "nil");
        assert!(!violations[0].always_runs);
    }

    #[test]
    fn finds_a_nested_constant_when() {
        let (_, violations) = whens("(defun f () (unless nil 1 2))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].always_runs);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(when t a b)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_constant_when_tests(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect constant when tests");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = whens("(when t a b)");
        let summary = summarize_constant_when_tests(count, items);

        let quiet =
            evaluate_constant_when_test_policy(ConstantWhenTestPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_constant_when_test_policy(ConstantWhenTestPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
