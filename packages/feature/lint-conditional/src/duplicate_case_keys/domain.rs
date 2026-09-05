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
use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

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
    /// The span of the whole `case`/`ecase`/`ccase` form.
    pub span: ByteSpan,
    /// The case operator as it is spelled in source.
    pub head: String,
    /// The repeated key, in its first-seen spelling.
    pub key: String,
    /// How many clauses match it.
    pub occurrence_count: usize,
}

impl Finding for DuplicateCaseKeyItem {
    /// The rule's own name. The three `eql`-comparing operators dispatch
    /// identically, so a repeated key is the same dead clause in each; the
    /// operator that carried it stays in the JSON rather than splitting the
    /// kind.
    fn kind(&self) -> &'static str {
        "duplicate-case-keys"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("head={}", self.head),
            format!("key={}", self.key),
            format!("count={}", self.occurrence_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("key", json!(self.key)),
            ("occurrence_count", json!(self.occurrence_count)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} repeats key {} ({}×)",
            self.head, self.key, self.occurrence_count
        )
    }
}

pub fn examine_case(
    view: &ExpressionView,
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
            span: view.span,
            head: head.to_owned(),
            key: key.clone(),
            occurrence_count: *occurrence_count,
        });
    }
}

/// Collects every duplicated `case`/`ecase`/`ccase` key in one file, with the
/// number of such forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_duplicate_case_key_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DuplicateCaseKeyItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("case_form_count", json!(0))],
        ));
    }

    let mut case_form_count = 0;
    let mut duplicates = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, &mut case_form_count, &mut duplicates);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        duplicates,
        vec![("case_form_count", json!(case_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DuplicateCaseKeyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_duplicate_case_key_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build duplicate case key report")
    }

    fn duplicates(input: &str) -> (u64, Vec<DuplicateCaseKeyItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "case_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("case_form_count in the summary");
        (count, report.findings)
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
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(case x (:a 1) (:a 2))", Dialect::Clojure)
            .expect("parse input");
        let report = build_duplicate_case_key_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build duplicate case key report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(case x (:a 1))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_key_and_its_count() {
        let report = report("(defun f (x)\n  (case x (:a 1) (:b 2) (:a 3)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "duplicate-case-keys");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head", json!("case")),
                ("key", json!(":a")),
                ("occurrence_count", json!(2)),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "head=case".to_owned(),
                "key=:a".to_owned(),
                "count=2".to_owned()
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_case_scanned_not_only_the_flagged_ones() {
        let report = report("(case x (:a 1) (:a 2))\n(case y (:b 1))\n");
        assert_eq!(report.summary, vec![("case_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
