//! Common Lisp single-clause-`cond` detection: a `cond` with exactly one
//! clause that has a test and a body. `cond` walks its clauses in order,
//! evaluating each test until one is non-nil and then running that clause's
//! body; with a single clause there is nothing to fall through to, so
//! `(cond (test a b))` is exactly `(when test a b)` — same test, same body,
//! same nil-on-failure result. The `when` states "run this body only when the
//! test holds" directly, without the reader scanning for further clauses.
//!
//! Only the narrow, provably-equivalent shape is flagged:
//!
//!   - exactly one clause (a two-or-more-clause `cond` is a real branch),
//!   - the clause is a parenthesized list holding a test **and** at least one
//!     body form (a test-only `(cond (test))` returns the *test value*, which
//!     `when` cannot express, so it is left alone), and
//!   - the test is not the literal `t`/`otherwise` catch-all (that shape is a
//!     plain `(progn …)`, a different rewrite), nor a reader-conditional
//!     operand (build-dependent).
//!
//! The fix rewrites `(cond (test body…))` as `(when test body…)` by wrapping
//! the clause interior verbatim, so the rule is auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteOffset, ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// clause containing one has no settled shape.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether `view` is the bare `t`/`otherwise` catch-all test (no reader
/// prefixes). Such a single clause is a `progn`, not a `when`.
fn is_catch_all_test(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| {
            text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("otherwise")
        })
}

#[derive(Debug, Clone)]
pub struct SingleClauseCondItem {
    /// The span of the whole `(cond (…))` form.
    pub span: ByteSpan,
    /// The span of the clause interior (`test body…`, parens excluded), which
    /// the fix wraps as `(when …)`.
    ///
    pub clause_inner_span: ByteSpan,
}

impl Finding for SingleClauseCondItem {
    /// The rule's own name. Every finding is the same shape — a `cond` with one
    /// body-bearing clause — so there is no sub-classification to make.
    fn kind(&self) -> &'static str {
        "single-clause-cond"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// None. The old renderer printed the path and the offset and nothing else,
    /// and both are the envelope's now.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// None, for the same reason: the old JSON carried only `path` and `span`.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn message(&self) -> String {
        "single-clause cond with a body is just when; (cond (test a b)) is (when test a b)"
            .to_owned()
    }
}

pub fn examine_cond(
    view: &ExpressionView,
    cond_form_count: &mut usize,
    violations: &mut Vec<SingleClauseCondItem>,
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
    // Need a test and at least one body form; a test-only clause returns the
    // test value, which `when` cannot reproduce.
    if clause.children.len() < 2 {
        return;
    }
    let test = &clause.children[0];
    if is_catch_all_test(test) {
        return;
    }
    if clause.children.iter().any(is_reader_conditional) {
        return;
    }

    // Strip the clause's own parentheses: `(test body…)` -> `test body…`.
    let inner_start = clause.span.start().get() + 1;
    let inner_end = clause.span.end().get().saturating_sub(1);
    if inner_end <= inner_start {
        return;
    }
    let clause_inner_span = ByteSpan::new(ByteOffset::new(inner_start), ByteOffset::new(inner_end));

    violations.push(SingleClauseCondItem {
        span: view.span,
        clause_inner_span,
    });
}

/// Collects every single-clause `cond` in one file, with the number of `cond`
/// forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_single_clause_cond_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SingleClauseCondItem>> {
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

    fn report(input: &str) -> FileFindings<SingleClauseCondItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_single_clause_cond_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build single-clause cond report")
    }

    fn conds(input: &str) -> (u64, Vec<SingleClauseCondItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "cond_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("cond_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_clause_with_body() {
        let source = "(cond ((> x 0) (f x)))";
        let (count, violations) = conds(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].clause_inner_span),
            "(> x 0) (f x)"
        );
    }

    #[test]
    fn flags_symbol_test() {
        let source = "(cond (ready (go)))";
        let (_, violations) = conds(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].clause_inner_span), "ready (go)");
    }

    #[test]
    fn preserves_multi_form_body_source() {
        let source = "(cond (test a b c))";
        let (_, violations) = conds(source);
        assert_eq!(slice(source, violations[0].clause_inner_span), "test a b c");
    }

    #[test]
    fn does_not_flag_two_clauses() {
        let (count, violations) = conds("(cond (a 1) (b 2))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_catch_all_t() {
        let (_, violations) = conds("(cond (t (default)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_otherwise() {
        let (_, violations) = conds("(cond (otherwise x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_test_only_clause() {
        // (cond (test)) returns the test value; when cannot express that.
        let (_, violations) = conds("(cond ((compute)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_cond() {
        let (count, violations) = conds("(cond)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_paren_clause() {
        let (_, violations) = conds("(cond foo)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_reader_conditional_body() {
        let (_, violations) = conds("(cond (test #+sbcl (sb-thing) other))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = conds("(COND (test body))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_single_clause_cond() {
        let (_, violations) = conds("(defun f (x) (cond ((plusp x) (g x))))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(cond (test body))", Dialect::Clojure).expect("parse");
        let report = build_single_clause_cond_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build single-clause cond report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("cond_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(cond (a 1) (b 2))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_no_columns_of_its_own() {
        let report = report("(defun f (x)\n  (cond ((plusp x) (g x))))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "single-clause-cond");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_cond_scanned_not_only_the_flagged_ones() {
        let report = report("(cond (a 1))\n(cond (b 2) (c 3))\n(cond (d 4))\n");
        assert_eq!(report.summary, vec![("cond_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
