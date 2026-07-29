//! Common Lisp dead-boolean-operand detection: an `and` whose non-final
//! operand is the literal `nil`, or an `or` whose non-final operand is the
//! literal `t`. Both operators short-circuit — `and` stops at the first
//! `nil`, `or` at the first non-`nil` — so every operand after that constant
//! can never be evaluated. `(and a nil b)` is just `(progn a nil)` with `b`
//! dead; `(or a t b)` is `(progn a t)` with `b` dead. Almost always the
//! constant is a leftover or a typo for a real test.
//!
//! Only a *non-final* constant is flagged, because a trailing constant is a
//! legitimate default: `(or x y t)` returns `t` when `x` and `y` are `nil`,
//! and `(and x y nil)` deliberately yields `nil`. The complementary
//! constants are pure redundancy rather than dead code — a non-final `t` in
//! `and` or `nil` in `or` contributes nothing but does not shadow later
//! operands — so this report leaves them to the redundancy-focused lints.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`], since an `and`/`or` can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only. The empty list `()` is treated as the `nil`
//! literal it reads as.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

fn is_nil_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
        || (is_paren_list(view) && view.children.is_empty())
}

fn is_t_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

#[derive(Debug, Clone)]
pub struct DeadBooleanOperandItem {
    /// The span of the whole `(and …)`/`(or …)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator as it is spelled in source (`and` or `or`).
    pub head: String,
    /// The literal that short-circuits it (`nil` for `and`, `t` for `or`).
    pub constant: String,
}

impl Finding for DeadBooleanOperandItem {
    /// The rule's own name. `and` short-circuiting at `nil` and `or`
    /// short-circuiting at `t` are the same defect under duality — every
    /// operand after the constant is unreachable — so splitting the kind by
    /// operator would offer a distinction without a difference. The operator
    /// and its constant stay in the JSON for anyone who wants them.
    fn kind(&self) -> &'static str {
        "dead-boolean-operand"
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
            format!("constant={}", self.constant),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("constant", json!(self.constant)),
        ]
    }

    /// The same sentence the `dead-boolean-operand` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} short-circuits at literal {}; later operands are dead",
            self.head, self.constant
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_boolean(
    view: &ExpressionView,
    source: &str,
    boolean_form_count: &mut usize,
    violations: &mut Vec<DeadBooleanOperandItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    // The constant whose appearance short-circuits this operator: `nil` for
    // `and`, `t` for `or`.
    let (constant, is_short_circuit): (&str, fn(&ExpressionView) -> bool) =
        if head.eq_ignore_ascii_case("and") {
            ("nil", is_nil_literal)
        } else if head.eq_ignore_ascii_case("or") {
            ("t", is_t_literal)
        } else {
            return;
        };
    *boolean_form_count += 1;

    let operands = &view.children[1..];
    // A non-final operand is any but the last; if the short-circuiting
    // constant appears there, later operands are unreachable.
    let last_index = operands.len().saturating_sub(1);
    if operands.iter().take(last_index).any(is_short_circuit) {
        violations.push(DeadBooleanOperandItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            head: head.to_owned(),
            constant: constant.to_owned(),
        });
    }
}

/// Collects every `and`/`or` with a short-circuiting non-final constant in one
/// file, with the number of `and`/`or` forms scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no dead operand here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_dead_boolean_operand_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DeadBooleanOperandItem>> {
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
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, source, &mut boolean_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
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

    fn report(input: &str) -> FileFindings<DeadBooleanOperandItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_dead_boolean_operand_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build dead boolean operand report")
    }

    /// The `(boolean_form_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<DeadBooleanOperandItem>) {
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
    fn flags_a_non_final_nil_in_and() {
        let (boolean_form_count, violations) = violations("(and a nil b)");
        assert_eq!(boolean_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "and");
        assert_eq!(violations[0].constant, "nil");
    }

    #[test]
    fn flags_a_non_final_t_in_or() {
        let (_, violations) = violations("(or a t b)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "or");
        assert_eq!(violations[0].constant, "t");
    }

    #[test]
    fn does_not_flag_a_trailing_t_in_or_default_idiom() {
        let (boolean_form_count, violations) = violations("(or x y t)");
        assert_eq!(boolean_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_trailing_nil_in_and() {
        let (_, violations) = violations("(and x y nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_final_t_in_and_which_is_only_redundant() {
        let (_, violations) = violations("(and a t b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn treats_empty_list_as_nil_in_and() {
        let (_, violations) = violations("(and a () b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_boolean_nested_in_a_function_body() {
        let (boolean_form_count, violations) = violations("(defun f (a b) (when (and a nil b) 1))");
        assert_eq!(boolean_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(and a nil b)", Dialect::Clojure).expect("parse input");
        let report =
            build_dead_boolean_operand_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build dead boolean operand report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("boolean_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(and a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_and_its_constant() {
        let report = report("(defun f (a b)\n  (or a t b))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "dead-boolean-operand");
        assert_eq!(
            finding.json_fields(),
            vec![("head", json!("or")), ("constant", json!("t"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["head=or".to_owned(), "constant=t".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_boolean_scanned_not_only_the_flagged_ones() {
        let report = report("(and a nil b)\n(or x y)\n");
        assert_eq!(report.summary, vec![("boolean_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
