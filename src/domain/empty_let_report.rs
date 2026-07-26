//! Common Lisp empty-`let` detection: a `let` whose binding list is empty —
//! `(let () body…)` or `(let nil body…)`. With no bindings established, the
//! `let` does nothing but evaluate its body in order, which is exactly what
//! `progn` does: `(let () a b)` is `(progn a b)`. The `let` wrapper is pure
//! noise, usually left behind after every binding is removed.
//!
//! Only bare `let` is handled. `(let* () …)` is first reduced to `(let () …)`
//! by the `redundant-let-star` rule (a `let*` with no bindings is a `let`), so
//! scoping here to `let` keeps the two rules from emitting overlapping fixes on
//! the same head and lets the empty-`let*` case compose through in two passes.
//!
//! A `let` whose body *leads* with a `(declare …)` is left alone: an
//! empty-binding `let` still establishes a declaration scope, but `progn` has
//! none, so `(progn (declare …) …)` would be malformed. An empty-body
//! `(let ())` is left alone too (there is nothing to splice).
//!
//! The fix rewrites the `(let ()` prefix as `(progn`, leaving the body
//! byte-identical, so the rule is auto-fixable.
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

/// Whether `view` is an empty binding list: `()` (a paren list with no
/// children) or the bare atom `nil`.
fn is_empty_binding_list(view: &ExpressionView) -> bool {
    if is_paren_list(view) {
        return view.children.is_empty();
    }
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// Whether `view` is a `(declare …)` form (valid in a `let` body, invalid in a
/// `progn`).
fn is_declare_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("declare"))
}

#[derive(Debug, Clone)]
pub struct EmptyLetItem {
    pub path: PathBuf,
    /// The span of the whole `(let () body…)` form.
    pub span: ByteSpan,
    /// The span of the `(let ()` prefix (form start through the binding list),
    /// which the fix replaces with `(progn`.
    pub prefix_span: ByteSpan,
}

#[derive(Debug)]
pub struct EmptyLetSummary {
    pub let_form_count: usize,
    pub violations: Vec<EmptyLetItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EmptyLetPolicyOptions {
    fail_on_violation: bool,
}

impl EmptyLetPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EmptyLetPolicy {
    pub fail_on_violation: bool,
    pub let_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_let(
    view: &ExpressionView,
    path: &Path,
    let_form_count: &mut usize,
    violations: &mut Vec<EmptyLetItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("let") {
        return;
    }
    *let_form_count += 1;

    // children: [let, binding-list, body…] — need the binding list and a body.
    if view.children.len() < 3 {
        return;
    }
    let binding = &view.children[1];
    if !is_empty_binding_list(binding) {
        return;
    }
    // A leading declaration cannot survive the move to progn.
    if is_declare_form(&view.children[2]) {
        return;
    }

    let prefix_span = ByteSpan::new(view.span.start(), binding.span.end());
    violations.push(EmptyLetItem {
        path: path.to_path_buf(),
        span: view.span,
        prefix_span,
    });
}

/// Collects every empty-binding `let` across a whole file, along with the total
/// number of `let` forms scanned.
pub fn collect_empty_lets(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EmptyLetItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut let_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let(subview, path, &mut let_form_count, &mut violations)
        });
    }
    Ok((let_form_count, violations))
}

pub fn summarize_empty_lets(
    let_form_count: usize,
    violations: Vec<EmptyLetItem>,
) -> EmptyLetSummary {
    EmptyLetSummary {
        let_form_count,
        violations,
    }
}

pub fn evaluate_empty_let_policy(
    options: EmptyLetPolicyOptions,
    summary: &EmptyLetSummary,
) -> EmptyLetPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EmptyLetPolicy {
        fail_on_violation: options.fail_on_violation(),
        let_form_count: summary.let_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lets(input: &str) -> (usize, Vec<EmptyLetItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_empty_lets(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect empty lets")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_empty_paren_bindings() {
        let source = "(let () (foo) (bar))";
        let (count, violations) = lets(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].prefix_span), "(let ()");
    }

    #[test]
    fn flags_nil_bindings() {
        let source = "(let nil (foo))";
        let (_, violations) = lets(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].prefix_span), "(let nil");
    }

    #[test]
    fn does_not_flag_non_empty_bindings() {
        let (count, violations) = lets("(let ((x 1)) x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_let_star() {
        // (let* () …) is redundant-let-star's job (it becomes (let () …) first).
        let (count, violations) = lets("(let* () (foo))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_leading_declare() {
        // (progn (declare …) …) would be malformed.
        let (_, violations) = lets("(let () (declare (ignore x)) (foo))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_body() {
        let (count, violations) = lets("(let ())");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = lets("(LET () (foo))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_empty_let() {
        let (_, violations) = lets("(defun f () (let () (side-effect) (result)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(let () (foo))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_empty_lets(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect empty lets");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = lets("(let () (foo))");
        let summary = summarize_empty_lets(count, items);

        let quiet = evaluate_empty_let_policy(EmptyLetPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_empty_let_policy(EmptyLetPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
