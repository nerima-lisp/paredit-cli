//! Common Lisp sharp-quoted-`lambda` detection: a `(lambda …)` form carrying a
//! `#'` (function) reader prefix — `#'(lambda (x) …)`. The `lambda` macro
//! already expands to `(function (lambda …))`, i.e. `#'(lambda …)`, so the
//! explicit `#'` in front is pure duplication: `#'(lambda (x) x)` is exactly
//! `(lambda (x) x)` in every position (argument, value, or binding). Dropping
//! the `#'` is the modern idiom and removes the noise.
//!
//! Only a `(lambda …)` form with exactly the function prefix is flagged. A bare
//! `(lambda …)` is already idiomatic; `#'foo` on a *symbol* is a normal
//! function reference (never redundant); and `#'` on any other form is left
//! alone. Auto-fixable: the fix strips the `#'`.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};

/// Whether `view` is a `(lambda …)` list form (ignoring any reader prefix).
fn is_lambda_form(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("lambda"))
}

#[derive(Debug, Clone)]
pub struct SharpQuotedLambdaItem {
    pub path: PathBuf,
    /// The span of the whole `#'(lambda …)` form (prefix included).
    pub span: ByteSpan,
}

#[derive(Debug)]
pub struct SharpQuotedLambdaSummary {
    pub lambda_form_count: usize,
    pub violations: Vec<SharpQuotedLambdaItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SharpQuotedLambdaPolicyOptions {
    fail_on_violation: bool,
}

impl SharpQuotedLambdaPolicyOptions {
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
pub struct SharpQuotedLambdaPolicy {
    pub fail_on_violation: bool,
    pub lambda_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_lambda(
    view: &ExpressionView,
    path: &Path,
    lambda_form_count: &mut usize,
    violations: &mut Vec<SharpQuotedLambdaItem>,
) {
    if !is_lambda_form(view) {
        return;
    }
    *lambda_form_count += 1;

    // Flag only a lambda form whose sole reader prefix is the function `#'`.
    if view.reader_prefixes.len() == 1 && view.reader_prefixes[0] == ReaderPrefix::Function {
        violations.push(SharpQuotedLambdaItem {
            path: path.to_path_buf(),
            span: view.span,
        });
    }
}

/// Collects every `#'(lambda …)` across a whole file, along with the total
/// number of `lambda` forms scanned.
pub fn collect_sharp_quoted_lambdas(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SharpQuotedLambdaItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut lambda_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_lambda(subview, path, &mut lambda_form_count, &mut violations);
        });
    }
    Ok((lambda_form_count, violations))
}

#[must_use]
pub const fn summarize_sharp_quoted_lambdas(
    lambda_form_count: usize,
    violations: Vec<SharpQuotedLambdaItem>,
) -> SharpQuotedLambdaSummary {
    SharpQuotedLambdaSummary {
        lambda_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_sharp_quoted_lambda_policy(
    options: SharpQuotedLambdaPolicyOptions,
    summary: &SharpQuotedLambdaSummary,
) -> SharpQuotedLambdaPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SharpQuotedLambdaPolicy {
        fail_on_violation: options.fail_on_violation(),
        lambda_form_count: summary.lambda_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lambdas(input: &str) -> (usize, Vec<SharpQuotedLambdaItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_sharp_quoted_lambdas(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect sharp-quoted lambdas")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_sharp_quoted_lambda() {
        let source = "(mapcar #'(lambda (x) (* x x)) xs)";
        let (count, violations) = lambdas(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].span), "#'(lambda (x) (* x x))");
    }

    #[test]
    fn does_not_flag_a_bare_lambda() {
        let (count, violations) = lambdas("(mapcar (lambda (x) x) xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_sharp_quoted_symbol() {
        // #'foo is a normal function reference.
        let (count, violations) = lambdas("(mapcar #'foo xs)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_lambda_head() {
        let (_, violations) = lambdas("#'(LAMBDA (x) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_a_top_level_sharp_quoted_lambda() {
        let (_, violations) = lambdas("(setf f #'(lambda () 42))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_sharp_quoted_lambda() {
        let (_, violations) = lambdas("(defun g () (sort xs #'(lambda (a b) (< a b))))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(mapcar #'(lambda (x) x) xs)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_sharp_quoted_lambdas(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect sharp-quoted lambdas");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = lambdas("#'(lambda (x) x)");
        let summary = summarize_sharp_quoted_lambdas(count, items);

        let quiet = evaluate_sharp_quoted_lambda_policy(
            SharpQuotedLambdaPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_sharp_quoted_lambda_policy(
            SharpQuotedLambdaPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
