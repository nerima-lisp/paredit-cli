//! Common Lisp manual-`push` detection: a `setf`/`setq` that assigns a variable
//! the result of consing a new element onto that same variable —
//! `(setf stack (cons item stack))`, `(setq xs (cons x xs))`. This re-implements
//! by hand exactly what the `push` modify macro expresses, and `push` states the
//! intent ("prepend to this place") directly.
//!
//! The rewrite is only offered when the assigned place is a *bare variable*
//! (a symbol). That is the condition under which `(setf P (cons E P))` and
//! `(push E P)` are unconditionally equivalent: a symbol place is read and
//! written with no subforms to evaluate, so nothing is duplicated. A compound
//! place like `(car node)` would evaluate `node` twice under the hand-written
//! `setf` but once under `push`, so such forms are deliberately left alone.
//!
//! Shape matched (with `P` the assigned variable and `E` any single form):
//!
//!   - `(setf P (cons E P))` / `(setq P (cons E P))` → `(push E P)`
//!
//! `cons` takes `(cons element list)`, so a push requires the *second* operand
//! to be the place; `(cons P E)` (consing the place onto something else) is not
//! a push and is not flagged. Only the single assignment pair is handled; a
//! multi-pair `setf` and any reader-conditional operand are skipped, and `P` is
//! matched to the cons tail by exact source text.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so it does not count as one settled operand.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `value` is `(cons E P)` for the variable named `place_text`, returns the
/// span of the pushed element `E`. Requires exactly two operands, the second
/// being the place.
fn cons_push_element(value: &ExpressionView, place_text: &str) -> Option<ByteSpan> {
    if !is_paren_list(value) {
        return None;
    }
    if value.children.iter().skip(1).any(is_reader_conditional) {
        return None;
    }
    let head = list_head(value)?;
    if !head.eq_ignore_ascii_case("cons") {
        return None;
    }
    let operands = &value.children[1..];
    if operands.len() != 2 {
        return None;
    }
    // `(cons element list)`: the list (second operand) must be the place.
    (atom_text(&operands[1]) == Some(place_text)).then_some(operands[0].span)
}

#[derive(Debug, Clone)]
pub struct ManualPushItem {
    pub path: PathBuf,
    /// The span of the whole `(setf P (cons E P))` form.
    pub span: ByteSpan,
    /// The span of the assigned variable `P` (for reconstructing the fix).
    pub place_span: ByteSpan,
    /// The span of the pushed element `E`.
    pub element_span: ByteSpan,
}

#[derive(Debug)]
pub struct ManualPushSummary {
    pub assignment_form_count: usize,
    pub violations: Vec<ManualPushItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ManualPushPolicyOptions {
    fail_on_violation: bool,
}

impl ManualPushPolicyOptions {
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
pub struct ManualPushPolicy {
    pub fail_on_violation: bool,
    pub assignment_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_assignment(
    view: &ExpressionView,
    path: &Path,
    assignment_form_count: &mut usize,
    violations: &mut Vec<ManualPushItem>,
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
    let Some(element_span) = cons_push_element(value, place_text) else {
        return;
    };

    violations.push(ManualPushItem {
        path: path.to_path_buf(),
        span: view.span,
        place_span: place.span,
        element_span,
    });
}

/// Collects every manual push across a whole file, along with the total number
/// of setf/setq forms scanned.
pub fn collect_manual_pushes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<ManualPushItem>)> {
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
pub const fn summarize_manual_pushes(
    assignment_form_count: usize,
    violations: Vec<ManualPushItem>,
) -> ManualPushSummary {
    ManualPushSummary {
        assignment_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_manual_push_policy(
    options: ManualPushPolicyOptions,
    summary: &ManualPushSummary,
) -> ManualPushPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ManualPushPolicy {
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

    fn pushes(input: &str) -> (usize, Vec<ManualPushItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_manual_pushes(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect manual pushes")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_setf_cons_onto_self() {
        let source = "(setf stack (cons item stack))";
        let (count, violations) = pushes(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].element_span), "item");
        assert_eq!(slice(source, violations[0].place_span), "stack");
    }

    #[test]
    fn flags_setq_cons() {
        let (_, violations) = pushes("(setq xs (cons (compute) xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_cons_with_place_as_element() {
        // (cons xs other) conses the place as the element, not onto it.
        let (_, violations) = pushes("(setf xs (cons xs other))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_cons_onto_other_variable() {
        let (_, violations) = pushes("(setf xs (cons item ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_compound_place() {
        let (_, violations) = pushes("(setf (car node) (cons item (car node)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_argument_cons() {
        // A malformed 3-arg cons is not a push; leave it alone.
        let (_, violations) = pushes("(setf xs (cons a b xs))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_pair_setf() {
        let (_, violations) = pushes("(setf xs (cons a xs) ys (cons b ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_unrelated_value() {
        let (_, violations) = pushes("(setf xs (reverse xs))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_setf_and_cons() {
        let (_, violations) = pushes("(SETF xs (CONS item xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_manual_push() {
        let (_, violations) = pushes("(defun record (item) (setf log (cons item log)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(setf xs (cons item xs))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_manual_pushes(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect manual pushes");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = pushes("(setf xs (cons item xs))");
        let summary = summarize_manual_pushes(count, items);

        let quiet = evaluate_manual_push_policy(ManualPushPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_manual_push_policy(ManualPushPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
