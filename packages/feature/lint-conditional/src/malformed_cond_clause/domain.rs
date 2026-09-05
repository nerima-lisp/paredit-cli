//! Common Lisp malformed-`cond`-clause detection: a `cond` clause that is not
//! a non-empty list. Every `cond` clause must be `(test-form form*)` — a bare
//! atom (`(cond ((foo) 1) bar …)`) or an empty `()` in clause position is a
//! program error, caught only at macroexpansion rather than by the reader.
//!
//! The highest-value catch is a dropped-parenthesis typo: writing
//! `(cond ((foo) 1) (bar) 2)` when a `(… 2)` clause was meant leaves `2` as a
//! bare-atom clause, which this rule flags directly.
//!
//! Forms whose clause structure is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted `cond` (data or a template, not a
//! call), and any clause guarded by a `#+`/`#-` reader conditional or spliced
//! in from a template (`,@`).
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// Whether a clause's structure is not statically visible: it is guarded by a
/// `#+`/`#-` reader conditional (which the dialect reader groups into an atom
/// whose text begins `#+`/`#-`, or leaves as a bare marker atom) or spliced
/// from a template (`,@`, or a Clojure `#?`/`#?@`).
fn is_structurally_opaque(clause: &ExpressionView) -> bool {
    let opaque_prefix = clause.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    opaque_prefix
        || atom_text(clause).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct MalformedCondClauseItem {
    pub span: ByteSpan,
    pub clause: String,
}

impl Finding for MalformedCondClauseItem {
    /// The rule's own name. Every finding here is the one shape "a clause that
    /// is not a non-empty list", with nothing to sub-divide it by.
    fn kind(&self) -> &'static str {
        "malformed-cond-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("clause={}", self.clause)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("clause", json!(self.clause))]
    }

    fn message(&self) -> String {
        format!("cond clause {} is not a non-empty list", self.clause)
    }
}

pub fn examine_cond(
    view: &ExpressionView,
    cond_form_count: &mut usize,
    violations: &mut Vec<MalformedCondClauseItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("cond")) {
        return;
    }
    // A quoted/quasiquoted/unquoted `cond` is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *cond_form_count += 1;

    for clause in view.children.iter().skip(1) {
        if is_structurally_opaque(clause) {
            continue;
        }
        // Every clause must be a non-empty list `(test form*)`.
        if !is_paren_list(clause) || clause.children.is_empty() {
            violations.push(MalformedCondClauseItem {
                span: clause.span,
                clause: render_expression(clause),
            });
        }
    }
}

/// Collects every malformed `cond` clause in one file, with the number of
/// `cond` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_malformed_cond_clause_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MalformedCondClauseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("cond_form_count", json!(0))],
        ));
    }

    let mut cond_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_cond(subview, &mut cond_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("cond_form_count", json!(cond_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MalformedCondClauseItem> {
        // Use the dialect-aware parse the CLI path uses, which groups Common
        // Lisp `#+`/`#-` reader conditionals into a single node.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_malformed_cond_clause_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build malformed cond clause report")
    }

    fn clauses(input: &str) -> (u64, Vec<MalformedCondClauseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "cond_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("cond_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_bare_atom_clause() {
        let (cond_form_count, violations) = clauses("(cond ((foo) 1) bar ((baz) 2))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].clause, "bar");
    }

    #[test]
    fn flags_a_dropped_paren_tail_clause() {
        // `(bar)` is a valid clause; the trailing `2` is a bare-atom clause.
        let (_, violations) = clauses("(cond ((foo) 1) (bar) 2)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].clause, "2");
    }

    #[test]
    fn flags_an_empty_clause() {
        let (_, violations) = clauses("(cond () ((foo) 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_valid_clauses() {
        let (cond_form_count, violations) = clauses("(cond ((foo) 1) ((bar) 2) (t 3))");
        assert_eq!(cond_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_test_only_clause() {
        // `((foo))` is a clause whose test is `(foo)` with no body — valid.
        let (_, violations) = clauses("(cond ((foo)) (t 1))");
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_feature_conditional_clause() {
        // `#+sbcl ((foo) 1)` reads as one node; its structure is not statically
        // visible, so it is not flagged.
        let (cond_form_count, violations) = clauses("(cond #+sbcl ((foo) 1) ((bar) 2))");
        assert_eq!(cond_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_quoted_cond_form() {
        let (cond_form_count, violations) = clauses("(list '(cond foo bar))");
        assert_eq!(cond_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_cond_nested_in_a_function_body() {
        let (cond_form_count, violations) = clauses("(defun f (x) (cond ((p x) 1) x))");
        assert_eq!(cond_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(cond ((foo) 1) bar)", Dialect::Clojure)
            .expect("parse input");
        let report =
            build_malformed_cond_clause_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build malformed cond clause report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("cond_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(cond ((foo) 1))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_clause() {
        let report = report("(defun f (x)\n  (cond ((p x) 1) x))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "malformed-cond-clause");
        assert_eq!(finding.json_fields(), vec![("clause", json!("x"))]);
        assert_eq!(finding.text_columns(), vec!["clause=x".to_owned()]);
        assert_eq!(finding.message(), "cond clause x is not a non-empty list");
    }

    #[test]
    fn the_summary_counts_every_cond_scanned_not_only_the_flagged_ones() {
        let report = report("(cond ((foo) 1) bar)\n(cond ((baz) 2))\n");
        assert_eq!(report.summary, vec![("cond_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
