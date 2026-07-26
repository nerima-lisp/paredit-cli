//! Common Lisp duplicate-`case`-key detection: a `case`, `ecase`, or
//! `ccase` form in which the same key appears in two different clauses.
//! `case` dispatches to the *first* clause whose key matches, so a key
//! repeated in a later clause makes that clause dead code — almost always a
//! copy-paste error or a mistaken key, never intentional.
//!
//! Unlike the other reports in this tool, a `case` form is not a top-level
//! declaration — it can appear anywhere in a function body — so this report
//! walks the *whole* expression tree rather than only its root children.
//!
//! Scope: Common Lisp only, and only the three `eql`-comparing conditionals
//! (`case`/`ecase`/`ccase`). The type-testing `typecase` family is
//! excluded: its clause heads are type specifiers, not `eql` keys, and two
//! syntactically different specifiers can still denote overlapping types —
//! a soundness question a purely syntactic key comparison cannot answer.
//! Keys are compared by ASCII-case-folded literal text, so `foo` and `FOO`
//! (the same symbol after the reader folds case) collide, while `:foo` (a
//! keyword) stays distinct from `foo` (a symbol) and from `"foo"` (a
//! string), matching `eql`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

const CASE_HEADS: [&str; 3] = ["case", "ecase", "ccase"];

/// A `case` key is compared with `eql`, which for symbols is case-folded
/// (the reader upcases unescaped symbol characters) but keeps a keyword,
/// symbol, and string of the same spelling distinct. ASCII-upcasing the raw
/// atom text — colon prefix and quotes included — reproduces exactly those
/// distinctions for the literal keys `case` clauses actually use.
fn case_key_needle(text: &str) -> String {
    text.to_ascii_uppercase()
}

/// Reads the literal keys a single `case` clause matches. Returns an empty
/// list for the default clause (`t` or `otherwise` as a bare key head), and
/// for a clause whose key head is a list, every atom in that list.
fn clause_keys(clause: &ExpressionView) -> Vec<&str> {
    let Some(key_head) = clause.children.first() else {
        return Vec::new();
    };

    if let Some(text) = atom_text(key_head) {
        if text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("otherwise") {
            return Vec::new();
        }
        return vec![text];
    }

    if is_paren_list(key_head) {
        return key_head.children.iter().filter_map(atom_text).collect();
    }

    Vec::new()
}

#[derive(Debug, Clone)]
pub struct DuplicateCaseKeyItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub key: String,
    pub occurrence_count: usize,
}

#[derive(Debug)]
pub struct DuplicateCaseKeySummary {
    pub case_form_count: usize,
    pub duplicates: Vec<DuplicateCaseKeyItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateCaseKeyPolicyOptions {
    fail_on_duplicate: bool,
}

impl DuplicateCaseKeyPolicyOptions {
    #[must_use]
    pub fn new(fail_on_duplicate: bool) -> Self {
        Self { fail_on_duplicate }
    }

    #[must_use]
    pub const fn fail_on_duplicate(self) -> bool {
        self.fail_on_duplicate
    }
}

#[derive(Debug)]
pub struct DuplicateCaseKeyPolicy {
    pub fail_on_duplicate: bool,
    pub case_form_count: usize,
    pub duplicate_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_case(
    view: &ExpressionView,
    path: &Path,
    case_form_count: &mut usize,
    duplicates: &mut Vec<DuplicateCaseKeyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !CASE_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *case_form_count += 1;

    // Preserve first-seen spelling and clause order while counting.
    let mut order: Vec<String> = Vec::new();
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for clause in view.children.iter().skip(2) {
        for key in clause_keys(clause) {
            let needle = case_key_needle(key);
            let entry = counts.entry(needle.clone()).or_insert_with(|| {
                order.push(needle);
                (key.to_owned(), 0)
            });
            entry.1 += 1;
        }
    }

    for needle in order {
        let (key, occurrence_count) = &counts[&needle];
        if *occurrence_count < 2 {
            continue;
        }
        duplicates.push(DuplicateCaseKeyItem {
            path: path.to_path_buf(),
            span: view.span,
            head: head.to_owned(),
            key: key.clone(),
            occurrence_count: *occurrence_count,
        });
    }
}

/// Collects every duplicated `case`/`ecase`/`ccase` key across a whole file,
/// along with the total number of such forms scanned.
pub fn collect_duplicate_case_keys(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateCaseKeyItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut case_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, path, &mut case_form_count, &mut duplicates);
        });
    }
    Ok((case_form_count, duplicates))
}

