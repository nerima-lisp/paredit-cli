//! Common Lisp self-assignment detection: a `setq`, `psetq`, `setf`, or
//! `psetf` pair whose place form is structurally identical to the value form
//! being assigned to it — `(setq x x)`, `(setf (aref a i) (aref a i))`. The
//! standard says the leftmost value wins for a repeated place, and assigning
//! a place to its own current value is a no-op; in practice it is a typo
//! (usually a wrong variable on one side that happened to be edited to match)
//! rather than an intended write.
//!
//! Like `duplicate-case-keys`, an assignment form can
//! appear anywhere in a body, so this report walks the whole expression tree.
//!
//! Scope: Common Lisp only. Structural equality folds symbol case exactly as
//! the reader does (`X` and `x` are the same symbol) but keeps string and
//! character literals case-sensitive (`"x"` and `"X"` are distinct under
//! `eql`), and treats a package-qualified symbol as distinct from an
//! unqualified one (`pkg:x` ≠ `x`). A place with a `setf`-expander side
//! effect (rare) would be reported too, since a purely syntactic view cannot
//! know the expansion is not a no-op — a documented, deliberately
//! conservative trade toward catching the common typo.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

const ASSIGNMENT_HEADS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];

#[derive(Debug, Clone)]
pub struct SelfAssignmentItem {
    pub span: ByteSpan,
    /// The 1-based line the assignment form starts on.
    pub line: usize,
    /// The operator as it is spelled in the source, so its case survives.
    /// Data rather than a tag: `SETQ` and `setq` are the same operator but not
    /// the same string, which is why this is not the finding's `kind`.
    pub operator: String,
    pub place: String,
}

impl Finding for SelfAssignmentItem {
    /// Fixed: the operator is source-cased data rather than a closed set of
    /// tags, and there is only one class of finding here.
    fn kind(&self) -> &'static str {
        "self-assignment"
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
            format!("place={}", self.place),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("place", json!(self.place)),
        ]
    }

    /// The same sentence the `self-assignment` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("{} assigns place {} to itself", self.operator, self.place)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_assignment(
    view: &ExpressionView,
    source: &str,
    assignment_form_count: &mut usize,
    self_assignments: &mut Vec<SelfAssignmentItem>,
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

    // Arguments are place/value pairs after the operator; a trailing unpaired
    // argument (malformed) is ignored.
    let mut pair = view.children.iter().skip(1);
    while let (Some(place), Some(value)) = (pair.next(), pair.next()) {
        if expressions_structurally_equal(place, value) {
            self_assignments.push(SelfAssignmentItem {
                span: view.span,
                line: line_of(source, view.span.start().get()),
                operator: head.to_owned(),
                place: render_expression(place),
            });
        }
    }
}

/// Collects every self-assigning place in one file, with the number of
/// assignment forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no self-assignment here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_self_assignment_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SelfAssignmentItem>> {
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
    let mut self_assignments = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(
                subview,
                source,
                &mut assignment_form_count,
                &mut self_assignments,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        self_assignments,
        vec![("assignment_form_count", json!(assignment_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SelfAssignmentItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_self_assignment_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build self assignment report")
    }

    /// The `(assignment_form_count, self_assignments)` pair the report is built
    /// from.
    fn assignments(input: &str) -> (u64, Vec<SelfAssignmentItem>) {
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
    fn flags_a_variable_assigned_to_itself() {
        let (assignment_form_count, self_assignments) = assignments("(setq x x)");
        assert_eq!(assignment_form_count, 1);
        assert_eq!(self_assignments.len(), 1);
        assert_eq!(self_assignments[0].operator, "setq");
        assert_eq!(self_assignments[0].place, "x");
    }

    #[test]
    fn does_not_flag_a_normal_assignment() {
        let (_, self_assignments) = assignments("(setq x y)");
        assert!(self_assignments.is_empty());
    }

    #[test]
    fn folds_symbol_case_like_the_reader() {
        let (_, self_assignments) = assignments("(setq X x)");
        assert_eq!(self_assignments.len(), 1);
    }

    #[test]
    fn flags_a_structural_place_assigned_to_itself() {
        let (_, self_assignments) = assignments("(setf (aref a i) (aref a i))");
        assert_eq!(self_assignments.len(), 1);
        assert_eq!(self_assignments[0].place, "(aref a i)");
    }

    #[test]
    fn does_not_flag_a_structural_place_with_a_different_value() {
        let (_, self_assignments) = assignments("(setf (aref a i) (aref a j))");
        assert!(self_assignments.is_empty());
    }

    #[test]
    fn flags_each_self_assigning_pair_in_a_multi_pair_setq() {
        let (_, self_assignments) = assignments("(setq a a b 2 c c)");
        assert_eq!(self_assignments.len(), 2);
    }

    #[test]
    fn finds_an_assignment_nested_in_a_function_body() {
        let (assignment_form_count, self_assignments) = assignments("(defun f () (setf x x))");
        assert_eq!(assignment_form_count, 1);
        assert_eq!(self_assignments.len(), 1);
    }

    #[test]
    fn keeps_string_literals_case_sensitive() {
        // `(setf x "x")` is not a self-assignment: the place is the symbol x,
        // the value is a string; and a string place would compare exactly.
        let (_, self_assignments) = assignments("(setf x \"x\")");
        assert!(self_assignments.is_empty());
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(setq x x)").expect("parse input");
        let report = build_self_assignment_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build self assignment report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("assignment_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(setq x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_place() {
        let report = report("(defun f ()\n  (setf (aref a i) (aref a i)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "self-assignment");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("setf")), ("place", json!("(aref a i)")),]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["op=setf".to_owned(), "place=(aref a i)".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_assignment_scanned_not_only_the_flagged_ones() {
        let report = report("(setq a a)\n(setq b 2)\n(setf c c)\n");
        assert_eq!(report.summary, vec![("assignment_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
