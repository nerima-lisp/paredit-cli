//! Common Lisp mixed-float-precision detection: an arithmetic form holding both
//! a `single-float` literal and a `double-float` literal as direct operands,
//! where the single-float literal cannot be widened without changing its value.
//!
//! # Why the obvious version of this rule would be wrong
//!
//! "Mixed float and integer arithmetic" is *not* a defect, and this rule
//! deliberately does not report it. CLHS 12.1.4.1 (float and rational
//! contagion) and 12.1.4.4 (float precision contagion) fully determine the
//! result type of a mixed form: a rational is converted to the float format of
//! the other operand, and the result takes the largest float format present.
//! `(+ x 1)`, `(* 2 x)` and `(+ 1 2.0)` are ordinary, correct Lisp — six other
//! rules in this package (`identity-arithmetic`, `one-step-arithmetic`,
//! `explicit-step-delta`, `step-zero`, `verbose-negation`, `zero-divisor`) each
//! already carve float literals *out* of their triggers precisely because the
//! coercion they cause is meaningful and intended.
//!
//! What is not benign is mixing the two float *formats*. SBCL:
//!
//! ```text
//! (* 3.14 1.0d0) => 3.140000104904175d0
//! (* 1.5  1.0d0) => 1.5d0
//! ```
//!
//! The form is written with a double-float literal, so the author wanted double
//! precision; the single-float literal beside it silently caps the result at
//! single-float accuracy, and the error is baked into a value that *looks*
//! double-precision from then on. That is the [`RuleCategory::NumericPrecision`]
//! case: "floating-point arithmetic whose result depends on precision".
//!
//! # Limits, on purpose
//!
//! - **Only direct operands.** `(+ 3.14 (* x 1.0d0))` is not reported; the two
//!   literals are in different forms and the inner result is a double before the
//!   outer addition sees it. Bounding the check to the matched node's own
//!   children is also what keeps this rule O(1) per invocation on the four
//!   densest heads in any Lisp file.
//! - **Only literals.** `(* x y)` may well mix formats at run time and nothing
//!   at this layer can tell.
//! - **Only a lossy single-float.** `(* 1.5 x 1.0d0)` is *not* reported: 1.5 is
//!   exactly representable in both formats, so widening it changes nothing.
//!   This is the guard that keeps the rule off correct code, and it is the
//!   difference between a rule and a nuisance.
//! - **`*read-default-float-format*` is assumed to be its default.** A file that
//!   rebinds it makes `1.0` a double-float and this rule silently reports
//!   nothing extra, which is the false-negative direction.
//!
//! Report-only: inserting a `d0` changes the computed result, which is the whole
//! point, and only the author knows which of the two precisions was intended.
//!
//! Scope: Common Lisp only.
//!
//! [`RuleCategory::NumericPrecision`]: paredit_core_lint_engine::model::RuleCategory::NumericPrecision

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

use crate::support::{FloatKind, float_kind, single_float_widens_lossily};

/// The four arithmetic heads, in the canonical spelling reported back.
///
/// These are the densest heads in any Lisp file, so [`examine`]'s rejection
/// path is ordered to give up on the cheapest possible test: the head lookup,
/// then a `Vec::len`, then a first-byte check per operand.
const ARITHMETIC_OPERATORS: [&str; 4] = ["+", "-", "*", "/"];

/// The canonical operator name, or `None` when the head is not an arithmetic
/// operator. A plain byte comparison: these four names have no case to fold.
fn arithmetic_operator(head: &str) -> Option<&'static str> {
    ARITHMETIC_OPERATORS
        .iter()
        .find(|name| **name == head)
        .copied()
}

#[derive(Debug, Clone)]
pub struct MixedFloatPrecisionItem {
    /// The span of the whole arithmetic form.
    pub span: ByteSpan,
    /// The operator (`+`, `-`, `*`, `/`).
    pub operator: &'static str,
    /// The single-float literal whose precision caps the result.
    pub single: String,
    /// The double-float literal that says double precision was wanted.
    pub double: String,
}

