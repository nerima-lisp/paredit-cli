//! Common Lisp `cond`-single-`t`-clause detection: a `cond` with exactly one
//! clause whose test is the literal `t` and which has a body — `(cond (t a b))`.
//! Since `t` always holds, the `cond` unconditionally runs that clause's body
//! and there is nothing else to fall through to, so the form is exactly
//! `(progn a b)` — same body, same last-form return value.
//!
//! This is the `t`-clause complement of
//! [`crate::single_clause_cond::domain`], which rewrites a single
//! *non-`t`* clause `(cond (test body…))` to `(when test body…)` and
//! deliberately leaves the `t` case to this rule.
//!
//! Only the provably-unconditional shape is flagged:
//!
//!   - exactly one clause (a multi-clause `cond` whose *last* clause is `(t …)`
//!     is the idiomatic else and is a real branch — left alone),
//!   - the clause is a parenthesized list whose test is the bare literal `t`
//!     (no reader prefix); `otherwise` is *not* special in `cond` (only in
//!     `case`/`typecase`), so it is not treated as a catch-all, and
//!   - the clause has at least one body form; a test-only `(cond (t))` returns
//!     the test value `t`, which `progn` cannot express, so it is left alone.
//!
//! The fix rewrites `(cond (t body…))` as `(progn body…)`, copying the body
//! forms verbatim, so the rule is auto-fixable.
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

/// Whether `view` is the bare literal `t` (no reader prefixes).
fn is_literal_t(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// clause containing one has no settled shape.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct CondTClauseItem {
    /// The span of the whole `(cond (t body…))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the clause body (`body…`, test and parens excluded), spliced
    /// into `(progn …)`.
    ///
    /// Both the rewrite's input and part of the report: the old renderer
    /// published it, and a consumer building its own rewrite needs the same
    /// extent the lint rule splices.
    pub body_span: ByteSpan,
}

impl Finding for CondTClauseItem {
    /// The rule's own name. A single-`t`-clause `cond` has exactly one shape and
    /// one rewrite, so there is no variant to split on.
    fn kind(&self) -> &'static str {
        "cond-t-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "body_span",
            json!({
                "start": self.body_span.start().get(),
                "end": self.body_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `cond-t-clause` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "single t-clause cond is just progn; (cond (t a b)) is (progn a b)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_cond(
    view: &ExpressionView,
    source: &str,
    cond_form_count: &mut usize,
    violations: &mut Vec<CondTClauseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("cond") {
        return;
    }
    *cond_form_count += 1;

    // children: [cond, clause] — require exactly one clause.
    if view.children.len() != 2 {
        return;
    }
    let clause = &view.children[1];
    if !is_paren_list(clause) {
        return;
    }
    // Need the literal `t` test and at least one body form.
    if clause.children.len() < 2 {
        return;
    }
    if !is_literal_t(&clause.children[0]) {
        return;
    }
    if clause.children.iter().any(is_reader_conditional) {
        return;
    }

    // Body span runs from the first body form through the last, dropping the
    // `t` test and the clause parentheses.
    let body_start = clause.children[1].span.start();
    let body_end = clause.children[clause.children.len() - 1].span.end();

    violations.push(CondTClauseItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        body_span: ByteSpan::new(body_start, body_end),
    });
}

/// Collects every single-`t`-clause `cond` in one file, with the number of
/// `cond` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no unconditional cond here" for Common
/// Lisp and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_cond_t_clause_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CondTClauseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("cond_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut cond_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cond(subview, source, &mut cond_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("cond_form_count", json!(cond_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CondTClauseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_cond_t_clause_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build cond-t-clause report")
    }

    /// The `(cond_form_count, violations)` pair the report is built from.
    fn conds(input: &str) -> (u64, Vec<CondTClauseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "cond_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("cond_form_count in the summary");
        (count, report.findings)
    }

    fn body<'a>(source: &'a str, item: &CondTClauseItem) -> &'a str {
        &source[item.body_span.start().get()..item.body_span.end().get()]
    }

    #[test]
    fn flags_single_t_clause_with_body() {
        let source = "(cond (t a b))";
        let (count, violations) = conds(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(body(source, &violations[0]), "a b");
    }

    #[test]
    fn preserves_compound_body_forms() {
        let source = "(cond (t (setup) (run x)))";
        let (_, violations) = conds(source);
        assert_eq!(body(source, &violations[0]), "(setup) (run x)");
    }

    #[test]
    fn does_not_flag_a_test_only_clause() {
        // (cond (t)) returns the test value t, which progn cannot express.
        let (_, violations) = conds("(cond (t))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_t_test() {
        let (_, violations) = conds("(cond (ready a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_clause_cond() {
        // A trailing (t …) else in a real branch is idiomatic, not a violation.
        let (count, violations) = conds("(cond (p a) (t b))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_test() {
        let (_, violations) = conds("(COND (T a))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_cond() {
        let (_, violations) = conds("(defun f () (cond (t (go))))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(cond (t a))", Dialect::Clojure).expect("parse");
        let report = build_cond_t_clause_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build cond-t-clause report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("cond_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(cond (p a))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_body_span() {
        let source = "(defun f ()\n  (cond (t a b)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "cond-t-clause");
        assert_eq!(body(source, finding), "a b");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "body_span",
                json!({
                    "start": finding.body_span.start().get(),
                    "end": finding.body_span.end().get(),
                })
            )]
        );
    }

    #[test]
    fn the_summary_counts_every_cond_scanned_not_only_the_flagged_ones() {
        let report = report("(cond (t a))\n(cond (p a) (q b))\n");
        assert_eq!(report.summary, vec![("cond_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
