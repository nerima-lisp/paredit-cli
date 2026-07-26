//! Common Lisp `multiple-value-list`-of-`values` detection: a
//! `(multiple-value-list (values a b …))`. `multiple-value-list` collects all the
//! values its form produces into a fresh list, so collecting the values of a
//! literal `(values a b …)` is exactly `(list a b …)` — same elements, same
//! order, same left-to-right evaluation, without routing through the
//! multiple-values machinery.
//!
//! This is the inverse of
//! [`crate::domain::values_list_of_list_report`] (`(values-list (list …))` is
//! `(values …)`). Only a literal `(values …)` argument is matched; a variable or
//! non-`values` argument and a reader-conditional element are left alone. An
//! empty `(values)` maps to `(list)` (`nil`).
//!
//! The fix rewrites `(multiple-value-list (values a b))` as `(list a b)`,
//! splicing the element source verbatim, so the rule is auto-fixable.
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
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct MultipleValueListOfValuesItem {
    pub path: PathBuf,
    /// The span of the whole `(multiple-value-list (values …))` form.
    pub span: ByteSpan,
    /// The span of the element list (`a b …`, the `values` head and parens
    /// excluded), or `None` for an empty `(values)` (rewrite to `(list)`).
    pub elements_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct MultipleValueListOfValuesSummary {
    pub mvl_form_count: usize,
    pub violations: Vec<MultipleValueListOfValuesItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MultipleValueListOfValuesPolicyOptions {
    fail_on_violation: bool,
}

impl MultipleValueListOfValuesPolicyOptions {
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
pub struct MultipleValueListOfValuesPolicy {
    pub fail_on_violation: bool,
    pub mvl_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    mvl_form_count: &mut usize,
    violations: &mut Vec<MultipleValueListOfValuesItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("multiple-value-list") {
        return;
    }
    *mvl_form_count += 1;

    // children: [multiple-value-list, arg] — exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let arg = &view.children[1];
    if !is_paren_list(arg) {
        return;
    }
    let Some(inner_head) = list_head(arg) else {
        return;
    };
    if !inner_head.eq_ignore_ascii_case("values") {
        return;
    }
    if arg.children.iter().any(is_reader_conditional) {
        return;
    }

    // The elements are children[1..] of the inner `(values …)`; an empty
    // `(values)` has just the head and maps to `(list)`.
    let elements_span = if arg.children.len() > 1 {
        let start = arg.children[1].span.start();
        let end = arg.children[arg.children.len() - 1].span.end();
        Some(ByteSpan::new(start, end))
    } else {
        None
    };

    violations.push(MultipleValueListOfValuesItem {
        path: path.to_path_buf(),
        span: view.span,
        elements_span,
    });
}

/// Collects every `(multiple-value-list (values …))` across a whole file, along
/// with the total number of `multiple-value-list` forms scanned.
pub fn collect_multiple_value_list_of_values(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<MultipleValueListOfValuesItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut mvl_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut mvl_form_count, &mut violations);
        });
    }
    Ok((mvl_form_count, violations))
}

#[must_use]
pub fn summarize_multiple_value_list_of_values(
    mvl_form_count: usize,
    violations: Vec<MultipleValueListOfValuesItem>,
) -> MultipleValueListOfValuesSummary {
    MultipleValueListOfValuesSummary {
        mvl_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_multiple_value_list_of_values_policy(
    options: MultipleValueListOfValuesPolicyOptions,
    summary: &MultipleValueListOfValuesSummary,
) -> MultipleValueListOfValuesPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MultipleValueListOfValuesPolicy {
        fail_on_violation: options.fail_on_violation(),
        mvl_form_count: summary.mvl_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<MultipleValueListOfValuesItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_multiple_value_list_of_values(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect multiple-value-list of values")
    }

    fn elements<'a>(source: &'a str, item: &MultipleValueListOfValuesItem) -> &'a str {
        match item.elements_span {
            Some(span) => &source[span.start().get()..span.end().get()],
            None => "",
        }
    }

    #[test]
    fn flags_mvl_of_values() {
        let source = "(multiple-value-list (values a b))";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(elements(source, &violations[0]), "a b");
    }

    #[test]
    fn preserves_compound_elements() {
        let source = "(multiple-value-list (values (car x) (cdr x)))";
        let (_, violations) = calls(source);
        assert_eq!(elements(source, &violations[0]), "(car x) (cdr x)");
    }

    #[test]
    fn handles_empty_values() {
        let (_, violations) = calls("(multiple-value-list (values))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].elements_span.is_none());
    }

    #[test]
    fn does_not_flag_variable_argument() {
        assert!(calls("(multiple-value-list (compute))").1.is_empty());
    }

    #[test]
    fn does_not_flag_non_values_call() {
        assert!(calls("(multiple-value-list (floor x y))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = calls("(MULTIPLE-VALUE-LIST (VALUES a))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = calls("(defun f (a b) (multiple-value-list (values a b)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(multiple-value-list (values a b))", Dialect::Clojure)
                .expect("parse");
        let (count, violations) = collect_multiple_value_list_of_values(
            &PathBuf::from("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("collect multiple-value-list of values");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(multiple-value-list (values a b))");
        let summary = summarize_multiple_value_list_of_values(count, items);

        let quiet = evaluate_multiple_value_list_of_values_policy(
            MultipleValueListOfValuesPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_multiple_value_list_of_values_policy(
            MultipleValueListOfValuesPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
