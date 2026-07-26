//! Common Lisp accessor-arity detection: a standard element or keyed-lookup
//! accessor called with the wrong number of arguments. The element accessors
//! `nth`, `elt`, `nthcdr`, `svref`, `char`, and `schar` take exactly two
//! arguments; the keyed lookups `gethash` and `getf` take two or three (with
//! an optional default). A wrong count — `(gethash key)` (a very common typo
//! that forgets the table), `(nth n)`, or `(elt seq idx extra)` — is a program
//! error, caught at compile time rather than by the reader.
//!
//! Like the other arity lints, this assumes the standard bindings: a local
//! `flet`/`macrolet` that shadows one of these names with a different arity is
//! rare and unsupported.
//!
//! Forms whose written arity may differ from their evaluated arity are skipped
//! to avoid false positives: a quoted/quasiquoted call (data or a template),
//! and any call with a `#+`/`#-` reader conditional or a splicing unquote
//! (`,@`) argument.
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

/// The inclusive `(min, max)` argument arity of a standard accessor, or `None`
/// if `head` is not one this rule checks.
fn expected_arity(head: &str) -> Option<(usize, usize)> {
    match head.to_ascii_lowercase().as_str() {
        "nth" | "elt" | "nthcdr" | "svref" | "char" | "schar" => Some((2, 2)),
        "gethash" | "getf" => Some((2, 3)),
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
pub struct AccessorArityItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub argument_count: usize,
    pub min_arity: usize,
    pub max_arity: usize,
}

#[derive(Debug)]
pub struct AccessorAritySummary {
    pub call_count: usize,
    pub violations: Vec<AccessorArityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct AccessorArityPolicyOptions {
    fail_on_violation: bool,
}

impl AccessorArityPolicyOptions {
    #[must_use]
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct AccessorArityPolicy {
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
    violations: &mut Vec<AccessorArityItem>,
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
        violations.push(AccessorArityItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_owned(),
            argument_count,
            min_arity,
            max_arity,
        });
    }
}

/// Collects every misarity accessor call across a whole file, along with the
/// total number of such calls scanned.
pub fn collect_accessor_arity_violations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<AccessorArityItem>)> {
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

#[must_use]
pub fn summarize_accessor_arity(
    call_count: usize,
    violations: Vec<AccessorArityItem>,
) -> AccessorAritySummary {
    AccessorAritySummary {
        call_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_accessor_arity_policy(
    options: AccessorArityPolicyOptions,
    summary: &AccessorAritySummary,
) -> AccessorArityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    AccessorArityPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_count: summary.call_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

/// A human phrase for the expected arity of one violation, e.g. `exactly 2`.
#[must_use]
pub fn expected_arity_phrase(item: &AccessorArityItem) -> String {
    arity_phrase(item.min_arity, item.max_arity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<AccessorArityItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_accessor_arity_violations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect accessor arity violations")
    }

    #[test]
    fn flags_gethash_missing_the_table() {
        let (call_count, items) = violations("(gethash key)");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "gethash");
        assert_eq!(items[0].argument_count, 1);
        assert_eq!(expected_arity_phrase(&items[0]), "2 or 3");
    }

    #[test]
    fn does_not_flag_gethash_with_a_default() {
        let (_, items) = violations("(gethash key table 0)");
        assert!(items.is_empty());
    }

    #[test]
    fn flags_gethash_with_too_many_arguments() {
        let (_, items) = violations("(gethash key table default extra)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].argument_count, 4);
    }

    #[test]
    fn flags_nth_missing_the_list() {
        let (_, items) = violations("(nth n)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "nth");
        assert_eq!(expected_arity_phrase(&items[0]), "exactly 2");
    }

    #[test]
    fn does_not_flag_valid_binary_accessors() {
        let (call_count, items) = violations("(nth n items) (elt seq 0) (svref v i) (char s 0)");
        assert_eq!(call_count, 4);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_getf_missing_the_indicator() {
        let (_, items) = violations("(getf plist)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "getf");
    }

    #[test]
    fn skips_a_reader_conditional_argument() {
        let (call_count, items) = violations("(gethash key #+sbcl a #-sbcl b)");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_call() {
        let (call_count, items) = violations("(list '(nth n))");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_call_nested_in_a_function_body() {
        let (call_count, items) = violations("(defun f (k h) (when (gethash k) 1))");
        assert_eq!(call_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(gethash key)", Dialect::Clojure).expect("parse input");
        let (call_count, items) =
            collect_accessor_arity_violations(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect accessor arity violations");
        assert_eq!(call_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (call_count, items) = violations("(nth n)");
        let summary = summarize_accessor_arity(call_count, items);

        let quiet =
            evaluate_accessor_arity_policy(AccessorArityPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_accessor_arity_policy(AccessorArityPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
