//! Common Lisp redundant-`prog1` detection: a `(prog1 x)` wrapping a single
//! form. `prog1` evaluates its forms left to right and returns the value(s) of
//! the *first*; with only one form there is nothing else to sequence, so
//! `(prog1 x)` is exactly `x` — same value(s), same single evaluation.
//!
//! Only the exact single-form shape is matched. A `(prog1 x y …)` with trailing
//! forms genuinely sequences side effects and is left alone, as is an empty
//! `(prog1)` (which is `nil`, a different rewrite) and a reader-conditional body
//! (build-dependent).
//!
//! Unlike `redundant-progn`, `prog1` returns the value of its *first* form,
//! which is why this is its own rule: a multi-form `prog1` cannot become
//! `progn` (that would return the last form instead).
//!
//! The fix replaces the whole form with its single inner form's source, so the
//! rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled body.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantProg1Item {
    /// The span of the whole `(prog1 x)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the single inner form (for reconstructing the fix).
    pub form_span: ByteSpan,
}

impl Finding for RedundantProg1Item {
    fn kind(&self) -> &'static str {
        "redundant-prog1"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the leading `kind`, path and line: this report's text
    /// rows have never carried a column of their own.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// `form_span` is the fix's input, but the JSON has always published it —
    /// it is how a consumer locates the form the `prog1` should collapse to —
    /// so it stays on the report.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "form_span",
            json!({
                "start": self.form_span.start().get(),
                "end": self.form_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `redundant-prog1` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "a prog1 wrapping a single form is just that form; (prog1 x) is x".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    prog1_form_count: &mut usize,
    violations: &mut Vec<RedundantProg1Item>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("prog1") {
        return;
    }
    *prog1_form_count += 1;

    // children: [prog1, form] — require exactly one body form.
    if view.children.len() != 2 {
        return;
    }
    let form = &view.children[1];
    if is_reader_conditional(form) {
        return;
    }

    violations.push(RedundantProg1Item {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        form_span: form.span,
    });
}

/// Collects every single-form `(prog1 x)` in one file, with the number of
/// `prog1` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant prog1 here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_prog1_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantProg1Item>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("prog1_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut prog1_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut prog1_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("prog1_form_count", json!(prog1_form_count))],
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

    fn report(input: &str) -> FileFindings<RedundantProg1Item> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_prog1_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant prog1 report")
    }

    /// The `(prog1_form_count, violations)` pair the report is built from.
    fn prog1s(input: &str) -> (u64, Vec<RedundantProg1Item>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "prog1_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("prog1_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_form_prog1() {
        let source = "(prog1 (compute))";
        let (count, violations) = prog1s(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn flags_atom_form() {
        let source = "(prog1 x)";
        let (_, violations) = prog1s(source);
        assert_eq!(slice(source, violations[0].form_span), "x");
    }

    #[test]
    fn does_not_flag_multi_form_prog1() {
        // (prog1 a b) sequences side effects and returns a; not redundant.
        let (count, violations) = prog1s("(prog1 a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_prog1() {
        let (_, violations) = prog1s("(prog1)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = prog1s("(PROG1 x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = prog1s("(defun f (x) (prog1 x))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(prog1 x)", Dialect::Clojure).expect("parse");
        let report = build_redundant_prog1_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant prog1 report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("prog1_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(prog1 a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_form_span() {
        let report = report("(defun f (x)\n  (prog1 x))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "redundant-prog1");
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
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_prog1_scanned_not_only_the_flagged_ones() {
        let report = report("(prog1 x)\n(prog1 a b)\n(prog1 y)\n");
        assert_eq!(report.summary, vec![("prog1_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
