//! Common Lisp nested-`when` detection: a `when` whose *only* body form is
//! itself a `when`. `(when a (when b body…))` runs `body` exactly when both `a`
//! and `b` are true, which is precisely `(when (and a b) body…)` — same guard,
//! same body, same result. Collapsing the two tests into one `and` removes a
//! level of indentation and states the combined condition directly.
//!
//! Only the tightly-nested shape is flagged: the outer `when` must have exactly
//! one body form, and that form must be a `when` with at least a test. An outer
//! `when` with additional body forms after the inner `when`
//! (`(when a (when b c) d)`) is left alone — `d` runs whenever `a` holds,
//! regardless of `b`, so the merge would change its guard. A reader-conditional
//! test is left alone as well (build-dependent).
//!
//! The fix rewrites `(when a (when b body…))` as `(when (and a b) body…)`,
//! copying both tests and the inner body from their exact source, so the rule is
//! auto-fixable. When the outer test is already an `and`, the resulting
//! `(and (and …) b)` is flattened by the `nested-boolean` rule on a later pass.
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

/// Whether `view` is a `(when …)` form.
fn is_when(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("when"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// test containing one has no settled value.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedWhenItem {
    pub path: PathBuf,
    /// The span of the whole outer `(when a (when b …))` form.
    pub span: ByteSpan,
    /// The span of the outer test `a`.
    pub outer_test_span: ByteSpan,
    /// The span of the inner test `b`.
    pub inner_test_span: ByteSpan,
    /// The span covering the inner `when`'s body forms (`None` when it has none).
    pub inner_body_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct NestedWhenSummary {
    pub when_form_count: usize,
    pub violations: Vec<NestedWhenItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NestedWhenPolicyOptions {
    fail_on_violation: bool,
}

impl NestedWhenPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NestedWhenPolicy {
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
    violations: &mut Vec<NestedWhenItem>,
) {
    if !is_when(view) {
        return;
    }
    *when_form_count += 1;

    // Outer must be exactly [when, test, single-body-form].
    if view.children.len() != 3 {
        return;
    }
    let outer_test = &view.children[1];
    let inner = &view.children[2];
    if is_reader_conditional(outer_test) {
        return;
    }
    // The single body form must itself be a `when` with at least a test.
    if !is_when(inner) || inner.children.len() < 2 {
        return;
    }
    let inner_test = &inner.children[1];
    if is_reader_conditional(inner_test) {
        return;
    }

    // Inner body spans from the first body form through the last; `None` when
    // the inner `when` is just a test.
    let inner_body_span = if inner.children.len() > 2 {
        Some(ByteSpan::new(
            inner.children[2].span.start(),
            inner.children[inner.children.len() - 1].span.end(),
        ))
    } else {
        None
    };

    violations.push(NestedWhenItem {
        path: path.to_path_buf(),
        span: view.span,
        outer_test_span: outer_test.span,
        inner_test_span: inner_test.span,
        inner_body_span,
    });
}

/// Collects every `when` whose sole body form is a `when` across a whole file,
/// along with the total number of `when` forms scanned.
pub fn collect_nested_whens(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NestedWhenItem>)> {
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

pub fn summarize_nested_whens(
    when_form_count: usize,
    violations: Vec<NestedWhenItem>,
) -> NestedWhenSummary {
    NestedWhenSummary {
        when_form_count,
        violations,
    }
}

pub fn evaluate_nested_when_policy(
    options: NestedWhenPolicyOptions,
    summary: &NestedWhenSummary,
) -> NestedWhenPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NestedWhenPolicy {
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

    fn nested(input: &str) -> (usize, Vec<NestedWhenItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nested_whens(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nested whens")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_when_in_when() {
        let source = "(when a (when b (do-it)))";
        let (count, violations) = nested(source);
        // Two when forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_test_span), "a");
        assert_eq!(slice(source, violations[0].inner_test_span), "b");
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "(do-it)");
    }

    #[test]
    fn captures_multi_form_inner_body() {
        let source = "(when a (when b c d e))";
        let (_, violations) = nested(source);
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "c d e");
    }

    #[test]
    fn preserves_compound_tests() {
        let source = "(when (ready-p x) (when (> n 0) (go)))";
        let (_, violations) = nested(source);
        assert_eq!(slice(source, violations[0].outer_test_span), "(ready-p x)");
        assert_eq!(slice(source, violations[0].inner_test_span), "(> n 0)");
    }

    #[test]
    fn does_not_flag_extra_outer_body_form() {
        // d runs whenever a holds, regardless of b; not mergeable.
        let (_, violations) = nested("(when a (when b c) d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_when_body() {
        let (_, violations) = nested("(when a (unless b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_outer_when() {
        // No body at all.
        let (_, violations) = nested("(when a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_both_whens() {
        let (_, violations) = nested("(WHEN a (When b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_when_deeper_in_a_form() {
        let (_, violations) = nested("(defun f (a b) (when a (when b (g))))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(when a (when b c))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_nested_whens(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nested whens");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = nested("(when a (when b c))");
        let summary = summarize_nested_whens(count, items);

        let quiet = evaluate_nested_when_policy(NestedWhenPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_nested_when_policy(NestedWhenPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
