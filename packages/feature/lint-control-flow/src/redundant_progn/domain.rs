//! Common Lisp redundant-`progn` detection: a `(progn …)` whose body is a
//! single form (`(progn X)` ≡ `X`) or empty (`(progn)` ≡ `nil`). `progn`
//! establishes no binding or control context — it only evaluates its body forms
//! in order and returns the last value — so a one-form or zero-form progn is
//! pure wrapping noise, common in macro-expanded or machine-generated code.
//!
//! Only these two unambiguous shapes are flagged. A progn with two or more body
//! forms is meaningful (it sequences side effects) and is never flagged. A
//! reader conditional (`#+`/`#-`) as the sole body element is left alone: it may
//! expand to zero or one form depending on the build, so the static single-form
//! count does not reflect its evaluated arity.
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

#[derive(Debug, Clone)]
pub struct RedundantPrognItem {
    pub path: PathBuf,
    /// The span of the whole `(progn …)` form (what a fix would replace).
    pub span: ByteSpan,
    /// The number of body forms (0 for `(progn)`, 1 for `(progn X)`).
    pub body_form_count: usize,
    /// The span of the single body form, or `None` for an empty progn (whose
    /// meaning is `nil`). Lets a fix substitute the exact inner source text.
    pub inner_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct RedundantPrognSummary {
    pub progn_form_count: usize,
    pub violations: Vec<RedundantPrognItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantPrognPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantPrognPolicyOptions {
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
pub struct RedundantPrognPolicy {
    pub fail_on_violation: bool,
    pub progn_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom in a progn body does not
/// represent one evaluated form. Mirrors the guard used by the arity lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_progn(
    view: &ExpressionView,
    path: &Path,
    progn_form_count: &mut usize,
    violations: &mut Vec<RedundantPrognItem>,
) {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("progn"))
    {
        return;
    }
    *progn_form_count += 1;

    // children[0] is the `progn` head; the remainder is the body.
    let body = &view.children[1..];
    let body_form_count = body.len();

    match body {
        // (progn) — an empty body evaluates to nil.
        [] => violations.push(RedundantPrognItem {
            path: path.to_path_buf(),
            span: view.span,
            body_form_count,
            inner_span: None,
        }),
        // (progn X) — a single body form; the progn is a no-op wrapper. A lone
        // reader conditional is exempt (its evaluated arity is build-dependent).
        [only] if !is_reader_conditional(only) => violations.push(RedundantPrognItem {
            path: path.to_path_buf(),
            span: view.span,
            body_form_count,
            inner_span: Some(only.span),
        }),
        _ => {}
    }
}

/// Collects every redundant progn (empty, or wrapping a single form) across a
/// whole file, along with the total number of progn forms scanned.
pub fn collect_redundant_progns(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantPrognItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut progn_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_progn(subview, path, &mut progn_form_count, &mut violations);
        });
    }
    Ok((progn_form_count, violations))
}

#[must_use]
pub const fn summarize_redundant_progns(
    progn_form_count: usize,
    violations: Vec<RedundantPrognItem>,
) -> RedundantPrognSummary {
    RedundantPrognSummary {
        progn_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_progn_policy(
    options: RedundantPrognPolicyOptions,
    summary: &RedundantPrognSummary,
) -> RedundantPrognPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantPrognPolicy {
        fail_on_violation: options.fail_on_violation(),
        progn_form_count: summary.progn_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progns(input: &str) -> (usize, Vec<RedundantPrognItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_progns(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant progns")
    }

    #[test]
    fn flags_a_progn_wrapping_a_single_form() {
        let (count, violations) = progns("(progn (foo))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 1);
        assert!(violations[0].inner_span.is_some());
    }

    #[test]
    fn flags_a_progn_wrapping_a_single_atom() {
        let (_, violations) = progns("(progn x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 1);
    }

    #[test]
    fn flags_an_empty_progn() {
        let (_, violations) = progns("(progn)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 0);
        assert!(violations[0].inner_span.is_none());
    }

    #[test]
    fn inner_span_covers_only_the_body_form() {
        let (_, violations) = progns("(progn (foo bar))");
        let inner = violations[0].inner_span.expect("inner span");
        // The recorded inner span isolates `(foo bar)`, not the whole progn.
        assert!(inner.start().get() > violations[0].span.start().get());
        assert!(inner.end().get() < violations[0].span.end().get());
    }

    #[test]
    fn does_not_flag_a_progn_with_two_forms() {
        let (count, violations) = progns("(progn (setup) (run))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_with_many_forms() {
        let (_, violations) = progns("(progn a b c d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = progns("(PROGN x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_redundant_progn() {
        // Both the outer (single body form) and inner progn are redundant.
        let (_, violations) = progns("(progn (progn x))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional_body() {
        // (progn #+sbcl x) statically has one body form, but the reader
        // conditional may vanish at read time, so the arity is build-dependent.
        let (_, violations) = progns("(progn #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_when_a_reader_conditional_precedes_a_real_form() {
        // With two body elements this is not the single-form shape at all.
        let (_, violations) = progns("(progn #+sbcl x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_prog1_or_prog2() {
        let (count, violations) = progns("(prog1 x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(progn x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_progns(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant progns");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = progns("(progn x)");
        let summary = summarize_redundant_progns(count, items);

        let quiet =
            evaluate_redundant_progn_policy(RedundantPrognPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_progn_policy(RedundantPrognPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
