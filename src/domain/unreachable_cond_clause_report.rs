//! Common Lisp unreachable-`cond`-clause detection: a `cond` form with one or
//! more clauses positioned *after* a catch-all `(t …)` clause. `cond`
//! evaluates clause tests top to bottom and takes the first that is true; `t`
//! is a constant that always evaluates true and cannot be lexically rebound,
//! so a `(t …)` clause provably fires first — every clause after it is dead
//! code that can never run.
//!
//! This is deliberately position-sensitive: only the *first* catch-all matters
//! and only the clauses that follow it are reported. A `(t …)` clause in the
//! final position is the ordinary `cond` else branch and is never flagged.
//! `otherwise` is not a `cond` catch-all in Common Lisp (only `case`/`typecase`
//! treat it specially), so it is not treated as always-true here.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// Whether a `cond` clause's test is the always-true catch-all symbol `t`. The
/// test must be a bare, unprefixed atom: `'t` is truthy too, but its quoted
/// spelling is unusual and excluded to keep the rule unambiguous.
fn is_catch_all_clause(clause: &ExpressionView) -> bool {
    clause.children.first().is_some_and(|test| {
        test.reader_prefixes.is_empty()
            && atom_text(test).is_some_and(|text| text.eq_ignore_ascii_case("t"))
    })
}

#[derive(Debug, Clone)]
pub struct UnreachableCondClauseItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub unreachable_count: usize,
}

#[derive(Debug)]
pub struct UnreachableCondClauseSummary {
    pub cond_form_count: usize,
    pub violations: Vec<UnreachableCondClauseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnreachableCondClausePolicyOptions {
    fail_on_violation: bool,
}

impl UnreachableCondClausePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct UnreachableCondClausePolicy {
    pub fail_on_violation: bool,
    pub cond_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_cond(
    view: &ExpressionView,
    path: &Path,
    cond_form_count: &mut usize,
    violations: &mut Vec<UnreachableCondClauseItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("cond")) {
        return;
    }
    *cond_form_count += 1;

    // Well-formed clauses only; a stray atom in clause position is malformed
    // and carries no test to reason about.
    let clauses: Vec<&ExpressionView> = view
        .children
        .iter()
        .skip(1)
        .filter(|clause| is_paren_list(clause))
        .collect();

    let Some(catch_all) = clauses
        .iter()
        .position(|clause| is_catch_all_clause(clause))
    else {
        return;
    };
    let unreachable = &clauses[catch_all + 1..];
    if let Some(first_dead) = unreachable.first() {
        violations.push(UnreachableCondClauseItem {
            path: path.to_path_buf(),
            span: first_dead.span,
            unreachable_count: unreachable.len(),
        });
    }
}

/// Collects every `cond` form with clauses stranded after a `(t …)` catch-all
/// across a whole file, along with the total number of `cond` forms scanned.
pub fn collect_unreachable_cond_clauses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<UnreachableCondClauseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut cond_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cond(subview, path, &mut cond_form_count, &mut violations)
        });
    }
    Ok((cond_form_count, violations))
}

pub fn summarize_unreachable_cond_clauses(
    cond_form_count: usize,
    violations: Vec<UnreachableCondClauseItem>,
) -> UnreachableCondClauseSummary {
    UnreachableCondClauseSummary {
        cond_form_count,
        violations,
    }
}

pub fn evaluate_unreachable_cond_clause_policy(
    options: UnreachableCondClausePolicyOptions,
    summary: &UnreachableCondClauseSummary,
) -> UnreachableCondClausePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    UnreachableCondClausePolicy {
        fail_on_violation: options.fail_on_violation(),
        cond_form_count: summary.cond_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clauses(input: &str) -> (usize, Vec<UnreachableCondClauseItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_unreachable_cond_clauses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect unreachable cond clauses")
    }

    #[test]
    fn flags_a_clause_after_a_catch_all() {
        let (cond_form_count, violations) = clauses("(cond ((foo) 1) (t 2) ((bar) 3))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].unreachable_count, 1);
    }

    #[test]
    fn counts_every_clause_after_the_catch_all() {
        let (_, violations) = clauses("(cond (t 1) ((a) 2) ((b) 3))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].unreachable_count, 2);
    }

    #[test]
    fn folds_the_catch_all_symbol_case() {
        let (_, violations) = clauses("(cond ((foo) 1) (T 2) ((bar) 3))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_trailing_catch_all() {
        let (cond_form_count, violations) = clauses("(cond ((foo) 1) ((bar) 2) (t 3))");
        assert_eq!(cond_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_cond_without_a_catch_all() {
        let (_, violations) = clauses("(cond ((foo) 1) ((bar) 2))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_treat_otherwise_as_a_catch_all() {
        let (_, violations) = clauses("(cond ((foo) 1) (otherwise 2) ((bar) 3))");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_cond_nested_in_a_function_body() {
        let (cond_form_count, violations) =
            clauses("(defun f (x) (cond ((p x) 1) (t 2) ((q x) 3)))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(cond ((foo) 1) (t 2) ((bar) 3))").expect("parse input");
        let (cond_form_count, violations) =
            collect_unreachable_cond_clauses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect unreachable cond clauses");
        assert_eq!(cond_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (cond_form_count, items) = clauses("(cond (t 1) ((a) 2))");
        let summary = summarize_unreachable_cond_clauses(cond_form_count, items);

        let quiet = evaluate_unreachable_cond_clause_policy(
            UnreachableCondClausePolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_unreachable_cond_clause_policy(
            UnreachableCondClausePolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
