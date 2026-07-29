//! Common Lisp `car`-of-`reverse` detection: a `(car (reverse x))` (or
//! `(first (reverse x))`). Taking the first element of the reversed sequence is
//! the *last* element of the original — but `reverse` builds a whole fresh copy
//! just to read one element. `(car (last x))` yields the same element (the last,
//! or `nil` when `x` is empty) without the O(n) allocation, so
//! `(car (reverse x))` is `(car (last x))`.
//!
//! Only the non-destructive `reverse` is matched. `nreverse` is excluded — it
//! mutates `x`, so `(car (nreverse x))` is not equivalent to the copy-free
//! `(car (last x))`. The outer accessor's `car`/`first` spelling is preserved. A
//! `reverse` with the wrong arity and a reader-conditional operand are left
//! alone.
//!
//! The fix rewrites `(car (reverse x))` as `(car (last x))` (keeping the outer
//! accessor), copying `x`'s source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct CarReverseItem {
    /// The span of the whole `(car (reverse x))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the outer accessor token (`car`/`first`), preserved in the fix.
    pub accessor_span: ByteSpan,
    /// The span of the sequence operand `x`.
    pub list_span: ByteSpan,
}

impl Finding for CarReverseItem {
    /// The rule's name, not the accessor.
    ///
    /// The outer accessor is `car` or `first` and this report carries it as a
    /// span rather than a name, so there is no `&'static str` to hand back;
    /// splitting the kind on it would also imply the two are different defects,
    /// which they are not.
    fn kind(&self) -> &'static str {
        "car-reverse"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing: the old text row carried only the path and the offset, both of
    /// which the envelope prints itself. The `message` override is what a
    /// reader of a text row has to go on.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// The accessor and sequence spans, which this report published before it
    /// moved onto the envelope. They are also the rewrite's inputs, but a
    /// consumer that wants to point at the accessor token — the `car`/`first`
    /// spelling the fix preserves — rather than the whole call already reads
    /// them.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("accessor_span", span_json(self.accessor_span)),
            ("list_span", span_json(self.list_span)),
        ]
    }

    /// The same sentence the `car-reverse` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "car of a reverse copies the whole list to read one element; use (car (last x))".to_owned()
    }
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    accessor_form_count: &mut usize,
    violations: &mut Vec<CarReverseItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("car") && !head.eq_ignore_ascii_case("first") {
        return;
    }
    *accessor_form_count += 1;

    // children: [car/first, inner] — the accessor takes exactly one argument.
    if view.children.len() != 2 {
        return;
    }
    let inner = &view.children[1];
    if !is_paren_list(inner) {
        return;
    }
    let Some(inner_head) = list_head(inner) else {
        return;
    };
    if !inner_head.eq_ignore_ascii_case("reverse") {
        return;
    }
    // inner children: [reverse, list] — reverse takes exactly one argument.
    if inner.children.len() != 2 {
        return;
    }
    let list = &inner.children[1];
    if is_reader_conditional(list) {
        return;
    }

    violations.push(CarReverseItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        accessor_span: view.children[0].span,
        list_span: list.span,
    });
}

/// Collects every `(car (reverse x))`/`(first (reverse x))` in one file, with
/// the number of `car`/`first` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no car of a reverse here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn collect_car_reverses(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CarReverseItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("accessor_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut accessor_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut accessor_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("accessor_form_count", json!(accessor_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CarReverseItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_car_reverses(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect car reverses")
    }

    /// The `(accessor_form_count, violations)` pair the report is built from.
    fn accessors(input: &str) -> (u64, Vec<CarReverseItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "accessor_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("accessor_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_car_reverse() {
        let source = "(car (reverse items))";
        let (count, violations) = accessors(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].accessor_span), "car");
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn flags_first_reverse_and_preserves_accessor() {
        let source = "(first (reverse xs))";
        let (_, violations) = accessors(source);
        assert_eq!(slice(source, violations[0].accessor_span), "first");
    }

    #[test]
    fn preserves_compound_list() {
        let source = "(car (reverse (mapcar #'f ys)))";
        let (_, violations) = accessors(source);
        assert_eq!(slice(source, violations[0].list_span), "(mapcar #'f ys)");
    }

    #[test]
    fn does_not_flag_nreverse() {
        // (car (nreverse x)) mutates x; not equivalent to the copy-free (car (last x)).
        assert!(accessors("(car (nreverse xs))").1.is_empty());
    }

    #[test]
    fn does_not_flag_plain_reverse() {
        assert!(accessors("(reverse xs)").1.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_reverse_arity() {
        assert!(accessors("(car (reverse))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = accessors("(CAR (REVERSE xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = accessors("(defun f (xs) (car (reverse xs)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(car (reverse xs))", Dialect::Clojure).expect("parse");
        let report = collect_car_reverses(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect car reverses");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("accessor_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(car xs)").dialect_modelled);
    }

    /// Both sub-spans survive the move onto the envelope: they were in the old
    /// JSON, and `accessor_span` is the only place the `car`/`first` spelling
    /// is reported at all.
    #[test]
    fn a_finding_carries_its_line_and_both_sub_spans() {
        let source = "(defun f (xs)\n  (first (reverse xs)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "car-reverse");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![
                ("accessor_span", span_json(finding.accessor_span)),
                ("list_span", span_json(finding.list_span)),
            ]
        );
        assert_eq!(slice(source, finding.accessor_span), "first");
        assert_eq!(slice(source, finding.list_span), "xs");
    }

    #[test]
    fn the_summary_counts_every_accessor_scanned_not_only_the_flagged_ones() {
        let report = report("(car (reverse xs))\n(car xs)\n");
        assert_eq!(report.summary, vec![("accessor_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
