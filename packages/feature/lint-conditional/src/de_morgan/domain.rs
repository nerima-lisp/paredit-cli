//! Common Lisp De Morgan detection: an `and`/`or` all of whose operands are
//! negations, which collapses to a single outer negation —
//! `(and (not a) (not b))` is `(not (or a b))`, `(or (not a) (not b))` is
//! `(not (and a b))`. This trades N inner `not`s for one outer `not` and often
//! reads more directly ("none of these" / "not all of these").
//!
//! The rewrite is exact down to short-circuit behavior: `(and (not a) (not b))`
//! skips evaluating `b` exactly when `a` is true, and `(not (or a b))` skips `b`
//! exactly when `a` is truthy — the same operand is elided in the same case, and
//! both forms yield a canonical `t`/`nil`.
//!
//! Every operand must be a single-argument `not`/`null` form, and there must be
//! at least two of them (a one-operand `and`/`or` is `single-operand-boolean`'s
//! job). If any operand is not a negation, the collapse does not apply and the
//! form is left alone; a reader-conditional operand is also exempt.
//!
//! The fix rewrites the whole form as `(not (OPPOSITE inner…))`, copying each
//! negation's inner operand from source, so the rule is auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The opposite boolean operator (`and`↔`or`) for a boolean head, or `None`
/// otherwise.
fn opposite_operator(head: &str) -> Option<(&'static str, &'static str)> {
    if head.eq_ignore_ascii_case("and") {
        Some(("and", "or"))
    } else if head.eq_ignore_ascii_case("or") {
        Some(("or", "and"))
    } else {
        None
    }
}

/// The inner operand `X` of a single-argument `(not X)`/`(null X)`, or `None`.
fn negation_inner(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("not") && !head.eq_ignore_ascii_case("null") {
        return None;
    }
    (view.children.len() == 2).then(|| &view.children[1])
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct DeMorganItem {
    /// The span of the whole `(and …)`/`(or …)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`and` or `or`).
    pub operator: &'static str,
    /// The opposite operator to place inside the outer `not` (`or` for `and`).
    ///
    /// Determined by `operator`, so the report carries only the latter; the
    /// rewrite and the finding message read this one.
    pub opposite: &'static str,
    /// The spans of each negation's inner operand, in order.
    ///
    pub inner_spans: Vec<ByteSpan>,
}

impl Finding for DeMorganItem {
    /// The operator that collapses, so an `and` of negations and an `or` of
    /// negations are separable without parsing JSON. They produce opposite
    /// rewrites — `(not (or …))` versus `(not (and …))` — and a consumer
    /// filtering on one of them is asking a real question.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("operator={}", self.operator)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    fn message(&self) -> String {
        format!(
            "{} of negations collapses by De Morgan to (not ({} …))",
            self.operator, self.opposite
        )
    }
}

pub fn examine_boolean(
    view: &ExpressionView,
    boolean_form_count: &mut usize,
    violations: &mut Vec<DeMorganItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((operator, opposite)) = opposite_operator(head) else {
        return;
    };
    *boolean_form_count += 1;

    let operands = &view.children[1..];
    // Need at least two operands, all negations, none build-dependent.
    if operands.len() < 2 {
        return;
    }
    if operands.iter().any(is_reader_conditional) {
        return;
    }

    let mut inner_spans = Vec::with_capacity(operands.len());
    for operand in operands {
        match negation_inner(operand) {
            Some(inner) => inner_spans.push(inner.span),
            None => return, // some operand is not a negation: no clean collapse.
        }
    }

    violations.push(DeMorganItem {
        span: view.span,
        operator,
        opposite,
        inner_spans,
    });
}

/// Collects every De Morgan-collapsible boolean form in one file, with the
/// number of `and`/`or` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_de_morgan_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DeMorganItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("boolean_form_count", json!(0))],
        ));
    }

    let mut boolean_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, &mut boolean_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("boolean_form_count", json!(boolean_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DeMorganItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_de_morgan_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build de morgan report")
    }

    fn booleans(input: &str) -> (u64, Vec<DeMorganItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "boolean_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("boolean_form_count in the summary");
        (count, report.findings)
    }

    fn inners<'a>(source: &'a str, item: &DeMorganItem) -> Vec<&'a str> {
        item.inner_spans
            .iter()
            .map(|s| &source[s.start().get()..s.end().get()])
            .collect()
    }

    #[test]
    fn and_of_nots_becomes_not_of_or() {
        let source = "(and (not a) (not b))";
        let (count, violations) = booleans(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
        assert_eq!(violations[0].opposite, "or");
        assert_eq!(inners(source, &violations[0]), vec!["a", "b"]);
    }

    #[test]
    fn or_of_nots_becomes_not_of_and() {
        let (_, violations) = booleans("(or (not a) (not b))");
        assert_eq!(violations[0].opposite, "and");
    }

    #[test]
    fn accepts_null_as_negation() {
        let (_, violations) = booleans("(and (null a) (not b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn preserves_compound_inner_source() {
        let source = "(and (not (foo x)) (not (bar y)))";
        let (_, violations) = booleans(source);
        assert_eq!(inners(source, &violations[0]), vec!["(foo x)", "(bar y)"]);
    }

    #[test]
    fn handles_three_operands() {
        let (_, violations) = booleans("(and (not a) (not b) (not c))");
        assert_eq!(violations[0].inner_spans.len(), 3);
    }

    #[test]
    fn does_not_flag_when_some_operand_is_not_a_negation() {
        let (_, violations) = booleans("(and (not a) b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_single_negation() {
        // (and (not a)) is single-operand-boolean's job.
        let (_, violations) = booleans("(and (not a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_argument_not() {
        let (_, violations) = booleans("(and (not a b) (not c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_operators() {
        let (_, violations) = booleans("(OR (NOT a) (NULL b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].opposite, "and");
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = booleans("(when (and (not a) (not b)) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(and (not a) (not b))", Dialect::Clojure)
            .expect("parse");
        let report = build_de_morgan_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build de morgan report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("boolean_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(and a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun f ()\n  (or (not a) (not b)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "or");
        assert_eq!(finding.json_fields(), vec![("operator", json!("or"))]);
        assert_eq!(finding.text_columns(), vec!["operator=or".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_boolean_scanned_not_only_the_flagged_ones() {
        let report = report("(and (not a) (not b))\n(or a b)\n");
        assert_eq!(report.summary, vec![("boolean_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
