//! Common Lisp `format`-to-string detection: a `(format nil "~A" x)` or
//! `(format nil "~S" x)` whose control string is exactly the single directive
//! `~A`/`~a` or `~S`/`~s`. With a `nil` destination `format` returns the produced
//! string, and a lone `~A` directive prints its argument with `princ` semantics
//! while `~S` uses `prin1` semantics, so:
//!
//!   - `(format nil "~A" x)` is exactly `(princ-to-string x)`, and
//!   - `(format nil "~S" x)` is exactly `(prin1-to-string x)`.
//!
//! Both name the intent directly and avoid re-parsing a control string at run
//! time. The equivalence is exact: `princ-to-string`/`prin1-to-string` are
//! defined to print as if by `princ`/`prin1`, matching `~A`/`~S`'s binding of
//! `*print-escape*`.
//!
//! Only a control string that is *exactly* the one directive is matched — any
//! surrounding text (`"value: ~A"`), extra directives, a non-`nil` destination
//! (`t` or a stream, whose return value is `nil`, not the string), an argument
//! count other than one, and a reader-conditional argument are all left alone.
//!
//! The fix rewrites the call as `(princ-to-string x)`/`(prin1-to-string x)`,
//! copying the argument's source verbatim, so the rule is auto-fixable.
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

/// Whether `view` is the bare literal `nil` destination (no reader prefixes).
fn is_nil_destination(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// The string-producing replacement function for a control string that is
/// exactly one `~A`/`~S` directive, or `None` for any other control string.
/// `text` is the atom's source, including its surrounding quotes.
fn directive_replacement(text: &str) -> Option<&'static str> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    match inner {
        "~a" | "~A" => Some("princ-to-string"),
        "~s" | "~S" => Some("prin1-to-string"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct FormatToStringItem {
    /// The span of the whole `(format nil "~A" x)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The replacement function name (`princ-to-string` or `prin1-to-string`).
    pub replacement: &'static str,
    /// The span of the single format argument (for reconstructing the fix).
    pub argument_span: ByteSpan,
}

impl Finding for FormatToStringItem {
    /// The replacement function, so `~A` and `~S` are separable without parsing
    /// JSON.
    ///
    /// They are two different rewrites — `~A` is `princ` semantics, `~S` is
    /// `prin1` — and a consumer filtering on one of them is asking a real
    /// question.
    fn kind(&self) -> &'static str {
        self.replacement
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.replacement.to_owned()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("replacement", json!(self.replacement)),
            ("argument_span", span_json(self.argument_span)),
        ]
    }

    /// The same sentence the `format-to-string` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "format to a string is just {}; use ({} x)",
            self.replacement, self.replacement
        )
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
    format_form_count: &mut usize,
    violations: &mut Vec<FormatToStringItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("format") {
        return;
    }
    *format_form_count += 1;

    // children: [format, destination, control, argument] — exactly one argument.
    if view.children.len() != 4 {
        return;
    }
    let destination = &view.children[1];
    let control = &view.children[2];
    let argument = &view.children[3];
    if !is_nil_destination(destination) {
        return;
    }
    if is_reader_conditional(argument) {
        return;
    }
    let Some(replacement) = atom_text(control).and_then(directive_replacement) else {
        return;
    };

    violations.push(FormatToStringItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        replacement,
        argument_span: argument.span,
    });
}

/// Collects every `(format nil "~A"/"~S" x)` in one file, with the number of
/// `format` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no single-directive format-to-string
/// here" for Common Lisp and "nothing was looked for" for Clojure, and the two
/// read identically without the flag.
pub fn build_format_to_string_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FormatToStringItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("format_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut format_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut format_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("format_form_count", json!(format_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<FormatToStringItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_format_to_string_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build format to string report")
    }

    /// The `(format_form_count, violations)` pair the report is built from.
    fn formats(input: &str) -> (u64, Vec<FormatToStringItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "format_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("format_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_aesthetic_directive_as_princ_to_string() {
        let source = "(format nil \"~A\" value)";
        let (count, violations) = formats(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].replacement, "princ-to-string");
        assert_eq!(slice(source, violations[0].argument_span), "value");
    }

    #[test]
    fn flags_standard_directive_as_prin1_to_string() {
        let (_, violations) = formats("(format nil \"~S\" x)");
        assert_eq!(violations[0].replacement, "prin1-to-string");
    }

    #[test]
    fn folds_directive_case() {
        assert_eq!(
            formats("(format nil \"~a\" x)").1[0].replacement,
            "princ-to-string"
        );
        assert_eq!(
            formats("(format nil \"~s\" x)").1[0].replacement,
            "prin1-to-string"
        );
    }

    #[test]
    fn preserves_a_compound_argument() {
        let source = "(format nil \"~A\" (compute x))";
        let (_, violations) = formats(source);
        assert_eq!(slice(source, violations[0].argument_span), "(compute x)");
    }

    #[test]
    fn does_not_flag_surrounding_text() {
        assert!(formats("(format nil \"value: ~A\" x)").1.is_empty());
        assert!(formats("(format nil \"~A~%\" x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_non_nil_destination() {
        // t / stream destinations return nil, not the string.
        assert!(formats("(format t \"~A\" x)").1.is_empty());
        assert!(formats("(format s \"~A\" x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_argument_count() {
        assert!(formats("(format nil \"~A\")").1.is_empty());
        assert!(formats("(format nil \"~A\" a b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_other_directives() {
        assert!(formats("(format nil \"~D\" x)").1.is_empty());
        assert!(formats("(format nil \"~%\" x)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_format() {
        let (_, violations) = formats("(defun f (x) (format nil \"~A\" x))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(format nil \"~A\" x)", Dialect::Clojure)
            .expect("parse");
        let report = build_format_to_string_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build format to string report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("format_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(format t \"~A\" x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_replacement() {
        let source = "(defun f (x)\n  (format nil \"~S\" x))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "prin1-to-string");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("replacement", json!("prin1-to-string")),
                ("argument_span", span_json(finding.argument_span)),
            ]
        );
        assert_eq!(slice(source, finding.argument_span), "x");
        assert_eq!(finding.text_columns(), vec!["prin1-to-string".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_format_scanned_not_only_the_flagged_ones() {
        let report = report("(format nil \"~A\" x)\n(format nil \"~D\" y)\n");
        assert_eq!(report.summary, vec![("format_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
