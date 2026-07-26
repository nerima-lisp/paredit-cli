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
//! [`crate::domain::duplicate_lambda_list_keyword_report`] so the two rules do
//! not both flag the same span.
//!
//! Any lambda list containing a keyword this rule does not rank — `&whole`,
//! `&environment`, or `&body`, whose macro-lambda-list position rules are more
//! subtle — is skipped wholesale, keeping the ordering claim airtight for the
//! ordinary keywords.
//!
//! Reuses the callable-definition recognizer
//! [`crate::domain::definition::definition_shape`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::definition::definition_shape;
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, list_head};

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
    pub path: PathBuf,
    pub span: ByteSpan,
    pub definition: String,
    pub keyword: String,
    pub after_keyword: String,
}

#[derive(Debug)]
pub struct LambdaListKeywordOrderSummary {
    pub definition_count: usize,
    pub violations: Vec<LambdaListKeywordOrderItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LambdaListKeywordOrderPolicyOptions {
    fail_on_violation: bool,
}

impl LambdaListKeywordOrderPolicyOptions {
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
pub struct LambdaListKeywordOrderPolicy {
    pub fail_on_violation: bool,
    pub definition_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Collects every callable definition whose lambda-list keywords are out of
/// order, along with the total number of callable definitions scanned.
pub fn collect_lambda_list_keyword_order(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<LambdaListKeywordOrderItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
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
                    path: path.to_path_buf(),
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

    Ok((definition_count, violations))
}

#[must_use]
pub const fn summarize_lambda_list_keyword_order(
    definition_count: usize,
    violations: Vec<LambdaListKeywordOrderItem>,
) -> LambdaListKeywordOrderSummary {
    LambdaListKeywordOrderSummary {
        definition_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_lambda_list_keyword_order_policy(
    options: LambdaListKeywordOrderPolicyOptions,
    summary: &LambdaListKeywordOrderSummary,
) -> LambdaListKeywordOrderPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LambdaListKeywordOrderPolicy {
        fail_on_violation: options.fail_on_violation(),
        definition_count: summary.definition_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<LambdaListKeywordOrderItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_lambda_list_keyword_order(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect lambda list keyword order")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(defun f (&key a &optional b) a)", Dialect::Clojure)
                .expect("parse input");
        let (definition_count, items) =
            collect_lambda_list_keyword_order(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect lambda list keyword order");
        assert_eq!(definition_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (definition_count, items) = violations("(defun f (&key a &optional b) a)");
        let summary = summarize_lambda_list_keyword_order(definition_count, items);

        let quiet = evaluate_lambda_list_keyword_order_policy(
            LambdaListKeywordOrderPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_lambda_list_keyword_order_policy(
            LambdaListKeywordOrderPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
