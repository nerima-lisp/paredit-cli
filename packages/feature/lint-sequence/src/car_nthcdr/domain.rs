//! Common Lisp `car`-of-`nthcdr` detection: a `(car (nthcdr n x))`. The
//! standard *defines* `nth` as the `car` of the `nthcdr`, so `(car (nthcdr n x))`
//! is exactly `(nth n x)` — same element, same nil-on-overrun, `n` and `x` each
//! evaluated once in the same order. The single `nth` accessor reads more
//! directly than the nested pair.
//!
//! Only the exact `(car (nthcdr n x))` two-level shape is matched: `car` with one
//! argument, whose argument is `(nthcdr n x)` with exactly two operands. `first`
//! is not matched here (the `car`/`first` spelling of the outer accessor is a
//! separate taste question); a `cdr`/other outer accessor, a wrong `nthcdr`
//! arity, and a reader-conditional operand are all left alone.
//!
//! The fix rewrites `(car (nthcdr n x))` as `(nth n x)`, copying the count and
//! list operands verbatim, so the rule is auto-fixable.
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
pub struct CarNthcdrItem {
    pub path: PathBuf,
    /// The span of the whole `(car (nthcdr n x))` form.
    pub span: ByteSpan,
    /// The span of the count operand `n`.
    pub count_span: ByteSpan,
    /// The span of the list operand `x`.
    pub list_span: ByteSpan,
}

#[derive(Debug)]
pub struct CarNthcdrSummary {
    pub car_form_count: usize,
    pub violations: Vec<CarNthcdrItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct CarNthcdrPolicyOptions {
    fail_on_violation: bool,
}

impl CarNthcdrPolicyOptions {
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
pub struct CarNthcdrPolicy {
    pub fail_on_violation: bool,
    pub car_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    car_form_count: &mut usize,
    violations: &mut Vec<CarNthcdrItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("car") {
        return;
    }
    *car_form_count += 1;

    // children: [car, inner] — car takes exactly one argument.
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
    if !inner_head.eq_ignore_ascii_case("nthcdr") {
        return;
    }
    // inner children: [nthcdr, count, list].
    if inner.children.len() != 3 {
        return;
    }
    let count = &inner.children[1];
    let list = &inner.children[2];
    if is_reader_conditional(count) || is_reader_conditional(list) {
        return;
    }

    violations.push(CarNthcdrItem {
        path: path.to_path_buf(),
        span: view.span,
        count_span: count.span,
        list_span: list.span,
    });
}

/// Collects every `(car (nthcdr n x))` across a whole file, along with the total
/// number of `car` forms scanned.
pub fn collect_car_nthcdrs(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<CarNthcdrItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut car_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut car_form_count, &mut violations);
        });
    }
    Ok((car_form_count, violations))
}

#[must_use]
pub const fn summarize_car_nthcdrs(
    car_form_count: usize,
    violations: Vec<CarNthcdrItem>,
) -> CarNthcdrSummary {
    CarNthcdrSummary {
        car_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_car_nthcdr_policy(
    options: CarNthcdrPolicyOptions,
    summary: &CarNthcdrSummary,
) -> CarNthcdrPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    CarNthcdrPolicy {
        fail_on_violation: options.fail_on_violation(),
        car_form_count: summary.car_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cars(input: &str) -> (usize, Vec<CarNthcdrItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_car_nthcdrs(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect car nthcdrs")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_car_nthcdr() {
        let source = "(car (nthcdr n items))";
        let (count, violations) = cars(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].count_span), "n");
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn preserves_compound_operands() {
        let source = "(car (nthcdr (+ i 1) (rest xs)))";
        let (_, violations) = cars(source);
        assert_eq!(slice(source, violations[0].count_span), "(+ i 1)");
        assert_eq!(slice(source, violations[0].list_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_plain_nthcdr() {
        let (_, violations) = cars("(nthcdr n x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_cdr_outer() {
        let (_, violations) = cars("(cdr (nthcdr n x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_nthcdr_arity() {
        assert!(cars("(car (nthcdr n))").1.is_empty());
        assert!(cars("(car (nthcdr n x y))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = cars("(CAR (NTHCDR n x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = cars("(defun f (n x) (car (nthcdr n x)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(car (nthcdr n x))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_car_nthcdrs(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect car nthcdrs");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = cars("(car (nthcdr n x))");
        let summary = summarize_car_nthcdrs(count, items);

        let quiet = evaluate_car_nthcdr_policy(CarNthcdrPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_car_nthcdr_policy(CarNthcdrPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
