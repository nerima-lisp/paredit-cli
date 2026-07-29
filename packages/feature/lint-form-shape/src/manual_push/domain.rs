//! Common Lisp manual-`push` detection: a `setf`/`setq` that assigns a variable
//! the result of consing a new element onto that same variable —
//! `(setf stack (cons item stack))`, `(setq xs (cons x xs))`. This re-implements
//! by hand exactly what the `push` modify macro expresses, and `push` states the
//! intent ("prepend to this place") directly.
//!
//! The rewrite is only offered when the assigned place is a *bare variable*
//! (a symbol). That is the condition under which `(setf P (cons E P))` and
//! `(push E P)` are unconditionally equivalent: a symbol place is read and
//! written with no subforms to evaluate, so nothing is duplicated. A compound
//! place like `(car node)` would evaluate `node` twice under the hand-written
//! `setf` but once under `push`, so such forms are deliberately left alone.
//!
//! Shape matched (with `P` the assigned variable and `E` any single form):
//!
//!   - `(setf P (cons E P))` / `(setq P (cons E P))` → `(push E P)`
//!
//! `cons` takes `(cons element list)`, so a push requires the *second* operand
//! to be the place; `(cons P E)` (consing the place onto something else) is not
//! a push and is not flagged. Only the single assignment pair is handled; a
//! multi-pair `setf` and any reader-conditional operand are skipped, and `P` is
//! matched to the cons tail by exact source text.
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

/// If `value` is `(cons E P)` for the variable named `place_text`, returns the
/// span of the pushed element `E`. Requires exactly two operands, the second
/// being the place.
fn cons_push_element(value: &ExpressionView, place_text: &str) -> Option<ByteSpan> {
    if !is_paren_list(value) {
        return None;
    }
    if value.children.iter().skip(1).any(is_reader_conditional) {
        return None;
    }
    let head = list_head(value)?;
    if !head.eq_ignore_ascii_case("cons") {
        return None;
    }
    let operands = &value.children[1..];
    if operands.len() != 2 {
        return None;
    }
    // `(cons element list)`: the list (second operand) must be the place.
    (atom_text(&operands[1]) == Some(place_text)).then_some(operands[0].span)
}

#[derive(Debug, Clone)]
pub struct ManualPushItem {
    /// The span of the whole `(setf P (cons E P))` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the assigned variable `P` (for reconstructing the fix).
    ///
    /// The rewrite's input, not the report's: the lint rule slices it to build
    /// `(push E P)`, and the command never printed it.
    pub place_span: ByteSpan,
    /// The span of the pushed element `E`.
    ///
    /// The rewrite's input, not the report's, for the same reason as
    /// `place_span`.
    pub element_span: ByteSpan,
}

impl Finding for ManualPushItem {
    /// The rule's own name. Every finding here is the same rewrite — a
    /// hand-written `cons` onto its own place — with nothing to sub-divide it
    /// by.
    fn kind(&self) -> &'static str {
        "manual-push"
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

    /// The same sentence the `manual-push` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "setf conses onto a variable; use push".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_assignment(
    view: &ExpressionView,
    source: &str,
    assignment_form_count: &mut usize,
    violations: &mut Vec<ManualPushItem>,
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
    let Some(element_span) = cons_push_element(value, place_text) else {
        return;
    };

    violations.push(ManualPushItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        place_span: place.span,
        element_span,
    });
}

/// Collects every manual push in one file, with the number of `setf`/`setq`
/// forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no hand-written push" for Common Lisp and
/// "nothing was looked for" for Clojure, and the two read identically without
/// the flag.
pub fn build_manual_push_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ManualPushItem>> {
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

    fn report(input: &str) -> FileFindings<ManualPushItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_manual_push_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build manual push report")
    }

    /// The `(assignment_form_count, violations)` pair the report is built from.
    fn pushes(input: &str) -> (u64, Vec<ManualPushItem>) {
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
    fn flags_setf_cons_onto_self() {
        let source = "(setf stack (cons item stack))";
        let (count, violations) = pushes(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].element_span), "item");
        assert_eq!(slice(source, violations[0].place_span), "stack");
    }

    #[test]
    fn flags_setq_cons() {
        let (_, violations) = pushes("(setq xs (cons (compute) xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_cons_with_place_as_element() {
        // (cons xs other) conses the place as the element, not onto it.
        let (_, violations) = pushes("(setf xs (cons xs other))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_cons_onto_other_variable() {
        let (_, violations) = pushes("(setf xs (cons item ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_compound_place() {
        let (_, violations) = pushes("(setf (car node) (cons item (car node)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_argument_cons() {
        // A malformed 3-arg cons is not a push; leave it alone.
        let (_, violations) = pushes("(setf xs (cons a b xs))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_multi_pair_setf() {
        let (_, violations) = pushes("(setf xs (cons a xs) ys (cons b ys))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_unrelated_value() {
        let (_, violations) = pushes("(setf xs (reverse xs))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_setf_and_cons() {
        let (_, violations) = pushes("(SETF xs (CONS item xs))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_manual_push() {
        let (_, violations) = pushes("(defun record (item) (setf log (cons item log)))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(setf xs (cons item xs))", Dialect::Clojure)
            .expect("parse");
        let report = build_manual_push_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build manual push report");
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
        let report = report("(defun record (item)\n  (setf log (cons item log)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "manual-push");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
        assert_eq!(finding.message(), "setf conses onto a variable; use push");
    }

    #[test]
    fn the_summary_counts_every_assignment_scanned_not_only_the_flagged_ones() {
        let report = report("(setf xs (cons a xs))\n(setf ys (reverse ys))\n");
        assert_eq!(report.summary, vec![("assignment_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
