//! Common Lisp `incf`/`decf`-by-`0` detection: an `(incf place 0)` or
//! `(decf place 0)` whose step is the literal `0`. Adding or subtracting `0`
//! changes nothing, so the form evaluates `place` and returns its value without
//! modifying it — almost always a mistake (a forgotten step, or a `0` left after
//! editing). This is a report-only smell: silently dropping the form could
//! discard a place's side effects, so no automatic fix is offered.
//!
//! Only the bare integer literal `0` is matched. A float `0.0` (which can coerce
//! the place to a float), a non-zero step, and a reader-conditional operand are
//! left alone.
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

/// Whether `view` is the bare integer `0` literal (no reader prefixes; `0.0` is a
/// different spelling, excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct StepZeroItem {
    /// The span of the whole `(incf place 0)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The operator, lowercased (`incf` or `decf`).
    pub operator: &'static str,
}

impl Finding for StepZeroItem {
    /// Which direction the no-op step was going, so `incf` and `decf` are
    /// separable without parsing JSON.
    ///
    /// A closed set of two, already normalized to lowercase `&'static str` by
    /// [`examine`] — the source casing is not retained, so this is the
    /// canonical name rather than whatever the file spelled.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the leading `kind`: the old text row's only column was
    /// the bare operator, which the `kind` column now carries.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `step-zero` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("{} by 0 is a no-op that changes nothing", self.operator)
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    step_form_count: &mut usize,
    violations: &mut Vec<StepZeroItem>,
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

    // children: [incf, place, step] — require the explicit-step shape.
    if view.children.len() != 3 {
        return;
    }
    let place = &view.children[1];
    let step = &view.children[2];
    if is_reader_conditional(place) || is_reader_conditional(step) {
        return;
    }
    if !is_zero_literal(step) {
        return;
    }

    violations.push(StepZeroItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        operator,
    });
}

/// Collects every `(incf place 0)`/`(decf place 0)` in one file, with the
/// number of `incf`/`decf` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no zero step here" for Common Lisp and
/// "nothing was looked for" for Clojure, and the two read identically without
/// the flag.
pub fn build_step_zero_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<StepZeroItem>> {
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
            examine(subview, source, &mut step_form_count, &mut violations);
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

    fn report(input: &str) -> FileFindings<StepZeroItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_step_zero_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build step zero report")
    }

    /// The `(step_form_count, violations)` pair the report is built from.
    fn steps(input: &str) -> (u64, Vec<StepZeroItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "step_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("step_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_incf_zero() {
        let (count, violations) = steps("(incf counter 0)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "incf");
    }

    #[test]
    fn flags_decf_zero() {
        let (_, violations) = steps("(decf x 0)");
        assert_eq!(violations[0].operator, "decf");
    }

    #[test]
    fn does_not_flag_nonzero_step() {
        let (count, violations) = steps("(incf x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_zero() {
        assert!(steps("(incf x 0.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_default_step() {
        // (incf x) has no explicit step; not this rule's shape.
        assert!(steps("(incf x)").1.is_empty());
    }

    #[test]
    fn flags_uppercase_head() {
        let (_, violations) = steps("(INCF x 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = steps("(defun f (x) (incf x 0))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(incf x 0)", Dialect::Clojure).expect("parse");
        let report = build_step_zero_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build step zero report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("step_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(incf x 2)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun bump (x)\n  (decf x 0))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "decf");
        assert_eq!(finding.json_fields(), vec![("operator", json!("decf"))]);
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.message(),
            "decf by 0 is a no-op that changes nothing"
        );
    }

    #[test]
    fn the_summary_counts_every_step_form_scanned_not_only_the_flagged_ones() {
        let report = report("(incf x 0)\n(incf y 2)\n(decf z)\n");
        assert_eq!(report.summary, vec![("step_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
