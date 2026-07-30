//! Common Lisp `typecase`-`nil`-key detection: a `typecase`, `etypecase`, or
//! `ctypecase` clause whose head is the bare atom `nil` —
//! `(typecase x (nil 1) …)`. In `typecase`, a clause head is a *type specifier*,
//! and `nil` is the **empty** type (the `nil` type), which no object is ever of,
//! so the clause is dead and can never be selected. Authors almost always mean
//! "match the value `nil`", which requires the `null` type — `(null …)`; the
//! bare `nil` is a silent dead clause.
//!
//! Only the bare, unquoted `nil` atom is flagged:
//!
//!   - `(nil …)`  → flagged: `nil` is the empty type, matches nothing.
//!   - `(null …)` → correct: the `null` type matches the value `nil`.
//!   - `('nil …)` → a quoted datum, not a type specifier, not flagged.
//!
//! The catch-all `(t …)` clause matches every object and is deliberately not
//! flagged. Scoped to `typecase`/`etypecase`/`ctypecase` — the type-dispatch
//! forms. `case`'s clause heads are key designators, a different shape, and are
//! handled by `case-nil-key`. This rule does not rewrite anything (whether the
//! clause is a typo or dead vestige is the author's call), so it is report-only.
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

const TYPECASE_HEADS: [&str; 3] = ["typecase", "etypecase", "ctypecase"];

/// Whether a clause's head is the bare, unquoted atom `nil` (the empty type). A
/// `null` type and a quoted `'nil` are both excluded.
fn is_bare_nil_key(key_designator: &ExpressionView) -> bool {
    key_designator.reader_prefixes.is_empty()
        && atom_text(key_designator).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct TypecaseNilKeyItem {
    /// The span of the offending `nil` type specifier.
    pub span: ByteSpan,
    /// The typecase operator (`typecase`/`etypecase`/`ctypecase`), for the finding message.
    pub head: String,
}

impl Finding for TypecaseNilKeyItem {
    /// The rule's own name. The operator is carried as data rather than as the
    /// tag, because it is taken verbatim from source and keeps its casing.
    fn kind(&self) -> &'static str {
        "typecase-nil-key"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.head.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("head", json!(self.head))]
    }

    /// The same sentence the `typecase-nil-key` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} clause type nil is the empty type and never matches; use null",
            self.head
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_case(
    view: &ExpressionView,
    typecase_form_count: &mut usize,
    violations: &mut Vec<TypecaseNilKeyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !TYPECASE_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted typecase form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *typecase_form_count += 1;

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
            violations.push(TypecaseNilKeyItem {
                span: key_designator.span,
                head: head.to_owned(),
            });
        }
    }
}

/// Collects every `typecase`/`etypecase`/`ctypecase` clause whose head is a bare
/// `nil` type specifier in one file, with the number of such forms scanned as
/// the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no bare nil type here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_typecase_nil_key_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TypecaseNilKeyItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("typecase_form_count", json!(0))],
        ));
    }

    let mut typecase_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, &mut typecase_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("typecase_form_count", json!(typecase_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<TypecaseNilKeyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_typecase_nil_key_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build typecase nil key report")
    }

    /// The `(typecase_form_count, violations)` pair the report is built from.
    fn keys(input: &str) -> (u64, Vec<TypecaseNilKeyItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "typecase_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("typecase_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_bare_nil_type() {
        let (typecase_form_count, items) = keys("(typecase x (nil 1) (t 2))");
        assert_eq!(typecase_form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "typecase");
    }

    #[test]
    fn does_not_flag_a_null_type() {
        // (null …) is the correct way to match the value nil.
        let (_, items) = keys("(typecase x (null 1) (t 2))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_nil() {
        // 'nil is a quoted datum, not a type specifier.
        let (_, items) = keys("(typecase x ('nil 1))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_ordinary_types() {
        let (typecase_form_count, items) = keys("(typecase x (integer 1) (string 2) (t 3))");
        assert_eq!(typecase_form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn flags_an_etypecase_nil_type() {
        let (_, items) = keys("(etypecase x (nil 1))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "etypecase");
    }

    #[test]
    fn case_folds_the_nil_type() {
        let (_, items) = keys("(typecase x (NIL 1))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_nil_type_in_any_clause_position() {
        // nil is never a catch-all, so a trailing nil clause is still dead.
        let (_, items) = keys("(typecase x (integer 1) (nil 2))");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn finds_a_typecase_nested_in_a_function_body() {
        let (typecase_form_count, items) = keys("(defun f (x) (typecase x (nil 1)))");
        assert_eq!(typecase_form_count, 1);
        assert_eq!(items.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(typecase x (nil 1))", Dialect::Clojure)
            .expect("parse");
        let report = build_typecase_nil_key_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build typecase nil key report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("typecase_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(typecase x (t 1))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_head() {
        let report = report("(defun f (x)\n  (typecase x (nil 1)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "typecase-nil-key");
        assert_eq!(finding.json_fields(), vec![("head", json!("typecase"))]);
        assert_eq!(finding.text_columns(), vec!["typecase".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_typecase_scanned_not_only_the_flagged_ones() {
        let report = report("(typecase x (nil 1))\n(typecase y (integer 1))\n");
        assert_eq!(report.summary, vec![("typecase_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
