//! Common Lisp redundant-`:from-end`-`nil` detection: a call to a standard
//! sequence operator with an explicit `:from-end nil`. For these operators
//! CLHS specifies that `:from-end` *defaults to* `nil`, so
//! `(find x seq :from-end nil)` is exactly `(find x seq)` — the explicit
//! `:from-end nil` restates the default.
//!
//! Scope is gated to the operators whose `:from-end` defaults to `nil`
//! ([`FROM_END_HEADS`] — `find`, `position`, `count`, `remove`, `substitute`,
//! the `-if`/`-if-not` variants, `remove-duplicates`, `reduce`, `search`,
//! `mismatch`, …). Only a bare `nil` literal value is flagged.
//!
//! The fix deletes the redundant ` :from-end nil` argument pair (from the end of
//! the preceding argument through the `nil`), leaving the rest byte-identical, so
//! the rule is auto-fixable.
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

/// Sequence operators whose `:from-end` keyword defaults to `nil` per CLHS.
const FROM_END_HEADS: [&str; 26] = [
    "find",
    "find-if",
    "find-if-not",
    "position",
    "position-if",
    "position-if-not",
    "count",
    "count-if",
    "count-if-not",
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
    "remove-duplicates",
    "delete-duplicates",
    "reduce",
    "search",
    "mismatch",
];

/// Whether `view` is the bare `:from-end` keyword atom.
fn is_from_end_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":from-end"))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct RedundantFromEndNilItem {
    pub path: PathBuf,
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :from-end nil` argument pair.
    pub removal_span: ByteSpan,
    /// The operator name, for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct RedundantFromEndNilSummary {
    pub call_form_count: usize,
    pub violations: Vec<RedundantFromEndNilItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantFromEndNilPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantFromEndNilPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct RedundantFromEndNilPolicy {
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
    violations: &mut Vec<RedundantFromEndNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !FROM_END_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_from_end_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(RedundantFromEndNilItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every sequence call with a redundant `:from-end nil` across a
/// whole file, along with the total number of such calls scanned.
pub fn collect_redundant_from_end_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantFromEndNilItem>)> {
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

pub fn summarize_redundant_from_end_nils(
    call_form_count: usize,
    violations: Vec<RedundantFromEndNilItem>,
) -> RedundantFromEndNilSummary {
    RedundantFromEndNilSummary {
        call_form_count,
        violations,
    }
}

pub fn evaluate_redundant_from_end_nil_policy(
    options: RedundantFromEndNilPolicyOptions,
    summary: &RedundantFromEndNilSummary,
) -> RedundantFromEndNilPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantFromEndNilPolicy {
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

    fn calls(input: &str) -> (usize, Vec<RedundantFromEndNilItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_from_end_nils(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant from-end nils")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_from_end_nil() {
        let source = "(find x seq :from-end nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :from-end nil");
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(remove x seq :from-end nil :count 3)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :from-end nil");
    }

    #[test]
    fn does_not_flag_non_nil() {
        let (count, violations) = calls("(find x seq :from-end t)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_allowlisted_head() {
        // sort is not in the :from-end-defaulting allowlist.
        let (count, violations) = calls("(sort xs #'< :from-end nil)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(FIND x seq :from-end nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (position y xs :from-end nil) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(find x seq :from-end nil)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_redundant_from_end_nils(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant from-end nils");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(find x seq :from-end nil)");
        let summary = summarize_redundant_from_end_nils(count, items);

        let quiet = evaluate_redundant_from_end_nil_policy(
            RedundantFromEndNilPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_from_end_nil_policy(
            RedundantFromEndNilPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
