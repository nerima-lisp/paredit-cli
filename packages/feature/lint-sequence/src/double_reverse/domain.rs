//! Common Lisp double-`reverse` detection: a `(reverse (reverse x))`. `reverse`
//! returns a fresh sequence of the same kind as its argument with the elements
//! in opposite order; reversing that fresh sequence again restores the original
//! order, yielding a fresh sequence of `x`'s kind whose elements equal `x`'s.
//! That is exactly `(copy-seq x)` — a wasteful, obfuscated copy.
//!
//! Only the non-destructive `reverse` is matched on *both* levels. The
//! destructive `nreverse` is deliberately excluded: `(nreverse (nreverse x))`
//! mutates `x`'s structure twice and cannot be reasoned about as a plain copy,
//! and a mixed `reverse`/`nreverse` nesting is a common deliberate
//! build-then-nreverse idiom, so neither is flagged. A reader-conditional inner
//! operand is left alone (build-dependent).
//!
//! The fix rewrites `(reverse (reverse x))` as `(copy-seq x)`, copying the inner
//! argument's source verbatim, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether `view` is a `(reverse arg)` call with exactly one operand; returns the
/// operand.
fn single_arg_reverse(view: &ExpressionView) -> Option<&ExpressionView> {
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("reverse") {
        return None;
    }
    // children: [reverse, arg] — exactly one operand.
    if view.children.len() != 2 {
        return None;
    }
    Some(&view.children[1])
}

#[derive(Debug, Clone)]
pub struct DoubleReverseItem {
    pub path: PathBuf,
    /// The span of the whole `(reverse (reverse x))` form.
    pub span: ByteSpan,
    /// The span of the innermost argument `x` (for reconstructing the fix).
    pub inner_span: ByteSpan,
}

#[derive(Debug)]
pub struct DoubleReverseSummary {
    pub reverse_form_count: usize,
    pub violations: Vec<DoubleReverseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DoubleReversePolicyOptions {
    fail_on_violation: bool,
}

impl DoubleReversePolicyOptions {
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
pub struct DoubleReversePolicy {
    pub fail_on_violation: bool,
    pub reverse_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    reverse_form_count: &mut usize,
    violations: &mut Vec<DoubleReverseItem>,
) {
    let Some(outer_arg) = single_arg_reverse(view) else {
        return;
    };
    *reverse_form_count += 1;

    // The single argument must itself be a `(reverse x)` call.
    if !is_paren_list(outer_arg) {
        return;
    }
    let Some(inner_arg) = single_arg_reverse(outer_arg) else {
        return;
    };
    if is_reader_conditional(inner_arg) {
        return;
    }

    violations.push(DoubleReverseItem {
        path: path.to_path_buf(),
        span: view.span,
        inner_span: inner_arg.span,
    });
}

/// Collects every `(reverse (reverse x))` across a whole file, along with the
/// total number of single-argument `reverse` forms scanned.
pub fn collect_double_reverses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DoubleReverseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut reverse_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut reverse_form_count, &mut violations);
        });
    }
    Ok((reverse_form_count, violations))
}

#[must_use]
pub const fn summarize_double_reverses(
    reverse_form_count: usize,
    violations: Vec<DoubleReverseItem>,
) -> DoubleReverseSummary {
    DoubleReverseSummary {
        reverse_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_double_reverse_policy(
    options: DoubleReversePolicyOptions,
    summary: &DoubleReverseSummary,
) -> DoubleReversePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    DoubleReversePolicy {
        fail_on_violation: options.fail_on_violation(),
        reverse_form_count: summary.reverse_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reverses(input: &str) -> (usize, Vec<DoubleReverseItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_double_reverses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect double reverses")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_double_reverse() {
        let source = "(reverse (reverse xs))";
        let (_, violations) = reverses(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].inner_span), "xs");
    }

    #[test]
    fn preserves_a_compound_inner_argument() {
        let source = "(reverse (reverse (mapcar #'f ys)))";
        let (_, violations) = reverses(source);
        assert_eq!(slice(source, violations[0].inner_span), "(mapcar #'f ys)");
    }

    #[test]
    fn does_not_flag_a_single_reverse() {
        let (count, violations) = reverses("(reverse xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_nreverse() {
        // Destructive nreverse cannot be reasoned about as a plain copy.
        assert!(reverses("(nreverse (nreverse xs))").1.is_empty());
        assert!(reverses("(reverse (nreverse xs))").1.is_empty());
        assert!(reverses("(nreverse (reverse xs))").1.is_empty());
    }

    #[test]
    fn does_not_flag_reverse_of_other_call() {
        let (_, violations) = reverses("(reverse (sort xs #'<))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = reverses("(REVERSE (REVERSE xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = reverses("(defun f (xs) (reverse (reverse xs)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(reverse (reverse xs))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_double_reverses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect double reverses");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = reverses("(reverse (reverse xs))");
        let summary = summarize_double_reverses(count, items);

        let quiet =
            evaluate_double_reverse_policy(DoubleReversePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_double_reverse_policy(DoubleReversePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
