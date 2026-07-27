//! Common Lisp manual-`pushnew` detection: a `setf`/`setq` that assigns a
//! variable the result of `adjoin`ing a new element onto that same variable —
//! `(setf set (adjoin item set))`, `(setq seen (adjoin k seen :test #'equal))`.
//! This re-implements by hand exactly what the `pushnew` modify macro expresses
//! (`pushnew` is *defined* as `(setf place (adjoin item place …))`), and
//! `pushnew` states the intent ("add to this set if new") directly.
//!
//! The rewrite is only offered when the assigned place is a *bare variable*
//! (a symbol): a symbol place has no subforms, so `(pushnew E P)` and the
//! hand-written `setf` evaluate identically. A compound place would evaluate its
//! subforms twice under the `setf` but once under `pushnew`, so such forms are
//! left alone.
//!
//! Shape matched (with `P` the assigned variable, `E` any element form, and any
//! trailing `:test`/`:key`/`:test-not` keyword arguments passed through):
//!
//!   - `(setf P (adjoin E P KW…))` → `(pushnew E P KW…)`
//!
//! `adjoin` takes `(adjoin item list &key …)`, so a `pushnew` requires the
//! *second* operand to be the place; `(adjoin P other)` (adjoining the place as
//! the element) is not flagged. Only the single assignment pair is handled; a
//! multi-pair `setf` and any reader-conditional operand are skipped, and `P` is
//! matched to the adjoin list by exact source text.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so it does not count as one settled operand.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `value` is `(adjoin E P KW…)` for the variable named `place_text`, returns
/// the span covering all of `adjoin`'s operands (`E P KW…`), which is exactly
/// the argument list `pushnew` needs. Requires at least two operands with the
/// second being the place.
fn adjoin_pushnew_args(value: &ExpressionView, place_text: &str) -> Option<ByteSpan> {
    if !is_paren_list(value) {
        return None;
    }
    if value.children.iter().skip(1).any(is_reader_conditional) {
        return None;
    }
    let head = list_head(value)?;
    if !head.eq_ignore_ascii_case("adjoin") {
        return None;
    }
    let operands = &value.children[1..];
    if operands.len() < 2 {
        return None;
    }
    // `(adjoin item list …)`: the list (second operand) must be the place.
    if atom_text(&operands[1]) != Some(place_text) {
        return None;
    }
    let first = operands.first()?;
    let last = operands.last()?;
    Some(ByteSpan::new(first.span.start(), last.span.end()))
}

#[derive(Debug, Clone)]
pub struct ManualPushnewItem {
    pub path: PathBuf,
    /// The span of the whole `(setf P (adjoin E P …))` form.
    pub span: ByteSpan,
    /// The span covering `adjoin`'s operand list (`E P KW…`), reused verbatim as
    /// `pushnew`'s argument list.
    pub args_span: ByteSpan,
}

#[derive(Debug)]
pub struct ManualPushnewSummary {
    pub assignment_form_count: usize,
    pub violations: Vec<ManualPushnewItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ManualPushnewPolicyOptions {
    fail_on_violation: bool,
}

impl ManualPushnewPolicyOptions {
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
pub struct ManualPushnewPolicy {
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
    violations: &mut Vec<ManualPushnewItem>,
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
    let Some(args_span) = adjoin_pushnew_args(value, place_text) else {
        return;
    };

    violations.push(ManualPushnewItem {
        path: path.to_path_buf(),
        span: view.span,
        args_span,
    });
}

/// Collects every manual pushnew across a whole file, along with the total
/// number of setf/setq forms scanned.
pub fn collect_manual_pushnews(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ManualPushnewItem>)> {
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
pub const fn summarize_manual_pushnews(
    assignment_form_count: usize,
    violations: Vec<ManualPushnewItem>,
) -> ManualPushnewSummary {
    ManualPushnewSummary {
        assignment_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_manual_pushnew_policy(
    options: ManualPushnewPolicyOptions,
    summary: &ManualPushnewSummary,
) -> ManualPushnewPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ManualPushnewPolicy {
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

    fn pushnews(input: &str) -> (usize, Vec<ManualPushnewItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_manual_pushnews(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect manual pushnews")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_setf_adjoin_onto_self() {
        let source = "(setf set (adjoin item set))";
        let (count, violations) = pushnews(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].args_span), "item set");
    }

    #[test]
    fn args_span_includes_keyword_arguments() {
        let source = "(setf seen (adjoin k seen :test #'equal))";
        let (_, violations) = pushnews(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].args_span),
            "k seen :test #'equal"
        );
    }

    #[test]
    fn flags_setq_adjoin() {
        let (_, violations) = pushnews("(setq xs (adjoin (compute) xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_adjoin_with_place_as_element() {
        let (_, violations) = pushnews("(setf xs (adjoin xs other))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_adjoin_onto_other_variable() {
        let (_, violations) = pushnews("(setf xs (adjoin item ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_compound_place() {
        let (_, violations) = pushnews("(setf (slot obj) (adjoin item (slot obj)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_pair_setf() {
        let (_, violations) = pushnews("(setf xs (adjoin a xs) ys (adjoin b ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_cons_which_is_manual_push() {
        // (cons …) is manual-push's job, not this rule's.
        let (_, violations) = pushnews("(setf xs (cons item xs))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_setf_and_adjoin() {
        let (_, violations) = pushnews("(SETF xs (ADJOIN item xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_manual_pushnew() {
        let (_, violations) = pushnews("(defun note (k) (setf keys (adjoin k keys)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(setf xs (adjoin item xs))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_manual_pushnews(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect manual pushnews");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = pushnews("(setf xs (adjoin item xs))");
        let summary = summarize_manual_pushnews(count, items);

        let quiet =
            evaluate_manual_pushnew_policy(ManualPushnewPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_manual_pushnew_policy(ManualPushnewPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
