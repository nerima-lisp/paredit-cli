//! Common Lisp `if-not` detection: a three-argument `if` whose then-branch is
//! the literal `nil` and whose else-branch is the literal `t` — `(if test nil t)`.
//! Such a form yields `nil` when `test` is true and `t` when it is false, which
//! is exactly `(not test)`. Because `if` consults only the primary value of
//! `test` for the branch decision and `not` does the same, and `test` sits in
//! evaluation position either way, the rewrite is exact for any `test` (no
//! double-evaluation and no lost side effects).
//!
//! Only the exact `then = nil`, `else = t` literal shape is matched. The dual
//! `(if test t nil)` is a boolean *coercion* with no single clearer builtin and
//! is left alone, as is `(if test nil nil)` (both branches equal — that is
//! `identical-if-branches`' concern) and any form with a non-literal branch. A
//! reader-conditional branch cannot be the bare `nil`/`t` atom, so none is ever
//! matched.
//!
//! The fix replaces the whole form with `(not TEST)`, copying the test's exact
//! source, so the rule is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// Whether `view` is the bare literal atom `expected` (`nil` or `t`), with no
/// reader prefixes.
fn is_bare_literal(view: &ExpressionView, expected: &str) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(expected))
}

#[derive(Debug, Clone)]
pub struct IfNotItem {
    /// The span of the whole `(if test nil t)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the test, copied verbatim into `(not …)`.
    ///
    /// The rewrite's input, but the report has always published it, so it stays
    /// in the JSON: a consumer holding the file can slice the test out of it.
    pub test_span: ByteSpan,
}

impl Finding for IfNotItem {
    /// The rule's own name. This rule matches exactly one shape, so there is no
    /// discriminator to draw a narrower kind from.
    fn kind(&self) -> &'static str {
        "if-not"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the path and line the envelope already prints: the old
    /// text row carried only those two. The `message` override is what carries
    /// the finding's meaning here.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "test_span",
            json!({
                "start": self.test_span.start().get(),
                "end": self.test_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `if-not` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "if with then=nil and else=t is a negation; (if test nil t) is (not test)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    source: &str,
    if_form_count: &mut usize,
    violations: &mut Vec<IfNotItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then, else] — require the full three-argument shape.
    if view.children.len() != 4 {
        return;
    }
    let then_branch = &view.children[2];
    let else_branch = &view.children[3];
    if !is_bare_literal(then_branch, "nil") || !is_bare_literal(else_branch, "t") {
        return;
    }

    violations.push(IfNotItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        test_span: view.children[1].span,
    });
}

/// Collects every `(if test nil t)` in one file, with the number of `if` forms
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no negation written this way here" for
/// Common Lisp and "nothing was looked for" for Fennel, and the two read
/// identically without the flag.
pub fn build_if_not_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IfNotItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("if_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, source, &mut if_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("if_form_count", json!(if_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<IfNotItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_if_not_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build if-not report")
    }

    /// The `(if_form_count, violations)` pair the report is built from.
    fn ifs(input: &str) -> (u64, Vec<IfNotItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "if_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("if_form_count in the summary");
        (count, report.findings)
    }

    fn test_src<'a>(source: &'a str, item: &IfNotItem) -> &'a str {
        &source[item.test_span.start().get()..item.test_span.end().get()]
    }

    #[test]
    fn flags_if_nil_t() {
        let source = "(if ready nil t)";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(test_src(source, &violations[0]), "ready");
    }

    #[test]
    fn preserves_a_compound_test() {
        let source = "(if (member x xs) nil t)";
        let (_, violations) = ifs(source);
        assert_eq!(test_src(source, &violations[0]), "(member x xs)");
    }

    #[test]
    fn does_not_flag_the_boolean_coercion_direction() {
        // (if test t nil) is a coercion with no clearer single builtin.
        let (_, violations) = ifs("(if test t nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equal_branches() {
        // (if test nil nil) is identical-if-branches' concern.
        let (_, violations) = ifs("(if test nil nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_literal_else() {
        let (_, violations) = ifs("(if test nil other)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_two_armed_if() {
        let (count, violations) = ifs("(if test nil)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_literals() {
        let (_, violations) = ifs("(IF test NIL T)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_if_not() {
        let (_, violations) = ifs("(defun f (x) (if x nil t))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(if x nil t)", Dialect::Clojure).expect("parse");
        let report = build_if_not_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build if-not report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if x a b)").dialect_modelled);
    }

    /// The test span is the rewrite's input, but the report has always
    /// published it, so it has to survive the move onto the envelope.
    #[test]
    fn a_finding_carries_its_line_and_its_test_span() {
        let report = report("(defun f (x)\n  (if x nil t))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "if-not");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "test_span",
                json!({
                    "start": finding.test_span.start().get(),
                    "end": finding.test_span.end().get(),
                })
            )]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if x nil t)\n(if x t nil)\n(if x a b)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
