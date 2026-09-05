//! Common Lisp float-round-trip detection: a `truncate`/`floor`/`ceiling`/
//! `round` whose argument is directly a `(coerce … 'double-float)` or
//! `(float …)`.
//!
//! The float conversion is discarded by the very next operation, and it is not
//! free: it *changes the answer*. SBCL:
//!
//! ```text
//! (truncate 123456789123456789)          => 123456789123456789
//! (truncate (float 123456789123456789))  => 123456790519087104
//! ```
//!
//! — off by 1,395,630,315. The mechanism is that `coerce` rounds to the nearest
//! representable float (CLHS: "equal in sign and magnitude to the object to
//! whatever degree of representational precision is permitted"), and `truncate`
//! is discontinuous at every integer, so any rounding at all can be amplified
//! into a full unit of error. Two independent failure modes:
//!
//! - **Above the significand width** distinct integers collapse: `2^53+1`
//!   coerces to `2^53`, so the truncation is off by one.
//! - **Across an integer boundary** a rational strictly below `n` but within
//!   half an ULP of it rounds *up*: `(truncate 99999999999999999999/10^20)` is
//!   `0`, but coercing first gives `1.0d0` and truncates to `1`.
//!
//! # Never fixable, and that is a finding in itself
//!
//! It is tempting to read this as "delete the coercion". That rewrite is
//! **unsound**: the two forms are genuinely different functions, and dropping
//! the `coerce` would silently change the result on exactly the inputs above.
//! Inserting one is no better. [`Fixability::ReportOnly`] here is a correctness
//! requirement, not a convenience.
//!
//! # Limits, on purpose
//!
//! - **Only a direct argument.** `(truncate (* 2 (float x)))` is not reported;
//!   the intervening arithmetic may be the point.
//! - **Only single-argument truncation.** `(truncate x 1.0d0)` is a two-argument
//!   call whose float divisor makes the remainder a float, which is a different
//!   and deliberate thing.
//! - **The deliberate round-trip is a different shape.** Rounding a value to an
//!   integer *is* the operation in `(truncate x)`, and that has no coercion in
//!   it, so it is never this rule's subject. What is reported is only the
//!   redundant conversion wrapped immediately inside it.
//!
//! Scope: Common Lisp only.
//!
//! [`Fixability::ReportOnly`]: paredit_core_lint_engine::model::Fixability::ReportOnly

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The integer-producing operators whose float argument is thrown away.
///
/// The `f`-prefixed family (`ffloor`, `fround`, …) is deliberately absent:
/// those return *floats*, so a float argument is not discarded and there is
/// nothing redundant about producing one.
const TRUNCATING_OPERATORS: [&str; 4] = ["truncate", "floor", "ceiling", "round"];

/// The float formats a `coerce` target names. Coercing to `short-float` or
/// `single-float` is if anything worse than to `double-float`, so all of them
/// count.
const FLOAT_TYPES: [&str; 5] = [
    "double-float",
    "single-float",
    "short-float",
    "long-float",
    "float",
];

#[derive(Debug, Clone)]
pub struct RedundantPrecisionCoercionItem {
    /// The span of the whole `(truncate (coerce …))` form.
    pub span: ByteSpan,
    /// The truncating operator, lowercased.
    pub operator: &'static str,
    /// How the coercion was spelled — `coerce` or `float`.
    pub coercion: &'static str,
}

impl Finding for RedundantPrecisionCoercionItem {
    /// The truncating operator, a closed set of four already normalized to
    /// lowercase by [`examine`].
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("coercion={}", self.coercion)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("coercion", json!(self.coercion)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} discards the float {} produces, and the conversion can change the result: \
             rounding to the nearest float can cross an integer boundary that {} is keyed on",
            self.operator, self.coercion, self.operator
        )
    }
}

/// The canonical truncating-operator name, or `None` for any other head.
fn truncating_operator(head: &str) -> Option<&'static str> {
    TRUNCATING_OPERATORS
        .iter()
        .find(|name| head.eq_ignore_ascii_case(name))
        .copied()
}

/// How `view` spells a float conversion, or `None` when it is not one.
///
/// `(float x)` and `(float x prototype)` both count. `(coerce x 'double-float)`
/// counts; `(coerce x 'list)` does not — that is a real conversion and
/// `coerce-to-t`'s neighbourhood, not this rule's.
fn float_coercion(view: &ExpressionView) -> Option<&'static str> {
    let head = list_head(view)?;
    if head.eq_ignore_ascii_case("float") {
        // `(float)` is not a call this can read.
        return (view.children.len() >= 2).then_some("float");
    }
    if !head.eq_ignore_ascii_case("coerce") {
        return None;
    }
    let target = view.children.get(2)?;
    let text = atom_text(target)?;
    // The target is quoted: `'double-float`. The reader keeps the quote as a
    // prefix, so the atom text is the bare type name.
    let name = text.trim_start_matches('\'');
    FLOAT_TYPES
        .iter()
        .any(|float| name.eq_ignore_ascii_case(float))
        .then_some("coerce")
}

///
/// Work is bounded to the matched form's own first argument.
pub fn examine(
    view: &ExpressionView,
    truncation_form_count: &mut usize,
    violations: &mut Vec<RedundantPrecisionCoercionItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = truncating_operator(head) else {
        return;
    };
    *truncation_form_count += 1;

    // Exactly `(op arg)`. A second argument is a divisor, which makes the call
    // a different operation entirely.
    if view.children.len() != 2 {
        return;
    }
    let argument = &view.children[1];
    if !argument.reader_prefixes.is_empty() {
        return;
    }
    let Some(coercion) = float_coercion(argument) else {
        return;
    };

    violations.push(RedundantPrecisionCoercionItem {
        span: view.span,
        operator,
        coercion,
    });
}

