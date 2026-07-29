//! Common Lisp malformed-`case`-clause detection: a `case`, `ccase`, `ecase`,
//! `typecase`, `ctypecase`, or `etypecase` clause that is not a non-empty
//! list. Every clause in these forms must be `(keys-or-type form*)` — a bare
//! atom (`(case x (1 :one) foo)`) or an empty `()` in clause position is a
//! program error, caught only at macroexpansion rather than by the reader.
//!
//! The highest-value catch is a dropped-parenthesis typo: writing
//! `(case x (1 :one) 2 :two)` when `(case x (1 :one) (2 :two))` was meant
//! leaves `2` and `:two` as bare-atom clauses, which this rule flags directly.
//!
//! Unlike [`crate::duplicate_case_keys::domain`] — which excludes the
//! type-testing `typecase` family because its clause heads are type specifiers
//! rather than `eql` keys — this report covers all six forms, since the
//! *structural* requirement "each clause is a non-empty list" is identical
//! across them.
//!
//! Forms whose clause structure is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted `case` (data or a template, not a
//! call), and any clause guarded by a `#+`/`#-` reader conditional or spliced
//! in from a template (`,@`).
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
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

const CASE_HEADS: [&str; 6] = [
    "case",
    "ccase",
    "ecase",
    "typecase",
    "ctypecase",
    "etypecase",
];

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
pub struct MalformedCaseClauseItem {
    pub span: ByteSpan,
    /// The 1-based line the clause starts on.
    pub line: usize,
    /// The `case`-family head as written, in the source's own case.
    pub head: String,
    pub clause: String,
}

impl Finding for MalformedCaseClauseItem {
    /// The rule's own name, not the head.
    ///
    /// `head` is the source's spelling — `CASE`, `Typecase`, `ecase` — and a
    /// `kind` is a fixed vocabulary a consumer can match on. The head is a
    /// `json_fields` entry instead, where its casing is data rather than a tag.
    fn kind(&self) -> &'static str {
        "malformed-case-clause"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("head={}", self.head),
            format!("clause={}", self.clause),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("head", json!(self.head)), ("clause", json!(self.clause))]
    }

    /// The same sentence the `malformed-case-clause` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} clause {} is not a non-empty list",
            self.head, self.clause
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_case(
    view: &ExpressionView,
    source: &str,
    case_form_count: &mut usize,
    violations: &mut Vec<MalformedCaseClauseItem>,
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
    // A quoted/quasiquoted/unquoted case form is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    *case_form_count += 1;

    // The keyform is child 1; clauses start at child 2.
    for clause in view.children.iter().skip(2) {
        if is_structurally_opaque(clause) {
            continue;
        }
        if !is_paren_list(clause) || clause.children.is_empty() {
            violations.push(MalformedCaseClauseItem {
                span: clause.span,
                line: line_of(source, clause.span.start().get()),
                head: head.to_owned(),
                clause: render_expression(clause),
            });
        }
    }
}

/// Collects every malformed `case`-family clause in one file, with the number
/// of `case`-family forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every clause is well formed" for Common
/// Lisp and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_malformed_case_clause_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MalformedCaseClauseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("case_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut case_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_case(subview, source, &mut case_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("case_form_count", json!(case_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MalformedCaseClauseItem> {
        // Use the dialect-aware parse the CLI path uses, which groups Common
        // Lisp `#+`/`#-` reader conditionals into a single node.
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_malformed_case_clause_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build malformed case clause report")
    }

    /// The `(case_form_count, violations)` pair the report is built from.
    fn clauses(input: &str) -> (u64, Vec<MalformedCaseClauseItem>) {
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
    fn flags_a_bare_atom_clause() {
        let (case_form_count, violations) = clauses("(case x (1 :one) foo (2 :two))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].clause, "foo");
        assert_eq!(violations[0].head, "case");
    }

    #[test]
    fn flags_dropped_paren_tail_clauses() {
        // `(1 :one)` is valid; the trailing `2` and `:two` are bare-atom clauses.
        let (_, violations) = clauses("(case x (1 :one) 2 :two)");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn flags_an_empty_clause() {
        let (_, violations) = clauses("(case x () (1 :one))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_valid_clauses() {
        let (case_form_count, violations) =
            clauses("(case x (1 :one) ((2 3) :multi) (otherwise :o))");
        assert_eq!(case_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_keylist_clause() {
        // `(() :never)` is a valid (if useless) clause whose keylist is empty.
        let (_, violations) = clauses("(case x (() :never) (t :yes))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_a_malformed_typecase_clause() {
        let (case_form_count, violations) = clauses("(typecase x (integer 1) bogus (t 0))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "typecase");
    }

    #[test]
    fn flags_a_malformed_ecase_clause() {
        let (_, violations) = clauses("(ecase x (:a 1) bad)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "ecase");
    }

    #[test]
    fn skips_a_feature_conditional_clause() {
        let (case_form_count, violations) = clauses("(case x (1 :one) #+sbcl (2 :two) (t :o))");
        assert_eq!(case_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn skips_a_quoted_case_form() {
        let (case_form_count, violations) = clauses("(list '(case x foo bar))");
        assert_eq!(case_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_case_nested_in_a_function_body() {
        let (case_form_count, violations) = clauses("(defun f (x) (case x (1 :one) oops))");
        assert_eq!(case_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(case x (1 :one) foo)", Dialect::Clojure)
            .expect("parse input");
        let report =
            build_malformed_case_clause_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build malformed case clause report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("case_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(case x (1 :one))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_and_its_clause() {
        let report = report("(defun f (x)\n  (case x (1 :one) oops))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "malformed-case-clause");
        assert_eq!(
            finding.json_fields(),
            vec![("head", json!("case")), ("clause", json!("oops"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["head=case".to_owned(), "clause=oops".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "case clause oops is not a non-empty list"
        );
    }

    #[test]
    fn the_summary_counts_every_case_scanned_not_only_the_flagged_ones() {
        let report = report("(case x (1 :one) oops)\n(case y (2 :two))\n");
        assert_eq!(report.summary, vec![("case_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
