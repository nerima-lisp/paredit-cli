//! Common Lisp manual-`incf`/`decf` detection: a `setf`/`setq` that assigns a
//! variable a value which is that same variable plus or minus something —
//! `(setf x (1+ x))`, `(setq n (+ n 2))`, `(setf i (- i 1))`. These re-implement
//! by hand exactly what the `incf`/`decf` modify macros express, and the modify
//! macro states the intent ("bump this place") directly.
//!
//! The rewrite is only offered when the assigned place is a *bare variable*
//! (a symbol). That is the condition under which `(setf V (1+ V))` and
//! `(incf V)` are unconditionally equivalent: a symbol place is read and written
//! with no subforms to evaluate, so nothing is duplicated. A compound place like
//! `(aref a (f))` would evaluate `(f)` twice under the hand-written `setf` but
//! once under `incf`, so such forms are deliberately left alone.
//!
//! Shapes matched (with `V` the assigned variable, `D` any single form):
//!
//!   - `(setf V (1+ V))` / `(setq V (1+ V))` → `(incf V)`
//!   - `(setf V (1- V))`                     → `(decf V)`
//!   - `(setf V (+ V D))` / `(setf V (+ D V))` → `(incf V D)`  (`+` commutes)
//!   - `(setf V (- V D))`                     → `(decf V D)`  (`-` does not)
//!
//! Only the single assignment pair is handled; a multi-pair `setf` and any
//! reader-conditional operand are skipped (their shape or arity is not settled
//! statically). `V` is matched to the incremented operand by exact source text,
//! so a prefixed or differently-spelled operand does not match.
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

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so it does not count as one settled operand.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `value` is a `1+`/`1-`/`+`/`-` form that increments or decrements the
/// variable named `place_text`, returns the suggested modify macro and the span
/// of the delta operand (`None` for the `1+`/`1-` unit step).
fn incf_decf_rewrite(
    value: &ExpressionView,
    place_text: &str,
) -> Option<(&'static str, Option<ByteSpan>)> {
    if !is_paren_list(value) {
        return None;
    }
    if value.children.iter().skip(1).any(is_reader_conditional) {
        return None;
    }
    let head = list_head(value)?;
    let operands = &value.children[1..];

    match head {
        "1+" if operands.len() == 1 && atom_text(&operands[0]) == Some(place_text) => {
            Some(("incf", None))
        }
        "1-" if operands.len() == 1 && atom_text(&operands[0]) == Some(place_text) => {
            Some(("decf", None))
        }
        "+" if operands.len() == 2 => {
            if atom_text(&operands[0]) == Some(place_text) {
                Some(("incf", Some(operands[1].span)))
            } else if atom_text(&operands[1]) == Some(place_text) {
                // `+` commutes, so `(+ D V)` is still an increment of V by D.
                Some(("incf", Some(operands[0].span)))
            } else {
                None
            }
        }
        "-" if operands.len() == 2 && atom_text(&operands[0]) == Some(place_text) => {
            // `-` does not commute: only `(- V D)` decrements V.
            Some(("decf", Some(operands[1].span)))
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct ManualIncfItem {
    /// The span of the whole `(setf V …)` form.
    pub span: ByteSpan,
    /// The suggested modify macro (`incf` or `decf`).
    pub suggested_head: &'static str,
    /// The span of the assigned variable `V` (for reconstructing the fix).
    ///
    pub place_span: ByteSpan,
    /// The span of the delta operand `D`, or `None` for a `1+`/`1-` unit step.
    ///
    /// The rewrite's input, not the report's, for the same reason as
    /// `place_span`.
    pub delta_span: Option<ByteSpan>,
}

impl Finding for ManualIncfItem {
    /// The suggested modify macro, so an increment and a decrement are separable
    /// without parsing JSON.
    ///
    /// A closed two-value set of canonical `&'static str`s the analysis already
    /// stores, and they are two different rewrites — a consumer filtering on one
    /// of them is asking a real question.
    fn kind(&self) -> &'static str {
        self.suggested_head
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("suggested={}", self.suggested_head)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("suggested", json!(self.suggested_head))]
    }

    fn message(&self) -> String {
        format!(
            "setf manually adjusts a variable; use {}",
            self.suggested_head
        )
    }
}

pub fn examine_assignment(
    view: &ExpressionView,
    assignment_form_count: &mut usize,
    violations: &mut Vec<ManualIncfItem>,
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
    let Some((suggested_head, delta_span)) = incf_decf_rewrite(value, place_text) else {
        return;
    };

    violations.push(ManualIncfItem {
        span: view.span,
        suggested_head,
        place_span: place.span,
        delta_span,
    });
}

/// Collects every manual increment/decrement in one file, with the number of
/// `setf`/`setq` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_manual_incf_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ManualIncfItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("assignment_form_count", json!(0))],
        ));
    }

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(subview, &mut assignment_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("assignment_form_count", json!(assignment_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ManualIncfItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_manual_incf_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build manual incf report")
    }

    fn incfs(input: &str) -> (u64, Vec<ManualIncfItem>) {
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
    fn flags_setf_1plus() {
        let (count, violations) = incfs("(setf x (1+ x))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "incf");
        assert!(violations[0].delta_span.is_none());
    }

    #[test]
    fn flags_setq_1minus_as_decf() {
        let (_, violations) = incfs("(setq n (1- n))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "decf");
        assert!(violations[0].delta_span.is_none());
    }

    #[test]
    fn flags_plus_with_delta() {
        let source = "(setf i (+ i 2))";
        let (_, violations) = incfs(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "incf");
        let delta = violations[0].delta_span.expect("delta span");
        assert_eq!(slice(source, delta), "2");
    }

    #[test]
    fn flags_commuted_plus() {
        // (+ 2 i) is still an increment of i by 2.
        let source = "(setf i (+ 2 i))";
        let (_, violations) = incfs(source);
        assert_eq!(violations.len(), 1);
        let delta = violations[0].delta_span.expect("delta span");
        assert_eq!(slice(source, delta), "2");
    }

    #[test]
    fn flags_minus_with_delta_as_decf() {
        let source = "(setf i (- i step))";
        let (_, violations) = incfs(source);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "decf");
        let delta = violations[0].delta_span.expect("delta span");
        assert_eq!(slice(source, delta), "step");
    }

    #[test]
    fn does_not_flag_non_commuting_minus() {
        // (- d i) is d - i, not a decrement of i.
        let (_, violations) = incfs("(setf i (- step i))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_different_variable() {
        let (_, violations) = incfs("(setf x (1+ y))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_compound_place() {
        // (aref a i) place: incf would change how many times subforms evaluate.
        let (_, violations) = incfs("(setf (aref a i) (1+ (aref a i)))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_pair_setf() {
        let (_, violations) = incfs("(setf x (1+ x) y (1+ y))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_unrelated_value() {
        let (_, violations) = incfs("(setf x (compute))");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_setf_head() {
        let (_, violations) = incfs("(SETF x (1+ x))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_manual_incf() {
        let (_, violations) = incfs("(defun step () (setf counter (1+ counter)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(setf x (1+ x))", Dialect::Clojure).expect("parse");
        let report = build_manual_incf_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build manual incf report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("assignment_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(setf x (compute))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_suggested_macro() {
        let report = report("(defun step ()\n  (setf counter (1+ counter)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "incf");
        assert_eq!(finding.json_fields(), vec![("suggested", json!("incf"))]);
        assert_eq!(finding.text_columns(), vec!["suggested=incf".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_assignment_scanned_not_only_the_flagged_ones() {
        let report = report("(setf x (1+ x))\n(setf y (compute))\n(setq z (1- z))\n");
        assert_eq!(report.summary, vec![("assignment_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
