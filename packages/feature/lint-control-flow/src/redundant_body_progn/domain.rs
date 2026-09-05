//! Common Lisp redundant-body-`progn` detection: a multi-form `(progn …)` that
//! appears as a body form of a macro whose body is *already* an implicit progn
//! — `when`, `unless`, `dolist`, `dotimes`, `block`, `lambda`, `let`, `let*`,
//! `flet`, `labels`, `macrolet`, `defun`, `defmacro`. In all of these, the forms
//! after the fixed prefix are evaluated in sequence, so wrapping them in an
//! explicit progn changes nothing: `(when c (progn a b))` is exactly
//! `(when c a b)`. The wrapper is habit carried over from languages without an
//! implicit progn.
//!
//! This is the implicit-body companion to two neighbouring rules:
//! [`crate::redundant_progn::domain`] owns the 0-form and 1-form progns
//! (redundant on their own, in any position), and
//! [`crate::nested_progn::domain`] owns a multi-form progn nested
//! directly inside another `progn`. This rule owns a multi-form progn in the
//! body of the *other* implicit-progn forms. The three never flag the same
//! span.
//!
//! Only slots at or after each form's body start are inspected — the binding
//! list of a `let`, the lambda list of a `defun`, the test of a `when`, etc. are
//! never body positions — so a `(progn …)` used as a binding init or a single
//! value expression is correctly left alone.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The index at which a form's implicit-progn body begins, or `None` if the head
/// is not one of the recognized implicit-progn forms. Everything from this index
/// onward is a body form (declarations and docstrings included, which are never
/// progns), so a `(progn …)` there is spliceable.
fn body_start(head: &str) -> Option<usize> {
    let after_two = [
        "when", "unless", "dolist", "dotimes", "block", "lambda", "let", "let*", "flet", "labels",
        "macrolet", "catch",
    ];
    let after_three = ["defun", "defmacro"];
    if after_two.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        Some(2)
    } else if after_three
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        Some(3)
    } else {
        None
    }
}

/// Whether `view` is a `(progn …)` form *in body position* — a bare one,
/// carrying no reader prefix of its own.
///
/// The prefix test is not a refinement of the head test, it is half of the
/// premise. `(defmacro m (x) `(progn (f ,x) (g ,x)))` has a `progn` sitting
/// where `body_start` expects a body form, but that progn is the macro's
/// *output*, not its body: it is the one form the macro has to return, and the
/// two calls under it are emitted, not evaluated here. Splicing it away leaves
/// a macro that evaluates `(f x)` and `(g x)` at expansion time and returns the
/// second — and, because the backquote goes with the spliced-out wrapper, one
/// whose remaining `,x` is a comma outside any backquote, which no longer
/// reads. `'(progn a b)` in the same position is a literal three-element list
/// and splicing rewrites the data.
///
/// Neither is a body progn, and both are exactly what this rule is otherwise
/// most likely to hit: `` `(progn …) `` is the standard shape of a multi-form
/// macro expansion.
fn is_progn(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && is_paren_list(view)
        && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("progn"))
}

#[derive(Debug, Clone)]
pub struct RedundantBodyPrognItem {
    /// The span of the inner (redundant) progn.
    pub span: ByteSpan,
    /// The span covering just the inner progn's body forms, so a fix can splice
    /// that source in place of the wrapper.
    ///
    pub body_span: ByteSpan,
    /// The inner progn's body form count (always >= 2 here).
    pub body_form_count: usize,
    /// The enclosing form's head (`when`, `let`, `defun`, …), for the message.
    pub parent: String,
}

impl Finding for RedundantBodyPrognItem {
    /// The rule's own name. The enclosing head is one of fourteen and is
    /// carried as data rather than as the tag: it is a `String` built per
    /// finding, not a canonical `&'static str` the analysis already holds.
    fn kind(&self) -> &'static str {
        "redundant-body-progn"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("parent={}", self.parent),
            format!("body_form_count={}", self.body_form_count),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("parent", json!(self.parent)),
            ("body_form_count", json!(self.body_form_count)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "progn with {} forms is a {} body; splice its forms in",
            self.body_form_count, self.parent
        )
    }
}

