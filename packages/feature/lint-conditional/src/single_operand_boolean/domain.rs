//! Common Lisp single-operand-`and`/`or` detection: `(and X)` or `(or X)` with
//! exactly one operand. A one-argument `and` or `or` evaluates that operand and
//! returns its value verbatim — there is no short-circuit to perform and no
//! boolean coercion (`and`/`or` return the operand object, not `t`/`nil`), so
//! `(and X)` and `(or X)` are each just `X`. The wrapper is pure redundancy,
//! common after mechanical macro expansion or when operands are edited away.
//!
//! Only the single-operand shape is flagged. The zero-operand identities
//! (`(and)` is `t`, `(or)` is `nil`) are legitimate macro-expansion building
//! blocks and are left alone, and two-or-more-operand forms are meaningful. A
//! lone reader conditional (`#+`/`#-`) as the sole operand is exempt: it may
//! expand to zero or one operand depending on the build.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// The canonical operator name for an `and`/`or` head, or `None` otherwise.
fn boolean_operator(head: &str) -> Option<&'static str> {
    if head.eq_ignore_ascii_case("and") {
        Some("and")
    } else if head.eq_ignore_ascii_case("or") {
        Some("or")
    } else {
        None
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom operand does not represent one
/// evaluated operand. Mirrors the guard used by the other progn/boolean lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SingleOperandBooleanItem {
    pub path: PathBuf,
    /// The span of the whole `(and X)`/`(or X)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`and` or `or`).
    pub operator: &'static str,
    /// The span of the sole operand `X` (lets a fix substitute its source).
    pub inner_span: ByteSpan,
}

#[derive(Debug)]
pub struct SingleOperandBooleanSummary {
    pub boolean_form_count: usize,
    pub violations: Vec<SingleOperandBooleanItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SingleOperandBooleanPolicyOptions {
    fail_on_violation: bool,
}

impl SingleOperandBooleanPolicyOptions {
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
pub struct SingleOperandBooleanPolicy {
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
    violations: &mut Vec<SingleOperandBooleanItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = boolean_operator(head) else {
        return;
    };
    *boolean_form_count += 1;

    // children[0] is the operator; a single operand means exactly two children.
    if view.children.len() != 2 {
        return;
    }
    let operand = &view.children[1];
    if is_reader_conditional(operand) {
        return;
    }
    violations.push(SingleOperandBooleanItem {
        path: path.to_path_buf(),
        span: view.span,
        operator,
        inner_span: operand.span,
    });
}

/// Collects every single-operand `and`/`or` across a whole file, along with the
/// total number of `and`/`or` forms scanned.
pub fn collect_single_operand_booleans(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<SingleOperandBooleanItem>)> {
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
pub const fn summarize_single_operand_booleans(
    boolean_form_count: usize,
    violations: Vec<SingleOperandBooleanItem>,
) -> SingleOperandBooleanSummary {
    SingleOperandBooleanSummary {
        boolean_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_single_operand_boolean_policy(
    options: SingleOperandBooleanPolicyOptions,
    summary: &SingleOperandBooleanSummary,
) -> SingleOperandBooleanPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SingleOperandBooleanPolicy {
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

    fn booleans(input: &str) -> (usize, Vec<SingleOperandBooleanItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_single_operand_booleans(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect single-operand booleans")
    }

    #[test]
    fn flags_single_operand_and() {
        let (count, violations) = booleans("(and x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
    }

    #[test]
    fn flags_single_operand_or() {
        let (_, violations) = booleans("(or (compute))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
    }

    #[test]
    fn inner_span_covers_only_the_operand() {
        let (_, violations) = booleans("(and (foo bar))");
        let inner = violations[0].inner_span;
        assert!(inner.start().get() > violations[0].span.start().get());
        assert!(inner.end().get() < violations[0].span.end().get());
    }

    #[test]
    fn case_folds_the_operator() {
        let (_, violations) = booleans("(AND x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "and");
    }

    #[test]
    fn does_not_flag_two_operands() {
        let (count, violations) = booleans("(and x y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_empty_identity() {
        let (_, violations) = booleans("(and)");
        assert!(violations.is_empty());
        let (_, violations) = booleans("(or)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional() {
        let (_, violations) = booleans("(and #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = booleans("(not x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_single_operand_boolean() {
        // Outer (and ...) has two operands; the inner (or z) is single-operand.
        let (_, violations) = booleans("(and y (or z))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "or");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(and x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_single_operand_booleans(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect single-operand booleans");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = booleans("(and x)");
        let summary = summarize_single_operand_booleans(count, items);

        let quiet = evaluate_single_operand_boolean_policy(
            SingleOperandBooleanPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_single_operand_boolean_policy(
            SingleOperandBooleanPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
