//! Common Lisp constant-`if`-test detection: an `if` whose test is the literal
//! constant `t` or `nil`. The branch is then statically decided and the other is
//! dead code — `(if t A B)` always evaluates `A`, `(if nil A B)` always `B`, and
//! a one-armed `(if nil A)` never runs `A` at all (so it is just `nil`). Because
//! `t` and `nil` are constants that cannot be rebound, the collapse is exact.
//!
//! Only the literal `t`/`nil` symbol is treated as constant; a truthy value like
//! `5` or a variable test is a legitimate condition and is left alone, as is a
//! reader-conditional branch (build-dependent arity).
//!
//! The fix replaces the whole form with the live branch's exact source (or the
//! literal `nil` for a false one-armed `if`), so the rule is auto-fixable.
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

/// Whether `view` is the bare literal `t` or `nil` (no reader prefixes); returns
/// which one so the caller can pick the live branch.
fn constant_test(view: &ExpressionView) -> Option<bool> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    if text.eq_ignore_ascii_case("t") {
        Some(true)
    } else if text.eq_ignore_ascii_case("nil") {
        Some(false)
    } else {
        None
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled arity.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct ConstantIfTestItem {
    pub path: PathBuf,
    /// The span of the whole `(if TEST …)` form.
    pub span: ByteSpan,
    /// The literal test, lowercased (`t` or `nil`).
    pub test: &'static str,
    /// The span of the live branch to keep, or `None` when the result is the
    /// literal `nil` (a false one-armed `if`).
    pub result_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct ConstantIfTestSummary {
    pub if_form_count: usize,
    pub violations: Vec<ConstantIfTestItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ConstantIfTestPolicyOptions {
    fail_on_violation: bool,
}

impl ConstantIfTestPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct ConstantIfTestPolicy {
    pub fail_on_violation: bool,
    pub if_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_if(
    view: &ExpressionView,
    path: &Path,
    if_form_count: &mut usize,
    violations: &mut Vec<ConstantIfTestItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("if") {
        return;
    }
    *if_form_count += 1;

    // children: [if, test, then] or [if, test, then, else].
    if view.children.len() != 3 && view.children.len() != 4 {
        return;
    }
    let Some(is_true) = constant_test(&view.children[1]) else {
        return;
    };
    if view.children[2..].iter().any(is_reader_conditional) {
        return;
    }

    let result_span = if is_true {
        // t: the then branch always runs.
        Some(view.children[2].span)
    } else if view.children.len() == 4 {
        // nil with an else branch: the else always runs.
        Some(view.children[3].span)
    } else {
        // nil, no else: the form is just nil.
        None
    };

    violations.push(ConstantIfTestItem {
        path: path.to_path_buf(),
        span: view.span,
        test: if is_true { "t" } else { "nil" },
        result_span,
    });
}

/// Collects every constant-test `if` across a whole file, along with the total
/// number of `if` forms scanned.
pub fn collect_constant_if_tests(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ConstantIfTestItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut if_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_if(subview, path, &mut if_form_count, &mut violations)
        });
    }
    Ok((if_form_count, violations))
}

pub fn summarize_constant_if_tests(
    if_form_count: usize,
    violations: Vec<ConstantIfTestItem>,
) -> ConstantIfTestSummary {
    ConstantIfTestSummary {
        if_form_count,
        violations,
    }
}

pub fn evaluate_constant_if_test_policy(
    options: ConstantIfTestPolicyOptions,
    summary: &ConstantIfTestSummary,
) -> ConstantIfTestPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ConstantIfTestPolicy {
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

    fn ifs(input: &str) -> (usize, Vec<ConstantIfTestItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_constant_if_tests(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect constant if tests")
    }

    fn result<'a>(source: &'a str, item: &ConstantIfTestItem) -> &'a str {
        match item.result_span {
            Some(span) => &source[span.start().get()..span.end().get()],
            None => "nil",
        }
    }

    #[test]
    fn true_test_keeps_then_branch() {
        let source = "(if t a b)";
        let (count, violations) = ifs(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].test, "t");
        assert_eq!(result(source, &violations[0]), "a");
    }

    #[test]
    fn nil_test_keeps_else_branch() {
        let source = "(if nil a b)";
        let (_, violations) = ifs(source);
        assert_eq!(violations[0].test, "nil");
        assert_eq!(result(source, &violations[0]), "b");
    }

    #[test]
    fn nil_one_armed_is_nil() {
        let source = "(if nil (side-effect))";
        let (_, violations) = ifs(source);
        assert!(violations[0].result_span.is_none());
        assert_eq!(result(source, &violations[0]), "nil");
    }

    #[test]
    fn true_one_armed_keeps_then() {
        let source = "(if t (go))";
        let (_, violations) = ifs(source);
        assert_eq!(result(source, &violations[0]), "(go)");
    }

    #[test]
    fn preserves_compound_branch_source() {
        let source = "(if t (compute x y) other)";
        let (_, violations) = ifs(source);
        assert_eq!(result(source, &violations[0]), "(compute x y)");
    }

    #[test]
    fn does_not_flag_a_variable_test() {
        let (count, violations) = ifs("(if ready a b)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_truthy_non_t_literal() {
        let (_, violations) = ifs("(if 5 a b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_test() {
        let (_, violations) = ifs("(IF NIL a b)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].test, "nil");
    }

    #[test]
    fn finds_a_nested_constant_if() {
        let (_, violations) = ifs("(defun f () (if t 1 2))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(if t a b)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_constant_if_tests(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect constant if tests");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = ifs("(if t a b)");
        let summary = summarize_constant_if_tests(count, items);

        let quiet =
            evaluate_constant_if_test_policy(ConstantIfTestPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_constant_if_test_policy(ConstantIfTestPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
