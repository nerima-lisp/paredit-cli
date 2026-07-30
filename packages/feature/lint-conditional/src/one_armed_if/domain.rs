//! Common Lisp one-armed-`if` detection: a two-argument `(if TEST THEN)` with
//! no else branch. A one-armed `if` and `(when TEST THEN)` are exactly
//! equivalent — both evaluate `THEN` when `TEST` is true and yield `nil`
//! otherwise — and every mainstream Common Lisp style guide (Google, Norvig's
//! PAIP, the SBCL sources) recommends `when`/`unless` over an `if` with a
//! missing branch, because `when` states "conditionally do this" without
//! implying a two-way choice and admits an implicit multi-form body.
//!
//! Only the exact two-argument shape is flagged:
//!
//!   - `(if test then else)` (a real two-way branch) is never flagged.
//!   - `(if test)` (a malformed, argument-short `if`) is left to `if-arity`.
//!   - A form with a reader-conditional operand (`#+`/`#-`) is exempt: its
//!     effective argument count is build-dependent, so "has no else" cannot be
//!     decided statically.
//!
//! The fix swaps the `if` head for `when` in place — a single-token edit that
//! leaves the test and then-branch byte-identical. When the then-branch is a
//! `(progn …)`, the follow-on `redundant-body-progn` fix splices it during the
//! same fixpoint pass, so `(if c (progn a b))` converges to `(when c a b)`.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so it does not count as one settled operand. Mirrors
/// the guard used by the progn/boolean lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct OneArmedIfItem {
    /// The span of the whole `(if TEST THEN)` form.
    pub span: ByteSpan,
    /// The span of the `if` head symbol (lets a fix swap it for `when`).
    ///
    /// The rewrite's input, not the report's: the lint rule replaces exactly
    /// this token, and neither the old renderer nor this one prints it.
    pub head_span: ByteSpan,
}

impl Finding for OneArmedIfItem {
    /// The rule's own name. Every finding is the same shape — a two-argument
    /// `if` — so there is no sub-classification to make.
    fn kind(&self) -> &'static str {
        "one-armed-if"
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

    /// The same sentence the `one-armed-if` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "if has no else branch; (if test then) is (when test then)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    if_form_count: &mut usize,
    violations: &mut Vec<OneArmedIfItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then]. A one-armed if is exactly three children.
    if view.children.len() != 3 {
        return;
    }
    // Skip when any operand is a reader conditional: the true arity is
    // build-dependent, so "no else branch" cannot be decided statically.
    if view.children[1..].iter().any(is_reader_conditional) {
        return;
    }

    violations.push(OneArmedIfItem {
        span: view.span,
        head_span: view.children[0].span,
    });
}

/// Collects every one-armed `if` in one file, with the number of `if` forms
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every `if` has an else" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_one_armed_if_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<OneArmedIfItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("if_form_count", json!(0))],
        ));
    }

    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, &mut if_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("if_form_count", json!(if_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<OneArmedIfItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_one_armed_if_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build one-armed if report")
    }

    /// The `(if_form_count, violations)` pair the report is built from.
    fn ifs(input: &str) -> (u64, Vec<OneArmedIfItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "if_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("if_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_two_argument_if() {
        let (count, violations) = ifs("(if ready (go))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn head_span_covers_only_the_if_symbol() {
        let source = "(if ready (go))";
        let (_, violations) = ifs(source);
        let head = violations[0].head_span;
        assert_eq!(source.get(head.start().get()..head.end().get()), Some("if"));
    }

    #[test]
    fn does_not_flag_two_armed_if() {
        let (count, violations) = ifs("(if test a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_argument_short_if() {
        // (if test) is malformed; leave it to if-arity.
        let (_, violations) = ifs("(if test)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_operand() {
        let (_, violations) = ifs("(if #+sbcl a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = ifs("(IF ready (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = ifs("(when ready (go))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_one_armed_if() {
        let (_, violations) = ifs("(defun f () (if ready (go)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(if ready (go))", Dialect::Clojure).expect("parse");
        let report = build_one_armed_if_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build one-armed if report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if ready a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_no_columns_of_its_own() {
        let report = report("(defun f ()\n  (if ready (go)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "one-armed-if");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if a (go))\n(if b 1 2)\n(if c (stop))\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
