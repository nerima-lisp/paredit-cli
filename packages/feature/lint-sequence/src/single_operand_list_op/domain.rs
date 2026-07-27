//! Common Lisp single-operand list-op detection: a one-argument call to
//! `append`, `nconc`, or `list*`. Each of these returns its *last* argument
//! unchanged, and with a single argument that argument is the last — so
//! `(append x)` is exactly `x`, `(nconc x)` is `x`, and `(list* x)` is `x`. The
//! single-argument call does nothing.
//!
//! Scope is limited to these three operators precisely because their
//! single-argument identity is *unconditional*: `(append x)` returns `x` for
//! any object (`(append 5)` is `5`), imposing no type constraint. Numeric n-ary
//! operators like `logand`/`max` are deliberately excluded — `(max x)` requires
//! `x` to be a real and would signal an error a bare `x` would not, so dropping
//! the wrapper there would change behavior. (`+`/`*` and `and`/`or` are covered
//! by `single-operand-arithmetic` and `single-operand-boolean`.)
//!
//! Only the exact two-element shape `(op x)` is flagged; a zero-argument
//! `(append)` (which is `nil`) and a reader-conditional operand are left alone.
//!
//! The fix replaces the whole form with the argument's source, so the rule is
//! auto-fixable.
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

/// The one-argument-identity n-ary list operators.
const LIST_OP_HEADS: [&str; 3] = ["append", "nconc", "list*"];

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SingleOperandListOpItem {
    pub path: PathBuf,
    /// The span of the whole `(op x)` form.
    pub span: ByteSpan,
    /// The span of the sole argument `x` (for reconstructing the fix).
    pub arg_span: ByteSpan,
    /// The operator name (`append`/`nconc`/`list*`), for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct SingleOperandListOpSummary {
    pub list_op_form_count: usize,
    pub violations: Vec<SingleOperandListOpItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SingleOperandListOpPolicyOptions {
    fail_on_violation: bool,
}

impl SingleOperandListOpPolicyOptions {
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
pub struct SingleOperandListOpPolicy {
    pub fail_on_violation: bool,
    pub list_op_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_form(
    view: &ExpressionView,
    path: &Path,
    list_op_form_count: &mut usize,
    violations: &mut Vec<SingleOperandListOpItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !LIST_OP_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *list_op_form_count += 1;

    // children: [op, arg] — require exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let arg = &view.children[1];
    if is_reader_conditional(arg) {
        return;
    }

    violations.push(SingleOperandListOpItem {
        path: path.to_path_buf(),
        span: view.span,
        arg_span: arg.span,
        head: head.to_owned(),
    });
}

/// Collects every single-argument `append`/`nconc`/`list*` across a whole file,
/// along with the total number of such forms scanned.
pub fn collect_single_operand_list_ops(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<SingleOperandListOpItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut list_op_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, path, &mut list_op_form_count, &mut violations);
        });
    }
    Ok((list_op_form_count, violations))
}

#[must_use]
pub const fn summarize_single_operand_list_ops(
    list_op_form_count: usize,
    violations: Vec<SingleOperandListOpItem>,
) -> SingleOperandListOpSummary {
    SingleOperandListOpSummary {
        list_op_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_single_operand_list_op_policy(
    options: SingleOperandListOpPolicyOptions,
    summary: &SingleOperandListOpSummary,
) -> SingleOperandListOpPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SingleOperandListOpPolicy {
        fail_on_violation: options.fail_on_violation(),
        list_op_form_count: summary.list_op_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(input: &str) -> (usize, Vec<SingleOperandListOpItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_single_operand_list_ops(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect single-operand list ops")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_single_arg_append() {
        let source = "(append xs)";
        let (count, violations) = forms(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "append");
        assert_eq!(slice(source, violations[0].arg_span), "xs");
    }

    #[test]
    fn flags_single_arg_nconc() {
        let (_, violations) = forms("(nconc items)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "nconc");
    }

    #[test]
    fn flags_single_arg_list_star() {
        let (_, violations) = forms("(list* tail)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "list*");
    }

    #[test]
    fn preserves_a_compound_argument() {
        let source = "(append (mapcar #'f xs))";
        let (_, violations) = forms(source);
        assert_eq!(slice(source, violations[0].arg_span), "(mapcar #'f xs)");
    }

    #[test]
    fn does_not_flag_two_arguments() {
        let (count, violations) = forms("(append a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_zero_arguments() {
        let (count, violations) = forms("(append)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_numeric_op() {
        // (max x) requires a real; not covered here.
        let (count, violations) = forms("(max x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = forms("(APPEND xs)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = forms("(defun f (xs) (nconc xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(append xs)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_single_operand_list_ops(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect single-operand list ops");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = forms("(append xs)");
        let summary = summarize_single_operand_list_ops(count, items);

        let quiet = evaluate_single_operand_list_op_policy(
            SingleOperandListOpPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_single_operand_list_op_policy(
            SingleOperandListOpPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
