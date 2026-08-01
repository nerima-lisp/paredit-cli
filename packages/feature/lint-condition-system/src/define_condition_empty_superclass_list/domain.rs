//! `define-condition` forms whose supertype list is `()`.
//!
//! `(define-condition my-error () …)` does not define an error. With no
//! supertype, `define-condition` defaults to `condition` — and `condition` is
//! not a subtype of `error`, so `(handler-case … (error () …))` and
//! `ignore-errors` both walk straight past it, and an unhandled `signal` of it
//! returns `nil`. The name says `error`, the debugger says nothing, and the
//! handler never runs.
//!
//! Only the literal empty list is flagged. `(define-condition my-note
//! (condition) …)` is the same hierarchy written on purpose and is left alone:
//! this rule is about the gap between what the form says and what it does, not
//! about which supertype is correct.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, supertype_list, symbol_name};

#[derive(Debug, Clone)]
pub struct DefineConditionEmptySuperclassListItem {
    /// The span of the whole `define-condition` form.
    pub span: ByteSpan,
    /// The condition's name, so the message can name what will not be caught.
    pub condition_name: String,
}

impl Finding for DefineConditionEmptySuperclassListItem {
    fn kind(&self) -> &'static str {
        "define-condition-empty-superclass-list"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("condition={}", self.condition_name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("condition", json!(self.condition_name))]
    }

    fn message(&self) -> String {
        format!(
            "condition `{}` lists no supertypes, so it defaults to condition, not error; \
             handler-case (error () …) and ignore-errors will not catch it",
            self.condition_name
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_define_condition(
    view: &ExpressionView,
    define_condition_form_count: &mut usize,
    violations: &mut Vec<DefineConditionEmptySuperclassListItem>,
) {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| symbol_is(head, "define-condition"))
    {
        return;
    }
    *define_condition_form_count += 1;

    let Some(name) = view.children.get(1).and_then(symbol_name) else {
        return;
    };
    // `None` means child 2 is not a list at all — a malformed definition, whose
    // shape this rule declines to guess at.
    let Some(supertypes) = supertype_list(view) else {
        return;
    };
    if supertypes.is_empty() {
        violations.push(DefineConditionEmptySuperclassListItem {
            span: view.span,
            condition_name: name,
        });
    }
}

/// Collects every supertype-less `define-condition` in one file, with the
/// number of `define-condition` forms scanned as the denominator beside them.
pub fn build_define_condition_empty_superclass_list_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DefineConditionEmptySuperclassListItem>> {
    let mut define_condition_form_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_define_condition(view, &mut define_condition_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![(
            "define_condition_form_count",
            json!(define_condition_form_count),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DefineConditionEmptySuperclassListItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_define_condition_empty_superclass_list_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<DefineConditionEmptySuperclassListItem> {
        report(input).findings
    }

    #[test]
    fn flags_an_empty_supertype_list() {
        let found = violations("(define-condition parse-failed () ())");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].condition_name, "parse-failed");
    }

    #[test]
    fn flags_an_empty_supertype_list_with_slots_and_options() {
        let found = violations(
            "(define-condition parse-failed ()\n  ((line :initarg :line))\n  (:report \"boom\"))",
        );
        assert_eq!(found.len(), 1);
    }

    /// The near miss: the same form with the supertype actually written.
    #[test]
    fn does_not_flag_a_definition_that_lists_error() {
        assert!(violations("(define-condition parse-failed (error) ())").is_empty());
    }

    #[test]
    fn does_not_flag_a_deliberate_non_error_condition() {
        assert!(violations("(define-condition progress (condition) ())").is_empty());
        assert!(violations("(define-condition deprecated (warning) ())").is_empty());
    }

    #[test]
    fn does_not_flag_a_form_with_no_supertype_list_at_all() {
        assert!(violations("(define-condition parse-failed)").is_empty());
        assert!(
            violations("(define-condition parse-failed error ())").is_empty(),
            "child 2 is an atom, so the form is malformed rather than empty-listed"
        );
    }

    #[test]
    fn does_not_flag_defclass_or_defstruct() {
        assert!(violations("(defclass thing () ())").is_empty());
        assert!(violations("(defstruct thing)").is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        assert_eq!(violations("(DEFINE-CONDITION parse-failed () ())").len(), 1);
    }

    #[test]
    fn reads_a_package_qualified_head_and_name() {
        let found = violations("(cl:define-condition app::parse-failed () ())");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].condition_name, "parse-failed");
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(define-condition parse-failed () ())").is_empty());
        assert!(violations("(quote (define-condition parse-failed () ()))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_backquote_with_no_unquote_is_data() {
        assert!(violations("`(define-condition parse-failed () ())").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations("`(progn ,(define-condition parse-failed () ()))").len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(format t \"(define-condition parse-failed () ())\")").is_empty());
    }

    #[test]
    fn the_summary_counts_every_definition_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(define-condition a () ())\n(define-condition b (error) ())\n\
             (define-condition c () ())\n",
        );
        assert_eq!(
            report.summary,
            vec![("define_condition_form_count", json!(3))]
        );
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn the_finding_carries_its_line_and_its_condition_name() {
        let report = report("(in-package :app)\n(define-condition parse-failed () ())\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "define-condition-empty-superclass-list");
        assert_eq!(
            finding.json_fields(),
            vec![("condition", json!("parse-failed"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["condition=parse-failed".to_owned()]
        );
        assert!(
            finding
                .message()
                .contains("defaults to condition, not error")
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(define-condition a () ())", Dialect::Clojure)
            .expect("parse");
        let report = build_define_condition_empty_superclass_list_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("define_condition_form_count", json!(0))]
        );
    }
}
