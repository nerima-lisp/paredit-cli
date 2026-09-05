//! `print-object-without-print-unreadable-object` detection: a `print-object`
//! method that writes to the stream itself instead of going through
//! `print-unreadable-object`.
//!
//! `print-unreadable-object` is not a formatting convenience. It is what makes
//! a `print-object` method obey the two variables the caller controls:
//!
//! - it signals a `print-not-readable` error under `*print-readably*` instead
//!   of emitting text that `read` would silently misparse, and
//! - it produces the `#<Class …>` framing that tells a reader the object is
//!   not re-readable in the first place.
//!
//! A method that calls `format` on the stream directly does neither, so a
//! `(write obj :readably t)` quietly produces something that reads back as the
//! wrong thing — the exact failure the standard reserves an error for.
//!
//! A method is only reported when it *does* write. A `print-object` method that
//! delegates — with `call-next-method`, or by calling `print-object` on a
//! component — is not producing its own unframed output and is left alone.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_lint_engine::LintResult;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::for_each_subview;
use serde_json::{Value, json};

use crate::support;

/// The operators that put characters on a stream. `format` leads the list
/// because it is how nearly every hand-written `print-object` method does it.
const STREAM_WRITERS: [&str; 10] = [
    "format",
    "write",
    "write-string",
    "write-line",
    "write-char",
    "princ",
    "prin1",
    "print",
    "pprint",
    "terpri",
];

/// The operators that mean this method is *not* producing its own unframed
/// output: it is framing it, or handing the job to another method.
const DELEGATES: [&str; 3] = [
    "print-unreadable-object",
    "print-object",
    "call-next-method",
];

#[derive(Debug, Clone)]
pub struct PrintObjectWithoutPrintUnreadableObjectItem {
    /// The span of the whole `defmethod` form.
    pub span: ByteSpan,
    /// The first stream-writing operator found in the body, which is what a
    /// reader has to go and look at.
    pub writer: String,
}

impl Finding for PrintObjectWithoutPrintUnreadableObjectItem {
    fn kind(&self) -> &'static str {
        "print-object-without-print-unreadable-object"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("writer={}", self.writer)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("writer", json!(self.writer))]
    }

    fn message(&self) -> String {
        format!(
            "this print-object method writes with {} instead of print-unreadable-object: \
             its output ignores *print-readably* rather than signalling print-not-readable",
            self.writer
        )
    }
}

/// The first stream-writing operator called anywhere under `view`, searched in
/// the order [`STREAM_WRITERS`] lists so the answer does not depend on which
/// branch of the body happens to be walked first.
fn first_writer(body: &[ExpressionView]) -> Option<&'static str> {
    STREAM_WRITERS
        .into_iter()
        .find(|writer| body.iter().any(|form| support::calls_any(form, &[writer])))
}

pub fn examine_print_object_without_print_unreadable_object(
    tree: &SyntaxTree,
    view: &ExpressionView,
    print_object_method_count: &mut usize,
    violations: &mut Vec<PrintObjectWithoutPrintUnreadableObjectItem>,
) {
    let Some(method) = support::method_form(view) else {
        return;
    };
    if !method
        .name
        .is_some_and(|name| paredit_core_syntax::view_query::symbol_is(name, "print-object"))
    {
        return;
    }
    let Some(site) = support::locate(tree, view.span) else {
        return;
    };
    if site.quoted || !site.top_level {
        return;
    }
    *print_object_method_count += 1;

    if method
        .body
        .iter()
        .any(|form| support::calls_any(form, &DELEGATES))
    {
        return;
    }
    let Some(writer) = first_writer(method.body) else {
        return;
    };

    violations.push(PrintObjectWithoutPrintUnreadableObjectItem {
        span: view.span,
        writer: writer.to_owned(),
    });
}

/// Collects every unframed `print-object` method in one file, with the number
/// of `print-object` methods scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_print_object_without_print_unreadable_object_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<PrintObjectWithoutPrintUnreadableObjectItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("print_object_method_count", json!(0))],
        ));
    }

    let mut print_object_method_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_print_object_without_print_unreadable_object(
                tree,
                subview,
                &mut print_object_method_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![(
            "print_object_method_count",
            json!(print_object_method_count),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<PrintObjectWithoutPrintUnreadableObjectItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_print_object_without_print_unreadable_object_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build print-object report")
    }

    fn violations(input: &str) -> Vec<PrintObjectWithoutPrintUnreadableObjectItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_print_object_method_that_formats_to_the_stream() {
        let found = violations(
            "(defmethod print-object ((c circle) stream)\n  (format stream \"circle ~a\" (r c)))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].writer, "format");
    }

    #[test]
    fn flags_a_method_that_uses_the_lower_level_writers() {
        let found = violations(
            "(defmethod print-object ((c circle) stream)\n  (write-string \"circle\" stream))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].writer, "write-string");
    }

    #[test]
    fn flags_an_around_qualified_print_object_method_too() {
        let found =
            violations("(defmethod print-object :around ((c circle) stream) (princ \"c\" stream))");
        assert_eq!(found.len(), 1);
    }

    /// The near miss: the framing is there, nested where it usually is.
    #[test]
    fn does_not_flag_a_method_that_frames_its_output() {
        let found = violations(
            "(defmethod print-object ((c circle) stream)\n  \
             (print-unreadable-object (c stream :type t)\n    (format stream \"~a\" (r c))))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_method_that_delegates_to_the_next_one() {
        let found = violations(
            "(defmethod print-object ((c circle) stream) (log-it c) (call-next-method))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_method_that_writes_nothing() {
        let found = violations("(defmethod print-object ((c circle) stream) c)");
        assert!(found.is_empty());
    }

    /// The other near miss: any other generic may write to a stream freely.
    #[test]
    fn does_not_flag_a_method_on_another_generic() {
        let found =
            violations("(defmethod describe-it ((c circle) stream) (format stream \"circle\"))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_plain_function_that_formats() {
        let found = violations("(defun print-object (c stream) (format stream \"circle\"))");
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_print_object_method() {
        let found = violations(
            "(setf template '(defmethod print-object ((c circle) s) (format s \"circle\")))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_print_object_method_inside_an_unescaped_quasiquote() {
        let found = violations(
            "(defmacro printer () `(defmethod print-object ((c circle) s) (format s \"c\")))",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn does_not_flag_a_print_object_method_written_inside_a_string() {
        let found = violations(
            "(defparameter *doc* \"(defmethod print-object ((c circle) s) (format s \\\"c\\\"))\")",
        );
        assert!(found.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_print_object_method_scanned() {
        let built = report(
            "(defmethod print-object ((c circle) s) (print-unreadable-object (c s) nil))\n\
             (defmethod print-object ((q square) s) (format s \"square\"))",
        );
        assert_eq!(built.summary, vec![("print_object_method_count", json!(2))]);
        assert_eq!(built.findings.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(defmethod print-object ((c circle) s) (format s \"c\"))",
            Dialect::Clojure,
        )
        .expect("parse");
        let built = build_print_object_without_print_unreadable_object_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!built.dialect_modelled);
        assert!(built.findings.is_empty());
    }

    #[test]
    fn a_finding_carries_its_line_and_its_message() {
        let built =
            report("(defun f ())\n(defmethod print-object ((c circle) s) (princ \"c\" s))\n");
        let finding = &built.findings[0];
        assert_eq!(built.line_of(finding), 2);
        assert_eq!(
            finding.kind(),
            "print-object-without-print-unreadable-object"
        );
        assert_eq!(finding.text_columns(), vec!["writer=princ".to_owned()]);
        assert!(finding.message().contains("*print-readably*"));
    }
}
