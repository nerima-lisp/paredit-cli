//! Common Lisp `defpackage`-quoted-designator detection: a quoted or
//! sharp-quoted name inside a `defpackage` option clause that takes symbol or
//! package designators. `defpackage` is a macro that does *not* evaluate its
//! options, so `(:export 'foo)` names the two symbols `quote` and `foo` (or, in
//! most readers, exports a symbol literally spelled with a leading quote) rather
//! than `foo` — almost always a bug where the author reflexively quoted the name.
//! This is report-only: the fix (drop the quote) is mechanical but changes the
//! reader-level shape, so it is surfaced for the author to confirm.
//!
//! Scanned clauses are the ones whose entries are symbol/package designators:
//! `:export`, `:shadow`, `:intern`, `:import-from`, `:shadowing-import-from`,
//! `:use`, and `:nicknames`. Any entry carrying a `'`/`#'` reader prefix is
//! flagged. (`:size`/`:documentation` and unknown clauses are ignored.)
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// `defpackage` option keywords whose entries are symbol/package designators.
const DESIGNATOR_CLAUSES: [&str; 7] = [
    ":export",
    ":shadow",
    ":intern",
    ":import-from",
    ":shadowing-import-from",
    ":use",
    ":nicknames",
];

/// Whether `view` carries a `'` or `#'` reader prefix (a quoted designator).
fn is_quoted(view: &ExpressionView) -> bool {
    !view.reader_prefixes.is_empty()
}

#[derive(Debug, Clone)]
pub struct DefpackageQuotedItem {
    /// The span of the whole `defpackage` form.
    pub span: ByteSpan,
    /// The 1-based line the `defpackage` form starts on.
    pub line: usize,
    /// The clause keyword, lowercased (`:export`, ...).
    ///
    /// One of [`DESIGNATOR_CLAUSES`], so it is `&'static str` rather than the
    /// spelling read from the source: the set is closed and the report leads
    /// each row with it.
    pub clause: &'static str,
    /// The span of the quoted designator.
    pub designator_span: ByteSpan,
}

impl Finding for DefpackageQuotedItem {
    /// The clause the quote appears in, so `:export` and `:import-from` are
    /// separable without parsing JSON.
    ///
    /// They are different mistakes with different blast radii — a quoted
    /// `:export` name is a symbol nobody can find, a quoted `:use` package is a
    /// load-time failure — and a consumer filtering on one of them is asking a
    /// real question.
    fn kind(&self) -> &'static str {
        self.clause
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the path, line, and leading clause: the old text row
    /// carried exactly those, and the clause is now the leading `kind`.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("clause", json!(self.clause)),
            (
                "designator_span",
                json!({
                    "start": self.designator_span.start().get(),
                    "end": self.designator_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `defpackage-quoted` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one defect described one way.
    fn message(&self) -> String {
        format!(
            "defpackage does not evaluate its options; drop the quote in the {} clause",
            self.clause
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    defpackage_form_count: &mut usize,
    violations: &mut Vec<DefpackageQuotedItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("defpackage") {
        return;
    }
    *defpackage_form_count += 1;

    // children: [defpackage, name, option-clause...]. Scan each option clause.
    for clause in view.children.iter().skip(2) {
        if !is_paren_list(clause) {
            continue;
        }
        let Some(keyword) = list_head(clause) else {
            continue;
        };
        let lower = keyword.to_ascii_lowercase();
        let Some(name) = DESIGNATOR_CLAUSES
            .iter()
            .copied()
            .find(|candidate| *candidate == lower)
        else {
            continue;
        };
        // Entries follow the clause keyword; flag any quoted designator.
        for entry in clause.children.iter().skip(1) {
            if is_quoted(entry) {
                violations.push(DefpackageQuotedItem {
                    span: view.span,
                    line: line_of(source, view.span.start().get()),
                    clause: name,
                    designator_span: entry.span,
                });
            }
        }
    }
}

/// Collects every quoted designator inside a `defpackage` in one file, with the
/// number of `defpackage` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no quoted designator here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_defpackage_quoted_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefpackageQuotedItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("defpackage_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut defpackage_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut defpackage_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("defpackage_form_count", json!(defpackage_form_count))],
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

    fn report(input: &str) -> FileFindings<DefpackageQuotedItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_defpackage_quoted_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build defpackage quoted report")
    }

    /// The `(defpackage_form_count, violations)` pair the report is built from.
    fn packages(input: &str) -> (u64, Vec<DefpackageQuotedItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "defpackage_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("defpackage_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_quoted_export() {
        let source = "(defpackage :app (:export 'foo 'bar))";
        let (count, violations) = packages(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].clause, ":export");
        assert_eq!(slice(source, violations[0].designator_span), "'foo");
    }

    #[test]
    fn flags_sharp_quoted_designator() {
        let (_, violations) = packages("(defpackage :app (:use #'cl))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_quoted_import_from_symbols() {
        let (_, violations) = packages("(defpackage :app (:import-from :other 'thing))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_unquoted_designators() {
        let (count, violations) = packages("(defpackage :app (:export foo bar) (:use :cl))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_designator_clause() {
        // :size takes a number; a quoted value there is out of scope.
        let (_, violations) = packages("(defpackage :app (:size 10) (:documentation \"d\"))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head_and_clause() {
        let (_, violations) = packages("(DEFPACKAGE :app (:EXPORT 'foo))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defpackage :app (:export 'foo))", Dialect::Clojure)
                .expect("parse");
        let report = build_defpackage_quoted_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build defpackage quoted report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("defpackage_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(defpackage :app (:export foo))").dialect_modelled);
    }

    #[test]
    fn a_finding_leads_with_its_clause_and_carries_its_line() {
        let report = report("(in-package :cl-user)\n(defpackage :app (:use #'cl))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), ":use");
        assert!(finding.text_columns().is_empty());
        assert_eq!(finding.json_fields()[0], ("clause", json!(":use")));
    }

    #[test]
    fn the_summary_counts_every_defpackage_scanned_not_only_the_flagged_ones() {
        let report = report("(defpackage :a (:export 'x))\n(defpackage :b (:export y))\n");
        assert_eq!(report.summary, vec![("defpackage_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
