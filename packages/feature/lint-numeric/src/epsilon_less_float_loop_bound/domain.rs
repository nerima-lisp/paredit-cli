//! Common Lisp float-loop-termination detection: a `do`/`do*` whose stepped
//! variable accumulates an *inexact* float and whose end test is `=` or `eql`
//! rather than an ordered comparison.
//!
//! Adding 0.1 ten times does not produce 1.0. SBCL:
//!
//! ```text
//! (let ((x 0.0)) (dotimes (i 10) (incf x 0.1)) x)  =>  1.0000001
//! ```
//!
//! so `(do ((x 0.0 (+ x 0.1))) ((= x 1)) …)` runs forever — verified, the loop
//! returns `:NO-TERMINATION` under a trip counter. An ordered `(>= x 1)` stops
//! on the first value past the bound and is correct for any step.
//!
//! # Disjoint from `float-equality`, by construction
//!
//! `float-equality` (in `paredit-feature-lint-portability`) already reports any
//! `=`/`eql`/`equal`/`eq`/`/=`/`zerop` form holding a **written-out float
//! literal** operand, and its own documentation claims the never-terminating
//! loop as its sharpest case. `(do ((x 0.0 (+ x 0.1))) ((= x 1.0)))` is
//! therefore *already* reported today, and a rule that also reported it would
//! double-report every literal-bound loop in the suite.
//!
//! So this rule requires the end test to hold **no float literal at all**. What
//! it adds is exactly the case the other rule documents itself as unable to
//! see: a bound that is a variable, a constant, or an *integer* literal, where
//! the float-ness comes from the step rather than from the comparison.
//! `(= x 1)` and `(= x +limit+)` are both silent under `float-equality` and both
//! loop forever. The two triggers cannot both fire on one form.
//!
//! # Limits, on purpose
//!
//! - **`do`/`do*` only.** `loop`'s clause grammar is parsed by
//!   `inspect loop`, not here, and guessing at it is a false-positive source.
//!   `dotimes` cannot have a float step at all — CLHS requires its count form
//!   to be an integer — so it is not a head this rule wants.
//! - **The step literal must be inexact.** A step of 0.5, 0.25 or 0.125 is a
//!   dyadic rational and accumulates with no drift whatever, so an equality
//!   test against it is sound; SBCL confirms such a loop terminates. Only a
//!   literal needing a factor of five in its denominator — 0.1, 0.2, 0.3 — is
//!   reported. See [`crate::support::is_exact_binary_float`].
//! - **A step of 0.75 is a false negative, deliberately.** It is exactly
//!   representable, yet a loop stepping by it from 0.0 steps *over* a bound of
//!   2 rather than landing on it. That is a bound/step mismatch rather than a
//!   precision defect, and diagnosing it needs the bound's value, which this
//!   layer does not have whenever the bound is a symbol.
//!
//! Report-only: replacing `=` with `>=` changes which iteration is the last
//! one, and only the author knows whether the bound is inclusive.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

use crate::support::{float_kind, is_exact_binary_float};

/// The equality predicates that make a float loop's end test unreliable.
///
/// `equal` and `equalp` are not here: neither is a loop end test anyone writes
/// over a number, and `equalp` compares floats by mathematical value across
/// types, which is a different question.
const EQUALITY_TESTS: [&str; 2] = ["=", "eql"];

/// The step operators whose result drifts when an inexact float is involved.
const STEP_OPERATORS: [&str; 4] = ["+", "-", "*", "/"];

#[derive(Debug, Clone)]
pub struct EpsilonLessLoopItem {
    /// The span of the whole `do`/`do*` form.
    pub span: ByteSpan,
    /// The loop operator, lowercased (`do` or `do*`).
    pub operator: &'static str,
    /// The stepped variable whose accumulation drifts.
    pub variable: String,
    /// The inexact float literal the step adds each iteration.
    pub step: String,
    /// The equality predicate the end test uses (`=` or `eql`).
    pub test: String,
}

impl Finding for EpsilonLessLoopItem {
    /// Which loop form it was, a closed set of two already normalized to
    /// lowercase by [`examine`].
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("variable={}", self.variable),
            format!("step={}", self.step),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            ("variable", json!(self.variable)),
            ("step", json!(self.step)),
            ("test", json!(self.test)),
        ]
    }

    /// The same sentence the `epsilon-less-float-loop-bound` lint rule writes,
    /// so a SARIF or JUnit consumer reading both sees one finding described one
    /// way.
    fn message(&self) -> String {
        format!(
            "{} accumulates the inexact float {} each iteration, so the end test ({} {} …) may \
             never hold; compare with < or >= instead",
            self.variable, self.step, self.test, self.variable
        )
    }
}

/// The canonical loop operator name, or `None` for any other head.
fn loop_operator(head: &str) -> Option<&'static str> {
    if head.eq_ignore_ascii_case("do") {
        Some("do")
    } else if head.eq_ignore_ascii_case("do*") {
        Some("do*")
    } else {
        None
    }
}

