//! Common Lisp double-`reverse` detection: a `(reverse (reverse x))`. `reverse`
//! returns a fresh sequence of the same kind as its argument with the elements
//! in opposite order; reversing that fresh sequence again restores the original
//! order, yielding a fresh sequence of `x`'s kind whose elements equal `x`'s.
//! That is exactly `(copy-seq x)` — a wasteful, obfuscated copy.
//!
//! Only the non-destructive `reverse` is matched on *both* levels. The
//! destructive `nreverse` is deliberately excluded: `(nreverse (nreverse x))`
//! mutates `x`'s structure twice and cannot be reasoned about as a plain copy,
//! and a mixed `reverse`/`nreverse` nesting is a common deliberate
//! build-then-nreverse idiom, so neither is flagged. A reader-conditional inner
//! operand is left alone (build-dependent).
//!
//! The fix rewrites `(reverse (reverse x))` as `(copy-seq x)`, copying the inner
//! argument's source verbatim, so the rule is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether `view` is a `(reverse arg)` call with exactly one operand; returns the
/// operand.
fn single_arg_reverse(view: &ExpressionView) -> Option<&ExpressionView> {
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("reverse") {
        return None;
    }
    // children: [reverse, arg] — exactly one operand.
    if view.children.len() != 2 {
        return None;
    }
    Some(&view.children[1])
}

#[derive(Debug, Clone)]
pub struct DoubleReverseItem {
    /// The span of the whole `(reverse (reverse x))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the innermost argument `x` (for reconstructing the fix).
    ///
    /// The rewrite's input, but the old report published it and a consumer
    /// splicing its own `(copy-seq …)` needs it, so it stays on the report.
    pub inner_span: ByteSpan,
}

impl Finding for DoubleReverseItem {
    /// The rule's own name. A double `reverse` has no sub-classes — every
    /// finding is the same collapse to `copy-seq` — so there is nothing for a
    /// per-finding discriminator to say.
    fn kind(&self) -> &'static str {
        "double-reverse"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the path and line the envelope already prints: the old
    /// text row carried exactly those two. `message` carries the description.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("inner_span", span_json(self.inner_span))]
    }

    /// The same sentence the `double-reverse` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "(reverse (reverse x)) is a wasteful copy; use (copy-seq x)".to_owned()
    }
}

/// A sub-span in the shape the old hand-written report published it.
fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    reverse_form_count: &mut usize,
    violations: &mut Vec<DoubleReverseItem>,
) {
    let Some(outer_arg) = single_arg_reverse(view) else {
        return;
    };
    *reverse_form_count += 1;

    // The single argument must itself be a `(reverse x)` call.
    if !is_paren_list(outer_arg) {
        return;
    }
    let Some(inner_arg) = single_arg_reverse(outer_arg) else {
        return;
    };
    if is_reader_conditional(inner_arg) {
        return;
    }

    violations.push(DoubleReverseItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        inner_span: inner_arg.span,
    });
}

/// Collects every `(reverse (reverse x))` in one file, with the number of
/// single-argument `reverse` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no double reverse here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_double_reverse_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DoubleReverseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("reverse_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut reverse_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut reverse_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("reverse_form_count", json!(reverse_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DoubleReverseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_double_reverse_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build double reverse report")
    }

    /// The `(reverse_form_count, violations)` pair the report is built from.
    fn reverses(input: &str) -> (u64, Vec<DoubleReverseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "reverse_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("reverse_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_double_reverse() {
        let source = "(reverse (reverse xs))";
        let (_, violations) = reverses(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].inner_span), "xs");
    }

    #[test]
    fn preserves_a_compound_inner_argument() {
        let source = "(reverse (reverse (mapcar #'f ys)))";
        let (_, violations) = reverses(source);
        assert_eq!(slice(source, violations[0].inner_span), "(mapcar #'f ys)");
    }

    #[test]
    fn does_not_flag_a_single_reverse() {
        let (count, violations) = reverses("(reverse xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_nreverse() {
        // Destructive nreverse cannot be reasoned about as a plain copy.
        assert!(reverses("(nreverse (nreverse xs))").1.is_empty());
        assert!(reverses("(reverse (nreverse xs))").1.is_empty());
        assert!(reverses("(nreverse (reverse xs))").1.is_empty());
    }

    #[test]
    fn does_not_flag_reverse_of_other_call() {
        let (_, violations) = reverses("(reverse (sort xs #'<))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = reverses("(REVERSE (REVERSE xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = reverses("(defun f (xs) (reverse (reverse xs)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(reverse (reverse xs))", Dialect::Clojure)
            .expect("parse");
        let report = build_double_reverse_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build double reverse report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("reverse_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(reverse xs)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_the_inner_span_the_fix_needs() {
        let source = "(defun f (xs)\n  (reverse (reverse xs)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "double-reverse");
        assert_eq!(slice(source, finding.inner_span), "xs");
        assert_eq!(
            finding.json_fields(),
            vec![("inner_span", span_json(finding.inner_span))]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_reverse_scanned_not_only_the_flagged_ones() {
        let report = report("(reverse (reverse xs))\n(reverse ys)\n");
        // Three: the outer and inner reverse of the flagged pair, plus the
        // lone one.
        assert_eq!(report.summary, vec![("reverse_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