/// Collects every discarded float coercion in one file, with the number of
/// truncating forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_redundant_precision_coercion_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantPrecisionCoercionItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("truncation_form_count", json!(0))],
        ));
    }

    let mut truncation_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut truncation_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("truncation_form_count", json!(truncation_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantPrecisionCoercionItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_precision_coercion_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn coercions(input: &str) -> (u64, Vec<RedundantPrecisionCoercionItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "truncation_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("truncation_form_count in the summary");
        (count, report.findings)
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_a_coerce_to_double_float_inside_a_truncate() {
        let (count, violations) = coercions("(truncate (coerce x 'double-float))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "truncate");
        assert_eq!(violations[0].coercion, "coerce");
    }

    /// SBCL: (truncate (float 123456789123456789)) is off by 1,395,630,315.
    #[test]
    fn flags_a_float_call_inside_a_truncate() {
        let (_, violations) = coercions("(truncate (float n))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].coercion, "float");
    }

    #[test]
    fn flags_every_truncating_operator() {
        for (source, operator) in [
            ("(truncate (float x))", "truncate"),
            ("(floor (float x))", "floor"),
            ("(ceiling (float x))", "ceiling"),
            ("(round (float x))", "round"),
        ] {
            let (_, violations) = coercions(source);
            assert_eq!(violations.len(), 1, "{source}");
            assert_eq!(violations[0].operator, operator, "{source}");
        }
    }

    #[test]
    fn flags_every_float_coercion_target_and_a_two_argument_float() {
        for source in [
            "(truncate (coerce x 'single-float))",
            "(truncate (coerce x 'short-float))",
            "(truncate (coerce x 'long-float))",
            "(truncate (coerce x 'float))",
            "(truncate (float x 1.0d0))",
        ] {
            assert_eq!(coercions(source).1.len(), 1, "{source}");
        }
    }

    #[test]
    fn case_folds_the_heads_and_finds_a_nested_form() {
        assert_eq!(coercions("(TRUNCATE (FLOAT x))").1.len(), 1);
        assert_eq!(
            coercions("(defun whole (x) (truncate (coerce x 'double-float)))")
                .1
                .len(),
            1
        );
    }

    // -- near-miss negatives -------------------------------------------------

    /// The trap named in the brief: rounding to an integer *is* the operation,
    /// and that shape has no coercion in it at all.
    #[test]
    fn does_not_flag_a_deliberate_truncation_with_no_coercion() {
        for source in [
            "(truncate x)",
            "(round 3.7)",
            "(floor (/ a b))",
            "(ceiling (* x 2))",
        ] {
            assert!(coercions(source).1.is_empty(), "{source}");
        }
    }

    /// A second argument is a divisor; the call is a different operation.
    #[test]
    fn does_not_flag_a_two_argument_truncation() {
        assert!(coercions("(truncate (float x) 2)").1.is_empty());
        assert!(coercions("(truncate x 1.0d0)").1.is_empty());
    }

    /// A real conversion to a non-float type is meaningful and is left alone.
    #[test]
    fn does_not_flag_a_coercion_to_a_non_float_type() {
        for source in [
            "(truncate (coerce x 'list))",
            "(truncate (coerce x 'integer))",
            "(truncate (coerce x 'rational))",
        ] {
            assert!(coercions(source).1.is_empty(), "{source}");
        }
    }

    /// Intervening arithmetic may well be the point.
    #[test]
    fn does_not_flag_a_coercion_that_is_not_the_direct_argument() {
        assert!(coercions("(truncate (* 2 (float x)))").1.is_empty());
        assert!(
            coercions("(truncate (+ (coerce x 'double-float) 1))")
                .1
                .is_empty()
        );
    }

    /// The f-prefixed family returns a float, so the argument is not discarded.
    #[test]
    fn does_not_flag_the_float_returning_truncation_family() {
        for source in [
            "(ffloor (float x))",
            "(fround (float x))",
            "(ftruncate (float x))",
            "(fceiling (float x))",
        ] {
            let (count, violations) = coercions(source);
            assert_eq!(count, 0, "{source}");
            assert!(violations.is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_a_bare_coercion_outside_a_truncation() {
        let (count, violations) = coercions("(coerce x 'double-float)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_call() {
        assert!(coercions("(truncate)").1.is_empty());
        assert!(coercions("(truncate (float))").1.is_empty());
        assert!(coercions("(truncate (coerce x))").1.is_empty());
    }

    /// A string is one atom, so its contents are never forms.
    #[test]
    fn does_not_flag_a_coercion_spelling_inside_a_string_literal() {
        assert!(
            coercions("(format nil \"(truncate (float x))\")")
                .1
                .is_empty()
        );
        assert!(coercions("(truncate \"(float x)\")").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_reader_conditional_argument() {
        assert!(coercions("(truncate #+sbcl (float x))").1.is_empty());
    }

    // -- report plumbing -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let source = "(truncate (float x))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse");
        let report = build_redundant_precision_coercion_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("truncation_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(truncate x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_coercion() {
        let report = report("(defun whole (x)\n  (round (coerce x 'double-float)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "round");
        assert_eq!(
            finding.json_fields(),
            vec![("operator", json!("round")), ("coercion", json!("coerce"))]
        );
        assert_eq!(finding.text_columns(), vec!["coercion=coerce".to_owned()]);
        assert!(finding.message().contains("integer boundary"));
    }

    #[test]
    fn the_summary_counts_every_truncation_form_scanned_not_only_the_flagged_ones() {
        let report = report("(truncate (float x))\n(truncate y)\n(floor a b)\n");
        assert_eq!(report.summary, vec![("truncation_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
