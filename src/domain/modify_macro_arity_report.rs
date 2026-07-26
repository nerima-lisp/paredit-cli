//! Common Lisp modify-macro-arity detection: a place-modifying macro called
//! with the wrong number of arguments. `incf` and `decf` are
//! `(incf place [delta])` — one or two arguments; `push` is
//! `(push item place)` — exactly two; `pop` is `(pop place)` — exactly one. A
//! wrong argument count (`(incf x 1 2)`, `(push a)`, `(pop)`) is a program
//! error, caught at macroexpansion rather than by the reader.
//!
//! Scoped to these fixed-arity macros on purpose: `pushnew` and the general
//! `setf`/`setq` family take a variable number of arguments (keyword options
//! or place/value pairs) and are handled elsewhere
//! ([`crate::domain::setf_arity_report`]).
//!
//! Forms whose argument count is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted form (data or a template), and any
//! call with a `#+`/`#-` reader conditional or `,@` splice argument, where the
//! written count differs from the evaluated one.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// The inclusive `(min, max)` argument arity of a place-modifying macro, or
/// `None` if `head` is not one this rule checks.
fn expected_arity(head: &str) -> Option<(usize, usize)> {
    match head.to_ascii_lowercase().as_str() {
        "incf" | "decf" => Some((1, 2)),
        "push" => Some((2, 2)),
        "pop" => Some((1, 1)),
        _ => None,
    }
}

/// Whether an argument's reader prefix or `#+`/`#-` marker makes the static
/// argument count unreliable.
fn is_arity_ambiguous(view: &ExpressionView) -> bool {
    let ambiguous_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    ambiguous_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

fn arity_phrase(min: usize, max: usize) -> String {
    if min == max {
        format!("exactly {min}")
    } else {
        format!("{min} or {max}")
    }
}

#[derive(Debug, Clone)]
pub struct ModifyMacroArityItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub argument_count: usize,
    pub min_arity: usize,
    pub max_arity: usize,
}

#[derive(Debug)]
pub struct ModifyMacroAritySummary {
    pub call_count: usize,
    pub violations: Vec<ModifyMacroArityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct ModifyMacroArityPolicyOptions {
    fail_on_violation: bool,
}

impl ModifyMacroArityPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct ModifyMacroArityPolicy {
    pub fail_on_violation: bool,
    pub call_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_call(
    view: &ExpressionView,
    path: &Path,
    call_count: &mut usize,
    violations: &mut Vec<ModifyMacroArityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some((min_arity, max_arity)) = expected_arity(head) else {
        return;
    };
    // A quoted/quasiquoted/unquoted call is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A `#+`/`#-` or `,@` argument makes the written arity unreliable.
    if view.children.iter().skip(1).any(is_arity_ambiguous) {
        return;
    }
    *call_count += 1;

    let argument_count = view.children.len() - 1;
    if !(min_arity..=max_arity).contains(&argument_count) {
        violations.push(ModifyMacroArityItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_owned(),
            argument_count,
            min_arity,
            max_arity,
        });
    }
}

/// Collects every misarity modify-macro call across a whole file, along with
/// the total number of such calls scanned.
pub fn collect_modify_macro_arity_violations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<ModifyMacroArityItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, path, &mut call_count, &mut violations);
        });
    }
    Ok((call_count, violations))
}

pub fn summarize_modify_macro_arity(
    call_count: usize,
    violations: Vec<ModifyMacroArityItem>,
) -> ModifyMacroAritySummary {
    ModifyMacroAritySummary {
        call_count,
        violations,
    }
}

pub fn evaluate_modify_macro_arity_policy(
    options: ModifyMacroArityPolicyOptions,
    summary: &ModifyMacroAritySummary,
) -> ModifyMacroArityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    ModifyMacroArityPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_count: summary.call_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

/// A human phrase for the expected arity of one violation, e.g. `exactly 2`.
pub fn expected_arity_phrase(item: &ModifyMacroArityItem) -> String {
    arity_phrase(item.min_arity, item.max_arity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<ModifyMacroArityItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_modify_macro_arity_violations(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect modify macro arity violations")
    }

    #[test]
    fn flags_incf_with_too_many_arguments() {
        let (call_count, items) = violations("(incf x 1 2)");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "incf");
        assert_eq!(items[0].argument_count, 3);
    }

    #[test]
    fn does_not_flag_incf_with_one_or_two_arguments() {
        let (_, one) = violations("(incf x)");
        assert!(one.is_empty());
        let (_, two) = violations("(incf x 2)");
        assert!(two.is_empty());
    }

    #[test]
    fn flags_decf_with_too_many_arguments() {
        let (_, items) = violations("(decf y 1 2)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "decf");
    }

    #[test]
    fn flags_pop_with_too_many_arguments() {
        let (_, items) = violations("(pop stack extra)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 2);
        assert_eq!(expected_arity_phrase(&items[0]), "exactly 1");
    }

    #[test]
    fn flags_pop_with_no_arguments() {
        let (_, items) = violations("(pop)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 0);
    }

    #[test]
    fn does_not_flag_valid_pop() {
        let (call_count, items) = violations("(pop stack)");
        assert_eq!(call_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_push_with_too_few_arguments() {
        let (_, items) = violations("(push item)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "push");
        assert_eq!(expected_arity_phrase(&items[0]), "exactly 2");
    }

    #[test]
    fn does_not_flag_valid_push() {
        let (_, items) = violations("(push item stack)");
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_argument() {
        let (call_count, items) = violations("(incf x #+sbcl 1 #-sbcl 2)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_call() {
        let (call_count, items) = violations("(list '(incf x 1 2))");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn folds_operator_case() {
        let (_, items) = violations("(INCF x 1 2)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn finds_a_call_nested_in_a_function_body() {
        let (call_count, items) = violations("(defun f (x) (incf x 1 2))");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(incf x 1 2)", Dialect::Clojure).expect("parse input");
        let (call_count, items) = collect_modify_macro_arity_violations(
            &PathBuf::from("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("collect modify macro arity violations");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (call_count, items) = violations("(incf x 1 2)");
        let summary = summarize_modify_macro_arity(call_count, items);

        let quiet =
            evaluate_modify_macro_arity_policy(ModifyMacroArityPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_modify_macro_arity_policy(ModifyMacroArityPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
