//! Deviation from the Emacs/SLIME indentation convention.
//!
//! This is not `edit format` with a different name. `format` states what *this
//! tool* would print; this states what an Emacs user's editor would produce on
//! `C-M-q`, and the two disagree on purpose. Most Lisp is written in Emacs, so
//! a file that `format` considers correct can still churn every line the moment
//! someone opens it — and that churn is what buries a real diff.
//!
//! The convention is one rule with a table of exceptions:
//!
//! - A form's arguments line up under its **first argument**, when the first
//!   argument is on the same line as the head. `(foo bar` puts the next line
//!   under `bar`.
//! - A form whose head is on a line by itself indents its children by **one
//!   space**.
//! - A **special form or macro with distinguished arguments** — `defun`, `let`,
//!   `when`, `dolist`, … — indents its body by **two spaces** regardless, and
//!   its distinguished arguments differently again. This is what
//!   `lisp-indent-function` encodes, and it is why a purely structural rule
//!   gets `defun` wrong.
//!
//! Only the body rule is checked, and only for forms whose head is in the
//! table. That is the deviation that actually happens: nobody hand-indents a
//! `defun` body to four spaces by choice, and a form this table does not know
//! is one where Emacs itself would fall back to the structural rule that the
//! author already followed.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteOffset, ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// Heads whose body Emacs indents by two spaces, paired with how many
/// distinguished arguments precede that body.
///
/// The count is what `lisp-indent-function` calls the form's "number of
/// distinguished arguments": `defun` has two (name and lambda list), `when` has
/// one (the test), `progn` has none.
const BODY_FORMS: [(&str, usize); 30] = [
    ("defun", 2),
    ("defmacro", 2),
    ("defmethod", 2),
    ("defgeneric", 2),
    ("defclass", 3),
    ("define-condition", 3),
    ("lambda", 1),
    ("let", 1),
    ("let*", 1),
    ("flet", 1),
    ("labels", 1),
    ("macrolet", 1),
    ("symbol-macrolet", 1),
    ("when", 1),
    ("unless", 1),
    ("dolist", 1),
    ("dotimes", 1),
    ("do", 2),
    ("do*", 2),
    ("case", 1),
    ("ecase", 1),
    ("typecase", 1),
    ("etypecase", 1),
    ("with-open-file", 1),
    ("with-slots", 2),
    ("with-accessors", 2),
    ("multiple-value-bind", 2),
    ("destructuring-bind", 2),
    ("handler-case", 1),
    ("progn", 0),
];

/// One body form indented differently from the convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndentFinding {
    /// The form's head.
    pub head: String,
    /// The column the body form starts at.
    pub actual: usize,
    /// The column the Emacs convention puts it at.
    pub expected: usize,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for IndentFinding {
    fn kind(&self) -> &'static str {
        "body-indent"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            self.head.clone(),
            format!("actual={}", self.actual),
            format!("expected={}", self.expected),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("actual_column", json!(self.actual)),
            ("expected_column", json!(self.expected)),
        ]
    }
}

/// How far the Emacs convention indents a body past its form's own column.
const BODY_INDENT: usize = 2;

#[must_use]
pub fn build_indentation_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<IndentFinding> {
    let source = tree.source();
    let mut findings = Vec::new();
    let mut checked = 0usize;
    collect(&tree.root_view(), source, &mut checked, &mut findings);

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // The convention is Emacs's, and Emacs indents every Lisp dialect this
        // way; the head table is Common Lisp's, so other dialects simply match
        // fewer forms rather than none.
        true,
        findings,
        vec![("checked_form_count", json!(checked))],
    )
}

fn collect(
    view: &ExpressionView,
    source: &str,
    checked: &mut usize,
    findings: &mut Vec<IndentFinding>,
) {
    if let Some(head) = list_head(view) {
        if let Some((_, distinguished)) = BODY_FORMS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(head))
        {
            *checked += 1;
            let expected = column_of(source, view.span.start().get()) + BODY_INDENT;
            for body in view.children.iter().skip(distinguished + 1) {
                // Only a body form that *starts* a line is indented. One
                // sharing a line with its predecessor is placed by the form
                // before it, and the convention says nothing about it.
                if !starts_line(source, body.span.start().get()) {
                    continue;
                }
                let actual = column_of(source, body.span.start().get());
                if actual != expected {
                    findings.push(IndentFinding {
                        head: head.to_owned(),
                        actual,
                        expected,
                        span: ByteSpan::new(
                            ByteOffset::new(body.span.start().get()),
                            body.span.end(),
                        ),
                        line: line_of(source, body.span.start().get()),
                    });
                }
            }
        }
    }

    for child in &view.children {
        collect(child, source, checked, findings);
    }
}

/// Whether only whitespace precedes this offset on its line.
fn starts_line(source: &str, offset: usize) -> bool {
    source
        .get(..offset.min(source.len()))
        .and_then(|before| before.rsplit_once('\n'))
        .is_some_and(|(_, line)| line.chars().all(char::is_whitespace))
}

/// The 0-based column of an offset, counted in characters.
fn column_of(source: &str, offset: usize) -> usize {
    let before = source.get(..offset.min(source.len())).unwrap_or(source);
    before
        .rsplit_once('\n')
        .map_or(before, |(_, line)| line)
        .chars()
        .count()
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

    fn report(source: &str) -> FileFindings<IndentFinding> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_indentation_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_defun_body_at_two_spaces_matches_the_convention() {
        let report = report("(defun f (x)\n  (list x))\n");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_defun_body_at_four_spaces_is_reported() {
        let report = report("(defun f (x)\n    (list x))\n");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].actual, 4);
        assert_eq!(report.findings[0].expected, 2);
    }

    #[test]
    fn a_nested_form_is_measured_against_its_own_column() {
        // The inner `when` starts at column 2, so its body belongs at 4.
        let report = report("(defun f (x)\n  (when x\n    (list x)))\n");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_body_form_sharing_a_line_is_not_measured() {
        let report = report("(defun f (x) (list x))\n");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_lambda_list_is_a_distinguished_argument_rather_than_a_body_form() {
        // The lambda list on its own line is not body, so its column is not
        // the body's column and it must not be reported.
        let report = report("(defun f\n    (x)\n  (list x))\n");
        assert!(
            report.findings.iter().all(|finding| finding.line != 2),
            "{report:?}"
        );
    }

    #[test]
    fn a_head_the_table_does_not_know_is_not_measured() {
        let report = report("(my-macro x\n        y)\n");
        assert!(report.findings.is_empty(), "{report:?}");
        assert_eq!(report.summary, vec![("checked_form_count", json!(0))]);
    }

    #[test]
    fn the_checked_count_reports_the_denominator() {
        let report = report("(defun f (x)\n  (when x\n    (list x)))\n");
        assert_eq!(report.summary, vec![("checked_form_count", json!(2))]);
    }

    #[test]
    fn columns_are_counted_in_characters_rather_than_bytes() {
        // A multi-byte comment before the form must not shift its column.
        let report = report(";; 状況\n(defun f (x)\n  (list x))\n");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defun f (x)\n      (a x)\n      (b x))\n");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(starts.len(), 2);
    }
}
