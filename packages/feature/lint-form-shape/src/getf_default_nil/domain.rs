//! Common Lisp `getf`-explicit-`nil`-default detection: a four-argument
//! `(getf plist indicator nil)` whose default value is the literal `nil`. The
//! `default` argument of `getf` *defaults to* `nil`, so `(getf p k nil)` is
//! exactly `(getf p k)` — same value when the indicator is absent. The explicit
//! `nil` restates the default and adds only noise. (Even in a `setf` place a
//! trailing `nil` default is ignored, so dropping it is safe there too.)
//!
//! Only the bare literal `nil` in the default slot is matched. A non-`nil`
//! default (`(getf p k 0)`) is meaningful and left alone, as is a
//! reader-conditional operand.
//!
//! The fix deletes the redundant ` nil` default argument (from the end of the
//! indicator operand through the `nil`), leaving the rest byte-identical, so the
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

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct GetfDefaultNilItem {
    /// The span of the whole `(getf …)` call form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span to delete: the trailing ` nil` default argument.
    ///
    /// The rewrite's input, but the old report published it, so it stays on the
    /// report too: a consumer applying the edit itself needs the same bytes the
    /// fix does.
    pub removal_span: ByteSpan,
}

impl Finding for GetfDefaultNilItem {
    /// The rule's name. Every finding here is the one shape — a `getf` whose
    /// fourth argument is the literal `nil` — so there is no closed set to
    /// discriminate on.
    fn kind(&self) -> &'static str {
        "getf-default-nil"
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

    /// The same sentence the `getf-default-nil` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "explicit nil default restates getf's default; (getf p k nil) is (getf p k)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    call_form_count: &mut usize,
    violations: &mut Vec<GetfDefaultNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("getf") {
        return;
    }
    *call_form_count += 1;

    // children: [getf, plist, indicator, nil] — exactly the explicit default.
    if view.children.len() != 4 {
        return;
    }
    if !is_nil_literal(&view.children[3]) {
        return;
    }
    let removal_span = ByteSpan::new(view.children[2].span.end(), view.children[3].span.end());
    violations.push(GetfDefaultNilItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        removal_span,
    });
}

/// Collects every `(getf plist indicator nil)` in one file, with the number of
/// `getf` calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant default here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_getf_default_nil_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<GetfDefaultNilItem>> {
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
            examine(subview, source, &mut call_form_count, &mut violations);
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

    fn report(input: &str) -> FileFindings<GetfDefaultNilItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_getf_default_nil_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build getf default nil report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<GetfDefaultNilItem>) {
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
    fn flags_explicit_nil_default() {
        let source = "(getf plist :key nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " nil");
    }

    #[test]
    fn does_not_flag_non_nil_default() {
        let (count, violations) = calls("(getf plist :key 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_getf() {
        let (_, violations) = calls("(getf plist :key)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(GETF plist :key nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (getf plist :key nil) (go))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(getf plist :key nil)", Dialect::Clojure)
            .expect("parse");
        let report = build_getf_default_nil_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build getf default nil report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(getf plist :key)").dialect_modelled);
    }

    /// `removal_span` was on the old JSON, so it stays: it is what a consumer
    /// applying the edit itself needs.
    #[test]
    fn a_finding_carries_its_line_and_its_removal_span() {
        let report = report("(defun f (plist)\n  (getf plist :key nil))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "getf-default-nil");
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
    fn the_summary_counts_every_getf_scanned_not_only_the_flagged_ones() {
        let report = report("(getf p :a nil)\n(getf p :b)\n(getf p :c 0)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
