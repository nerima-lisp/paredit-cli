//! Common Lisp nested-boolean detection: an `and`/`or` form that appears as a
//! direct operand of another `and`/`or` with the *same* operator. Because both
//! operators are associative and evaluate their operands left to right with the
//! same short-circuit rule, a nested same-operator form splices in with no
//! change: `(or a (or b c) d)` is exactly `(or a b c d)` and `(and a (and b c))`
//! is `(and a b c)`. The nesting is pure structure noise, common after
//! mechanical macro expansion or incremental edits.
//!
//! This rule is the multi-operand companion to
//! [`crate::single_operand_boolean::domain`]: that rule owns the
//! single-operand collapse (`(or x)` is `x`), while this rule owns a nested
//! same-operator form with two or more operands that is redundant *because* of
//! where it sits. Requiring two or more inner operands keeps the two rules from
//! flagging the same span.
//!
//! Splicing preserves evaluation order and short-circuiting regardless of the
//! inner operands' contents or any `#+`/`#-` reader conditional among them
//! (which attaches to the form it precedes either way), so no guard is needed
//! beyond skipping an inner form that itself carries a reader prefix
//! (`,(or …)`/`#'(or …)`), which is not a plain boolean subform.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};

/// The boolean operator (`and`/`or`) heading `view`, or `None` when `view` is
/// not such a form.
fn boolean_operator(view: &ExpressionView) -> Option<&'static str> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if head.eq_ignore_ascii_case("and") {
        Some("and")
    } else if head.eq_ignore_ascii_case("or") {
        Some("or")
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct NestedBooleanItem {
    pub path: PathBuf,
    /// The span of the inner (nested) `and`/`or` form.
    pub span: ByteSpan,
    /// The span of the inner form's interior (`op`'s operands, parens excluded),
    /// which a fix splices in place of the wrapper.
    pub inner_span: ByteSpan,
    /// The shared operator (`and`/`or`), for the finding message.
    pub operator: &'static str,
}

#[derive(Debug)]
pub struct NestedBooleanSummary {
    pub boolean_form_count: usize,
    pub violations: Vec<NestedBooleanItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NestedBooleanPolicyOptions {
    fail_on_violation: bool,
}

impl NestedBooleanPolicyOptions {
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
pub struct NestedBooleanPolicy {
    pub fail_on_violation: bool,
    pub boolean_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_boolean(
    view: &ExpressionView,
    path: &Path,
    boolean_form_count: &mut usize,
    violations: &mut Vec<NestedBooleanItem>,
) {
    let Some(op) = boolean_operator(view) else {
        return;
    };
    *boolean_form_count += 1;

    // children[0] is the operator; any operand that is a same-operator form with
    // two or more operands of its own splices redundantly into this one.
    for child in &view.children[1..] {
        if !child.reader_prefixes.is_empty() {
            continue;
        }
        if boolean_operator(child) != Some(op) {
            continue;
        }
        // Inner operands = children beyond the inner head; require two or more so
        // the single-operand-boolean rule keeps the `(or x)` collapse.
        if child.children.len() < 3 {
            continue;
        }
        // Splice just the operands (drop the inner operator): the interior span
        // runs from the first operand's start to the last operand's end.
        let operands = &child.children[1..];
        let inner_span = ByteSpan::new(
            operands[0].span.start(),
            operands[operands.len() - 1].span.end(),
        );
        violations.push(NestedBooleanItem {
            path: path.to_path_buf(),
            span: child.span,
            inner_span,
            operator: op,
        });
    }
}

/// Collects every `and`/`or` nested directly inside a same-operator `and`/`or`
/// across a whole file, along with the total number of `and`/`or` forms
/// scanned.
pub fn collect_nested_booleans(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<NestedBooleanItem>)> {
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
pub const fn summarize_nested_booleans(
    boolean_form_count: usize,
    violations: Vec<NestedBooleanItem>,
) -> NestedBooleanSummary {
    NestedBooleanSummary {
        boolean_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_nested_boolean_policy(
    options: NestedBooleanPolicyOptions,
    summary: &NestedBooleanSummary,
) -> NestedBooleanPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NestedBooleanPolicy {
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

    fn nested(input: &str) -> (usize, Vec<NestedBooleanItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nested_booleans(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nested booleans")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_or_nested_in_or() {
        let source = "(or a (or b c) d)";
        let (count, violations) = nested(source);
        // Two or forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
        assert_eq!(slice(source, violations[0].inner_span), "b c");
    }

    #[test]
    fn flags_and_nested_in_and() {
        let source = "(and a (and b c))";
        let (_, violations) = nested(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
        assert_eq!(slice(source, violations[0].inner_span), "b c");
    }

    #[test]
    fn does_not_flag_different_operator() {
        // (or a (and b c)) is not associative-flattenable.
        let (_, violations) = nested("(or a (and b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_single_operand_inner() {
        // (or x) is the single-operand-boolean rule's job.
        let (_, violations) = nested("(or a (or x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_empty_inner() {
        let (_, violations) = nested("(and a (and))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_head_position() {
        // The inner form must be an operand, never the operator slot.
        let (_, violations) = nested("(or (or a b))");
        assert_eq!(violations.len(), 1); // still an operand here, flagged
        let (_, none) = nested("(and)");
        assert!(none.is_empty());
    }

    #[test]
    fn case_folds_both_operators() {
        let (_, violations) = nested("(OR a (Or b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_multiple_nested_forms() {
        let (_, violations) = nested("(or (or a b) (or c d))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn flags_deeply_nested_forms() {
        // Inner-most nested in middle, middle nested in outer.
        let (_, violations) = nested("(or a (or b (or c d)))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn does_not_flag_a_prefixed_inner() {
        // ,(or b c) inside a backquote is not a plain boolean subform.
        let (_, violations) = nested("(or a `(or b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(or a (or b c))", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_nested_booleans(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nested booleans");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = nested("(or a (or b c))");
        let summary = summarize_nested_booleans(count, items);

        let quiet =
            evaluate_nested_boolean_policy(NestedBooleanPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_nested_boolean_policy(NestedBooleanPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
