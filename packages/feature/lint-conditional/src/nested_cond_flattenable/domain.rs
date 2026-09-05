//! Common Lisp nested-`cond` detection: a multi-clause `cond` whose final
//! `(t …)` clause holds nothing but another `cond`.
//!
//! `(cond (a 1) (t (cond (b 2) (c 3))))` and `(cond (a 1) (b 2) (c 3))` are the
//! same function. `cond` tries its clauses in order and yields `nil` when none
//! matches, so an inner `cond` reached only when every outer test failed is
//! exactly a continuation of the outer clause list — the nesting adds a level
//! of indentation and a second `t` to read past, and nothing else.
//!
//! The equivalence is exact rather than approximate, which is why the shape is
//! worth naming: if no `D` clause matches, the inner `cond` returns `nil` and
//! so does the flattened form; if some `D` matches, both return its body. No
//! outer test is re-evaluated either way, because the `t` clause is last and
//! nothing follows it.
//!
//! # What this refuses to report, and why
//!
//! - **Only a literal `t` catch-all.** `otherwise` is *not* a `cond` catch-all
//!   — in `cond` it is an ordinary variable reference, which is why
//!   `unreachable-cond-clause` refuses to treat it as one either. Flattening on
//!   an `otherwise` clause would change the program.
//! - **Only when the `t` clause is last.** A `t` clause with clauses after it
//!   is `unreachable-cond-clause`'s subject, and its "else" is not an else at
//!   all.
//! - **Only when the inner `cond` is the clause's *whole* body.** `(t (foo)
//!   (cond …))` cannot be spliced without losing `(foo)`.
//! - **At least two outer clauses.** A `cond` whose only clause is `(t …)` is
//!   `cond-t-clause`'s subject, which rewrites it to `progn`; reporting it here
//!   as well would give two different instructions for one form.
//! - **At least two inner clauses.** A one-clause inner `cond` is
//!   `single-clause-cond`'s subject, which rewrites it to `when`. Flagging it
//!   here would fire on the exact shape a sibling rule recommends producing.
//!
//! # The judgement this rule does not make
//!
//! Nesting is sometimes the honest structure: an outer `cond` that dispatches
//! on one axis and an inner one that dispatches on another documents two
//! decisions rather than one flat list, and flattening it would blur them. This
//! rule cannot tell those apart from an accidental staircase — the tests are
//! just expressions and "unrelated" is not a property of the syntax. It
//! therefore reports the shape and stops there, which is the whole reason it is
//! [`Fixability::ReportOnly`] and not a rewrite.
//!
//! [`Fixability::ReportOnly`]: paredit_core_lint_engine::model::Fixability::ReportOnly
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{
    bare_symbol, for_each_evaluated_subview, has_reader_conditional_child, is_clause,
};

/// The fewest clauses the outer `cond` must have. One clause is
/// `cond-t-clause`'s shape.
const MIN_OUTER_CLAUSES: usize = 2;

/// The fewest clauses the inner `cond` must have. One clause is
/// `single-clause-cond`'s shape.
const MIN_INNER_CLAUSES: usize = 2;

#[derive(Debug, Clone)]
pub struct NestedCondItem {
    /// The span of the *inner* `cond` — the form that would be spliced away.
    pub span: ByteSpan,
    /// How many clauses the outer `cond` has, the final `t` included.
    pub outer_clauses: usize,
    /// How many clauses the inner `cond` has.
    pub inner_clauses: usize,
}

impl Finding for NestedCondItem {
    fn kind(&self) -> &'static str {
        "nested-cond-flattenable"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.outer_clauses.to_string(),
            self.inner_clauses.to_string(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("outer_clauses", json!(self.outer_clauses)),
            ("inner_clauses", json!(self.inner_clauses)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "the final t clause holds only a cond; its {} clause(s) can be spliced into the outer cond",
            self.inner_clauses
        )
    }
}

/// Whether `clause` is the literal-`t` catch-all.
fn is_literal_t_clause(clause: &ExpressionView) -> bool {
    clause
        .children
        .first()
        .and_then(bare_symbol)
        .is_some_and(|name| name == "t")
}

/// The inner `cond` a `(t …)` clause consists of, if that is all it consists
/// of.
fn sole_inner_cond(clause: &ExpressionView) -> Option<&ExpressionView> {
    // Exactly the `t` and one body form.
    if clause.children.len() != 2 {
        return None;
    }
    let body = &clause.children[1];
    if !body.reader_prefixes.is_empty() {
        return None;
    }
    if !list_head(body).is_some_and(|head| symbol_is(head, "cond")) {
        return None;
    }
    Some(body)
}

pub fn examine_cond(
    view: &ExpressionView,
    cond_form_count: &mut usize,
    violations: &mut Vec<NestedCondItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_is(head, "cond") {
        return;
    }
    *cond_form_count += 1;

    let clauses = &view.children[1..];
    if clauses.len() < MIN_OUTER_CLAUSES {
        return;
    }
    if clauses
        .iter()
        .any(|clause| !is_clause(clause) || has_reader_conditional_child(clause))
    {
        return;
    }

    let Some(last) = clauses.last() else {
        return;
    };
    if !is_literal_t_clause(last) {
        return;
    }
    let Some(inner) = sole_inner_cond(last) else {
        return;
    };

    let inner_clauses = &inner.children[1..];
    if inner_clauses.len() < MIN_INNER_CLAUSES {
        return;
    }
    // A malformed or build-dependent inner clause means the splice is not the
    // simple concatenation this rule claims it is.
    if inner_clauses
        .iter()
        .any(|clause| !is_clause(clause) || has_reader_conditional_child(clause))
    {
        return;
    }

    violations.push(NestedCondItem {
        span: inner.span,
        outer_clauses: clauses.len(),
        inner_clauses: inner_clauses.len(),
    });
}

