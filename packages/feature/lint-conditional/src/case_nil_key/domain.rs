//! Common Lisp `case`-`nil`-key detection: a `case`, `ccase`, or `ecase` clause
//! whose key designator is the bare atom `nil` — `(case x (nil 1) …)`. In
//! `case`, a key designator is a *designator for a list of objects*, and `nil`
//! designates the **empty** list, so the clause has no keys and can never be
//! selected. Authors almost always mean "match the value `nil`", which requires
//! the one-element key list `((nil) …)`; the bare `nil` is a silent dead clause.
//!
//! Only the bare, unquoted `nil` atom is flagged:
//!
//!   - `(nil …)`   → flagged: `nil` is the empty key list, never matches.
//!   - `((nil) …)` → correct: a key list containing the symbol `nil`.
//!   - `('nil …)`  → the `quoted-case-key` rule's concern, not this one.
//!
//! Scoped to `case`/`ccase`/`ecase` — the `eql`-key forms. `typecase`'s clause
//! heads are type specifiers, a different shape, and are not inspected here.
//! This rule does not rewrite anything (whether the clause is a typo or dead
//! vestige is the author's call), so it is report-only.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

const CASE_HEADS: [&str; 3] = ["case", "ccase", "ecase"];

/// Whether a clause's key designator is the bare, unquoted atom `nil` (the empty
/// key list). A `(nil)` key *list* and a quoted `'nil` are both excluded.
fn is_bare_nil_key(key_designator: &ExpressionView) -> bool {
    key_designator.reader_prefixes.is_empty()
        && atom_text(key_designator).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct CaseNilKeyItem {
    /// The span of the offending `nil` key designator.
    pub span: ByteSpan,
    /// The case operator (`case`/`ccase`/`ecase`), for the finding message.
    pub head: String,
}

impl Finding for CaseNilKeyItem {
    /// The rule's own name. There is one defect here, not a family of them: the
    /// operator varies but the mistake — a bare `nil` where a key *list* was
    /// meant — is the same one in `case`, `ccase`, and `ecase`, so splitting the
    /// kind by operator would offer a filter no consumer is asking for.
    fn kind(&self) -> &'static str {
        "case-nil-key"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("head={}", self.head)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("head", json!(self.head))]
    }

    /// The same sentence the `case-nil-key` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} clause key nil is the empty key list and never matches; use ((nil) …)",
            self.head
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_case(
    view: &ExpressionView,
    case_form_count: &mut usize,
    violations: &mut Vec<CaseNilKeyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !CASE_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted case form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *case_form_count += 1;

    // The keyform is child 1; clauses start at child 2. A feature-conditional
    // clause reads as an opaque atom (not a list) and is skipped.
    for clause in view.children.iter().skip(2) {
        if !is_paren_list(clause) {
            continue;
        }
        let Some(key_designator) = clause.children.first() else {
            continue;
        };
        if is_bare_nil_key(key_designator) {
            violations.push(CaseNilKeyItem {
                span: key_designator.span,
                head: head.to_owned(),
            });
        }
    }
}

/// Collects every `case`/`ccase`/`ecase` clause with a bare `nil` key designator
/// in one file, with the number of such forms scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no bare nil key here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_case_nil_key_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CaseNilKeyItem>> {
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
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, &mut case_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("case_form_count", json!(case_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CaseNilKeyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_case_nil_key_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build case nil key report")
    }

    /// The `(case_form_count, violations)` pair the report is built from.
    fn keys(input: &str) -> (u64, Vec<CaseNilKeyItem>) {
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
    fn flags_a_bare_nil_key() {
        let (case_form_count, items) = keys("(case x (nil 1) (t 2))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "case");
    }

    #[test]
    fn does_not_flag_a_nil_key_list() {
        // ((nil) …) is the correct way to match the value nil.
        let (_, items) = keys("(case x ((nil) 1) (t 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_nil() {
        // 'nil is quoted-case-key's concern.
        let (_, items) = keys("(case x ('nil 1))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_keys() {
        let (case_form_count, items) = keys("(case x (a 1) (b 2) (t 3))");
        assert_eq!(case_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_an_ecase_nil_key() {
        let (_, items) = keys("(ecase x (nil 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "ecase");
    }

    #[test]
    fn case_folds_the_nil_key() {
        let (_, items) = keys("(case x (NIL 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_nil_key_in_any_clause_position() {
        // nil is never a catch-all, so a trailing nil clause is still dead.
        let (_, items) = keys("(case x (a 1) (nil 2))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (case_form_count, items) = keys("(defun f (x) (case x (nil 1)))");
        assert_eq!(case_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(case x (nil 1))", Dialect::Clojure).expect("parse");
        let report = build_case_nil_key_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build case nil key report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(case x (a 1))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_head() {
        let report = report("(defun f (x)\n  (ecase x (nil 1)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "case-nil-key");
        assert_eq!(finding.json_fields(), vec![("head", json!("ecase"))]);
        assert_eq!(finding.text_columns(), vec!["head=ecase".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_case_scanned_not_only_the_flagged_ones() {
        let report = report("(case x (nil 1))\n(case y (a 1))\n");
        assert_eq!(report.summary, vec![("case_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
