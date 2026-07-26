//! Common Lisp nested-`unless` detection: an `unless` whose *only* body form is
//! itself an `unless`. `unless` runs its body when the test is nil, so
//! `(unless a (unless b body…))` runs `body` exactly when both `a` and `b` are
//! nil — which is precisely when `(or a b)` is nil. Thus the nesting is exactly
//! `(unless (or a b) body…)`: same guard, same body, same result, one fewer
//! level of indentation.
//!
//! Only the tightly-nested shape is flagged: the outer `unless` must have
//! exactly one body form, and that form must be an `unless` with at least a
//! test. An outer `unless` with additional body forms after the inner `unless`
//! (`(unless a (unless b c) d)`) is left alone — `d` runs whenever `a` is nil,
//! regardless of `b`, so the merge would change its guard. A reader-conditional
//! test is left alone as well (build-dependent).
//!
//! The fix rewrites `(unless a (unless b body…))` as `(unless (or a b) body…)`,
//! copying both tests and the inner body from their exact source, so the rule is
//! auto-fixable. When the outer test is already an `or`, the resulting
//! `(or (or …) b)` is flattened by the `nested-boolean` rule on a later pass.
//!
//! This is the `or`-combining mirror of
//! [`crate::domain::nested_when_report`], which combines nested `when` tests
//! with `and`.
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

/// Whether `view` is an `(unless …)` form.
fn is_unless(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("unless"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// test containing one has no settled value.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedUnlessItem {
    pub path: PathBuf,
    /// The span of the whole outer `(unless a (unless b …))` form.
    pub span: ByteSpan,
    /// The span of the outer test `a`.
    pub outer_test_span: ByteSpan,
    /// The span of the inner test `b`.
    pub inner_test_span: ByteSpan,
    /// The span covering the inner `unless`'s body forms (`None` when it has
    /// none).
    pub inner_body_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct NestedUnlessSummary {
    pub unless_form_count: usize,
    pub violations: Vec<NestedUnlessItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NestedUnlessPolicyOptions {
    fail_on_violation: bool,
}

impl NestedUnlessPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NestedUnlessPolicy {
    pub fail_on_violation: bool,
    pub unless_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_unless(
    view: &ExpressionView,
    path: &Path,
    unless_form_count: &mut usize,
    violations: &mut Vec<NestedUnlessItem>,
) {
    if !is_unless(view) {
        return;
    }
    *unless_form_count += 1;

    // Outer must be exactly [unless, test, single-body-form].
    if view.children.len() != 3 {
        return;
    }
    let outer_test = &view.children[1];
    let inner = &view.children[2];
    if is_reader_conditional(outer_test) {
        return;
    }
    // The single body form must itself be an `unless` with at least a test.
    if !is_unless(inner) || inner.children.len() < 2 {
        return;
    }
    let inner_test = &inner.children[1];
    if is_reader_conditional(inner_test) {
        return;
    }

    // Inner body spans from the first body form through the last; `None` when
    // the inner `unless` is just a test.
    let inner_body_span = if inner.children.len() > 2 {
        Some(ByteSpan::new(
            inner.children[2].span.start(),
            inner.children[inner.children.len() - 1].span.end(),
        ))
    } else {
        None
    };

    violations.push(NestedUnlessItem {
        path: path.to_path_buf(),
        span: view.span,
        outer_test_span: outer_test.span,
        inner_test_span: inner_test.span,
        inner_body_span,
    });
}

/// Collects every `unless` whose sole body form is an `unless` across a whole
/// file, along with the total number of `unless` forms scanned.
pub fn collect_nested_unlesses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NestedUnlessItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut unless_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_unless(subview, path, &mut unless_form_count, &mut violations)
        });
    }
    Ok((unless_form_count, violations))
}

pub fn summarize_nested_unlesses(
    unless_form_count: usize,
    violations: Vec<NestedUnlessItem>,
) -> NestedUnlessSummary {
    NestedUnlessSummary {
        unless_form_count,
        violations,
    }
}

pub fn evaluate_nested_unless_policy(
    options: NestedUnlessPolicyOptions,
    summary: &NestedUnlessSummary,
) -> NestedUnlessPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NestedUnlessPolicy {
        fail_on_violation: options.fail_on_violation(),
        unless_form_count: summary.unless_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested(input: &str) -> (usize, Vec<NestedUnlessItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nested_unlesses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nested unlesses")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_unless_in_unless() {
        let source = "(unless a (unless b (do-it)))";
        let (count, violations) = nested(source);
        // Two unless forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_test_span), "a");
        assert_eq!(slice(source, violations[0].inner_test_span), "b");
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "(do-it)");
    }

    #[test]
    fn captures_multi_form_inner_body() {
        let source = "(unless a (unless b c d e))";
        let (_, violations) = nested(source);
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "c d e");
    }

    #[test]
    fn preserves_compound_tests() {
        let source = "(unless (done-p x) (unless (< n 0) (go)))";
        let (_, violations) = nested(source);
        assert_eq!(slice(source, violations[0].outer_test_span), "(done-p x)");
        assert_eq!(slice(source, violations[0].inner_test_span), "(< n 0)");
    }

    #[test]
    fn does_not_flag_extra_outer_body_form() {
        // d runs whenever a is nil, regardless of b; not mergeable.
        let (_, violations) = nested("(unless a (unless b c) d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_unless_body() {
        let (_, violations) = nested("(unless a (when b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_outer_unless() {
        let (_, violations) = nested("(unless a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_both_unlesses() {
        let (_, violations) = nested("(UNLESS a (Unless b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_unless_deeper_in_a_form() {
        let (_, violations) = nested("(defun f (a b) (unless a (unless b (g))))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(unless a (unless b c))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_nested_unlesses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nested unlesses");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = nested("(unless a (unless b c))");
        let summary = summarize_nested_unlesses(count, items);

        let quiet = evaluate_nested_unless_policy(NestedUnlessPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_nested_unless_policy(NestedUnlessPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
