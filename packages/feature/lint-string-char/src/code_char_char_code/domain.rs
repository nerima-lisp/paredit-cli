//! Common Lisp `code-char`-of-`char-code` detection: a `(code-char (char-code c))`.
//!
//! `char-code` maps a character to its code and `code-char` maps that code back to
//! the same character, so `(code-char (char-code c))` is exactly `c` — same
//! character, `c` evaluated once. The bare `c` reads more directly than the
//! round-trip pair.
//!
//! Only the exact `(code-char (char-code c))` two-level shape is matched:
//! `code-char` with one argument, whose argument is `(char-code c)` with exactly
//! one operand. The reverse direction `(char-code (code-char n))` is NOT handled:
//! `code-char` can return `nil` for codes with no corresponding character, so the
//! reverse is not an identity and cannot be safely unwrapped. A reader-conditional
//! operand is left alone.
//!
//! The fix rewrites `(code-char (char-code c))` as `c`, copying the character
//! operand verbatim, so the rule is auto-fixable.
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
pub struct CodeCharCharCodeItem {
    /// The span of the whole `(code-char (char-code c))` form.
    pub span: ByteSpan,
    /// The span of the character operand `c`.
    pub char_span: ByteSpan,
}

impl Finding for CodeCharCharCodeItem {
    /// The rule's own name: the round-trip has one shape, and the reverse
    /// direction is deliberately not matched, so there is nothing to separate.
    fn kind(&self) -> &'static str {
        "code-char-char-code"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// The operand span, which the old report already published and a caller
    /// unwrapping the round-trip reads.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("char_span", span_json(self.char_span))]
    }

    /// The same sentence the `code-char-char-code` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "code-char of char-code is a round-trip; (code-char (char-code c)) is c".to_owned()
    }
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    code_char_form_count: &mut usize,
    violations: &mut Vec<CodeCharCharCodeItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("code-char") {
        return;
    }
    *code_char_form_count += 1;

    // children: [code-char, inner] — code-char takes exactly one argument.
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
    if !inner_head.eq_ignore_ascii_case("char-code") {
        return;
    }
    // inner children: [char-code, c].
    if inner.children.len() != 2 {
        return;
    }
    let character = &inner.children[1];
    if is_reader_conditional(character) {
        return;
    }

    violations.push(CodeCharCharCodeItem {
        span: view.span,
        char_span: character.span,
    });
}

/// Collects every `(code-char (char-code c))` in one file, with the number of
/// `code-char` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no round-trip here" for Common Lisp and
/// "nothing was looked for" for Clojure, and the two read identically without
/// the flag.
pub fn build_code_char_char_code_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CodeCharCharCodeItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("code_char_form_count", json!(0))],
        ));
    }

    let mut code_char_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut code_char_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("code_char_form_count", json!(code_char_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CodeCharCharCodeItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_code_char_char_code_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build code char char code report")
    }

    /// The `(code_char_form_count, violations)` pair the report is built from.
    fn code_chars(input: &str) -> (u64, Vec<CodeCharCharCodeItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "code_char_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("code_char_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_code_char_char_code() {
        let source = "(code-char (char-code c))";
        let (count, violations) = code_chars(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].char_span), "c");
    }

    #[test]
    fn preserves_compound_char() {
        let source = "(code-char (char-code (elt s i)))";
        let (_, violations) = code_chars(source);
        assert_eq!(slice(source, violations[0].char_span), "(elt s i)");
    }

    #[test]
    fn does_not_flag_plain_char_code() {
        let (_, violations) = code_chars("(char-code c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_reverse() {
        let (_, violations) = code_chars("(char-code (code-char n))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = code_chars("(CODE-CHAR (CHAR-CODE c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = code_chars("(defun f (c) (code-char (char-code c)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(code-char (char-code c))", Dialect::Clojure)
            .expect("parse");
        let report =
            build_code_char_char_code_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build code char char code report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("code_char_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(code-char n)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operand_span() {
        let source = "(defun f (c)\n  (code-char (char-code c)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "code-char-char-code");
        assert_eq!(
            finding.json_fields(),
            vec![("char_span", span_json(finding.char_span))]
        );
        assert_eq!(slice(source, finding.char_span), "c");
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_code_char_scanned_not_only_the_flagged_ones() {
        let report = report("(code-char (char-code c))\n(code-char n)\n");
        assert_eq!(report.summary, vec![("code_char_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
