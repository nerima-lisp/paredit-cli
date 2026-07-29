//! Common Lisp empty-`handler-case` detection: a `(handler-case expr)` with no
//! handler clauses. `handler-case` establishes handlers around `expr` and returns
//! its value(s); with zero clauses it establishes nothing, so it is exactly
//! `expr` — a leftover wrapper (often after the last handler clause was deleted).
//!
//! Only the exact no-clause shape `(handler-case expr)` is matched; a
//! `handler-case` with any clause is a real construct and left alone, as is a
//! reader-conditional protected form.
//!
//! The fix replaces the whole form with the protected form's source, so the rule
//! is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct HandlerCaseNoClausesItem {
    /// The span of the whole `(handler-case expr)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the protected form `expr` (for reconstructing the fix).
    pub form_span: ByteSpan,
}

impl Finding for HandlerCaseNoClausesItem {
    /// The rule's own name. There is exactly one shape this rule flags — the
    /// clauseless `(handler-case expr)` — so there is no variant to separate.
    fn kind(&self) -> &'static str {
        "handler-case-no-clauses"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Empty: the old text row carried nothing beyond the path and offset, both
    /// of which the envelope supplies.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// `form_span` is the protected form the fix splices in. It is on the
    /// report because the hand-written renderer published it, and a consumer
    /// applying the same edit itself needs it.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "form_span",
            json!({
                "start": self.form_span.start().get(),
                "end": self.form_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `handler-case-no-clauses` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "a handler-case with no clauses is just its body; (handler-case x) is x".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    handler_case_form_count: &mut usize,
    violations: &mut Vec<HandlerCaseNoClausesItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("handler-case") {
        return;
    }
    *handler_case_form_count += 1;

    // children: [handler-case, expr] — no handler clauses.
    if view.children.len() != 2 {
        return;
    }
    let form = &view.children[1];
    if is_reader_conditional(form) {
        return;
    }

    violations.push(HandlerCaseNoClausesItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        form_span: form.span,
    });
}

/// Collects every clauseless `(handler-case expr)` in one file, with the number
/// of `handler-case` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every handler-case has clauses here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_handler_case_no_clauses_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<HandlerCaseNoClausesItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("handler_case_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut handler_case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                source,
                &mut handler_case_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("handler_case_form_count", json!(handler_case_form_count))],
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

    fn report(input: &str) -> FileFindings<HandlerCaseNoClausesItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_handler_case_no_clauses_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build handler-case no clauses report")
    }

    /// The `(handler_case_form_count, violations)` pair the report is built
    /// from.
    fn cases(input: &str) -> (u64, Vec<HandlerCaseNoClausesItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "handler_case_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("handler_case_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_clauseless_handler_case() {
        let source = "(handler-case (compute))";
        let (count, violations) = cases(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn does_not_flag_handler_case_with_clauses() {
        assert!(
            cases("(handler-case (compute) (error (e) nil))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = cases("(HANDLER-CASE x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = cases("(defun f (x) (handler-case x))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(handler-case x)", Dialect::Clojure).expect("parse");
        let report =
            build_handler_case_no_clauses_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build handler-case no clauses report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("handler_case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(handler-case (compute) (error (e) nil))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_protected_form_span() {
        let source = "(defun f ()\n  (handler-case (compute)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "handler-case-no-clauses");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![(
                "form_span",
                json!({
                    "start": finding.form_span.start().get(),
                    "end": finding.form_span.end().get(),
                })
            )]
        );
        assert_eq!(slice(source, finding.form_span), "(compute)");
    }

    #[test]
    fn the_summary_counts_every_handler_case_scanned_not_only_the_flagged_ones() {
        let report = report("(handler-case x)\n(handler-case y (error (e) nil))\n");
        assert_eq!(report.summary, vec![("handler_case_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
