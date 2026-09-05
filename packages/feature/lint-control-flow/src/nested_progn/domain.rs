//! Common Lisp nested-`progn` detection: an explicit `(progn …)` that appears
//! as a body form directly inside another `progn`. Because `progn` evaluates
//! its body in order and a nested progn's own result is spliced into that
//! sequence, wrapping body forms in an inner progn changes nothing:
//! `(progn a (progn b c) d)` is exactly `(progn a b c d)`. The nesting is pure
//! structure noise — common after mechanical macro expansion or code motion.
//!
//! This rule is the multi-form companion to
//! [`crate::redundant_progn::domain`]: that rule owns the 0-form and
//! 1-form progns (which are redundant on their own, in any position), while this
//! rule owns progns with two or more body forms that are redundant *because* of
//! where they sit. The two never flag the same span, so `inspect lint` reports
//! each redundant progn once.
//!
//! Splicing is semantics-preserving regardless of reader conditionals or the
//! inner body's contents, so no `#+`/`#-` guard is needed here.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// Whether `view` is a `(progn …)` form.
fn is_progn(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("progn"))
}

#[derive(Debug, Clone)]
pub struct NestedPrognItem {
    /// The span of the inner (nested) progn.
    pub span: ByteSpan,
    /// The span covering just the inner progn's body forms (first form start to
    /// last form end), so a fix can splice that source in place of the wrapper.
    ///
    pub body_span: ByteSpan,
    /// The inner progn's body form count (always >= 2 here; the 0/1 cases
    /// belong to the redundant-progn rule).
    pub body_form_count: usize,
}

impl Finding for NestedPrognItem {
    /// The rule's own name. There is exactly one shape this rule flags — a
    /// multi-form progn directly inside another — so there is no variant to
    /// separate.
    fn kind(&self) -> &'static str {
        "nested-progn"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("body_form_count={}", self.body_form_count)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("body_form_count", json!(self.body_form_count))]
    }

    fn message(&self) -> String {
        format!(
            "progn with {} forms is nested directly in another progn; splice its forms in",
            self.body_form_count
        )
    }
}

pub fn examine_progn(
    view: &ExpressionView,
    progn_form_count: &mut usize,
    violations: &mut Vec<NestedPrognItem>,
) {
    if !is_progn(view) {
        return;
    }
    *progn_form_count += 1;

    // children[0] is the `progn` head; the rest is the body. Any body form that
    // is itself a multi-form progn splices redundantly into this one.
    for child in &view.children[1..] {
        if !is_progn(child) {
            continue;
        }
        // A prefixed child is not a nested progn to splice away, it is a datum
        // sitting in that slot. `` `(progn (f) `(progn (g) (h))) `` has an inner
        // progn that is the *template* the outer one emits, not a body it
        // sequences, and splicing it drops the backquote that makes the commas
        // underneath legal — the file then stops reading. `'(progn a b)` in the
        // same slot is a literal three-element list and splicing rewrites data.
        //
        // The test is on the *child*, deliberately not inside `is_progn`, which
        // also decides the enclosing form: a `` `(progn (progn a b) c) `` really
        // does contain a redundant nested progn, and suppressing the whole
        // template would be a false negative.
        if !child.reader_prefixes.is_empty() {
            continue;
        }
        let body = &child.children[1..];
        let inner_body_form_count = body.len();
        if inner_body_form_count >= 2 {
            // Body span runs from the first body form's start to the last's end.
            let body_span = ByteSpan::new(
                body[0].span.start(),
                body[inner_body_form_count - 1].span.end(),
            );
            violations.push(NestedPrognItem {
                span: child.span,
                body_span,
                body_form_count: inner_body_form_count,
            });
        }
    }
}

/// Collects every progn nested directly inside another progn in one file, with
/// the number of progn forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_nested_progn_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedPrognItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("progn_form_count", json!(0))],
        ));
    }

    let mut progn_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_progn(subview, &mut progn_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("progn_form_count", json!(progn_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NestedPrognItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_progn_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested progn report")
    }

    fn nested(input: &str) -> (u64, Vec<NestedPrognItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "progn_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("progn_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_a_multi_form_progn_nested_in_progn() {
        let (count, violations) = nested("(progn a (progn b c) d)");
        // Two progn forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].body_form_count, 2);
    }

    #[test]
    fn body_span_isolates_the_inner_body_forms() {
        let input = "(progn a (progn b c) d)";
        let (_, violations) = nested(input);
        let body = violations[0].body_span;
        assert_eq!(
            &input[body.start().get()..body.end().get()],
            "b c",
            "body span must cover just the inner progn's body, not its parens"
        );
    }

    #[test]
    fn flags_a_trailing_nested_progn() {
        let (_, violations) = nested("(progn a (progn b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_multiple_nested_progns() {
        let (_, violations) = nested("(progn (progn a b) (progn c d))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn case_folds_both_progns() {
        let (_, violations) = nested("(PROGN x (PROGN y z))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_single_form_inner_progn() {
        // (progn x) is the redundant-progn rule's job, not this one.
        let (_, violations) = nested("(progn a (progn x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_inner_progn() {
        let (_, violations) = nested("(progn a (progn))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_not_inside_a_progn() {
        // A multi-form progn in a value slot (an if branch) is meaningful.
        let (_, violations) = nested("(if c (progn a b) d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_outer_progn_itself() {
        let (_, violations) = nested("(progn (foo) (bar))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_deeply_nested_progns() {
        // Inner-most is nested in the middle progn; middle is nested in outer.
        let (_, violations) = nested("(progn (progn (progn a b) c))");
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(progn a (progn b c))", Dialect::Clojure)
            .expect("parse");
        let report = build_nested_progn_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nested progn report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("progn_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(progn (foo) (bar))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_body_form_count() {
        let report = report("(progn\n  a\n  (progn b c))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "nested-progn");
        assert_eq!(finding.json_fields(), vec![("body_form_count", json!(2))]);
        assert_eq!(finding.text_columns(), vec!["body_form_count=2".to_owned()]);
        assert_eq!(
            finding.message(),
            "progn with 2 forms is nested directly in another progn; splice its forms in"
        );
    }

    #[test]
    fn the_summary_counts_every_progn_scanned_not_only_the_flagged_ones() {
        // Outer plus inner, of which only the inner is a finding.
        let report = report("(progn a (progn b c) d)\n");
        assert_eq!(report.summary, vec![("progn_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
