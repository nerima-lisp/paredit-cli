//! Common Lisp `subseq`-zero detection: a two-argument `(subseq seq 0)` whose
//! start index is the literal `0` and which has no end argument. `subseq` with
//! start `0` and no end returns a fresh subsequence spanning the whole
//! sequence — that is exactly `(copy-seq seq)`, a fresh copy of the same kind.
//! `copy-seq` states the intent (copy) directly and skips the bounds arithmetic.
//!
//! Only the bare integer literal `0` is matched, and only the two-argument shape
//! (no `end` operand). A non-`0` start, a float `0.0`, a `#x0`/prefixed
//! spelling, a variable start, a present `end` argument (`(subseq seq 0 n)` is a
//! genuine slice), and a reader-conditional operand are all left alone.
//!
//! The fix rewrites `(subseq seq 0)` as `(copy-seq seq)`, copying the sequence
//! operand from its exact source, so the rule is auto-fixable.
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

/// Whether `view` is the bare integer `0` literal (no reader prefixes, so `#x0`
/// and a prefixed `,0` are excluded; `0.0` is a different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SubseqZeroItem {
    /// The span of the whole `(subseq seq 0)` form.
    pub span: ByteSpan,
    /// The span of the sequence operand (for reconstructing the fix).
    pub sequence_span: ByteSpan,
}

impl Finding for SubseqZeroItem {
    /// One tag for every finding: this report has a single shape to describe, a
    /// whole-sequence copy written as a slice from index 0.
    fn kind(&self) -> &'static str {
        "subseq-zero"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// None: the old text row carried the path and the offset, both of which
    /// the envelope prints itself.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// `sequence_span` is the rewrite's input, but the old JSON published it,
    /// so a consumer reconstructing `(copy-seq …)` from this report keeps it.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "sequence_span",
            json!({
                "start": self.sequence_span.start().get(),
                "end": self.sequence_span.end().get(),
            }),
        )]
    }

    fn message(&self) -> String {
        "a subseq from index 0 copies the whole sequence; (subseq seq 0) is (copy-seq seq)"
            .to_owned()
    }
}

pub fn examine(
    view: &ExpressionView,
    subseq_form_count: &mut usize,
    violations: &mut Vec<SubseqZeroItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("subseq") {
        return;
    }
    *subseq_form_count += 1;

    // children: [subseq, sequence, start] — require exactly the no-end shape.
    if view.children.len() != 3 {
        return;
    }
    let sequence = &view.children[1];
    let start = &view.children[2];
    if is_reader_conditional(sequence) || is_reader_conditional(start) {
        return;
    }
    if !is_zero_literal(start) {
        return;
    }

    violations.push(SubseqZeroItem {
        span: view.span,
        sequence_span: sequence.span,
    });
}

/// Collects every `(subseq seq 0)` in one file, with the number of `subseq`
/// forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_subseq_zero_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SubseqZeroItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("subseq_form_count", json!(0))],
        ));
    }

    let mut subseq_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut subseq_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("subseq_form_count", json!(subseq_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SubseqZeroItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_subseq_zero_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build subseq zero report")
    }

    fn subseqs(input: &str) -> (u64, Vec<SubseqZeroItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "subseq_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("subseq_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_subseq_zero() {
        let source = "(subseq items 0)";
        let (count, violations) = subseqs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].sequence_span), "items");
    }

    #[test]
    fn preserves_a_compound_sequence() {
        let source = "(subseq (rest xs) 0)";
        let (_, violations) = subseqs(source);
        assert_eq!(slice(source, violations[0].sequence_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_with_end_argument() {
        // (subseq seq 0 n) is a genuine slice, not a whole-sequence copy.
        let (count, violations) = subseqs("(subseq seq 0 5)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_nonzero_start() {
        let (_, violations) = subseqs("(subseq seq 1)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_zero() {
        assert!(subseqs("(subseq seq 0.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_variable_start() {
        assert!(subseqs("(subseq seq n)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = subseqs("(SUBSEQ x 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = subseqs("(defun f (x) (subseq x 0))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(subseq x 0)", Dialect::Clojure).expect("parse");
        let report = build_subseq_zero_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build subseq zero report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("subseq_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(subseq x 1)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_sequence_span() {
        let report = report("(defun f (x)\n  (subseq x 0))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "subseq-zero");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "sequence_span",
                json!({
                    "start": finding.sequence_span.start().get(),
                    "end": finding.sequence_span.end().get(),
                })
            )]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_subseq_scanned_not_only_the_flagged_ones() {
        let report = report("(subseq x 0)\n(subseq x 0 5)\n(subseq y 0)\n");
        assert_eq!(report.summary, vec![("subseq_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
