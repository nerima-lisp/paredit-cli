//! Common Lisp `if-not` detection: a three-argument `if` whose then-branch is
//! the literal `nil` and whose else-branch is the literal `t` — `(if test nil t)`.
//! Such a form yields `nil` when `test` is true and `t` when it is false, which
//! is exactly `(not test)`. Because `if` consults only the primary value of
//! `test` for the branch decision and `not` does the same, and `test` sits in
//! evaluation position either way, the rewrite is exact for any `test` (no
//! double-evaluation and no lost side effects).
//!
//! Only the exact `then = nil`, `else = t` literal shape is matched. The dual
//! `(if test t nil)` is a boolean *coercion* with no single clearer builtin and
//! is left alone, as is `(if test nil nil)` (both branches equal — that is
//! `identical-if-branches`' concern) and any form with a non-literal branch. A
//! reader-conditional branch cannot be the bare `nil`/`t` atom, so none is ever
//! matched.
//!
//! The fix replaces the whole form with `(not TEST)`, copying the test's exact
//! source, so the rule is auto-fixable.
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

/// Whether `view` is the bare literal atom `expected` (`nil` or `t`), with no
/// reader prefixes.
fn is_bare_literal(view: &ExpressionView, expected: &str) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(expected))
}

#[derive(Debug, Clone)]
pub struct IfNotItem {
    pub path: PathBuf,
    /// The span of the whole `(if test nil t)` form.
    pub span: ByteSpan,
    /// The span of the test, copied verbatim into `(not …)`.
    pub test_span: ByteSpan,
}

#[derive(Debug)]
pub struct IfNotSummary {
    pub if_form_count: usize,
    pub violations: Vec<IfNotItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct IfNotPolicyOptions {
    fail_on_violation: bool,
}

impl IfNotPolicyOptions {
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
pub struct IfNotPolicy {
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
    violations: &mut Vec<IfNotItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then, else] — require the full three-argument shape.
    if view.children.len() != 4 {
        return;
    }
    let then_branch = &view.children[2];
    let else_branch = &view.children[3];
    if !is_bare_literal(then_branch, "nil") || !is_bare_literal(else_branch, "t") {
        return;
    }

    violations.push(IfNotItem {
        path: path.to_path_buf(),
        span: view.span,
        test_span: view.children[1].span,
    });
}

/// Collects every `(if test nil t)` across a whole file, along with the total
/// number of `if` forms scanned.
pub fn collect_if_nots(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<IfNotItem>)> {
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
pub fn summarize_if_nots(if_form_count: usize, violations: Vec<IfNotItem>) -> IfNotSummary {
    IfNotSummary {
        if_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_if_not_policy(options: IfNotPolicyOptions, summary: &IfNotSummary) -> IfNotPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    IfNotPolicy {
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

    fn ifs(input: &str) -> (usize, Vec<IfNotItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_if_nots(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect if-not")
    }

    fn test_src<'a>(source: &'a str, item: &IfNotItem) -> &'a str {
        &source[item.test_span.start().get()..item.test_span.end().get()]
    }

    #[test]
    fn flags_if_nil_t() {
        let source = "(if ready nil t)";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(test_src(source, &violations[0]), "ready");
    }

    #[test]
    fn preserves_a_compound_test() {
        let source = "(if (member x xs) nil t)";
        let (_, violations) = ifs(source);
        assert_eq!(test_src(source, &violations[0]), "(member x xs)");
    }

    #[test]
    fn does_not_flag_the_boolean_coercion_direction() {
        // (if test t nil) is a coercion with no clearer single builtin.
        let (_, violations) = ifs("(if test t nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equal_branches() {
        // (if test nil nil) is identical-if-branches' concern.
        let (_, violations) = ifs("(if test nil nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_literal_else() {
        let (_, violations) = ifs("(if test nil other)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_two_armed_if() {
        let (count, violations) = ifs("(if test nil)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_literals() {
        let (_, violations) = ifs("(IF test NIL T)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_if_not() {
        let (_, violations) = ifs("(defun f (x) (if x nil t))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(if x nil t)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_if_nots(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect if-not");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ifs("(if x nil t)");
        let summary = summarize_if_nots(count, items);

        let quiet = evaluate_if_not_policy(IfNotPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_if_not_policy(IfNotPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
