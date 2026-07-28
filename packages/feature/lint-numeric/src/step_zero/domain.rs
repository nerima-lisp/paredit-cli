//! Common Lisp `incf`/`decf`-by-`0` detection: an `(incf place 0)` or
//! `(decf place 0)` whose step is the literal `0`. Adding or subtracting `0`
//! changes nothing, so the form evaluates `place` and returns its value without
//! modifying it — almost always a mistake (a forgotten step, or a `0` left after
//! editing). This is a report-only smell: silently dropping the form could
//! discard a place's side effects, so no automatic fix is offered.
//!
//! Only the bare integer literal `0` is matched. A float `0.0` (which can coerce
//! the place to a float), a non-zero step, and a reader-conditional operand are
//! left alone.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare integer `0` literal (no reader prefixes; `0.0` is a
/// different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct StepZeroItem {
    pub path: PathBuf,
    /// The span of the whole `(incf place 0)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`incf` or `decf`).
    pub operator: &'static str,
}

#[derive(Debug)]
pub struct StepZeroSummary {
    pub step_form_count: usize,
    pub violations: Vec<StepZeroItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct StepZeroPolicyOptions {
    fail_on_violation: bool,
}

impl StepZeroPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct StepZeroPolicy {
    pub fail_on_violation: bool,
    pub step_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    step_form_count: &mut usize,
    violations: &mut Vec<StepZeroItem>,
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

    // children: [incf, place, step] — require the explicit-step shape.
    if view.children.len() != 3 {
        return;
    }
    let place = &view.children[1];
    let step = &view.children[2];
    if is_reader_conditional(place) || is_reader_conditional(step) {
        return;
    }
    if !is_zero_literal(step) {
        return;
    }

    violations.push(StepZeroItem {
        path: path.to_path_buf(),
        span: view.span,
        operator,
    });
}

/// Collects every `(incf place 0)`/`(decf place 0)` across a whole file, along
/// with the total number of `incf`/`decf` forms scanned.
pub fn collect_step_zeros(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<StepZeroItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut step_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut step_form_count, &mut violations);
        });
    }
    Ok((step_form_count, violations))
}

#[must_use]
pub const fn summarize_step_zeros(
    step_form_count: usize,
    violations: Vec<StepZeroItem>,
) -> StepZeroSummary {
    StepZeroSummary {
        step_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_step_zero_policy(
    options: StepZeroPolicyOptions,
    summary: &StepZeroSummary,
) -> StepZeroPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    StepZeroPolicy {
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

    fn steps(input: &str) -> (usize, Vec<StepZeroItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_step_zeros(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect step zeros")
    }

    #[test]
    fn flags_incf_zero() {
        let (count, violations) = steps("(incf counter 0)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "incf");
    }

    #[test]
    fn flags_decf_zero() {
        let (_, violations) = steps("(decf x 0)");
        assert_eq!(violations[0].operator, "decf");
    }

    #[test]
    fn does_not_flag_nonzero_step() {
        let (count, violations) = steps("(incf x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_zero() {
        assert!(steps("(incf x 0.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_default_step() {
        // (incf x) has no explicit step; not this rule's shape.
        assert!(steps("(incf x)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = steps("(INCF x 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = steps("(defun f (x) (incf x 0))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(incf x 0)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_step_zeros(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect step zeros");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = steps("(incf x 0)");
        let summary = summarize_step_zeros(count, items);

        let quiet = evaluate_step_zero_policy(StepZeroPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_step_zero_policy(StepZeroPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
