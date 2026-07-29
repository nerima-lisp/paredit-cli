//! Common Lisp `list*`-to-`cons` detection: a two-argument `(list* a b)`. By
//! definition `list*` of exactly two arguments builds a single cons of the first
//! onto the second — `(list* a b)` is exactly `(cons a b)`. Same one fresh cons,
//! same sharing of `b`, `a` and `b` each evaluated once in the same order; the
//! plain `cons` states the intent directly.
//!
//! Only the exact two-argument shape is matched. A single-argument `(list* x)`
//! (which is just `x`) is [`crate::single_operand_list_op::domain`]'s
//! concern, and a three-or-more-argument `list*` is a genuine `list*` (nested
//! conses) and is left alone, as is a reader-conditional operand.
//!
//! The fix rewrites `(list* a b)` as `(cons a b)`, copying both operands from
//! their exact source, so the rule is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ListStarToConsItem {
    /// The span of the whole `(list* a b)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the first operand `a`.
    ///
    /// The rewrite's input, but the old report published it and a consumer
    /// synthesizing its own `(cons a b)` needs it, so it stays on the report.
    pub car_span: ByteSpan,
    /// The span of the second operand `b`. Published for the same reason as
    /// `car_span`.
    pub cdr_span: ByteSpan,
}

impl Finding for ListStarToConsItem {
    /// The rule's own name. Every finding is the same two-argument `list*`, so
    /// there is nothing for a per-finding discriminator to say.
    fn kind(&self) -> &'static str {
        "list-star-to-cons"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the path and line the envelope already prints: the old
    /// text row carried exactly those two. `message` carries the description.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("car_span", span_json(self.car_span)),
            ("cdr_span", span_json(self.cdr_span)),
        ]
    }

    /// The same sentence the `list-star-to-cons` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "a two-argument list* is just a cons; (list* a b) is (cons a b)".to_owned()
    }
}

/// A sub-span in the shape the old hand-written report published it.
fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    list_star_form_count: &mut usize,
    violations: &mut Vec<ListStarToConsItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("list*") {
        return;
    }
    *list_star_form_count += 1;

    // children: [list*, a, b] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let car = &view.children[1];
    let cdr = &view.children[2];
    if is_reader_conditional(car) || is_reader_conditional(cdr) {
        return;
    }

    violations.push(ListStarToConsItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        car_span: car.span,
        cdr_span: cdr.span,
    });
}

/// Collects every two-argument `(list* a b)` in one file, with the number of
/// `list*` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no two-argument list* here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_list_star_to_cons_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ListStarToConsItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("list_star_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut list_star_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut list_star_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("list_star_form_count", json!(list_star_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ListStarToConsItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_list_star_to_cons_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build list* to cons report")
    }

    /// The `(list_star_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<ListStarToConsItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "list_star_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("list_star_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_two_argument_list_star() {
        let source = "(list* a b)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].car_span), "a");
        assert_eq!(slice(source, violations[0].cdr_span), "b");
    }

    #[test]
    fn preserves_compound_operands() {
        let source = "(list* (car x) (cdr y))";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].car_span), "(car x)");
        assert_eq!(slice(source, violations[0].cdr_span), "(cdr y)");
    }

    #[test]
    fn does_not_flag_single_argument() {
        // (list* x) is x, single-operand-list-op's concern.
        let (count, violations) = calls("(list* x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_or_more_arguments() {
        // (list* a b c) is (cons a (cons b c)), a genuine list*.
        assert!(calls("(list* a b c)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = calls("(LIST* a b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = calls("(defun f (a b) (list* a b))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(list* a b)", Dialect::Clojure).expect("parse");
        let report = build_list_star_to_cons_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build list* to cons report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("list_star_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(list* a b c)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_operand_spans_the_fix_needs() {
        let source = "(defun f (a b)\n  (list* (car a) (cdr b)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "list-star-to-cons");
        assert_eq!(slice(source, finding.car_span), "(car a)");
        assert_eq!(slice(source, finding.cdr_span), "(cdr b)");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("car_span", span_json(finding.car_span)),
                ("cdr_span", span_json(finding.cdr_span)),
            ]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_list_star_form_not_only_the_flagged_ones() {
        let report = report("(list* a b)\n(list* a b c)\n(list* x)\n");
        assert_eq!(report.summary, vec![("list_star_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
