//! Common Lisp duplicate-keyword-argument detection: a call that passes the same
//! keyword argument twice, e.g. `(make-instance 'c :x 1 :x 2)`. When a keyword
//! appears more than once in a keyword-argument list, the *leftmost* value wins
//! and the rest are silently ignored — almost always a copy-paste bug. This is
//! report-only (which duplicate the author meant to keep is ambiguous).
//!
//! To stay free of false positives without a full arglist model, scope is gated
//! to operators with a *fixed, known* positional-argument prefix
//! (`KEY_OPERATORS` — the `make-*` constructors), so the keyword plist begins
//! at a known index and a positional argument that happens to be a keyword can
//! never be mistaken for a keyword-argument name. Within the plist, keywords sit
//! at even offsets; a name repeated across those offsets is flagged. A malformed
//! (odd-length) plist or a non-keyword in a name slot leaves the form alone.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

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
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The duplicated keyword name, lowercased.
    pub keyword: String,
    /// The span of the duplicate (second) occurrence.
    pub duplicate_span: ByteSpan,
}

impl Finding for DuplicateKeywordItem {
    /// The rule's own name, not the keyword.
    ///
    /// The keyword is read from the source and so is an open set — any
    /// initarg a program invents — while `kind` is `&'static str`. It stays a
    /// text column and a JSON field instead.
    fn kind(&self) -> &'static str {
        "duplicate-keyword"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.keyword.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("keyword", json!(self.keyword)),
            (
                "duplicate_span",
                json!({
                    "start": self.duplicate_span.start().get(),
                    "end": self.duplicate_span.end().get(),
                }),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "keyword {} is passed more than once; the leftmost value wins",
            self.keyword
        )
    }
}

pub fn examine(
    view: &ExpressionView,
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

/// Collects every call in `KEY_OPERATORS` passing a duplicate keyword argument
/// in one file, with the number of such calls scanned as the denominator beside
/// them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_duplicate_keyword_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateKeywordItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("call_form_count", json!(0))],
        ));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut call_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("call_form_count", json!(call_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateKeywordItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_duplicate_keyword_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build duplicate keyword report")
    }

    fn calls(input: &str) -> (u64, Vec<DuplicateKeywordItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "call_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("call_form_count in the summary");
        (count, report.findings)
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
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(make-instance 'c :x 1 :x 2)", Dialect::Clojure)
            .expect("parse");
        let report = build_duplicate_keyword_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build duplicate keyword report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(make-instance 'c :x 1)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_keyword() {
        let report = report("(defun f ()\n  (make-instance 'c :x 1 :x 2))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "duplicate-keyword");
        assert_eq!(finding.text_columns(), vec![":x".to_owned()]);
        assert_eq!(finding.json_fields()[0], ("keyword", json!(":x")));
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(make-instance 'c :x 1 :x 2)\n(make-hash-table :test 'eq)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
