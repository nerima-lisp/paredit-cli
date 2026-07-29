//! Common Lisp redundant-`parse-integer`-`:radix`-`10` detection: a call
//! `(parse-integer s :radix 10)`. CLHS defines `parse-integer` as
//! `parse-integer string &key start end radix junk-allowed` with `radix`
//! defaulting to `10`, so `(parse-integer s :radix 10)` restates the default and
//! is exactly `(parse-integer s)`.
//!
//! Only a bare integer `10` literal value (no reader prefixes) is flagged; a
//! non-`10` radix (`(parse-integer s :radix 16)`) is meaningful and left alone.
//! Other keyword arguments (`:start`, `:junk-allowed`, …) are preserved.
//!
//! The fix deletes the redundant ` :radix 10` argument pair (from the end of the
//! preceding argument through the `10`), leaving the rest byte-identical, so the
//! rule is auto-fixable.
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

/// Whether `view` is the bare `:radix` keyword atom.
fn is_radix_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":radix"))
}

/// Whether `view` is the bare integer `10` literal (no reader prefixes).
fn is_ten_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view).is_some_and(|t| t == "10")
}

#[derive(Debug, Clone)]
pub struct ParseIntegerDefaultRadixItem {
    /// The span of the whole `(parse-integer …)` call form.
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The span to delete: the ` :radix 10` argument pair.
    pub removal_span: ByteSpan,
}

impl Finding for ParseIntegerDefaultRadixItem {
    /// The rule's own name. Every finding is the same restated default; there
    /// is nothing to discriminate on.
    fn kind(&self) -> &'static str {
        "parse-integer-default-radix"
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

    /// The removal span the previous renderer emitted, unchanged. It is the
    /// fix's input, but it was part of this report's published JSON, so
    /// dropping it here would be a silent break for anything already reading
    /// it.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "removal_span",
            json!({
                "start": self.removal_span.start().get(),
                "end": self.removal_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `parse-integer-default-radix` lint rule writes, so
    /// a SARIF or JUnit consumer reading both sees one finding described one
    /// way.
    fn message(&self) -> String {
        "explicit :radix 10 restates parse-integer's default; drop it".to_owned()
    }
}

/// Whether a `:radix` argument is provably ten.
///
/// The standalone `inspect parse-integer-default-radix` command reads only the
/// literal `10`, having no semantic tables to consult. The lint suite passes a
/// test that resolves constants and folds arithmetic, so it also sees `#xA`
/// and `(let ((r 10)) … :radix r)` — the same redundant argument, spelled
/// differently.
pub type IsDefaultRadix<'a> = &'a dyn Fn(&ExpressionView) -> bool;

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    is_ten: IsDefaultRadix<'_>,
    call_form_count: &mut usize,
    violations: &mut Vec<ParseIntegerDefaultRadixItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("parse-integer") {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_radix_keyword(&view.children[index]) {
            continue;
        }
        if !is_ten(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(ParseIntegerDefaultRadixItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            removal_span,
        });
        return;
    }
}

/// Collects every `(parse-integer s :radix 10)` in one file, with the number of
/// `parse-integer` calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no restated default here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_parse_integer_default_radix_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ParseIntegerDefaultRadixItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("call_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                source,
                &is_ten_literal,
                &mut call_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("call_form_count", json!(call_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ParseIntegerDefaultRadixItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_parse_integer_default_radix_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build parse-integer default radix report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<ParseIntegerDefaultRadixItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "call_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("call_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_radix_ten() {
        let source = "(parse-integer s :radix 10)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :radix 10");
    }

    #[test]
    fn keeps_other_keywords() {
        let source = "(parse-integer s :radix 10 :junk-allowed t)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :radix 10");
    }

    #[test]
    fn does_not_flag_non_ten_radix() {
        let (count, violations) = calls("(parse-integer s :radix 16)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_parse_integer() {
        let (_, violations) = calls("(parse-integer s)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(PARSE-INTEGER s :RADIX 10)");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(parse-integer s :radix 10)", Dialect::Clojure)
            .expect("parse");
        let report =
            build_parse_integer_default_radix_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build parse-integer default radix report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(parse-integer s)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_removal_span() {
        let report = report("(defun read-count (s)\n  (parse-integer s :radix 10))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "parse-integer-default-radix");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![(
                "removal_span",
                json!({
                    "start": finding.removal_span.start().get(),
                    "end": finding.removal_span.end().get(),
                })
            )]
        );
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(parse-integer s :radix 10)\n(parse-integer t :radix 16)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
