//! Common Lisp redundant-`if`-`nil`-else detection: a three-argument `if` whose
//! else branch is a bare `nil` — `(if test then nil)`. A two-argument `if`
//! already yields `nil` when the test is false, so the explicit `nil` else adds
//! nothing: `(if test then nil)` is exactly `(if test then)`. Dropping it is a
//! provably behavior-preserving cleanup.
//!
//! Only a *bare* `nil` else is flagged, and only when the then branch is not
//! itself `nil` (a `(if c nil nil)` is the identical-if-branches rule's
//! territory). A quoted `'nil`, a `()` list, or any non-`nil` else is left
//! alone.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare atom `nil`.
fn is_bare_nil(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct RedundantIfNilItem {
    pub path: PathBuf,
    /// The span of the whole `(if …)` form.
    pub span: ByteSpan,
    /// The span from the end of the then branch to the end of the `nil` else —
    /// the ` nil` a fix deletes to leave `(if test then)`.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct RedundantIfNilSummary {
    pub if_form_count: usize,
    pub violations: Vec<RedundantIfNilItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantIfNilPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantIfNilPolicyOptions {
    #[must_use]
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantIfNilPolicy {
    pub fail_on_violation: bool,
    pub if_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_if(
    view: &ExpressionView,
    path: &Path,
    if_form_count: &mut usize,
    violations: &mut Vec<RedundantIfNilItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("if")) {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then, else]. Only a bare-nil else with a non-nil
    // then is a redundant-nil-else (the nil/nil case is identical-if-branches).
    if view.children.len() != 4 {
        return;
    }
    let then_branch = &view.children[2];
    let else_branch = &view.children[3];
    if is_bare_nil(else_branch) && !is_bare_nil(then_branch) {
        violations.push(RedundantIfNilItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span: ByteSpan::new(then_branch.span.end(), else_branch.span.end()),
        });
    }
}

/// Collects every `if` with a redundant `nil` else across a whole file, along
/// with the total number of `if` forms scanned.
pub fn collect_redundant_if_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantIfNilItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, path, &mut if_form_count, &mut violations);
        });
    }
    Ok((if_form_count, violations))
}

#[must_use]
pub fn summarize_redundant_if_nils(
    if_form_count: usize,
    violations: Vec<RedundantIfNilItem>,
) -> RedundantIfNilSummary {
    RedundantIfNilSummary {
        if_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_if_nil_policy(
    options: RedundantIfNilPolicyOptions,
    summary: &RedundantIfNilSummary,
) -> RedundantIfNilPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantIfNilPolicy {
        fail_on_violation: options.fail_on_violation(),
        if_form_count: summary.if_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ifs(input: &str) -> (usize, Vec<RedundantIfNilItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_if_nils(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant if nils")
    }

    #[test]
    fn flags_a_nil_else() {
        let (count, violations) = ifs("(if ready (go) nil)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn removal_span_covers_the_space_and_nil() {
        let input = "(if ready (go) nil)";
        let (_, violations) = ifs(input);
        let removal = violations[0].removal_span;
        assert_eq!(&input[removal.start().get()..removal.end().get()], " nil");
    }

    #[test]
    fn case_folds_the_head_and_nil() {
        let (_, violations) = ifs("(IF c x NIL)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_two_argument_if() {
        let (count, violations) = ifs("(if c x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_nil_else() {
        let (_, violations) = ifs("(if c x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_when_then_is_also_nil() {
        // (if c nil nil) is the identical-if-branches rule's territory.
        let (_, violations) = ifs("(if c nil nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_nil_else() {
        let (_, violations) = ifs("(if c x 'nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_nil_then_branch() {
        // Only the else nil is redundant; a nil then with a real else stays.
        let (_, violations) = ifs("(if c nil y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_if() {
        let (_, violations) = ifs("(defun f (c x) (list (if c x nil)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(if c x nil)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_if_nils(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant if nils");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ifs("(if c x nil)");
        let summary = summarize_redundant_if_nils(count, items);

        let quiet =
            evaluate_redundant_if_nil_policy(RedundantIfNilPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_if_nil_policy(RedundantIfNilPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
