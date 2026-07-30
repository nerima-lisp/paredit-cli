//! Common Lisp `append`-with-a-trailing-`nil` detection: a two-argument
//! `(append x nil)`. `append` copies every argument but the last and links the
//! copy's final `cdr` to the last argument; with the last argument `nil`, the
//! result is a fresh top-level copy of `x` — exactly `(copy-seq`… no: for a list
//! it is `(copy-list x)`. Both `append` (whose non-last argument must be a proper
//! list) and `copy-list` (which requires a proper list) reject a non-list `x`
//! identically, so the rewrite preserves the type-check domain as well as the
//! value.
//!
//! Only the two-argument `(append x nil)` shape with a bare literal `nil` tail is
//! matched. A non-`nil` tail, more than two arguments, and a reader-conditional
//! operand are left alone. (`(nconc x nil)` is deliberately *not* handled by an
//! analogous rule: it would rewrite to bare `x`, which — unlike `nconc` — accepts
//! a non-list, silently dropping a type check.)
//!
//! The fix rewrites `(append x nil)` as `(copy-list x)`, copying `x`'s source, so
//! the rule is auto-fixable.
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
pub struct AppendNilItem {
    /// The span of the whole `(append x nil)` form.
    pub span: ByteSpan,
    /// The span of the list operand `x`.
    pub list_span: ByteSpan,
}

impl Finding for AppendNilItem {
    fn kind(&self) -> &'static str {
        "append-nil"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing: the old text row carried only the path and the offset, both of
    /// which the envelope prints itself. The `message` override is what a
    /// reader of a text row has to go on.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// The list operand's span, which this report published before it moved
    /// onto the envelope. It is also the rewrite's input, but a consumer that
    /// highlights the copied list rather than the whole call already reads it.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("list_span", span_json(self.list_span))]
    }

    /// The same sentence the `append-nil` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "append with a nil tail is a fresh copy; (append x nil) is (copy-list x)".to_owned()
    }
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    append_form_count: &mut usize,
    violations: &mut Vec<AppendNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("append") {
        return;
    }
    *append_form_count += 1;

    // children: [append, x, nil] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let list = &view.children[1];
    let tail = &view.children[2];
    if is_reader_conditional(list) {
        return;
    }
    if !is_nil_literal(tail) {
        return;
    }

    violations.push(AppendNilItem {
        span: view.span,
        list_span: list.span,
    });
}

/// Collects every `(append x nil)` in one file, with the number of `append`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no nil-tailed append here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn collect_append_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AppendNilItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("append_form_count", json!(0))],
        ));
    }

    let mut append_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut append_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("append_form_count", json!(append_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AppendNilItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_append_nils(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect append nils")
    }

    /// The `(append_form_count, violations)` pair the report is built from.
    fn appends(input: &str) -> (u64, Vec<AppendNilItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "append_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("append_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_append_nil() {
        let source = "(append xs nil)";
        let (count, violations) = appends(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].list_span), "xs");
    }

    #[test]
    fn preserves_compound_list() {
        let source = "(append (mapcar #'f ys) nil)";
        let (_, violations) = appends(source);
        assert_eq!(slice(source, violations[0].list_span), "(mapcar #'f ys)");
    }

    #[test]
    fn does_not_flag_non_nil_tail() {
        let (_, violations) = appends("(append xs ys)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_leading_nil() {
        // (append nil xs) is a different shape (nil is not the last argument).
        let (_, violations) = appends("(append nil xs)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_arguments() {
        assert!(appends("(append xs ys nil)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = appends("(APPEND xs NIL)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = appends("(defun f (xs) (append xs nil))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(append xs nil)", Dialect::Clojure).expect("parse");
        let report = collect_append_nils(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect append nils");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("append_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(append xs ys)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_list_span() {
        let report = report("(defun f (xs)\n  (append xs nil))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "append-nil");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![("list_span", span_json(finding.list_span))]
        );
    }

    #[test]
    fn the_summary_counts_every_append_scanned_not_only_the_flagged_ones() {
        let report = report("(append xs nil)\n(append xs ys)\n");
        assert_eq!(report.summary, vec![("append_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
