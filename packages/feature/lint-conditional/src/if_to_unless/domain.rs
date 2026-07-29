//! Common Lisp `if`-to-`unless` detection: a three-argument `(if c nil e)` whose
//! then-branch is the literal `nil`. Such a form yields `nil` when `c` holds and
//! `e` otherwise, which is exactly `(unless c e)` — same test (evaluated once),
//! same else value(s), same nil-on-true result. `unless` states "run this only
//! when the test fails" directly, without a dead `nil` then-branch.
//!
//! To avoid overlapping sibling rules, several else-branch shapes are left alone:
//!
//!   - `else = t` is [`crate::if_not::domain`] (`(if c nil t)` is
//!     `(not c)`), a cleaner rewrite;
//!   - `else = nil` is [`crate::identical_if_branches::domain`]
//!     (`(if c nil nil)`);
//!   - a literal `t`/`nil` *test* is [`crate::constant_if_test::domain`].
//!
//! A reader-conditional test or else operand is also left alone.
//!
//! The fix rewrites `(if c nil e)` as `(unless c e)`, copying the test and else
//! source, so the rule is auto-fixable.
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

/// Whether `view` is the bare literal atom `expected` (`nil`/`t`), no prefixes.
fn is_bare_literal(view: &ExpressionView, expected: &str) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(expected))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct IfToUnlessItem {
    /// The span of the whole `(if c nil e)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the test `c`.
    pub test_span: ByteSpan,
    /// The span of the else branch `e`.
    pub else_span: ByteSpan,
}

impl Finding for IfToUnlessItem {
    /// The rule's own name. Every finding here is the one shape `(if c nil e)`,
    /// with nothing to sub-divide it by.
    fn kind(&self) -> &'static str {
        "if-to-unless"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing: the old text row carried only the path and the offset, which
    /// the envelope prints itself.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// The two operand spans, which this report has always published. They are
    /// the rewrite's inputs, but a consumer reading them out of the JSON to
    /// build its own `unless` is doing something this report already supported.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("test_span", span_json(self.test_span)),
            ("else_span", span_json(self.else_span)),
        ]
    }

    /// The same sentence the `if-to-unless` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "an if with a nil then-branch is an unless; (if c nil e) is (unless c e)".to_owned()
    }
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    if_form_count: &mut usize,
    violations: &mut Vec<IfToUnlessItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then, else] — the three-argument shape.
    if view.children.len() != 4 {
        return;
    }
    let test = &view.children[1];
    let then_branch = &view.children[2];
    let else_branch = &view.children[3];

    // then must be the literal nil.
    if !is_bare_literal(then_branch, "nil") {
        return;
    }
    // else = t is if-not's job; else = nil is identical-if-branches'.
    if is_bare_literal(else_branch, "t") || is_bare_literal(else_branch, "nil") {
        return;
    }
    // a constant t/nil test is constant-if-test's job.
    if is_bare_literal(test, "t") || is_bare_literal(test, "nil") {
        return;
    }
    if is_reader_conditional(test) || is_reader_conditional(else_branch) {
        return;
    }

    violations.push(IfToUnlessItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        test_span: test.span,
        else_span: else_branch.span,
    });
}

/// Collects every `(if c nil e)` rewritable to `unless` in one file, with the
/// number of `if` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no rewritable `if` here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_if_to_unless_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IfToUnlessItem>> {
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
            examine(subview, source, &mut if_form_count, &mut violations);
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

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<IfToUnlessItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_if_to_unless_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build if to unless report")
    }

    /// The `(if_form_count, violations)` pair the report is built from.
    fn ifs(input: &str) -> (u64, Vec<IfToUnlessItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "if_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("if_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_if_nil_else() {
        let source = "(if ready nil (do-work))";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].test_span), "ready");
        assert_eq!(slice(source, violations[0].else_span), "(do-work)");
    }

    #[test]
    fn does_not_flag_else_t() {
        // (if c nil t) is if-not's concern.
        assert!(ifs("(if c nil t)").1.is_empty());
    }

    #[test]
    fn does_not_flag_else_nil() {
        // (if c nil nil) is identical-if-branches' concern.
        assert!(ifs("(if c nil nil)").1.is_empty());
    }

    #[test]
    fn does_not_flag_constant_test() {
        assert!(ifs("(if t nil e)").1.is_empty());
        assert!(ifs("(if nil nil e)").1.is_empty());
    }

    #[test]
    fn does_not_flag_non_nil_then() {
        assert!(ifs("(if c x e)").1.is_empty());
    }

    #[test]
    fn does_not_flag_two_armed_if() {
        assert!(ifs("(if c nil)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = ifs("(IF c NIL e)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = ifs("(defun f (c e) (if c nil e))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(if c nil e)", Dialect::Clojure).expect("parse");
        let report = build_if_to_unless_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build if to unless report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if ready nil (do-work))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_operand_spans() {
        let source = "(defun f ()\n  (if ready nil (do-work)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "if-to-unless");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![
                ("test_span", span_json(finding.test_span)),
                ("else_span", span_json(finding.else_span)),
            ]
        );
        assert_eq!(slice(source, finding.test_span), "ready");
        assert_eq!(slice(source, finding.else_span), "(do-work)");
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if a nil e)\n(if b c d)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
