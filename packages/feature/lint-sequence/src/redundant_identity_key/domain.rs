//! Common Lisp redundant-`:key`-`identity` detection: a call to one of the
//! standard `:key`-taking sequence/list operators with an explicit
//! `:key #'identity` (or `:key nil`). CLHS specifies that `:key` defaults to
//! `nil`, meaning "use the element itself" — exactly what `identity` does — so
//! `(sort xs #'< :key #'identity)` is `(sort xs #'<)` and `:key nil` merely
//! restates the default.
//!
//! Scope is gated to operators documented to accept a `:key` argument
//! (`KEY_HEADS`). Note this list is *not* the same as the eql-defaulting
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

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

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
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The span to delete: the ` :key #'identity` argument pair.
    ///
    /// The rewrite's input, not the report's: the lint rule deletes it, and
    /// unlike its `:start`/`:end`/`:count`/`:from-end` siblings this command
    /// has never printed it.
    pub removal_span: ByteSpan,
    /// The operator name, as spelled at the call site.
    pub head: String,
}

impl Finding for RedundantIdentityKeyItem {
    /// The rule's own name. The operator varies per finding, but it is a
    /// source-cased `String` off the call site rather than a canonical tag, so
    /// it stays data in `head` and the kind names the rule.
    fn kind(&self) -> &'static str {
        "redundant-identity-key"
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

    /// The same sentence the `redundant-identity-key` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} defaults :key to identity; the explicit :key #'identity is redundant",
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
            span: view.span,
            line: line_of(source, view.span.start().get()),
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every `:key`-taking call with a redundant explicit
/// `:key #'identity`/`:key nil` in one file, with the number of such calls
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant `:key` here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_redundant_identity_key_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantIdentityKeyItem>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantIdentityKeyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_identity_key_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant identity key report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<RedundantIdentityKeyItem>) {
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

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(find x list :key #'identity)", Dialect::Clojure)
                .expect("parse");
        let report =
            build_redundant_identity_key_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build redundant identity key report");
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
        let report = report("(defun f (xs)\n  (sort xs #'< :key #'identity))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "redundant-identity-key");
        assert_eq!(finding.text_columns(), vec!["sort".to_owned()]);
        assert_eq!(finding.json_fields(), vec![("head", json!("sort"))]);
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report =
            report("(sort xs #'< :key #'identity)\n(find y ys)\n(count z zs :key #'car)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
