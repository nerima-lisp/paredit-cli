//! `restart-case` clauses with no `:report`.
//!
//! A restart is a choice offered to a human at the debugger prompt. Without a
//! `:report`, the only thing the debugger can print for it is the restart's own
//! symbol — so a carefully designed recovery path shows up as `SKIP-ENTRY` in a
//! list of otherwise-explained options, and the person reading it has to go find
//! the source to learn what it does.
//!
//! Only the clause shape `(name (lambda-list) option* body*)` is examined. A
//! clause whose second element is not a lambda list is not a restart-case clause
//! this analysis recognises, and guessing at a malformed one would put the
//! option scan at the wrong index.
//!
//! **Five names are exempt.** `continue`, `abort`, `use-value`, `store-value`
//! and `muffle-warning` are the restarts CLHS 9.1.4.2.2 names, each with a
//! standard invoker function of the same name (`(continue c)`, `(abort c)`, …).
//! For those, the bare name *is* the interface: a reader of the debugger's
//! restart list already knows what `ABORT` does, and a handler invokes it by
//! that name rather than by reading its report. Demanding a `:report` there
//! flags idiomatic, correct code, so [`STANDARD_RESTARTS`] clauses are neither
//! flagged nor counted — they are not clauses this rule has an opinion about,
//! so putting them in the denominator would understate the rate for the clauses
//! it does.
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

#[derive(Debug, Clone)]
pub struct RestartCaseClauseWithoutReportItem {
    /// The span of the offending clause, not of the whole `restart-case`: the
    /// clause is where a `:report` would be written.
    pub span: ByteSpan,
    /// The restart's name, as the debugger would print it.
    pub restart_name: String,
}

impl Finding for RestartCaseClauseWithoutReportItem {
    fn kind(&self) -> &'static str {
        "restart-case-clause-without-report"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("restart={}", self.restart_name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("restart", json!(self.restart_name))]
    }

    fn message(&self) -> String {
        format!(
            "restart `{}` has no :report; the debugger can only offer its bare name",
            self.restart_name
        )
    }
}

/// The options a `restart-case` clause may carry between its lambda list and
/// its body, in any order (CLHS 9.1.4.2).
///
/// The scan has to stop at the first non-option, or a body form beginning with
/// a keyword would be read as another option and shift every following pair.
const CLAUSE_OPTIONS: [&str; 3] = [":report", ":interactive", ":test"];

/// Whether a clause carries `:report`, given that it is a clause at all.
fn declares_a_report(clause: &ExpressionView) -> bool {
    let mut index = 2;
    while let Some(child) = clause.children.get(index) {
        let Some(keyword) = symbol_name(child) else {
            return false;
        };
        if !CLAUSE_OPTIONS.contains(&keyword.as_str()) {
            return false;
        }
        if keyword == ":report" {
            return true;
        }
        // Skip the option's value as well as its keyword.
        index += 2;
    }
    false
}

/// The restarts CLHS establishes by name, each with a function that invokes it
/// (CLHS 9.1.4.2.2 and the `continue`/`abort`/`use-value`/`store-value`/
/// `muffle-warning` function pages).
///
/// A clause named for one of these is exempt from the `:report` requirement:
/// the name is a documented part of the language rather than something this
/// program invented, so it is already understood by whoever reads the restart
/// list and by whatever handler invokes it.
///
/// Normalized spelling, so `CL:ABORT` and `abort` both match.
const STANDARD_RESTARTS: [&str; 5] = [
    "abort",
    "continue",
    "muffle-warning",
    "store-value",
    "use-value",
];

/// Whether the bare name already explains the restart, per [`STANDARD_RESTARTS`].
fn is_standard_restart(name: &str) -> bool {
    STANDARD_RESTARTS.contains(&name)
}

/// The restart name of a well-formed clause, or `None` for anything that is not
/// one.
fn restart_name(clause: &ExpressionView) -> Option<String> {
    if !is_paren_list(clause) {
        return None;
    }
    let name = clause.children.first().and_then(symbol_name)?;
    // A keyword where the restart's name belongs means this is not a clause.
    if name.starts_with(':') {
        return None;
    }
    let lambda_list = clause.children.get(1)?;
    is_paren_list(lambda_list).then_some(name)
}

pub fn examine_restart_case(
    view: &ExpressionView,
    restart_clause_count: &mut usize,
    violations: &mut Vec<RestartCaseClauseWithoutReportItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "restart-case"))
    {
        return;
    }

    // children[0] is the head and children[1] is the protected form; the
    // clauses are everything after that.
    for clause in view.children.iter().skip(2) {
        let Some(name) = restart_name(clause) else {
            continue;
        };
        // Neither flagged nor counted: a standard restart is outside this
        // rule's subject, not an instance of it that happens to pass.
        if is_standard_restart(&name) {
            continue;
        }
        *restart_clause_count += 1;
        if !declares_a_report(clause) {
            violations.push(RestartCaseClauseWithoutReportItem {
                span: clause.span,
                restart_name: name,
            });
        }
    }
}

