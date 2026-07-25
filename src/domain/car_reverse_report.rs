//! Common Lisp `car`-of-`reverse` detection: a `(car (reverse x))` (or
//! `(first (reverse x))`). Taking the first element of the reversed sequence is
//! the *last* element of the original — but `reverse` builds a whole fresh copy
//! just to read one element. `(car (last x))` yields the same element (the last,
//! or `nil` when `x` is empty) without the O(n) allocation, so
//! `(car (reverse x))` is `(car (last x))`.
//!
//! Only the non-destructive `reverse` is matched. `nreverse` is excluded — it
//! mutates `x`, so `(car (nreverse x))` is not equivalent to the copy-free
//! `(car (last x))`. The outer accessor's `car`/`first` spelling is preserved. A
//! `reverse` with the wrong arity and a reader-conditional operand are left
//! alone.
//!
//! The fix rewrites `(car (reverse x))` as `(car (last x))` (keeping the outer
//! accessor), copying `x`'s source, so the rule is auto-fixable.
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

#[derive(Debug, Clone)]
pub struct CarReverseItem {
    pub path: PathBuf,
    /// The span of the whole `(car (reverse x))` form.
    pub span: ByteSpan,
    /// The span of the outer accessor token (`car`/`first`), preserved in the fix.
    pub accessor_span: ByteSpan,
    /// The span of the sequence operand `x`.
    pub list_span: ByteSpan,
}

#[derive(Debug)]
pub struct CarReverseSummary {
    pub accessor_form_count: usize,
    pub violations: Vec<CarReverseItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CarReversePolicyOptions {
    fail_on_violation: bool,
}

impl CarReversePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct CarReversePolicy {
    pub fail_on_violation: bool,
    pub accessor_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    accessor_form_count: &mut usize,
    violations: &mut Vec<CarReverseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("car") && !head.eq_ignore_ascii_case("first") {
        return;
    }
    *accessor_form_count += 1;

    // children: [car/first, inner] — the accessor takes exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let inner = &view.children[1];
    if !is_paren_list(inner) {
        return;
    }
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    if !inner_head.eq_ignore_ascii_case("reverse") {
        return;
    }
    // inner children: [reverse, list] — reverse takes exactly one argument.
    if inner.children.len() != 2 {
        return;
    }
    let list = &inner.children[1];
    if is_reader_conditional(list) {
        return;
    }

    violations.push(CarReverseItem {
        path: path.to_path_buf(),
        span: view.span,
        accessor_span: view.children[0].span,
        list_span: list.span,
    });
}

/// Collects every `(car (reverse x))`/`(first (reverse x))` across a whole file,
/// along with the total number of `car`/`first` forms scanned.
pub fn collect_car_reverses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<CarReverseItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut accessor_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut accessor_form_count, &mut violations)
        });
    }
    Ok((accessor_form_count, violations))
}

pub fn summarize_car_reverses(
    accessor_form_count: usize,
    violations: Vec<CarReverseItem>,
) -> CarReverseSummary {
    CarReverseSummary {
        accessor_form_count,
        violations,
    }
}

pub fn evaluate_car_reverse_policy(
    options: CarReversePolicyOptions,
    summary: &CarReverseSummary,
) -> CarReversePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CarReversePolicy {
        fail_on_violation: options.fail_on_violation(),
        accessor_form_count: summary.accessor_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accessors(input: &str) -> (usize, Vec<CarReverseItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_car_reverses(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect car reverses")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_car_reverse() {
        let source = "(car (reverse items))";
        let (count, violations) = accessors(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].accessor_span), "car");
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn flags_first_reverse_and_preserves_accessor() {
        let source = "(first (reverse xs))";
        let (_, violations) = accessors(source);
        assert_eq!(slice(source, violations[0].accessor_span), "first");
    }

    #[test]
    fn preserves_compound_list() {
        let source = "(car (reverse (mapcar #'f ys)))";
        let (_, violations) = accessors(source);
        assert_eq!(slice(source, violations[0].list_span), "(mapcar #'f ys)");
    }

    #[test]
    fn does_not_flag_nreverse() {
        // (car (nreverse x)) mutates x; not equivalent to the copy-free (car (last x)).
        assert!(accessors("(car (nreverse xs))").1.is_empty());
    }

    #[test]
    fn does_not_flag_plain_reverse() {
        assert!(accessors("(reverse xs)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_reverse_arity() {
        assert!(accessors("(car (reverse))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = accessors("(CAR (REVERSE xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = accessors("(defun f (xs) (car (reverse xs)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(car (reverse xs))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_car_reverses(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect car reverses");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = accessors("(car (reverse xs))");
        let summary = summarize_car_reverses(count, items);

        let quiet = evaluate_car_reverse_policy(CarReversePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_car_reverse_policy(CarReversePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
