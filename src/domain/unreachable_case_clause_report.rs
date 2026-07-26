//! Common Lisp unreachable-`case`-clause detection: a `case` or `typecase`
//! form with one or more clauses positioned *after* a catch-all clause. In
//! `case`, a bare `t` or `otherwise` key designator is the default clause; in
//! `typecase`, a bare `t` is the universal type (which matches every object)
//! and `otherwise` is the default. Any of these provably fires before the
//! clauses that follow it, so those clauses are dead code that can never run.
//!
//! The catch-all must be a *bare* `t`/`otherwise` atom: `((t) …)` — a
//! one-element key *list* — means "match the literal symbol `T`" and is not a
//! catch-all, so it is not treated as one here.
//!
//! Scoped to `case`/`typecase` on purpose. The exhaustive variants
//! (`ecase`/`ccase`/`etypecase`/`ctypecase`) signal on no match and the
//! standard's treatment of `t`/`otherwise` in them is not a clean default, so
//! they are excluded to keep this a zero-false-positive rule.
//!
//! Forms whose clause structure is not statically visible are skipped: a
//! quoted/quasiquoted `case` (data or a template), and clauses guarded by a
//! `#+`/`#-` reader conditional or spliced from a template (which do not parse
//! as plain list clauses and so are never seen as a catch-all or as a
//! following clause).
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

const CATCH_ALL_HEADS: [&str; 2] = ["case", "typecase"];

/// Whether a clause's key designator is a bare `t` or `otherwise` catch-all. A
/// key *list* such as `(t)` matches the literal symbol and is not a catch-all,
/// so the first child must be an unprefixed atom.
fn is_catch_all_clause(clause: &ExpressionView) -> bool {
    clause.children.first().is_some_and(|key| {
        key.reader_prefixes.is_empty()
            && atom_text(key).is_some_and(|text| {
                text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("otherwise")
            })
    })
}

#[derive(Debug, Clone)]
pub struct UnreachableCaseClauseItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub unreachable_count: usize,
}

#[derive(Debug)]
pub struct UnreachableCaseClauseSummary {
    pub case_form_count: usize,
    pub violations: Vec<UnreachableCaseClauseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct UnreachableCaseClausePolicyOptions {
    fail_on_violation: bool,
}

impl UnreachableCaseClausePolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct UnreachableCaseClausePolicy {
    pub fail_on_violation: bool,
    pub case_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_case(
    view: &ExpressionView,
    path: &Path,
    case_form_count: &mut usize,
    violations: &mut Vec<UnreachableCaseClauseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !CATCH_ALL_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted case form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *case_form_count += 1;

    // The keyform is child 1; clauses start at child 2. Only well-formed list
    // clauses are considered (a feature-conditional clause reads as an opaque
    // atom and is skipped, keeping the rule conservative).
    let clauses: Vec<&ExpressionView> = view
        .children
        .iter()
        .skip(2)
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
        violations.push(UnreachableCaseClauseItem {
            path: path.to_path_buf(),
            span: first_dead.span,
            head: head.to_owned(),
            unreachable_count: unreachable.len(),
        });
    }
}

/// Collects every `case`/`typecase` form with clauses stranded after a
/// `t`/`otherwise` catch-all across a whole file, along with the total number
/// of `case`/`typecase` forms scanned.
pub fn collect_unreachable_case_clauses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<UnreachableCaseClauseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, path, &mut case_form_count, &mut violations);
        });
    }
    Ok((case_form_count, violations))
}

#[must_use]
pub const fn summarize_unreachable_case_clauses(
    case_form_count: usize,
    violations: Vec<UnreachableCaseClauseItem>,
) -> UnreachableCaseClauseSummary {
    UnreachableCaseClauseSummary {
        case_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_unreachable_case_clause_policy(
    options: UnreachableCaseClausePolicyOptions,
    summary: &UnreachableCaseClauseSummary,
) -> UnreachableCaseClausePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    UnreachableCaseClausePolicy {
        fail_on_violation: options.fail_on_violation(),
        case_form_count: summary.case_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clauses(input: &str) -> (usize, Vec<UnreachableCaseClauseItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_unreachable_case_clauses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect unreachable case clauses")
    }

    #[test]
    fn flags_a_clause_after_a_t_catch_all() {
        let (case_form_count, violations) = clauses("(case x (1 :one) (t :def) (2 :two))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].unreachable_count, 1);
        assert_eq!(violations[0].head, "case");
    }

    #[test]
    fn flags_a_clause_after_an_otherwise_catch_all() {
        let (_, violations) = clauses("(case x (1 :one) (otherwise :def) (2 :two))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn counts_every_clause_after_the_catch_all() {
        let (_, violations) = clauses("(case x (t 1) (2 :two) (3 :three))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].unreachable_count, 2);
    }

    #[test]
    fn does_not_flag_a_trailing_catch_all() {
        let (case_form_count, violations) = clauses("(case x (1 :one) (2 :two) (t :def))");
        assert_eq!(case_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_literal_t_key_list() {
        // `((t) :sym)` matches the literal symbol T, not a catch-all.
        let (_, violations) = clauses("(case x ((t) :sym) (2 :two))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_a_typecase_clause_after_t() {
        let (case_form_count, violations) = clauses("(typecase x (integer 1) (t 0) (string 2))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "typecase");
    }

    #[test]
    fn does_not_flag_exhaustive_ecase() {
        // ecase is exhaustive; `t`/`otherwise` are not catch-alls there.
        let (case_form_count, violations) = clauses("(ecase x (t 1) (2 :two))");
        assert_eq!(case_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_feature_conditional_following_clause() {
        // Conservative: the feature-guarded clause reads as an opaque atom.
        let (case_form_count, violations) = clauses("(case x (t 1) #+sbcl (2 :two))");
        assert_eq!(case_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_quoted_case_form() {
        let (case_form_count, violations) = clauses("(list '(case x (t 1) (2 :two)))");
        assert_eq!(case_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (case_form_count, violations) =
            clauses("(defun f (x) (case x (1 :one) (t 2) (3 :three)))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(case x (t 1) (2 :two))", Dialect::Clojure)
            .expect("parse input");
        let (case_form_count, violations) =
            collect_unreachable_case_clauses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect unreachable case clauses");
        assert_eq!(case_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (case_form_count, items) = clauses("(case x (t 1) (2 :two))");
        let summary = summarize_unreachable_case_clauses(case_form_count, items);

        let quiet = evaluate_unreachable_case_clause_policy(
            UnreachableCaseClausePolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_unreachable_case_clause_policy(
            UnreachableCaseClausePolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
