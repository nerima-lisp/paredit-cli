//! Common Lisp redundant-`:start`-`0` detection: a call to a standard
//! bounded-sequence operator with an explicit `:start 0`. For these operators
//! CLHS specifies that `:start` *defaults to* `0`, so `(find x seq :start 0)` is
//! exactly `(find x seq)` — the explicit start restates the default.
//!
//! Scope is gated to the single-sequence operators whose `:start` defaults to `0`
//! ([`START_HEADS`] — `find`, `position`, `count`, `remove`, `substitute`, the
//! `-if`/`-if-not` variants, `fill`, `reduce`, `parse-integer`, the string-case
//! functions, …). Two-sequence operators (`search`, `mismatch`, `replace`) use
//! `:start1`/`:start2`, never a bare `:start`, and are not touched. Only a bare
//! integer literal `0` value is flagged.
//!
//! The fix deletes the redundant ` :start 0` argument pair (from the end of the
//! preceding argument through the `0`), leaving the rest byte-identical, so the
//! rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Single-sequence operators whose `:start` keyword defaults to `0` per CLHS.
const START_HEADS: [&str; 35] = [
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
    "fill",
    "reduce",
    "parse-integer",
    "read-from-string",
    "string-upcase",
    "string-downcase",
    "string-capitalize",
    "nstring-upcase",
    "nstring-downcase",
    "nstring-capitalize",
    "write-string",
    "write-line",
];

/// Whether `view` is the bare `:start` keyword atom.
fn is_start_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":start"))
}

/// Whether `view` is the bare integer `0` literal (no reader prefixes).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

#[derive(Debug, Clone)]
pub struct RedundantStartZeroItem {
    pub path: PathBuf,
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :start 0` argument pair.
    pub removal_span: ByteSpan,
    /// The operator name, for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct RedundantStartZeroSummary {
    pub call_form_count: usize,
    pub violations: Vec<RedundantStartZeroItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantStartZeroPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantStartZeroPolicyOptions {
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
pub struct RedundantStartZeroPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<RedundantStartZeroItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !START_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_start_keyword(&view.children[index]) {
            continue;
        }
        if !is_zero_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(RedundantStartZeroItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every bounded-sequence call with a redundant `:start 0` across a
/// whole file, along with the total number of such calls scanned.
pub fn collect_redundant_start_zeros(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantStartZeroItem>)> {
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
pub const fn summarize_redundant_start_zeros(
    call_form_count: usize,
    violations: Vec<RedundantStartZeroItem>,
) -> RedundantStartZeroSummary {
    RedundantStartZeroSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_start_zero_policy(
    options: RedundantStartZeroPolicyOptions,
    summary: &RedundantStartZeroSummary,
) -> RedundantStartZeroPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantStartZeroPolicy {
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

    fn calls(input: &str) -> (usize, Vec<RedundantStartZeroItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_start_zeros(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant start zeros")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_start_zero() {
        let source = "(find x seq :start 0)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :start 0");
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(remove x seq :start 0 :from-end t)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :start 0");
    }

    #[test]
    fn does_not_flag_nonzero_start() {
        let (count, violations) = calls("(find x seq :start 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_allowlisted_head() {
        // subseq takes a positional start, not a :start keyword.
        let (count, violations) = calls("(subseq seq :start 0)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(FIND x seq :start 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (position y xs :start 0) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(find x seq :start 0)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_redundant_start_zeros(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant start zeros");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(find x seq :start 0)");
        let summary = summarize_redundant_start_zeros(count, items);

        let quiet = evaluate_redundant_start_zero_policy(
            RedundantStartZeroPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_start_zero_policy(
            RedundantStartZeroPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
