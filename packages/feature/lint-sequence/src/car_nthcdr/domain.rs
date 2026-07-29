//! Common Lisp `car`-of-`nthcdr` detection: a `(car (nthcdr n x))`. The
//! standard *defines* `nth` as the `car` of the `nthcdr`, so `(car (nthcdr n x))`
//! is exactly `(nth n x)` — same element, same nil-on-overrun, `n` and `x` each
//! evaluated once in the same order. The single `nth` accessor reads more
//! directly than the nested pair.
//!
//! Only the exact `(car (nthcdr n x))` two-level shape is matched: `car` with one
//! argument, whose argument is `(nthcdr n x)` with exactly two operands. `first`
//! is not matched here (the `car`/`first` spelling of the outer accessor is a
//! separate taste question); a `cdr`/other outer accessor, a wrong `nthcdr`
//! arity, and a reader-conditional operand are all left alone.
//!
//! The fix rewrites `(car (nthcdr n x))` as `(nth n x)`, copying the count and
//! list operands verbatim, so the rule is auto-fixable.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct CarNthcdrItem {
    /// The span of the whole `(car (nthcdr n x))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the count operand `n`.
    pub count_span: ByteSpan,
    /// The span of the list operand `x`.
    pub list_span: ByteSpan,
}

impl Finding for CarNthcdrItem {
    fn kind(&self) -> &'static str {
        "car-nthcdr"
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

    /// The two operand spans, which this report published before it moved onto
    /// the envelope. They are also the rewrite's inputs, but a consumer that
    /// highlights the count and the list separately already reads them.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("count_span", span_json(self.count_span)),
            ("list_span", span_json(self.list_span)),
        ]
    }

    /// The same sentence the `car-nthcdr` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "car of an nthcdr is nth; (car (nthcdr n x)) is (nth n x)".to_owned()
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
    car_form_count: &mut usize,
    violations: &mut Vec<CarNthcdrItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("car") {
        return;
    }
    *car_form_count += 1;

    // children: [car, inner] — car takes exactly one argument.
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
    if !inner_head.eq_ignore_ascii_case("nthcdr") {
        return;
    }
    // inner children: [nthcdr, count, list].
    if inner.children.len() != 3 {
        return;
    }
    let count = &inner.children[1];
    let list = &inner.children[2];
    if is_reader_conditional(count) || is_reader_conditional(list) {
        return;
    }

    violations.push(CarNthcdrItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        count_span: count.span,
        list_span: list.span,
    });
}

/// Collects every `(car (nthcdr n x))` in one file, with the number of `car`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no car of an nthcdr here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn collect_car_nthcdrs(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CarNthcdrItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("car_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut car_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut car_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("car_form_count", json!(car_form_count))],
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

    fn report(input: &str) -> FileFindings<CarNthcdrItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_car_nthcdrs(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect car nthcdrs")
    }

    /// The `(car_form_count, violations)` pair the report is built from.
    fn cars(input: &str) -> (u64, Vec<CarNthcdrItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "car_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("car_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_car_nthcdr() {
        let source = "(car (nthcdr n items))";
        let (count, violations) = cars(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].count_span), "n");
        assert_eq!(slice(source, violations[0].list_span), "items");
    }

    #[test]
    fn preserves_compound_operands() {
        let source = "(car (nthcdr (+ i 1) (rest xs)))";
        let (_, violations) = cars(source);
        assert_eq!(slice(source, violations[0].count_span), "(+ i 1)");
        assert_eq!(slice(source, violations[0].list_span), "(rest xs)");
    }

    #[test]
    fn does_not_flag_plain_nthcdr() {
        let (_, violations) = cars("(nthcdr n x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_cdr_outer() {
        let (_, violations) = cars("(cdr (nthcdr n x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_wrong_nthcdr_arity() {
        assert!(cars("(car (nthcdr n))").1.is_empty());
        assert!(cars("(car (nthcdr n x y))").1.is_empty());
    }

    #[test]
    fn flags_uppercase_heads() {
        let (_, violations) = cars("(CAR (NTHCDR n x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = cars("(defun f (n x) (car (nthcdr n x)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(car (nthcdr n x))", Dialect::Clojure).expect("parse");
        let report = collect_car_nthcdrs(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("collect car nthcdrs");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("car_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(car xs)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_operand_spans() {
        let report = report("(defun f (n x)\n  (car (nthcdr n x)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "car-nthcdr");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![
                ("count_span", span_json(finding.count_span)),
                ("list_span", span_json(finding.list_span)),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_car_scanned_not_only_the_flagged_ones() {
        let report = report("(car (nthcdr n x))\n(car xs)\n");
        assert_eq!(report.summary, vec![("car_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
