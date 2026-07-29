//! Common Lisp explicit-step-delta detection: an `incf`/`decf` whose delta
//! operand is the literal `1`. The delta argument of `incf`/`decf` defaults to
//! `1`, so `(incf x 1)` is exactly `(incf x)` and `(decf x 1)` is exactly
//! `(decf x)` — same place, same step, same result. Dropping the redundant
//! delta states the unit step the way the macro was designed to express it.
//!
//! Only the bare integer literal `1` is matched. A float `1.0` is left alone:
//! `(incf x 1.0)` can coerce `x` from an integer to a float, so it is *not*
//! equivalent to `(incf x)`. A non-`1` delta, a `#x1`/prefixed spelling, a
//! variable delta, and a reader-conditional operand are all left alone.
//!
//! The fix rewrites `(incf place 1)` as `(incf place)` (and likewise for
//! `decf`), copying the head and place from their exact source, so the rule is
//! auto-fixable.
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
pub struct ExplicitStepDeltaItem {
    /// The span of the whole `(incf place 1)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the `incf`/`decf` head symbol (preserves its exact source).
    ///
    /// The rewrite's input, not the report's: the lint rule copies it into the
    /// shortened form, and the command never prints it.
    pub head_span: ByteSpan,
    /// The span of the place operand (for reconstructing the fix).
    ///
    /// Also rewrite-only, for the same reason as `head_span`.
    pub place_span: ByteSpan,
    /// The canonical operator name (`incf`/`decf`), for the finding message.
    pub operator: &'static str,
}

impl Finding for ExplicitStepDeltaItem {
    /// Which of the two step macros carries the redundant delta.
    ///
    /// Already canonical on the item — `examine_step` case-folds the head into
    /// one of two `&'static str`s — so a consumer can select `incf` or `decf`
    /// straight off the leading text column.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// None: the only thing the old text row carried past the location was the
    /// operator, and that now leads the row as the `kind`.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `explicit-step-delta` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        let operator = self.operator;
        format!("{operator} delta of 1 is the default; ({operator} x 1) is ({operator} x)")
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_step(
    view: &ExpressionView,
    source: &str,
    step_form_count: &mut usize,
    violations: &mut Vec<ExplicitStepDeltaItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let operator = if head.eq_ignore_ascii_case("incf") {
        "incf"
    } else if head.eq_ignore_ascii_case("decf") {
        "decf"
    } else {
        return;
    };
    *step_form_count += 1;

    // children: [incf, place, delta] — require exactly the two-operand shape.
    if view.children.len() != 3 {
        return;
    }
    let place = &view.children[1];
    let delta = &view.children[2];
    if is_reader_conditional(place) || is_reader_conditional(delta) {
        return;
    }
    if !is_one_literal(delta) {
        return;
    }

    violations.push(ExplicitStepDeltaItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        head_span: view.children[0].span,
        place_span: place.span,
        operator,
    });
}

/// Collects every explicit `1` delta on an `incf`/`decf` in one file, with the
/// number of `incf`/`decf` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant delta here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_explicit_step_delta_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ExplicitStepDeltaItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("step_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut step_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_step(subview, source, &mut step_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("step_form_count", json!(step_form_count))],
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

    fn report(input: &str) -> FileFindings<ExplicitStepDeltaItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_explicit_step_delta_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build explicit step delta report")
    }

    /// The `(step_form_count, violations)` pair the report is built from.
    fn steps(input: &str) -> (u64, Vec<ExplicitStepDeltaItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "step_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("step_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_incf_one() {
        let source = "(incf counter 1)";
        let (count, violations) = steps(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "incf");
        assert_eq!(slice(source, violations[0].place_span), "counter");
    }

    #[test]
    fn flags_decf_one() {
        let (_, violations) = steps("(decf n 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "decf");
    }

    #[test]
    fn preserves_a_compound_place() {
        let source = "(incf (aref v i) 1)";
        let (_, violations) = steps(source);
        assert_eq!(slice(source, violations[0].place_span), "(aref v i)");
    }

    #[test]
    fn does_not_flag_a_non_unit_delta() {
        let (count, violations) = steps("(incf x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_one() {
        // (incf x 1.0) can coerce x to a float; not equivalent to (incf x).
        let (_, violations) = steps("(incf x 1.0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_implicit_default() {
        // (incf x) is already the desired shape; nothing to simplify.
        let (count, violations) = steps("(incf x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_variable_delta() {
        let (_, violations) = steps("(incf x step)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = steps("(INCF x 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "incf");
    }

    #[test]
    fn finds_a_nested_step() {
        let (_, violations) = steps("(dolist (x xs) (decf total 1))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(incf x 1)", Dialect::Clojure).expect("parse");
        let report =
            build_explicit_step_delta_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build explicit step delta report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("step_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(incf x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun f ()\n  (decf n 1))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "decf");
        assert_eq!(finding.json_fields(), vec![("operator", json!("decf"))]);
        // The operator leads the row as the `kind`, so nothing follows it.
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_step_form_scanned_not_only_the_flagged_ones() {
        let report = report("(incf x 1)\n(incf y 2)\n(decf z)\n");
        assert_eq!(report.summary, vec![("step_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
