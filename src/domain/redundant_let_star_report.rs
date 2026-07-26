//! Common Lisp redundant-`let*` detection: a `let*` whose binding list holds
//! zero or one binding. `let*` differs from `let` only in that each binding's
//! init form is evaluated in the scope of the *earlier* bindings; with no
//! earlier binding to see, that difference vanishes. So `(let* ((x e)) body)`
//! is exactly `(let ((x e)) body)` and `(let* () body)` is `(let () body)` —
//! same evaluation order, same scope, same result. The plain `let` states "no
//! binding depends on another" directly, and readers no longer have to check.
//!
//! Only the zero- and one-binding shapes are flagged; a `let*` with two or more
//! bindings may genuinely rely on the sequential scope, so it is left alone. A
//! binding list that is not a parenthesized list (e.g. the atom `nil`, or a
//! reader-conditional operand) is left alone as well, since its settled binding
//! count is not statically knowable here.
//!
//! The fix rewrites just the `let*` head symbol to `let`, leaving the binding
//! list, body, spacing, and comments byte-identical, so the rule is
//! auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// binding list containing one has no settled binding count.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantLetStarItem {
    pub path: PathBuf,
    /// The span of the whole `(let* … …)` form.
    pub span: ByteSpan,
    /// The span of the `let*` head symbol (for the head-only rewrite to `let`).
    pub head_span: ByteSpan,
    /// The number of bindings (0 or 1) that made the `let*` redundant.
    pub binding_count: usize,
}

#[derive(Debug)]
pub struct RedundantLetStarSummary {
    pub let_star_form_count: usize,
    pub violations: Vec<RedundantLetStarItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantLetStarPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantLetStarPolicyOptions {
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
pub struct RedundantLetStarPolicy {
    pub fail_on_violation: bool,
    pub let_star_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_let_star(
    view: &ExpressionView,
    path: &Path,
    let_star_form_count: &mut usize,
    violations: &mut Vec<RedundantLetStarItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("let*") {
        return;
    }
    *let_star_form_count += 1;

    // children: [let*, binding-list, body…] — need at least the binding list.
    if view.children.len() < 2 {
        return;
    }
    let binding_list = &view.children[1];
    // Only a parenthesized binding list has a statically knowable count; a bare
    // `nil` or a macro-expanded operand is left alone.
    if !is_paren_list(binding_list) {
        return;
    }
    // A reader-conditional binding makes the true count build-dependent.
    if binding_list.children.iter().any(is_reader_conditional) {
        return;
    }
    let binding_count = binding_list.children.len();
    if binding_count > 1 {
        return;
    }

    violations.push(RedundantLetStarItem {
        path: path.to_path_buf(),
        span: view.span,
        head_span: view.children[0].span,
        binding_count,
    });
}

/// Collects every redundant `let*` (≤ 1 binding) across a whole file, along
/// with the total number of `let*` forms scanned.
pub fn collect_redundant_let_stars(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantLetStarItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut let_star_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let_star(subview, path, &mut let_star_form_count, &mut violations);
        });
    }
    Ok((let_star_form_count, violations))
}

#[must_use]
pub fn summarize_redundant_let_stars(
    let_star_form_count: usize,
    violations: Vec<RedundantLetStarItem>,
) -> RedundantLetStarSummary {
    RedundantLetStarSummary {
        let_star_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_let_star_policy(
    options: RedundantLetStarPolicyOptions,
    summary: &RedundantLetStarSummary,
) -> RedundantLetStarPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantLetStarPolicy {
        fail_on_violation: options.fail_on_violation(),
        let_star_form_count: summary.let_star_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn let_stars(input: &str) -> (usize, Vec<RedundantLetStarItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_let_stars(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant let*")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_binding() {
        let source = "(let* ((x 1)) (+ x x))";
        let (count, violations) = let_stars(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].binding_count, 1);
        assert_eq!(slice(source, violations[0].head_span), "let*");
    }

    #[test]
    fn flags_zero_bindings() {
        let (_, violations) = let_stars("(let* () (side-effect))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].binding_count, 0);
    }

    #[test]
    fn flags_single_bare_symbol_binding() {
        // `(let* (x) ...)` binds x to nil — one binding, still redundant.
        let (_, violations) = let_stars("(let* (x) x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].binding_count, 1);
    }

    #[test]
    fn does_not_flag_two_bindings() {
        let (count, violations) = let_stars("(let* ((x 1) (y (* x 2))) y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_plain_let() {
        let (count, violations) = let_stars("(let ((x 1)) x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_nil_binding_list() {
        // A bare `nil` binding list is not a paren list; leave it alone.
        let (_, violations) = let_stars("(let* nil (foo))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_reader_conditional_binding() {
        let (_, violations) = let_stars("(let* (#+sbcl (x 1)) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = let_stars("(LET* ((x 1)) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_redundant_let_star() {
        let (_, violations) = let_stars("(defun f () (let* ((x 1)) x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(let* ((x 1)) x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_let_stars(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant let*");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = let_stars("(let* ((x 1)) x)");
        let summary = summarize_redundant_let_stars(count, items);

        let quiet =
            evaluate_redundant_let_star_policy(RedundantLetStarPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_let_star_policy(RedundantLetStarPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
