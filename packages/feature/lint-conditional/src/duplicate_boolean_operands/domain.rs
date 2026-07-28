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

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};

const BOOLEAN_HEADS: [&str; 2] = ["and", "or"];

#[derive(Debug, Clone)]
pub struct DuplicateBooleanOperandItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub operand: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateBooleanOperandSummary {
    pub boolean_form_count: usize,
    pub duplicates: Vec<DuplicateBooleanOperandItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateBooleanOperandPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateBooleanOperandPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    #[must_use]
    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateBooleanOperandPolicy {
    pub fail_on_duplicate: bool,
    pub boolean_form_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_boolean(
    view: &ExpressionView,
    path: &Path,
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
                path: path.to_path_buf(),
                span: view.span,
                head: head.to_owned(),
                operand: render_expression(operands[anchor]),
                occurrence_count,
            });
        }
    }
}

/// Collects every duplicated `and`/`or` operand across a whole file, along
/// with the total number of `and`/`or` forms scanned.
pub fn collect_duplicate_boolean_operands(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<DuplicateBooleanOperandItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut boolean_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, path, &mut boolean_form_count, &mut duplicates);
        });
    }
    Ok((boolean_form_count, duplicates))
}

#[must_use]
pub const fn summarize_duplicate_boolean_operands(
    boolean_form_count: usize,
    duplicates: Vec<DuplicateBooleanOperandItem>,
) -> DuplicateBooleanOperandSummary {
    DuplicateBooleanOperandSummary {
        boolean_form_count,
        duplicates,
    }
}

#[must_use]
pub fn evaluate_duplicate_boolean_operand_policy(
    options: DuplicateBooleanOperandPolicyOptions,
    summary: &DuplicateBooleanOperandSummary,
) -> DuplicateBooleanOperandPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateBooleanOperandPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        boolean_form_count: summary.boolean_form_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateBooleanOperandItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_duplicate_boolean_operands(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate boolean operands")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(or x x)").expect("parse input");
        let (boolean_form_count, duplicates) =
            collect_duplicate_boolean_operands(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate boolean operands");
        assert_eq!(boolean_form_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (boolean_form_count, items) = duplicates("(or x x)");
        let summary = summarize_duplicate_boolean_operands(boolean_form_count, items);

        let quiet = evaluate_duplicate_boolean_operand_policy(
            DuplicateBooleanOperandPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict = evaluate_duplicate_boolean_operand_policy(
            DuplicateBooleanOperandPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
