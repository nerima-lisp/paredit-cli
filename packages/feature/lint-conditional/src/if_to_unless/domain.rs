//! Common Lisp `if`-to-`unless` detection: a three-argument `(if c nil e)` whose
//! then-branch is the literal `nil`. Such a form yields `nil` when `c` holds and
//! `e` otherwise, which is exactly `(unless c e)` — same test (evaluated once),
//! same else value(s), same nil-on-true result. `unless` states "run this only
//! when the test fails" directly, without a dead `nil` then-branch.
//!
//! To avoid overlapping sibling rules, several else-branch shapes are left alone:
//!
//!   - `else = t` is [`crate::if_not::domain`] (`(if c nil t)` is
//!     `(not c)`), a cleaner rewrite;
//!   - `else = nil` is [`crate::identical_if_branches::domain`]
//!     (`(if c nil nil)`);
//!   - a literal `t`/`nil` *test* is [`crate::constant_if_test::domain`].
//!
//! A reader-conditional test or else operand is also left alone.
//!
//! The fix rewrites `(if c nil e)` as `(unless c e)`, copying the test and else
//! source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare literal atom `expected` (`nil`/`t`), no prefixes.
fn is_bare_literal(view: &ExpressionView, expected: &str) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(expected))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct IfToUnlessItem {
    pub path: PathBuf,
    /// The span of the whole `(if c nil e)` form.
    pub span: ByteSpan,
    /// The span of the test `c`.
    pub test_span: ByteSpan,
    /// The span of the else branch `e`.
    pub else_span: ByteSpan,
}

#[derive(Debug)]
pub struct IfToUnlessSummary {
    pub if_form_count: usize,
    pub violations: Vec<IfToUnlessItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct IfToUnlessPolicyOptions {
    fail_on_violation: bool,
}

impl IfToUnlessPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct IfToUnlessPolicy {
    pub fail_on_violation: bool,
    pub if_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    if_form_count: &mut usize,
    violations: &mut Vec<IfToUnlessItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then, else] — the three-argument shape.
    if view.children.len() != 4 {
        return;
    }
    let test = &view.children[1];
    let then_branch = &view.children[2];
    let else_branch = &view.children[3];

    // then must be the literal nil.
    if !is_bare_literal(then_branch, "nil") {
        return;
    }
    // else = t is if-not's job; else = nil is identical-if-branches'.
    if is_bare_literal(else_branch, "t") || is_bare_literal(else_branch, "nil") {
        return;
    }
    // a constant t/nil test is constant-if-test's job.
    if is_bare_literal(test, "t") || is_bare_literal(test, "nil") {
        return;
    }
    if is_reader_conditional(test) || is_reader_conditional(else_branch) {
        return;
    }

    violations.push(IfToUnlessItem {
        path: path.to_path_buf(),
        span: view.span,
        test_span: test.span,
        else_span: else_branch.span,
    });
}

/// Collects every `(if c nil e)` rewritable to `unless` across a whole file,
/// along with the total number of `if` forms scanned.
pub fn collect_if_to_unless(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<IfToUnlessItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut if_form_count, &mut violations);
        });
    }
    Ok((if_form_count, violations))
}

#[must_use]
pub const fn summarize_if_to_unless(
    if_form_count: usize,
    violations: Vec<IfToUnlessItem>,
) -> IfToUnlessSummary {
    IfToUnlessSummary {
        if_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_if_to_unless_policy(
    options: IfToUnlessPolicyOptions,
    summary: &IfToUnlessSummary,
) -> IfToUnlessPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    IfToUnlessPolicy {
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

    fn ifs(input: &str) -> (usize, Vec<IfToUnlessItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_if_to_unless(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect if to unless")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_if_nil_else() {
        let source = "(if ready nil (do-work))";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].test_span), "ready");
        assert_eq!(slice(source, violations[0].else_span), "(do-work)");
    }

    #[test]
    fn does_not_flag_else_t() {
        // (if c nil t) is if-not's concern.
        assert!(ifs("(if c nil t)").1.is_empty());
    }

    #[test]
    fn does_not_flag_else_nil() {
        // (if c nil nil) is identical-if-branches' concern.
        assert!(ifs("(if c nil nil)").1.is_empty());
    }

    #[test]
    fn does_not_flag_constant_test() {
        assert!(ifs("(if t nil e)").1.is_empty());
        assert!(ifs("(if nil nil e)").1.is_empty());
    }

    #[test]
    fn does_not_flag_non_nil_then() {
        assert!(ifs("(if c x e)").1.is_empty());
    }

    #[test]
    fn does_not_flag_two_armed_if() {
        assert!(ifs("(if c nil)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = ifs("(IF c NIL e)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = ifs("(defun f (c e) (if c nil e))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(if c nil e)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_if_to_unless(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect if to unless");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ifs("(if c nil e)");
        let summary = summarize_if_to_unless(count, items);

        let quiet = evaluate_if_to_unless_policy(IfToUnlessPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_if_to_unless_policy(IfToUnlessPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
