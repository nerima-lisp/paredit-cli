//! Common Lisp nested-boolean detection: an `and`/`or` form that appears as a
//! direct operand of another `and`/`or` with the *same* operator. Because both
//! operators are associative and evaluate their operands left to right with the
//! same short-circuit rule, a nested same-operator form splices in with no
//! change: `(or a (or b c) d)` is exactly `(or a b c d)` and `(and a (and b c))`
//! is `(and a b c)`. The nesting is pure structure noise, common after
//! mechanical macro expansion or incremental edits.
//!
//! This rule is the multi-operand companion to
//! [`crate::single_operand_boolean::domain`]: that rule owns the
//! single-operand collapse (`(or x)` is `x`), while this rule owns a nested
//! same-operator form with two or more operands that is redundant *because* of
//! where it sits. Requiring two or more inner operands keeps the two rules from
//! flagging the same span.
//!
//! Splicing preserves evaluation order and short-circuiting regardless of the
//! inner operands' contents or any `#+`/`#-` reader conditional among them
//! (which attaches to the form it precedes either way), so no guard is needed
//! beyond skipping an inner form that itself carries a reader prefix
//! (`,(or …)`/`#'(or …)`), which is not a plain boolean subform.
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
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The boolean operator (`and`/`or`) heading `view`, or `None` when `view` is
/// not such a form.
fn boolean_operator(view: &ExpressionView) -> Option<&'static str> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if head.eq_ignore_ascii_case("and") {
        Some("and")
    } else if head.eq_ignore_ascii_case("or") {
        Some("or")
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct NestedBooleanItem {
    /// The span of the inner (nested) `and`/`or` form.
    pub span: ByteSpan,
    /// The 1-based line the inner form starts on.
    pub line: usize,
    /// The span of the inner form's interior (`op`'s operands, parens excluded),
    /// which a fix splices in place of the wrapper.
    ///
    /// The rewrite's input, not the report's: the lint rule slices it to build
    /// the replacement, and the command has never printed it.
    pub inner_span: ByteSpan,
    /// The shared operator (`and`/`or`), for the finding message.
    pub operator: &'static str,
}

impl Finding for NestedBooleanItem {
    /// The shared operator, so `and` and `or` are separable without parsing
    /// JSON.
    ///
    /// A closed, canonical pair — `boolean_operator` accepts nothing else and
    /// returns the lowercased form regardless of how the source spelled it — so
    /// it is a fixed vocabulary rather than an echo of the file.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing: the old text row's only column beyond the path and the offset
    /// was the operator, which now leads the row as the `kind`.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `nested-boolean` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    /// Load-bearing here, since this finding has no text columns of its own.
    fn message(&self) -> String {
        let operator = self.operator;
        format!("{operator} nested in a {operator} flattens; its operands splice in")
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_boolean(
    view: &ExpressionView,
    source: &str,
    boolean_form_count: &mut usize,
    violations: &mut Vec<NestedBooleanItem>,
) {
    let Some(op) = boolean_operator(view) else {
        return;
    };
    *boolean_form_count += 1;

    // children[0] is the operator; any operand that is a same-operator form with
    // two or more operands of its own splices redundantly into this one.
    for child in &view.children[1..] {
        if !child.reader_prefixes.is_empty() {
            continue;
        }
        if boolean_operator(child) != Some(op) {
            continue;
        }
        // Inner operands = children beyond the inner head; require two or more so
        // the single-operand-boolean rule keeps the `(or x)` collapse.
        if child.children.len() < 3 {
            continue;
        }
        // Splice just the operands (drop the inner operator): the interior span
        // runs from the first operand's start to the last operand's end.
        let operands = &child.children[1..];
        let inner_span = ByteSpan::new(
            operands[0].span.start(),
            operands[operands.len() - 1].span.end(),
        );
        violations.push(NestedBooleanItem {
            span: child.span,
            line: line_of(source, child.span.start().get()),
            inner_span,
            operator: op,
        });
    }
}

/// Collects every `and`/`or` nested directly inside a same-operator `and`/`or`
/// in one file, with the number of `and`/`or` forms scanned as the denominator
/// beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant nesting here" for Common
/// Lisp and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_nested_boolean_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedBooleanItem>> {
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

    fn report(input: &str) -> FileFindings<NestedBooleanItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_boolean_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested boolean report")
    }

    /// The `(boolean_form_count, violations)` pair the report is built from.
    fn nested(input: &str) -> (u64, Vec<NestedBooleanItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "boolean_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("boolean_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_or_nested_in_or() {
        let source = "(or a (or b c) d)";
        let (count, violations) = nested(source);
        // Two or forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
        assert_eq!(slice(source, violations[0].inner_span), "b c");
    }

    #[test]
    fn flags_and_nested_in_and() {
        let source = "(and a (and b c))";
        let (_, violations) = nested(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
        assert_eq!(slice(source, violations[0].inner_span), "b c");
    }

    #[test]
    fn does_not_flag_different_operator() {
        // (or a (and b c)) is not associative-flattenable.
        let (_, violations) = nested("(or a (and b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_single_operand_inner() {
        // (or x) is the single-operand-boolean rule's job.
        let (_, violations) = nested("(or a (or x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_inner() {
        let (_, violations) = nested("(and a (and))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_head_position() {
        // The inner form must be an operand, never the operator slot.
        let (_, violations) = nested("(or (or a b))");
        assert_eq!(violations.len(), 1); // still an operand here, flagged
        let (_, none) = nested("(and)");
        assert!(none.is_empty());
    }

    #[test]
    fn case_folds_both_operators() {
        let (_, violations) = nested("(OR a (Or b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_multiple_nested_forms() {
        let (_, violations) = nested("(or (or a b) (or c d))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn flags_deeply_nested_forms() {
        // Inner-most nested in middle, middle nested in outer.
        let (_, violations) = nested("(or a (or b (or c d)))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn does_not_flag_a_prefixed_inner() {
        // ,(or b c) inside a backquote is not a plain boolean subform.
        let (_, violations) = nested("(or a `(or b c))");
        assert!(violations.is_empty());
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(or a (or b c))", Dialect::Clojure).expect("parse");
        let report = build_nested_boolean_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nested boolean report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("boolean_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(or a b)").dialect_modelled);
    }

    /// The operator leads the row as the `kind`, and stays in the JSON where
    /// the old renderer published it.
    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun f ()\n  (or a (or b c) d))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "or");
        assert_eq!(finding.json_fields(), vec![("operator", json!("or"))]);
        assert!(finding.text_columns().is_empty());
    }

    /// `and` is the other half of the closed pair, so the two are separable on
    /// `kind` alone.
    #[test]
    fn an_and_finding_is_a_different_kind_from_an_or_finding() {
        assert_eq!(report("(and a (and b c))").findings[0].kind(), "and");
    }

    #[test]
    fn the_summary_counts_every_boolean_scanned_not_only_the_flagged_ones() {
        // Three `or` forms: the outer, the nested one, and the clean sibling.
        let report = report("(or a (or b c))\n(or d e)\n");
        assert_eq!(report.summary, vec![("boolean_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
