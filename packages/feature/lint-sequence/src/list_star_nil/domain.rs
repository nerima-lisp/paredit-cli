//! Common Lisp `list*`-with-`nil`-tail detection: a `(list* a … nil)` whose
//! final argument is the literal `nil`. `list*` conses every argument but the
//! last onto that last argument as the tail; with a `nil` tail the result is a
//! proper list, exactly what `list` builds — `(list* a b nil)` is
//! `(cons a (cons b nil))` is `(list a b)`. The `list*`-against-`nil` spelling is
//! a spelled-out `list`.
//!
//! Only a bare `nil` final argument is matched, and only with at least one
//! preceding element (`(list* a nil)` → `(list a)`). A single-argument
//! `(list* nil)` (which is just `nil`) is
//! [`crate::single_operand_list_op::domain`]'s concern, a non-`nil` final
//! argument is a genuine `list*`, and a reader-conditional operand is left alone.
//!
//! The fix rewrites the head `list*` to `list` and deletes the trailing ` nil`,
//! leaving the kept elements byte-identical, so the rule is auto-fixable.
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

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct ListStarNilItem {
    pub path: PathBuf,
    /// The span of the whole `(list* …)` call form.
    pub span: ByteSpan,
    /// The span of the `list*` head token, rewritten to `list`.
    pub head_span: ByteSpan,
    /// The span to delete: the trailing ` nil` tail argument.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct ListStarNilSummary {
    pub call_form_count: usize,
    pub violations: Vec<ListStarNilItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ListStarNilPolicyOptions {
    fail_on_violation: bool,
}

impl ListStarNilPolicyOptions {
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
pub struct ListStarNilPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<ListStarNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("list*") {
        return;
    }
    *call_form_count += 1;

    // children: [list*, elem, …, nil] — at least one element before the tail.
    if view.children.len() < 3 {
        return;
    }
    let last = &view.children[view.children.len() - 1];
    if !is_nil_literal(last) {
        return;
    }
    let prev = &view.children[view.children.len() - 2];
    let removal_span = ByteSpan::new(prev.span.end(), last.span.end());
    violations.push(ListStarNilItem {
        path: path.to_path_buf(),
        span: view.span,
        head_span: view.children[0].span,
        removal_span,
    });
}

/// Collects every `(list* a … nil)` across a whole file, along with the total
/// number of `list*` calls scanned.
pub fn collect_list_star_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<ListStarNilItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut call_form_count, &mut violations);
        });
    }
    Ok((call_form_count, violations))
}

#[must_use]
pub const fn summarize_list_star_nils(
    call_form_count: usize,
    violations: Vec<ListStarNilItem>,
) -> ListStarNilSummary {
    ListStarNilSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_list_star_nil_policy(
    options: ListStarNilPolicyOptions,
    summary: &ListStarNilSummary,
) -> ListStarNilPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ListStarNilPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_form_count: summary.call_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<ListStarNilItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_list_star_nils(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect list* nils")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_two_elements_and_nil() {
        let source = "(list* a b nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].head_span), "list*");
        assert_eq!(slice(source, violations[0].removal_span), " nil");
    }

    #[test]
    fn flags_one_element_and_nil() {
        let (_, violations) = calls("(list* a nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_non_nil_tail() {
        // (list* a b) is a genuine two-argument list* — cons-to-list's dual.
        assert!(calls("(list* a b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_single_nil() {
        // (list* nil) is just nil — single-operand-list-op's concern.
        assert!(calls("(list* nil)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(LIST* a b nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(apply #'f (list* a b nil))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(list* a b nil)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_list_star_nils(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect list* nils");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(list* a b nil)");
        let summary = summarize_list_star_nils(count, items);

        let quiet = evaluate_list_star_nil_policy(ListStarNilPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_list_star_nil_policy(ListStarNilPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
