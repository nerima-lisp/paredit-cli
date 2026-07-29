//! Common Lisp single-operand-`and`/`or` detection: `(and X)` or `(or X)` with
//! exactly one operand. A one-argument `and` or `or` evaluates that operand and
//! returns its value verbatim — there is no short-circuit to perform and no
//! boolean coercion (`and`/`or` return the operand object, not `t`/`nil`), so
//! `(and X)` and `(or X)` are each just `X`. The wrapper is pure redundancy,
//! common after mechanical macro expansion or when operands are edited away.
//!
//! Only the single-operand shape is flagged. The zero-operand identities
//! (`(and)` is `t`, `(or)` is `nil`) are legitimate macro-expansion building
//! blocks and are left alone, and two-or-more-operand forms are meaningful. A
//! lone reader conditional (`#+`/`#-`) as the sole operand is exempt: it may
//! expand to zero or one operand depending on the build.
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

/// The canonical operator name for an `and`/`or` head, or `None` otherwise.
fn boolean_operator(head: &str) -> Option<&'static str> {
    if head.eq_ignore_ascii_case("and") {
        Some("and")
    } else if head.eq_ignore_ascii_case("or") {
        Some("or")
    } else {
        None
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom operand does not represent one
/// evaluated operand. Mirrors the guard used by the other progn/boolean lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SingleOperandBooleanItem {
    /// The span of the whole `(and X)`/`(or X)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator, lowercased (`and` or `or`).
    pub operator: &'static str,
    /// The span of the sole operand `X` (lets a fix substitute its source).
    ///
    /// The rewrite's input, not the report's: the lint rule slices it to build
    /// the replacement, and the command has never printed it.
    pub inner_span: ByteSpan,
}

impl Finding for SingleOperandBooleanItem {
    /// The operator, so `and` and `or` are separable without parsing JSON.
    ///
    /// A closed two-value set the analysis has already case-folded to a
    /// canonical spelling, which is what makes it a tag rather than data.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the leading `kind`, which is the operator this report's
    /// only text column used to carry.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `single-operand-boolean` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} has a single operand; ({} X) is just X",
            self.operator, self.operator
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_boolean(
    view: &ExpressionView,
    source: &str,
    boolean_form_count: &mut usize,
    violations: &mut Vec<SingleOperandBooleanItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = boolean_operator(head) else {
        return;
    };
    *boolean_form_count += 1;

    // children[0] is the operator; a single operand means exactly two children.
    if view.children.len() != 2 {
        return;
    }
    let operand = &view.children[1];
    if is_reader_conditional(operand) {
        return;
    }
    violations.push(SingleOperandBooleanItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        operator,
        inner_span: operand.span,
    });
}

/// Collects every single-operand `and`/`or` in one file, with the number of
/// `and`/`or` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no single-operand boolean here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_single_operand_boolean_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SingleOperandBooleanItem>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SingleOperandBooleanItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_single_operand_boolean_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build single-operand boolean report")
    }

    /// The `(boolean_form_count, violations)` pair the report is built from.
    fn booleans(input: &str) -> (u64, Vec<SingleOperandBooleanItem>) {
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
    fn flags_single_operand_and() {
        let (count, violations) = booleans("(and x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
    }

    #[test]
    fn flags_single_operand_or() {
        let (_, violations) = booleans("(or (compute))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
    }

    #[test]
    fn inner_span_covers_only_the_operand() {
        let (_, violations) = booleans("(and (foo bar))");
        let inner = violations[0].inner_span;
        assert!(inner.start().get() > violations[0].span.start().get());
        assert!(inner.end().get() < violations[0].span.end().get());
    }

    #[test]
    fn case_folds_the_operator() {
        let (_, violations) = booleans("(AND x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
    }

    #[test]
    fn does_not_flag_two_operands() {
        let (count, violations) = booleans("(and x y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_empty_identity() {
        let (_, violations) = booleans("(and)");
        assert!(violations.is_empty());
        let (_, violations) = booleans("(or)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional() {
        let (_, violations) = booleans("(and #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = booleans("(not x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_single_operand_boolean() {
        // Outer (and ...) has two operands; the inner (or z) is single-operand.
        let (_, violations) = booleans("(and y (or z))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(and x)", Dialect::Clojure).expect("parse");
        let report =
            build_single_operand_boolean_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build single-operand boolean report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("boolean_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(and x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun f (x)\n  (or x))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "or");
        assert_eq!(finding.json_fields(), vec![("operator", json!("or"))]);
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_boolean_scanned_not_only_the_flagged_ones() {
        let report = report("(and x)\n(or a b)\n(or y)\n");
        assert_eq!(report.summary, vec![("boolean_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
