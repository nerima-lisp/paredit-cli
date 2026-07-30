//! Common Lisp empty-`let` detection: a `let` whose binding list is empty —
//! `(let () body…)` or `(let nil body…)`. With no bindings established, the
//! `let` does nothing but evaluate its body in order, which is exactly what
//! `progn` does: `(let () a b)` is `(progn a b)`. The `let` wrapper is pure
//! noise, usually left behind after every binding is removed.
//!
//! Only bare `let` is handled. `(let* () …)` is first reduced to `(let () …)`
//! by the `redundant-let-star` rule (a `let*` with no bindings is a `let`), so
//! scoping here to `let` keeps the two rules from emitting overlapping fixes on
//! the same head and lets the empty-`let*` case compose through in two passes.
//!
//! A `let` whose body *leads* with a `(declare …)` is left alone: an
//! empty-binding `let` still establishes a declaration scope, but `progn` has
//! none, so `(progn (declare …) …)` would be malformed. An empty-body
//! `(let ())` is left alone too (there is nothing to splice).
//!
//! The fix rewrites the `(let ()` prefix as `(progn`, leaving the body
//! byte-identical, so the rule is auto-fixable.
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

/// Whether `view` is an empty binding list: `()` (a paren list with no
/// children) or the bare atom `nil`.
fn is_empty_binding_list(view: &ExpressionView) -> bool {
    if is_paren_list(view) {
        return view.children.is_empty();
    }
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

/// Whether `view` is a `(declare …)` form (valid in a `let` body, invalid in a
/// `progn`).
fn is_declare_form(view: &ExpressionView) -> bool {
    list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("declare"))
}

#[derive(Debug, Clone)]
pub struct EmptyLetItem {
    /// The span of the whole `(let () body…)` form.
    pub span: ByteSpan,
    /// The span of the `(let ()` prefix (form start through the binding list),
    /// which the fix replaces with `(progn`.
    ///
    /// The rewrite's input, not the report's: the lint rule reads it to splice
    /// `(progn` in, and neither the old report nor this one printed it.
    pub prefix_span: ByteSpan,
}

impl Finding for EmptyLetItem {
    /// The rule's name. Every finding here is the one shape — a `let` with no
    /// bindings — so there is no closed set to discriminate on.
    fn kind(&self) -> &'static str {
        "empty-let"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the path and location: the old text row carried only
    /// those, and the finding has no field the report published. `message` is
    /// what a reader gets here.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `empty-let` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "let with no bindings is just progn; (let () body) is (progn body)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_let(
    view: &ExpressionView,
    let_form_count: &mut usize,
    violations: &mut Vec<EmptyLetItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("let") {
        return;
    }
    *let_form_count += 1;

    // children: [let, binding-list, body…] — need the binding list and a body.
    if view.children.len() < 3 {
        return;
    }
    let binding = &view.children[1];
    if !is_empty_binding_list(binding) {
        return;
    }
    // A leading declaration cannot survive the move to progn.
    if is_declare_form(&view.children[2]) {
        return;
    }

    let prefix_span = ByteSpan::new(view.span.start(), binding.span.end());
    violations.push(EmptyLetItem {
        span: view.span,
        prefix_span,
    });
}

/// Collects every empty-binding `let` in one file, with the number of `let`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no empty `let` here" for Common Lisp and
/// "nothing was looked for" for Clojure, and the two read identically without
/// the flag.
pub fn build_empty_let_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EmptyLetItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("let_form_count", json!(0))],
        ));
    }

    let mut let_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let(subview, &mut let_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("let_form_count", json!(let_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<EmptyLetItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_empty_let_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build empty let report")
    }

    /// The `(let_form_count, violations)` pair the report is built from.
    fn lets(input: &str) -> (u64, Vec<EmptyLetItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "let_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("let_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_empty_paren_bindings() {
        let source = "(let () (foo) (bar))";
        let (count, violations) = lets(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].prefix_span), "(let ()");
    }

    #[test]
    fn flags_nil_bindings() {
        let source = "(let nil (foo))";
        let (_, violations) = lets(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].prefix_span), "(let nil");
    }

    #[test]
    fn does_not_flag_non_empty_bindings() {
        let (count, violations) = lets("(let ((x 1)) x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_let_star() {
        // (let* () …) is redundant-let-star's job (it becomes (let () …) first).
        let (count, violations) = lets("(let* () (foo))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_leading_declare() {
        // (progn (declare …) …) would be malformed.
        let (_, violations) = lets("(let () (declare (ignore x)) (foo))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_body() {
        let (count, violations) = lets("(let ())");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = lets("(LET () (foo))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_empty_let() {
        let (_, violations) = lets("(defun f () (let () (side-effect) (result)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(let () (foo))", Dialect::Clojure).expect("parse");
        let report = build_empty_let_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build empty let report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("let_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(let ((x 1)) x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_leans_on_its_message() {
        let report = report("(defun f ()\n  (let () (side-effect) (result)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "empty-let");
        assert!(finding.text_columns().is_empty());
        assert!(finding.json_fields().is_empty());
        assert_eq!(
            finding.message(),
            "let with no bindings is just progn; (let () body) is (progn body)"
        );
    }

    #[test]
    fn the_summary_counts_every_let_scanned_not_only_the_flagged_ones() {
        let report = report("(let () (foo))\n(let ((x 1)) x)\n(let nil (bar))\n");
        assert_eq!(report.summary, vec![("let_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
