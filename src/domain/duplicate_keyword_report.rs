//! Common Lisp duplicate-keyword-argument detection: a call that passes the same
//! keyword argument twice, e.g. `(make-instance 'c :x 1 :x 2)`. When a keyword
//! appears more than once in a keyword-argument list, the *leftmost* value wins
//! and the rest are silently ignored — almost always a copy-paste bug. This is
//! report-only (which duplicate the author meant to keep is ambiguous).
//!
//! To stay free of false positives without a full arglist model, scope is gated
//! to operators with a *fixed, known* positional-argument prefix
//! ([`KEY_OPERATORS`] — the `make-*` constructors), so the keyword plist begins
//! at a known index and a positional argument that happens to be a keyword can
//! never be mistaken for a keyword-argument name. Within the plist, keywords sit
//! at even offsets; a name repeated across those offsets is flagged. A malformed
//! (odd-length) plist or a non-keyword in a name slot leaves the form alone.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

/// Operators whose keyword plist starts after a fixed number of positional
/// arguments, so a repeated keyword is unambiguously a duplicate.
const KEY_OPERATORS: [(&str, usize); 7] = [
    ("make-instance", 1),
    ("make-hash-table", 0),
    ("make-array", 1),
    ("make-string", 1),
    ("make-condition", 1),
    ("make-pathname", 0),
    ("make-string-output-stream", 0),
];

/// The keyword name (e.g. `:x`) if `view` is a bare keyword literal, lowercased.
fn keyword_name(view: &ExpressionView) -> Option<String> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(view)?;
    if text.starts_with(':') && text.len() >= 2 {
        Some(text.to_ascii_lowercase())
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct DuplicateKeywordItem {
    pub path: PathBuf,
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The duplicated keyword name, lowercased.
    pub keyword: String,
    /// The span of the duplicate (second) occurrence.
    pub duplicate_span: ByteSpan,
}

#[derive(Debug)]
pub struct DuplicateKeywordSummary {
    pub call_form_count: usize,
    pub violations: Vec<DuplicateKeywordItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateKeywordPolicyOptions {
    fail_on_violation: bool,
}

impl DuplicateKeywordPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct DuplicateKeywordPolicy {
    pub fail_on_violation: bool,
    pub call_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine(
    view: &ExpressionView,
    path: &Path,
    call_form_count: &mut usize,
    violations: &mut Vec<DuplicateKeywordItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let lower = head.to_ascii_lowercase();
    let Some(&(_, positional)) = KEY_OPERATORS.iter().find(|(op, _)| *op == lower) else {
        return;
    };
    *call_form_count += 1;

    // The keyword plist begins after the operator and the fixed positionals.
    let plist_start = 1 + positional;
    if view.children.len() <= plist_start {
        return;
    }
    let plist_len = view.children.len() - plist_start;
    // A well-formed keyword plist has an even number of elements.
    if plist_len % 2 != 0 {
        return;
    }

    let mut seen: Vec<String> = Vec::new();
    let mut index = plist_start;
    while index < view.children.len() {
        let Some(name) = keyword_name(&view.children[index]) else {
            // A non-keyword in a name slot means this is not the plist we model;
            // bail rather than risk a false positive.
            return;
        };
        if seen.contains(&name) {
            violations.push(DuplicateKeywordItem {
                path: path.to_path_buf(),
                span: view.span,
                keyword: name,
                duplicate_span: view.children[index].span,
            });
            return;
        }
        seen.push(name);
        index += 2;
    }
}

/// Collects every call in [`KEY_OPERATORS`] passing a duplicate keyword argument
/// across a whole file, along with the total number of such calls scanned.
pub fn collect_duplicate_keywords(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DuplicateKeywordItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, path, &mut call_form_count, &mut violations)
        });
    }
    Ok((call_form_count, violations))
}

pub fn summarize_duplicate_keywords(
    call_form_count: usize,
    violations: Vec<DuplicateKeywordItem>,
) -> DuplicateKeywordSummary {
    DuplicateKeywordSummary {
        call_form_count,
        violations,
    }
}

pub fn evaluate_duplicate_keyword_policy(
    options: DuplicateKeywordPolicyOptions,
    summary: &DuplicateKeywordSummary,
) -> DuplicateKeywordPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    DuplicateKeywordPolicy {
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

    fn calls(input: &str) -> (usize, Vec<DuplicateKeywordItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_duplicate_keywords(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect duplicate keywords")
    }

    #[test]
    fn flags_duplicate_initarg() {
        let (count, violations) = calls("(make-instance 'c :x 1 :x 2)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].keyword, ":x");
    }

    #[test]
    fn flags_duplicate_on_zero_positional_operator() {
        let (_, violations) = calls("(make-hash-table :test 'equal :test 'eq)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].keyword, ":test");
    }

    #[test]
    fn does_not_flag_distinct_keywords() {
        let (count, violations) = calls("(make-instance 'c :x 1 :y 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_mistake_a_keyword_value_for_a_name() {
        // :x's value is the keyword :y; the real names are :x and :z (distinct).
        let (_, violations) = calls("(make-instance 'c :x :y :z :y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_allowlisted_operator() {
        // list keywords are data, not keyword arguments.
        let (count, violations) = calls("(list :x 1 :x 2)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_odd_plist() {
        assert!(calls("(make-hash-table :test)").1.is_empty());
    }

    #[test]
    fn case_folds_head_and_keyword() {
        let (_, violations) = calls("(MAKE-INSTANCE 'c :X 1 :x 2)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(make-instance 'c :x 1 :x 2)", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_duplicate_keywords(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect duplicate keywords");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = calls("(make-instance 'c :x 1 :x 2)");
        let summary = summarize_duplicate_keywords(count, items);

        let quiet =
            evaluate_duplicate_keyword_policy(DuplicateKeywordPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_duplicate_keyword_policy(DuplicateKeywordPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
