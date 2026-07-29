//! Common Lisp redundant-`:test`-`eql` detection: a call to one of the standard
//! eql-defaulting sequence/list operators with an explicit `:test #'eql`. For
//! these operators CLHS specifies that the `:test` argument *defaults to* `eql`,
//! so `(find x list :test #'eql)` is exactly `(find x list)` — the explicit test
//! restates the default and adds only noise.
//!
//! Scope is gated to the operators whose `:test` is documented to default to
//! `eql` (`EQL_TEST_HEADS` — `member`, `assoc`, `find`, `position`, `count`,
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
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

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
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The span to delete: the ` :test #'eql` argument pair, from the end of the
    /// preceding argument through the eql designator.
    ///
    /// The rewrite's input, not the report's: the lint rule deletes it, and
    /// unlike its `:start`/`:end`/`:count`/`:from-end` siblings this command
    /// has never printed it.
    pub removal_span: ByteSpan,
    /// The operator name, as spelled at the call site.
    pub head: String,
}

impl Finding for RedundantEqlTestItem {
    /// The rule's own name. The operator varies per finding, but it is a
    /// source-cased `String` off the call site rather than a canonical tag, so
    /// it stays data in `head` and the kind names the rule.
    fn kind(&self) -> &'static str {
        "redundant-eql-test"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.head.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("head", json!(self.head))]
    }

    /// The same sentence the `redundant-eql-test` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} defaults :test to eql; the explicit :test #'eql is redundant",
            self.head
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_call(
    view: &ExpressionView,
    source: &str,
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
            span: view.span,
            line: line_of(source, view.span.start().get()),
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every eql-defaulting call with a redundant explicit `:test #'eql`
/// in one file, with the number of such calls scanned as the denominator
/// beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant `:test #'eql` here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_redundant_eql_test_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantEqlTestItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("call_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_call(subview, source, &mut call_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("call_form_count", json!(call_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantEqlTestItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_eql_test_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant eql test report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<RedundantEqlTestItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "call_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("call_form_count in the summary");
        (count, report.findings)
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(find x list :test #'eql)", Dialect::Clojure)
            .expect("parse");
        let report = build_redundant_eql_test_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant eql test report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(find x list)").dialect_modelled);
    }

    /// `removal_span` stays off the report: unlike its sibling rules, this
    /// one's JSON never carried it.
    #[test]
    fn a_finding_carries_its_line_and_its_head_but_not_the_removal_span() {
        let report = report("(defun f (x list)\n  (find x list :test #'eql))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "redundant-eql-test");
        assert_eq!(finding.text_columns(), vec!["find".to_owned()]);
        assert_eq!(finding.json_fields(), vec![("head", json!("find"))]);
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report =
            report("(find x list :test #'eql)\n(member y ys)\n(count z zs :test #'equal)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
