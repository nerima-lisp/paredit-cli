//! Common Lisp explicit-step-delta detection: an `incf`/`decf` whose delta
//! operand is the literal `1`. The delta argument of `incf`/`decf` defaults to
//! `1`, so `(incf x 1)` is exactly `(incf x)` and `(decf x 1)` is exactly
//! `(decf x)` — same place, same step, same result. Dropping the redundant
//! delta states the unit step the way the macro was designed to express it.
//!
//! Only the bare integer literal `1` is matched. A float `1.0` is left alone:
//! `(incf x 1.0)` can coerce `x` from an integer to a float, so it is *not*
//! equivalent to `(incf x)`. A non-`1` delta, a `#x1`/prefixed spelling, a
//! variable delta, and a reader-conditional operand are all left alone.
//!
//! The fix rewrites `(incf place 1)` as `(incf place)` (and likewise for
//! `decf`), copying the head and place from their exact source, so the rule is
//! auto-fixable.
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

/// Whether `view` is the bare integer `1` literal (no reader prefixes, so `#x1`
/// and a prefixed `,1` are excluded; `1.0` is a different spelling, excluded).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("1")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ExplicitStepDeltaItem {
    pub path: PathBuf,
    /// The span of the whole `(incf place 1)` form.
    pub span: ByteSpan,
    /// The span of the `incf`/`decf` head symbol (preserves its exact source).
    pub head_span: ByteSpan,
    /// The span of the place operand (for reconstructing the fix).
    pub place_span: ByteSpan,
    /// The canonical operator name (`incf`/`decf`), for the finding message.
    pub operator: &'static str,
}

#[derive(Debug)]
pub struct ExplicitStepDeltaSummary {
    pub step_form_count: usize,
    pub violations: Vec<ExplicitStepDeltaItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ExplicitStepDeltaPolicyOptions {
    fail_on_violation: bool,
}

impl ExplicitStepDeltaPolicyOptions {
    #[must_use]
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct ExplicitStepDeltaPolicy {
    pub fail_on_violation: bool,
    pub step_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_step(
    view: &ExpressionView,
    path: &Path,
    step_form_count: &mut usize,
    violations: &mut Vec<ExplicitStepDeltaItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let operator = if head.eq_ignore_ascii_case("incf") {
        "incf"
    } else if head.eq_ignore_ascii_case("decf") {
        "decf"
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
    if !is_one_literal(delta) {
        return;
    }

    violations.push(ExplicitStepDeltaItem {
        path: path.to_path_buf(),
        span: view.span,
        head_span: view.children[0].span,
        place_span: place.span,
        operator,
    });
}

/// Collects every explicit `1` delta on an `incf`/`decf` across a whole file,
/// along with the total number of `incf`/`decf` forms scanned.
pub fn collect_explicit_step_deltas(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ExplicitStepDeltaItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut step_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_step(subview, path, &mut step_form_count, &mut violations);
        });
    }
    Ok((step_form_count, violations))
}

#[must_use]
pub fn summarize_explicit_step_deltas(
    step_form_count: usize,
    violations: Vec<ExplicitStepDeltaItem>,
) -> ExplicitStepDeltaSummary {
    ExplicitStepDeltaSummary {
        step_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_explicit_step_delta_policy(
    options: ExplicitStepDeltaPolicyOptions,
    summary: &ExplicitStepDeltaSummary,
) -> ExplicitStepDeltaPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ExplicitStepDeltaPolicy {
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

    fn steps(input: &str) -> (usize, Vec<ExplicitStepDeltaItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_explicit_step_deltas(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect explicit step deltas")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_incf_one() {
        let source = "(incf counter 1)";
        let (count, violations) = steps(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "incf");
        assert_eq!(slice(source, violations[0].place_span), "counter");
    }

    #[test]
    fn flags_decf_one() {
        let (_, violations) = steps("(decf n 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "decf");
    }

    #[test]
    fn preserves_a_compound_place() {
        let source = "(incf (aref v i) 1)";
        let (_, violations) = steps(source);
        assert_eq!(slice(source, violations[0].place_span), "(aref v i)");
    }

    #[test]
    fn does_not_flag_a_non_unit_delta() {
        let (count, violations) = steps("(incf x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_one() {
        // (incf x 1.0) can coerce x to a float; not equivalent to (incf x).
        let (_, violations) = steps("(incf x 1.0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_implicit_default() {
        // (incf x) is already the desired shape; nothing to simplify.
        let (count, violations) = steps("(incf x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_variable_delta() {
        let (_, violations) = steps("(incf x step)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = steps("(INCF x 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "incf");
    }

    #[test]
    fn finds_a_nested_step() {
        let (_, violations) = steps("(dolist (x xs) (decf total 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(incf x 1)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_explicit_step_deltas(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect explicit step deltas");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = steps("(incf x 1)");
        let summary = summarize_explicit_step_deltas(count, items);

        let quiet = evaluate_explicit_step_delta_policy(
            ExplicitStepDeltaPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_explicit_step_delta_policy(
            ExplicitStepDeltaPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
