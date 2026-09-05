//! Common Lisp `coerce`-to-`t` detection: a `(coerce x t)`. Per CLHS, coercing
//! an object to type `t` returns the object unchanged — every object is of type
//! `t` — so `(coerce x t)` is exactly `x`, evaluated once. The wrapper is pure
//! noise (often a leftover after the target type was edited to `t`).
//!
//! Only the bare literal `t` result type is matched. A real coercion
//! (`(coerce x 'list)`, `(coerce x 'double-float)`) is meaningful and left alone,
//! as is a computed type argument and a reader-conditional `x`.
//!
//! The fix replaces the whole form with its object operand's source, so the rule
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

/// Whether `view` is the bare literal `t` result type (no reader prefixes).
fn is_t_type(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct CoerceToTItem {
    /// The span of the whole `(coerce x t)` form.
    pub span: ByteSpan,
    /// The span of the object operand `x`.
    ///
    /// The rewrite's input, and also published: the old report printed it, and
    /// a caller unwrapping the form itself has no other way to locate the
    /// operand to keep.
    pub object_span: ByteSpan,
}

impl Finding for CoerceToTItem {
    /// The rule's own name. Every finding here is the same defect — a coercion
    /// to `t`, which is not a coercion — with nothing to sort it by.
    fn kind(&self) -> &'static str {
        "coerce-to-t"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the path and line the envelope already prints: the old
    /// text row carried exactly those.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "object_span",
            json!({
                "start": self.object_span.start().get(),
                "end": self.object_span.end().get(),
            }),
        )]
    }

    fn message(&self) -> String {
        "coerce to type t returns the object unchanged; (coerce x t) is x".to_owned()
    }
}

pub fn examine(
    view: &ExpressionView,
    coerce_form_count: &mut usize,
    violations: &mut Vec<CoerceToTItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("coerce") {
        return;
    }
    *coerce_form_count += 1;

    // children: [coerce, object, result-type] — require exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let object = &view.children[1];
    let result_type = &view.children[2];
    if is_reader_conditional(object) {
        return;
    }
    if !is_t_type(result_type) {
        return;
    }

    violations.push(CoerceToTItem {
        span: view.span,
        object_span: object.span,
    });
}

/// Collects every `(coerce x t)` in one file, with the number of `coerce` forms
/// scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_coerce_to_t_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CoerceToTItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("coerce_form_count", json!(0))],
        ));
    }

    let mut coerce_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut coerce_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("coerce_form_count", json!(coerce_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CoerceToTItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_coerce_to_t_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build coerce to t report")
    }

    fn coerces(input: &str) -> (u64, Vec<CoerceToTItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "coerce_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("coerce_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_coerce_to_t() {
        let source = "(coerce value t)";
        let (count, violations) = coerces(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].object_span), "value");
    }

    #[test]
    fn preserves_compound_object() {
        let source = "(coerce (compute x) t)";
        let (_, violations) = coerces(source);
        assert_eq!(slice(source, violations[0].object_span), "(compute x)");
    }

    #[test]
    fn does_not_flag_real_coercion() {
        assert!(coerces("(coerce x 'list)").1.is_empty());
        assert!(coerces("(coerce x 'double-float)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_arity() {
        assert!(coerces("(coerce x)").1.is_empty());
    }

    #[test]
    fn case_folds_head_and_type() {
        let (_, violations) = coerces("(COERCE x T)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = coerces("(defun f (x) (coerce x t))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(coerce x t)", Dialect::Clojure).expect("parse");
        let report = build_coerce_to_t_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build coerce to t report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("coerce_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(coerce x 'list)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_object_span() {
        let report = report("(defun f (x)\n  (coerce x t))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "coerce-to-t");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![(
                "object_span",
                json!({
                    "start": finding.object_span.start().get(),
                    "end": finding.object_span.end().get(),
                })
            )]
        );
    }

    #[test]
    fn the_summary_counts_every_coerce_scanned_not_only_the_flagged_ones() {
        let report = report("(coerce x t)\n(coerce y 'list)\n(coerce z t)\n");
        assert_eq!(report.summary, vec![("coerce_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
