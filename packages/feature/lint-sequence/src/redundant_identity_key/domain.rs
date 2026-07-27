//! Common Lisp redundant-`:key`-`identity` detection: a call to one of the
//! standard `:key`-taking sequence/list operators with an explicit
//! `:key #'identity` (or `:key nil`). CLHS specifies that `:key` defaults to
//! `nil`, meaning "use the element itself" — exactly what `identity` does — so
//! `(sort xs #'< :key #'identity)` is `(sort xs #'<)` and `:key nil` merely
//! restates the default.
//!
//! Scope is gated to operators documented to accept a `:key` argument
//! ([`KEY_HEADS`]). Note this list is *not* the same as the eql-defaulting
//! `:test` set: `tree-equal` takes `:test` but no `:key` (excluded here), while
//! `sort`/`stable-sort`/`merge`/`reduce` and the `-if` variants take `:key` but
//! no `:test` (included here). The list keeps only CLHS-verified `:key`-takers,
//! erring toward omission so a redundant keyword is never stripped from code
//! that would not accept it.
//!
//! The redundant value is `#'identity`, `'identity`, `(function identity)`, or
//! the literal `nil`. The fix deletes the ` :key #'identity` argument pair,
//! leaving the rest of the call byte-identical, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// Operators that accept a `:key` argument (defaulting to `nil` = identity).
const KEY_HEADS: [&str; 37] = [
    "adjoin",
    "assoc",
    "assoc-if",
    "count",
    "count-if",
    "delete",
    "delete-duplicates",
    "delete-if",
    "find",
    "find-if",
    "intersection",
    "member",
    "member-if",
    "merge",
    "mismatch",
    "nintersection",
    "nset-difference",
    "nset-exclusive-or",
    "nsubstitute",
    "nunion",
    "position",
    "position-if",
    "pushnew",
    "rassoc",
    "reduce",
    "remove",
    "remove-duplicates",
    "remove-if",
    "search",
    "set-difference",
    "set-exclusive-or",
    "sort",
    "stable-sort",
    "subsetp",
    "substitute",
    "substitute-if",
    "union",
];

/// Whether `view` designates `identity` (`#'identity`, `'identity`,
/// `(function identity)`) or the literal `nil` — the redundant `:key` values.
fn is_identity_or_nil(view: &ExpressionView) -> bool {
    if let Some(text) = atom_text(view) {
        // Bare `nil` (no reader prefix) is the explicit default.
        if view.reader_prefixes.is_empty() {
            return text.eq_ignore_ascii_case("nil");
        }
        // `#'identity` / `'identity`: symbol content begins at `symbol_offset`.
        let symbol = text.get(view.symbol_offset..).unwrap_or(text);
        return symbol.eq_ignore_ascii_case("identity")
            && view.reader_prefixes.len() == 1
            && matches!(
                view.reader_prefixes[0],
                ReaderPrefix::Function | ReaderPrefix::Quote
            );
    }
    if is_paren_list(view) && view.children.len() == 2 && view.reader_prefixes.is_empty() {
        let heads_function = list_head(view)
            .is_some_and(|h| h.eq_ignore_ascii_case("function") || h.eq_ignore_ascii_case("quote"));
        if heads_function {
            return atom_text(&view.children[1])
                .is_some_and(|t| t.eq_ignore_ascii_case("identity"));
        }
    }
    false
}

/// Whether `view` is the `:key` keyword atom.
fn is_key_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":key"))
}

#[derive(Debug, Clone)]
pub struct RedundantIdentityKeyItem {
    pub path: PathBuf,
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :key #'identity` argument pair.
    pub removal_span: ByteSpan,
    /// The operator name, for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct RedundantIdentityKeySummary {
    pub call_form_count: usize,
    pub violations: Vec<RedundantIdentityKeyItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantIdentityKeyPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantIdentityKeyPolicyOptions {
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
pub struct RedundantIdentityKeyPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_call(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<RedundantIdentityKeyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !KEY_HEADS.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        return;
    }
    *call_form_count += 1;

    // Scan for a `:key` keyword immediately followed by an identity/nil value.
    for index in 1..view.children.len().saturating_sub(1) {
        if !is_key_keyword(&view.children[index]) {
            continue;
        }
        let value = &view.children[index + 1];
        if !is_identity_or_nil(value) {
            continue;
        }
        let removal_span = ByteSpan::new(view.children[index - 1].span.end(), value.span.end());
        violations.push(RedundantIdentityKeyItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every `:key`-taking call with a redundant explicit
/// `:key #'identity`/`:key nil` across a whole file, along with the total
/// number of such calls scanned.
pub fn collect_redundant_identity_keys(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantIdentityKeyItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, path, &mut call_form_count, &mut violations);
        });
    }
    Ok((call_form_count, violations))
}

#[must_use]
pub const fn summarize_redundant_identity_keys(
    call_form_count: usize,
    violations: Vec<RedundantIdentityKeyItem>,
) -> RedundantIdentityKeySummary {
    RedundantIdentityKeySummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_identity_key_policy(
    options: RedundantIdentityKeyPolicyOptions,
    summary: &RedundantIdentityKeySummary,
) -> RedundantIdentityKeyPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantIdentityKeyPolicy {
        fail_on_violation: options.fail_on_violation(),
        call_form_count: summary.call_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(input: &str) -> (usize, Vec<RedundantIdentityKeyItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_identity_keys(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant identity keys")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_sharp_quoted_identity_key() {
        let source = "(sort xs #'< :key #'identity)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :key #'identity"
        );
    }

    #[test]
    fn flags_nil_key() {
        let (_, violations) = calls("(find x list :key nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_quoted_identity_key() {
        let (_, violations) = calls("(remove-duplicates seq :key 'identity)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_explicit_function_identity_key() {
        let (_, violations) = calls("(count x seq :key (function identity))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn removal_keeps_trailing_keywords() {
        let source = "(remove x seq :key #'identity :from-end t)";
        let (_, violations) = calls(source);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :key #'identity"
        );
    }

    #[test]
    fn does_not_flag_a_custom_key() {
        let (count, violations) = calls("(sort xs #'< :key #'car)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_key_head() {
        // tree-equal takes :test but not :key.
        let (count, violations) = calls("(tree-equal a b :key #'identity)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(FIND x list :key #'identity)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (member y xs :key #'identity) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(find x list :key #'identity)", Dialect::Clojure)
                .expect("parse");
        let (count, violations) =
            collect_redundant_identity_keys(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant identity keys");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(find x list :key #'identity)");
        let summary = summarize_redundant_identity_keys(count, items);

        let quiet = evaluate_redundant_identity_key_policy(
            RedundantIdentityKeyPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_identity_key_policy(
            RedundantIdentityKeyPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
