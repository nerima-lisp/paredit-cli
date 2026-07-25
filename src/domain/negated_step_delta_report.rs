//! Common Lisp negated-step-delta detection: an `incf`/`decf` whose delta is a
//! *negative numeric literal* — `(incf x -1)`, `(decf n -5)`. Adding a negative
//! delta is subtracting its magnitude, so `(incf place -N)` is exactly
//! `(decf place N)` and `(decf place -N)` is `(incf place N)`: the operator
//! flips and the literal loses its sign. The positive-delta form states the
//! direction of the step directly.
//!
//! Only a bare negative numeric literal delta is matched (integer, ratio, or
//! float — the sign is simply dropped, which preserves the numeric type). A
//! variable delta, a positive literal, and a reader-conditional operand are all
//! left alone.
//!
//! The fix rewrites `(incf x -1)` as `(decf x 1)` (and vice versa); a resulting
//! unit `(decf x 1)` is then reduced to `(decf x)` by the `explicit-step-delta`
//! rule on a later pass. Auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is a bare negative numeric literal (`-1`, `-5`, `-1.0`,
/// `-1/2`): a `-` immediately followed by a digit or decimal point, with no
/// reader prefix.
fn is_negative_number_literal(view: &ExpressionView) -> bool {
    if !view.reader_prefixes.is_empty() {
        return false;
    }
    atom_text(view).is_some_and(|text| {
        let mut chars = text.chars();
        chars.next() == Some('-')
            && matches!(chars.next(), Some(c) if c.is_ascii_digit() || c == '.')
    })
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NegatedStepDeltaItem {
    pub path: PathBuf,
    /// The span of the whole `(incf place -N)` form.
    pub span: ByteSpan,
    /// The span of the place operand.
    pub place_span: ByteSpan,
    /// The span of the negative delta literal (`-N`).
    pub delta_span: ByteSpan,
    /// The flipped operator to emit (`decf` for `incf`, `incf` for `decf`).
    pub opposite: &'static str,
}

#[derive(Debug)]
pub struct NegatedStepDeltaSummary {
    pub step_form_count: usize,
    pub violations: Vec<NegatedStepDeltaItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NegatedStepDeltaPolicyOptions {
    fail_on_violation: bool,
}

impl NegatedStepDeltaPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NegatedStepDeltaPolicy {
    pub fail_on_violation: bool,
    pub step_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_step(
    view: &ExpressionView,
    path: &Path,
    step_form_count: &mut usize,
    violations: &mut Vec<NegatedStepDeltaItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let opposite = if head.eq_ignore_ascii_case("incf") {
        "decf"
    } else if head.eq_ignore_ascii_case("decf") {
        "incf"
    } else {
        return;
    };
    *step_form_count += 1;

    // children: [incf, place, delta] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let place = &view.children[1];
    let delta = &view.children[2];
    if is_reader_conditional(place) || is_reader_conditional(delta) {
        return;
    }
    if !is_negative_number_literal(delta) {
        return;
    }

    violations.push(NegatedStepDeltaItem {
        path: path.to_path_buf(),
        span: view.span,
        place_span: place.span,
        delta_span: delta.span,
        opposite,
    });
}

/// Collects every `incf`/`decf` with a negative literal delta across a whole
/// file, along with the total number of `incf`/`decf` forms scanned.
pub fn collect_negated_step_deltas(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NegatedStepDeltaItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut step_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_step(subview, path, &mut step_form_count, &mut violations)
        });
    }
    Ok((step_form_count, violations))
}

pub fn summarize_negated_step_deltas(
    step_form_count: usize,
    violations: Vec<NegatedStepDeltaItem>,
) -> NegatedStepDeltaSummary {
    NegatedStepDeltaSummary {
        step_form_count,
        violations,
    }
}

pub fn evaluate_negated_step_delta_policy(
    options: NegatedStepDeltaPolicyOptions,
    summary: &NegatedStepDeltaSummary,
) -> NegatedStepDeltaPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NegatedStepDeltaPolicy {
        fail_on_violation: options.fail_on_violation(),
        step_form_count: summary.step_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(input: &str) -> (usize, Vec<NegatedStepDeltaItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_negated_step_deltas(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect negated step deltas")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_incf_negative_one() {
        let source = "(incf counter -1)";
        let (count, violations) = steps(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].opposite, "decf");
        assert_eq!(slice(source, violations[0].place_span), "counter");
        assert_eq!(slice(source, violations[0].delta_span), "-1");
    }

    #[test]
    fn flags_decf_negative_five() {
        let (_, violations) = steps("(decf n -5)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].opposite, "incf");
    }

    #[test]
    fn flags_negative_float() {
        let source = "(incf x -1.5)";
        let (_, violations) = steps(source);
        assert_eq!(slice(source, violations[0].delta_span), "-1.5");
    }

    #[test]
    fn preserves_a_compound_place() {
        let source = "(incf (aref v i) -2)";
        let (_, violations) = steps(source);
        assert_eq!(slice(source, violations[0].place_span), "(aref v i)");
    }

    #[test]
    fn does_not_flag_positive_delta() {
        let (count, violations) = steps("(incf x 1)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_variable_delta() {
        let (_, violations) = steps("(incf x neg-step)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_minus_symbol() {
        // (incf x -) has the `-` symbol as delta, not a negative number.
        let (_, violations) = steps("(incf x -)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_implicit_delta() {
        let (count, violations) = steps("(incf x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = steps("(INCF x -1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].opposite, "decf");
    }

    #[test]
    fn finds_a_nested_step() {
        let (_, violations) = steps("(dolist (x xs) (incf total -1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(incf x -1)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_negated_step_deltas(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect negated step deltas");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = steps("(incf x -1)");
        let summary = summarize_negated_step_deltas(count, items);

        let quiet =
            evaluate_negated_step_delta_policy(NegatedStepDeltaPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_negated_step_delta_policy(NegatedStepDeltaPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
