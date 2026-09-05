//! Common Lisp `append`-of-a-singleton detection: a two-argument `append` whose
//! first argument is a one-element `(list x)` — `(append (list x) rest)`.
//! `append` copies every list but the last and links the copy's final `cdr` to
//! the last argument, so `(append (list x) rest)` builds a single fresh cons
//! whose car is `x` and whose cdr is the shared `rest` — which is exactly
//! `(cons x rest)`. The rewrite is exact: same one fresh cons, same sharing of
//! `rest`, `x` and `rest` each evaluated once in the same left-to-right order.
//!
//! Only the narrow, provably-equivalent shape is flagged: exactly two `append`
//! arguments, the first a `(list x)` with exactly one element. A multi-element
//! `(list x y)` first argument is `(list* x y rest)`, not `cons`, so it is left
//! alone, as is a non-`list` first argument, a different argument count, and a
//! reader-conditional operand.
//!
//! The fix rewrites `(append (list x) rest)` as `(cons x rest)`, copying the
//! element and the tail from their exact source, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// Whether `view` is a one-element `(list x)` call; returns the element `x`.
fn singleton_list(view: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    if !head.eq_ignore_ascii_case("list") {
        return None;
    }
    // children: [list, element] — exactly one element.
    if view.children.len() != 2 {
        return None;
    }
    Some(&view.children[1])
}

#[derive(Debug, Clone)]
pub struct AppendListToConsItem {
    /// The span of the whole `(append (list x) rest)` form.
    pub span: ByteSpan,
    /// The span of the singleton element `x`.
    pub element_span: ByteSpan,
    /// The span of the tail `rest`.
    pub rest_span: ByteSpan,
}

impl Finding for AppendListToConsItem {
    fn kind(&self) -> &'static str {
        "append-list-to-cons"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing: the old text row carried only the path and the offset, both of
    /// which the envelope prints itself. The `message` override is what a
    /// reader of a text row has to go on.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// The two operand spans, which this report published before it moved onto
    /// the envelope. They are also the rewrite's inputs, but a consumer that
    /// highlights the element and the tail separately already reads them.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("element_span", span_json(self.element_span)),
            ("rest_span", span_json(self.rest_span)),
        ]
    }

    fn message(&self) -> String {
        "a one-element append is just a cons; (append (list x) rest) is (cons x rest)".to_owned()
    }
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

pub fn examine(
    view: &ExpressionView,
    append_form_count: &mut usize,
    violations: &mut Vec<AppendListToConsItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("append") {
        return;
    }
    *append_form_count += 1;

    // children: [append, first, rest] — exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let first = &view.children[1];
    let rest = &view.children[2];
    if is_reader_conditional(rest) {
        return;
    }
    let Some(element) = singleton_list(first) else {
        return;
    };
    if is_reader_conditional(element) {
        return;
    }

    violations.push(AppendListToConsItem {
        span: view.span,
        element_span: element.span,
        rest_span: rest.span,
    });
}

/// Collects every `(append (list x) rest)` in one file, with the number of
/// `append` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn collect_append_list_to_cons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<AppendListToConsItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("append_form_count", json!(0))],
        ));
    }

    let mut append_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut append_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("append_form_count", json!(append_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<AppendListToConsItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_append_list_to_cons(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect append list to cons")
    }

    fn appends(input: &str) -> (u64, Vec<AppendListToConsItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "append_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("append_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_append_singleton() {
        let source = "(append (list x) rest)";
        let (count, violations) = appends(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].element_span), "x");
        assert_eq!(slice(source, violations[0].rest_span), "rest");
    }

    #[test]
    fn preserves_compound_element_and_tail() {
        let source = "(append (list (car a)) (cdr b))";
        let (_, violations) = appends(source);
        assert_eq!(slice(source, violations[0].element_span), "(car a)");
        assert_eq!(slice(source, violations[0].rest_span), "(cdr b)");
    }

    #[test]
    fn does_not_flag_multi_element_list() {
        // (append (list x y) rest) is (list* x y rest), not cons.
        let (_, violations) = appends("(append (list x y) rest)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_list_first_argument() {
        assert!(appends("(append xs rest)").1.is_empty());
        assert!(appends("(append (reverse xs) rest)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_argument_count() {
        assert!(appends("(append (list x))").1.is_empty());
        assert!(appends("(append (list x) a b)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = appends("(APPEND (LIST x) rest)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = appends("(defun f (x r) (append (list x) r))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(append (list x) rest)", Dialect::Clojure)
            .expect("parse");
        let report = collect_append_list_to_cons(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect append list to cons");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("append_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(append xs rest)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_operand_spans() {
        let report = report("(defun f (x r)\n  (append (list x) r))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "append-list-to-cons");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![
                ("element_span", span_json(finding.element_span)),
                ("rest_span", span_json(finding.rest_span)),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_append_scanned_not_only_the_flagged_ones() {
        let report = report("(append (list x) r)\n(append xs ys)\n");
        assert_eq!(report.summary, vec![("append_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
