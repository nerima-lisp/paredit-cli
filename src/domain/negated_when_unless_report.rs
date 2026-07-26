//! Common Lisp negated-`when`/`unless` detection: a `(when (not X) …)` or
//! `(unless (not X) …)` whose test is a single negation. `when` and `unless`
//! are exact complements, so negating the test just to pick one is a
//! double-negative that reads backwards — `(when (not ready) …)` is more
//! directly written `(unless ready …)`, and `(unless (not ready) …)` as
//! `(when ready …)`.
//!
//! Both `not` and its list-specific synonym `null` count as the negation (they
//! are interchangeable on a generalized boolean). Only a negation of *exactly
//! one* argument is flagged — a malformed `(not)` or `(not a b)` is left for the
//! arity lints, and its rewrite is not well defined.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, is_paren_list, list_head};

/// The complement macro for a `when`/`unless` head: flipping the conditional is
/// half of removing the redundant negation.
fn complement_head(head: &str) -> Option<&'static str> {
    if head.eq_ignore_ascii_case("when") {
        Some("unless")
    } else if head.eq_ignore_ascii_case("unless") {
        Some("when")
    } else {
        None
    }
}

/// The negation operator of a `(not X)` / `(null X)` test with exactly one
/// argument, lowercased, or `None` if the view is not such a negation.
fn single_negation(view: &ExpressionView) -> Option<&'static str> {
    if !is_paren_list(view) || view.children.len() != 2 {
        return None;
    }
    let head = list_head(view)?;
    if head.eq_ignore_ascii_case("not") {
        Some("not")
    } else if head.eq_ignore_ascii_case("null") {
        Some("null")
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct NegatedWhenUnlessItem {
    pub path: PathBuf,
    /// The span of the whole `(when …)`/`(unless …)` form.
    pub span: ByteSpan,
    /// The span of the head symbol (`when`/`unless`), for a flip-the-macro fix.
    pub head_span: ByteSpan,
    /// The span of the whole `(not X)`/`(null X)` test, replaced by `X`.
    pub test_span: ByteSpan,
    /// The span of the negation's sole argument `X` (the un-negated test).
    pub inner_span: ByteSpan,
    /// The conditional macro as written, lowercased (`when` or `unless`).
    pub head: &'static str,
    /// The negation operator (`not` or `null`).
    pub negator: &'static str,
    /// The macro the rewrite would use instead (the complement of `head`).
    pub suggested_head: &'static str,
}

#[derive(Debug)]
pub struct NegatedWhenUnlessSummary {
    pub conditional_form_count: usize,
    pub violations: Vec<NegatedWhenUnlessItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NegatedWhenUnlessPolicyOptions {
    fail_on_violation: bool,
}

impl NegatedWhenUnlessPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NegatedWhenUnlessPolicy {
    pub fail_on_violation: bool,
    pub conditional_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_conditional(
    view: &ExpressionView,
    path: &Path,
    conditional_form_count: &mut usize,
    violations: &mut Vec<NegatedWhenUnlessItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(suggested_head) = complement_head(head) else {
        return;
    };
    *conditional_form_count += 1;

    // children[0] is the head; children[1] is the test (if present).
    let Some(test) = view.children.get(1) else {
        return;
    };
    let Some(negator) = single_negation(test) else {
        return;
    };
    // single_negation guarantees the test is `(not X)`/`(null X)` with exactly
    // two children, so children[1] (the un-negated argument) exists.
    let inner_span = test.children[1].span;
    // `head` is a canonical when/unless (verified by complement_head), so the
    // stored form is the complement of the suggestion.
    let canonical_head = complement_head(suggested_head).expect("complement is involutive");
    violations.push(NegatedWhenUnlessItem {
        path: path.to_path_buf(),
        span: view.span,
        head_span: view.children[0].span,
        test_span: test.span,
        inner_span,
        head: canonical_head,
        negator,
        suggested_head,
    });
}

/// Collects every negated `when`/`unless` across a whole file, along with the
/// total number of `when`/`unless` forms scanned.
pub fn collect_negated_when_unless(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NegatedWhenUnlessItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut conditional_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_conditional(subview, path, &mut conditional_form_count, &mut violations);
        });
    }
    Ok((conditional_form_count, violations))
}

pub fn summarize_negated_when_unless(
    conditional_form_count: usize,
    violations: Vec<NegatedWhenUnlessItem>,
) -> NegatedWhenUnlessSummary {
    NegatedWhenUnlessSummary {
        conditional_form_count,
        violations,
    }
}

pub fn evaluate_negated_when_unless_policy(
    options: NegatedWhenUnlessPolicyOptions,
    summary: &NegatedWhenUnlessSummary,
) -> NegatedWhenUnlessPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NegatedWhenUnlessPolicy {
        fail_on_violation: options.fail_on_violation(),
        conditional_form_count: summary.conditional_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conditionals(input: &str) -> (usize, Vec<NegatedWhenUnlessItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_negated_when_unless(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect negated when/unless")
    }

    #[test]
    fn flags_when_with_a_not_test() {
        let (count, violations) = conditionals("(when (not ready) (go))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].negator, "not");
        assert_eq!(violations[0].suggested_head, "unless");
    }

    #[test]
    fn fix_spans_isolate_the_head_test_and_inner() {
        let input = "(when (not ready) (go))";
        let (_, violations) = conditionals(input);
        let item = &violations[0];
        assert_eq!(
            &input[item.head_span.start().get()..item.head_span.end().get()],
            "when"
        );
        assert_eq!(
            &input[item.test_span.start().get()..item.test_span.end().get()],
            "(not ready)"
        );
        assert_eq!(
            &input[item.inner_span.start().get()..item.inner_span.end().get()],
            "ready"
        );
    }

    #[test]
    fn flags_unless_with_a_not_test() {
        let (_, violations) = conditionals("(unless (not ready) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "unless");
        assert_eq!(violations[0].suggested_head, "when");
    }

    #[test]
    fn flags_a_null_test() {
        let (_, violations) = conditionals("(when (null lst) (init))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].negator, "null");
        assert_eq!(violations[0].suggested_head, "unless");
    }

    #[test]
    fn case_folds_heads_and_negators() {
        let (_, violations) = conditionals("(WHEN (NOT x) y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].negator, "not");
    }

    #[test]
    fn does_not_flag_a_plain_test() {
        let (count, violations) = conditionals("(when ready (go))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_nested_call_that_is_not_a_negation() {
        let (_, violations) = conditionals("(when (evenp n) (go))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_argument_not() {
        // (not a b) is malformed arity, not a single negation; leave it be.
        let (_, violations) = conditionals("(when (not a b) c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_zero_argument_not() {
        let (_, violations) = conditionals("(when (not) c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_if_or_cond() {
        let (count, violations) = conditionals("(if (not x) a b)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_a_nested_negation_test() {
        // (when (not (null x))) is still a single negation of one argument.
        let (_, violations) = conditionals("(when (not (null x)) y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "unless");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(when (not x) y)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_negated_when_unless(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect negated when/unless");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = conditionals("(when (not x) y)");
        let summary = summarize_negated_when_unless(count, items);

        let quiet = evaluate_negated_when_unless_policy(
            NegatedWhenUnlessPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_negated_when_unless_policy(
            NegatedWhenUnlessPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
