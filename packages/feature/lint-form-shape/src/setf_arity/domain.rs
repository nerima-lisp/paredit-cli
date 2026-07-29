//! Common Lisp `setf`-arity detection: a `setq`, `psetq`, `setf`, or `psetf`
//! form with an odd number of arguments. All four take a flat sequence of
//! place/value *pairs*, so an odd argument count means the final place has no
//! value form — `(setf a 1 b)` and `(setq x 1 y)` are always errors, caught
//! only at macroexpansion (or silently miscompiled) rather than by the
//! reader. There is no valid odd-arity `setf`, so this report has no
//! false positives.
//!
//! An empty form (`(setf)`) has zero arguments — even — and is left alone;
//! this reports only a *non-zero* odd count.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`], since an assignment can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

const ASSIGNMENT_HEADS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];

#[derive(Debug, Clone)]
pub struct SetfArityItem {
    pub span: ByteSpan,
    /// The 1-based line the assignment form starts on.
    pub line: usize,
    /// The operator as it is spelled in the source, so its case survives.
    /// Data rather than a tag: `SETF` and `setf` are the same operator but not
    /// the same string, which is why this is not the finding's `kind`.
    pub operator: String,
    pub argument_count: usize,
}

impl Finding for SetfArityItem {
    /// Fixed: the operator is source-cased data rather than a closed set of
    /// tags, and there is only one class of finding here.
    fn kind(&self) -> &'static str {
        "setf-arity"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("op={}", self.operator),
            format!("args={}", self.argument_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("argument_count", json!(self.argument_count)),
        ]
    }

    /// The same sentence the `setf-arity` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} has {} arguments; place/value pairs require an even count",
            self.operator, self.argument_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_assignment(
    view: &ExpressionView,
    source: &str,
    assignment_form_count: &mut usize,
    violations: &mut Vec<SetfArityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !ASSIGNMENT_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *assignment_form_count += 1;

    let argument_count = view.children.len() - 1;
    if argument_count > 0 && argument_count % 2 == 1 {
        violations.push(SetfArityItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            operator: head.to_owned(),
            argument_count,
        });
    }
}

/// Collects every odd-arity `setq`/`setf`/`psetq`/`psetf` in one file, with the
/// number of assignment forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every assignment here is even-arity" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_setf_arity_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SetfArityItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("assignment_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(subview, source, &mut assignment_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("assignment_form_count", json!(assignment_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SetfArityItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_setf_arity_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build setf arity report")
    }

    /// The `(assignment_form_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<SetfArityItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "assignment_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("assignment_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_an_odd_arity_setf() {
        let (assignment_form_count, violations) = violations("(setf a 1 b)");
        assert_eq!(assignment_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "setf");
        assert_eq!(violations[0].argument_count, 3);
    }

    #[test]
    fn flags_an_odd_arity_setq() {
        let (_, violations) = violations("(setq x 1 y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "setq");
    }

    #[test]
    fn does_not_flag_a_well_formed_setf() {
        let (assignment_form_count, violations) = violations("(setf a 1 b 2)");
        assert_eq!(assignment_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_pair() {
        let (_, violations) = violations("(setf place value)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_setf() {
        let (assignment_form_count, violations) = violations("(setf)");
        assert_eq!(assignment_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_an_assignment_nested_in_a_function_body() {
        let (assignment_form_count, violations) = violations("(defun f () (psetq a 1 b))");
        assert_eq!(assignment_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(setf a 1 b)").expect("parse input");
        let report = build_setf_arity_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build setf arity report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("assignment_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(setf a 1 b 2)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_argument_count() {
        let report = report("(defun f ()\n  (psetq a 1 b))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "setf-arity");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("psetq")), ("argument_count", json!(3))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op=psetq".to_owned(), "args=3".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_assignment_scanned_not_only_the_flagged_ones() {
        let report = report("(setf a 1 b)\n(setf c 2)\n(setq d 3 e)\n");
        assert_eq!(report.summary, vec![("assignment_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
