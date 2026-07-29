//! Common Lisp duplicate-boolean-operand detection: an `and` or `or` form
//! that lists the same operand more than once — `(or x x)`, `(and a b a)`.
//! `and`/`or` are idempotent, so a repeated operand is pure redundancy: `(or
//! x x)` is just `x`, and `(and a b a)` is `(and a b)`. A repeat is usually a
//! copy-paste slip or a wrong operand that was meant to differ, and — for an
//! operand with a side effect — can even change behavior versus the intended
//! distinct test.
//!
//! Reuses the two shared expression primitives: the whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`] (an `and`/`or` nests
//! anywhere) and the reader-aware structural comparison from
//! [`paredit_core_syntax::expression_equality`], so `(or (p x) (p X))` counts as a
//! repeat (symbols fold case) while `(or (p x) (p y))` does not.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

const BOOLEAN_HEADS: [&str; 2] = ["and", "or"];

#[derive(Debug, Clone)]
pub struct DuplicateBooleanOperandItem {
    /// The span of the whole `(and …)`/`(or …)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator as it is spelled in source (`and` or `or`).
    pub head: String,
    /// The repeated operand, rendered from its first occurrence.
    pub operand: String,
    /// How many times it appears.
    pub occurrence_count: usize,
}

impl Finding for DuplicateBooleanOperandItem {
    /// The rule's own name. `and` and `or` are both idempotent, so a repeat is
    /// the same redundancy in either, and the operator that carried it stays in
    /// the JSON rather than splitting the kind.
    fn kind(&self) -> &'static str {
        "duplicate-boolean-operands"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("head={}", self.head),
            format!("operand={}", self.operand),
            format!("count={}", self.occurrence_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("operand", json!(self.operand)),
            ("occurrence_count", json!(self.occurrence_count)),
        ]
    }

    /// The same sentence the `duplicate-boolean-operands` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} repeats operand {} ({}×)",
            self.head, self.operand, self.occurrence_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_boolean(
    view: &ExpressionView,
    source: &str,
    boolean_form_count: &mut usize,
    duplicates: &mut Vec<DuplicateBooleanOperandItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !BOOLEAN_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *boolean_form_count += 1;

    // Operands are all children after the operator. Pairwise grouping by
    // structural equality — operand counts are small, so the quadratic scan
    // is cheaper than canonicalizing every operand.
    let operands: Vec<&ExpressionView> = view.children.iter().skip(1).collect();
    let mut grouped = vec![false; operands.len()];
    for anchor in 0..operands.len() {
        if grouped[anchor] {
            continue;
        }
        let mut occurrence_count = 1;
        for candidate in (anchor + 1)..operands.len() {
            if !grouped[candidate]
                && expressions_structurally_equal(operands[anchor], operands[candidate])
            {
                grouped[candidate] = true;
                occurrence_count += 1;
            }
        }
        if occurrence_count >= 2 {
            duplicates.push(DuplicateBooleanOperandItem {
                span: view.span,
                line: line_of(source, view.span.start().get()),
                head: head.to_owned(),
                operand: render_expression(operands[anchor]),
                occurrence_count,
            });
        }
    }
}

/// Collects every duplicated `and`/`or` operand in one file, with the number of
/// `and`/`or` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no repeated operand here" for Common
/// Lisp and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_duplicate_boolean_operand_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateBooleanOperandItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("boolean_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut boolean_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, source, &mut boolean_form_count, &mut duplicates);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        duplicates,
        vec![("boolean_form_count", json!(boolean_form_count))],
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

    fn report(input: &str) -> FileFindings<DuplicateBooleanOperandItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_duplicate_boolean_operand_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build duplicate boolean operand report")
    }

    /// The `(boolean_form_count, duplicates)` pair the report is built from.
    fn duplicates(input: &str) -> (u64, Vec<DuplicateBooleanOperandItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "boolean_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("boolean_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_repeated_operand_in_an_or() {
        let (boolean_form_count, duplicates) = duplicates("(or x y x)");
        assert_eq!(boolean_form_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].head, "or");
        assert_eq!(duplicates[0].operand, "x");
        assert_eq!(duplicates[0].occurrence_count, 2);
    }

    #[test]
    fn flags_a_repeated_structural_operand_in_an_and() {
        let (_, duplicates) = duplicates("(and (p x) (q y) (p x))");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].operand, "(p x)");
    }

    #[test]
    fn folds_symbol_case_across_operands() {
        let (_, duplicates) = duplicates("(or (p x) (p X))");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_operands() {
        let (boolean_form_count, duplicates) = duplicates("(and a b c)");
        assert_eq!(boolean_form_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn finds_a_boolean_nested_in_a_function_body() {
        let (boolean_form_count, duplicates) =
            duplicates("(defun f (x) (when (or (p x) (p x)) 1))");
        assert_eq!(boolean_form_count, 1);
        assert_eq!(duplicates.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(or x x)", Dialect::Clojure).expect("parse input");
        let report =
            build_duplicate_boolean_operand_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build duplicate boolean operand report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("boolean_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(and a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operand_and_its_count() {
        let report = report("(defun f (x)\n  (or x y x))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "duplicate-boolean-operands");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head", json!("or")),
                ("operand", json!("x")),
                ("occurrence_count", json!(2)),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "head=or".to_owned(),
                "operand=x".to_owned(),
                "count=2".to_owned()
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_boolean_scanned_not_only_the_flagged_ones() {
        let report = report("(or x x)\n(and a b)\n");
        assert_eq!(report.summary, vec![("boolean_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
