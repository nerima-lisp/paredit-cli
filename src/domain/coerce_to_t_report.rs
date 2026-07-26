//! Common Lisp `coerce`-to-`t` detection: a `(coerce x t)`. Per CLHS, coercing
//! an object to type `t` returns the object unchanged — every object is of type
//! `t` — so `(coerce x t)` is exactly `x`, evaluated once. The wrapper is pure
//! noise (often a leftover after the target type was edited to `t`).
//!
//! Only the bare literal `t` result type is matched. A real coercion
//! (`(coerce x 'list)`, `(coerce x 'double-float)`) is meaningful and left alone,
//! as is a computed type argument and a reader-conditional `x`.
//!
//! The fix replaces the whole form with its object operand's source, so the rule
//! is auto-fixable.
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

/// Whether `view` is the bare literal `t` result type (no reader prefixes).
fn is_t_type(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct CoerceToTItem {
    pub path: PathBuf,
    /// The span of the whole `(coerce x t)` form.
    pub span: ByteSpan,
    /// The span of the object operand `x` (for reconstructing the fix).
    pub object_span: ByteSpan,
}

#[derive(Debug)]
pub struct CoerceToTSummary {
    pub coerce_form_count: usize,
    pub violations: Vec<CoerceToTItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CoerceToTPolicyOptions {
    fail_on_violation: bool,
}

impl CoerceToTPolicyOptions {
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
pub struct CoerceToTPolicy {
    pub fail_on_violation: bool,
    pub coerce_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    coerce_form_count: &mut usize,
    violations: &mut Vec<CoerceToTItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("coerce") {
        return;
    }
    *coerce_form_count += 1;

    // children: [coerce, object, result-type] — require exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let object = &view.children[1];
    let result_type = &view.children[2];
    if is_reader_conditional(object) {
        return;
    }
    if !is_t_type(result_type) {
        return;
    }

    violations.push(CoerceToTItem {
        path: path.to_path_buf(),
        span: view.span,
        object_span: object.span,
    });
}

/// Collects every `(coerce x t)` across a whole file, along with the total
/// number of `coerce` forms scanned.
pub fn collect_coerce_to_ts(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<CoerceToTItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut coerce_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut coerce_form_count, &mut violations);
        });
    }
    Ok((coerce_form_count, violations))
}

#[must_use]
pub fn summarize_coerce_to_ts(
    coerce_form_count: usize,
    violations: Vec<CoerceToTItem>,
) -> CoerceToTSummary {
    CoerceToTSummary {
        coerce_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_coerce_to_t_policy(
    options: CoerceToTPolicyOptions,
    summary: &CoerceToTSummary,
) -> CoerceToTPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CoerceToTPolicy {
        fail_on_violation: options.fail_on_violation(),
        coerce_form_count: summary.coerce_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coerces(input: &str) -> (usize, Vec<CoerceToTItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_coerce_to_ts(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect coerce to t")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_coerce_to_t() {
        let source = "(coerce value t)";
        let (count, violations) = coerces(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].object_span), "value");
    }

    #[test]
    fn preserves_compound_object() {
        let source = "(coerce (compute x) t)";
        let (_, violations) = coerces(source);
        assert_eq!(slice(source, violations[0].object_span), "(compute x)");
    }

    #[test]
    fn does_not_flag_real_coercion() {
        assert!(coerces("(coerce x 'list)").1.is_empty());
        assert!(coerces("(coerce x 'double-float)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_arity() {
        assert!(coerces("(coerce x)").1.is_empty());
    }

    #[test]
    fn case_folds_head_and_type() {
        let (_, violations) = coerces("(COERCE x T)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = coerces("(defun f (x) (coerce x t))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(coerce x t)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_coerce_to_ts(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect coerce to t");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = coerces("(coerce x t)");
        let summary = summarize_coerce_to_ts(count, items);

        let quiet = evaluate_coerce_to_t_policy(CoerceToTPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_coerce_to_t_policy(CoerceToTPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