#[must_use]
pub fn summarize_duplicate_case_keys(
    case_form_count: usize,
    duplicates: Vec<DuplicateCaseKeyItem>,
) -> DuplicateCaseKeySummary {
    DuplicateCaseKeySummary {
        case_form_count,
        duplicates,
    }
}

#[must_use]
pub fn evaluate_duplicate_case_key_policy(
    options: DuplicateCaseKeyPolicyOptions,
    summary: &DuplicateCaseKeySummary,
) -> DuplicateCaseKeyPolicy {
    let duplicate_count = summary.duplicates.len();
    let mut violations = Vec::new();
    if options.fail_on_duplicate() && duplicate_count > 0 {
        violations.push(format!("duplicate_count {duplicate_count} exceeds 0"));
    }

    DuplicateCaseKeyPolicy {
        fail_on_duplicate: options.fail_on_duplicate(),
        case_form_count: summary.case_form_count,
        duplicate_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duplicates(input: &str) -> (usize, Vec<DuplicateCaseKeyItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_duplicate_case_keys(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate case keys")
    }

    #[test]
    fn flags_a_key_repeated_across_two_clauses() {
        let (case_form_count, duplicates) = duplicates("(case x (:a 1) (:b 2) (:a 3))");
        assert_eq!(case_form_count, 1);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].key, ":a");
        assert_eq!(duplicates[0].occurrence_count, 2);
        assert_eq!(duplicates[0].head, "case");
    }

    #[test]
    fn flags_a_key_repeated_inside_a_key_list() {
        let (_, duplicates) = duplicates("(case x ((:a :b) 1) (:a 2))");
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].key, ":a");
    }

    #[test]
    fn folds_symbol_case_like_the_reader() {
        let (_, duplicates) = duplicates("(ecase x (foo 1) (FOO 2))");
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn keeps_a_keyword_distinct_from_a_symbol_of_the_same_name() {
        let (_, duplicates) = duplicates("(case x (:a 1) (a 2))");
        assert!(duplicates.is_empty());
    }

    #[test]
    fn does_not_flag_the_otherwise_default_clause() {
        let (_, duplicates) = duplicates("(case x (:a 1) (t 2))");
        assert!(duplicates.is_empty());
    }

    #[test]
    fn finds_a_case_nested_inside_a_function_body() {
        let (case_form_count, duplicates) =
            duplicates("(defun f (x) (progn (case x (:a 1) (:a 2))))");
        assert_eq!(case_form_count, 1);
        assert_eq!(duplicates.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_keys() {
        let (case_form_count, duplicates) = duplicates("(case x (:a 1) (:b 2) (:c 3))");
        assert_eq!(case_form_count, 1);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(case x (:a 1) (:a 2))").expect("parse input");
        let (case_form_count, duplicates) =
            collect_duplicate_case_keys(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate case keys");
        assert_eq!(case_form_count, 0);
        assert!(duplicates.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (case_form_count, dups) = duplicates("(case x (:a 1) (:a 2))");
        let summary = summarize_duplicate_case_keys(case_form_count, dups);

        let quiet =
            evaluate_duplicate_case_key_policy(DuplicateCaseKeyPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.duplicate_count, 1);

        let strict =
            evaluate_duplicate_case_key_policy(DuplicateCaseKeyPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
