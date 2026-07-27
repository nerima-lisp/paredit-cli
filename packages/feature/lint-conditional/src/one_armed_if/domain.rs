//! Common Lisp one-armed-`if` detection: a two-argument `(if TEST THEN)` with
//! no else branch. A one-armed `if` and `(when TEST THEN)` are exactly
//! equivalent — both evaluate `THEN` when `TEST` is true and yield `nil`
//! otherwise — and every mainstream Common Lisp style guide (Google, Norvig's
//! PAIP, the SBCL sources) recommends `when`/`unless` over an `if` with a
//! missing branch, because `when` states "conditionally do this" without
//! implying a two-way choice and admits an implicit multi-form body.
//!
//! Only the exact two-argument shape is flagged:
//!
//!   - `(if test then else)` (a real two-way branch) is never flagged.
//!   - `(if test)` (a malformed, argument-short `if`) is left to `if-arity`.
//!   - A form with a reader-conditional operand (`#+`/`#-`) is exempt: its
//!     effective argument count is build-dependent, so "has no else" cannot be
//!     decided statically.
//!
//! The fix swaps the `if` head for `when` in place — a single-token edit that
//! leaves the test and then-branch byte-identical. When the then-branch is a
//! `(progn …)`, the follow-on `redundant-body-progn` fix splices it during the
//! same fixpoint pass, so `(if c (progn a b))` converges to `(when c a b)`.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so it does not count as one settled operand. Mirrors
/// the guard used by the progn/boolean lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct OneArmedIfItem {
    pub path: PathBuf,
    /// The span of the whole `(if TEST THEN)` form.
    pub span: ByteSpan,
    /// The span of the `if` head symbol (lets a fix swap it for `when`).
    pub head_span: ByteSpan,
}

#[derive(Debug)]
pub struct OneArmedIfSummary {
    pub if_form_count: usize,
    pub violations: Vec<OneArmedIfItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct OneArmedIfPolicyOptions {
    fail_on_violation: bool,
}

impl OneArmedIfPolicyOptions {
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
pub struct OneArmedIfPolicy {
    pub fail_on_violation: bool,
    pub if_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    path: &Path,
    if_form_count: &mut usize,
    violations: &mut Vec<OneArmedIfItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then]. A one-armed if is exactly three children.
    if view.children.len() != 3 {
        return;
    }
    // Skip when any operand is a reader conditional: the true arity is
    // build-dependent, so "no else branch" cannot be decided statically.
    if view.children[1..].iter().any(is_reader_conditional) {
        return;
    }

    violations.push(OneArmedIfItem {
        path: path.to_path_buf(),
        span: view.span,
        head_span: view.children[0].span,
    });
}

/// Collects every one-armed `if` across a whole file, along with the total
/// number of `if` forms scanned.
pub fn collect_one_armed_ifs(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<OneArmedIfItem>)> {
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
pub const fn summarize_one_armed_ifs(
    if_form_count: usize,
    violations: Vec<OneArmedIfItem>,
) -> OneArmedIfSummary {
    OneArmedIfSummary {
        if_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_one_armed_if_policy(
    options: OneArmedIfPolicyOptions,
    summary: &OneArmedIfSummary,
) -> OneArmedIfPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    OneArmedIfPolicy {
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

    fn ifs(input: &str) -> (usize, Vec<OneArmedIfItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_one_armed_ifs(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect one-armed ifs")
    }

    #[test]
    fn flags_two_argument_if() {
        let (count, violations) = ifs("(if ready (go))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn head_span_covers_only_the_if_symbol() {
        let source = "(if ready (go))";
        let (_, violations) = ifs(source);
        let head = violations[0].head_span;
        assert_eq!(source.get(head.start().get()..head.end().get()), Some("if"));
    }

    #[test]
    fn does_not_flag_two_armed_if() {
        let (count, violations) = ifs("(if test a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_argument_short_if() {
        // (if test) is malformed; leave it to if-arity.
        let (_, violations) = ifs("(if test)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_operand() {
        let (_, violations) = ifs("(if #+sbcl a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = ifs("(IF ready (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = ifs("(when ready (go))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_one_armed_if() {
        let (_, violations) = ifs("(defun f () (if ready (go)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(if ready (go))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_one_armed_ifs(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect one-armed ifs");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ifs("(if ready (go))");
        let summary = summarize_one_armed_ifs(count, items);

        let quiet = evaluate_one_armed_if_policy(OneArmedIfPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_one_armed_if_policy(OneArmedIfPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
