//! Common Lisp redundant-`:count`-`nil` detection: a call to a standard
//! bounded-sequence operator with an explicit `:count nil`. For these operators
//! CLHS specifies that `:count` *defaults to* `nil` (meaning "unlimited"), so
//! `(remove x seq :count nil)` is exactly `(remove x seq)` — the explicit count
//! restates the default.
//!
//! Scope is gated to the removing/substituting operators whose `:count` defaults
//! to `nil` ([`COUNT_HEADS`] — `remove`, `delete`, `substitute`, `nsubstitute`,
//! and their `-if`/`-if-not` variants). Operators that do not take a `:count`
//! keyword (`find`, `position`, …) are not touched. Only a bare `nil` literal
//! value is flagged.
//!
//! The fix deletes the redundant ` :count nil` argument pair (from the end of the
//! preceding argument through the `nil`), leaving the rest byte-identical, so the
//! rule is auto-fixable.
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

/// Removing/substituting operators whose `:count` keyword defaults to `nil` per CLHS.
const COUNT_HEADS: [&str; 12] = [
    "remove",
    "remove-if",
    "remove-if-not",
    "delete",
    "delete-if",
    "delete-if-not",
    "substitute",
    "substitute-if",
    "substitute-if-not",
    "nsubstitute",
    "nsubstitute-if",
    "nsubstitute-if-not",
];

/// Whether `view` is the bare `:count` keyword atom.
fn is_count_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":count"))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct RedundantCountNilItem {
    pub path: PathBuf,
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :count nil` argument pair.
    pub removal_span: ByteSpan,
    /// The operator name, for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct RedundantCountNilSummary {
    pub call_form_count: usize,
    pub violations: Vec<RedundantCountNilItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantCountNilPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantCountNilPolicyOptions {
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
pub struct RedundantCountNilPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<RedundantCountNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !COUNT_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_count_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(RedundantCountNilItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every bounded-sequence call with a redundant `:count nil` across a
/// whole file, along with the total number of such calls scanned.
pub fn collect_redundant_count_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantCountNilItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut call_form_count, &mut violations);
        });
    }
    Ok((call_form_count, violations))
}

#[must_use]
pub const fn summarize_redundant_count_nils(
    call_form_count: usize,
    violations: Vec<RedundantCountNilItem>,
) -> RedundantCountNilSummary {
    RedundantCountNilSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_count_nil_policy(
    options: RedundantCountNilPolicyOptions,
    summary: &RedundantCountNilSummary,
) -> RedundantCountNilPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantCountNilPolicy {
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

    fn calls(input: &str) -> (usize, Vec<RedundantCountNilItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_count_nils(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant count nils")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_count_nil() {
        let source = "(remove x seq :count nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :count nil");
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(remove x seq :count nil :from-end t)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :count nil");
    }

    #[test]
    fn does_not_flag_non_nil() {
        let (count, violations) = calls("(remove x seq :count 3)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_allowlisted_head() {
        // find does not take a :count keyword.
        let (count, violations) = calls("(find x seq :count nil)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(REMOVE x seq :count nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (delete y xs :count nil) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(remove x seq :count nil)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_redundant_count_nils(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant count nils");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(remove x seq :count nil)");
        let summary = summarize_redundant_count_nils(count, items);

        let quiet = evaluate_redundant_count_nil_policy(
            RedundantCountNilPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_count_nil_policy(
            RedundantCountNilPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
