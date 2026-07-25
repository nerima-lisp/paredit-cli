//! Common Lisp `the`-arity detection: a `the` special form with the wrong
//! number of arguments. `the` is `(the value-type form)` — it takes exactly
//! two arguments, a type specifier and a form. Fewer (`(the fixnum)`) or more
//! (`(the fixnum x y)`) is a program error, caught at compile time rather than
//! by the reader.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted `the` (data or a template),
//! and any `the` with a child carrying a `#+`/`#-` reader conditional or a
//! splicing unquote (`,@`) — e.g. `(the #+sbcl fixnum #-sbcl integer x)` is a
//! valid feature-portable declaration whose written three-token shape is not a
//! real arity error.
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
pub struct TheArityItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub argument_count: usize,
}

#[derive(Debug)]
pub struct TheAritySummary {
    pub the_form_count: usize,
    pub violations: Vec<TheArityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct TheArityPolicyOptions {
    fail_on_violation: bool,
}

impl TheArityPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct TheArityPolicy {
    pub fail_on_violation: bool,
    pub the_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_the(
    view: &ExpressionView,
    path: &Path,
    the_form_count: &mut usize,
    violations: &mut Vec<TheArityItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("the")) {
        return;
    }
    // A quoted/quasiquoted/unquoted `the` is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A `#+`/`#-` or `,@` argument makes the written arity unreliable.
    if view.children.iter().skip(1).any(is_arity_ambiguous) {
        return;
    }
    *the_form_count += 1;

    let argument_count = view.children.len() - 1;
    if argument_count != EXPECTED_ARGUMENTS {
        violations.push(TheArityItem {
            path: path.to_path_buf(),
            span: view.span,
            argument_count,
        });
    }
}

/// Collects every misarity `the` form across a whole file, along with the total
/// number of `the` forms scanned.
pub fn collect_the_arity_violations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<TheArityItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut the_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_the(subview, path, &mut the_form_count, &mut violations)
        });
    }
    Ok((the_form_count, violations))
}

pub fn summarize_the_arity(
    the_form_count: usize,
    violations: Vec<TheArityItem>,
) -> TheAritySummary {
    TheAritySummary {
        the_form_count,
        violations,
    }
}

pub fn evaluate_the_arity_policy(
    options: TheArityPolicyOptions,
    summary: &TheAritySummary,
) -> TheArityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    TheArityPolicy {
        fail_on_violation: options.fail_on_violation(),
        the_form_count: summary.the_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<TheArityItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_the_arity_violations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect the arity violations")
    }

    #[test]
    fn flags_too_few_arguments() {
        let (the_form_count, items) = violations("(the fixnum)");
        assert_eq!(the_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 1);
    }

    #[test]
    fn flags_too_many_arguments() {
        let (_, items) = violations("(the fixnum x y)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 3);
    }

    #[test]
    fn does_not_flag_a_two_argument_the() {
        let (the_form_count, items) = violations("(the fixnum (+ a b))");
        assert_eq!(the_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_reader_conditional_type() {
        let (the_form_count, items) = violations("(the #+sbcl fixnum #-sbcl integer x)");
        assert_eq!(the_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_the_form() {
        let (the_form_count, items) = violations("(list '(the fixnum))");
        assert_eq!(the_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_the_nested_in_a_function_body() {
        let (the_form_count, items) = violations("(defun f (x) (the fixnum x x))");
        assert_eq!(the_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(the fixnum)", Dialect::Clojure).expect("parse input");
        let (the_form_count, items) =
            collect_the_arity_violations(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect the arity violations");
        assert_eq!(the_form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (the_form_count, items) = violations("(the fixnum)");
        let summary = summarize_the_arity(the_form_count, items);

        let quiet = evaluate_the_arity_policy(TheArityPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_the_arity_policy(TheArityPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
