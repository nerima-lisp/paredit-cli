//! Common Lisp `format`-newline detection: a `(format t "~%")` whose control
//! string is exactly the single newline directive `~%` and whose destination is
//! `t`. `format` with destination `t` writes to `*standard-output*` and returns
//! `nil`; `~%` unconditionally emits one `#\Newline`. That is exactly what
//! `(terpri)` does — write one newline to `*standard-output*` and return `nil` —
//! so `(format t "~%")` is `(terpri)`, stated directly and without a control
//! string to parse at run time.
//!
//! The rule is deliberately narrow for soundness:
//!
//!   - Only the `t` destination is matched. `t` always denotes
//!     `*standard-output*` (a stream), so `(terpri)` (also `*standard-output*`)
//!     is an exact match. An arbitrary destination expression could be a
//!     string with a fill pointer — a valid `format` destination but *not* a
//!     valid `terpri` argument — so a non-`t` destination is left alone.
//!   - Only `~%` (unconditional newline) is matched, never `~&` (`fresh-line`):
//!     `fresh-line` returns a generalized boolean, whereas `(format t "~&")`
//!     returns `nil`, so that rewrite would not preserve the return value.
//!   - The call must have no format arguments (exactly `format`, `t`, control);
//!     `format` evaluates every argument, so a trailing argument's evaluation
//!     would be lost by `(terpri)`.
//!
//! The fix rewrites the call as `(terpri)`, so the rule is auto-fixable.
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

/// Whether `view` is the bare literal `t` destination (no reader prefixes).
fn is_t_destination(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// Whether the control-string atom's source is exactly the `~%` directive.
/// `text` includes the surrounding quotes.
fn is_newline_control(text: &str) -> bool {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .is_some_and(|inner| inner == "~%")
}

#[derive(Debug, Clone)]
pub struct FormatNewlineItem {
    /// The span of the whole `(format t "~%")` form.
    pub span: ByteSpan,
}

impl Finding for FormatNewlineItem {
    /// The rule's own name: only the exact `(format t "~%")` shape is matched,
    /// so every finding is the same call.
    fn kind(&self) -> &'static str {
        "format-newline"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// Nothing beyond the path, line, and span: the matched form is fixed, so
    /// there is no per-finding detail left to name.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `format-newline` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "(format t \"~%\") just writes a newline; use (terpri)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    format_form_count: &mut usize,
    violations: &mut Vec<FormatNewlineItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("format") {
        return;
    }
    *format_form_count += 1;

    // children: [format, destination, control] — no format arguments.
    if view.children.len() != 3 {
        return;
    }
    let destination = &view.children[1];
    let control = &view.children[2];
    if !is_t_destination(destination) {
        return;
    }
    if !atom_text(control).is_some_and(is_newline_control) {
        return;
    }

    violations.push(FormatNewlineItem { span: view.span });
}

/// Collects every `(format t "~%")` in one file, with the number of `format`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no bare newline format here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_format_newline_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<FormatNewlineItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("format_form_count", json!(0))],
        ));
    }

    let mut format_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut format_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("format_form_count", json!(format_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<FormatNewlineItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_format_newline_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build format newline report")
    }

    /// The `(format_form_count, violations)` pair the report is built from.
    fn formats(input: &str) -> (u64, Vec<FormatNewlineItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "format_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("format_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_format_t_newline() {
        let source = "(format t \"~%\")";
        let (count, violations) = formats(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn folds_head_case() {
        let (_, violations) = formats("(FORMAT T \"~%\")");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_nil_destination() {
        // (format nil "~%") returns the newline string, not terpri's nil.
        assert!(formats("(format nil \"~%\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_stream_destination() {
        // A stream/fill-pointer-string destination is not a valid terpri arg.
        assert!(formats("(format s \"~%\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_fresh_line() {
        // ~& is fresh-line, whose return value differs from format's nil.
        assert!(formats("(format t \"~&\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_control_with_extra_text() {
        assert!(formats("(format t \"~%~%\")").1.is_empty());
        assert!(formats("(format t \"done~%\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_call_with_arguments() {
        // format evaluates every argument; terpri would drop x's evaluation.
        assert!(formats("(format t \"~%\" x)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_format() {
        let (_, violations) = formats("(defun f () (format t \"~%\"))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(format t \"~%\")", Dialect::Clojure).expect("parse");
        let report = build_format_newline_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build format newline report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("format_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(format t \"~a\" x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line() {
        let report = report("(defun f ()\n  (format t \"~%\"))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "format-newline");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_format_scanned_not_only_the_flagged_ones() {
        let report = report("(format t \"~%\")\n(format t \"~a\" x)\n");
        assert_eq!(report.summary, vec![("format_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
