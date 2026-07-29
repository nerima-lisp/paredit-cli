//! Common Lisp negated-`if` detection: a three-argument `if` whose test is a
//! `(not X)`/`(null X)` negation — `(if (not ready) a b)`. Negating the test
//! just flips which branch runs, so `(if (not X) A B)` is exactly `(if X B A)`:
//! the negation drops away and the two branches swap, evaluating `X` once. The
//! positive test reads more directly than "if not …, else …".
//!
//! Only the three-argument shape is flagged. A one-armed `(if (not X) A)` has no
//! else branch to swap into; that shape is the province of the `when`/`unless`
//! idiom (`(unless X A)`), not this rule. A reader-conditional branch is exempt
//! (build-dependent arity).
//!
//! The fix rewrites `(if (not X) A B)` as `(if X B A)`, copying `X`, `A`, and `B`
//! from their exact source, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled arity.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// The un-negated test `X` of a `(not X)`/`(null X)` form, or `None` when `view`
/// is not a single-argument `not`/`null`.
fn negated_test(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("not") && !head.eq_ignore_ascii_case("null") {
        return None;
    }
    // (not X) / (null X): exactly one argument.
    (view.children.len() == 2).then(|| &view.children[1])
}

#[derive(Debug, Clone)]
pub struct NegatedIfItem {
    /// The span of the whole `(if (not X) A B)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the un-negated test `X`.
    ///
    /// One of three rewrite inputs the lint rule slices to build `(if X B A)`;
    /// the command has never printed any of them.
    pub test_span: ByteSpan,
    /// The span of the then-branch `A`.
    pub then_span: ByteSpan,
    /// The span of the else-branch `B`.
    pub else_span: ByteSpan,
}

impl Finding for NegatedIfItem {
    /// The rule's own name. Every finding here is the one shape
    /// `(if (not X) A B)`, with nothing to sub-divide it by — `not` and `null`
    /// are interchangeable spellings of the same negation, not two defects.
    fn kind(&self) -> &'static str {
        "negated-if"
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

    /// Nothing: the old JSON carried only the path and the span, both of which
    /// the envelope publishes. The three branch spans are the fix's inputs and
    /// stay unpublished rather than becoming a new contract here.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `negated-if` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way. Load-bearing
    /// here, since this finding has no text columns of its own.
    fn message(&self) -> String {
        "if test is negated; (if (not c) a b) is (if c b a)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    source: &str,
    if_form_count: &mut usize,
    violations: &mut Vec<NegatedIfItem>,
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
    let Some(inner) = negated_test(&view.children[1]) else {
        return;
    };
    if is_reader_conditional(inner)
        || is_reader_conditional(&view.children[2])
        || is_reader_conditional(&view.children[3])
    {
        return;
    }

    violations.push(NegatedIfItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        test_span: inner.span,
        then_span: view.children[2].span,
        else_span: view.children[3].span,
    });
}

/// Collects every negated three-argument `if` in one file, with the number of
/// `if` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no negated `if` here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_negated_if_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NegatedIfItem>> {
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

    fn report(input: &str) -> FileFindings<NegatedIfItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_negated_if_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build negated if report")
    }

    /// The `(if_form_count, violations)` pair the report is built from.
    fn ifs(input: &str) -> (u64, Vec<NegatedIfItem>) {
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
    fn flags_not_test() {
        let source = "(if (not ready) a b)";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].test_span), "ready");
        assert_eq!(slice(source, violations[0].then_span), "a");
        assert_eq!(slice(source, violations[0].else_span), "b");
    }

    #[test]
    fn flags_null_test() {
        let (_, violations) = ifs("(if (null xs) 0 (length xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn preserves_compound_branch_source() {
        let source = "(if (not p) (do-a x) (do-b y))";
        let (_, violations) = ifs(source);
        assert_eq!(slice(source, violations[0].then_span), "(do-a x)");
        assert_eq!(slice(source, violations[0].else_span), "(do-b y)");
    }

    #[test]
    fn does_not_flag_one_armed_if() {
        // (if (not c) a) has no else to swap.
        let (_, violations) = ifs("(if (not c) a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_positive_test() {
        let (count, violations) = ifs("(if c a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_argument_not() {
        // (not a b) is malformed; leave it alone.
        let (_, violations) = ifs("(if (not a b) x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_negator() {
        let (_, violations) = ifs("(IF (NOT c) a b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_negated_if() {
        let (_, violations) = ifs("(defun f (c) (if (not c) 1 2))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(if (not c) a b)", Dialect::Clojure).expect("parse");
        let report = build_negated_if_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build negated if report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("if_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(if c a b)").dialect_modelled);
    }

    /// With no columns and no extra JSON, the message override is the whole
    /// description a SARIF or JUnit consumer gets.
    #[test]
    fn a_finding_carries_its_line_and_leans_on_its_message() {
        let report = report("(defun pick (ready)\n  (if (not ready) 0 1))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "negated-if");
        assert!(finding.text_columns().is_empty());
        assert!(finding.json_fields().is_empty());
        assert_eq!(
            finding.message(),
            "if test is negated; (if (not c) a b) is (if c b a)"
        );
    }

    #[test]
    fn the_summary_counts_every_if_scanned_not_only_the_flagged_ones() {
        let report = report("(if (not c) a b)\n(if c a b)\n");
        assert_eq!(report.summary, vec![("if_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
