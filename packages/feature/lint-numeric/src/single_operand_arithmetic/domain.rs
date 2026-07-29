//! Common Lisp single-operand `+`/`*` detection: `(+ X)` or `(* X)` with
//! exactly one operand. A one-argument `+` or `*` folds the additive/multiplicative
//! identity over a single value and returns that value verbatim — `(+ X)` is `X`
//! and `(* X)` is `X`. The wrapper is pure redundancy, common after mechanical
//! macro expansion or when a `reduce`/`apply` was inlined down to one argument.
//!
//! Only `+` and `*` are flagged, and only in their single-operand shape:
//!
//!   - `(- X)` is *negation* and `(/ X)` is *reciprocal* — meaningful unary ops,
//!     never flagged.
//!   - The zero-operand identities `(+)` (which is `0`) and `(*)` (which is `1`)
//!     are legitimate macro-expansion building blocks and are left alone.
//!   - Two-or-more-operand forms are meaningful arithmetic.
//!   - A lone reader conditional (`#+`/`#-`) as the sole operand is exempt: it
//!     may expand to zero or one operand depending on the build.
//!
//! Because the fix simply unwraps to the sole operand (an already-present
//! subexpression, copied verbatim), this rule is auto-fixable — the same fix
//! shape as `single-operand-boolean`. Contrast
//! [`crate::identity_arithmetic::domain`], which would have to *delete*
//! an operand from a multi-argument form and is therefore report-only.
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

/// The canonical operator name for a `+`/`*` head, or `None` otherwise. Unlike
/// the boolean operators these are punctuation, so an exact match is used (no
/// case folding — `+` has no alphabetic case).
fn identity_fold_operator(head: &str) -> Option<&'static str> {
    match head {
        "+" => Some("+"),
        "*" => Some("*"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom operand does not represent one
/// evaluated operand. Mirrors the guard used by the other progn/boolean lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SingleOperandArithmeticItem {
    /// The span of the whole `(+ X)`/`(* X)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator (`+` or `*`).
    pub operator: &'static str,
    /// The span of the sole operand `X`.
    ///
    /// The rewrite's input, not the report's: the lint rule copies `X`'s source
    /// to replace the wrapper with it, and the command has never printed it.
    pub inner_span: ByteSpan,
}

impl Finding for SingleOperandArithmeticItem {
    /// The rule's own name.
    ///
    /// Both operators are punctuation (`+`, `*`), which makes a poor `grep`
    /// selector and a worse SARIF rule id, and the two are the same redundant
    /// wrapper either way. The operator stays a reported field instead.
    fn kind(&self) -> &'static str {
        "single-operand-arithmetic"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("operator={}", self.operator)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `single-operand-arithmetic` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} has a single operand; ({} X) is just X",
            self.operator, self.operator
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_arithmetic(
    view: &ExpressionView,
    source: &str,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<SingleOperandArithmeticItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = identity_fold_operator(head) else {
        return;
    };
    *arithmetic_form_count += 1;

    // children[0] is the operator; a single operand means exactly two children.
    if view.children.len() != 2 {
        return;
    }
    let operand = &view.children[1];
    if is_reader_conditional(operand) {
        return;
    }
    violations.push(SingleOperandArithmeticItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        operator,
        inner_span: operand.span,
    });
}

/// Collects every single-operand `+`/`*` in one file, with the number of
/// `+`/`*` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every `+`/`*` here does real arithmetic"
/// for Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_single_operand_arithmetic_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SingleOperandArithmeticItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("arithmetic_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_arithmetic(subview, source, &mut arithmetic_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("arithmetic_form_count", json!(arithmetic_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SingleOperandArithmeticItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_single_operand_arithmetic_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build single-operand arithmetic report")
    }

    /// The `(arithmetic_form_count, violations)` pair the report is built from.
    fn arithmetic(input: &str) -> (u64, Vec<SingleOperandArithmeticItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "arithmetic_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("arithmetic_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_single_operand_plus() {
        let (count, violations) = arithmetic("(+ x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "+");
    }

    #[test]
    fn flags_single_operand_star() {
        let (_, violations) = arithmetic("(* (compute))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "*");
    }

    #[test]
    fn inner_span_covers_only_the_operand() {
        let (_, violations) = arithmetic("(+ (foo bar))");
        let inner = violations[0].inner_span;
        assert!(inner.start().get() > violations[0].span.start().get());
        assert!(inner.end().get() < violations[0].span.end().get());
    }

    #[test]
    fn does_not_flag_two_operands() {
        let (count, violations) = arithmetic("(+ x y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_empty_identity() {
        let (_, violations) = arithmetic("(+)");
        assert!(violations.is_empty());
        let (_, violations) = arithmetic("(*)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_unary_minus_or_divide() {
        // (- x) negates and (/ x) is a reciprocal — both meaningful.
        let (count, violations) = arithmetic("(- x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
        let (count, violations) = arithmetic("(/ x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional() {
        let (_, violations) = arithmetic("(+ #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = arithmetic("(list x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_single_operand_arithmetic() {
        // Outer (+ ...) has two operands; the inner (* z) is single-operand.
        let (_, violations) = arithmetic("(+ y (* z))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "*");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(+ x)", Dialect::Clojure).expect("parse");
        let report =
            build_single_operand_arithmetic_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build single-operand arithmetic report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(+ x y)").dialect_modelled);
    }

    /// The operator is punctuation, so it stays a column and a JSON field
    /// rather than becoming the `kind`.
    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun f (x)\n  (* x))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "single-operand-arithmetic");
        assert_eq!(finding.json_fields(), vec![("operator", json!("*"))]);
        assert_eq!(finding.text_columns(), vec!["operator=*".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_arithmetic_form_scanned_not_only_the_flagged_ones() {
        let report = report("(+ x)\n(+ x y)\n(*)\n");
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