impl Finding for MixedFloatPrecisionItem {
    /// The operator, a closed set of four, so `+` mistakes are separable from
    /// `*` ones without parsing JSON.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("single={}", self.single),
            format!("double={}", self.double),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("single", json!(self.single)),
            ("double", json!(self.double)),
        ]
    }

    /// The same sentence the `mixed-float-precision-arithmetic` lint rule
    /// writes, so a SARIF or JUnit consumer reading both sees one finding
    /// described one way.
    fn message(&self) -> String {
        format!(
            "the single-float literal {} is widened into a double-float result alongside {}, \
             capping the form at single precision; write it in double-float notation",
            self.single, self.double
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// The predicate order is load-bearing, not stylistic. On the shape the
/// `clean/forms/*` benchmarks are built from — `(defun clean-fn-N (a b) "doc"
/// (+ a (* b 2)))` — this function is invoked on every arithmetic form and must
/// reject each one having touched only: one head comparison, one `Vec::len`,
/// and one first-byte test per direct operand. Nothing allocates and nothing
/// descends.
pub fn examine(
    view: &ExpressionView,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<MixedFloatPrecisionItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = arithmetic_operator(head) else {
        return;
    };
    *arithmetic_form_count += 1;

    // Two literals cannot both be operands of a form with fewer than two of
    // them. A `Vec::len` before any text is read.
    //
    // A *performance* guard, not a correctness one, and mutation-verified as
    // such: removing it changes no verdict, because the loop below cannot find
    // both a single and a double among fewer than two operands. It is here
    // because these four heads are the densest in any Lisp file and this is the
    // cheapest possible way to reject most of them.
    if view.children.len() < 3 {
        return;
    }

    let mut double: Option<&str> = None;
    let mut single: Option<&str> = None;
    for child in view.children.iter().skip(1) {
        // A quoted operand is a datum, not a number being added.
        //
        // Defence in depth rather than the deciding test, and mutation-verified
        // as such: `atom_text` keeps the prefix (`'3.14` reads back as
        // `"'3.14"`), and a reader conditional folds into a single atom whose
        // text starts with `#`, so `float_kind` already rejects both on their
        // first byte. Kept because relying on that coincidence would make this
        // loop wrong the moment `atom_text` starts stripping prefixes.
        if !child.reader_prefixes.is_empty() {
            continue;
        }
        // `None` for every list operand, so a nested form costs one match.
        let Some(text) = atom_text(child) else {
            continue;
        };
        // `float_kind` gives up after one byte on a symbol, which is what
        // almost every operand of almost every arithmetic form is.
        match float_kind(text) {
            Some(FloatKind::Double) if double.is_none() => double = Some(text),
            // `single.is_none()` is tested before the parse on purpose: the
            // parse is the only expensive step in this loop, and it is reached
            // only by an operand already known to be a single-float literal —
            // never by a symbol, an integer, or a list.
            Some(FloatKind::Single) if single.is_none() && single_float_widens_lossily(text) => {
                single = Some(text);
            }
            _ => {}
        }
        if let (Some(_), Some(_)) = (double, single) {
            break;
        }
    }

    let (Some(double), Some(single)) = (double, single) else {
        return;
    };
    violations.push(MixedFloatPrecisionItem {
        span: view.span,
        operator,
        single: single.to_owned(),
        double: double.to_owned(),
    });
}

/// Collects every mixed-precision arithmetic form in one file, with the number
/// of arithmetic forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no mixed precision here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_mixed_float_precision_arithmetic_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MixedFloatPrecisionItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("arithmetic_form_count", json!(0))],
        ));
    }

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut arithmetic_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("arithmetic_form_count", json!(arithmetic_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MixedFloatPrecisionItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_mixed_float_precision_arithmetic_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    /// The `(arithmetic_form_count, violations)` pair the report is built from.
    fn mixed(input: &str) -> (u64, Vec<MixedFloatPrecisionItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "arithmetic_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("arithmetic_form_count in the summary");
        (count, report.findings)
    }

    // -- positive ------------------------------------------------------------

    /// SBCL prints this as 3.140000104904175d0, not 3.14d0.
    #[test]
    fn flags_a_lossy_single_float_beside_a_double_float() {
        let (count, violations) = mixed("(* 3.14 1.0d0)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "*");
        assert_eq!(violations[0].single, "3.14");
        assert_eq!(violations[0].double, "1.0d0");
    }

    #[test]
    fn flags_every_arithmetic_operator() {
        for (source, operator) in [
            ("(+ 0.1 2.0d0)", "+"),
            ("(- 0.1 2.0d0)", "-"),
            ("(* 0.1 2.0d0)", "*"),
            ("(/ 0.1 2.0d0)", "/"),
        ] {
            let (_, violations) = mixed(source);
            assert_eq!(violations.len(), 1, "{source}");
            assert_eq!(violations[0].operator, operator, "{source}");
        }
    }

    #[test]
    fn flags_regardless_of_operand_order_and_finds_a_nested_form() {
        assert_eq!(mixed("(+ 1.0d0 3.14)").1.len(), 1);
        assert_eq!(mixed("(defun f (x) (* x 3.14 1.0d0))").1.len(), 1);
    }

    /// A long-float literal is wider than single too.
    #[test]
    fn flags_a_long_float_as_the_wide_operand() {
        assert_eq!(mixed("(* 3.14 1.0L0)").1.len(), 1);
    }

    // -- near-miss negatives -------------------------------------------------

    /// The trap named in the brief: an exact integer beside a float is ordinary,
    /// fully-specified contagion, and six other rules in this package rely on it
    /// being meaningful.
    #[test]
    fn does_not_flag_an_integer_beside_a_float() {
        assert!(mixed("(+ x 1)").1.is_empty());
        assert!(mixed("(* 2 x)").1.is_empty());
        assert!(mixed("(+ 1 2.0)").1.is_empty());
        assert!(mixed("(+ 1 2.0d0)").1.is_empty());
        assert!(mixed("(* 1/3 2.0d0)").1.is_empty());
    }

    /// Uniform precision is exactly what this rule wants people to write.
    #[test]
    fn does_not_flag_a_uniform_precision_form() {
        assert!(mixed("(* 3.14 2.0)").1.is_empty());
        assert!(mixed("(* 3.14d0 2.0d0)").1.is_empty());
    }

    /// SBCL: `(* 1.5 1.0d0)` => 1.5d0 exactly. Reporting these would be
    /// reporting correct code, and this is the guard that prevents it.
    #[test]
    fn does_not_flag_a_single_float_that_widens_exactly() {
        for source in [
            "(* 1.5 1.0d0)",
            "(+ 2.0 1.0d0)",
            "(* 0.25 4.0d0)",
            "(- 100.0 1.0d0)",
            "(/ 0.5 2.0d0)",
        ] {
            assert!(mixed(source).1.is_empty(), "{source}");
        }
    }

    /// The two literals are in different forms; the inner one is already a
    /// double by the time the outer addition sees it.
    #[test]
    fn does_not_flag_literals_in_different_forms() {
        let (count, violations) = mixed("(+ 3.14 (* x 1.0d0))");
        assert_eq!(count, 2, "both forms are counted");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_form_with_a_single_operand() {
        assert!(mixed("(- 3.14)").1.is_empty());
        assert!(mixed("(/ 1.0d0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_arithmetic_head() {
        let (count, violations) = mixed("(max 3.14 1.0d0)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// A string is one atom, so its contents are never operands.
    #[test]
    fn does_not_flag_a_float_spelling_inside_a_string_literal() {
        assert!(mixed("(+ \"3.14\" \"1.0d0\")").1.is_empty());
        assert!(
            mixed("(format nil \"~A\" (+ x \"3.14 1.0d0\"))")
                .1
                .is_empty()
        );
    }

    /// Neither a build-dependent operand nor a quoted datum is a number this
    /// rule may reason about.
    ///
    /// Both are rejected by `float_kind` on their first byte rather than by the
    /// `reader_prefixes` check: a reader conditional folds into one atom whose
    /// text is `"#+sbcl 3.14"`, and `atom_text` returns `'3.14` with its quote
    /// still attached. The assertion is on the verdict, which is what callers
    /// depend on, not on which of the two guards produced it.
    #[test]
    fn does_not_flag_a_reader_conditional_or_quoted_operand() {
        assert!(mixed("(* #+sbcl 3.14 1.0d0)").1.is_empty());
        assert!(mixed("(* '3.14 1.0d0)").1.is_empty());
        assert!(mixed("(* 3.14 '1.0d0)").1.is_empty());
    }

    // -- report plumbing -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(* 3.14 1.0d0)", Dialect::Clojure).expect("parse");
        let report = build_mixed_float_precision_arithmetic_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(+ x 1)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_literals() {
        let report = report("(defun scale (x)\n  (* x 3.14 1.0d0))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "*");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("*")),
                ("single", json!("3.14")),
                ("double", json!("1.0d0")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["single=3.14".to_owned(), "double=1.0d0".to_owned()]
        );
        assert!(finding.message().contains("single precision"));
    }

    #[test]
    fn the_summary_counts_every_arithmetic_form_scanned_not_only_the_flagged_ones() {
        let report = report("(* 3.14 1.0d0)\n(+ a b)\n(- x 1)\n");
        assert_eq!(report.summary, vec![("arithmetic_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
