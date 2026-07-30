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
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

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
    /// The span of the first stranded clause.
    pub span: ByteSpan,
    /// How many clauses are stranded after the catch-all.
    pub unreachable_count: usize,
}

impl Finding for UnreachableCondClauseItem {
    fn kind(&self) -> &'static str {
        "unreachable-cond-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("unreachable_count={}", self.unreachable_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("unreachable_count", json!(self.unreachable_count))]
    }

    /// The same sentence the `unreachable-cond-clause` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "cond has {} unreachable clause(s) after a t clause",
            self.unreachable_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_cond(
    view: &ExpressionView,
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
            span: first_dead.span,
            unreachable_count: unreachable.len(),
        });
    }
}

/// Collects every `cond` form with clauses stranded after a `(t …)` catch-all
/// in one file, with the number of `cond` forms scanned as the denominator
/// beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no stranded clause here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_unreachable_cond_clause_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<UnreachableCondClauseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("cond_form_count", json!(0))],
        ));
    }

    let mut cond_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cond(subview, &mut cond_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("cond_form_count", json!(cond_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<UnreachableCondClauseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_unreachable_cond_clause_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build unreachable cond clause report")
    }

    /// The `(cond_form_count, violations)` pair the report is built from.
    fn clauses(input: &str) -> (u64, Vec<UnreachableCondClauseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "cond_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("cond_form_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(cond ((foo) 1) (t 2) ((bar) 3))", Dialect::Clojure)
                .expect("parse input");
        let report =
            build_unreachable_cond_clause_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build unreachable cond clause report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("cond_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(cond ((foo) 1))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_count() {
        let report = report("(defun f (x)\n  (cond (t 1) ((a) 2)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "unreachable-cond-clause");
        assert_eq!(finding.json_fields(), vec![("unreachable_count", json!(1))]);
        assert_eq!(
            finding.text_columns(),
            vec!["unreachable_count=1".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_cond_scanned_not_only_the_flagged_ones() {
        let report = report("(cond (t 1) ((a) 2))\n(cond ((b) 1) (t 2))\n");
        assert_eq!(report.summary, vec![("cond_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
