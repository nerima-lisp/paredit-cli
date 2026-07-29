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
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

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
    /// The span of the first stranded clause.
    pub span: ByteSpan,
    /// The 1-based line that clause starts on.
    pub line: usize,
    /// The dispatch operator (`case`/`typecase`), for the finding message.
    pub head: String,
    /// How many clauses are stranded after the catch-all.
    pub unreachable_count: usize,
}

impl Finding for UnreachableCaseClauseItem {
    /// The rule's own name. The operator is carried as data rather than as the
    /// tag, because it is taken verbatim from source and keeps its casing.
    fn kind(&self) -> &'static str {
        "unreachable-case-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("head={}", self.head),
            format!("unreachable_count={}", self.unreachable_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("unreachable_count", json!(self.unreachable_count)),
        ]
    }

    /// The same sentence the `unreachable-case-clause` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} has {} unreachable clause(s) after a t/otherwise catch-all",
            self.head, self.unreachable_count
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_case(
    view: &ExpressionView,
    source: &str,
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
            span: first_dead.span,
            line: line_of(source, first_dead.span.start().get()),
            head: head.to_owned(),
            unreachable_count: unreachable.len(),
        });
    }
}

/// Collects every `case`/`typecase` form with clauses stranded after a
/// `t`/`otherwise` catch-all in one file, with the number of `case`/`typecase`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no stranded clause here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_unreachable_case_clause_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<UnreachableCaseClauseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("case_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, source, &mut case_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("case_form_count", json!(case_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<UnreachableCaseClauseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_unreachable_case_clause_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build unreachable case clause report")
    }

    /// The `(case_form_count, violations)` pair the report is built from.
    fn clauses(input: &str) -> (u64, Vec<UnreachableCaseClauseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "case_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("case_form_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(case x (t 1) (2 :two))", Dialect::Clojure)
            .expect("parse input");
        let report =
            build_unreachable_case_clause_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build unreachable case clause report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(case x (1 :one))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_head_and_count() {
        let report = report("(defun f (x)\n  (case x (t 1) (2 :two)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "unreachable-case-clause");
        assert_eq!(
            finding.json_fields(),
            vec![("head", json!("case")), ("unreachable_count", json!(1))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["head=case".to_owned(), "unreachable_count=1".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_case_scanned_not_only_the_flagged_ones() {
        let report = report("(case x (t 1) (2 :two))\n(case y (1 :one) (t 2))\n");
        assert_eq!(report.summary, vec![("case_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
