//! Common Lisp nested string-case detection: a `(OUTER (INNER s))` where BOTH
//! heads are non-destructive string case operations (`string-upcase`,
//! `string-downcase`, `string-capitalize`). Because case operations change only
//! letter case — never letter identity or word boundaries — the outer operation
//! fully determines the result. So `(string-upcase (string-downcase s))` is
//! exactly `(string-upcase s)`: the inner call is dead work. This holds for any
//! two of upcase/downcase/capitalize, including the idempotent
//! `(string-upcase (string-upcase s))`.
//!
//! Only the three non-destructive operations are matched. The destructive
//! `nstring-upcase`/`nstring-downcase`/`nstring-capitalize` are excluded —
//! dropping the inner one would drop its in-place mutation. The outer head token
//! is preserved in the fix, so the result reads with the dominating operation. A
//! reader-conditional operand `s` is left alone.
//!
//! The fix rewrites `(OUTER (INNER s))` as `(OUTER s)` (keeping the outer head),
//! copying `s`'s source, so the rule is auto-fixable.
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

/// The non-destructive string case operations. The destructive `nstring-*`
/// counterparts are excluded because dropping the inner one would drop its
/// in-place mutation.
const CASE_OPS: [&str; 3] = ["string-upcase", "string-downcase", "string-capitalize"];

fn is_case_op(head: &str) -> bool {
    CASE_OPS.iter().any(|op| head.eq_ignore_ascii_case(op))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedStringCaseItem {
    /// The span of the whole `(OUTER (INNER s))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the outer case-op head token, preserved in the fix.
    pub outer_span: ByteSpan,
    /// The span of the string operand `s`.
    pub string_span: ByteSpan,
}

impl Finding for NestedStringCaseItem {
    /// The rule's own name. Which of the three case operations nests inside
    /// which does not change the finding — the outer one dominates in every
    /// pair — so there is no variant worth separating.
    fn kind(&self) -> &'static str {
        "nested-string-case"
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

    /// The head and operand spans, which the old report already published and a
    /// caller collapsing the pair reads.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("outer_span", span_json(self.outer_span)),
            ("string_span", span_json(self.string_span)),
        ]
    }

    /// The same sentence the `nested-string-case` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "the outer string case op dominates; the inner one is dead work".to_owned()
    }
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    string_case_form_count: &mut usize,
    violations: &mut Vec<NestedStringCaseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !is_case_op(head) {
        return;
    }
    *string_case_form_count += 1;

    // children: [outer, inner] — the case op takes exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let inner = &view.children[1];
    if !is_paren_list(inner) {
        return;
    }
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    if !is_case_op(inner_head) {
        return;
    }
    // inner children: [inner, s] — the inner case op takes exactly one argument.
    if inner.children.len() != 2 {
        return;
    }
    let string = &inner.children[1];
    if is_reader_conditional(string) {
        return;
    }

    violations.push(NestedStringCaseItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        outer_span: view.children[0].span,
        string_span: string.span,
    });
}

/// Collects every nested `(OUTER (INNER s))` case-op pair in one file, with the
/// number of case-op forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no nested case-op pair here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_nested_string_case_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedStringCaseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("string_case_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut string_case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                source,
                &mut string_case_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("string_case_form_count", json!(string_case_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NestedStringCaseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_string_case_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested string case report")
    }

    /// The `(string_case_form_count, violations)` pair the report is built
    /// from.
    fn cases(input: &str) -> (u64, Vec<NestedStringCaseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "string_case_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("string_case_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_upcase_of_downcase() {
        let source = "(string-upcase (string-downcase s))";
        let (count, violations) = cases(source);
        // Both the outer upcase and the inner downcase are case-op forms scanned.
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_span), "string-upcase");
        assert_eq!(slice(source, violations[0].string_span), "s");
    }

    #[test]
    fn flags_downcase_of_capitalize() {
        let (_, violations) = cases("(string-downcase (string-capitalize s))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_idempotent() {
        let (_, violations) = cases("(string-upcase (string-upcase s))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_single_case() {
        assert!(cases("(string-upcase s)").1.is_empty());
    }

    #[test]
    fn does_not_flag_inner_non_case() {
        assert!(cases("(string-upcase (subseq s 1))").1.is_empty());
    }

    #[test]
    fn does_not_flag_destructive_inner() {
        // (string-upcase (nstring-downcase s)) mutates s in place; dropping the
        // inner call would drop that mutation, so it is not equivalent.
        assert!(cases("(string-upcase (nstring-downcase s))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = cases("(STRING-UPCASE (STRING-DOWNCASE s))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(string-upcase (string-downcase s))", Dialect::Clojure)
                .expect("parse");
        let report = build_nested_string_case_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nested string case report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("string_case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(string-upcase s)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_head_and_operand_spans() {
        let source = "(defun f (s)\n  (string-upcase (string-downcase s)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "nested-string-case");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("outer_span", span_json(finding.outer_span)),
                ("string_span", span_json(finding.string_span)),
            ]
        );
        assert_eq!(slice(source, finding.outer_span), "string-upcase");
        assert_eq!(slice(source, finding.string_span), "s");
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_case_op_scanned_not_only_the_flagged_ones() {
        // Three case-op forms: the outer, the inner, and the lone one below.
        let report = report("(string-upcase (string-downcase s))\n(string-upcase t)\n");
        assert_eq!(report.summary, vec![("string_case_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
