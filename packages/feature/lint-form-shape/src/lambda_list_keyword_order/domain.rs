//! Common Lisp lambda-list-keyword-order detection: a callable definition
//! whose lambda list lists its keywords out of the standard order. An ordinary
//! lambda list must present its keywords as `&optional`, then `&rest`, then
//! `&key`, then `&allow-other-keys`, then `&aux` — e.g. `(defun f (&key a
//! &optional b) …)` is malformed because `&optional` follows `&key`. A
//! misordered lambda list is a program error, caught at macroexpansion rather
//! than by the reader.
//!
//! Only a strict order *decrease* is reported (a keyword whose rank is lower
//! than one already seen). A repeated keyword — equal rank — is left to
//! [`crate::duplicate_lambda_list_keyword::domain`] so the two rules do
//! not both flag the same span.
//!
//! Any lambda list containing a keyword this rule does not rank — `&whole`,
//! `&environment`, or `&body`, whose macro-lambda-list position rules are more
//! subtle — is skipped wholesale, keeping the ordering claim airtight for the
//! ordinary keywords.
//!
//! Reuses the callable-definition recognizer
//! [`paredit_core_syntax::definition::definition_shape`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::definition::definition_shape;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, list_head};
use serde_json::{Value, json};

/// The canonical rank of an ordinary lambda-list keyword, or `None` for a
/// keyword whose position this rule does not rank (`&whole`, `&environment`,
/// `&body`, or any nonstandard `&`-marker).
fn keyword_rank(text: &str) -> Option<u8> {
    match text.to_ascii_uppercase().as_str() {
        "&OPTIONAL" => Some(1),
        "&REST" => Some(2),
        "&KEY" => Some(3),
        "&ALLOW-OTHER-KEYS" => Some(4),
        "&AUX" => Some(5),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct LambdaListKeywordOrderItem {
    pub span: ByteSpan,
    pub definition: String,
    pub keyword: String,
    pub after_keyword: String,
}

impl Finding for LambdaListKeywordOrderItem {
    /// The rule's name, not the misplaced keyword: `keyword` is source text
    /// carried per finding, and `kind` is a fixed vocabulary the interop
    /// formats turn into a rule id. It stays a `json_fields` entry, where
    /// filtering on it still works.
    fn kind(&self) -> &'static str {
        "lambda-list-keyword-order"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("definition={}", self.definition),
            format!("keyword={}", self.keyword),
            format!("after={}", self.after_keyword),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("definition", json!(self.definition)),
            ("keyword", json!(self.keyword)),
            ("after_keyword", json!(self.after_keyword)),
        ]
    }

    /// The same sentence the `lambda-list-keyword-order` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} lists lambda-list keyword {} after {}",
            self.definition, self.keyword, self.after_keyword
        )
    }
}

/// Collects every callable definition in one file whose lambda-list keywords
/// are out of order, with the number of callable definitions scanned as the
/// denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no misordered lambda list here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_lambda_list_keyword_order_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LambdaListKeywordOrderItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("definition_count", json!(0))],
        ));
    }

    let mut definition_count = 0;
    let mut violations = Vec::new();

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

        // Collect the top-level `&`-keywords in order. A keyword this rule does
        // not rank makes the lambda list unclassifiable, so skip it entirely.
        let mut ranked: Vec<(String, u8)> = Vec::new();
        let mut unrankable = false;
        for child in &lambda_list.children {
            let Some(text) = atom_text(child) else {
                continue;
            };
            if !text.starts_with('&') || text.len() < 2 {
                continue;
            }
            match keyword_rank(text) {
                Some(rank) => ranked.push((text.to_owned(), rank)),
                None => {
                    unrankable = true;
                    break;
                }
            }
        }
        if unrankable {
            continue;
        }

        let definition = shape.name(&view).unwrap_or("?").to_owned();

        // Report the first keyword whose rank strictly decreases from the
        // highest rank seen so far; equal ranks (duplicates) are not our
        // concern.
        let mut max_rank = 0;
        let mut max_keyword = String::new();
        for (keyword, rank) in &ranked {
            if *rank < max_rank {
                violations.push(LambdaListKeywordOrderItem {
                    span: view.span,
                    definition: definition.clone(),
                    keyword: keyword.clone(),
                    after_keyword: max_keyword.clone(),
                });
                break;
            }
            if *rank > max_rank {
                max_rank = *rank;
                max_keyword = keyword.clone();
            }
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("definition_count", json!(definition_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<LambdaListKeywordOrderItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_lambda_list_keyword_order_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build lambda list keyword order report")
    }

    /// The `(definition_count, violations)` pair the report is built from.
    fn violations(input: &str) -> (u64, Vec<LambdaListKeywordOrderItem>) {
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
    fn flags_key_before_optional() {
        let (definition_count, items) = violations("(defun f (&key a &optional b) a)");
        assert_eq!(definition_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].keyword, "&optional");
        assert_eq!(items[0].after_keyword, "&key");
    }

    #[test]
    fn flags_rest_before_optional() {
        let (_, items) = violations("(defun f (&rest r &optional o) r)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].keyword, "&optional");
    }

    #[test]
    fn flags_aux_before_key() {
        let (_, items) = violations("(defun f (&aux (x 1) &key k) x)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].keyword, "&key");
    }

    #[test]
    fn does_not_flag_canonical_order() {
        let (definition_count, items) =
            violations("(defun f (a &optional b &rest c &key d &allow-other-keys &aux (e 1)) a)");
        assert_eq!(definition_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_keywordless_lambda_list() {
        let (_, items) = violations("(defun f (a b c) a)");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_pure_duplicate() {
        // Equal ranks are a duplicate-keyword issue, not an ordering one.
        let (_, items) = violations("(defun f (&optional a &optional b) a)");
        assert!(items.is_empty());
    }

    #[test]
    fn folds_keyword_case() {
        let (_, items) = violations("(defun f (&KEY a &optional b) a)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn skips_a_lambda_list_with_body() {
        // `&body` is not ranked, so the whole lambda list is skipped.
        let (definition_count, items) = violations("(defmacro m (&body b &optional o) b)");
        assert_eq!(definition_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_lambda_list_with_environment() {
        let (_, items) = violations("(defmacro m (&key k &environment e &optional o) k)");
        assert!(items.is_empty());
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(defun f (&key a &optional b) a)", Dialect::Clojure)
                .expect("parse input");
        let report =
            build_lambda_list_keyword_order_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build lambda list keyword order report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("definition_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(defun f (a b) a)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_definition_and_both_keywords() {
        let report = report("(in-package :app)\n(defun f (&key a &optional b) a)\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "lambda-list-keyword-order");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("definition", json!("f")),
                ("keyword", json!("&optional")),
                ("after_keyword", json!("&key")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "definition=f".to_owned(),
                "keyword=&optional".to_owned(),
                "after=&key".to_owned(),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_definition_scanned_not_only_the_flagged_ones() {
        let report = report("(defun f (&key a &optional b) a)\n(defun g (a b) a)\n");
        assert_eq!(report.summary, vec![("definition_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
