//! Common Lisp redundant-`:end`-`nil` detection: a call to a standard
//! bounded-sequence operator with an explicit `:end nil`. For these operators
//! CLHS specifies that `:end` *defaults to* `nil` (= end of sequence), so
//! `(find x seq :end nil)` is exactly `(find x seq)` — the explicit end restates
//! the default.
//!
//! Scope is gated to the single-sequence operators whose `:end` defaults to `nil`
//! ([`END_HEADS`] — `find`, `position`, `count`, `remove`, `substitute`, the
//! `-if`/`-if-not` variants, `fill`, `reduce`, `parse-integer`, the string-case
//! functions, …). Two-sequence operators (`search`, `mismatch`, `replace`) use
//! `:end1`/`:end2`, never a bare `:end`, and are not touched. Only a bare
//! `nil` literal value is flagged.
//!
//! The fix deletes the redundant ` :end nil` argument pair (from the end of the
//! preceding argument through the `nil`), leaving the rest byte-identical, so the
//! rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Single-sequence operators whose `:end` keyword defaults to `nil` per CLHS.
const END_HEADS: [&str; 35] = [
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

/// Whether `view` is the bare `:end` keyword atom.
fn is_end_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":end"))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct RedundantEndNilItem {
    pub path: PathBuf,
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :end nil` argument pair.
    pub removal_span: ByteSpan,
    /// The operator name, for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct RedundantEndNilSummary {
    pub call_form_count: usize,
    pub violations: Vec<RedundantEndNilItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantEndNilPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantEndNilPolicyOptions {
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
pub struct RedundantEndNilPolicy {
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
    violations: &mut Vec<RedundantEndNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !END_HEADS.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_end_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(RedundantEndNilItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every bounded-sequence call with a redundant `:end nil` across a
/// whole file, along with the total number of such calls scanned.
pub fn collect_redundant_end_nils(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<RedundantEndNilItem>)> {
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
pub const fn summarize_redundant_end_nils(
    call_form_count: usize,
    violations: Vec<RedundantEndNilItem>,
) -> RedundantEndNilSummary {
    RedundantEndNilSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_end_nil_policy(
    options: RedundantEndNilPolicyOptions,
    summary: &RedundantEndNilSummary,
) -> RedundantEndNilPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantEndNilPolicy {
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

    fn calls(input: &str) -> (usize, Vec<RedundantEndNilItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_end_nils(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant end nils")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_end_nil() {
        let source = "(find x seq :end nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :end nil");
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(remove x seq :end nil :from-end t)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :end nil");
    }

    #[test]
    fn does_not_flag_non_nil_end() {
        let (count, violations) = calls("(find x seq :end 5)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_allowlisted_head() {
        // subseq takes a positional end, not a :end keyword.
        let (count, violations) = calls("(subseq seq :end nil)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(FIND x seq :end nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (position y xs :end nil) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(find x seq :end nil)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_redundant_end_nils(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant end nils");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(find x seq :end nil)");
        let summary = summarize_redundant_end_nils(count, items);

        let quiet =
            evaluate_redundant_end_nil_policy(RedundantEndNilPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_end_nil_policy(RedundantEndNilPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
