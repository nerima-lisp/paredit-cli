//! `cerror` calls whose continue-format-control says nothing.
//!
//! `cerror`'s first argument is the *only* reason to use it instead of `error`:
//! it is what the debugger prints beside the automatically-established
//! `continue` restart, and it is how the person at the prompt learns what
//! continuing will do. A `nil` there — or no argument at all — leaves a
//! continuable error whose continuation is unexplained, which is strictly worse
//! than a plain `error`, because it invites a choice nobody has described.
//!
//! Two shapes, one rule: missing, and literal `nil`. Both are cases of "the
//! argument that makes `cerror` `cerror` is not there", and telling them apart
//! is what the message is for.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, symbol_name};

/// Why the continue-format-control conveys nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingContinueFormat {
    /// `(cerror)` — the argument is not there.
    Absent,
    /// `(cerror nil …)` — the argument is there and is `nil`.
    LiteralNil,
}

impl MissingContinueFormat {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::LiteralNil => "literal-nil",
        }
    }

    const fn detail(self) -> &'static str {
        match self {
            Self::Absent => "cerror was given no continue-format-control at all",
            Self::LiteralNil => "the continue-format-control is nil",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CerrorMissingContinueFormatItem {
    /// The span of the whole `(cerror …)` call.
    pub span: ByteSpan,
    pub reason: MissingContinueFormat,
}

impl Finding for CerrorMissingContinueFormatItem {
    /// The rule's own name. The two shapes are told apart by `reason`, which is
    /// also what the message says out loud.
    fn kind(&self) -> &'static str {
        "cerror-missing-continue-format"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("reason={}", self.reason.label())]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("reason", json!(self.reason.label()))]
    }

    fn message(&self) -> String {
        format!(
            "{}, so the debugger cannot say what continuing would do",
            self.reason.detail()
        )
    }
}

pub fn examine_cerror(
    view: &ExpressionView,
    cerror_call_count: &mut usize,
    violations: &mut Vec<CerrorMissingContinueFormatItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "cerror")) {
        return;
    }
    *cerror_call_count += 1;

    let reason = match view.children.get(1) {
        None => MissingContinueFormat::Absent,
        Some(argument) if symbol_name(argument).is_some_and(|name| name == "nil") => {
            MissingContinueFormat::LiteralNil
        }
        Some(_) => return,
    };
    violations.push(CerrorMissingContinueFormatItem {
        span: view.span,
        reason,
    });
}

/// Collects every uninformative `cerror` in one file, with the number of
/// `cerror` calls scanned as the denominator beside them.
pub fn build_cerror_missing_continue_format_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CerrorMissingContinueFormatItem>> {
    let mut cerror_call_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_cerror(view, &mut cerror_call_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("cerror_call_count", json!(cerror_call_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CerrorMissingContinueFormatItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_cerror_missing_continue_format_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<CerrorMissingContinueFormatItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_literal_nil_continue_format() {
        let found = violations("(cerror nil 'disk-full)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, MissingContinueFormat::LiteralNil);
    }

    #[test]
    fn flags_an_absent_continue_format() {
        let found = violations("(cerror)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reason, MissingContinueFormat::Absent);
    }

    /// The near miss: the same call with the argument it exists for.
    #[test]
    fn does_not_flag_a_cerror_that_explains_its_restart() {
        assert!(violations("(cerror \"Skip this entry.\" 'disk-full)").is_empty());
        assert!(violations("(cerror \"Retry ~A.\" 'disk-full path)").is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_continue_format() {
        assert!(
            violations("(cerror (continue-message) 'disk-full)").is_empty(),
            "a computed control string cannot be read, so nothing is claimed about it"
        );
    }

    #[test]
    fn does_not_flag_error_or_warn() {
        assert!(violations("(error nil)").is_empty());
        assert!(violations("(warn nil)").is_empty());
    }

    #[test]
    fn case_folds_the_head_and_the_nil() {
        assert_eq!(violations("(CERROR NIL 'disk-full)").len(), 1);
        assert_eq!(violations("(cl:cerror nil 'disk-full)").len(), 1);
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(cerror nil 'disk-full)").is_empty());
        assert!(violations("(quote (cerror nil 'disk-full))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_backquote_with_no_unquote_is_data() {
        assert!(violations("`(cerror nil 'disk-full)").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(violations("`(progn ,(cerror nil 'disk-full))").len(), 1);
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(format t \"(cerror nil 'disk-full)\")").is_empty());
    }

    #[test]
    fn the_summary_counts_every_cerror_scanned_not_only_the_flagged_ones() {
        let report = report("(cerror \"Skip.\" 'a)\n(cerror nil 'b)\n");
        assert_eq!(report.summary, vec![("cerror_call_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_message_distinguishes_the_two_shapes() {
        assert_eq!(
            violations("(cerror nil 'a)")[0].message(),
            "the continue-format-control is nil, so the debugger cannot say what continuing \
             would do"
        );
        assert_eq!(
            violations("(cerror)")[0].message(),
            "cerror was given no continue-format-control at all, so the debugger cannot say \
             what continuing would do"
        );
    }

    #[test]
    fn the_finding_carries_its_line_and_its_reason() {
        let report = report("(defun f ()\n  (cerror nil 'disk-full))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "cerror-missing-continue-format");
        assert_eq!(
            finding.json_fields(),
            vec![("reason", json!("literal-nil"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["reason=literal-nil".to_owned()]
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(cerror nil 'a)", Dialect::Clojure).expect("parse");
        let report = build_cerror_missing_continue_format_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("cerror_call_count", json!(0))]);
    }
}