pub fn examine_form(
    view: &ExpressionView,
    implicit_progn_form_count: &mut usize,
    violations: &mut Vec<RedundantBodyPrognItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(start) = body_start(head) else {
        return;
    };
    *implicit_progn_form_count += 1;

    // Any body form that is itself a multi-form progn splices redundantly.
    for child in view.children.iter().skip(start) {
        if !is_progn(child) {
            continue;
        }
        let body = &child.children[1..];
        if body.len() >= 2 {
            let body_span = ByteSpan::new(body[0].span.start(), body[body.len() - 1].span.end());
            violations.push(RedundantBodyPrognItem {
                span: child.span,
                body_span,
                body_form_count: body.len(),
                parent: head.to_ascii_lowercase(),
            });
        }
    }
}

/// Collects every multi-form progn used as a body form of an implicit-progn
/// macro in one file, with the number of such *macro* forms scanned as the
/// denominator beside them.
///
/// The denominator counts the enclosing macro forms, not the progns: the
/// question this report answers is "how many implicit-progn bodies were looked
/// at, and how many of them wrap their forms needlessly".
///
/// Reports unsupported dialects as unmodelled.
pub fn build_redundant_body_progn_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantBodyPrognItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("implicit_progn_form_count", json!(0))],
        ));
    }

    let mut implicit_progn_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, &mut implicit_progn_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![(
            "implicit_progn_form_count",
            json!(implicit_progn_form_count),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RedundantBodyPrognItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_body_progn_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant body progn report")
    }

    /// The `(implicit_progn_form_count, violations)` pair the report is built
    /// from.
    fn progns(input: &str) -> (u64, Vec<RedundantBodyPrognItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "implicit_progn_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("implicit_progn_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_progn_body_of_when() {
        let (count, violations) = progns("(when c (progn a b))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "when");
        assert_eq!(violations[0].body_form_count, 2);
    }

    #[test]
    fn body_span_isolates_the_inner_body_forms() {
        let input = "(unless done (progn a b c))";
        let (_, violations) = progns(input);
        let body = violations[0].body_span;
        assert_eq!(&input[body.start().get()..body.end().get()], "a b c");
    }

    #[test]
    fn flags_a_progn_body_of_let_and_defun() {
        let (_, violations) = progns("(let ((x 1)) (progn a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "let");

        let (_, violations) = progns("(defun f (x) (progn a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "defun");
    }

    #[test]
    fn flags_a_progn_after_a_declaration() {
        let (_, violations) = progns("(let ((x 1)) (declare (ignore x)) (progn a b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_single_form_progn() {
        // (progn x) is the redundant-progn rule's job, not this one.
        let (_, violations) = progns("(when c (progn x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_in_the_test_position() {
        // The progn is the when test (index 1), a single value form, not a body.
        let (_, violations) = progns("(when (progn a b) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_as_a_let_binding_init() {
        let (_, violations) = progns("(let ((x (progn a b))) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_inside_a_progn() {
        // That is the nested-progn rule's territory (parent is progn).
        let (count, violations) = progns("(progn (progn a b))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_parent_head() {
        let (_, violations) = progns("(WHEN c (progn a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "when");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(when c (progn a b))", Dialect::Clojure)
            .expect("parse");
        let report =
            build_redundant_body_progn_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build redundant body progn report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("implicit_progn_form_count", json!(0))]
        );
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(when c a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_parent_and_body_form_count() {
        let report = report("(defun f (x)\n  (when x (progn a b)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-body-progn");
        assert_eq!(
            finding.json_fields(),
            vec![("parent", json!("when")), ("body_form_count", json!(2))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["parent=when".to_owned(), "body_form_count=2".to_owned()]
        );
    }

    /// The denominator counts the *enclosing* implicit-progn macro forms, so it
    /// moves independently of the finding count — and, deliberately, does not
    /// count the progns themselves.
    #[test]
    fn the_summary_counts_every_implicit_progn_form_not_only_the_flagged_ones() {
        let report = report("(when c (progn a b))\n(unless d x)\n");
        assert_eq!(
            report.summary,
            vec![("implicit_progn_form_count", json!(2))]
        );
        assert_eq!(report.findings.len(), 1);
    }
}
