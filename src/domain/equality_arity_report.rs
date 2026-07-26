//! Common Lisp equality-predicate-arity detection: an `eq`, `eql`, `equal`, or
//! `equalp` call with the wrong number of arguments. All four are strictly
//! binary — `(eq object-1 object-2)` — so `(eq x)` (too few) or `(eql a b c)`
//! (too many) is a program error, caught at compile time rather than by the
//! reader.
//!
//! Scoped to these four general equality predicates on purpose: the numeric and
//! character/string comparisons (`=`, `<`, `char=`, `string=`, …) are variadic
//! (`(< a b c)` chains, `(= a)` is vacuously true), so they are not checked
//! here.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted call (data or a template),
//! and any call with a `#+`/`#-` reader conditional or a splicing unquote
//! (`,@`) argument — e.g. `(eq x #+sbcl y #-sbcl z)` is a valid feature-portable
//! comparison whose written three-token shape is not a real arity error.
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

const EQUALITY_HEADS: [&str; 4] = ["eq", "eql", "equal", "equalp"];
const EXPECTED_ARGUMENTS: usize = 2;

/// Whether a child form can change how many argument forms actually reach the
/// evaluator, making a static arity count unreliable: a `,@` splice or Clojure
/// reader conditional (a prefix), or a Common Lisp `#+`/`#-` conditional (an
/// atom whose text begins `#+`/`#-`).
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

#[derive(Debug, Clone)]
pub struct EqualityArityItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub argument_count: usize,
}

#[derive(Debug)]
pub struct EqualityAritySummary {
    pub call_count: usize,
    pub violations: Vec<EqualityArityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EqualityArityPolicyOptions {
    fail_on_violation: bool,
}

impl EqualityArityPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EqualityArityPolicy {
    pub fail_on_violation: bool,
    pub call_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub(crate) fn examine_call(
    view: &ExpressionView,
    path: &Path,
    call_count: &mut usize,
    violations: &mut Vec<EqualityArityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EQUALITY_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
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
    if argument_count != EXPECTED_ARGUMENTS {
        violations.push(EqualityArityItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_owned(),
            argument_count,
        });
    }
}

/// Collects every misarity equality-predicate call across a whole file, along
/// with the total number of such calls scanned.
pub fn collect_equality_arity_violations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EqualityArityItem>)> {
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

pub fn summarize_equality_arity(
    call_count: usize,
    violations: Vec<EqualityArityItem>,
) -> EqualityAritySummary {
    EqualityAritySummary {
        call_count,
        violations,
    }
}

pub fn evaluate_equality_arity_policy(
    options: EqualityArityPolicyOptions,
    summary: &EqualityAritySummary,
) -> EqualityArityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EqualityArityPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_count: summary.call_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<EqualityArityItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_equality_arity_violations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect equality arity violations")
    }

    #[test]
    fn flags_eq_with_too_few_arguments() {
        let (call_count, items) = violations("(eq x)");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "eq");
        assert_eq!(items[0].argument_count, 1);
    }

    #[test]
    fn flags_eql_with_too_many_arguments() {
        let (_, items) = violations("(eql a b c)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "eql");
        assert_eq!(items[0].argument_count, 3);
    }

    #[test]
    fn flags_equal_and_equalp() {
        let (_, one) = violations("(equal a)");
        assert_eq!(one.len(), 1);
        let (_, two) = violations("(equalp a b c d)");
        assert_eq!(two.len(), 1);
    }

    #[test]
    fn does_not_flag_a_binary_call() {
        let (call_count, items) = violations("(eq a b)");
        assert_eq!(call_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_variadic_numeric_comparisons() {
        let (call_count, items) = violations("(= a b c) (< x)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_argument() {
        let (call_count, items) = violations("(eq x #+sbcl y #-sbcl z)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_call() {
        let (call_count, items) = violations("(list '(eq x))");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_call_nested_in_a_function_body() {
        let (call_count, items) = violations("(defun f (x) (when (eq x) 1))");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(eq x)", Dialect::Clojure).expect("parse input");
        let (call_count, items) =
            collect_equality_arity_violations(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect equality arity violations");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (call_count, items) = violations("(eq x)");
        let summary = summarize_equality_arity(call_count, items);

        let quiet =
            evaluate_equality_arity_policy(EqualityArityPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_equality_arity_policy(EqualityArityPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
