//! Common Lisp nested-`progn` detection: an explicit `(progn …)` that appears
//! as a body form directly inside another `progn`. Because `progn` evaluates
//! its body in order and a nested progn's own result is spliced into that
//! sequence, wrapping body forms in an inner progn changes nothing:
//! `(progn a (progn b c) d)` is exactly `(progn a b c d)`. The nesting is pure
//! structure noise — common after mechanical macro expansion or code motion.
//!
//! This rule is the multi-form companion to
//! [`crate::domain::redundant_progn_report`]: that rule owns the 0-form and
//! 1-form progns (which are redundant on their own, in any position), while this
//! rule owns progns with two or more body forms that are redundant *because* of
//! where they sit. The two never flag the same span, so `inspect lint` reports
//! each redundant progn once.
//!
//! Splicing is semantics-preserving regardless of reader conditionals or the
//! inner body's contents, so no `#+`/`#-` guard is needed here.
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

/// Whether `view` is a `(progn …)` form.
fn is_progn(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("progn"))
}

#[derive(Debug, Clone)]
pub struct NestedPrognItem {
    pub path: PathBuf,
    /// The span of the inner (nested) progn.
    pub span: ByteSpan,
    /// The span covering just the inner progn's body forms (first form start to
    /// last form end), so a fix can splice that source in place of the wrapper.
    pub body_span: ByteSpan,
    /// The inner progn's body form count (always >= 2 here; the 0/1 cases
    /// belong to the redundant-progn rule).
    pub body_form_count: usize,
}

#[derive(Debug)]
pub struct NestedPrognSummary {
    pub progn_form_count: usize,
    pub violations: Vec<NestedPrognItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NestedPrognPolicyOptions {
    fail_on_violation: bool,
}

impl NestedPrognPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NestedPrognPolicy {
    pub fail_on_violation: bool,
    pub progn_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_progn(
    view: &ExpressionView,
    path: &Path,
    progn_form_count: &mut usize,
    violations: &mut Vec<NestedPrognItem>,
) {
    if !is_progn(view) {
        return;
    }
    *progn_form_count += 1;

    // children[0] is the `progn` head; the rest is the body. Any body form that
    // is itself a multi-form progn splices redundantly into this one.
    for child in &view.children[1..] {
        if !is_progn(child) {
            continue;
        }
        let body = &child.children[1..];
        let inner_body_form_count = body.len();
        if inner_body_form_count >= 2 {
            // Body span runs from the first body form's start to the last's end.
            let body_span = ByteSpan::new(
                body[0].span.start(),
                body[inner_body_form_count - 1].span.end(),
            );
            violations.push(NestedPrognItem {
                path: path.to_path_buf(),
                span: child.span,
                body_span,
                body_form_count: inner_body_form_count,
            });
        }
    }
}

/// Collects every progn nested directly inside another progn across a whole
/// file, along with the total number of progn forms scanned.
pub fn collect_nested_progns(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NestedPrognItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut progn_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_progn(subview, path, &mut progn_form_count, &mut violations);
        });
    }
    Ok((progn_form_count, violations))
}

pub fn summarize_nested_progns(
    progn_form_count: usize,
    violations: Vec<NestedPrognItem>,
) -> NestedPrognSummary {
    NestedPrognSummary {
        progn_form_count,
        violations,
    }
}

pub fn evaluate_nested_progn_policy(
    options: NestedPrognPolicyOptions,
    summary: &NestedPrognSummary,
) -> NestedPrognPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NestedPrognPolicy {
        fail_on_violation: options.fail_on_violation(),
        progn_form_count: summary.progn_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested(input: &str) -> (usize, Vec<NestedPrognItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nested_progns(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nested progns")
    }

    #[test]
    fn flags_a_multi_form_progn_nested_in_progn() {
        let (count, violations) = nested("(progn a (progn b c) d)");
        // Two progn forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 2);
    }

    #[test]
    fn body_span_isolates_the_inner_body_forms() {
        let input = "(progn a (progn b c) d)";
        let (_, violations) = nested(input);
        let body = violations[0].body_span;
        assert_eq!(
            &input[body.start().get()..body.end().get()],
            "b c",
            "body span must cover just the inner progn's body, not its parens"
        );
    }

    #[test]
    fn flags_a_trailing_nested_progn() {
        let (_, violations) = nested("(progn a (progn b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_multiple_nested_progns() {
        let (_, violations) = nested("(progn (progn a b) (progn c d))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn case_folds_both_progns() {
        let (_, violations) = nested("(PROGN x (PROGN y z))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_single_form_inner_progn() {
        // (progn x) is the redundant-progn rule's job, not this one.
        let (_, violations) = nested("(progn a (progn x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_inner_progn() {
        let (_, violations) = nested("(progn a (progn))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_not_inside_a_progn() {
        // A multi-form progn in a value slot (an if branch) is meaningful.
        let (_, violations) = nested("(if c (progn a b) d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_outer_progn_itself() {
        let (_, violations) = nested("(progn (foo) (bar))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_deeply_nested_progns() {
        // Inner-most is nested in the middle progn; middle is nested in outer.
        let (_, violations) = nested("(progn (progn (progn a b) c))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(progn a (progn b c))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_nested_progns(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nested progns");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = nested("(progn a (progn b c))");
        let summary = summarize_nested_progns(count, items);

        let quiet = evaluate_nested_progn_policy(NestedPrognPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_nested_progn_policy(NestedPrognPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
