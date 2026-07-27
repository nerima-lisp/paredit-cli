//! Common Lisp redundant-`funcall` detection: `(funcall #'FN ARGS…)` where the
//! function argument is a sharp-quoted plain symbol. `(funcall #'foo a b)` and
//! `(foo a b)` resolve `foo` through the exact same lexical function namespace —
//! a `flet`/`labels` binding if one is in scope, otherwise the global function —
//! so the `funcall` and the `#'` are pure ceremony. This shape is common after a
//! higher-order helper is inlined or a macro is expanded mechanically.
//!
//! Only the sharp-quoted *symbol* form is flagged, because that is the only one
//! whose direct-call rewrite is guaranteed equivalent:
//!
//!   - `#'foo` reads as an atom `foo` carrying [`ReaderPrefix::Function`]; a
//!     direct `(foo …)` uses the identical lexical resolution. Equivalent.
//!   - `(funcall fn …)` with a *variable* `fn` (no sharp-quote) genuinely
//!     dispatches on a runtime value and is never flagged.
//!   - `(funcall #'(lambda …) …)` sharp-quotes a *list*, not a symbol, so it is
//!     left alone (unwrapping a lambda is a different transformation).
//!   - `(funcall 'foo …)` (ordinary quote) resolves `foo`'s *global* function at
//!     call time and is **not** equivalent to `(foo …)` under a local `flet`, so
//!     only the sharp-quote (`#'`) form is matched.
//!
//! The fix deletes the `funcall ` head and the `#'` prefix in one contiguous
//! cut — from the `funcall` symbol up to the callee symbol — leaving the callee
//! and every argument byte-identical. That makes the rule auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteOffset, ByteSpan, ExpressionKind, ExpressionView, Path as SexprPath, ReaderPrefix,
    SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// The sharp-quoted symbol name of `view` (`#'foo` → `foo`), or `None` when
/// `view` is not an atom carrying exactly the `#'` function prefix over a
/// non-empty symbol. Requiring the prefix list to be *exactly* `[Function]`
/// rejects combinations like `,#'foo` whose rewrite is not a plain deletion.
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
pub struct RedundantFuncallItem {
    pub path: PathBuf,
    /// The span of the whole `(funcall #'FN …)` form.
    pub span: ByteSpan,
    /// The callee symbol name (`foo` for `#'foo`).
    pub callee: String,
    /// The bytes a fix deletes: from the `funcall` head through the `#'` prefix,
    /// i.e. everything up to the callee symbol. Deleting this turns
    /// `(funcall #'foo a b)` into `(foo a b)`.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct RedundantFuncallSummary {
    pub funcall_form_count: usize,
    pub violations: Vec<RedundantFuncallItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantFuncallPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantFuncallPolicyOptions {
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
pub struct RedundantFuncallPolicy {
    pub fail_on_violation: bool,
    pub funcall_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_funcall(
    view: &ExpressionView,
    path: &Path,
    funcall_form_count: &mut usize,
    violations: &mut Vec<RedundantFuncallItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("funcall") {
        return;
    }
    *funcall_form_count += 1;

    // children[0] is `funcall`; children[1] is the function argument.
    if view.children.len() < 2 {
        return;
    }
    let funcall_head = &view.children[0];
    let function_arg = &view.children[1];
    let Some(callee) = sharp_quoted_symbol(function_arg) else {
        return;
    };

    // Delete `funcall ` and the `#'`: from the head start to the callee symbol.
    let callee_symbol_start =
        ByteOffset::new(function_arg.span.start().get() + function_arg.symbol_offset);
    violations.push(RedundantFuncallItem {
        path: path.to_path_buf(),
        span: view.span,
        callee: callee.to_owned(),
        removal_span: ByteSpan::new(funcall_head.span.start(), callee_symbol_start),
    });
}

/// Collects every redundant `(funcall #'symbol …)` across a whole file, along
/// with the total number of `funcall` forms scanned.
pub fn collect_redundant_funcalls(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<RedundantFuncallItem>)> {
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
pub const fn summarize_redundant_funcalls(
    funcall_form_count: usize,
    violations: Vec<RedundantFuncallItem>,
) -> RedundantFuncallSummary {
    RedundantFuncallSummary {
        funcall_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_funcall_policy(
    options: RedundantFuncallPolicyOptions,
    summary: &RedundantFuncallSummary,
) -> RedundantFuncallPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantFuncallPolicy {
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

    fn funcalls(input: &str) -> (usize, Vec<RedundantFuncallItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_funcalls(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant funcalls")
    }

    #[test]
    fn flags_sharp_quoted_symbol_with_args() {
        let (count, violations) = funcalls("(funcall #'foo a b)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn flags_sharp_quoted_symbol_with_no_args() {
        let (_, violations) = funcalls("(funcall #'foo)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn removal_span_covers_funcall_and_the_sharp_quote() {
        // Deleting the removal span must turn the form into `(foo a b)`.
        let source = "(funcall #'foo a b)";
        let (_, violations) = funcalls(source);
        let removal = violations[0].removal_span;
        let (start, end) = (removal.start().get(), removal.end().get());
        let mut rewritten = String::from(source);
        rewritten.replace_range(start..end, "");
        assert_eq!(rewritten, "(foo a b)");
    }

    #[test]
    fn does_not_flag_a_variable_function_argument() {
        let (count, violations) = funcalls("(funcall fn a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_sharp_quoted_lambda() {
        let (_, violations) = funcalls("(funcall #'(lambda (x) x) y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_ordinary_quoted_symbol() {
        // '(foo) resolves the *global* function, not equivalent under flet.
        let (_, violations) = funcalls("(funcall 'foo a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = funcalls("(apply #'foo args)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_funcall_head() {
        let (_, violations) = funcalls("(FUNCALL #'foo x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "foo");
    }

    #[test]
    fn finds_a_nested_redundant_funcall() {
        let (_, violations) = funcalls("(mapcar (lambda (g) (funcall #'h g)) xs)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].callee, "h");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(funcall #'foo a)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_funcalls(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant funcalls");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = funcalls("(funcall #'foo a)");
        let summary = summarize_redundant_funcalls(count, items);

        let quiet =
            evaluate_redundant_funcall_policy(RedundantFuncallPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_funcall_policy(RedundantFuncallPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
