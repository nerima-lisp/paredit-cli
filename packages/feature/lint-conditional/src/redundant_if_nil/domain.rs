//! Common Lisp redundant-`if`-`nil`-else detection: a three-argument `if` whose
//! else branch is a bare `nil` — `(if test then nil)`. A two-argument `if`
//! already yields `nil` when the test is false, so the explicit `nil` else adds
//! nothing: `(if test then nil)` is exactly `(if test then)`. Dropping it is a
//! provably behavior-preserving cleanup.
//!
//! Only a *bare* `nil` else is flagged, and only when the then branch is not
//! itself `nil` (a `(if c nil nil)` is the identical-if-branches rule's
//! territory). A quoted `'nil`, a `()` list, or any non-`nil` else is left
//! alone.
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

/// Whether `view` is the bare atom `nil`.
fn is_bare_nil(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct RedundantIfNilItem {
    /// The span of the whole `(if …)` form.
    pub span: ByteSpan,
    /// The span from the end of the then branch to the end of the `nil` else —
    /// the ` nil` a fix deletes to leave `(if test then)`.
    ///
    /// The rewrite's input, not the report's: the lint rule deletes exactly
    /// this range, and neither the old renderer nor this one prints it.
    pub removal_span: ByteSpan,
}

impl Finding for RedundantIfNilItem {
    /// The rule's own name. Every finding is the same shape — a bare `nil`
    /// else — so there is no sub-classification to make.
    fn kind(&self) -> &'static str {
        "redundant-if-nil"
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

    /// The same sentence the `redundant-if-nil` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "if else branch is a redundant nil; (if c x nil) is (if c x)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    if_form_count: &mut usize,
    violations: &mut Vec<RedundantIfNilItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("if")) {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then, else]. Only a bare-nil else with a non-nil
    // then is a redundant-nil-else (the nil/nil case is identical-if-branches).
    if view.children.len() != 4 {
        return;
    }
    let then_branch = &view.children[2];
    let else_branch = &view.children[3];
    if is_bare_nil(else_branch) && !is_bare_nil(then_branch) {
        violations.push(RedundantIfNilItem {
            span: view.span,
            removal_span: ByteSpan::new(then_branch.span.end(), else_branch.span.end()),
        });
    }
}

/// Collects every `if` with a redundant `nil` else in one file, with the number
/// of `if` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant nil else here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_if_nil_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantIfNilItem>> {
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

    fn report(input: &str) -> FileFindings<RedundantIfNilItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_if_nil_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant if nil report")
    }

    /// The `(if_form_count, violations)` pair the report is built from.
    fn ifs(input: &str) -> (u64, Vec<RedundantIfNilItem>) {
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
    fn flags_a_nil_else() {
        let (count, violations) = ifs("(if ready (go) nil)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn removal_span_covers_the_space_and_nil() {
        let input = "(if ready (go) nil)";
        let (_, violations) = ifs(input);
        let removal = violations[0].removal_span;
        assert_eq!(&input[removal.start().get()..removal.end().get()], " nil");
    }

    #[test]
    fn case_folds_the_head_and_nil() {
        let (_, violations) = ifs("(IF c x NIL)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_two_argument_if() {
        let (count, violations) = ifs("(if c x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_nil_else() {
        let (_, violations) = ifs("(if c x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_when_then_is_also_nil() {
        // (if c nil nil) is the identical-if-branches rule's territory.
        let (_, violations) = ifs("(if c nil nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_nil_else() {
        let (_, violations) = ifs("(if c x 'nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_nil_then_branch() {
        // Only the else nil is redundant; a nil then with a real else stays.
        let (_, violations) = ifs("(if c nil y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_if() {
        let (_, violations) = ifs("(defun f (c x) (list (if c x nil)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(if c x nil)", Dialect::Clojure).expect("parse");
        let report = build_redundant_if_nil_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant if nil report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if c x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_no_columns_of_its_own() {
        let report = report("(defun f (c x)\n  (if c x nil))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-if-nil");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if c x nil)\n(if c x y)\n(if d z nil)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
