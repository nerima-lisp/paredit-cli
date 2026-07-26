//! Common Lisp redundant-`:test`-`eql` detection: a call to one of the standard
//! eql-defaulting sequence/list operators with an explicit `:test #'eql`. For
//! these operators CLHS specifies that the `:test` argument *defaults to* `eql`,
//! so `(find x list :test #'eql)` is exactly `(find x list)` — the explicit test
//! restates the default and adds only noise.
//!
//! Scope is gated to the operators whose `:test` is documented to default to
//! `eql` ([`EQL_TEST_HEADS`] — `member`, `assoc`, `find`, `position`, `count`,
//! `remove`, the set operations, …). A function with a *required* predicate
//! (e.g. `sort`) or a different default is never touched, and only `:test`
//! (never `:test-not`, which means "not eql") with an `eql` designator is
//! flagged.
//!
//! The three eql designators are recognized: `#'eql`, `'eql`, and the explicit
//! `(function eql)` / `(quote eql)` list. The fix deletes the redundant
//! ` :test #'eql` argument pair, leaving the rest of the call byte-identical, so
//! the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

/// Operators whose `:test` keyword argument defaults to `eql` per CLHS.
const EQL_TEST_HEADS: [&str; 30] = [
    "adjoin",
    "assoc",
    "count",
    "delete",
    "delete-duplicates",
    "find",
    "intersection",
    "member",
    "mismatch",
    "nintersection",
    "nset-difference",
    "nset-exclusive-or",
    "nsublis",
    "nsubst",
    "nsubstitute",
    "nunion",
    "position",
    "pushnew",
    "rassoc",
    "remove",
    "remove-duplicates",
    "search",
    "set-difference",
    "set-exclusive-or",
    "sublis",
    "subsetp",
    "subst",
    "substitute",
    "tree-equal",
    "union",
];

/// Whether `view` designates the `eql` function: `#'eql`, `'eql`, or the
/// explicit `(function eql)` / `(quote eql)` list.
fn is_eql_designator(view: &ExpressionView) -> bool {
    if let Some(text) = atom_text(view) {
        // A prefixed atom's `text` includes the prefix spelling; the symbol
        // content begins at `symbol_offset` (so `#'eql`/`'eql` -> `eql`).
        let symbol = text.get(view.symbol_offset..).unwrap_or(text);
        return symbol.eq_ignore_ascii_case("eql")
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
            return atom_text(&view.children[1]).is_some_and(|t| t.eq_ignore_ascii_case("eql"));
        }
    }
    false
}

/// Whether `view` is the `:test` keyword atom (not `:test-not`).
fn is_test_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":test"))
}

#[derive(Debug, Clone)]
pub struct RedundantEqlTestItem {
    pub path: PathBuf,
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :test #'eql` argument pair, from the end of the
    /// preceding argument through the eql designator.
    pub removal_span: ByteSpan,
    /// The operator name, for the finding message.
    pub head: String,
}

#[derive(Debug)]
pub struct RedundantEqlTestSummary {
    pub call_form_count: usize,
    pub violations: Vec<RedundantEqlTestItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantEqlTestPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantEqlTestPolicyOptions {
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
pub struct RedundantEqlTestPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_call(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<RedundantEqlTestItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EQL_TEST_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *call_form_count += 1;

    // Scan the argument list for a `:test` keyword immediately followed by an
    // `eql` designator. `children[0]` is the operator.
    for index in 1..view.children.len().saturating_sub(1) {
        if !is_test_keyword(&view.children[index]) {
            continue;
        }
        let value = &view.children[index + 1];
        if !is_eql_designator(value) {
            continue;
        }
        // Delete from the end of the preceding element through the eql
        // designator, so the leading whitespace before `:test` goes too.
        let removal_span = ByteSpan::new(view.children[index - 1].span.end(), value.span.end());
        violations.push(RedundantEqlTestItem {
            path: path.to_path_buf(),
            span: view.span,
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every eql-defaulting call with a redundant explicit `:test #'eql`
/// across a whole file, along with the total number of such calls scanned.
pub fn collect_redundant_eql_tests(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<RedundantEqlTestItem>)> {
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
pub const fn summarize_redundant_eql_tests(
    call_form_count: usize,
    violations: Vec<RedundantEqlTestItem>,
) -> RedundantEqlTestSummary {
    RedundantEqlTestSummary {
        call_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_eql_test_policy(
    options: RedundantEqlTestPolicyOptions,
    summary: &RedundantEqlTestSummary,
) -> RedundantEqlTestPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantEqlTestPolicy {
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

    fn calls(input: &str) -> (usize, Vec<RedundantEqlTestItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_eql_tests(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant eql tests")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_sharp_quoted_eql_test() {
        let source = "(find x list :test #'eql)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :test #'eql");
    }

    #[test]
    fn flags_quoted_eql_test() {
        let (_, violations) = calls("(member y items :test 'eql)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_explicit_function_eql_test() {
        let (_, violations) = calls("(assoc k alist :test (function eql))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn removal_keeps_trailing_keywords() {
        let source = "(remove-duplicates seq :test #'eql :from-end t)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :test #'eql");
    }

    #[test]
    fn does_not_flag_a_custom_test() {
        let (count, violations) = calls("(find x list :test #'equal)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_test_not() {
        // :test-not #'eql means "not eql"; not redundant.
        let (_, violations) = calls("(remove x list :test-not #'eql)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_eql_defaulting_head() {
        // sort takes a required predicate, not an eql-defaulting :test.
        let (count, violations) = calls("(sort xs #'< :key #'eql)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_eql_as_a_search_item() {
        // Here :test is the item being searched for, not a keyword.
        let (_, violations) = calls("(find :test plist)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(FIND x list :test #'eql)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (member y xs :test #'eql) (go))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(find x list :test #'eql)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_redundant_eql_tests(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant eql tests");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(find x list :test #'eql)");
        let summary = summarize_redundant_eql_tests(count, items);

        let quiet =
            evaluate_redundant_eql_test_policy(RedundantEqlTestPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_redundant_eql_test_policy(RedundantEqlTestPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
