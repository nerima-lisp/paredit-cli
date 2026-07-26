//! Common Lisp if-to-`or` detection: a three-argument `if` whose test and
//! then-branch are the *same bare atom* — `(if x x y)`. Since evaluating an
//! atom (a variable, number, keyword, …) has no side effects and yields the
//! same value each time, `(if x x y)` returns `x` when `x` is non-nil and `y`
//! otherwise, which is exactly `(or x y)`. The `or` form states that intent
//! directly and evaluates `x` once instead of twice.
//!
//! Only an *atom* test-and-then pair is flagged. A compound test like
//! `(if (pop s) (pop s) y)` would evaluate its side effect twice, so `or` would
//! not preserve behavior; those are left alone. A literal `t`/`nil` test is left
//! alone as well — that shape belongs to the `constant-if-test` rule — as is a
//! reader-conditional operand (build-dependent arity).
//!
//! The fix rewrites `(if x x y)` as `(or x y)`, copying the test and else from
//! their exact source, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled arity.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// The atom text of `view` if it is a bare atom (no reader prefixes); `None` for
/// a compound form or a prefixed atom.
fn bare_atom(view: &ExpressionView) -> Option<&str> {
    if view.reader_prefixes.is_empty() {
        atom_text(view)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct IfToOrItem {
    pub path: PathBuf,
    /// The span of the whole `(if x x y)` form.
    pub span: ByteSpan,
    /// The span of the shared test/then atom `x`.
    pub test_span: ByteSpan,
    /// The span of the else branch `y`.
    pub else_span: ByteSpan,
}

#[derive(Debug)]
pub struct IfToOrSummary {
    pub if_form_count: usize,
    pub violations: Vec<IfToOrItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct IfToOrPolicyOptions {
    fail_on_violation: bool,
}

impl IfToOrPolicyOptions {
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
pub struct IfToOrPolicy {
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
    violations: &mut Vec<IfToOrItem>,
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
    let then = &view.children[2];
    let els = &view.children[3];

    // Test and then must be the same bare atom.
    let (Some(test_text), Some(then_text)) = (bare_atom(test), bare_atom(then)) else {
        return;
    };
    if test_text != then_text {
        return;
    }
    // Leave a literal t/nil test to constant-if-test, and skip reader
    // conditionals in the test or the else.
    if test_text.eq_ignore_ascii_case("t")
        || test_text.eq_ignore_ascii_case("nil")
        || test_text.starts_with("#+")
        || test_text.starts_with("#-")
        || is_reader_conditional(els)
    {
        return;
    }

    violations.push(IfToOrItem {
        path: path.to_path_buf(),
        span: view.span,
        test_span: test.span,
        else_span: els.span,
    });
}

/// Collects every `(if x x y)` (test == then, both bare atoms) across a whole
/// file, along with the total number of `if` forms scanned.
pub fn collect_if_to_ors(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<IfToOrItem>)> {
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
pub fn summarize_if_to_ors(if_form_count: usize, violations: Vec<IfToOrItem>) -> IfToOrSummary {
    IfToOrSummary {
        if_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_if_to_or_policy(
    options: IfToOrPolicyOptions,
    summary: &IfToOrSummary,
) -> IfToOrPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    IfToOrPolicy {
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

    fn ifs(input: &str) -> (usize, Vec<IfToOrItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_if_to_ors(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect if-to-or")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_if_x_x_y() {
        let source = "(if cached cached (compute))";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].test_span), "cached");
        assert_eq!(slice(source, violations[0].else_span), "(compute)");
    }

    #[test]
    fn does_not_flag_differing_test_and_then() {
        let (count, violations) = ifs("(if a b c)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_compound_test() {
        // (pop s) would be evaluated twice; not an or.
        let (_, violations) = ifs("(if (pop s) (pop s) y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_literal_t_or_nil() {
        // Those belong to constant-if-test.
        assert!(ifs("(if t t y)").1.is_empty());
        assert!(ifs("(if nil nil y)").1.is_empty());
    }

    #[test]
    fn does_not_flag_two_argument_if() {
        // (if x x) has no else; it is one-armed-if's concern.
        let (_, violations) = ifs("(if x x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = ifs("(IF x x y)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_if_to_or() {
        let (_, violations) = ifs("(defun f (x y) (if x x y))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(if x x y)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_if_to_ors(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect if-to-or");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ifs("(if x x y)");
        let summary = summarize_if_to_ors(count, items);

        let quiet = evaluate_if_to_or_policy(IfToOrPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_if_to_or_policy(IfToOrPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
