//! Common Lisp De Morgan detection: an `and`/`or` all of whose operands are
//! negations, which collapses to a single outer negation —
//! `(and (not a) (not b))` is `(not (or a b))`, `(or (not a) (not b))` is
//! `(not (and a b))`. This trades N inner `not`s for one outer `not` and often
//! reads more directly ("none of these" / "not all of these").
//!
//! The rewrite is exact down to short-circuit behavior: `(and (not a) (not b))`
//! skips evaluating `b` exactly when `a` is true, and `(not (or a b))` skips `b`
//! exactly when `a` is truthy — the same operand is elided in the same case, and
//! both forms yield a canonical `t`/`nil`.
//!
//! Every operand must be a single-argument `not`/`null` form, and there must be
//! at least two of them (a one-operand `and`/`or` is `single-operand-boolean`'s
//! job). If any operand is not a negation, the collapse does not apply and the
//! form is left alone; a reader-conditional operand is also exempt.
//!
//! The fix rewrites the whole form as `(not (OPPOSITE inner…))`, copying each
//! negation's inner operand from source, so the rule is auto-fixable.
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

/// The opposite boolean operator (`and`↔`or`) for a boolean head, or `None`
/// otherwise.
fn opposite_operator(head: &str) -> Option<(&'static str, &'static str)> {
    if head.eq_ignore_ascii_case("and") {
        Some(("and", "or"))
    } else if head.eq_ignore_ascii_case("or") {
        Some(("or", "and"))
    } else {
        None
    }
}

/// The inner operand `X` of a single-argument `(not X)`/`(null X)`, or `None`.
fn negation_inner(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("not") && !head.eq_ignore_ascii_case("null") {
        return None;
    }
    (view.children.len() == 2).then(|| &view.children[1])
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct DeMorganItem {
    pub path: PathBuf,
    /// The span of the whole `(and …)`/`(or …)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`and` or `or`).
    pub operator: &'static str,
    /// The opposite operator to place inside the outer `not` (`or` for `and`).
    pub opposite: &'static str,
    /// The spans of each negation's inner operand, in order.
    pub inner_spans: Vec<ByteSpan>,
}

#[derive(Debug)]
pub struct DeMorganSummary {
    pub boolean_form_count: usize,
    pub violations: Vec<DeMorganItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DeMorganPolicyOptions {
    fail_on_violation: bool,
}

impl DeMorganPolicyOptions {
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
pub struct DeMorganPolicy {
    pub fail_on_violation: bool,
    pub boolean_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_boolean(
    view: &ExpressionView,
    path: &Path,
    boolean_form_count: &mut usize,
    violations: &mut Vec<DeMorganItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((operator, opposite)) = opposite_operator(head) else {
        return;
    };
    *boolean_form_count += 1;

    let operands = &view.children[1..];
    // Need at least two operands, all negations, none build-dependent.
    if operands.len() < 2 {
        return;
    }
    if operands.iter().any(is_reader_conditional) {
        return;
    }

    let mut inner_spans = Vec::with_capacity(operands.len());
    for operand in operands {
        match negation_inner(operand) {
            Some(inner) => inner_spans.push(inner.span),
            None => return, // some operand is not a negation: no clean collapse.
        }
    }

    violations.push(DeMorganItem {
        path: path.to_path_buf(),
        span: view.span,
        operator,
        opposite,
        inner_spans,
    });
}

/// Collects every De Morgan-collapsible boolean form across a whole file, along
/// with the total number of `and`/`or` forms scanned.
pub fn collect_de_morgans(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DeMorganItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut boolean_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, path, &mut boolean_form_count, &mut violations);
        });
    }
    Ok((boolean_form_count, violations))
}

#[must_use]
pub const fn summarize_de_morgans(
    boolean_form_count: usize,
    violations: Vec<DeMorganItem>,
) -> DeMorganSummary {
    DeMorganSummary {
        boolean_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_de_morgan_policy(
    options: DeMorganPolicyOptions,
    summary: &DeMorganSummary,
) -> DeMorganPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    DeMorganPolicy {
        fail_on_violation: options.fail_on_violation(),
        boolean_form_count: summary.boolean_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn booleans(input: &str) -> (usize, Vec<DeMorganItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_de_morgans(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect de morgans")
    }

    fn inners<'a>(source: &'a str, item: &DeMorganItem) -> Vec<&'a str> {
        item.inner_spans
            .iter()
            .map(|s| &source[s.start().get()..s.end().get()])
            .collect()
    }

    #[test]
    fn and_of_nots_becomes_not_of_or() {
        let source = "(and (not a) (not b))";
        let (count, violations) = booleans(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
        assert_eq!(violations[0].opposite, "or");
        assert_eq!(inners(source, &violations[0]), vec!["a", "b"]);
    }

    #[test]
    fn or_of_nots_becomes_not_of_and() {
        let (_, violations) = booleans("(or (not a) (not b))");
        assert_eq!(violations[0].opposite, "and");
    }

    #[test]
    fn accepts_null_as_negation() {
        let (_, violations) = booleans("(and (null a) (not b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn preserves_compound_inner_source() {
        let source = "(and (not (foo x)) (not (bar y)))";
        let (_, violations) = booleans(source);
        assert_eq!(inners(source, &violations[0]), vec!["(foo x)", "(bar y)"]);
    }

    #[test]
    fn handles_three_operands() {
        let (_, violations) = booleans("(and (not a) (not b) (not c))");
        assert_eq!(violations[0].inner_spans.len(), 3);
    }

    #[test]
    fn does_not_flag_when_some_operand_is_not_a_negation() {
        let (_, violations) = booleans("(and (not a) b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_single_negation() {
        // (and (not a)) is single-operand-boolean's job.
        let (_, violations) = booleans("(and (not a))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_argument_not() {
        let (_, violations) = booleans("(and (not a b) (not c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_operators() {
        let (_, violations) = booleans("(OR (NOT a) (NULL b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].opposite, "and");
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = booleans("(when (and (not a) (not b)) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(and (not a) (not b))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_de_morgans(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect de morgans");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = booleans("(and (not a) (not b))");
        let summary = summarize_de_morgans(count, items);

        let quiet = evaluate_de_morgan_policy(DeMorganPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_de_morgan_policy(DeMorganPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
