//! Common Lisp `values-list`-of-`list` detection: a `(values-list (list a b …))`
//! whose sole argument is a `list` construction. `values-list` returns the
//! elements of its list argument as multiple values, so building a fresh list
//! only to immediately spread it is exactly `(values a b …)` — same values, same
//! order, same left-to-right evaluation, without the throwaway list.
//!
//! Only a literal `(list …)` argument is matched. A quoted list (`'(a b)`) is
//! left alone — its elements are data to return verbatim, not forms to evaluate,
//! so `(values-list '(a b))` is `(values 'a 'b)`, a different rewrite. A variable
//! argument, a non-`list` constructor, and a reader-conditional element are all
//! left alone. An empty `(list)` maps to `(values)` (zero values).
//!
//! The fix rewrites `(values-list (list a b))` as `(values a b)`, splicing the
//! element source verbatim, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ValuesListOfListItem {
    /// The span of the whole `(values-list (list …))` form.
    pub span: ByteSpan,
    /// The span of the element list (`a b …`, the `list` head and parens
    /// excluded), or `None` for an empty `(list)` (rewrite to `(values)`).
    pub elements_span: Option<ByteSpan>,
}

impl Finding for ValuesListOfListItem {
    /// One tag for every finding: this report has a single shape to describe,
    /// a list built only to be spread by the `values-list` around it.
    fn kind(&self) -> &'static str {
        "values-list-of-list"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// None: the old text row carried the path and the offset, both of which
    /// the envelope prints itself.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// `elements_span` stays a key even when it is `null`, which is how the old
    /// renderer wrote an empty `(list)`. A consumer testing the key's presence
    /// rather than its value would otherwise read that case as a missing field.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "elements_span",
            self.elements_span.map_or(Value::Null, |span| {
                json!({
                    "start": span.start().get(),
                    "end": span.end().get(),
                })
            }),
        )]
    }

    /// The same sentence the `values-list-of-list` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "values-list of a fresh list is just values; (values-list (list a b)) is (values a b)"
            .to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    values_list_form_count: &mut usize,
    violations: &mut Vec<ValuesListOfListItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("values-list") {
        return;
    }
    *values_list_form_count += 1;

    // children: [values-list, arg] — exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let arg = &view.children[1];
    if !is_paren_list(arg) {
        return;
    }
    let Some(inner_head) = list_head(arg) else {
        return;
    };
    if !inner_head.eq_ignore_ascii_case("list") {
        return;
    }
    if arg.children.iter().any(is_reader_conditional) {
        return;
    }

    // The elements are children[1..] of the inner `(list …)`; an empty `(list)`
    // has just the head and maps to `(values)`.
    let elements_span = if arg.children.len() > 1 {
        let start = arg.children[1].span.start();
        let end = arg.children[arg.children.len() - 1].span.end();
        Some(ByteSpan::new(start, end))
    } else {
        None
    };

    violations.push(ValuesListOfListItem {
        span: view.span,
        elements_span,
    });
}

/// Collects every `(values-list (list …))` in one file, with the number of
/// `values-list` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no throwaway list here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_values_list_of_list_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ValuesListOfListItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("values_list_form_count", json!(0))],
        ));
    }

    let mut values_list_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut values_list_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("values_list_form_count", json!(values_list_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ValuesListOfListItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_values_list_of_list_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build values-list of list report")
    }

    /// The `(values_list_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<ValuesListOfListItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "values_list_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("values_list_form_count in the summary");
        (count, report.findings)
    }

    fn elements<'a>(source: &'a str, item: &ValuesListOfListItem) -> &'a str {
        match item.elements_span {
            Some(span) => &source[span.start().get()..span.end().get()],
            None => "",
        }
    }

    #[test]
    fn flags_values_list_of_list() {
        let source = "(values-list (list a b))";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(elements(source, &violations[0]), "a b");
    }

    #[test]
    fn preserves_compound_elements() {
        let source = "(values-list (list (car x) (cdr x)))";
        let (_, violations) = calls(source);
        assert_eq!(elements(source, &violations[0]), "(car x) (cdr x)");
    }

    #[test]
    fn handles_empty_list() {
        let (_, violations) = calls("(values-list (list))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].elements_span.is_none());
    }

    #[test]
    fn does_not_flag_quoted_list() {
        // (values-list '(a b)) returns the symbols, not evaluated forms.
        assert!(calls("(values-list '(a b))").1.is_empty());
    }

    #[test]
    fn does_not_flag_variable_argument() {
        assert!(calls("(values-list xs)").1.is_empty());
    }

    #[test]
    fn does_not_flag_non_list_constructor() {
        assert!(calls("(values-list (reverse xs))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = calls("(VALUES-LIST (LIST a))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = calls("(defun f (a b) (values-list (list a b)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(values-list (list a b))", Dialect::Clojure)
            .expect("parse");
        let report =
            build_values_list_of_list_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build values-list of list report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("values_list_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(values-list xs)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_element_span() {
        let report = report("(defun f (a b)\n  (values-list (list a b)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "values-list-of-list");
        let span = finding.elements_span.expect("two elements");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "elements_span",
                json!({ "start": span.start().get(), "end": span.end().get() })
            )]
        );
        assert!(finding.text_columns().is_empty());
    }

    /// An empty `(list)` published `"elements_span": null`; the key stays.
    #[test]
    fn an_empty_list_still_carries_the_element_span_key() {
        let report = report("(values-list (list))");
        assert_eq!(
            report.findings[0].json_fields(),
            vec![("elements_span", Value::Null)]
        );
    }

    #[test]
    fn the_summary_counts_every_values_list_scanned_not_only_the_flagged_ones() {
        let report = report("(values-list (list a))\n(values-list xs)\n(values-list (list))\n");
        assert_eq!(report.summary, vec![("values_list_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
