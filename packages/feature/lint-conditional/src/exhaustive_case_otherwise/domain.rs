//! Common Lisp exhaustive-case-otherwise detection: an `ecase`, `ccase`,
//! `etypecase`, or `ctypecase` form with a `t` or `otherwise` clause. These
//! are the *exhaustive* case forms — they signal an error when no clause
//! matches, so a default clause is not permitted (CLHS: "no explicit
//! otherwise or t clause is permitted"). Writing one is almost always a bug: a
//! `case` copied into an `ecase`, silently defeating the exhaustiveness check.
//!
//! The catch-all must be a *bare* `t`/`otherwise` key designator; a key list
//! such as `((t) …)` matches the literal symbol `T` and is not a default, so
//! it is not flagged — the same distinction as
//! [`crate::unreachable_case_clause::domain`], which handles the
//! non-exhaustive `case`/`typecase` where a default *is* permitted.
//!
//! Forms whose clause structure is not statically visible are skipped: a
//! quoted/quasiquoted form, and clauses guarded by a `#+`/`#-` reader
//! conditional (which parse as opaque atoms, not list clauses).
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

const EXHAUSTIVE_HEADS: [&str; 4] = ["ecase", "ccase", "etypecase", "ctypecase"];

/// The bare `t`/`otherwise` catch-all designator of a clause, if any. A key
/// *list* (`(t)`) matches the literal symbol and is not a catch-all, so the
/// first child must be an unprefixed atom.
fn catch_all_designator(clause: &ExpressionView) -> Option<String> {
    let key = clause.children.first()?;
    if !key.reader_prefixes.is_empty() {
        return None;
    }
    let text = atom_text(key)?;
    if text.eq_ignore_ascii_case("t") || text.eq_ignore_ascii_case("otherwise") {
        Some(text.to_owned())
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct ExhaustiveCaseOtherwiseItem {
    /// The span of the offending clause.
    pub span: ByteSpan,
    pub head: String,
    pub designator: String,
}

impl Finding for ExhaustiveCaseOtherwiseItem {
    /// The rule's own name rather than the head or the designator. Both are
    /// copied source-cased out of the file and typed `String`, not one of a
    /// closed set of `&'static str` names, so both stay JSON fields and columns.
    fn kind(&self) -> &'static str {
        "exhaustive-case-otherwise"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("head={}", self.head),
            format!("designator={}", self.designator),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("designator", json!(self.designator)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "{} does not permit a {} clause (it is exhaustive)",
            self.head, self.designator
        )
    }
}

pub fn examine_case(
    view: &ExpressionView,
    case_form_count: &mut usize,
    violations: &mut Vec<ExhaustiveCaseOtherwiseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !EXHAUSTIVE_HEADS
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
        if let Some(designator) = catch_all_designator(clause) {
            violations.push(ExhaustiveCaseOtherwiseItem {
                span: clause.span,
                head: head.to_owned(),
                designator,
            });
        }
    }
}

/// Collects every exhaustive case form (`ecase`/`ccase`/`etypecase`/
/// `ctypecase`) with a forbidden `t`/`otherwise` clause in one file, with the
/// number of such forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_exhaustive_case_otherwise_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ExhaustiveCaseOtherwiseItem>> {
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

    fn report(input: &str) -> FileFindings<ExhaustiveCaseOtherwiseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_exhaustive_case_otherwise_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build exhaustive case otherwise report")
    }

    fn violations(input: &str) -> (u64, Vec<ExhaustiveCaseOtherwiseItem>) {
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
    fn flags_a_t_clause_in_ecase() {
        let (form_count, items) = violations("(ecase x (1 :a) (t :b))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "ecase");
        assert_eq!(items[0].designator, "t");
    }

    #[test]
    fn flags_an_otherwise_clause_in_etypecase() {
        let (_, items) = violations("(etypecase x (integer 1) (otherwise 2))");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].head, "etypecase");
        assert_eq!(items[0].designator, "otherwise");
    }

    #[test]
    fn flags_a_ccase_and_ctypecase() {
        let (_, ccase) = violations("(ccase x (1 :a) (otherwise :b))");
        assert_eq!(ccase.len(), 1);
        let (_, ctypecase) = violations("(ctypecase x (integer 1) (t 2))");
        assert_eq!(ctypecase.len(), 1);
    }

    #[test]
    fn does_not_flag_a_default_in_case() {
        // `case` permits a t/otherwise default; only the exhaustive forms don't.
        let (form_count, items) = violations("(case x (1 :a) (t :b))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_literal_t_key_list() {
        let (_, items) = violations("(ecase x ((t) :sym) (1 :a))");
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_normal_ecase() {
        let (form_count, items) = violations("(ecase x (1 :a) (2 :b) (3 :c))");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_case_form() {
        let (form_count, items) = violations("(list '(ecase x (t 1)))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (form_count, items) = violations("(defun f (x) (ecase x (1 :a) (otherwise :b)))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(ecase x (t 1))", Dialect::Clojure)
            .expect("parse input");
        let report =
            build_exhaustive_case_otherwise_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build exhaustive case otherwise report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(ecase x (1 :a))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_and_its_designator() {
        let report = report("(defun f (x)\n  (ecase x (1 :a) (t :b)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "exhaustive-case-otherwise");
        assert_eq!(
            finding.json_fields(),
            vec![("head", json!("ecase")), ("designator", json!("t"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["head=ecase".to_owned(), "designator=t".to_owned()]
        );
    }

    #[test]
    fn the_summary_counts_every_exhaustive_form_scanned_not_only_the_flagged_ones() {
        let report = report("(ecase x (t 1))\n(etypecase y (integer 1))\n");
        assert_eq!(report.summary, vec![("case_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
