//! Common Lisp malformed-`cond`-clause detection: a `cond` clause that is not
//! a non-empty list. Every `cond` clause must be `(test-form form*)` — a bare
//! atom (`(cond ((foo) 1) bar …)`) or an empty `()` in clause position is a
//! program error, caught only at macroexpansion rather than by the reader.
//!
//! The highest-value catch is a dropped-parenthesis typo: writing
//! `(cond ((foo) 1) (bar) 2)` when a `(… 2)` clause was meant leaves `2` as a
//! bare-atom clause, which this rule flags directly.
//!
//! Forms whose clause structure is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted `cond` (data or a template, not a
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
pub struct MalformedCondClauseItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub clause: String,
}

#[derive(Debug)]
pub struct MalformedCondClauseSummary {
    pub cond_form_count: usize,
    pub violations: Vec<MalformedCondClauseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MalformedCondClausePolicyOptions {
    fail_on_violation: bool,
}

impl MalformedCondClausePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct MalformedCondClausePolicy {
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
    violations: &mut Vec<MalformedCondClauseItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("cond")) {
        return;
    }
    // A quoted/quasiquoted/unquoted `cond` is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *cond_form_count += 1;

    for clause in view.children.iter().skip(1) {
        if is_structurally_opaque(clause) {
            continue;
        }
        // Every clause must be a non-empty list `(test form*)`.
        if !is_paren_list(clause) || clause.children.is_empty() {
            violations.push(MalformedCondClauseItem {
                path: path.to_path_buf(),
                span: clause.span,
                clause: render_expression(clause),
            });
        }
    }
}

/// Collects every malformed `cond` clause across a whole file, along with the
/// total number of `cond` forms scanned.
pub fn collect_malformed_cond_clauses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<MalformedCondClauseItem>)> {
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

pub fn summarize_malformed_cond_clauses(
    cond_form_count: usize,
    violations: Vec<MalformedCondClauseItem>,
) -> MalformedCondClauseSummary {
    MalformedCondClauseSummary {
        cond_form_count,
        violations,
    }
}

pub fn evaluate_malformed_cond_clause_policy(
    options: MalformedCondClausePolicyOptions,
    summary: &MalformedCondClauseSummary,
) -> MalformedCondClausePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MalformedCondClausePolicy {
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

    fn clauses(input: &str) -> (usize, Vec<MalformedCondClauseItem>) {
        // Use the dialect-aware parse the CLI path uses, which groups Common
        // Lisp `#+`/`#-` reader conditionals into a single node.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_malformed_cond_clauses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect malformed cond clauses")
    }

    #[test]
    fn flags_a_bare_atom_clause() {
        let (cond_form_count, violations) = clauses("(cond ((foo) 1) bar ((baz) 2))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].clause, "bar");
    }

    #[test]
    fn flags_a_dropped_paren_tail_clause() {
        // `(bar)` is a valid clause; the trailing `2` is a bare-atom clause.
        let (_, violations) = clauses("(cond ((foo) 1) (bar) 2)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].clause, "2");
    }

    #[test]
    fn flags_an_empty_clause() {
        let (_, violations) = clauses("(cond () ((foo) 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_valid_clauses() {
        let (cond_form_count, violations) = clauses("(cond ((foo) 1) ((bar) 2) (t 3))");
        assert_eq!(cond_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_test_only_clause() {
        // `((foo))` is a clause whose test is `(foo)` with no body — valid.
        let (_, violations) = clauses("(cond ((foo)) (t 1))");
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_feature_conditional_clause() {
        // `#+sbcl ((foo) 1)` reads as one node; its structure is not statically
        // visible, so it is not flagged.
        let (cond_form_count, violations) = clauses("(cond #+sbcl ((foo) 1) ((bar) 2))");
        assert_eq!(cond_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_quoted_cond_form() {
        let (cond_form_count, violations) = clauses("(list '(cond foo bar))");
        assert_eq!(cond_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_cond_nested_in_a_function_body() {
        let (cond_form_count, violations) = clauses("(defun f (x) (cond ((p x) 1) x))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(cond ((foo) 1) bar)", Dialect::Clojure)
            .expect("parse input");
        let (cond_form_count, violations) =
            collect_malformed_cond_clauses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect malformed cond clauses");
        assert_eq!(cond_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (cond_form_count, items) = clauses("(cond ((foo) 1) bar)");
        let summary = summarize_malformed_cond_clauses(cond_form_count, items);

        let quiet = evaluate_malformed_cond_clause_policy(
            MalformedCondClausePolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_malformed_cond_clause_policy(
            MalformedCondClausePolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
