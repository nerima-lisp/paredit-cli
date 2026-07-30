//! Common Lisp missing-`format`-destination detection: a `(format …)` call
//! whose first argument is a *string literal*. `format`'s first argument is the
//! destination — `nil` (return a string), `t` (write to `*standard-output*`),
//! or a stream — and the control string is the *second*. So `(format "~a" x)`
//! quietly uses the control string `"~a"` as the destination and `x` as the
//! control string, which is never what the author meant: a literal string can
//! never be a valid destination (that needs a string with a fill pointer, never
//! a constant). The fix is a missing `nil`/`t`, but no compiler flags the call
//! because it is technically well-formed.
//!
//! Only a string *literal* in the destination slot is flagged. A `nil`/`t`
//! symbol, a stream variable, or a `(make-…-stream)` form there is a correct
//! destination and is left alone.
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
use paredit_core_syntax::view_query::{atom_child, for_each_subview, list_head};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct FormatMissingDestinationItem {
    /// The span of the whole `(format …)` form.
    pub span: ByteSpan,
    /// The string literal found in the destination slot (its source text).
    pub literal: String,
}

impl Finding for FormatMissingDestinationItem {
    /// The rule's own name: only a string literal in the destination slot is
    /// matched, so every finding is the same mistake.
    fn kind(&self) -> &'static str {
        "format-missing-destination"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("literal={}", self.literal)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("literal", json!(self.literal))]
    }

    /// The same sentence the `format-missing-destination` lint rule writes, so
    /// a SARIF or JUnit consumer reading both sees one finding described one
    /// way.
    fn message(&self) -> String {
        format!(
            "format destination is the string literal {}; a nil/t/stream destination is missing",
            self.literal
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_format(
    view: &ExpressionView,
    format_call_count: &mut usize,
    violations: &mut Vec<FormatMissingDestinationItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("format")) {
        return;
    }
    *format_call_count += 1;

    // children[0] is `format`; children[1] is the destination argument.
    let Some(destination) = atom_child(view, 1) else {
        return;
    };
    if destination.starts_with('"') {
        violations.push(FormatMissingDestinationItem {
            span: view.span,
            literal: destination.to_owned(),
        });
    }
}

/// Collects every `format` call whose destination slot holds a string literal
/// in one file, with the number of `format` calls scanned as the denominator
/// beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every format call here names a
/// destination" for Common Lisp and "nothing was looked for" for Clojure, and
/// the two read identically without the flag.
pub fn build_format_missing_destination_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FormatMissingDestinationItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("format_call_count", json!(0))],
        ));
    }

    let mut format_call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_format(subview, &mut format_call_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("format_call_count", json!(format_call_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<FormatMissingDestinationItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_format_missing_destination_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build format missing destination report")
    }

    /// The `(format_call_count, violations)` pair the report is built from.
    fn formats(input: &str) -> (u64, Vec<FormatMissingDestinationItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "format_call_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("format_call_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_string_literal_destination() {
        let (count, violations) = formats("(format \"~a~%\" x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal, "\"~a~%\"");
    }

    #[test]
    fn does_not_flag_nil_destination() {
        let (count, violations) = formats("(format nil \"~a\" x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_t_destination() {
        let (_, violations) = formats("(format t \"~a\" x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_stream_variable_destination() {
        let (_, violations) = formats("(format stream \"~a\" x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_stream_form_destination() {
        let (_, violations) = formats("(format (make-string-output-stream) \"~a\")");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = formats("(FORMAT \"done\")");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_a_nested_format() {
        let (_, violations) = formats("(when ready (format \"~a\" x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_non_format_heads() {
        let (count, violations) = formats("(list \"~a\" x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_format_with_no_arguments() {
        // Degenerate `(format)` has no destination slot to inspect.
        let (count, violations) = formats("(format)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(format \"~a\" x)", Dialect::Clojure).expect("parse");
        let report =
            build_format_missing_destination_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build format missing destination report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("format_call_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(format t \"~a\" x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_literal() {
        let report = report("(defun f (x)\n  (format \"~a~%\" x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "format-missing-destination");
        assert_eq!(finding.json_fields(), vec![("literal", json!("\"~a~%\""))]);
        assert_eq!(finding.text_columns(), vec!["literal=\"~a~%\"".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_format_call_scanned_not_only_the_flagged_ones() {
        let report = report("(format \"~a\" x)\n(format t \"~a\" y)\n");
        assert_eq!(report.summary, vec![("format_call_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
