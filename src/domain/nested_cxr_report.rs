//! Common Lisp nested-`cXr` detection: a `car`/`cdr`-family accessor applied
//! directly to another `car`/`cdr`-family accessor, which the standard combines
//! into a single accessor — `(car (cdr x))` is `(cadr x)`, `(cdr (cdr x))` is
//! `(cddr x)`, `(car (cddr x))` is `(caddr x)`. The composite `cXr` names are
//! *defined* as exactly these nestings, so the collapse is an exact rewrite that
//! reads the argument once, and the combined form is the idiomatic spelling.
//!
//! A `cXr` accessor is `c` followed by one to four `a`/`d` letters and a final
//! `r` (`car`, `cdr`, `caar`, …, `cddddr`). A nesting is flagged only when the
//! outer accessor has exactly one argument (the inner accessor form), the inner
//! accessor has exactly one argument, and the two middles concatenated are still
//! at most four letters — i.e. the combined accessor is itself a standard `cXr`.
//! Deeper nestings collapse one layer per pass under `--fix`'s fixpoint, so
//! `(car (cdr (cdr x)))` converges to `(caddr x)`.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, list_head};

/// The `a`/`d` middle of a `cXr` accessor symbol (`cadr` → `ad`), or `None` when
/// `symbol` is not a one-to-four-letter `c…r` accessor. Case-insensitive.
fn cxr_middle(symbol: &str) -> Option<String> {
    let lower = symbol.to_ascii_lowercase();
    let middle = lower.strip_prefix('c')?.strip_suffix('r')?;
    if middle.is_empty() || middle.len() > 4 {
        return None;
    }
    middle
        .bytes()
        .all(|byte| byte == b'a' || byte == b'd')
        .then(|| middle.to_owned())
}

#[derive(Debug, Clone)]
pub struct NestedCxrItem {
    pub path: PathBuf,
    /// The span of the whole `(OUTER (INNER x))` form.
    pub span: ByteSpan,
    /// The combined accessor name (`cadr` for `(car (cdr x))`).
    pub combined: String,
    /// The span of the innermost argument `x` (for reconstructing the fix).
    pub arg_span: ByteSpan,
}

#[derive(Debug)]
pub struct NestedCxrSummary {
    pub accessor_form_count: usize,
    pub violations: Vec<NestedCxrItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NestedCxrPolicyOptions {
    fail_on_violation: bool,
}

impl NestedCxrPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NestedCxrPolicy {
    pub fail_on_violation: bool,
    pub accessor_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_accessor(
    view: &ExpressionView,
    path: &Path,
    accessor_form_count: &mut usize,
    violations: &mut Vec<NestedCxrItem>,
) {
    let Some(outer_head) = list_head(view) else {
        return;
    };
    let Some(outer_middle) = cxr_middle(outer_head) else {
        return;
    };
    *accessor_form_count += 1;

    // The outer accessor must take exactly one argument (the inner form).
    if view.children.len() != 2 {
        return;
    }
    let inner = &view.children[1];
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    let Some(inner_middle) = cxr_middle(inner_head) else {
        return;
    };
    // The inner accessor must take exactly one argument.
    if inner.children.len() != 2 {
        return;
    }

    // The combined accessor must itself be a standard cXr (<= 4 middle letters).
    if outer_middle.len() + inner_middle.len() > 4 {
        return;
    }
    let combined = format!("c{outer_middle}{inner_middle}r");

    violations.push(NestedCxrItem {
        path: path.to_path_buf(),
        span: view.span,
        combined,
        arg_span: inner.children[1].span,
    });
}

/// Collects every combinable nested `cXr` accessor across a whole file, along
/// with the total number of `cXr` accessor forms scanned.
pub fn collect_nested_cxrs(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NestedCxrItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut accessor_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_accessor(subview, path, &mut accessor_form_count, &mut violations)
        });
    }
    Ok((accessor_form_count, violations))
}

pub fn summarize_nested_cxrs(
    accessor_form_count: usize,
    violations: Vec<NestedCxrItem>,
) -> NestedCxrSummary {
    NestedCxrSummary {
        accessor_form_count,
        violations,
    }
}

pub fn evaluate_nested_cxr_policy(
    options: NestedCxrPolicyOptions,
    summary: &NestedCxrSummary,
) -> NestedCxrPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NestedCxrPolicy {
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

    fn cxrs(input: &str) -> (usize, Vec<NestedCxrItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nested_cxrs(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nested cxrs")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn combines_car_of_cdr_into_cadr() {
        let source = "(car (cdr x))";
        let (count, violations) = cxrs(source);
        // Both the outer `car` and the inner `cdr` are cXr forms scanned.
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
        assert_eq!(slice(source, violations[0].arg_span), "x");
    }

    #[test]
    fn combines_cdr_of_cdr_into_cddr() {
        let (_, violations) = cxrs("(cdr (cdr items))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cddr");
    }

    #[test]
    fn combines_multi_letter_accessors() {
        // (car (cddr x)) -> caddr
        let (_, violations) = cxrs("(car (cddr x))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "caddr");
    }

    #[test]
    fn preserves_a_compound_argument() {
        let source = "(car (cdr (lookup k)))";
        let (_, violations) = cxrs(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
        assert_eq!(slice(source, violations[0].arg_span), "(lookup k)");
    }

    #[test]
    fn does_not_flag_when_combined_exceeds_four_letters() {
        // caddr of caddr would be caddaddr (6) — not a standard accessor.
        let (_, violations) = cxrs("(caddr (caddr x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_accessor() {
        let (count, violations) = cxrs("(car x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_car_of_a_non_accessor() {
        let (_, violations) = cxrs("(car (reverse x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_first_or_rest() {
        // first/rest are not cXr spellings.
        let (count, violations) = cxrs("(first (rest x))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_argument_accessor() {
        // A malformed 2-arg car is left to accessor/arity rules.
        let (_, violations) = cxrs("(car (cdr x) y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_accessors() {
        let (_, violations) = cxrs("(CAR (CDR x))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
    }

    #[test]
    fn finds_a_nested_accessor_inside_a_form() {
        let (_, violations) = cxrs("(list (car (cdr pair)))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].combined, "cadr");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(car (cdr x))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_nested_cxrs(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nested cxrs");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = cxrs("(car (cdr x))");
        let summary = summarize_nested_cxrs(count, items);

        let quiet = evaluate_nested_cxr_policy(NestedCxrPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_nested_cxr_policy(NestedCxrPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
