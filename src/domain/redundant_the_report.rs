//! Common Lisp redundant-`the` detection: a `(the t form)` type declaration.
//! `the` asserts that `form` yields values of the given type; the type `t`
//! matches every object, so `(the t form)` asserts nothing and simply returns
//! `form`'s values unchanged (`the` passes all values through, so this holds in
//! multiple-value contexts too). `(the t x)` is exactly `x`.
//!
//! Only the `t` value-type is flagged — a specific type like `(the fixnum x)`
//! is a meaningful assertion and is left alone. Only the exact
//! `(the t form)` three-element shape is matched; a reader-conditional operand
//! is left alone (build-dependent).
//!
//! The fix replaces the whole form with the inner form's source, so the rule is
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

/// Whether `view` is the bare `t` type specifier (no reader prefixes).
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
pub struct RedundantTheItem {
    pub path: PathBuf,
    /// The span of the whole `(the t form)` form.
    pub span: ByteSpan,
    /// The span of the inner form (for reconstructing the fix).
    pub form_span: ByteSpan,
}

#[derive(Debug)]
pub struct RedundantTheSummary {
    pub the_form_count: usize,
    pub violations: Vec<RedundantTheItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantThePolicyOptions {
    fail_on_violation: bool,
}

impl RedundantThePolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantThePolicy {
    pub fail_on_violation: bool,
    pub the_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_the(
    view: &ExpressionView,
    path: &Path,
    the_form_count: &mut usize,
    violations: &mut Vec<RedundantTheItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("the") {
        return;
    }
    *the_form_count += 1;

    // children: [the, value-type, form] — require exactly this shape.
    if view.children.len() != 3 {
        return;
    }
    let value_type = &view.children[1];
    let form = &view.children[2];
    if is_reader_conditional(value_type) || is_reader_conditional(form) {
        return;
    }
    if !is_t_type(value_type) {
        return;
    }

    violations.push(RedundantTheItem {
        path: path.to_path_buf(),
        span: view.span,
        form_span: form.span,
    });
}

/// Collects every `(the t form)` across a whole file, along with the total
/// number of `the` forms scanned.
pub fn collect_redundant_thes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantTheItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut the_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_the(subview, path, &mut the_form_count, &mut violations)
        });
    }
    Ok((the_form_count, violations))
}

pub fn summarize_redundant_thes(
    the_form_count: usize,
    violations: Vec<RedundantTheItem>,
) -> RedundantTheSummary {
    RedundantTheSummary {
        the_form_count,
        violations,
    }
}

pub fn evaluate_redundant_the_policy(
    options: RedundantThePolicyOptions,
    summary: &RedundantTheSummary,
) -> RedundantThePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantThePolicy {
        fail_on_violation: options.fail_on_violation(),
        the_form_count: summary.the_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thes(input: &str) -> (usize, Vec<RedundantTheItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_thes(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant the")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_the_t() {
        let source = "(the t (compute))";
        let (count, violations) = thes(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].form_span), "(compute)");
    }

    #[test]
    fn flags_the_t_on_a_symbol() {
        let source = "(the t x)";
        let (_, violations) = thes(source);
        assert_eq!(slice(source, violations[0].form_span), "x");
    }

    #[test]
    fn does_not_flag_a_specific_type() {
        let (count, violations) = thes("(the fixnum x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_compound_type() {
        let (_, violations) = thes("(the (values integer) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_type() {
        let (_, violations) = thes("(THE T x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_wrong_arity() {
        // (the t) and (the t a b) are the-arity's concern, not this rule.
        assert!(thes("(the t)").1.is_empty());
        assert!(thes("(the t a b)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_the() {
        let (_, violations) = thes("(defun f (x) (the t (g x)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(the t x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_thes(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant the");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = thes("(the t x)");
        let summary = summarize_redundant_thes(count, items);

        let quiet = evaluate_redundant_the_policy(RedundantThePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_the_policy(RedundantThePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
