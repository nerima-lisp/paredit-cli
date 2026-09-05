//! Common Lisp `eq`-on-a-number detection: a call to `eq` with a numeric
//! literal argument, such as `(eq n 5)` or `(eq (len x) 0)`. `eq` tests
//! object identity, and the standard only guarantees identity for the same
//! object — numbers are not required to be `eq` even when mathematically
//! equal. Small fixnums often happen to be `eq` on a given implementation,
//! but bignums and floats are not, so `(eq n 5)` silently works until `n`
//! grows past the fixnum range. The correct comparison is `eql` (or `=` for
//! numeric equality across types).
//!
//! Only `eq` is covered: `eql` and `=` compare numbers correctly, so they
//! are not flagged. A number literal is recognized by Rust's own integer or
//! float parse, which rejects the classic gotcha symbols `1+` and `1-`
//! (increment/decrement functions, not numbers); the first character must
//! also be a digit, sign, or dot, so the symbols `inf`/`nan` — which Rust's
//! float parser would otherwise accept — are excluded.
//!
//! The literal spelling is only how the bug is *usually* written, not what
//! makes it a bug: the CLHS lets an implementation copy a number at any time,
//! so `eq` is unreliable on any number whatever produced it. Callers that
//! have a type context therefore pass a second test — see [`IsNumberArgument`]
//! — which catches `(eq (length xs) n)` too.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`], since such a call can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

fn is_number_literal(text: &str) -> bool {
    text.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '+' | '-' | '.')
    }) && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok())
}

fn number_argument(view: &ExpressionView) -> Option<&str> {
    atom_text(view).filter(|text| is_number_literal(text))
}

/// Why an argument counts as a number.
///
/// An enum rather than an empty `literal`, because "recognized without a
/// spelling to quote" and "recognized by the empty spelling" are different
/// facts and only one of them is ever true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumberEvidence {
    /// A numeric literal written at the call site: `(eq n 5)`.
    Literal(String),
    /// An argument a type context proves is a number however it is spelled:
    /// `(eq (length xs) n)`.
    InferredType,
}

#[derive(Debug, Clone)]
pub struct EqNumberComparisonItem {
    pub span: ByteSpan,
    /// The span of the `eq` head symbol, for an `eq` -> `eql` fix.
    ///
    pub head_span: ByteSpan,
    pub evidence: NumberEvidence,
}

impl EqNumberComparisonItem {
    /// The literal spelling this was recognized by.
    ///
    /// Empty for a type-derived detection, which the standalone `inspect`
    /// command never produces — it passes [`never`], so every item it renders
    /// carries a spelling.
    #[must_use]
    pub fn literal(&self) -> &str {
        match &self.evidence {
            NumberEvidence::Literal(text) => text,
            NumberEvidence::InferredType => "",
        }
    }
}

impl Finding for EqNumberComparisonItem {
    /// The rule's own name rather than a variant of it.
    ///
    /// [`NumberEvidence`] is the only thing that varies, and the standalone
    /// command that produces this report passes [`never`] — so every finding it
    /// can emit is a `Literal` one. A `kind` with a single reachable value
    /// would name a distinction the report cannot make.
    fn kind(&self) -> &'static str {
        "eq-number-comparison"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("literal={}", self.literal())]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("literal", json!(self.literal()))]
    }

    fn message(&self) -> String {
        match &self.evidence {
            NumberEvidence::Literal(literal) => {
                format!("eq compares against number literal {literal}")
            }
            NumberEvidence::InferredType => {
                "eq compares against an argument of inferred type number".to_owned()
            }
        }
    }
}

/// Whether an argument is provably a number without being spelled as one.
///
/// The standalone `inspect eq-number-comparison` command has no semantic
/// tables to consult, so it passes [`never`] and keeps reading literals only.
/// The lint suite passes a test backed by the type context, so it also sees
/// `(eq (length xs) n)` — the same undefined comparison, spelled in a way the
/// reader alone cannot recognize.
pub type IsNumberArgument<'a> = &'a dyn Fn(&ExpressionView) -> bool;

