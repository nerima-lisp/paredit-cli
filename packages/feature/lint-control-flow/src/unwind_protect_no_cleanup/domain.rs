//! Common Lisp cleanupless-`unwind-protect` detection: an `(unwind-protect x)`
//! with no cleanup forms. `unwind-protect` evaluates the protected form and then
//! runs its cleanup forms on any exit; with zero cleanup forms there is nothing
//! to protect, so it is exactly `x` — a wrapper left behind (often after the
//! cleanup forms were deleted).
//!
//! Only the exact no-cleanup shape `(unwind-protect x)` is matched; an
//! `unwind-protect` with any cleanup form is a real construct and left alone, as
//! is a reader-conditional protected form.
//!
//! The fix replaces the whole form with the protected form's source, so the rule
//! is auto-fixable.
//!
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
pub struct UnwindProtectNoCleanupItem {
    /// The span of the whole `(unwind-protect x)` form.
    pub span: ByteSpan,
    /// The span of the protected form `x` (for reconstructing the fix).
    pub form_span: ByteSpan,
}

impl Finding for UnwindProtectNoCleanupItem {
    fn kind(&self) -> &'static str {
        "unwind-protect-no-cleanup"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the leading `kind`, path and line: this report's text
    /// rows have never carried a column of their own.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// `form_span` is the fix's input, but the JSON has always published it —
    /// it is how a consumer locates the protected form the wrapper collapses
    /// to — so it stays on the report.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "form_span",
            json!({
                "start": self.form_span.start().get(),
                "end": self.form_span.end().get(),
            }),
        )]
    }

    fn message(&self) -> String {
        "an unwind-protect with no cleanup is just its body; (unwind-protect x) is x".to_owned()
    }
}

pub fn examine(
    view: &ExpressionView,
    unwind_protect_form_count: &mut usize,
    violations: &mut Vec<UnwindProtectNoCleanupItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("unwind-protect") {
        return;
    }
    *unwind_protect_form_count += 1;

    // children: [unwind-protect, protected] — no cleanup forms.
    if view.children.len() != 2 {
        return;
    }
    let form = &view.children[1];
    if is_reader_conditional(form) {
        return;
    }

    violations.push(UnwindProtectNoCleanupItem {
        span: view.span,
        form_span: form.span,
    });
}

/// Collects every cleanupless `(unwind-protect x)` in one file, with the number
/// of `unwind-protect` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_unwind_protect_no_cleanup_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<UnwindProtectNoCleanupItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("unwind_protect_form_count", json!(0))],
        ));
    }

    let mut unwind_protect_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut unwind_protect_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![(
            "unwind_protect_form_count",
            json!(unwind_protect_form_count),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<UnwindProtectNoCleanupItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_unwind_protect_no_cleanup_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build unwind-protect no cleanup report")
    }

    /// The `(unwind_protect_form_count, violations)` pair the report is built
    /// from.
    fn ups(input: &str) -> (u64, Vec<UnwindProtectNoCleanupItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "unwind_protect_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("unwind_protect_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_cleanupless_unwind_protect() {
        let source = "(unwind-protect (compute))";
        let (count, violations) = ups(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn does_not_flag_unwind_protect_with_cleanup() {
        assert!(ups("(unwind-protect (compute) (cleanup))").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = ups("(UNWIND-PROTECT x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = ups("(defun f (x) (unwind-protect x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(unwind-protect x)", Dialect::Clojure).expect("parse");
        let report =
            build_unwind_protect_no_cleanup_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build unwind-protect no cleanup report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("unwind_protect_form_count", json!(0))]
        );
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(unwind-protect x (cleanup))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_form_span() {
        let report = report("(defun f (x)\n  (unwind-protect x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "unwind-protect-no-cleanup");
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
    fn the_summary_counts_every_unwind_protect_scanned_not_only_the_flagged_ones() {
        let report = report("(unwind-protect x)\n(unwind-protect y (cleanup))\n");
        assert_eq!(
            report.summary,
            vec![("unwind_protect_form_count", json!(2))]
        );
        assert_eq!(report.findings.len(), 1);
    }
}
