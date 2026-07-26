//! Common Lisp `append`-of-a-singleton detection: a two-argument `append` whose
//! first argument is a one-element `(list x)` — `(append (list x) rest)`.
//! `append` copies every list but the last and links the copy's final `cdr` to
//! the last argument, so `(append (list x) rest)` builds a single fresh cons
//! whose car is `x` and whose cdr is the shared `rest` — which is exactly
//! `(cons x rest)`. The rewrite is exact: same one fresh cons, same sharing of
//! `rest`, `x` and `rest` each evaluated once in the same left-to-right order.
//!
//! Only the narrow, provably-equivalent shape is flagged: exactly two `append`
//! arguments, the first a `(list x)` with exactly one element. A multi-element
//! `(list x y)` first argument is `(list* x y rest)`, not `cons`, so it is left
//! alone, as is a non-`list` first argument, a different argument count, and a
//! reader-conditional operand.
//!
//! The fix rewrites `(append (list x) rest)` as `(cons x rest)`, copying the
//! element and the tail from their exact source, so the rule is auto-fixable.
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

/// Whether `view` is a one-element `(list x)` call; returns the element `x`.
fn singleton_list(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("list") {
        return None;
    }
    // children: [list, element] — exactly one element.
    if view.children.len() != 2 {
        return None;
    }
    Some(&view.children[1])
}

#[derive(Debug, Clone)]
pub struct AppendListToConsItem {
    pub path: PathBuf,
    /// The span of the whole `(append (list x) rest)` form.
    pub span: ByteSpan,
    /// The span of the singleton element `x`.
    pub element_span: ByteSpan,
    /// The span of the tail `rest`.
    pub rest_span: ByteSpan,
}

#[derive(Debug)]
pub struct AppendListToConsSummary {
    pub append_form_count: usize,
    pub violations: Vec<AppendListToConsItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct AppendListToConsPolicyOptions {
    fail_on_violation: bool,
}

impl AppendListToConsPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct AppendListToConsPolicy {
    pub fail_on_violation: bool,
    pub append_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    append_form_count: &mut usize,
    violations: &mut Vec<AppendListToConsItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("append") {
        return;
    }
    *append_form_count += 1;

    // children: [append, first, rest] — exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let first = &view.children[1];
    let rest = &view.children[2];
    if is_reader_conditional(rest) {
        return;
    }
    let Some(element) = singleton_list(first) else {
        return;
    };
    if is_reader_conditional(element) {
        return;
    }

    violations.push(AppendListToConsItem {
        path: path.to_path_buf(),
        span: view.span,
        element_span: element.span,
        rest_span: rest.span,
    });
}

/// Collects every `(append (list x) rest)` across a whole file, along with the
/// total number of `append` forms scanned.
pub fn collect_append_list_to_cons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<AppendListToConsItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut append_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut append_form_count, &mut violations)
        });
    }
    Ok((append_form_count, violations))
}

pub fn summarize_append_list_to_cons(
    append_form_count: usize,
    violations: Vec<AppendListToConsItem>,
) -> AppendListToConsSummary {
    AppendListToConsSummary {
        append_form_count,
        violations,
    }
}

pub fn evaluate_append_list_to_cons_policy(
    options: AppendListToConsPolicyOptions,
    summary: &AppendListToConsSummary,
) -> AppendListToConsPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    AppendListToConsPolicy {
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

    fn appends(input: &str) -> (usize, Vec<AppendListToConsItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_append_list_to_cons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect append list to cons")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_append_singleton() {
        let source = "(append (list x) rest)";
        let (count, violations) = appends(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].element_span), "x");
        assert_eq!(slice(source, violations[0].rest_span), "rest");
    }

    #[test]
    fn preserves_compound_element_and_tail() {
        let source = "(append (list (car a)) (cdr b))";
        let (_, violations) = appends(source);
        assert_eq!(slice(source, violations[0].element_span), "(car a)");
        assert_eq!(slice(source, violations[0].rest_span), "(cdr b)");
    }

    #[test]
    fn does_not_flag_multi_element_list() {
        // (append (list x y) rest) is (list* x y rest), not cons.
        let (_, violations) = appends("(append (list x y) rest)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_list_first_argument() {
        assert!(appends("(append xs rest)").1.is_empty());
        assert!(appends("(append (reverse xs) rest)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_argument_count() {
        assert!(appends("(append (list x))").1.is_empty());
        assert!(appends("(append (list x) a b)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = appends("(APPEND (LIST x) rest)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = appends("(defun f (x r) (append (list x) r))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(append (list x) rest)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_append_list_to_cons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect append list to cons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = appends("(append (list x) rest)");
        let summary = summarize_append_list_to_cons(count, items);

        let quiet = evaluate_append_list_to_cons_policy(
            AppendListToConsPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_append_list_to_cons_policy(AppendListToConsPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