/// The [`IsNumberArgument`] of a caller with no type context.
const fn never(_: &ExpressionView) -> bool {
    false
}

pub fn examine_comparison(
    view: &ExpressionView,
    is_number: IsNumberArgument<'_>,
    comparison_form_count: &mut usize,
    violations: &mut Vec<EqNumberComparisonItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("eq")) {
        return;
    }
    *comparison_form_count += 1;

    // A comparison needs both operands. `(eq 1)` is a one-argument pattern in a
    // match DSL such as trivia's `(is-match 1 (eq 1))`, and SBCL already warns
    // that `eq` "is called with one argument, but wants exactly two" — so
    // whatever it is, it is not the bug this rule is named for. Counted in the
    // denominator either way: it *is* an `eq` form that was scanned.
    if view.children.len() < 3 {
        return;
    }

    // Report the first numeric argument (after the operator); a call with two
    // numbers is still one bug, not two. A literal is looked for across every
    // argument before the type context is asked about any, so a call that has
    // one is still reported by its spelling.
    let arguments = || view.children.iter().skip(1);
    let evidence = arguments()
        .find_map(number_argument)
        .map(|literal| NumberEvidence::Literal(literal.to_owned()))
        .or_else(|| {
            arguments()
                .any(is_number)
                .then_some(NumberEvidence::InferredType)
        });

    if let Some(evidence) = evidence {
        violations.push(EqNumberComparisonItem {
            span: view.span,
            head_span: view.children[0].span,
            evidence,
        });
    }
}

/// Collects every `eq` call with a numeric-literal argument in one file, with
/// the number of `eq` calls scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_eq_number_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EqNumberComparisonItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("comparison_form_count", json!(0))],
        ));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, &never, &mut comparison_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("comparison_form_count", json!(comparison_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EqNumberComparisonItem> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        build_eq_number_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build eq number comparison report")
    }

    fn comparisons(input: &str) -> (u64, Vec<EqNumberComparisonItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "comparison_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("comparison_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_eq_against_an_integer_literal() {
        let (comparison_form_count, violations) = comparisons("(eq n 5)");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal(), "5");
    }

    #[test]
    fn flags_eq_against_a_float_literal() {
        let (_, violations) = comparisons("(eq (ratio x) 1.0)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal(), "1.0");
    }

    #[test]
    fn flags_eq_against_a_negative_literal() {
        let (_, violations) = comparisons("(eq delta -1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_eql_or_numeric_equality() {
        let (comparison_form_count, violations) = comparisons("(and (eql n 5) (= n 5))");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_eq_between_two_symbols() {
        let (comparison_form_count, violations) = comparisons("(eq x y)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_increment_function_symbol() {
        // `1+` is a symbol (the increment function), not a number literal.
        let (_, violations) = comparisons("(eq op '1+)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) =
            comparisons("(defun f (n) (when (eq n 0) :zero))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn the_standalone_collector_only_ever_reports_a_spelling() {
        // It passes `never`, so the type-derived case cannot arise here and
        // the rendered `literal=` field is always populated.
        assert!(comparisons("(eq (length xs) n)").1.is_empty());
        let (_, violations) = comparisons("(eq n 5)");
        assert_eq!(
            violations[0].evidence,
            NumberEvidence::Literal("5".to_owned())
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse("(eq n 5)").expect("parse input");
        let report =
            build_eq_number_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build eq number comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eq x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_literal() {
        let report = report("(defun f (n)\n  (eq n 5))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eq-number-comparison");
        assert_eq!(finding.json_fields(), vec![("literal", json!("5"))]);
        assert_eq!(finding.text_columns(), vec!["literal=5".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_eq_scanned_not_only_the_flagged_ones() {
        let report = report("(eq n 5)\n(eq x y)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
