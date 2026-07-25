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
//! [`crate::domain::definition::definition_shape`] — the same one
//! [`crate::domain::duplicate_parameter_report`] uses — so `defun`,
//! `defmacro`, `defmethod`, and the other callable definers are all covered.
//!
//! Scope: Common Lisp only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, list_head};

#[derive(Debug, Clone)]
pub struct DuplicateLambdaListKeywordItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub definition: String,
    pub keyword: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateLambdaListKeywordSummary {
    pub definition_count: usize,
    pub duplicates: Vec<DuplicateLambdaListKeywordItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateLambdaListKeywordPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateLambdaListKeywordPolicyOptions {
    pub fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateLambdaListKeywordPolicy {
    pub fail_on_duplicate: bool,
    pub definition_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every callable definition whose lambda list repeats a lambda-list
/// keyword, along with the total number of callable definitions scanned.
pub fn collect_duplicate_lambda_list_keywords(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateLambdaListKeywordItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

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
                path: path.to_path_buf(),
                span: view.span,
                definition: definition.clone(),
                keyword: keyword.clone(),
                occurrence_count: *occurrence_count,
            });
        }
    }

    Ok((definition_count, duplicates))
}

pub fn summarize_duplicate_lambda_list_keywords(
    definition_count: usize,
    duplicates: Vec<DuplicateLambdaListKeywordItem>,
) -> DuplicateLambdaListKeywordSummary {
    DuplicateLambdaListKeywordSummary {
        definition_count,
        duplicates,
    }
}

pub fn evaluate_duplicate_lambda_list_keyword_policy(
    options: DuplicateLambdaListKeywordPolicyOptions,
    summary: &DuplicateLambdaListKeywordSummary,
) -> DuplicateLambdaListKeywordPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateLambdaListKeywordPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        definition_count: summary.definition_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateLambdaListKeywordItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_duplicate_lambda_list_keywords(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect duplicate lambda list keywords")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect(
            "(defun f (&optional x &optional y) x)",
            Dialect::Clojure,
        )
        .expect("parse input");
        let (definition_count, duplicates) = collect_duplicate_lambda_list_keywords(
            &PathBuf::from("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("collect duplicate lambda list keywords");
        assert_eq!(definition_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (definition_count, items) = duplicates("(defun f (&optional x &optional y) x)");
        let summary = summarize_duplicate_lambda_list_keywords(definition_count, items);

        let quiet = evaluate_duplicate_lambda_list_keyword_policy(
            DuplicateLambdaListKeywordPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict = evaluate_duplicate_lambda_list_keyword_policy(
            DuplicateLambdaListKeywordPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
