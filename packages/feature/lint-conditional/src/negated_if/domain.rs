//! Common Lisp negated-`if` detection: a three-argument `if` whose test is a
//! `(not X)`/`(null X)` negation — `(if (not ready) a b)`. Negating the test
//! just flips which branch runs, so `(if (not X) A B)` is exactly `(if X B A)`:
//! the negation drops away and the two branches swap, evaluating `X` once. The
//! positive test reads more directly than "if not …, else …".
//!
//! Only the three-argument shape is flagged. A one-armed `(if (not X) A)` has no
//! else branch to swap into; that shape is the province of the `when`/`unless`
//! idiom (`(unless X A)`), not this rule. A reader-conditional branch is exempt
//! (build-dependent arity).
//!
//! The fix rewrites `(if (not X) A B)` as `(if X B A)`, copying `X`, `A`, and `B`
//! from their exact source, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled arity.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// The un-negated test `X` of a `(not X)`/`(null X)` form, or `None` when `view`
/// is not a single-argument `not`/`null`.
fn negated_test(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("not") && !head.eq_ignore_ascii_case("null") {
        return None;
    }
    // (not X) / (null X): exactly one argument.
    (view.children.len() == 2).then(|| &view.children[1])
}

#[derive(Debug, Clone)]
pub struct NegatedIfItem {
    pub path: PathBuf,
    /// The span of the whole `(if (not X) A B)` form.
    pub span: ByteSpan,
    /// The span of the un-negated test `X`.
    pub test_span: ByteSpan,
    /// The span of the then-branch `A`.
    pub then_span: ByteSpan,
    /// The span of the else-branch `B`.
    pub else_span: ByteSpan,
}

#[derive(Debug)]
pub struct NegatedIfSummary {
    pub if_form_count: usize,
    pub violations: Vec<NegatedIfItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NegatedIfPolicyOptions {
    fail_on_violation: bool,
}

impl NegatedIfPolicyOptions {
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
pub struct NegatedIfPolicy {
    pub fail_on_violation: bool,
    pub if_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_if(
    view: &ExpressionView,
    path: &Path,
    if_form_count: &mut usize,
    violations: &mut Vec<NegatedIfItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then, else] — the three-argument shape.
    if view.children.len() != 4 {
        return;
    }
    let Some(inner) = negated_test(&view.children[1]) else {
        return;
    };
    if is_reader_conditional(inner)
        || is_reader_conditional(&view.children[2])
        || is_reader_conditional(&view.children[3])
    {
        return;
    }

    violations.push(NegatedIfItem {
        path: path.to_path_buf(),
        span: view.span,
        test_span: inner.span,
        then_span: view.children[2].span,
        else_span: view.children[3].span,
    });
}

/// Collects every negated three-argument `if` across a whole file, along with
/// the total number of `if` forms scanned.
pub fn collect_negated_ifs(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NegatedIfItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, path, &mut if_form_count, &mut violations);
        });
    }
    Ok((if_form_count, violations))
}

#[must_use]
pub const fn summarize_negated_ifs(
    if_form_count: usize,
    violations: Vec<NegatedIfItem>,
) -> NegatedIfSummary {
    NegatedIfSummary {
        if_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_negated_if_policy(
    options: NegatedIfPolicyOptions,
    summary: &NegatedIfSummary,
) -> NegatedIfPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NegatedIfPolicy {
        fail_on_violation: options.fail_on_violation(),
        if_form_count: summary.if_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ifs(input: &str) -> (usize, Vec<NegatedIfItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_negated_ifs(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect negated ifs")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_not_test() {
        let source = "(if (not ready) a b)";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].test_span), "ready");
        assert_eq!(slice(source, violations[0].then_span), "a");
        assert_eq!(slice(source, violations[0].else_span), "b");
    }

    #[test]
    fn flags_null_test() {
        let (_, violations) = ifs("(if (null xs) 0 (length xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn preserves_compound_branch_source() {
        let source = "(if (not p) (do-a x) (do-b y))";
        let (_, violations) = ifs(source);
        assert_eq!(slice(source, violations[0].then_span), "(do-a x)");
        assert_eq!(slice(source, violations[0].else_span), "(do-b y)");
    }

    #[test]
    fn does_not_flag_one_armed_if() {
        // (if (not c) a) has no else to swap.
        let (_, violations) = ifs("(if (not c) a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_positive_test() {
        let (count, violations) = ifs("(if c a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_argument_not() {
        // (not a b) is malformed; leave it alone.
        let (_, violations) = ifs("(if (not a b) x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_negator() {
        let (_, violations) = ifs("(IF (NOT c) a b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_negated_if() {
        let (_, violations) = ifs("(defun f (c) (if (not c) 1 2))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(if (not c) a b)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_negated_ifs(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect negated ifs");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ifs("(if (not c) a b)");
        let summary = summarize_negated_ifs(count, items);

        let quiet = evaluate_negated_if_policy(NegatedIfPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_negated_if_policy(NegatedIfPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
