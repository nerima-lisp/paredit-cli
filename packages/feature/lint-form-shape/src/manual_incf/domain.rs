//! Common Lisp manual-`incf`/`decf` detection: a `setf`/`setq` that assigns a
//! variable a value which is that same variable plus or minus something —
//! `(setf x (1+ x))`, `(setq n (+ n 2))`, `(setf i (- i 1))`. These re-implement
//! by hand exactly what the `incf`/`decf` modify macros express, and the modify
//! macro states the intent ("bump this place") directly.
//!
//! The rewrite is only offered when the assigned place is a *bare variable*
//! (a symbol). That is the condition under which `(setf V (1+ V))` and
//! `(incf V)` are unconditionally equivalent: a symbol place is read and written
//! with no subforms to evaluate, so nothing is duplicated. A compound place like
//! `(aref a (f))` would evaluate `(f)` twice under the hand-written `setf` but
//! once under `incf`, so such forms are deliberately left alone.
//!
//! Shapes matched (with `V` the assigned variable, `D` any single form):
//!
//!   - `(setf V (1+ V))` / `(setq V (1+ V))` → `(incf V)`
//!   - `(setf V (1- V))`                     → `(decf V)`
//!   - `(setf V (+ V D))` / `(setf V (+ D V))` → `(incf V D)`  (`+` commutes)
//!   - `(setf V (- V D))`                     → `(decf V D)`  (`-` does not)
//!
//! Only the single assignment pair is handled; a multi-pair `setf` and any
//! reader-conditional operand are skipped (their shape or arity is not settled
//! statically). `V` is matched to the incremented operand by exact source text,
//! so a prefixed or differently-spelled operand does not match.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so it does not count as one settled operand.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `value` is a `1+`/`1-`/`+`/`-` form that increments or decrements the
/// variable named `place_text`, returns the suggested modify macro and the span
/// of the delta operand (`None` for the `1+`/`1-` unit step).
fn incf_decf_rewrite(
    value: &ExpressionView,
    place_text: &str,
) -> Option<(&'static str, Option<ByteSpan>)> {
    if !is_paren_list(value) {
        return None;
    }
    if value.children.iter().skip(1).any(is_reader_conditional) {
        return None;
    }
    let head = list_head(value)?;
    let operands = &value.children[1..];

    match head {
        "1+" if operands.len() == 1 && atom_text(&operands[0]) == Some(place_text) => {
            Some(("incf", None))
        }
        "1-" if operands.len() == 1 && atom_text(&operands[0]) == Some(place_text) => {
            Some(("decf", None))
        }
        "+" if operands.len() == 2 => {
            if atom_text(&operands[0]) == Some(place_text) {
                Some(("incf", Some(operands[1].span)))
            } else if atom_text(&operands[1]) == Some(place_text) {
                // `+` commutes, so `(+ D V)` is still an increment of V by D.
                Some(("incf", Some(operands[0].span)))
            } else {
                None
            }
        }
        "-" if operands.len() == 2 && atom_text(&operands[0]) == Some(place_text) => {
            // `-` does not commute: only `(- V D)` decrements V.
            Some(("decf", Some(operands[1].span)))
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct ManualIncfItem {
    pub path: PathBuf,
    /// The span of the whole `(setf V …)` form.
    pub span: ByteSpan,
    /// The suggested modify macro (`incf` or `decf`).
    pub suggested_head: &'static str,
    /// The span of the assigned variable `V` (for reconstructing the fix).
    pub place_span: ByteSpan,
    /// The span of the delta operand `D`, or `None` for a `1+`/`1-` unit step.
    pub delta_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct ManualIncfSummary {
    pub assignment_form_count: usize,
    pub violations: Vec<ManualIncfItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ManualIncfPolicyOptions {
    fail_on_violation: bool,
}

impl ManualIncfPolicyOptions {
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
pub struct ManualIncfPolicy {
    pub fail_on_violation: bool,
    pub assignment_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_assignment(
    view: &ExpressionView,
    path: &Path,
    assignment_form_count: &mut usize,
    violations: &mut Vec<ManualIncfItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("setf") && !head.eq_ignore_ascii_case("setq") {
        return;
    }
    *assignment_form_count += 1;

    // children: [setf, place, value] — exactly one assignment pair.
    if view.children.len() != 3 {
        return;
    }
    let place = &view.children[1];
    if !place.reader_prefixes.is_empty() {
        return;
    }
    // The place must be a bare variable (symbol) for the rewrite to be sound.
    let Some(place_text) = atom_text(place) else {
        return;
    };
    let value = &view.children[2];
    let Some((suggested_head, delta_span)) = incf_decf_rewrite(value, place_text) else {
        return;
    };

    violations.push(ManualIncfItem {
        path: path.to_path_buf(),
        span: view.span,
        suggested_head,
        place_span: place.span,
        delta_span,
    });
}

/// Collects every manual increment/decrement across a whole file, along with
/// the total number of setf/setq forms scanned.
pub fn collect_manual_incfs(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ManualIncfItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(subview, path, &mut assignment_form_count, &mut violations);
        });
    }
    Ok((assignment_form_count, violations))
}

#[must_use]
pub const fn summarize_manual_incfs(
    assignment_form_count: usize,
    violations: Vec<ManualIncfItem>,
) -> ManualIncfSummary {
    ManualIncfSummary {
        assignment_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_manual_incf_policy(
    options: ManualIncfPolicyOptions,
    summary: &ManualIncfSummary,
) -> ManualIncfPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ManualIncfPolicy {
        fail_on_violation: options.fail_on_violation(),
        assignment_form_count: summary.assignment_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incfs(input: &str) -> (usize, Vec<ManualIncfItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_manual_incfs(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect manual incfs")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_setf_1plus() {
        let (count, violations) = incfs("(setf x (1+ x))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "incf");
        assert!(violations[0].delta_span.is_none());
    }

    #[test]
    fn flags_setq_1minus_as_decf() {
        let (_, violations) = incfs("(setq n (1- n))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "decf");
        assert!(violations[0].delta_span.is_none());
    }

    #[test]
    fn flags_plus_with_delta() {
        let source = "(setf i (+ i 2))";
        let (_, violations) = incfs(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "incf");
        let delta = violations[0].delta_span.expect("delta span");
        assert_eq!(slice(source, delta), "2");
    }

    #[test]
    fn flags_commuted_plus() {
        // (+ 2 i) is still an increment of i by 2.
        let source = "(setf i (+ 2 i))";
        let (_, violations) = incfs(source);
        assert_eq!(violations.len(), 1);
        let delta = violations[0].delta_span.expect("delta span");
        assert_eq!(slice(source, delta), "2");
    }

    #[test]
    fn flags_minus_with_delta_as_decf() {
        let source = "(setf i (- i step))";
        let (_, violations) = incfs(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "decf");
        let delta = violations[0].delta_span.expect("delta span");
        assert_eq!(slice(source, delta), "step");
    }

    #[test]
    fn does_not_flag_non_commuting_minus() {
        // (- d i) is d - i, not a decrement of i.
        let (_, violations) = incfs("(setf i (- step i))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_different_variable() {
        let (_, violations) = incfs("(setf x (1+ y))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_compound_place() {
        // (aref a i) place: incf would change how many times subforms evaluate.
        let (_, violations) = incfs("(setf (aref a i) (1+ (aref a i)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_pair_setf() {
        let (_, violations) = incfs("(setf x (1+ x) y (1+ y))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_unrelated_value() {
        let (_, violations) = incfs("(setf x (compute))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_setf_head() {
        let (_, violations) = incfs("(SETF x (1+ x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_manual_incf() {
        let (_, violations) = incfs("(defun step () (setf counter (1+ counter)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(setf x (1+ x))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_manual_incfs(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect manual incfs");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = incfs("(setf x (1+ x))");
        let summary = summarize_manual_incfs(count, items);

        let quiet = evaluate_manual_incf_policy(ManualIncfPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_manual_incf_policy(ManualIncfPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
