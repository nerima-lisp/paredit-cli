//! Common Lisp constant-`if`-test detection: an `if` whose test is the literal
//! constant `t` or `nil`. The branch is then statically decided and the other is
//! dead code — `(if t A B)` always evaluates `A`, `(if nil A B)` always `B`, and
//! a one-armed `(if nil A)` never runs `A` at all (so it is just `nil`). Because
//! `t` and `nil` are constants that cannot be rebound, the collapse is exact.
//!
//! Only the literal `t`/`nil` symbol is treated as constant; a truthy value like
//! `5` or a variable test is a legitimate condition and is left alone, as is a
//! reader-conditional branch (build-dependent arity).
//!
//! The fix replaces the whole form with the live branch's exact source (or the
//! literal `nil` for a false one-armed `if`), so the rule is auto-fixable.
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

/// Whether `view` is the bare literal `t` or `nil` (no reader prefixes); returns
/// which one so the caller can pick the live branch.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled arity.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ConstantIfTestItem {
    /// The span of the whole `(if TEST …)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The literal test, lowercased (`t` or `nil`).
    pub test: &'static str,
    /// The span of the live branch to keep, or `None` when the result is the
    /// literal `nil` (a false one-armed `if`).
    ///
    /// The rewrite's input, not the report's: the lint rule reads it to splice
    /// the live branch in, and the command never prints it.
    pub result_span: Option<ByteSpan>,
}

impl Finding for ConstantIfTestItem {
    /// The literal test, so `t` and `nil` are separable without parsing JSON.
    ///
    /// They are two different collapses — `t` kills the else branch, `nil`
    /// kills the then branch — and a consumer filtering on one of them is
    /// asking a real question.
    fn kind(&self) -> &'static str {
        self.test
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("test={}", self.test)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("test", json!(self.test))]
    }

    /// The same sentence the `constant-if-test` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("if test is the constant {}; one branch is dead", self.test)
    }
}

/// Whether a test is provably constant, and to what.
///
/// The standalone `inspect constant-if-test` command recognises only the
/// literal `t`/`nil` spelling, having no semantic tables to consult. The lint
/// suite passes a test backed by the value layer, so `(if (= 1 1) a b)` and
/// `(let ((flag t)) (if flag a b))` are the same settled branch.
///
/// Widening is safe for the fix as well as the report: the value layer folds
/// only pure operations, so a test it can evaluate has no side effect for the
/// rewrite to drop.
pub type ConstantTest<'a> = &'a dyn Fn(&ExpressionView) -> Option<bool>;

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    source: &str,
    constant_test: ConstantTest<'_>,
    if_form_count: &mut usize,
    violations: &mut Vec<ConstantIfTestItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then] or [if, test, then, else].
    if view.children.len() != 3 && view.children.len() != 4 {
        return;
    }
    let Some(is_true) = constant_test(&view.children[1]) else {
        return;
    };
    if view.children[2..].iter().any(is_reader_conditional) {
        return;
    }

    let result_span = if is_true {
        // t: the then branch always runs.
        Some(view.children[2].span)
    } else if view.children.len() == 4 {
        // nil with an else branch: the else always runs.
        Some(view.children[3].span)
    } else {
        // nil, no else: the form is just nil.
        None
    };

    violations.push(ConstantIfTestItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        test: if is_true { "t" } else { "nil" },
        result_span,
    });
}

/// Collects every constant-test `if` in one file, with the number of `if` forms
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no constant test here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_constant_if_test_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ConstantIfTestItem>> {
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
            examine_if(
                subview,
                source,
                &constant_test,
                &mut if_form_count,
                &mut violations,
            );
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

    fn report(input: &str) -> FileFindings<ConstantIfTestItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_constant_if_test_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build constant if test report")
    }

    /// The `(if_form_count, violations)` pair the report is built from.
    fn ifs(input: &str) -> (u64, Vec<ConstantIfTestItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "if_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("if_form_count in the summary");
        (count, report.findings)
    }

    fn result<'a>(source: &'a str, item: &ConstantIfTestItem) -> &'a str {
        match item.result_span {
            Some(span) => &source[span.start().get()..span.end().get()],
            None => "nil",
        }
    }

    #[test]
    fn true_test_keeps_then_branch() {
        let source = "(if t a b)";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].test, "t");
        assert_eq!(result(source, &violations[0]), "a");
    }

    #[test]
    fn nil_test_keeps_else_branch() {
        let source = "(if nil a b)";
        let (_, violations) = ifs(source);
        assert_eq!(violations[0].test, "nil");
        assert_eq!(result(source, &violations[0]), "b");
    }

    #[test]
    fn nil_one_armed_is_nil() {
        let source = "(if nil (side-effect))";
        let (_, violations) = ifs(source);
        assert!(violations[0].result_span.is_none());
        assert_eq!(result(source, &violations[0]), "nil");
    }

    #[test]
    fn true_one_armed_keeps_then() {
        let source = "(if t (go))";
        let (_, violations) = ifs(source);
        assert_eq!(result(source, &violations[0]), "(go)");
    }

    #[test]
    fn preserves_compound_branch_source() {
        let source = "(if t (compute x y) other)";
        let (_, violations) = ifs(source);
        assert_eq!(result(source, &violations[0]), "(compute x y)");
    }

    #[test]
    fn does_not_flag_a_variable_test() {
        let (count, violations) = ifs("(if ready a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_truthy_non_t_literal() {
        let (_, violations) = ifs("(if 5 a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_test() {
        let (_, violations) = ifs("(IF NIL a b)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].test, "nil");
    }

    #[test]
    fn finds_a_nested_constant_if() {
        let (_, violations) = ifs("(defun f () (if t 1 2))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(if t a b)", Dialect::Clojure).expect("parse");
        let report = build_constant_if_test_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build constant if test report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if ready a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_test() {
        let report = report("(defun f ()\n  (if t 1 2))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "t");
        assert_eq!(finding.json_fields(), vec![("test", json!("t"))]);
        assert_eq!(finding.text_columns(), vec!["test=t".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if t 1 2)\n(if a 3 4)\n(if nil 5 6)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
