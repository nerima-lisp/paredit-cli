//! Common Lisp self-assignment detection: a `setq`, `psetq`, `setf`, or
//! `psetf` pair whose place form is structurally identical to the value form
//! being assigned to it — `(setq x x)`, `(setf (aref a i) (aref a i))`. The
//! standard says the leftmost value wins for a repeated place, and assigning
//! a place to its own current value is a no-op; in practice it is a typo
//! (usually a wrong variable on one side that happened to be edited to match)
//! rather than an intended write.
//!
//! Like [`crate::domain::duplicate_case_key_report`], an assignment form can
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

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::{expressions_structurally_equal, render_expression};
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, list_head};

const ASSIGNMENT_HEADS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];

#[derive(Debug, Clone)]
pub struct SelfAssignmentItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub place: String,
}

#[derive(Debug)]
pub struct SelfAssignmentSummary {
    pub assignment_form_count: usize,
    pub self_assignments: Vec<SelfAssignmentItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SelfAssignmentPolicyOptions {
    fail_on_self_assignment: bool,
}

impl SelfAssignmentPolicyOptions {
    pub fn new(fail_on_self_assignment: bool) -> Self {
        Self {
            fail_on_self_assignment,
        }
    }

    pub const fn fail_on_self_assignment(self) -> bool {
        self.fail_on_self_assignment
    }
}

#[derive(Debug)]
pub struct SelfAssignmentPolicy {
    pub fail_on_self_assignment: bool,
    pub assignment_form_count: usize,
    pub self_assignment_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_assignment(
    view: &ExpressionView,
    path: &Path,
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
                path: path.to_path_buf(),
                span: view.span,
                operator: head.to_owned(),
                place: render_expression(place),
            });
        }
    }
}

/// Collects every self-assigning place across a whole file, along with the
/// total number of assignment forms scanned.
pub fn collect_self_assignments(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SelfAssignmentItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut assignment_form_count = 0;
    let mut self_assignments = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(
                subview,
                path,
                &mut assignment_form_count,
                &mut self_assignments,
            )
        });
    }
    Ok((assignment_form_count, self_assignments))
}

pub fn summarize_self_assignments(
    assignment_form_count: usize,
    self_assignments: Vec<SelfAssignmentItem>,
) -> SelfAssignmentSummary {
    SelfAssignmentSummary {
        assignment_form_count,
        self_assignments,
    }
}

pub fn evaluate_self_assignment_policy(
    options: SelfAssignmentPolicyOptions,
    summary: &SelfAssignmentSummary,
) -> SelfAssignmentPolicy {
    let self_assignment_count = summary.self_assignments.len();
    let mut violations = Vec::new();
    if options.fail_on_self_assignment() && self_assignment_count > 0 {
        violations.push(format!(
            "self_assignment_count {self_assignment_count} exceeds 0"
        ));
    }

    SelfAssignmentPolicy {
        fail_on_self_assignment: options.fail_on_self_assignment(),
        assignment_form_count: summary.assignment_form_count,
        self_assignment_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignments(input: &str) -> (usize, Vec<SelfAssignmentItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_self_assignments(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect self assignments")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(setq x x)").expect("parse input");
        let (assignment_form_count, self_assignments) =
            collect_self_assignments(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect self assignments");
        assert_eq!(assignment_form_count, 0);
        assert!(self_assignments.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (assignment_form_count, items) = assignments("(setq x x)");
        let summary = summarize_self_assignments(assignment_form_count, items);

        let quiet =
            evaluate_self_assignment_policy(SelfAssignmentPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.self_assignment_count, 1);

        let strict =
            evaluate_self_assignment_policy(SelfAssignmentPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
