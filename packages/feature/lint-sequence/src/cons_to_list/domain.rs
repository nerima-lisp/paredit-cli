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
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

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
    /// The span of the whole `(cons X TAIL)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the prepended element `X`.
    ///
    /// The rewrite's input, not the report's: the lint rule reads it to build
    /// the `list` call, and the command has never printed it.
    pub element_span: ByteSpan,
    /// The span of the tail list's elements (`b c` in `(list b c)`), or `None`
    /// for a `nil`/empty-list tail.
    ///
    /// The rewrite's input, like `element_span`, and likewise unpublished.
    pub tail_elements_span: Option<ByteSpan>,
}

impl Finding for ConsToListItem {
    fn kind(&self) -> &'static str {
        "cons-to-list"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing: the old text row carried only the path and the offset, both of
    /// which the envelope prints itself. The `message` override is what a
    /// reader of a text row has to go on.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// Nothing beyond the span the envelope already prints. The two operand
    /// spans this item carries feed the autofix and were never in the report's
    /// JSON; moving onto the envelope is not the occasion to start publishing
    /// them.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `cons-to-list` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "cons onto nil/a list is a list constructor; use list".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_cons(
    view: &ExpressionView,
    source: &str,
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
        span: view.span,
        line: line_of(source, view.span.start().get()),
        element_span: element.span,
        tail_elements_span,
    });
}

/// Collects every collapsible `cons` in one file, with the number of `cons`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every cons here is a genuine cons" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn collect_cons_to_lists(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ConsToListItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("cons_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut cons_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cons(subview, source, &mut cons_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("cons_form_count", json!(cons_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ConsToListItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_cons_to_lists(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect cons to lists")
    }

    /// The `(cons_form_count, violations)` pair the report is built from.
    fn conses(input: &str) -> (u64, Vec<ConsToListItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "cons_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("cons_form_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(cons a nil)", Dialect::Clojure).expect("parse");
        let report = collect_cons_to_lists(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect cons to lists");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("cons_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(cons a xs)").dialect_modelled);
    }

    /// The old JSON published the path and the whole form's span and nothing
    /// else, so the envelope's own fields carry the entire finding.
    #[test]
    fn a_finding_carries_its_line_and_no_extra_json() {
        let report = report("(defun f (a)\n  (cons a nil))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "cons-to-list");
        assert!(finding.text_columns().is_empty());
        assert!(finding.json_fields().is_empty());
        assert_eq!(
            finding.message(),
            "cons onto nil/a list is a list constructor; use list"
        );
    }

    #[test]
    fn the_summary_counts_every_cons_scanned_not_only_the_flagged_ones() {
        let report = report("(cons a nil)\n(cons a xs)\n");
        assert_eq!(report.summary, vec![("cons_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
