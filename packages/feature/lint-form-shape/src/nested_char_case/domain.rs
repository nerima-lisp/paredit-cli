//! Common Lisp nested char-case detection: a `(OUTER (INNER c))` where BOTH
//! heads are character case operations (`char-upcase`, `char-downcase`). A char
//! case operation changes only case — never character identity — so the outer
//! operation fully determines the result. So `(char-upcase (char-downcase c))`
//! is exactly `(char-upcase c)`: the inner call is dead work. This holds for any
//! two of upcase/downcase, including the idempotent
//! `(char-upcase (char-upcase c))`.
//!
//! Both `char-upcase` and `char-downcase` are non-destructive (they return a
//! fresh character; there are no `nchar-*` mutators), so there is no mutation to
//! preserve. The outer head token is kept in the fix so the result reads with
//! the dominating operation. A reader-conditional operand `c` is left alone.
//!
//! The fix rewrites `(OUTER (INNER c))` as `(OUTER c)` (keeping the outer head),
//! copying `c`'s source, so the rule is auto-fixable.
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

/// The character case operations. Both are non-destructive.
const CASE_OPS: [&str; 2] = ["char-upcase", "char-downcase"];

fn is_case_op(head: &str) -> bool {
    CASE_OPS.iter().any(|op| head.eq_ignore_ascii_case(op))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedCharCaseItem {
    /// The span of the whole `(OUTER (INNER c))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the outer case-op head token, preserved in the fix.
    pub outer_span: ByteSpan,
    /// The span of the character operand `c`.
    pub char_span: ByteSpan,
}

impl Finding for NestedCharCaseItem {
    /// The rule's own name. There is nothing to discriminate on: every finding
    /// is the same collapse, and which of the four upcase/downcase pairings
    /// produced it lives in the source the spans point at.
    fn kind(&self) -> &'static str {
        "nested-char-case"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// None. The old text row carried the path and offset the envelope now
    /// prints itself, and nothing else.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// Both spans the previous renderer emitted, unchanged. They are the fix's
    /// inputs, but they were part of this report's published JSON, so dropping
    /// them here would be a silent break for anything already reading them.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            (
                "outer_span",
                json!({
                    "start": self.outer_span.start().get(),
                    "end": self.outer_span.end().get(),
                }),
            ),
            (
                "char_span",
                json!({
                    "start": self.char_span.start().get(),
                    "end": self.char_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `nested-char-case` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "the outer char case op dominates; the inner one is dead work".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    char_case_form_count: &mut usize,
    violations: &mut Vec<NestedCharCaseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !is_case_op(head) {
        return;
    }
    *char_case_form_count += 1;

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
    // inner children: [inner, c] — the inner case op takes exactly one argument.
    if inner.children.len() != 2 {
        return;
    }
    let character = &inner.children[1];
    if is_reader_conditional(character) {
        return;
    }

    violations.push(NestedCharCaseItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        outer_span: view.children[0].span,
        char_span: character.span,
    });
}

/// Collects every nested `(OUTER (INNER c))` char case-op pair in one file, with
/// the number of outer case-op forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no nested char case here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_nested_char_case_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedCharCaseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("char_case_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut char_case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut char_case_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("char_case_form_count", json!(char_case_form_count))],
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

    fn report(input: &str) -> FileFindings<NestedCharCaseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_char_case_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested char case report")
    }

    /// The `(char_case_form_count, violations)` pair the report is built from.
    fn cases(input: &str) -> (u64, Vec<NestedCharCaseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "char_case_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("char_case_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_upcase_of_downcase() {
        let source = "(char-upcase (char-downcase c))";
        let (count, violations) = cases(source);
        // Both the outer upcase and the inner downcase are case-op forms scanned.
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_span), "char-upcase");
        assert_eq!(slice(source, violations[0].char_span), "c");
    }

    #[test]
    fn flags_idempotent() {
        let (_, violations) = cases("(char-downcase (char-downcase c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_single_case() {
        assert!(cases("(char-upcase c)").1.is_empty());
    }

    #[test]
    fn does_not_flag_inner_non_case() {
        assert!(cases("(char-upcase (elt s 1))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = cases("(CHAR-UPCASE (CHAR-DOWNCASE c))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(char-upcase (char-downcase c))", Dialect::Clojure)
                .expect("parse");
        let report = build_nested_char_case_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nested char case report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("char_case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(char-upcase c)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_fix_spans() {
        let source = "(defun f (c)\n  (char-upcase (char-downcase c)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "nested-char-case");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![
                (
                    "outer_span",
                    json!({
                        "start": finding.outer_span.start().get(),
                        "end": finding.outer_span.end().get(),
                    })
                ),
                (
                    "char_span",
                    json!({
                        "start": finding.char_span.start().get(),
                        "end": finding.char_span.end().get(),
                    })
                ),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_case_op_scanned_not_only_the_flagged_ones() {
        let report = report("(char-upcase (char-downcase c))\n(char-upcase d)\n");
        assert_eq!(report.summary, vec![("char_case_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
