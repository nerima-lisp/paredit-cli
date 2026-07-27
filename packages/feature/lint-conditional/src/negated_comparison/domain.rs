//! Common Lisp negated-comparison detection: a `not`/`null` wrapping a
//! two-argument numeric comparison, which has an exact complementary operator —
//! `(not (= a b))` is `(/= a b)`, `(not (< a b))` is `(>= a b)`, `(null (> a b))`
//! is `(<= a b)`. Over Common Lisp's total order on reals every comparison has a
//! complement, so the negation collapses to the opposite operator, evaluating
//! each operand exactly once and stating the intent directly.
//!
//! Complement table (for exactly two operands):
//!
//!   - `=`  ↔ `/=`
//!   - `<`  ↔ `>=`
//!   - `>`  ↔ `<=`
//!
//! Only the two-operand shape is flagged. `(not (= a b c))` is *not* `(/= a b c)`
//! — a three-argument `=` tests all-equal while `/=` tests pairwise-distinct — so
//! a comparison with any operand count other than two is left alone, as is a
//! reader-conditional operand (build-dependent arity).
//!
//! The fix rewrites the whole `(not (OP a b))` form as `(COMPLEMENT a b)`,
//! copying the operands' source verbatim, so the rule is auto-fixable.
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

/// The complementary operator for a two-argument numeric comparison, or `None`
/// for a non-comparison head.
fn complement_operator(op: &str) -> Option<&'static str> {
    match op {
        "=" => Some("/="),
        "/=" => Some("="),
        "<" => Some(">="),
        ">" => Some("<="),
        "<=" => Some(">"),
        ">=" => Some("<"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// comparison containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NegatedComparisonItem {
    pub path: PathBuf,
    /// The span of the whole `(not (OP a b))` form.
    pub span: ByteSpan,
    /// The complementary operator to substitute (`/=`, `>=`, …).
    pub complement: &'static str,
    /// The span covering the two operands (`a b`), reused verbatim in the fix.
    pub operands_span: ByteSpan,
}

#[derive(Debug)]
pub struct NegatedComparisonSummary {
    pub negation_form_count: usize,
    pub violations: Vec<NegatedComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NegatedComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl NegatedComparisonPolicyOptions {
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
pub struct NegatedComparisonPolicy {
    pub fail_on_violation: bool,
    pub negation_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_negation(
    view: &ExpressionView,
    path: &Path,
    negation_form_count: &mut usize,
    violations: &mut Vec<NegatedComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("not") && !head.eq_ignore_ascii_case("null") {
        return;
    }
    // A negation is (not X) / (null X): exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    *negation_form_count += 1;

    let inner = &view.children[1];
    if !is_paren_list(inner) {
        return;
    }
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    let Some(complement) = complement_operator(inner_head) else {
        return;
    };
    // The comparison must have exactly two operands for the complement to hold.
    if inner.children.len() != 3 {
        return;
    }
    if is_reader_conditional(&inner.children[1]) || is_reader_conditional(&inner.children[2]) {
        return;
    }

    let operands_span = ByteSpan::new(inner.children[1].span.start(), inner.children[2].span.end());
    violations.push(NegatedComparisonItem {
        path: path.to_path_buf(),
        span: view.span,
        complement,
        operands_span,
    });
}

/// Collects every negated two-argument comparison across a whole file, along
/// with the total number of `not`/`null` forms scanned.
pub fn collect_negated_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NegatedComparisonItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut negation_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_negation(subview, path, &mut negation_form_count, &mut violations);
        });
    }
    Ok((negation_form_count, violations))
}

#[must_use]
pub const fn summarize_negated_comparisons(
    negation_form_count: usize,
    violations: Vec<NegatedComparisonItem>,
) -> NegatedComparisonSummary {
    NegatedComparisonSummary {
        negation_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_negated_comparison_policy(
    options: NegatedComparisonPolicyOptions,
    summary: &NegatedComparisonSummary,
) -> NegatedComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NegatedComparisonPolicy {
        fail_on_violation: options.fail_on_violation(),
        negation_form_count: summary.negation_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn negations(input: &str) -> (usize, Vec<NegatedComparisonItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_negated_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect negated comparisons")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn not_equal_becomes_slash_equal() {
        let source = "(not (= a b))";
        let (count, violations) = negations(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].complement, "/=");
        assert_eq!(slice(source, violations[0].operands_span), "a b");
    }

    #[test]
    fn not_less_becomes_ge() {
        let (_, violations) = negations("(not (< x y))");
        assert_eq!(violations[0].complement, ">=");
    }

    #[test]
    fn not_greater_becomes_le() {
        let (_, violations) = negations("(not (> x y))");
        assert_eq!(violations[0].complement, "<=");
    }

    #[test]
    fn not_le_becomes_greater() {
        let (_, violations) = negations("(not (<= x y))");
        assert_eq!(violations[0].complement, ">");
    }

    #[test]
    fn not_ne_becomes_equal() {
        let (_, violations) = negations("(not (/= x y))");
        assert_eq!(violations[0].complement, "=");
    }

    #[test]
    fn null_head_is_also_a_negation() {
        let (_, violations) = negations("(null (>= p q))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].complement, "<");
    }

    #[test]
    fn preserves_compound_operand_source() {
        let source = "(not (= (length xs) 0))";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operands_span), "(length xs) 0");
    }

    #[test]
    fn does_not_flag_three_operand_comparison() {
        // (not (= a b c)) is not (/= a b c).
        let (_, violations) = negations("(not (= a b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_comparison_inner() {
        let (_, violations) = negations("(not (evenp x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_not_of_symbol() {
        let (count, violations) = negations("(not flag)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_negated_comparison() {
        let (_, violations) = negations("(when (not (= a b)) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].complement, "/=");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(not (= a b))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_negated_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect negated comparisons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = negations("(not (= a b))");
        let summary = summarize_negated_comparisons(count, items);

        let quiet = evaluate_negated_comparison_policy(
            NegatedComparisonPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_negated_comparison_policy(NegatedComparisonPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
