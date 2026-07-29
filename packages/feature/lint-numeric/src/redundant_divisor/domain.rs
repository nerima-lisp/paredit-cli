//! Common Lisp redundant-divisor detection: a two-argument quotient operation
//! whose divisor is the literal integer `1`. `floor`, `ceiling`, `truncate`,
//! `round` and their float-returning variants `ffloor`, `fceiling`, `ftruncate`,
//! `fround` all take an optional `divisor` that defaults to `1`, so
//! `(floor x 1)` is exactly `(floor x)` — same quotient, same remainder, same
//! two returned values. Dropping the redundant `1` states the unit divisor the
//! way the operator was designed to express it.
//!
//! Only the bare integer literal `1` is matched. A float `1.0` is left alone:
//! `(floor 3 1.0)` returns a float remainder `0.0` while `(floor 3)` returns the
//! integer `0`, so the two are *not* equivalent. A non-`1` divisor, a
//! `#x1`/prefixed spelling, a variable divisor, and a reader-conditional operand
//! are all left alone. `mod`/`rem` are excluded — they require two arguments and
//! have no defaultable divisor.
//!
//! The fix rewrites `(floor x 1)` as `(floor x)`, copying the operator and
//! number operand from their exact source, so the rule is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The quotient operators whose divisor defaults to `1`.
const QUOTIENT_OPS: [&str; 8] = [
    "floor",
    "ceiling",
    "truncate",
    "round",
    "ffloor",
    "fceiling",
    "ftruncate",
    "fround",
];

/// Whether `view` is the bare integer `1` literal (no reader prefixes, so `#x1`
/// and a prefixed `,1` are excluded; `1.0` is a different spelling, excluded).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("1")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct RedundantDivisorItem {
    /// The span of the whole `(floor x 1)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator, lowercased (`floor`, `ceiling`, ...).
    pub operator: &'static str,
    /// The span of the operator token (preserves the source casing).
    ///
    /// The rewrite's input, not the report's: the lint rule copies the operator
    /// as written into the shortened form, and the command never prints it.
    pub operator_span: ByteSpan,
    /// The span of the number operand (for reconstructing the fix).
    pub number_span: ByteSpan,
}

impl Finding for RedundantDivisorItem {
    /// The quotient operator, which is already one of eight lowercase names.
    /// They round differently, so which one carried the redundant divisor is
    /// part of what the finding says.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the leading `kind`: the old text row carried the operator
    /// and no other column, and that operator is now the kind.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// `number_span` is a fix input, but the old report published it, so it
    /// stays published.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            (
                "number_span",
                json!({
                    "start": self.number_span.start().get(),
                    "end": self.number_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `redundant-divisor` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "the divisor defaults to 1; ({} x 1) is ({} x)",
            self.operator, self.operator
        )
    }
}

/// The canonical (lowercase) operator name if `head` is a quotient operator.
fn quotient_operator(head: &str) -> Option<&'static str> {
    QUOTIENT_OPS
        .iter()
        .copied()
        .find(|op| head.eq_ignore_ascii_case(op))
}

pub fn examine(
    view: &ExpressionView,
    source: &str,
    quotient_form_count: &mut usize,
    violations: &mut Vec<RedundantDivisorItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = quotient_operator(head) else {
        return;
    };
    *quotient_form_count += 1;

    // children: [op, number, divisor] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let number = &view.children[1];
    let divisor = &view.children[2];
    if is_reader_conditional(number) || is_reader_conditional(divisor) {
        return;
    }
    if !is_one_literal(divisor) {
        return;
    }

    violations.push(RedundantDivisorItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        operator,
        operator_span: view.children[0].span,
        number_span: number.span,
    });
}

/// Collects every `(op x 1)` quotient form in one file, with the number of
/// quotient forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no unit divisor here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_redundant_divisor_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantDivisorItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("quotient_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut quotient_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut quotient_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("quotient_form_count", json!(quotient_form_count))],
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

    fn report(input: &str) -> FileFindings<RedundantDivisorItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_divisor_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant divisor report")
    }

    /// The `(quotient_form_count, violations)` pair the report is built from.
    fn quotients(input: &str) -> (u64, Vec<RedundantDivisorItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "quotient_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("quotient_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_floor_by_one() {
        let source = "(floor total 1)";
        let (count, violations) = quotients(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "floor");
        assert_eq!(slice(source, violations[0].number_span), "total");
    }

    #[test]
    fn flags_every_quotient_operator() {
        for op in [
            "ceiling",
            "truncate",
            "round",
            "ffloor",
            "fceiling",
            "ftruncate",
            "fround",
        ] {
            let source = format!("({op} x 1)");
            let (_, violations) = quotients(&source);
            assert_eq!(violations.len(), 1, "expected {op} flagged");
            assert_eq!(violations[0].operator, op);
        }
    }

    #[test]
    fn preserves_operator_source_casing() {
        let source = "(FLOOR x 1)";
        let (_, violations) = quotients(source);
        assert_eq!(slice(source, violations[0].operator_span), "FLOOR");
    }

    #[test]
    fn does_not_flag_non_one_divisor() {
        let (count, violations) = quotients("(floor x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_one() {
        // (floor 3 1.0) yields a float remainder, unlike (floor 3).
        assert!(quotients("(floor x 1.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_one_argument_form() {
        // (floor x) already has no divisor.
        let (count, violations) = quotients("(floor x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_mod_or_rem() {
        assert!(quotients("(mod x 1)").1.is_empty());
        assert!(quotients("(rem x 1)").1.is_empty());
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = quotients("(defun f (x) (truncate x 1))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(floor x 1)", Dialect::Clojure).expect("parse");
        let report = build_redundant_divisor_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build redundant divisor report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("quotient_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(floor x)").dialect_modelled);
    }

    /// `number_span` is the fix's input, but the old report published it, so it
    /// is still published.
    #[test]
    fn a_finding_carries_its_line_its_operator_and_the_number_span() {
        let report = report("(defun f (x)\n  (floor x 1))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "floor");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("floor")),
                (
                    "number_span",
                    json!({
                        "start": finding.number_span.start().get(),
                        "end": finding.number_span.end().get(),
                    })
                ),
            ]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_quotient_form_scanned_not_only_the_flagged_ones() {
        let report = report("(floor x 1)\n(floor x 2)\n(round y 1)\n");
        assert_eq!(report.summary, vec![("quotient_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
