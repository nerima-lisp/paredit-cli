//! Common Lisp literal-zero-divisor detection: a division-family form with a
//! literal `0` in a divisor position, which always signals `division-by-zero` at
//! run time. `(/ x 0)`, `(mod x 0)`, `(rem x 0)`, `(floor x 0)`, … can never
//! succeed, so this is a report-only bug (there is no meaningful rewrite).
//!
//! Divisor positions depend on the operator:
//!
//!   - `/`: with one argument `(/ d)` is `1/d`, so the sole argument is the
//!     divisor; with two or more `(/ n d …)` every argument after the first is a
//!     divisor.
//!   - `mod`/`rem`: the second (and only other) argument is the divisor.
//!   - `floor`/`ceiling`/`truncate`/`round` and their `f`-variants: the optional
//!     second argument is the divisor, flagged only when present.
//!
//! Only the bare integer/ratio literal `0` is matched (a float `0.0` divisor is a
//! different, defined-in-some-impls story and is left alone), and a
//! reader-conditional operand is left alone.
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

/// Operators whose second argument is the (optional) divisor.
const QUOTIENT_OPS: [&str; 10] = [
    "mod",
    "rem",
    "floor",
    "ceiling",
    "truncate",
    "round",
    "ffloor",
    "fceiling",
    "ftruncate",
    "fround",
];

/// Whether `view` is the bare integer `0` literal (no reader prefixes; `0.0` is a
/// different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// The child indices that sit in a divisor position for `head` with `child_count`
/// children (index 0 is the operator).
fn divisor_indices(head: &str, child_count: usize) -> Vec<usize> {
    if head == "/" {
        return match child_count {
            // (/ d) -> 1/d: the sole argument is the divisor.
            2 => vec![1],
            // (/ n d ...) -> every argument after the numerator divides.
            n if n >= 3 => (2..n).collect(),
            _ => Vec::new(),
        };
    }
    if QUOTIENT_OPS.contains(&head) && child_count == 3 {
        // (op number divisor): the divisor is the second argument.
        return vec![2];
    }
    Vec::new()
}

#[derive(Debug, Clone)]
pub struct ZeroDivisorItem {
    /// The span of the whole division form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator, lowercased.
    pub operator: String,
    /// The span of the literal `0` divisor.
    pub divisor_span: ByteSpan,
}

impl Finding for ZeroDivisorItem {
    /// The rule's own name.
    ///
    /// The operator would not do. `/` is punctuation, which makes a poor `grep`
    /// selector and a worse SARIF rule id, and the eleven heads are one bug
    /// either way — every one of them signals `division-by-zero`. The operator
    /// stays a reported field instead, which is also the only place it could
    /// live: it is an owned `String` and `kind` is `&'static str`.
    fn kind(&self) -> &'static str {
        "zero-divisor"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Bare rather than `operator=…`, which is the column this report's text
    /// row has always printed.
    fn text_columns(&self) -> Vec<String> {
        vec![self.operator.clone()]
    }

    /// `divisor_span` is kept: this rule is report-only, so the span of the
    /// literal `0` is not a fix's input but the one thing pointing at the
    /// offending operand inside a form the outer span only bounds.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("operator", json!(self.operator)),
            (
                "divisor_span",
                json!({
                    "start": self.divisor_span.start().get(),
                    "end": self.divisor_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `zero-divisor` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} by a literal 0 always signals division-by-zero",
            self.operator
        )
    }
}

/// Whether a divisor is provably zero.
///
/// The standalone `inspect zero-divisor` command reads only the literal `0`,
/// because it has no semantic tables to consult. The lint suite passes a test
/// that also resolves constants and folds arithmetic, so it sees
/// `(let ((z 0)) (/ x z))` and `(/ x (- 1 1))` — the same bug, spelled in a
/// way the reader alone cannot recognise.
pub type IsZeroDivisor<'a> = &'a dyn Fn(&ExpressionView) -> bool;

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    is_zero: IsZeroDivisor<'_>,
    division_form_count: &mut usize,
    violations: &mut Vec<ZeroDivisorItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let lower = head.to_ascii_lowercase();
    if lower != "/" && !QUOTIENT_OPS.contains(&lower.as_str()) {
        return;
    }
    *division_form_count += 1;

    for index in divisor_indices(&lower, view.children.len()) {
        let divisor = &view.children[index];
        if is_zero(divisor) {
            violations.push(ZeroDivisorItem {
                span: view.span,
                line: line_of(source, view.span.start().get()),
                operator: lower.clone(),
                divisor_span: divisor.span,
            });
            return;
        }
    }
}

/// Collects every division-family form with a literal `0` divisor in one file,
/// with the number of division forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no division by a literal 0 here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_zero_divisor_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ZeroDivisorItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("division_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut division_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(
                subview,
                source,
                &is_zero_literal,
                &mut division_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("division_form_count", json!(division_form_count))],
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

    fn report(input: &str) -> FileFindings<ZeroDivisorItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_zero_divisor_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build zero divisor report")
    }

    /// The `(division_form_count, violations)` pair the report is built from.
    fn divs(input: &str) -> (u64, Vec<ZeroDivisorItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "division_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("division_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_divide_by_zero() {
        let (count, violations) = divs("(/ x 0)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "/");
    }

    #[test]
    fn flags_reciprocal_of_zero() {
        // (/ 0) is 1/0.
        let (_, violations) = divs("(/ 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_later_zero_divisor() {
        let (_, violations) = divs("(/ n a 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_zero_numerator() {
        // (/ 0 x) is 0, perfectly fine.
        let (_, violations) = divs("(/ 0 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_mod_rem_zero() {
        assert_eq!(divs("(mod x 0)").1.len(), 1);
        assert_eq!(divs("(rem x 0)").1.len(), 1);
    }

    #[test]
    fn flags_quotient_op_zero_divisor() {
        assert_eq!(divs("(floor x 0)").1.len(), 1);
        assert_eq!(divs("(truncate x 0)").1.len(), 1);
    }

    #[test]
    fn does_not_flag_single_arg_quotient() {
        // (floor x) has no divisor argument.
        assert!(divs("(floor x)").1.is_empty());
    }

    #[test]
    fn does_not_flag_nonzero_or_float_divisor() {
        assert!(divs("(/ x 2)").1.is_empty());
        assert!(divs("(/ x 0.0)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        assert_eq!(divs("(MOD x 0)").1.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(/ x 0)", Dialect::Clojure).expect("parse");
        let report = build_zero_divisor_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build zero divisor report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("division_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(/ x 2)").dialect_modelled);
    }

    /// `divisor_span` points at the `0` itself, which the outer span only
    /// bounds; it stayed a reported field when this report moved onto the
    /// envelope.
    #[test]
    fn a_finding_carries_its_line_its_operator_and_its_divisor_span() {
        let source = "(defun f (x)\n  (mod x 0))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "zero-divisor");
        assert_eq!(finding.text_columns(), vec!["mod".to_owned()]);

        let start = finding.divisor_span.start().get();
        let end = finding.divisor_span.end().get();
        assert_eq!(&source[start..end], "0");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("operator", json!("mod")),
                ("divisor_span", json!({ "start": start, "end": end })),
            ]
        );
    }

    #[test]
    fn the_summary_counts_every_division_form_scanned_not_only_the_flagged_ones() {
        let report = report("(/ x 0)\n(/ x 2)\n(mod a b)\n");
        assert_eq!(report.summary, vec![("division_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
