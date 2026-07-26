//! Common Lisp malformed-`case`-clause detection: a `case`, `ccase`, `ecase`,
//! `typecase`, `ctypecase`, or `etypecase` clause that is not a non-empty
//! list. Every clause in these forms must be `(keys-or-type form*)` — a bare
//! atom (`(case x (1 :one) foo)`) or an empty `()` in clause position is a
//! program error, caught only at macroexpansion rather than by the reader.
//!
//! The highest-value catch is a dropped-parenthesis typo: writing
//! `(case x (1 :one) 2 :two)` when `(case x (1 :one) (2 :two))` was meant
//! leaves `2` and `:two` as bare-atom clauses, which this rule flags directly.
//!
//! Unlike [`crate::domain::duplicate_case_key_report`] — which excludes the
//! type-testing `typecase` family because its clause heads are type specifiers
//! rather than `eql` keys — this report covers all six forms, since the
//! *structural* requirement "each clause is a non-empty list" is identical
//! across them.
//!
//! Forms whose clause structure is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted `case` (data or a template, not a
//! call), and any clause guarded by a `#+`/`#-` reader conditional or spliced
//! in from a template (`,@`).
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::render_expression;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

const CASE_HEADS: [&str; 6] = [
    "case",
    "ccase",
    "ecase",
    "typecase",
    "ctypecase",
    "etypecase",
];

/// Whether a clause's structure is not statically visible: it is guarded by a
/// `#+`/`#-` reader conditional (which the dialect reader groups into an atom
/// whose text begins `#+`/`#-`, or leaves as a bare marker atom) or spliced
/// from a template (`,@`, or a Clojure `#?`/`#?@`).
fn is_structurally_opaque(clause: &ExpressionView) -> bool {
    let opaque_prefix = clause.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    opaque_prefix
        || atom_text(clause).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct MalformedCaseClauseItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub clause: String,
}

#[derive(Debug)]
pub struct MalformedCaseClauseSummary {
    pub case_form_count: usize,
    pub violations: Vec<MalformedCaseClauseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MalformedCaseClausePolicyOptions {
    fail_on_violation: bool,
}

impl MalformedCaseClausePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct MalformedCaseClausePolicy {
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
    violations: &mut Vec<MalformedCaseClauseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !CASE_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted/unquoted case form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *case_form_count += 1;

    // The keyform is child 1; clauses start at child 2.
    for clause in view.children.iter().skip(2) {
        if is_structurally_opaque(clause) {
            continue;
        }
        if !is_paren_list(clause) || clause.children.is_empty() {
            violations.push(MalformedCaseClauseItem {
                path: path.to_path_buf(),
                span: clause.span,
                head: head.to_owned(),
                clause: render_expression(clause),
            });
        }
    }
}

/// Collects every malformed `case`-family clause across a whole file, along
/// with the total number of `case`-family forms scanned.
pub fn collect_malformed_case_clauses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<MalformedCaseClauseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, path, &mut case_form_count, &mut violations)
        });
    }
    Ok((case_form_count, violations))
}

pub fn summarize_malformed_case_clauses(
    case_form_count: usize,
    violations: Vec<MalformedCaseClauseItem>,
) -> MalformedCaseClauseSummary {
    MalformedCaseClauseSummary {
        case_form_count,
        violations,
    }
}

pub fn evaluate_malformed_case_clause_policy(
    options: MalformedCaseClausePolicyOptions,
    summary: &MalformedCaseClauseSummary,
) -> MalformedCaseClausePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MalformedCaseClausePolicy {
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

    fn clauses(input: &str) -> (usize, Vec<MalformedCaseClauseItem>) {
        // Use the dialect-aware parse the CLI path uses, which groups Common
        // Lisp `#+`/`#-` reader conditionals into a single node.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_malformed_case_clauses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect malformed case clauses")
    }

    #[test]
    fn flags_a_bare_atom_clause() {
        let (case_form_count, violations) = clauses("(case x (1 :one) foo (2 :two))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].clause, "foo");
        assert_eq!(violations[0].head, "case");
    }

    #[test]
    fn flags_dropped_paren_tail_clauses() {
        // `(1 :one)` is valid; the trailing `2` and `:two` are bare-atom clauses.
        let (_, violations) = clauses("(case x (1 :one) 2 :two)");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn flags_an_empty_clause() {
        let (_, violations) = clauses("(case x () (1 :one))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_valid_clauses() {
        let (case_form_count, violations) =
            clauses("(case x (1 :one) ((2 3) :multi) (otherwise :o))");
        assert_eq!(case_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_keylist_clause() {
        // `(() :never)` is a valid (if useless) clause whose keylist is empty.
        let (_, violations) = clauses("(case x (() :never) (t :yes))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_a_malformed_typecase_clause() {
        let (case_form_count, violations) = clauses("(typecase x (integer 1) bogus (t 0))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "typecase");
    }

    #[test]
    fn flags_a_malformed_ecase_clause() {
        let (_, violations) = clauses("(ecase x (:a 1) bad)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "ecase");
    }

    #[test]
    fn skips_a_feature_conditional_clause() {
        let (case_form_count, violations) = clauses("(case x (1 :one) #+sbcl (2 :two) (t :o))");
        assert_eq!(case_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_quoted_case_form() {
        let (case_form_count, violations) = clauses("(list '(case x foo bar))");
        assert_eq!(case_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (case_form_count, violations) = clauses("(defun f (x) (case x (1 :one) oops))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(case x (1 :one) foo)", Dialect::Clojure)
            .expect("parse input");
        let (case_form_count, violations) =
            collect_malformed_case_clauses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect malformed case clauses");
        assert_eq!(case_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (case_form_count, items) = clauses("(case x (1 :one) foo)");
        let summary = summarize_malformed_case_clauses(case_form_count, items);

        let quiet = evaluate_malformed_case_clause_policy(
            MalformedCaseClausePolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_malformed_case_clause_policy(
            MalformedCaseClausePolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
