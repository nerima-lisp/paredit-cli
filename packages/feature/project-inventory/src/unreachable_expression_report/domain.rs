//! Dead code *inside* a body: forms after a non-local exit in the same
//! implicit progn.
//!
//! `inspect reachability` answers this between definitions — which functions
//! nothing calls. Nothing answered it within one, and within one is where it
//! hides: a `(return-from f x)` with three forms after it looks exactly like a
//! function with four steps, and Common Lisp compilers warn about it
//! inconsistently or not at all.
//!
//! An exit is `return`, `return-from`, `go`, `throw`, or a call to `error`,
//! because none of them returns normally to the form after it. `error` is the
//! one worth including and the one a purely control-flow analysis misses: it is
//! an ordinary function call in the tree, and it never comes back.
//!
//! Only a *tail* position is examined, and only within the same implicit progn.
//! An exit inside an `if` branch does not kill the forms after the `if` — the
//! other branch may still fall through — and treating it as if it did would
//! report working code.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::common_lisp::common_lisp_operator_head_eq;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

/// Operators that do not return to the form after them.
const EXITS: [&str; 6] = ["return", "return-from", "go", "throw", "error", "abort"];

/// Forms whose trailing children are an implicit progn.
///
/// A form not on this list is not examined: without knowing that its trailing
/// children run in sequence, "after" has no meaning, and an unknown head may be
/// a macro whose arguments are not evaluated at all.
const IMPLICIT_PROGN: [(&str, usize); 12] = [
    ("progn", 1),
    ("when", 2),
    ("unless", 2),
    ("let", 2),
    ("let*", 2),
    ("block", 2),
    ("dolist", 2),
    ("dotimes", 2),
    ("lambda", 2),
    ("defun", 3),
    ("defmethod", 3),
    ("defmacro", 3),
];

/// One form that cannot run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreachableExpression {
    /// The operator that made it unreachable.
    pub after: String,
    /// The enclosing form's head.
    pub within: String,
    /// The dead form, elided.
    pub text: String,
    pub span: ByteSpan,
    pub line: usize,
}

impl Finding for UnreachableExpression {
    fn kind(&self) -> &'static str {
        "unreachable"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("after={}", self.after),
            format!("within={}", self.within),
            self.text.clone(),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("after", json!(self.after)),
            ("within", json!(self.within)),
            ("text", json!(self.text)),
        ]
    }
}

const TEXT_LIMIT: usize = 48;

#[must_use]
pub fn build_unreachable_expression_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> FileFindings<UnreachableExpression> {
    let source = tree.source();
    let mut findings = Vec::new();
    collect(&tree.root_view(), source, &mut findings);
    let dead = findings.len();

    FileFindings::new(
        path.to_path_buf(),
        dialect,
        // The exit operators are Common Lisp's, but the shape is shared; other
        // dialects simply match fewer forms.
        true,
        findings,
        vec![("dead_form_count", json!(dead))],
    )
}

fn collect(view: &ExpressionView, source: &str, findings: &mut Vec<UnreachableExpression>) {
    if let Some(head) = list_head(view) {
        if let Some((_, body_start)) = IMPLICIT_PROGN
            .iter()
            .find(|(name, _)| common_lisp_operator_head_eq(head, name))
        {
            let body = view.children.get(*body_start..).unwrap_or_default();
            if let Some(index) = body.iter().position(|form| exit_operator(form).is_some()) {
                let exit = exit_operator(&body[index]).unwrap_or_default();
                for dead in &body[index + 1..] {
                    findings.push(UnreachableExpression {
                        after: exit.clone(),
                        within: head.to_owned(),
                        text: elide(source, dead.span),
                        span: dead.span,
                        line: line_of(source, dead.span.start().get()),
                    });
                }
            }
        }
    }

    for child in &view.children {
        collect(child, source, findings);
    }
}

/// The exit operator this form *is*, or `None`.
///
/// Only the form's own head is read. An exit nested inside it — in an `if`
/// branch, say — does not make the following sibling unreachable, and reading
/// it as if it did would report working code.
fn exit_operator(view: &ExpressionView) -> Option<String> {
    let head = list_head(view)?;
    EXITS
        .iter()
        .find(|exit| common_lisp_operator_head_eq(head, exit))
        .map(|exit| (*exit).to_owned())
}

fn elide(source: &str, span: ByteSpan) -> String {
    let text = source
        .get(span.start().get()..span.end().get())
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() <= TEXT_LIMIT {
        return text;
    }
    let head = text
        .chars()
        .take(TEXT_LIMIT.saturating_sub(1))
        .collect::<String>();
    format!("{head}…")
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

    fn report(source: &str) -> FileFindings<UnreachableExpression> {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        build_unreachable_expression_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
    }

    #[test]
    fn a_form_after_a_return_from_is_unreachable() {
        let report = report("(defun f () (return-from f 1) (side-effect))");
        assert_eq!(report.findings.len(), 1, "{report:?}");
        assert_eq!(report.findings[0].after, "return-from");
        assert_eq!(report.findings[0].text, "(side-effect)");
    }

    #[test]
    fn a_form_after_an_error_is_unreachable_too() {
        let report = report("(defun f () (error \"no\") (side-effect))");
        assert_eq!(report.findings[0].after, "error");
    }

    #[test]
    fn every_form_after_the_exit_is_reported() {
        let report = report("(defun f () (return-from f 1) (a) (b) (c))");
        assert_eq!(report.findings.len(), 3);
    }

    #[test]
    fn an_exit_in_tail_position_kills_nothing() {
        let report = report("(defun f () (side-effect) (return-from f 1))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn an_exit_inside_a_branch_does_not_kill_the_following_form() {
        // The `else` branch falls through, so `(cleanup)` still runs.
        let report = report("(defun f (x) (if x (return-from f 1) nil) (cleanup))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_nested_body_is_examined_too() {
        let report = report("(defun f () (when t (return-from f 1) (dead)))");
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].within, "when");
    }

    #[test]
    fn a_lambda_list_is_not_read_as_a_body_form() {
        let report = report("(defun f (return) (list return))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn a_head_with_no_implicit_progn_is_not_examined() {
        let report = report("(defun f () (my-macro (return-from f 1) (maybe-run)))");
        assert!(report.findings.is_empty(), "{report:?}");
    }

    #[test]
    fn findings_are_in_source_order() {
        let report = report("(defun f () (return-from f 1) (a) (b) (c))");
        let starts = report
            .findings
            .iter()
            .map(|finding| finding.span.start().get())
            .collect::<Vec<_>>();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
    }
}