/// Whether this operand is a written-out float literal.
///
/// The disjointness gate: an end test holding one of these is already
/// `float-equality`'s finding, and this rule stays silent on it.
fn is_float_literal_operand(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| float_kind(text).is_some())
}

/// The inexact float literal a step form accumulates, if it has one.
///
/// Only direct operands of the step form: `(+ x 0.1)` yes, `(+ x (f 0.1))` no,
/// because what `f` returns is not knowable here.
fn inexact_step_literal(step: &ExpressionView) -> Option<String> {
    let head = list_head(step)?;
    if !STEP_OPERATORS.contains(&head) {
        return None;
    }
    step.children.iter().skip(1).find_map(|operand| {
        if !operand.reader_prefixes.is_empty() {
            return None;
        }
        let text = atom_text(operand)?;
        // A float first, so an integer operand costs one first-byte test.
        float_kind(text)?;
        (!is_exact_binary_float(text)).then(|| text.to_owned())
    })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Work is bounded to the matched `do` form's own binding list and end test —
/// never a subtree walk, and never the whole file.
pub fn examine(
    view: &ExpressionView,
    do_form_count: &mut usize,
    violations: &mut Vec<EpsilonLessLoopItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = loop_operator(head) else {
        return;
    };
    *do_form_count += 1;

    // (do (bindings) (end-test result*) body*)
    if view.children.len() < 3 {
        return;
    }
    let bindings = &view.children[1];
    let end_clause = &view.children[2];
    if !is_paren_list(bindings) || !is_paren_list(end_clause) {
        return;
    }
    let Some(test) = end_clause.children.first() else {
        // `(do (…) () …)` is a deliberate infinite loop, not this defect.
        return;
    };

    let Some(test_head) = list_head(test) else {
        return;
    };
    if !EQUALITY_TESTS.contains(&test_head) {
        return;
    }
    // Exactly two operands: `(= a b c)` is not a loop bound anyone writes, and
    // reasoning about which pair drifts would be a guess.
    if test.children.len() != 3 {
        return;
    }
    // The disjointness gate. A written-out float operand here is
    // `float-equality`'s finding, not this rule's.
    if test.children.iter().skip(1).any(is_float_literal_operand) {
        return;
    }

    // One side of the test must name a variable the loop steps by an inexact
    // float.
    for operand in test.children.iter().skip(1) {
        if !operand.reader_prefixes.is_empty() {
            continue;
        }
        let Some(name) = atom_text(operand) else {
            continue;
        };
        for binding in &bindings.children {
            // (var init step) — a binding with no step form never changes.
            if !is_paren_list(binding) || binding.children.len() < 3 {
                continue;
            }
            let Some(bound) = atom_text(&binding.children[0]) else {
                continue;
            };
            if !bound.eq_ignore_ascii_case(name) {
                continue;
            }
            if let Some(step) = inexact_step_literal(&binding.children[2]) {
                violations.push(EpsilonLessLoopItem {
                    span: view.span,
                    operator,
                    variable: bound.to_owned(),
                    step,
                    test: test_head.to_ascii_lowercase(),
                });
                return;
            }
        }
    }
}

/// Collects every equality-terminated inexact float loop in one file, with the
/// number of `do`/`do*` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no drifting loop here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_epsilon_less_float_loop_bound_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EpsilonLessLoopItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("do_form_count", json!(0))],
        ));
    }

    let mut do_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut do_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("do_form_count", json!(do_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EpsilonLessLoopItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_epsilon_less_float_loop_bound_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    /// The `(do_form_count, violations)` pair the report is built from.
    fn loops(input: &str) -> (u64, Vec<EpsilonLessLoopItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "do_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("do_form_count in the summary");
        (count, report.findings)
    }

    // -- positive ------------------------------------------------------------

    /// SBCL returns :NO-TERMINATION for exactly this loop.
    #[test]
    fn flags_an_integer_bound_with_an_inexact_float_step() {
        let (count, violations) = loops("(do ((x 0.0 (+ x 0.1))) ((= x 1)) (body))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "do");
        assert_eq!(violations[0].variable, "x");
        assert_eq!(violations[0].step, "0.1");
        assert_eq!(violations[0].test, "=");
    }

    /// The case `float-equality` documents itself as unable to see.
    #[test]
    fn flags_a_symbolic_bound() {
        let (_, violations) = loops("(do ((x 0.0 (+ x 0.2))) ((= x limit)) (body))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].step, "0.2");
    }

    #[test]
    fn flags_do_star_and_eql_and_a_reversed_test() {
        assert_eq!(
            loops("(do* ((x 0.0 (+ x 0.1))) ((= x n)))").1[0].operator,
            "do*"
        );
        assert_eq!(
            loops("(do ((x 0.0 (+ x 0.1))) ((eql x n)))").1[0].test,
            "eql"
        );
        assert_eq!(loops("(do ((x 0.0 (+ x 0.1))) ((= n x)))").1.len(), 1);
    }

    #[test]
    fn flags_a_multiplicative_and_a_subtractive_drift() {
        assert_eq!(loops("(do ((x 1.0 (* x 1.1))) ((= x n)))").1.len(), 1);
        assert_eq!(loops("(do ((x 1.0 (- x 0.3))) ((= x n)))").1.len(), 1);
    }

    #[test]
    fn finds_a_loop_nested_in_a_definition() {
        assert_eq!(
            loops("(defun sweep () (do ((x 0.0 (+ x 0.1))) ((= x n)) (f x)))")
                .1
                .len(),
            1
        );
    }

    // -- the disjointness gate -----------------------------------------------

    /// A written-out float bound is `float-equality`'s finding. Reporting it
    /// here too would double-report every literal-bound loop in the suite.
    #[test]
    fn does_not_flag_a_float_literal_bound_which_float_equality_already_owns() {
        assert!(
            loops("(do ((x 0.0 (+ x 0.1))) ((= x 1.0)) (body))")
                .1
                .is_empty()
        );
        assert!(loops("(do ((x 0.0 (+ x 0.1))) ((eql x 2.5)))").1.is_empty());
    }

    // -- near-miss negatives -------------------------------------------------

    /// The trap named in the brief: an ordered comparison is the correct way to
    /// write this and must never be reported.
    #[test]
    fn does_not_flag_an_ordered_comparison() {
        for source in [
            "(do ((x 0.0 (+ x 0.1))) ((>= x 1)) (body))",
            "(do ((x 0.0 (+ x 0.1))) ((< x 1)) (body))",
            "(do ((x 0.0 (+ x 0.1))) ((> x n)))",
            "(do ((x 0.0 (+ x 0.1))) ((<= x n)))",
        ] {
            assert!(loops(source).1.is_empty(), "{source}");
        }
    }

    /// SBCL: a loop stepping by an exactly-representable 0.5 or 0.125
    /// terminates. Reporting these would be reporting correct code.
    #[test]
    fn does_not_flag_an_exactly_representable_step() {
        for source in [
            "(do ((x 0.0 (+ x 0.5))) ((= x n)))",
            "(do ((x 0.0 (+ x 0.25))) ((= x n)))",
            "(do ((x 0.0 (+ x 0.125))) ((= x n)))",
            "(do ((x 0.0 (+ x 1.0))) ((= x n)))",
        ] {
            assert!(loops(source).1.is_empty(), "{source}");
        }
    }

    #[test]
    fn does_not_flag_an_integer_step() {
        assert!(loops("(do ((i 0 (+ i 1))) ((= i n)) (body))").1.is_empty());
        assert!(loops("(do ((i 0 (1+ i))) ((= i 10)))").1.is_empty());
    }

    /// A different variable drifts than the one the test reads.
    #[test]
    fn does_not_flag_when_the_tested_variable_is_not_the_drifting_one() {
        assert!(
            loops("(do ((x 0.0 (+ x 0.1)) (i 0 (1+ i))) ((= i 10)) (body))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_binding_with_no_step_form() {
        assert!(loops("(do ((x 0.0)) ((= x n)))").1.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_end_test_or_a_malformed_form() {
        assert!(loops("(do ((x 0.0 (+ x 0.1))) ())").1.is_empty());
        assert!(loops("(do ((x 0.0 (+ x 0.1))))").1.is_empty());
        assert!(loops("(do)").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_loop_head() {
        let (count, violations) = loops("(dotimes (i 10) (body))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    /// A three-operand comparison is not a loop bound anyone writes.
    #[test]
    fn does_not_flag_a_three_operand_test() {
        assert!(loops("(do ((x 0.0 (+ x 0.1))) ((= x y n)))").1.is_empty());
    }

    /// A string is one atom, so its contents are never a step form.
    #[test]
    fn does_not_flag_a_step_spelling_inside_a_string_literal() {
        assert!(
            loops("(do ((x 0.0 (f \"(+ x 0.1)\"))) ((= x n)))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_reader_conditional_step_operand() {
        assert!(
            loops("(do ((x 0.0 (+ x #+sbcl 0.1))) ((= x n)))")
                .1
                .is_empty()
        );
    }

    // -- report plumbing -----------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let source = "(do ((x 0.0 (+ x 0.1))) ((= x 1)))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::Clojure).expect("parse");
        let report = build_epsilon_less_float_loop_bound_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("do_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(do ((i 0 (1+ i))) ((= i 10)))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_variable_and_its_step() {
        let report = report("(defun sweep ()\n  (do ((x 0.0 (+ x 0.1))) ((= x n))\n    (f x)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "do");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("do")),
                ("variable", json!("x")),
                ("step", json!("0.1")),
                ("test", json!("=")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["variable=x".to_owned(), "step=0.1".to_owned()]
        );
        assert!(finding.message().contains("never hold"));
    }

    #[test]
    fn the_summary_counts_every_do_form_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(do ((x 0.0 (+ x 0.1))) ((= x n)))\n(do ((i 0 (1+ i))) ((= i 10)))\n(do* ((a 1 a)) (nil))\n",
        );
        assert_eq!(report.summary, vec![("do_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
