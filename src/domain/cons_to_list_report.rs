//! Common Lisp cons-to-`list` detection: a `cons` whose tail is `nil` or a
//! `list` literal, which is really a `list` construction — `(cons a nil)` is
//! `(list a)`, and `(cons a (list b c))` is `(list a b c)`. Building a proper
//! list with `list` reads more directly than a `cons` spelled out against `nil`.
//!
//! Both shapes are exact: `(cons X nil)` and `(cons X (list …))` each build a
//! fresh proper list with `X` prepended, evaluating `X` once. Because the
//! recursive shape (`list` tail) is handled too, a spelled-out cons chain
//! collapses one layer per `--fix` pass — `(cons a (cons b nil))` converges to
//! `(list a b)`.
//!
//! Only a `nil`/`()` tail or a `(list …)` tail is matched; a `cons` onto any
//! other form (a variable, an improper pair like `(cons a b)`) is a genuine cons
//! and is left alone, as is a reader-conditional operand.
//!
//! The fix rewrites the form as `(list X …)`, copying the element and the tail
//! list's elements from source, so the rule is auto-fixable.
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

/// Whether `view` is the empty-list tail: the bare `nil` symbol (no reader
/// prefixes) or a literal `()`.
fn is_nil_tail(view: &ExpressionView) -> bool {
    if is_paren_list(view) {
        return view.children.is_empty() && view.reader_prefixes.is_empty();
    }
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ConsToListItem {
    pub path: PathBuf,
    /// The span of the whole `(cons X TAIL)` form.
    pub span: ByteSpan,
    /// The span of the prepended element `X`.
    pub element_span: ByteSpan,
    /// The span of the tail list's elements (`b c` in `(list b c)`), or `None`
    /// for a `nil`/empty-list tail.
    pub tail_elements_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct ConsToListSummary {
    pub cons_form_count: usize,
    pub violations: Vec<ConsToListItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ConsToListPolicyOptions {
    fail_on_violation: bool,
}

impl ConsToListPolicyOptions {
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
pub struct ConsToListPolicy {
    pub fail_on_violation: bool,
    pub cons_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_cons(
    view: &ExpressionView,
    path: &Path,
    cons_form_count: &mut usize,
    violations: &mut Vec<ConsToListItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("cons") {
        return;
    }
    *cons_form_count += 1;

    // children: [cons, element, tail].
    if view.children.len() != 3 {
        return;
    }
    let element = &view.children[1];
    let tail = &view.children[2];
    if is_reader_conditional(element) || is_reader_conditional(tail) {
        return;
    }

    let tail_elements_span = if is_nil_tail(tail) {
        None
    } else if is_paren_list(tail)
        && tail.reader_prefixes.is_empty()
        && list_head(tail).is_some_and(|h| h.eq_ignore_ascii_case("list"))
    {
        let list_args = &tail.children[1..];
        if list_args.iter().any(is_reader_conditional) {
            return;
        }
        match (list_args.first(), list_args.last()) {
            (Some(first), Some(last)) => Some(ByteSpan::new(first.span.start(), last.span.end())),
            _ => None, // empty (list)
        }
    } else {
        return; // tail is not nil or a list literal.
    };

    violations.push(ConsToListItem {
        path: path.to_path_buf(),
        span: view.span,
        element_span: element.span,
        tail_elements_span,
    });
}

/// Collects every collapsible `cons` across a whole file, along with the total
/// number of `cons` forms scanned.
pub fn collect_cons_to_lists(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ConsToListItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut cons_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cons(subview, path, &mut cons_form_count, &mut violations);
        });
    }
    Ok((cons_form_count, violations))
}

#[must_use]
pub const fn summarize_cons_to_lists(
    cons_form_count: usize,
    violations: Vec<ConsToListItem>,
) -> ConsToListSummary {
    ConsToListSummary {
        cons_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_cons_to_list_policy(
    options: ConsToListPolicyOptions,
    summary: &ConsToListSummary,
) -> ConsToListPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ConsToListPolicy {
        fail_on_violation: options.fail_on_violation(),
        cons_form_count: summary.cons_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conses(input: &str) -> (usize, Vec<ConsToListItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_cons_to_lists(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect cons to lists")
    }

    fn parts<'a>(source: &'a str, item: &ConsToListItem) -> (&'a str, Option<&'a str>) {
        let el = &source[item.element_span.start().get()..item.element_span.end().get()];
        let tail = item
            .tail_elements_span
            .map(|s| &source[s.start().get()..s.end().get()]);
        (el, tail)
    }

    #[test]
    fn cons_onto_nil_is_singleton() {
        let source = "(cons a nil)";
        let (count, violations) = conses(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(parts(source, &violations[0]), ("a", None));
    }

    #[test]
    fn cons_onto_empty_list_literal() {
        let source = "(cons a ())";
        let (_, violations) = conses(source);
        assert_eq!(parts(source, &violations[0]), ("a", None));
    }

    #[test]
    fn cons_onto_list_prepends() {
        let source = "(cons a (list b c))";
        let (_, violations) = conses(source);
        assert_eq!(parts(source, &violations[0]), ("a", Some("b c")));
    }

    #[test]
    fn preserves_compound_element_source() {
        let source = "(cons (f x) nil)";
        let (_, violations) = conses(source);
        assert_eq!(parts(source, &violations[0]).0, "(f x)");
    }

    #[test]
    fn does_not_flag_cons_onto_variable() {
        let (count, violations) = conses("(cons a xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_improper_pair() {
        let (_, violations) = conses("(cons a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_cons_and_list() {
        let (_, violations) = conses("(CONS a (LIST b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(parts("(CONS a (LIST b))", &violations[0]), ("a", Some("b")));
    }

    #[test]
    fn finds_a_nested_cons_chain_outer() {
        // The outer cons matches (tail is (cons b nil), not nil/list) — actually
        // the inner one matches; the outer is caught after the inner is fixed.
        let (_, violations) = conses("(cons a (cons b nil))");
        assert_eq!(violations.len(), 1); // only the inner (cons b nil)
        assert_eq!(parts("(cons a (cons b nil))", &violations[0]), ("b", None));
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(cons a nil)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_cons_to_lists(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect cons to lists");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = conses("(cons a nil)");
        let summary = summarize_cons_to_lists(count, items);

        let quiet = evaluate_cons_to_list_policy(ConsToListPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_cons_to_list_policy(ConsToListPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
