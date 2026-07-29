//! Common Lisp duplicate-lambda-list-keyword detection: a callable
//! definition whose lambda list names the same lambda-list keyword
//! (`&optional`, `&rest`, `&key`, `&aux`, `&body`, `&allow-other-keys`,
//! `&whole`, `&environment`, …) more than once — `(defun f (&optional x
//! &optional y) …)`. Each lambda-list keyword may appear at most once in a
//! lambda list, so a repeat is always a program error, caught at
//! macroexpansion rather than by the reader.
//!
//! Only the *direct* tokens of the lambda list are inspected, so a repeated
//! keyword inside a `defmacro` destructuring sublist (a nested pattern with
//! its own keywords) is not confused with a top-level repeat.
//!
//! Reuses the callable-definition recognizer
//! [`paredit_core_syntax::definition::definition_shape`] — the same one
//! [`crate::duplicate_parameters::domain`] uses — so `defun`,
//! `defmacro`, `defmethod`, and the other callable definers are all covered.
//!
//! Scope: Common Lisp only.

use std::collections::BTreeMap;
use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head};
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub struct DuplicateLambdaListKeywordItem {
    pub span: ByteSpan,
    /// The 1-based line the definition starts on.
    pub line: usize,
    pub definition: String,
    pub keyword: String,
    pub occurrence_count: usize,
}

impl Finding for DuplicateLambdaListKeywordItem {
    /// The rule's own name, not the keyword.
    ///
    /// The keyword is the source's own spelling of an `&`-token, so it is
    /// neither `&'static str` nor identifier-like; it stays a text column and a
    /// JSON field instead.
    fn kind(&self) -> &'static str {
        "duplicate-lambda-list-keyword"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("definition={}", self.definition),
            format!("keyword={}", self.keyword),
            format!("occurrences={}", self.occurrence_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("definition", json!(self.definition)),
            ("keyword", json!(self.keyword)),
            ("occurrence_count", json!(self.occurrence_count)),
        ]
    }

    /// The same sentence the `duplicate-lambda-list-keyword` lint rule writes,
    /// so a SARIF or JUnit consumer reading both sees one defect described one
    /// way.
    fn message(&self) -> String {
        format!(
            "{} repeats lambda-list keyword {} ({}×)",
            self.definition, self.keyword, self.occurrence_count
        )
    }
}

/// Collects every callable definition in one file whose lambda list repeats a
/// lambda-list keyword, with the number of callable definitions scanned as the
/// denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no repeated keyword here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_duplicate_lambda_list_keyword_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateLambdaListKeywordItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("definition_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut definition_count = 0;
    let mut duplicates = Vec::new();

    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        let Some(head) = list_head(&view) else {
            continue;
        };
        let Some(shape) = definition_shape(dialect, &view, head) else {
            continue;
        };
        if !shape.category.is_callable() {
            continue;
        }
        let Some(lambda_list) = shape.lambda_list(&view) else {
            continue;
        };
        definition_count += 1;

        let definition = shape.name(&view).unwrap_or("?").to_owned();

        // Count the `&`-keyword tokens at the top level of the lambda list.
        // Nested destructuring sublists are list children, so `atom_text`
        // returns `None` for them and they are naturally skipped.
        let mut order: Vec<String> = Vec::new();
        let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for child in &lambda_list.children {
            let Some(text) = atom_text(child) else {
                continue;
            };
            if !text.starts_with('&') || text.len() < 2 {
                continue;
            }
            let needle = text.to_ascii_uppercase();
            let entry = counts.entry(needle.clone()).or_insert_with(|| {
                order.push(needle);
                (text.to_owned(), 0)
            });
            entry.1 += 1;
        }

        for needle in order {
            let (keyword, occurrence_count) = &counts[&needle];
            if *occurrence_count < 2 {
                continue;
            }
            duplicates.push(DuplicateLambdaListKeywordItem {
                span: view.span,
                line: line_of(source, view.span.start().get()),
                definition: definition.clone(),
                keyword: keyword.clone(),
                occurrence_count: *occurrence_count,
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        duplicates,
        vec![("definition_count", json!(definition_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateLambdaListKeywordItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_duplicate_lambda_list_keyword_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build duplicate lambda list keyword report")
    }

    /// The `(definition_count, duplicates)` pair the report is built from.
    fn duplicates(input: &str) -> (u64, Vec<DuplicateLambdaListKeywordItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "definition_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("definition_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_repeated_optional_keyword() {
        let (definition_count, duplicates) = duplicates("(defun f (&optional x &optional y) x)");
        assert_eq!(definition_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].keyword, "&optional");
        assert_eq!(duplicates[0].occurrence_count, 2);
    }

    #[test]
    fn flags_a_repeated_key_keyword() {
        let (_, duplicates) = duplicates("(defun f (a &key b &key c) a)");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].keyword, "&key");
    }

    #[test]
    fn folds_keyword_case() {
        let (_, duplicates) = duplicates("(defun f (&OPTIONAL x &optional y) x)");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_keywords() {
        let (definition_count, duplicates) =
            duplicates("(defun f (a &optional b &rest c &key d) a)");
        assert_eq!(definition_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_flag_a_keyword_inside_a_destructuring_sublist() {
        // Outer `&optional` appears once; the inner `&optional` is inside a
        // destructuring sublist and is a different lambda list.
        let (_, duplicates) = duplicates("(defmacro m ((a &optional b) &optional c) a)");
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_flag_a_keywordless_lambda_list() {
        let (definition_count, duplicates) = duplicates("(defun f (a b c) a)");
        assert_eq!(definition_count, 1);
        assert!(duplicates.is_empty());
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(defun f (&optional x &optional y) x)",
            Dialect::Clojure,
        )
        .expect("parse input");
        let report = build_duplicate_lambda_list_keyword_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build duplicate lambda list keyword report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("definition_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(defun f (a b) a)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_columns() {
        let report = report("(defun g (a) a)\n(defun f (&key b &key c) b)\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "duplicate-lambda-list-keyword");
        assert_eq!(
            finding.text_columns(),
            vec![
                "definition=f".to_owned(),
                "keyword=&key".to_owned(),
                "occurrences=2".to_owned(),
            ]
        );
        assert_eq!(
            finding.json_fields(),
            vec![
                ("definition", json!("f")),
                ("keyword", json!("&key")),
                ("occurrence_count", json!(2)),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_definition_scanned_not_only_the_flagged_ones() {
        let report = report("(defun f (&key a &key b) a)\n(defun g (c) c)\n");
        assert_eq!(report.summary, vec![("definition_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
