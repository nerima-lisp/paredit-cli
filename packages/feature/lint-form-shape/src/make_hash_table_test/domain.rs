//! Common Lisp `make-hash-table`-redundant-`:test`-`eql` detection: a
//! `(make-hash-table … :test 'eql …)`. The `:test` argument of `make-hash-table`
//! *defaults to* `eql`, so an explicit `:test 'eql`/`#'eql` restates the default
//! and can be dropped with no behavioral change.
//!
//! The three eql designators are recognized: `#'eql`, `'eql`, and the explicit
//! `(function eql)` / `(quote eql)` list. Any other test (`'equal`, `'equalp`,
//! `'eq`) is a real choice and left alone. The pair may sit anywhere in the
//! keyword arguments; the fix deletes just the ` :test 'eql` pair (from the end
//! of the preceding argument through the eql designator), leaving other keyword
//! arguments byte-identical, so the rule is auto-fixable.
//!
//! This is the `make-hash-table` sibling of
//! `redundant-eql-test` (which covers the sequence/list
//! operators whose `:test` defaults to `eql`).
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

/// Whether `view` designates the `eql` function: `#'eql`, `'eql`, or the
/// explicit `(function eql)` / `(quote eql)` list.
fn is_eql_designator(view: &ExpressionView) -> bool {
    if let Some(text) = atom_text(view) {
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

/// Whether `view` is the `:test` keyword atom.
fn is_test_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":test"))
}

#[derive(Debug, Clone)]
pub struct MakeHashTableTestItem {
    /// The span of the whole `make-hash-table` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span to delete: the ` :test 'eql` argument pair.
    ///
    /// Both the fix's input and part of the report: an agent that wants to
    /// perform the deletion itself needs the exact bytes, and the old report
    /// published them.
    pub removal_span: ByteSpan,
}

impl Finding for MakeHashTableTestItem {
    /// The rule's own name. Every finding here is the same defect — an explicit
    /// `:test 'eql` — with nothing to sub-divide it by.
    fn kind(&self) -> &'static str {
        "make-hash-table-test"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "removal_span",
            json!({
                "start": self.removal_span.start().get(),
                "end": self.removal_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `make-hash-table-test` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "the make-hash-table :test defaults to eql; drop the explicit :test 'eql".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    make_hash_table_form_count: &mut usize,
    violations: &mut Vec<MakeHashTableTestItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("make-hash-table") {
        return;
    }
    *make_hash_table_form_count += 1;

    // Scan the keyword arguments for a `:test` immediately followed by an `eql`
    // designator. `children[0]` is the operator.
    for index in 1..view.children.len().saturating_sub(1) {
        if !is_test_keyword(&view.children[index]) {
            continue;
        }
        let value = &view.children[index + 1];
        if !is_eql_designator(value) {
            continue;
        }
        let removal_span = ByteSpan::new(view.children[index - 1].span.end(), value.span.end());
        violations.push(MakeHashTableTestItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            removal_span,
        });
        return;
    }
}

/// Collects every `(make-hash-table … :test 'eql …)` in one file, with the
/// number of `make-hash-table` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant `:test`" for Common Lisp and
/// "nothing was looked for" for Clojure, and the two read identically without
/// the flag.
pub fn build_make_hash_table_test_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MakeHashTableTestItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("make_hash_table_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut make_hash_table_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                source,
                &mut make_hash_table_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![(
            "make_hash_table_form_count",
            json!(make_hash_table_form_count),
        )],
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

    fn report(input: &str) -> FileFindings<MakeHashTableTestItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_make_hash_table_test_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build make-hash-table test report")
    }

    /// The `(make_hash_table_form_count, violations)` pair the report is built
    /// from.
    fn calls(input: &str) -> (u64, Vec<MakeHashTableTestItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "make_hash_table_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("make_hash_table_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_quoted_eql_test() {
        let source = "(make-hash-table :test 'eql)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :test 'eql");
    }

    #[test]
    fn flags_sharp_quoted_eql_test() {
        let (_, violations) = calls("(make-hash-table :test #'eql)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(make-hash-table :size 16 :test 'eql)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :test 'eql");
    }

    #[test]
    fn does_not_flag_a_custom_test() {
        assert!(calls("(make-hash-table :test 'equal)").1.is_empty());
        assert!(calls("(make-hash-table :test #'equalp)").1.is_empty());
    }

    #[test]
    fn does_not_flag_bare_make_hash_table() {
        let (count, violations) = calls("(make-hash-table)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(MAKE-HASH-TABLE :test 'eql)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = calls("(defun f () (make-hash-table :test 'eql))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(make-hash-table :test 'eql)", Dialect::Clojure)
            .expect("parse");
        let report =
            build_make_hash_table_test_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build make-hash-table test report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("make_hash_table_form_count", json!(0))]
        );
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(make-hash-table)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_removal_span() {
        let report = report("(defun f ()\n  (make-hash-table :test 'eql))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "make-hash-table-test");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "removal_span",
                json!({
                    "start": finding.removal_span.start().get(),
                    "end": finding.removal_span.end().get(),
                }),
            )]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(make-hash-table)\n(make-hash-table :test 'equal)\n(make-hash-table :test 'eql)\n",
        );
        assert_eq!(
            report.summary,
            vec![("make_hash_table_form_count", json!(3))]
        );
        assert_eq!(report.findings.len(), 1);
    }
}
