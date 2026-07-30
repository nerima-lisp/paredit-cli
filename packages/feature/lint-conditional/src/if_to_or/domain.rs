//! Common Lisp if-to-`or` detection: a three-argument `if` whose test and
//! then-branch are the *same bare atom* — `(if x x y)`. Since evaluating an
//! atom (a variable, number, keyword, …) has no side effects and yields the
//! same value each time, `(if x x y)` returns `x` when `x` is non-nil and `y`
//! otherwise, which is exactly `(or x y)`. The `or` form states that intent
//! directly and evaluates `x` once instead of twice.
//!
//! Only an *atom* test-and-then pair is flagged. A compound test like
//! `(if (pop s) (pop s) y)` would evaluate its side effect twice, so `or` would
//! not preserve behavior; those are left alone. A literal `t`/`nil` test is left
//! alone as well — that shape belongs to the `constant-if-test` rule — as is a
//! reader-conditional operand (build-dependent arity).
//!
//! The fix rewrites `(if x x y)` as `(or x y)`, copying the test and else from
//! their exact source, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled arity.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// The atom text of `view` if it is a bare atom (no reader prefixes); `None` for
/// a compound form or a prefixed atom.
fn bare_atom(view: &ExpressionView) -> Option<&str> {
    if view.reader_prefixes.is_empty() {
        atom_text(view)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct IfToOrItem {
    /// The span of the whole `(if x x y)` form.
    pub span: ByteSpan,
    /// The span of the shared test/then atom `x`.
    ///
    /// The rewrite's input, not the report's: the lint rule reads it to build
    /// `(or x y)`, and the command has never printed it.
    pub test_span: ByteSpan,
    /// The span of the else branch `y`. The rewrite's input, like `test_span`.
    pub else_span: ByteSpan,
}

impl Finding for IfToOrItem {
    /// The rule's own name. This rule matches exactly one shape, so there is no
    /// discriminator to draw a narrower kind from.
    fn kind(&self) -> &'static str {
        "if-to-or"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the path and line the envelope already prints: the old
    /// text row carried only those two. The `message` override is what carries
    /// the finding's meaning here.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// Nothing beyond the span the envelope already prints. The two operand
    /// spans exist only to feed the autofix, and the old JSON never published
    /// them.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `if-to-or` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "if returns its test or the else; (if x x y) is (or x y)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    if_form_count: &mut usize,
    violations: &mut Vec<IfToOrItem>,
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
    let then = &view.children[2];
    let els = &view.children[3];

    // Test and then must be the same bare atom.
    let (Some(test_text), Some(then_text)) = (bare_atom(test), bare_atom(then)) else {
        return;
    };
    if test_text != then_text {
        return;
    }
    // Leave a literal t/nil test to constant-if-test, and skip reader
    // conditionals in the test or the else.
    if test_text.eq_ignore_ascii_case("t")
        || test_text.eq_ignore_ascii_case("nil")
        || test_text.starts_with("#+")
        || test_text.starts_with("#-")
        || is_reader_conditional(els)
    {
        return;
    }

    violations.push(IfToOrItem {
        span: view.span,
        test_span: test.span,
        else_span: els.span,
    });
}

/// Collects every `(if x x y)` (test == then, both bare atoms) in one file, with
/// the number of `if` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no `if` here is an `or` in disguise" for
/// Common Lisp and "nothing was looked for" for Fennel, and the two read
/// identically without the flag.
pub fn build_if_to_or_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<IfToOrItem>> {
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

    fn report(input: &str) -> FileFindings<IfToOrItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_if_to_or_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build if-to-or report")
    }

    /// The `(if_form_count, violations)` pair the report is built from.
    fn ifs(input: &str) -> (u64, Vec<IfToOrItem>) {
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
    fn flags_if_x_x_y() {
        let source = "(if cached cached (compute))";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].test_span), "cached");
        assert_eq!(slice(source, violations[0].else_span), "(compute)");
    }

    #[test]
    fn does_not_flag_differing_test_and_then() {
        let (count, violations) = ifs("(if a b c)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_compound_test() {
        // (pop s) would be evaluated twice; not an or.
        let (_, violations) = ifs("(if (pop s) (pop s) y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_literal_t_or_nil() {
        // Those belong to constant-if-test.
        assert!(ifs("(if t t y)").1.is_empty());
        assert!(ifs("(if nil nil y)").1.is_empty());
    }

    #[test]
    fn does_not_flag_two_argument_if() {
        // (if x x) has no else; it is one-armed-if's concern.
        let (_, violations) = ifs("(if x x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = ifs("(IF x x y)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_if_to_or() {
        let (_, violations) = ifs("(defun f (x y) (if x x y))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(if x x y)", Dialect::Clojure).expect("parse");
        let report = build_if_to_or_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build if-to-or report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if a b c)").dialect_modelled);
    }

    /// The two operand spans feed the autofix only; the old JSON never
    /// published them, so the envelope does not either.
    #[test]
    fn a_finding_carries_its_line_and_no_operand_spans() {
        let report = report("(defun f (x y)\n  (if x x y))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "if-to-or");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if x x y)\n(if a b c)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
