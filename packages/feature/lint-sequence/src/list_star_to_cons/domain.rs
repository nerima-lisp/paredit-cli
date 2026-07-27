//! Common Lisp `list*`-to-`cons` detection: a two-argument `(list* a b)`. By
//! definition `list*` of exactly two arguments builds a single cons of the first
//! onto the second — `(list* a b)` is exactly `(cons a b)`. Same one fresh cons,
//! same sharing of `b`, `a` and `b` each evaluated once in the same order; the
//! plain `cons` states the intent directly.
//!
//! Only the exact two-argument shape is matched. A single-argument `(list* x)`
//! (which is just `x`) is [`crate::single_operand_list_op::domain`]'s
//! concern, and a three-or-more-argument `list*` is a genuine `list*` (nested
//! conses) and is left alone, as is a reader-conditional operand.
//!
//! The fix rewrites `(list* a b)` as `(cons a b)`, copying both operands from
//! their exact source, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ListStarToConsItem {
    pub path: PathBuf,
    /// The span of the whole `(list* a b)` form.
    pub span: ByteSpan,
    /// The span of the first operand `a`.
    pub car_span: ByteSpan,
    /// The span of the second operand `b`.
    pub cdr_span: ByteSpan,
}

#[derive(Debug)]
pub struct ListStarToConsSummary {
    pub list_star_form_count: usize,
    pub violations: Vec<ListStarToConsItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ListStarToConsPolicyOptions {
    fail_on_violation: bool,
}

impl ListStarToConsPolicyOptions {
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
pub struct ListStarToConsPolicy {
    pub fail_on_violation: bool,
    pub list_star_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    list_star_form_count: &mut usize,
    violations: &mut Vec<ListStarToConsItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("list*") {
        return;
    }
    *list_star_form_count += 1;

    // children: [list*, a, b] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let car = &view.children[1];
    let cdr = &view.children[2];
    if is_reader_conditional(car) || is_reader_conditional(cdr) {
        return;
    }

    violations.push(ListStarToConsItem {
        path: path.to_path_buf(),
        span: view.span,
        car_span: car.span,
        cdr_span: cdr.span,
    });
}

/// Collects every two-argument `(list* a b)` across a whole file, along with the
/// total number of `list*` forms scanned.
pub fn collect_list_star_to_cons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<ListStarToConsItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut list_star_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut list_star_form_count, &mut violations);
        });
    }
    Ok((list_star_form_count, violations))
}

#[must_use]
pub const fn summarize_list_star_to_cons(
    list_star_form_count: usize,
    violations: Vec<ListStarToConsItem>,
) -> ListStarToConsSummary {
    ListStarToConsSummary {
        list_star_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_list_star_to_cons_policy(
    options: ListStarToConsPolicyOptions,
    summary: &ListStarToConsSummary,
) -> ListStarToConsPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ListStarToConsPolicy {
        fail_on_violation: options.fail_on_violation(),
        list_star_form_count: summary.list_star_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<ListStarToConsItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_list_star_to_cons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect list* to cons")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_two_argument_list_star() {
        let source = "(list* a b)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].car_span), "a");
        assert_eq!(slice(source, violations[0].cdr_span), "b");
    }

    #[test]
    fn preserves_compound_operands() {
        let source = "(list* (car x) (cdr y))";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].car_span), "(car x)");
        assert_eq!(slice(source, violations[0].cdr_span), "(cdr y)");
    }

    #[test]
    fn does_not_flag_single_argument() {
        // (list* x) is x, single-operand-list-op's concern.
        let (count, violations) = calls("(list* x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_or_more_arguments() {
        // (list* a b c) is (cons a (cons b c)), a genuine list*.
        assert!(calls("(list* a b c)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = calls("(LIST* a b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = calls("(defun f (a b) (list* a b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(list* a b)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_list_star_to_cons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect list* to cons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(list* a b)");
        let summary = summarize_list_star_to_cons(count, items);

        let quiet =
            evaluate_list_star_to_cons_policy(ListStarToConsPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_list_star_to_cons_policy(ListStarToConsPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