/// Collects every `:report`-less restart clause in one file, with the number of
/// restart clauses this rule has an opinion about as the denominator beside
/// them — every clause scanned except the [`STANDARD_RESTARTS`] ones.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_restart_case_clause_without_report_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RestartCaseClauseWithoutReportItem>> {
    let mut restart_clause_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_restart_case(view, &mut restart_clause_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("restart_clause_count", json!(restart_clause_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<RestartCaseClauseWithoutReportItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_restart_case_clause_without_report_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<RestartCaseClauseWithoutReportItem> {
        report(input).findings
    }

    fn clause_count(input: &str) -> u64 {
        report(input)
            .summary
            .iter()
            .find(|(name, _)| *name == "restart_clause_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("restart_clause_count in the summary")
    }

    #[test]
    fn flags_a_clause_with_no_report() {
        let found = violations("(restart-case (parse-entry) (skip-entry () nil))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].restart_name, "skip-entry");
    }

    #[test]
    fn does_not_flag_a_clause_that_declares_a_report() {
        assert!(
            violations(
                "(restart-case (parse-entry)\n  (skip-entry () :report \"Skip this entry.\" nil))"
            )
            .is_empty()
        );
    }

    #[test]
    fn finds_the_report_past_another_option() {
        assert!(
            violations(
                "(restart-case (parse-entry)\n  \
                 (skip-entry () :interactive read-choice :report \"Skip it.\" nil))"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_a_clause_that_declares_only_the_other_options() {
        let found = violations(
            "(restart-case (parse-entry)\n  (skip-entry () :interactive read-choice nil))",
        );
        assert_eq!(found.len(), 1);
    }

    /// A body form that happens to start with a keyword must not be read as an
    /// option, or the scan would run past the end of the clause.
    #[test]
    fn a_keyword_valued_body_is_not_mistaken_for_an_option() {
        let found = violations("(restart-case (parse-entry) (skip-entry () :skipped))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_each_clause_that_needs_it_and_leaves_the_others_alone() {
        let found = violations(
            "(restart-case (parse-entry)\n  \
             (skip-entry () :report \"Skip.\" nil)\n  \
             (retry-entry () nil)\n  \
             (abort-parse () nil))",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].restart_name, "retry-entry");
        assert_eq!(found[1].restart_name, "abort-parse");
    }

    #[test]
    fn case_folds_the_head_and_the_option() {
        assert!(
            violations("(RESTART-CASE (f) (skip () :REPORT \"Skip.\" nil))").is_empty(),
            "the reader upcases a keyword like any other symbol"
        );
        assert_eq!(violations("(RESTART-CASE (f) (skip () nil))").len(), 1);
    }

    /// The four CLHS-standard restarts an idiomatic handler establishes. Each
    /// has an invoker function of the same name, so the bare name is the
    /// interface and a `:report` adds nothing.
    #[test]
    fn the_standard_restart_names_are_not_flagged() {
        assert!(
            violations(
                "(restart-case (do-thing)\n  \
                 (continue () ...)\n  \
                 (abort () ...)\n  \
                 (use-value (v) ...)\n  \
                 (store-value (v) ...))"
            )
            .is_empty()
        );
        // Not vacuous: the same four clauses under names of this program's own
        // invention are four findings, so the exemption is what silences them
        // and not a shape the scan failed to recognise.
        assert_eq!(
            violations(
                "(restart-case (do-thing)\n  \
                 (keep-going () ...)\n  \
                 (give-up () ...)\n  \
                 (take-value (v) ...)\n  \
                 (keep-value (v) ...))"
            )
            .len(),
            4
        );
    }

    #[test]
    fn muffle_warning_is_standard_too() {
        assert!(violations("(restart-case (emit) (muffle-warning () nil))").is_empty());
    }

    #[test]
    fn a_non_standard_restart_name_still_needs_a_report() {
        let found = violations(
            "(restart-case (do-thing)\n  \
             (continue () nil)\n  \
             (skip-entry () nil))",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].restart_name, "skip-entry");
    }

    #[test]
    fn a_standard_name_is_recognised_case_folded_and_package_qualified() {
        assert!(violations("(restart-case (f) (ABORT () nil))").is_empty());
        assert!(violations("(restart-case (f) (cl:use-value (v) nil))").is_empty());
    }

    /// A near-miss must not inherit the exemption: only the five names are
    /// standard, not everything that starts like one.
    #[test]
    fn a_name_that_merely_resembles_a_standard_one_is_flagged() {
        let found = violations("(restart-case (f) (continue-anyway () nil))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].restart_name, "continue-anyway");
    }

    #[test]
    fn a_standard_restart_is_left_out_of_the_denominator_as_well() {
        assert_eq!(
            clause_count("(restart-case (f) (continue () nil) (skip-entry () nil))"),
            1
        );
        assert_eq!(clause_count("(restart-case (f) (continue () nil))"), 0);
    }

    #[test]
    fn a_clause_without_a_lambda_list_is_not_a_clause() {
        assert!(violations("(restart-case (f) skip-entry)").is_empty());
        assert!(violations("(restart-case (f) (skip-entry))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(restart-case (f) (skip-entry () nil))").is_empty());
        assert!(violations("(quote (restart-case (f) (skip-entry () nil)))").is_empty());
    }

    #[test]
    fn a_matching_shape_inside_a_backquote_with_no_unquote_is_data() {
        assert!(violations("`(restart-case (f) (skip-entry () nil))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations("`(defun g () ,(restart-case (f) (skip-entry () nil)))").len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(violations("(format t \"(restart-case (f) (skip-entry () nil))\")").is_empty());
    }

    #[test]
    fn the_summary_counts_every_clause_scanned_not_only_the_flagged_ones() {
        assert_eq!(
            clause_count("(restart-case (f) (skip () :report \"Skip.\" nil) (retry () nil))"),
            2
        );
    }

    #[test]
    fn the_finding_carries_its_line_and_its_restart_name() {
        let report = report("(defun f ()\n  (restart-case (g)\n    (skip-entry () nil)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 3);
        assert_eq!(finding.kind(), "restart-case-clause-without-report");
        assert_eq!(
            finding.json_fields(),
            vec![("restart", json!("skip-entry"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["restart=skip-entry".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "restart `skip-entry` has no :report; the debugger can only offer its bare name"
        );
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(restart-case (f) (skip () nil))", Dialect::Clojure)
                .expect("parse");
        let report = build_restart_case_clause_without_report_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("restart_clause_count", json!(0))]);
    }
}
