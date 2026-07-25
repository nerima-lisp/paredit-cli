//! Common Lisp string-case-fold detection: a `string=` comparing two operands
//! that are each case-folded the same way — `(string= (string-downcase a)
//! (string-downcase b))` (or both `string-upcase`). Case-folding both sides and
//! then comparing case-sensitively (`string=`) is exactly a case-insensitive
//! comparison, which `string-equal` performs directly (and without allocating
//! the two folded copies). So the form is `(string-equal a b)`.
//!
//! Both operands must use the *same* case operation (`string-downcase` on both,
//! or `string-upcase` on both); a mixed pair (`(string= (string-downcase a)
//! (string-upcase b))`) compares different casings and is *not* case-insensitive,
//! so it is left alone. Only the exact two-operand `(string= X Y)` shape is
//! matched (a `:start`/`:end`-keyworded or three-argument comparison is left
//! alone), and a reader-conditional inner argument is left alone.
//!
//! The fix rewrites the form as `(string-equal a b)`, copying the two inner
//! arguments verbatim, so the rule is auto-fixable.
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

/// The non-destructive string case operations that fold to a canonical case.
const CASE_OPS: [&str; 2] = ["string-downcase", "string-upcase"];

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `view` is a single-argument `(string-downcase x)` / `(string-upcase x)`
/// call, returns `(lowercased-op, x)`.
fn case_folded(view: &ExpressionView) -> Option<(String, &ExpressionView)> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    let op = CASE_OPS.iter().find(|op| head.eq_ignore_ascii_case(op))?;
    // children: [op, arg] — exactly one argument.
    if view.children.len() != 2 {
        return None;
    }
    Some(((*op).to_owned(), &view.children[1]))
}

#[derive(Debug, Clone)]
pub struct StringCaseFoldItem {
    pub path: PathBuf,
    /// The span of the whole `(string= …)` form.
    pub span: ByteSpan,
    /// The span of the first folded operand's argument `a`.
    pub left_span: ByteSpan,
    /// The span of the second folded operand's argument `b`.
    pub right_span: ByteSpan,
}

#[derive(Debug)]
pub struct StringCaseFoldSummary {
    pub compare_form_count: usize,
    pub violations: Vec<StringCaseFoldItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct StringCaseFoldPolicyOptions {
    fail_on_violation: bool,
}

impl StringCaseFoldPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct StringCaseFoldPolicy {
    pub fail_on_violation: bool,
    pub compare_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    compare_form_count: &mut usize,
    violations: &mut Vec<StringCaseFoldItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("string=") {
        return;
    }
    *compare_form_count += 1;

    // children: [string=, left, right] — exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let Some((left_op, left_arg)) = case_folded(&view.children[1]) else {
        return;
    };
    let Some((right_op, right_arg)) = case_folded(&view.children[2]) else {
        return;
    };
    // Both sides must fold the same way to be a case-insensitive comparison.
    if !left_op.eq_ignore_ascii_case(&right_op) {
        return;
    }
    if is_reader_conditional(left_arg) || is_reader_conditional(right_arg) {
        return;
    }

    violations.push(StringCaseFoldItem {
        path: path.to_path_buf(),
        span: view.span,
        left_span: left_arg.span,
        right_span: right_arg.span,
    });
}

/// Collects every `(string= (string-downcase a) (string-downcase b))` (or
/// upcase) across a whole file, along with the total number of `string=` forms.
pub fn collect_string_case_folds(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<StringCaseFoldItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut compare_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut compare_form_count, &mut violations)
        });
    }
    Ok((compare_form_count, violations))
}

pub fn summarize_string_case_folds(
    compare_form_count: usize,
    violations: Vec<StringCaseFoldItem>,
) -> StringCaseFoldSummary {
    StringCaseFoldSummary {
        compare_form_count,
        violations,
    }
}

pub fn evaluate_string_case_fold_policy(
    options: StringCaseFoldPolicyOptions,
    summary: &StringCaseFoldSummary,
) -> StringCaseFoldPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    StringCaseFoldPolicy {
        fail_on_violation: options.fail_on_violation(),
        compare_form_count: summary.compare_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmps(input: &str) -> (usize, Vec<StringCaseFoldItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_string_case_folds(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect string case folds")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_downcase_both_sides() {
        let source = "(string= (string-downcase a) (string-downcase b))";
        let (count, violations) = cmps(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].left_span), "a");
        assert_eq!(slice(source, violations[0].right_span), "b");
    }

    #[test]
    fn flags_upcase_both_sides() {
        let (_, violations) = cmps("(string= (string-upcase a) (string-upcase b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn preserves_compound_arguments() {
        let source = "(string= (string-downcase (name x)) (string-downcase key))";
        let (_, violations) = cmps(source);
        assert_eq!(slice(source, violations[0].left_span), "(name x)");
        assert_eq!(slice(source, violations[0].right_span), "key");
    }

    #[test]
    fn does_not_flag_mixed_case_ops() {
        assert!(
            cmps("(string= (string-downcase a) (string-upcase b))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_one_folded_side() {
        assert!(cmps("(string= (string-downcase a) b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_keyworded_comparison() {
        // (string= X Y :start1 0) is a four-argument form, not the bare shape.
        assert!(
            cmps("(string= (string-downcase a) (string-downcase b) :start1 0)")
                .1
                .is_empty()
        );
    }

    #[test]
    fn case_folds_head_and_ops() {
        let (_, violations) = cmps("(STRING= (STRING-DOWNCASE a) (STRING-DOWNCASE b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = cmps("(when (string= (string-downcase a) (string-downcase b)) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect(
            "(string= (string-downcase a) (string-downcase b))",
            Dialect::Clojure,
        )
        .expect("parse");
        let (count, violations) =
            collect_string_case_folds(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect string case folds");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = cmps("(string= (string-downcase a) (string-downcase b))");
        let summary = summarize_string_case_folds(count, items);

        let quiet =
            evaluate_string_case_fold_policy(StringCaseFoldPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_string_case_fold_policy(StringCaseFoldPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