/// Collects every flattenable nested `cond` in one file, with the number of
/// `cond` forms scanned as the denominator beside them.
pub fn build_nested_cond_flattenable_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedCondItem>> {
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
        for_each_evaluated_subview(&view, |subview| {
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

    fn report(input: &str) -> FileFindings<NestedCondItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_cond_flattenable_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested-cond-flattenable report")
    }

    fn findings(input: &str) -> Vec<NestedCondItem> {
        report(input).findings
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_cond_whose_final_t_clause_is_a_cond() {
        let source = "(cond (a 1) (t (cond (b 2) (c 3))))";
        let items = findings(source);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].outer_clauses, 2);
        assert_eq!(items[0].inner_clauses, 2);
        assert_eq!(slice(source, items[0].span), "(cond (b 2) (c 3))");
    }

    #[test]
    fn flags_an_inner_cond_that_has_its_own_t_clause() {
        assert_eq!(
            findings("(cond (a 1) (t (cond (b 2) (t 3))))").len(),
            1,
            "flattening is still exact"
        );
    }

    #[test]
    fn case_folds_both_heads() {
        assert_eq!(findings("(COND (a 1) (T (CL:COND (b 2) (c 3))))").len(), 1);
    }

    #[test]
    fn finds_a_nested_cond_in_a_function_body() {
        assert_eq!(
            findings("(defun f (a b c) (cond (a 1) (t (cond (b 2) (c 3)))))").len(),
            1
        );
    }

    #[test]
    fn flags_each_level_of_a_three_deep_staircase() {
        // The outer/inner pair and the inner/innermost pair are two findings.
        let items = findings("(cond (a 1) (t (cond (b 2) (t (cond (c 3) (d 4))))))");
        assert_eq!(items.len(), 2);
    }

    // -- near-miss negatives -------------------------------------------------

    /// `cond-t-clause` owns the one-clause outer form and rewrites it to
    /// `progn`.
    #[test]
    fn does_not_flag_a_single_clause_outer_cond() {
        assert!(findings("(cond (t (cond (b 2) (c 3))))").is_empty());
    }

    /// `single-clause-cond` owns the one-clause inner form and rewrites it to
    /// `when`; firing here too would contradict it.
    #[test]
    fn does_not_flag_a_single_clause_inner_cond() {
        assert!(findings("(cond (a 1) (t (cond (b 2))))").is_empty());
    }

    #[test]
    fn does_not_flag_when_the_t_clause_has_more_than_the_cond() {
        assert!(findings("(cond (a 1) (t (log) (cond (b 2) (c 3))))").is_empty());
        assert!(findings("(cond (a 1) (t (cond (b 2) (c 3)) (log)))").is_empty());
    }

    /// In `cond`, `otherwise` is an ordinary variable reference, not a
    /// catch-all.
    #[test]
    fn does_not_treat_otherwise_as_a_catch_all() {
        assert!(findings("(cond (a 1) (otherwise (cond (b 2) (c 3))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_t_clause_that_is_not_last() {
        assert!(findings("(cond (t (cond (b 2) (c 3))) (a 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_non_cond_body() {
        assert!(findings("(cond (a 1) (t (when b 2)))").is_empty());
        assert!(findings("(cond (a 1) (t (case x (1 2) (3 4))))").is_empty());
        assert!(findings("(cond (a 1) (t (if b 2 3)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_nested_cond_in_a_non_final_clause() {
        assert!(findings("(cond (a (cond (b 2) (c 3))) (t 1))").is_empty());
    }

    #[test]
    fn does_not_flag_an_ordinary_flat_cond() {
        assert!(findings("(cond (a 1) (b 2) (t 3))").is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_clause() {
        assert!(findings("(cond (#+sbcl a 1) (t (cond (b 2) (c 3))))").is_empty());
        assert!(findings("(cond (a 1) (t (cond (#+sbcl b 2) (c 3))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_clause() {
        assert!(findings("(cond a (t (cond (b 2) (c 3))))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_inner_cond() {
        assert!(findings("(cond (a 1) (t '(cond (b 2) (c 3))))").is_empty());
    }

    // -- the five quote shapes -----------------------------------------------

    const CANDIDATE: &str = "(cond (a 1) (t (cond (b 2) (c 3))))";

    #[test]
    fn bare_code_fires() {
        assert_eq!(findings(CANDIDATE).len(), 1);
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(findings(&format!("'{CANDIDATE}")).is_empty());
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(findings(&format!("(quote {CANDIDATE})")).is_empty());
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(findings(&format!("'(a ,{CANDIDATE})")).is_empty());
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert_eq!(findings(&format!("`(a ,{CANDIDATE})")).len(), 1);
    }

    #[test]
    fn a_cond_inside_a_string_literal_is_not_a_form() {
        assert!(findings("(format t \"(cond (a 1) (t (cond (b 2) (c 3))))\")").is_empty());
    }

    // -- envelope ------------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(CANDIDATE, Dialect::Clojure).expect("parse");
        let built =
            build_nested_cond_flattenable_report(Path::new("a.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_cond_scanned_not_only_the_flagged_ones() {
        let built = report(&format!("{CANDIDATE}\n(cond (a 1) (t 2))\n"));
        // Two top-level conds plus the nested one.
        assert_eq!(built.summary, vec![("cond_form_count", json!(3))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_fields() {
        let built = report(&format!("(defun f (a b c)\n  {CANDIDATE})\n"));
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("outer_clauses", json!(2_usize)),
                ("inner_clauses", json!(2_usize)),
            ]
        );
    }
}
