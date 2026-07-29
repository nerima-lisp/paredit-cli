//! Common Lisp manual-`pushnew` detection: a `setf`/`setq` that assigns a
//! variable the result of `adjoin`ing a new element onto that same variable —
//! `(setf set (adjoin item set))`, `(setq seen (adjoin k seen :test #'equal))`.
//! This re-implements by hand exactly what the `pushnew` modify macro expresses
//! (`pushnew` is *defined* as `(setf place (adjoin item place …))`), and
//! `pushnew` states the intent ("add to this set if new") directly.
//!
//! The rewrite is only offered when the assigned place is a *bare variable*
//! (a symbol): a symbol place has no subforms, so `(pushnew E P)` and the
//! hand-written `setf` evaluate identically. A compound place would evaluate its
//! subforms twice under the `setf` but once under `pushnew`, so such forms are
//! left alone.
//!
//! Shape matched (with `P` the assigned variable, `E` any element form, and any
//! trailing `:test`/`:key`/`:test-not` keyword arguments passed through):
//!
//!   - `(setf P (adjoin E P KW…))` → `(pushnew E P KW…)`
//!
//! `adjoin` takes `(adjoin item list &key …)`, so a `pushnew` requires the
//! *second* operand to be the place; `(adjoin P other)` (adjoining the place as
//! the element) is not flagged. Only the single assignment pair is handled; a
//! multi-pair `setf` and any reader-conditional operand are skipped, and `P` is
//! matched to the adjoin list by exact source text.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so it does not count as one settled operand.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `value` is `(adjoin E P KW…)` for the variable named `place_text`, returns
/// the span covering all of `adjoin`'s operands (`E P KW…`), which is exactly
/// the argument list `pushnew` needs. Requires at least two operands with the
/// second being the place.
fn adjoin_pushnew_args(value: &ExpressionView, place_text: &str) -> Option<ByteSpan> {
    if !is_paren_list(value) {
        return None;
    }
    if value.children.iter().skip(1).any(is_reader_conditional) {
        return None;
    }
    let head = list_head(value)?;
    if !head.eq_ignore_ascii_case("adjoin") {
        return None;
    }
    let operands = &value.children[1..];
    if operands.len() < 2 {
        return None;
    }
    // `(adjoin item list …)`: the list (second operand) must be the place.
    if atom_text(&operands[1]) != Some(place_text) {
        return None;
    }
    let first = operands.first()?;
    let last = operands.last()?;
    Some(ByteSpan::new(first.span.start(), last.span.end()))
}

#[derive(Debug, Clone)]
pub struct ManualPushnewItem {
    /// The span of the whole `(setf P (adjoin E P …))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span covering `adjoin`'s operand list (`E P KW…`), reused verbatim as
    /// `pushnew`'s argument list.
    ///
    /// The rewrite's input, not the report's: the lint rule slices it to build
    /// `(pushnew E P KW…)`, and the command never printed it.
    pub args_span: ByteSpan,
}

impl Finding for ManualPushnewItem {
    /// The rule's own name. Every finding here is the same rewrite — a
    /// hand-written `adjoin` onto its own place — with nothing to sub-divide it
    /// by.
    fn kind(&self) -> &'static str {
        "manual-pushnew"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `manual-pushnew` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "setf adjoins onto a variable; use pushnew".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_assignment(
    view: &ExpressionView,
    source: &str,
    assignment_form_count: &mut usize,
    violations: &mut Vec<ManualPushnewItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("setf") && !head.eq_ignore_ascii_case("setq") {
        return;
    }
    *assignment_form_count += 1;

    // children: [setf, place, value] — exactly one assignment pair.
    if view.children.len() != 3 {
        return;
    }
    let place = &view.children[1];
    if !place.reader_prefixes.is_empty() {
        return;
    }
    // The place must be a bare variable (symbol) for the rewrite to be sound.
    let Some(place_text) = atom_text(place) else {
        return;
    };
    let value = &view.children[2];
    let Some(args_span) = adjoin_pushnew_args(value, place_text) else {
        return;
    };

    violations.push(ManualPushnewItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        args_span,
    });
}

/// Collects every manual pushnew in one file, with the number of `setf`/`setq`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no hand-written pushnew" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_manual_pushnew_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ManualPushnewItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("assignment_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(subview, source, &mut assignment_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("assignment_form_count", json!(assignment_form_count))],
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

    fn report(input: &str) -> FileFindings<ManualPushnewItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_manual_pushnew_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build manual pushnew report")
    }

    /// The `(assignment_form_count, violations)` pair the report is built from.
    fn pushnews(input: &str) -> (u64, Vec<ManualPushnewItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "assignment_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("assignment_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_setf_adjoin_onto_self() {
        let source = "(setf set (adjoin item set))";
        let (count, violations) = pushnews(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].args_span), "item set");
    }

    #[test]
    fn args_span_includes_keyword_arguments() {
        let source = "(setf seen (adjoin k seen :test #'equal))";
        let (_, violations) = pushnews(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].args_span),
            "k seen :test #'equal"
        );
    }

    #[test]
    fn flags_setq_adjoin() {
        let (_, violations) = pushnews("(setq xs (adjoin (compute) xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_adjoin_with_place_as_element() {
        let (_, violations) = pushnews("(setf xs (adjoin xs other))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_adjoin_onto_other_variable() {
        let (_, violations) = pushnews("(setf xs (adjoin item ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_compound_place() {
        let (_, violations) = pushnews("(setf (slot obj) (adjoin item (slot obj)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_pair_setf() {
        let (_, violations) = pushnews("(setf xs (adjoin a xs) ys (adjoin b ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_cons_which_is_manual_push() {
        // (cons …) is manual-push's job, not this rule's.
        let (_, violations) = pushnews("(setf xs (cons item xs))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_setf_and_adjoin() {
        let (_, violations) = pushnews("(SETF xs (ADJOIN item xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_manual_pushnew() {
        let (_, violations) = pushnews("(defun note (k) (setf keys (adjoin k keys)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(setf xs (adjoin item xs))", Dialect::Clojure)
            .expect("parse");
        let report = build_manual_pushnew_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build manual pushnew report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("assignment_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(setf xs (reverse xs))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_leaves_the_description_to_its_message() {
        let report = report("(defun note (k)\n  (setf keys (adjoin k keys)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "manual-pushnew");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.message(),
            "setf adjoins onto a variable; use pushnew"
        );
    }

    #[test]
    fn the_summary_counts_every_assignment_scanned_not_only_the_flagged_ones() {
        let report = report("(setf xs (adjoin a xs))\n(setf ys (reverse ys))\n");
        assert_eq!(report.summary, vec![("assignment_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
