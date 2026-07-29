//! Common Lisp `gethash`-explicit-`nil`-default detection: a three-argument
//! `(gethash key table nil)` whose default value is the literal `nil`. The
//! `default` argument of `gethash` *defaults to* `nil`, so `(gethash k h nil)` is
//! exactly `(gethash k h)` — same primary value and same second (present-p)
//! return value. The explicit `nil` restates the default and adds only noise.
//!
//! Only the bare literal `nil` in the default slot is matched. A non-`nil`
//! default (`(gethash k h 0)`) is meaningful and left alone, as is a
//! reader-conditional operand.
//!
//! The fix deletes the redundant ` nil` default argument (from the end of the
//! table operand through the `nil`), leaving the rest byte-identical, so the rule
//! is auto-fixable.
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

/// Whether `view` is the bare literal `nil` (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct GethashDefaultItem {
    /// The span of the whole `(gethash k h nil)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span to delete: the ` nil` default, from the end of the table operand
    /// through the `nil`.
    ///
    /// The rewrite's input, but the old report published it, so it stays on the
    /// report too: a consumer applying the edit itself needs the same bytes the
    /// fix does.
    pub removal_span: ByteSpan,
}

impl Finding for GethashDefaultItem {
    /// The rule's name. Every finding here is the one shape — a `gethash` whose
    /// third operand is the literal `nil` — so there is no closed set to
    /// discriminate on.
    fn kind(&self) -> &'static str {
        "gethash-default"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the path and location: the old text row carried only
    /// those. `message` is what a reader gets here.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "removal_span",
            json!({
                "start": self.removal_span.start().get(),
                "end": self.removal_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `gethash-default` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "the gethash default is nil; (gethash k h nil) is (gethash k h)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    gethash_form_count: &mut usize,
    violations: &mut Vec<GethashDefaultItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("gethash") {
        return;
    }
    *gethash_form_count += 1;

    // children: [gethash, key, table, default] — require the three-operand shape.
    if view.children.len() != 4 {
        return;
    }
    let key = &view.children[1];
    let table = &view.children[2];
    let default = &view.children[3];
    if is_reader_conditional(key) || is_reader_conditional(table) {
        return;
    }
    if !is_nil_literal(default) {
        return;
    }

    // Delete from the end of the table operand through the `nil`, so the leading
    // whitespace before `nil` goes too.
    let removal_span = ByteSpan::new(table.span.end(), default.span.end());
    violations.push(GethashDefaultItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        removal_span,
    });
}

/// Collects every `(gethash k h nil)` in one file, with the number of `gethash`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant default here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_gethash_default_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<GethashDefaultItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("gethash_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut gethash_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut gethash_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("gethash_form_count", json!(gethash_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<GethashDefaultItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_gethash_default_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build gethash default report")
    }

    /// The `(gethash_form_count, violations)` pair the report is built from.
    fn gethashes(input: &str) -> (u64, Vec<GethashDefaultItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "gethash_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("gethash_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_gethash_nil_default() {
        let source = "(gethash k table nil)";
        let (count, violations) = gethashes(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " nil");
    }

    #[test]
    fn does_not_flag_non_nil_default() {
        let (count, violations) = gethashes("(gethash k table 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_two_argument_gethash() {
        let (count, violations) = gethashes("(gethash k table)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_key_named_nil_literally() {
        // nil in the key or table slot is a real argument, not the default.
        let (_, violations) = gethashes("(gethash nil table)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = gethashes("(GETHASH k table NIL)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = gethashes("(defun f (k h) (gethash k h nil))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(gethash k h nil)", Dialect::Clojure).expect("parse");
        let report = build_gethash_default_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build gethash default report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("gethash_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(gethash k table)").dialect_modelled);
    }

    /// `removal_span` was on the old JSON, so it stays: it is what a consumer
    /// applying the edit itself needs.
    #[test]
    fn a_finding_carries_its_line_and_its_removal_span() {
        let report = report("(defun f (k h)\n  (gethash k h nil))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "gethash-default");
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
    fn the_summary_counts_every_gethash_scanned_not_only_the_flagged_ones() {
        let report = report("(gethash a h nil)\n(gethash b h)\n(gethash c h 0)\n");
        assert_eq!(report.summary, vec![("gethash_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
