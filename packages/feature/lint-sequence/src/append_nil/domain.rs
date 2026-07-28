//! Common Lisp `append`-with-a-trailing-`nil` detection: a two-argument
//! `(append x nil)`. `append` copies every argument but the last and links the
//! copy's final `cdr` to the last argument; with the last argument `nil`, the
//! result is a fresh top-level copy of `x` — exactly `(copy-seq`… no: for a list
//! it is `(copy-list x)`. Both `append` (whose non-last argument must be a proper
//! list) and `copy-list` (which requires a proper list) reject a non-list `x`
//! identically, so the rewrite preserves the type-check domain as well as the
//! value.
//!
//! Only the two-argument `(append x nil)` shape with a bare literal `nil` tail is
//! matched. A non-`nil` tail, more than two arguments, and a reader-conditional
//! operand are left alone. (`(nconc x nil)` is deliberately *not* handled by an
//! analogous rule: it would rewrite to bare `x`, which — unlike `nconc` — accepts
//! a non-list, silently dropping a type check.)
//!
//! The fix rewrites `(append x nil)` as `(copy-list x)`, copying `x`'s source, so
//! the rule is auto-fixable.
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

/// Whether `view` is the bare literal `nil` (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct AppendNilItem {
    pub path: PathBuf,
    /// The span of the whole `(append x nil)` form.
    pub span: ByteSpan,
    /// The span of the list operand `x`.
    pub list_span: ByteSpan,
}

#[derive(Debug)]
pub struct AppendNilSummary {
    pub append_form_count: usize,
    pub violations: Vec<AppendNilItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct AppendNilPolicyOptions {
    fail_on_violation: bool,
}

impl AppendNilPolicyOptions {
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
pub struct AppendNilPolicy {
    pub fail_on_violation: bool,
    pub append_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    append_form_count: &mut usize,
    violations: &mut Vec<AppendNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("append") {
        return;
    }
    *append_form_count += 1;

    // children: [append, x, nil] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let list = &view.children[1];
    let tail = &view.children[2];
    if is_reader_conditional(list) {
        return;
    }
    if !is_nil_literal(tail) {
        return;
    }

    violations.push(AppendNilItem {
        path: path.to_path_buf(),
        span: view.span,
        list_span: list.span,
    });
}

/// Collects every `(append x nil)` across a whole file, along with the total
/// number of `append` forms scanned.
pub fn collect_append_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<AppendNilItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut append_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut append_form_count, &mut violations);
        });
    }
    Ok((append_form_count, violations))
}

#[must_use]
pub const fn summarize_append_nils(
    append_form_count: usize,
    violations: Vec<AppendNilItem>,
) -> AppendNilSummary {
    AppendNilSummary {
        append_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_append_nil_policy(
    options: AppendNilPolicyOptions,
    summary: &AppendNilSummary,
) -> AppendNilPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    AppendNilPolicy {
        fail_on_violation: options.fail_on_violation(),
        append_form_count: summary.append_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appends(input: &str) -> (usize, Vec<AppendNilItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_append_nils(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect append nils")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_append_nil() {
        let source = "(append xs nil)";
        let (count, violations) = appends(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].list_span), "xs");
    }

    #[test]
    fn preserves_compound_list() {
        let source = "(append (mapcar #'f ys) nil)";
        let (_, violations) = appends(source);
        assert_eq!(slice(source, violations[0].list_span), "(mapcar #'f ys)");
    }

    #[test]
    fn does_not_flag_non_nil_tail() {
        let (_, violations) = appends("(append xs ys)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_leading_nil() {
        // (append nil xs) is a different shape (nil is not the last argument).
        let (_, violations) = appends("(append nil xs)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_arguments() {
        assert!(appends("(append xs ys nil)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = appends("(APPEND xs NIL)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = appends("(defun f (xs) (append xs nil))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(append xs nil)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_append_nils(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect append nils");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = appends("(append xs nil)");
        let summary = summarize_append_nils(count, items);

        let quiet = evaluate_append_nil_policy(AppendNilPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_append_nil_policy(AppendNilPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
