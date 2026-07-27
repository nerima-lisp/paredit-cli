//! Common Lisp funcall-of-lambda detection: a `funcall` whose first argument is
//! a literal `(lambda …)` form. A lambda form evaluates to a function and is
//! directly applicable in operator position, so `(funcall (lambda (x) …) a b)`
//! is exactly `((lambda (x) …) a b)` — same function, same arguments, same
//! values (both pass every returned value through). The `funcall` adds nothing.
//!
//! Only a bare `(lambda …)` first argument is flagged. A sharp-quoted symbol
//! (`(funcall #'foo …)`) is the province of the `redundant-funcall` rule, and a
//! `#'(lambda …)` first argument carries a reader prefix that cannot sit in
//! operator position after the rewrite, so it is left alone here.
//!
//! The fix drops the `funcall ` head, leaving the lambda form as the operator,
//! so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, is_paren_list, list_head};

/// Whether `view` is a bare `(lambda …)` form (no reader prefix, so a
/// `#'(lambda …)` is excluded — its prefix would be illegal in operator
/// position).
fn is_lambda_form(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && is_paren_list(view)
        && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("lambda"))
}

#[derive(Debug, Clone)]
pub struct FuncallLambdaItem {
    pub path: PathBuf,
    /// The span of the whole `(funcall (lambda …) …)` form.
    pub span: ByteSpan,
    /// The span of the `funcall` head symbol.
    pub head_span: ByteSpan,
    /// The span of the `(lambda …)` form (its start marks where the rewrite
    /// keeps the source; everything from the head to here is dropped).
    pub lambda_span: ByteSpan,
}

#[derive(Debug)]
pub struct FuncallLambdaSummary {
    pub funcall_form_count: usize,
    pub violations: Vec<FuncallLambdaItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct FuncallLambdaPolicyOptions {
    fail_on_violation: bool,
}

impl FuncallLambdaPolicyOptions {
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
pub struct FuncallLambdaPolicy {
    pub fail_on_violation: bool,
    pub funcall_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_funcall(
    view: &ExpressionView,
    path: &Path,
    funcall_form_count: &mut usize,
    violations: &mut Vec<FuncallLambdaItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("funcall") {
        return;
    }
    *funcall_form_count += 1;

    // children: [funcall, callee, args…] — need at least the callee.
    if view.children.len() < 2 {
        return;
    }
    let callee = &view.children[1];
    if !is_lambda_form(callee) {
        return;
    }

    violations.push(FuncallLambdaItem {
        path: path.to_path_buf(),
        span: view.span,
        head_span: view.children[0].span,
        lambda_span: callee.span,
    });
}

/// Collects every `funcall` of a bare lambda form across a whole file, along
/// with the total number of `funcall` forms scanned.
pub fn collect_funcall_lambdas(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<FuncallLambdaItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut funcall_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_funcall(subview, path, &mut funcall_form_count, &mut violations);
        });
    }
    Ok((funcall_form_count, violations))
}

#[must_use]
pub const fn summarize_funcall_lambdas(
    funcall_form_count: usize,
    violations: Vec<FuncallLambdaItem>,
) -> FuncallLambdaSummary {
    FuncallLambdaSummary {
        funcall_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_funcall_lambda_policy(
    options: FuncallLambdaPolicyOptions,
    summary: &FuncallLambdaSummary,
) -> FuncallLambdaPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    FuncallLambdaPolicy {
        fail_on_violation: options.fail_on_violation(),
        funcall_form_count: summary.funcall_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn funcalls(input: &str) -> (usize, Vec<FuncallLambdaItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_funcall_lambdas(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect funcall lambdas")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_funcall_of_lambda() {
        let source = "(funcall (lambda (x) (* x x)) 5)";
        let (count, violations) = funcalls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].head_span), "funcall");
        assert_eq!(
            slice(source, violations[0].lambda_span),
            "(lambda (x) (* x x))"
        );
    }

    #[test]
    fn flags_funcall_of_lambda_no_args() {
        let (_, violations) = funcalls("(funcall (lambda () 42))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_funcall_of_sharp_quoted_symbol() {
        // That is the redundant-funcall rule's job.
        let (count, violations) = funcalls("(funcall #'foo 1 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_funcall_of_sharp_quoted_lambda() {
        // #'(lambda …) carries a reader prefix illegal in operator position.
        let (_, violations) = funcalls("(funcall #'(lambda (x) x) 5)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_funcall_of_a_variable() {
        let (_, violations) = funcalls("(funcall fn 1 2)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_funcall() {
        let (count, violations) = funcalls("(funcall)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = funcalls("(FUNCALL (lambda (x) x) 1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_funcall_lambda() {
        let (_, violations) = funcalls("(mapcar (lambda (y) (funcall (lambda (x) x) y)) xs)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(funcall (lambda (x) x) 5)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_funcall_lambdas(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect funcall lambdas");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = funcalls("(funcall (lambda (x) x) 5)");
        let summary = summarize_funcall_lambdas(count, items);

        let quiet =
            evaluate_funcall_lambda_policy(FuncallLambdaPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_funcall_lambda_policy(FuncallLambdaPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
