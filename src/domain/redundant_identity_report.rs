//! Common Lisp redundant-`identity` detection: a call `(identity X)`, which
//! returns `X` unchanged. `identity` is defined to return its single argument
//! verbatim — no coercion, no multiple-value truncation — so `(identity X)` is
//! exactly `X` and the call is pure noise (common after a higher-order default
//! `#'identity` is inlined, or in mechanically generated code).
//!
//! Only the one-argument call shape is flagged. A function *reference*
//! `#'identity` (used as a `:key`/`:test` argument, say) is a list head with a
//! reader prefix, not a call, and is left alone; an argument-mismatched
//! `(identity)` or `(identity a b)` is left to the arity rules.
//!
//! The fix replaces the whole form with the argument's exact source, so the rule
//! is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, list_head};

#[derive(Debug, Clone)]
pub struct RedundantIdentityItem {
    pub path: PathBuf,
    /// The span of the whole `(identity X)` form.
    pub span: ByteSpan,
    /// The span of the argument `X` (lets a fix substitute its source).
    pub inner_span: ByteSpan,
}

#[derive(Debug)]
pub struct RedundantIdentitySummary {
    pub identity_form_count: usize,
    pub violations: Vec<RedundantIdentityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantIdentityPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantIdentityPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantIdentityPolicy {
    pub fail_on_violation: bool,
    pub identity_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_identity(
    view: &ExpressionView,
    path: &Path,
    identity_form_count: &mut usize,
    violations: &mut Vec<RedundantIdentityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("identity") {
        return;
    }
    *identity_form_count += 1;

    // children[0] is `identity`; a single argument means exactly two children.
    if view.children.len() != 2 {
        return;
    }

    violations.push(RedundantIdentityItem {
        path: path.to_path_buf(),
        span: view.span,
        inner_span: view.children[1].span,
    });
}

/// Collects every redundant `(identity X)` call across a whole file, along with
/// the total number of `identity` forms scanned.
pub fn collect_redundant_identities(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantIdentityItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut identity_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_identity(subview, path, &mut identity_form_count, &mut violations)
        });
    }
    Ok((identity_form_count, violations))
}

pub fn summarize_redundant_identities(
    identity_form_count: usize,
    violations: Vec<RedundantIdentityItem>,
) -> RedundantIdentitySummary {
    RedundantIdentitySummary {
        identity_form_count,
        violations,
    }
}

pub fn evaluate_redundant_identity_policy(
    options: RedundantIdentityPolicyOptions,
    summary: &RedundantIdentitySummary,
) -> RedundantIdentityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantIdentityPolicy {
        fail_on_violation: options.fail_on_violation(),
        identity_form_count: summary.identity_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identities(input: &str) -> (usize, Vec<RedundantIdentityItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_identities(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant identities")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_identity_of_a_symbol() {
        let source = "(identity x)";
        let (count, violations) = identities(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].inner_span), "x");
    }

    #[test]
    fn preserves_compound_argument_source() {
        let source = "(identity (compute a b))";
        let (_, violations) = identities(source);
        assert_eq!(slice(source, violations[0].inner_span), "(compute a b)");
    }

    #[test]
    fn does_not_flag_zero_or_multi_argument() {
        assert!(identities("(identity)").1.is_empty());
        assert!(identities("(identity a b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_function_reference() {
        // #'identity is a reference, not a call; sort's :key here is fine.
        let (count, violations) = identities("(sort xs #'< :key #'identity)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = identities("(IDENTITY x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_identity() {
        let (_, violations) = identities("(list (identity y))");
        assert_eq!(violations.len(), 1);
        assert_eq!(slice("(list (identity y))", violations[0].inner_span), "y");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(identity x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_redundant_identities(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant identities");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = identities("(identity x)");
        let summary = summarize_redundant_identities(count, items);

        let quiet = evaluate_redundant_identity_policy(
            RedundantIdentityPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_identity_policy(RedundantIdentityPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
