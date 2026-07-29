//! Common Lisp `multiple-value-list`-of-`values` detection: a
//! `(multiple-value-list (values a b …))`. `multiple-value-list` collects all the
//! values its form produces into a fresh list, so collecting the values of a
//! literal `(values a b …)` is exactly `(list a b …)` — same elements, same
//! order, same left-to-right evaluation, without routing through the
//! multiple-values machinery.
//!
//! This is the inverse of
//! [`crate::values_list_of_list::domain`] (`(values-list (list …))` is
//! `(values …)`). Only a literal `(values …)` argument is matched; a variable or
//! non-`values` argument and a reader-conditional element are left alone. An
//! empty `(values)` maps to `(list)` (`nil`).
//!
//! The fix rewrites `(multiple-value-list (values a b))` as `(list a b)`,
//! splicing the element source verbatim, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
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
pub struct MultipleValueListOfValuesItem {
    /// The span of the whole `(multiple-value-list (values …))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the element list (`a b …`, the `values` head and parens
    /// excluded), or `None` for an empty `(values)` (rewrite to `(list)`).
    ///
    /// Both the fix's input and part of the report: an agent that wants to
    /// perform the rewrite itself needs the exact bytes, and the old report
    /// published them — `null` included, for the empty `(values)` case.
    pub elements_span: Option<ByteSpan>,
}

impl Finding for MultipleValueListOfValuesItem {
    /// The rule's own name. Every finding here is the same rewrite — a
    /// `multiple-value-list` of a literal `values` — with nothing to sub-divide
    /// it by.
    fn kind(&self) -> &'static str {
        "multiple-value-list-of-values"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "elements_span",
            match self.elements_span {
                Some(span) => json!({
                    "start": span.start().get(),
                    "end": span.end().get(),
                }),
                None => json!(null),
            },
        )]
    }

    /// The same sentence the `multiple-value-list-of-values` lint rule writes,
    /// so a SARIF or JUnit consumer reading both sees one finding described one
    /// way.
    fn message(&self) -> String {
        "multiple-value-list of a values form is just list; (multiple-value-list (values a b)) is (list a b)"
            .to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    mvl_form_count: &mut usize,
    violations: &mut Vec<MultipleValueListOfValuesItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("multiple-value-list") {
        return;
    }
    *mvl_form_count += 1;

    // children: [multiple-value-list, arg] — exactly one argument.
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
    if !inner_head.eq_ignore_ascii_case("values") {
        return;
    }
    if arg.children.iter().any(is_reader_conditional) {
        return;
    }

    // The elements are children[1..] of the inner `(values …)`; an empty
    // `(values)` has just the head and maps to `(list)`.
    let elements_span = if arg.children.len() > 1 {
        let start = arg.children[1].span.start();
        let end = arg.children[arg.children.len() - 1].span.end();
        Some(ByteSpan::new(start, end))
    } else {
        None
    };

    violations.push(MultipleValueListOfValuesItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        elements_span,
    });
}

/// Collects every `(multiple-value-list (values …))` in one file, with the
/// number of `multiple-value-list` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no `multiple-value-list` of a literal
/// `values`" for Common Lisp and "nothing was looked for" for Clojure, and the
/// two read identically without the flag.
pub fn build_multiple_value_list_of_values_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MultipleValueListOfValuesItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("mvl_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut mvl_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut mvl_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("mvl_form_count", json!(mvl_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MultipleValueListOfValuesItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_multiple_value_list_of_values_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build multiple-value-list of values report")
    }

    /// The `(mvl_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<MultipleValueListOfValuesItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "mvl_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("mvl_form_count in the summary");
        (count, report.findings)
    }

    fn elements<'a>(source: &'a str, item: &MultipleValueListOfValuesItem) -> &'a str {
        match item.elements_span {
            Some(span) => &source[span.start().get()..span.end().get()],
            None => "",
        }
    }

    #[test]
    fn flags_mvl_of_values() {
        let source = "(multiple-value-list (values a b))";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(elements(source, &violations[0]), "a b");
    }

    #[test]
    fn preserves_compound_elements() {
        let source = "(multiple-value-list (values (car x) (cdr x)))";
        let (_, violations) = calls(source);
        assert_eq!(elements(source, &violations[0]), "(car x) (cdr x)");
    }

    #[test]
    fn handles_empty_values() {
        let (_, violations) = calls("(multiple-value-list (values))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].elements_span.is_none());
    }

    #[test]
    fn does_not_flag_variable_argument() {
        assert!(calls("(multiple-value-list (compute))").1.is_empty());
    }

    #[test]
    fn does_not_flag_non_values_call() {
        assert!(calls("(multiple-value-list (floor x y))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = calls("(MULTIPLE-VALUE-LIST (VALUES a))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = calls("(defun f (a b) (multiple-value-list (values a b)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(multiple-value-list (values a b))", Dialect::Clojure)
                .expect("parse");
        let report = build_multiple_value_list_of_values_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build multiple-value-list of values report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("mvl_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(multiple-value-list (compute))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_elements_span() {
        let report = report("(defun f (a b)\n  (multiple-value-list (values a b)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "multiple-value-list-of-values");
        let span = finding.elements_span.expect("elements span");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "elements_span",
                json!({ "start": span.start().get(), "end": span.end().get() }),
            )]
        );
        assert!(finding.text_columns().is_empty());
    }

    /// The empty `(values)` still publishes the key, as `null`, exactly as the
    /// hand-written renderer did.
    #[test]
    fn an_empty_values_reports_a_null_elements_span() {
        let report = report("(multiple-value-list (values))");
        assert_eq!(
            report.findings[0].json_fields(),
            vec![("elements_span", json!(null))]
        );
    }

    #[test]
    fn the_summary_counts_every_form_scanned_not_only_the_flagged_ones() {
        let report =
            report("(multiple-value-list (compute))\n(multiple-value-list (values a b))\n");
        assert_eq!(report.summary, vec![("mvl_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
