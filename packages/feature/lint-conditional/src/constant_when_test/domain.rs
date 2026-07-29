//! Common Lisp constant-`when`/`unless`-test detection: a `when` or `unless`
//! whose test is the literal constant `t` or `nil`. Because `t` and `nil` are
//! constants that cannot be rebound, the branch is statically decided:
//!
//! - `(when t body…)` always runs its body, so it is `(progn body…)`.
//! - `(unless nil body…)` always runs its body, so it is `(progn body…)`.
//! - `(when nil body…)` never runs its body and yields `nil` — dead code.
//! - `(unless t body…)` never runs its body and yields `nil` — dead code.
//!
//! The "always runs" cases are rewritten to `progn` (splicing the head/test down
//! to `progn`), which preserves the last-form return value and evaluation order.
//! The "never runs" cases collapse to the literal `nil`; the discarded body is
//! never evaluated, so no side effects are lost. Both rewrites are exact, so the
//! rule is auto-fixable.
//!
//! Only the literal `t`/`nil` symbol is treated as constant; a truthy value like
//! `5` or a variable test is a legitimate condition and is left alone. Body forms
//! are preserved verbatim (always case) or discarded whole (never case), so a
//! reader-conditional in the body needs no special handling.
//!
//! This is the `when`/`unless` sibling of
//! [`crate::constant_if_test::domain`]. Reuses the shared whole-tree walk
//! from [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// Whether `view` is the bare literal `t` or `nil` (no reader prefixes); returns
/// which one so the caller can decide the branch.
fn constant_test(view: &ExpressionView) -> Option<bool> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    if text.eq_ignore_ascii_case("t") {
        Some(true)
    } else if text.eq_ignore_ascii_case("nil") {
        Some(false)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ConstantWhenTestItem {
    /// The span of the whole `(when TEST …)`/`(unless TEST …)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The head operator, lowercased (`when` or `unless`).
    pub head: &'static str,
    /// The literal test, lowercased (`t` or `nil`).
    pub test: &'static str,
    /// Whether the body always runs (rewrite to `progn`) or never runs
    /// (collapse to `nil`).
    pub always_runs: bool,
    /// For the "always runs" rewrite: the span from the form's opening paren
    /// through the test atom, replaced wholesale with `(progn`.
    ///
    /// The rewrite's input, not the report's: the lint rule reads it to splice
    /// the head down to `progn`, and the command never printed it.
    pub splice_span: ByteSpan,
}

impl Finding for ConstantWhenTestItem {
    /// Which way the form collapses, in the vocabulary the text output already
    /// used. `(when t …)` and `(unless nil …)` are the same defect — a wrapper
    /// that is really a `progn` — while `(when nil …)` and `(unless t …)` are
    /// dead code, and a consumer that cares about one of those rarely cares
    /// about the other. The `head`/`test` pair that produced it stays in the
    /// JSON for anyone who wants the finer split.
    fn kind(&self) -> &'static str {
        if self.always_runs { "progn" } else { "dead" }
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("head={}", self.head), format!("test={}", self.test)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("test", json!(self.test)),
            ("always_runs", json!(self.always_runs)),
        ]
    }

    /// The same sentence the `constant-when-test` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        if self.always_runs {
            format!(
                "{} test is the constant {}; the body always runs, so this is a progn",
                self.head, self.test
            )
        } else {
            format!(
                "{} test is the constant {}; the body never runs, so this is nil",
                self.head, self.test
            )
        }
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_when(
    view: &ExpressionView,
    source: &str,
    when_form_count: &mut usize,
    violations: &mut Vec<ConstantWhenTestItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let is_when = head.eq_ignore_ascii_case("when");
    let is_unless = head.eq_ignore_ascii_case("unless");
    if !is_when && !is_unless {
        return;
    }
    *when_form_count += 1;

    // children: [head, test, body…] — require at least the head and test.
    if view.children.len() < 2 {
        return;
    }
    let test = &view.children[1];
    let Some(is_true) = constant_test(test) else {
        return;
    };

    // `when` runs its body when the test is true; `unless` when it is false.
    let always_runs = is_when == is_true;
    let splice_span = ByteSpan::new(view.span.start(), test.span.end());

    violations.push(ConstantWhenTestItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        head: if is_when { "when" } else { "unless" },
        test: if is_true { "t" } else { "nil" },
        always_runs,
        splice_span,
    });
}

