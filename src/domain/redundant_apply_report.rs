//! Common Lisp redundant-`apply` detection: `(apply #'FN (list ARGS…))` where
//! the function is a sharp-quoted symbol and the final argument is a literal
//! `(list …)`. `apply` spreads its last argument as the call's arguments, and a
//! literal `(list a b c)` *is* that argument list verbatim — so
//! `(apply #'f (list a b c))` is exactly `(f a b c)`. The `apply`, the `#'`, and
//! the `list` wrapper are all ceremony.
//!
//! Only the reducible shape is flagged, mirroring
//! [`crate::domain::redundant_funcall_report`]:
//!
//!   - `#'FN` reads as an atom carrying [`ReaderPrefix::Function`]; a direct
//!     `(FN …)` uses the identical lexical resolution. Equivalent.
//!   - The final argument must be a literal `(list …)`. `(apply #'f xs)` with a
//!     *variable* list argument is genuinely dynamic and never flagged.
//!   - There must be no arguments between the function and the list — the common
//!     `(apply #'f (list …))` form. `(apply #'f a b (list …))` (with fixed
//!     leading arguments) is left alone to keep the fix a single, obvious splice.
//!   - `(apply 'f …)` (ordinary quote) and `#'(lambda …)` are not matched, for
//!     the same reasons as `redundant-funcall`.
//!
//! The fix rewrites the whole form as `(FN ARGS…)`, copying the `list` form's
//! element source verbatim, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// The sharp-quoted symbol name of `view` (`#'foo` → `foo`), or `None` when
/// `view` is not an atom carrying exactly the `#'` function prefix over a
/// non-empty symbol. Mirrors `redundant_funcall_report::sharp_quoted_symbol`.
fn sharp_quoted_symbol(view: &ExpressionView) -> Option<&str> {
    if view.kind != ExpressionKind::Atom {
        return None;
    }
    if view.reader_prefixes.as_slice() != [ReaderPrefix::Function] {
        return None;
    }
    let symbol = atom_text(view)?.get(view.symbol_offset..)?;
    (!symbol.is_empty()).then_some(symbol)
}

#[derive(Debug, Clone)]
pub struct RedundantApplyItem {
    pub path: PathBuf,
    /// The span of the whole `(apply #'FN (list …))` form.
    pub span: ByteSpan,
    /// The callee symbol name (`foo` for `#'foo`).
    pub callee: String,
    /// The span of the `list` form's arguments (`a b c` in `(list a b c)`), or
    /// `None` when the list is empty (`(list)` → `(foo)`).
    pub args_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct RedundantApplySummary {
    pub apply_form_count: usize,
    pub violations: Vec<RedundantApplyItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantApplyPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantApplyPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantApplyPolicy {
    pub fail_on_violation: bool,
    pub apply_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_apply(
    view: &ExpressionView,
    path: &Path,
    apply_form_count: &mut usize,
    violations: &mut Vec<RedundantApplyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("apply") {
        return;
    }
    *apply_form_count += 1;

    // children: [apply, #'fn, (list …)] — exactly the no-intermediate-args form.
    if view.children.len() != 3 {
        return;
    }
    let Some(callee) = sharp_quoted_symbol(&view.children[1]) else {
        return;
    };
    let list_form = &view.children[2];
    if !is_paren_list(list_form) {
        return;
    }
    if list_head(list_form).is_none_or(|head| !head.eq_ignore_ascii_case("list")) {
        return;
    }
    // The list form must carry no reader prefix (a plain `(list …)`).
    if !list_form.reader_prefixes.is_empty() {
        return;
    }

    // Arguments to splice: everything after `list`'s head, or none for `(list)`.
    let list_args = &list_form.children[1..];
    let args_span = match (list_args.first(), list_args.last()) {
        (Some(first), Some(last)) => Some(ByteSpan::new(first.span.start(), last.span.end())),
        _ => None,
    };

    violations.push(RedundantApplyItem {
        path: path.to_path_buf(),
        span: view.span,
        callee: callee.to_owned(),
        args_span,
    });
}

/// Collects every redundant `(apply #'fn (list …))` across a whole file, along
/// with the total number of `apply` forms scanned.
pub fn collect_redundant_applies(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantApplyItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut apply_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_apply(subview, path, &mut apply_form_count, &mut violations);
        });
    }
    Ok((apply_form_count, violations))
}

pub fn summarize_redundant_applies(
    apply_form_count: usize,
    violations: Vec<RedundantApplyItem>,
) -> RedundantApplySummary {
    RedundantApplySummary {
        apply_form_count,
        violations,
    }
}

pub fn evaluate_redundant_apply_policy(
    options: RedundantApplyPolicyOptions,
    summary: &RedundantApplySummary,
) -> RedundantApplyPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantApplyPolicy {
        fail_on_violation: options.fail_on_violation(),
        apply_form_count: summary.apply_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applies(input: &str) -> (usize, Vec<RedundantApplyItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_applies(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant applies")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_apply_of_list_literal() {
        let source = "(apply #'foo (list a b))";
        let (count, violations) = applies(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
        assert_eq!(slice(source, violations[0].args_span.unwrap()), "a b");
    }

    #[test]
    fn flags_empty_list_as_a_no_argument_call() {
        let (_, violations) = applies("(apply #'foo (list))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].args_span.is_none());
    }

    #[test]
    fn preserves_compound_argument_source() {
        let source = "(apply #'foo (list (g x) 3))";
        let (_, violations) = applies(source);
        assert_eq!(slice(source, violations[0].args_span.unwrap()), "(g x) 3");
    }

    #[test]
    fn does_not_flag_variable_list_argument() {
        let (count, violations) = applies("(apply #'foo args)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_intermediate_arguments() {
        // (apply #'foo a (list b)) has a fixed leading arg; left alone.
        let (_, violations) = applies("(apply #'foo a (list b))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_quoted_function() {
        let (_, violations) = applies("(apply 'foo (list a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_sharp_quoted_lambda() {
        let (_, violations) = applies("(apply #'(lambda (x) x) (list a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_funcall() {
        let (count, violations) = applies("(funcall #'foo (list a))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_apply_and_list() {
        let (_, violations) = applies("(APPLY #'foo (LIST a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn finds_a_nested_redundant_apply() {
        let (_, violations) = applies("(progn (apply #'h (list g)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "h");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(apply #'foo (list a))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_redundant_applies(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant applies");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = applies("(apply #'foo (list a))");
        let summary = summarize_redundant_applies(count, items);

        let quiet =
            evaluate_redundant_apply_policy(RedundantApplyPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_apply_policy(RedundantApplyPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
