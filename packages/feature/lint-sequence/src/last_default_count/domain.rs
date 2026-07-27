//! Common Lisp redundant-`last`-count detection: a call `(last list 1)` whose
//! trailing count argument is the literal `1`. CLHS defines `last` as
//! `last list &optional (n 1)` — the count defaults to `1`, so `(last list 1)`
//! restates the default and is exactly `(last list)`.
//!
//! Only the exact three-element shape `(last x 1)` is flagged, with the count a
//! bare integer `1` literal (no reader prefixes); a non-`1` count (`(last x 2)`),
//! the already-minimal `(last x)`, and a reader-conditional count are left alone.
//!
//! The fix deletes the redundant trailing ` 1` argument (from the end of the
//! list argument through the `1`), leaving the rest byte-identical, so the rule
//! is auto-fixable.
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

/// Whether `view` is the bare integer `1` literal (no reader prefixes).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view).is_some_and(|t| t == "1")
}

#[derive(Debug, Clone)]
pub struct LastDefaultCountItem {
    pub path: PathBuf,
    /// The span of the whole `(last …)` call form.
    pub span: ByteSpan,
    /// The span to delete: the trailing ` 1` count argument.
    pub removal_span: ByteSpan,
}

#[derive(Debug)]
pub struct LastDefaultCountSummary {
    pub call_form_count: usize,
    pub violations: Vec<LastDefaultCountItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LastDefaultCountPolicyOptions {
    fail_on_violation: bool,
}

impl LastDefaultCountPolicyOptions {
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
pub struct LastDefaultCountPolicy {
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
    violations: &mut Vec<LastDefaultCountItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("last") {
        return;
    }
    *call_form_count += 1;

    // children: [last, list, 1] — exactly the list plus an explicit count.
    if view.children.len() != 3 {
        return;
    }
    if !is_one_literal(&view.children[2]) {
        return;
    }
    let removal_span = ByteSpan::new(view.children[1].span.end(), view.children[2].span.end());
    violations.push(LastDefaultCountItem {
        path: path.to_path_buf(),
        span: view.span,
        removal_span,
    });
}

/// Collects every `(last list 1)` across a whole file, along with the total
/// number of `last` calls scanned.
pub fn collect_last_default_counts(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<LastDefaultCountItem>)> {
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
pub const fn summarize_last_default_counts(
    call_form_count: usize,
    violations: Vec<LastDefaultCountItem>,
) -> LastDefaultCountSummary {
    LastDefaultCountSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_last_default_count_policy(
    options: LastDefaultCountPolicyOptions,
    summary: &LastDefaultCountSummary,
) -> LastDefaultCountPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LastDefaultCountPolicy {
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

    fn calls(input: &str) -> (usize, Vec<LastDefaultCountItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_last_default_counts(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect last default counts")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_explicit_one() {
        let source = "(last xs 1)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " 1");
    }

    #[test]
    fn does_not_flag_bare_last() {
        let (count, violations) = calls("(last xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_count() {
        let (_, violations) = calls("(last xs 2)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(LAST xs 1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(car (last xs 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(last xs 1)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_last_default_counts(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect last default counts");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(last xs 1)");
        let summary = summarize_last_default_counts(count, items);

        let quiet =
            evaluate_last_default_count_policy(LastDefaultCountPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_last_default_count_policy(LastDefaultCountPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