/// Collects every constant-test `when`/`unless` in one file, with the number of
/// `when`/`unless` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no constant test here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_constant_when_test_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ConstantWhenTestItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("when_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut when_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_when(subview, source, &mut when_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("when_form_count", json!(when_form_count))],
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

    fn report(input: &str) -> FileFindings<ConstantWhenTestItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_constant_when_test_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build constant when test report")
    }

    /// The `(when_form_count, violations)` pair the report is built from.
    fn whens(input: &str) -> (u64, Vec<ConstantWhenTestItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "when_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("when_form_count in the summary");
        (count, report.findings)
    }

    fn splice<'a>(source: &'a str, item: &ConstantWhenTestItem) -> &'a str {
        &source[item.splice_span.start().get()..item.splice_span.end().get()]
    }

    #[test]
    fn when_t_always_runs() {
        let source = "(when t a b)";
        let (count, violations) = whens(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].test, "t");
        assert!(violations[0].always_runs);
        assert_eq!(splice(source, &violations[0]), "(when t");
    }

    #[test]
    fn unless_nil_always_runs() {
        let source = "(unless nil a b)";
        let (_, violations) = whens(source);
        assert_eq!(violations[0].head, "unless");
        assert_eq!(violations[0].test, "nil");
        assert!(violations[0].always_runs);
        assert_eq!(splice(source, &violations[0]), "(unless nil");
    }

    #[test]
    fn when_nil_never_runs() {
        let source = "(when nil (side-effect))";
        let (_, violations) = whens(source);
        assert!(!violations[0].always_runs);
        assert_eq!(violations[0].test, "nil");
    }

    #[test]
    fn unless_t_never_runs() {
        let (_, violations) = whens("(unless t (go))");
        assert!(!violations[0].always_runs);
        assert_eq!(violations[0].head, "unless");
        assert_eq!(violations[0].test, "t");
    }

    #[test]
    fn empty_body_is_still_flagged() {
        let (_, violations) = whens("(when t)");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].always_runs);
    }

    #[test]
    fn does_not_flag_a_variable_test() {
        let (count, violations) = whens("(when ready a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_truthy_non_t_literal() {
        let (_, violations) = whens("(when 5 a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_test() {
        let (_, violations) = whens("(WHEN NIL a)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].test, "nil");
        assert!(!violations[0].always_runs);
    }

    #[test]
    fn finds_a_nested_constant_when() {
        let (_, violations) = whens("(defun f () (unless nil 1 2))");
        assert_eq!(violations.len(), 1);
        assert!(violations[0].always_runs);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(when t a b)", Dialect::Clojure).expect("parse");
        let report = build_constant_when_test_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build constant when test report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("when_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(when ready a)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_how_the_form_collapses() {
        let report = report("(defun f ()\n  (when t 1 2))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "progn");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head", json!("when")),
                ("test", json!("t")),
                ("always_runs", json!(true)),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["head=when".to_owned(), "test=t".to_owned()]
        );
    }

    /// A body that never runs is dead code, not a redundant wrapper, and the
    /// kind separates them.
    #[test]
    fn a_never_running_body_is_kinded_as_dead() {
        let report = report("(unless t (go))");
        assert_eq!(report.findings[0].kind(), "dead");
    }

    #[test]
    fn the_summary_counts_every_when_scanned_not_only_the_flagged_ones() {
        let report = report("(when t 1)\n(when ready 2)\n(unless nil 3)\n");
        assert_eq!(report.summary, vec![("when_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
