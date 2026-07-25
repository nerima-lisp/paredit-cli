//! Common Lisp `eq`/`eql`-on-a-quoted-list detection: a call to `eq` or
//! `eql` with a quoted, non-empty list literal argument — `(eq x '(1 2))`,
//! `(eql (path node) '(:a :b))`. `eq` and `eql` both test object identity for
//! conses, and two list literals are distinct objects (a fresh cons on each
//! read/evaluation), so such a comparison is essentially always false
//! regardless of the lists' contents — a classic bug where `equal` was meant.
//!
//! A quoted *symbol* (`'foo`) is fine: symbols are interned, so `(eq x 'foo)`
//! is a correct and common idiom, and is not flagged. The empty quoted list
//! `'()` is `nil` — also a symbol, also `eq`-comparable — and is likewise
//! left alone. Only a quoted, non-empty list triggers the report, whether
//! written with the reader prefix (`'(...)`) or the explicit `(quote (...))`
//! form.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`] and the display rendering
//! from [`crate::domain::expression_equality`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::render_expression;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{for_each_subview, is_paren_list, list_head};

const EQ_HEADS: [&str; 2] = ["eq", "eql"];

/// Whether `view` is a quoted, non-empty list — either `'(a b)` (a paren
/// list carrying a `Quote` reader prefix) or `(quote (a b))` (an explicit
/// quote of a non-empty list). An empty quoted list (`'()` / `(quote ())`)
/// is `nil` and is not a list literal for this purpose.
fn is_quoted_list_literal(view: &ExpressionView) -> bool {
    if is_paren_list(view)
        && view.reader_prefixes.contains(&ReaderPrefix::Quote)
        && !view.children.is_empty()
    {
        return true;
    }

    if list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("quote")) {
        if let Some(quoted) = view.children.get(1) {
            return is_paren_list(quoted) && !quoted.children.is_empty();
        }
    }

    false
}

#[derive(Debug, Clone)]
pub struct EqlListComparisonItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub literal: String,
}

#[derive(Debug)]
pub struct EqlListComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<EqlListComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EqlListComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl EqlListComparisonPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EqlListComparisonPolicy {
    pub fail_on_violation: bool,
    pub comparison_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_comparison(
    view: &ExpressionView,
    path: &Path,
    comparison_form_count: &mut usize,
    violations: &mut Vec<EqlListComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EQ_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *comparison_form_count += 1;

    // Report the first quoted-list argument (after the operator); a call with
    // two such literals is still one bug, not two.
    if let Some(literal) = view
        .children
        .iter()
        .skip(1)
        .find(|argument| is_quoted_list_literal(argument))
    {
        violations.push(EqlListComparisonItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_owned(),
            literal: render_expression(literal),
        });
    }
}

/// Collects every `eq`/`eql` call with a quoted-list-literal argument across
/// a whole file, along with the total number of `eq`/`eql` calls scanned.
pub fn collect_eql_list_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EqlListComparisonItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, path, &mut comparison_form_count, &mut violations)
        });
    }
    Ok((comparison_form_count, violations))
}

pub fn summarize_eql_list_comparisons(
    comparison_form_count: usize,
    violations: Vec<EqlListComparisonItem>,
) -> EqlListComparisonSummary {
    EqlListComparisonSummary {
        comparison_form_count,
        violations,
    }
}

pub fn evaluate_eql_list_comparison_policy(
    options: EqlListComparisonPolicyOptions,
    summary: &EqlListComparisonSummary,
) -> EqlListComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EqlListComparisonPolicy {
        fail_on_violation: options.fail_on_violation(),
        comparison_form_count: summary.comparison_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparisons(input: &str) -> (usize, Vec<EqlListComparisonItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_eql_list_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect eql list comparisons")
    }

    #[test]
    fn flags_eql_against_a_quoted_list() {
        let (comparison_form_count, violations) = comparisons("(eql x '(1 2))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eql");
        assert_eq!(violations[0].literal, "(1 2)");
    }

    #[test]
    fn flags_eq_against_an_explicit_quote_form() {
        let (_, violations) = comparisons("(eq (path n) (quote (:a :b)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn does_not_flag_a_quoted_symbol() {
        let (comparison_form_count, violations) = comparisons("(eq x 'foo)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_empty_quoted_list_which_is_nil() {
        let (_, violations) = comparisons("(eq x '())");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equal_against_a_quoted_list() {
        let (comparison_form_count, violations) = comparisons("(equal x '(1 2))");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) =
            comparisons("(defun f (x) (when (eql x '(a)) 1))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(eq x '(1 2))").expect("parse input");
        let (comparison_form_count, violations) =
            collect_eql_list_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect eql list comparisons");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (comparison_form_count, items) = comparisons("(eql x '(1 2))");
        let summary = summarize_eql_list_comparisons(comparison_form_count, items);

        let quiet = evaluate_eql_list_comparison_policy(
            EqlListComparisonPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_eql_list_comparison_policy(
            EqlListComparisonPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
