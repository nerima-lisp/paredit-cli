//! Common Lisp `loop` accumulator-conflict detection: one `into` variable
//! accumulated by two clauses that build incompatible kinds of value.
//!
//! `(loop for x in items collect x into acc sum x into acc …)` asks the macro
//! to make `acc` both a list and a number. CLHS 6.1.3 requires the
//! accumulation clauses naming one variable to agree on what they build, and
//! implementations reject the disagreement with a macroexpansion error that
//! names neither the variable nor the two clauses.
//!
//! # What this rule recognises
//!
//! Exactly one shape: a top-level accumulation verb, its value operand, then
//! the keyword `into`, then a bare symbol. Two such clauses naming the same
//! symbol conflict when one builds a **list** and the other a **number**.
//!
//! # What this rule deliberately does not flag
//!
//! - **Numeric verbs disagreeing with each other.** `sum` and `count` into one
//!   variable, or `sum` and `maximize`, are treated as compatible here.
//!   Implementations differ on whether they are, and reporting them would make
//!   this rule right on one implementation rather than on the standard.
//! - **The implicit result** — two verbs with *no* `into`. That is the
//!   complementary case, and it is already covered by the `inspect loop`
//!   report's `conflicting-accumulation` finding in
//!   `paredit-feature-lisp-analysis`, which explicitly drops any verb that
//!   names an `into` target. Between them the two cover the whole question and
//!   neither repeats the other.
//! - **An `into` target whose accumulation kind is not visible**, including a
//!   verb inside a conditional branch whose sibling branch this reader cannot
//!   correlate, and any form [`crate::loop_syntax`] declines to read.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use serde_json::{Value, json};

use crate::loop_syntax::{LoopScan, accumulation_kind};
use crate::support::for_each_evaluated_subview;

#[derive(Debug, Clone)]
pub struct LoopAccumulatorConflictItem {
    /// The span of the second, conflicting accumulation keyword.
    pub span: ByteSpan,
    /// The `into` variable both clauses name, lowercased.
    pub into_variable: String,
    /// The verb that claimed the variable first, and what it builds.
    pub first_verb: String,
    pub first_kind: String,
    /// The verb that disagrees, and what it builds.
    pub verb: String,
    pub kind: String,
}

impl Finding for LoopAccumulatorConflictItem {
    fn kind(&self) -> &'static str {
        "loop-into-accumulator-kind-conflict"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("into={}", self.into_variable),
            format!("first={}:{}", self.first_verb, self.first_kind),
            format!("conflicting={}:{}", self.verb, self.kind),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("into_variable", json!(self.into_variable)),
            ("first_verb", json!(self.first_verb)),
            ("first_kind", json!(self.first_kind)),
            ("verb", json!(self.verb)),
            ("kind", json!(self.kind)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "loop accumulates into `{}` as a {} with `{}` and as a {} with `{}`; \
             one variable cannot hold both",
            self.into_variable, self.first_kind, self.first_verb, self.kind, self.verb
        )
    }
}

/// One `into` target already claimed by a verb.
struct Claim {
    name: String,
    kind: &'static str,
    verb: String,
}

pub fn examine_loop_accumulator_conflict(
    view: &ExpressionView,
    scan: &mut LoopScan,
    violations: &mut Vec<LoopAccumulatorConflictItem>,
) {
    let Some(clauses) = scan.read(view) else {
        return;
    };

    let mut claims: Vec<Claim> = Vec::new();
    // The verb whose `into` target has not been read yet. Cleared by any other
    // keyword, so only `verb <value> into <name>` is ever matched.
    let mut pending: Option<(String, &'static str, ByteSpan)> = None;

    for (position, token) in clauses.tokens.iter().enumerate() {
        let Some(keyword) = token.keyword() else {
            continue;
        };

        if keyword == "into" {
            let Some((verb, kind, verb_span)) = pending.take() else {
                continue;
            };
            let Some(target) = clauses.symbol_after(position) else {
                continue;
            };
            let Some(name) = target.operand_symbol() else {
                continue;
            };
            match claims.iter().find(|claim| claim.name == name) {
                Some(claim) if claim.kind != kind => {
                    violations.push(LoopAccumulatorConflictItem {
                        span: verb_span,
                        into_variable: name.to_owned(),
                        first_verb: claim.verb.clone(),
                        first_kind: claim.kind.to_owned(),
                        verb,
                        kind: kind.to_owned(),
                    });
                }
                Some(_) => {}
                None => claims.push(Claim {
                    name: name.to_owned(),
                    kind,
                    verb,
                }),
            }
            continue;
        }

        pending = accumulation_kind(keyword).map(|kind| (keyword.to_owned(), kind, token.span()));
    }
}

/// Collects every conflicting `into` accumulator in one file, with the number
/// of `loop` forms scanned and the number actually modelled beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_loop_accumulator_conflict_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LoopAccumulatorConflictItem>> {
    let mut scan = LoopScan::default();
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            for_each_evaluated_subview(&view, |subview| {
                examine_loop_accumulator_conflict(subview, &mut scan, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![
            ("loop_form_count", json!(scan.loop_form_count)),
            (
                "modelled_loop_form_count",
                json!(scan.modelled_loop_form_count),
            ),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<LoopAccumulatorConflictItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_loop_accumulator_conflict_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build loop accumulator conflict report")
    }

    fn violations(input: &str) -> Vec<LoopAccumulatorConflictItem> {
        report(input).findings
    }

    #[test]
    fn flags_collect_and_sum_into_one_variable() {
        let found = violations("(loop for x in items collect x into acc sum x into acc)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].into_variable, "acc");
        assert_eq!(found[0].first_verb, "collect");
        assert_eq!(found[0].first_kind, "list");
        assert_eq!(found[0].verb, "sum");
        assert_eq!(found[0].kind, "number");
    }

    #[test]
    fn flags_count_and_append_into_one_variable() {
        let found = violations("(loop for x in items count x into acc appending x into acc)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].first_kind, "number");
        assert_eq!(found[0].kind, "list");
    }

    #[test]
    fn does_not_flag_two_list_verbs_into_one_variable() {
        assert!(
            violations("(loop for x in items collect x into acc nconc (list x) into acc)")
                .is_empty()
        );
    }

    /// `sum` and `count` are both numeric. Implementations disagree about
    /// whether they may share a target, so this rule stays silent.
    #[test]
    fn does_not_flag_two_numeric_verbs_into_one_variable() {
        assert!(violations("(loop for x in items sum x into acc count x into acc)").is_empty());
        assert!(violations("(loop for x in items sum x into acc maximize x into acc)").is_empty());
    }

    #[test]
    fn does_not_flag_different_kinds_into_different_variables() {
        assert!(violations("(loop for x in items collect x into xs sum x into total)").is_empty());
    }

    /// The implicit result is the `inspect loop` report's business, not this
    /// rule's.
    #[test]
    fn does_not_flag_conflicting_verbs_with_no_into_target() {
        assert!(violations("(loop for x in items collect x sum x)").is_empty());
    }

    #[test]
    fn does_not_flag_a_realistic_extended_loop() {
        let found = violations(
            "(loop for x from 1 to 10\n\
             \x20     with limit = 50\n\
             \x20     while (< x limit)\n\
             \x20     collect x into taken\n\
             \x20     append (list x x) into doubled\n\
             \x20     count x into seen\n\
             \x20     finally (return (values taken doubled seen)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn does_not_flag_a_loop_with_when_and_else() {
        let found = violations(
            "(loop for x in items\n\
             \x20     when (evenp x) collect x into evens\n\
             \x20     else collect x into odds\n\
             \x20     end\n\
             \x20     finally (return (list evens odds)))",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn says_nothing_about_a_being_the_hash_keys_loop() {
        let report =
            report("(loop for k being the hash-keys of table collect k into ks sum k into ks)");
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![
                ("loop_form_count", json!(1)),
                ("modelled_loop_form_count", json!(0)),
            ]
        );
    }

    #[test]
    fn does_not_flag_a_quoted_loop() {
        assert!(violations("(defparameter *f* '(loop collect x into a sum x into a))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_loop_with_no_unquote() {
        assert!(violations("(defmacro m () `(loop collect x into a sum x into a))").is_empty());
    }

    /// The quote need not sit on the `loop` itself: a target nested *inside*
    /// quoted data is still data.
    #[test]
    fn does_not_flag_a_loop_nested_inside_quoted_data() {
        assert!(
            violations("(defparameter *f* '(progn (loop collect x into a sum x into a)))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_nested_inside_a_macro_template() {
        assert!(
            violations(
                "(defmacro m (&body body)\n  `(progn (loop collect x into a sum x into a) ,@body))"
            )
            .is_empty()
        );
    }

    /// A known, deliberate false negative. `and` is a name introducer, so the
    /// token after it never reads as a keyword — which means the second verb
    /// of `collect x into a and sum x into a` is invisible here while the
    /// un-`and`ed spelling is reported. Losing a finding is the safe
    /// direction, and narrowing `and` would cost findings elsewhere.
    #[test]
    fn does_not_flag_a_conflict_written_with_and() {
        assert!(violations("(loop for x in items collect x into a and sum x into a)").is_empty());
        // The same conflict without `and` is reported.
        assert_eq!(
            violations("(loop for x in items collect x into a sum x into a)").len(),
            1
        );
    }

    #[test]
    fn does_not_flag_clause_keywords_inside_a_string_literal() {
        assert!(
            violations("(defparameter *doc* \"loop collect x into a sum x into a\")").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_loop_whose_variable_is_named_like_a_verb() {
        // `sum` here is a `for` variable, not an accumulation clause.
        assert!(violations("(loop for sum in items collect sum into acc)").is_empty());
    }

    #[test]
    fn finds_a_conflict_nested_in_a_function_body() {
        let found = violations(
            "(defun f (items)\n  (loop for x in items collect x into acc sum x into acc))",
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(loop for x in items collect x into acc sum x into acc)",
            Dialect::Clojure,
        )
        .expect("parse");
        let report =
            build_loop_accumulator_conflict_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build loop accumulator conflict report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(loop for x in items collect x)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report(
            "(defun f (items)\n  (loop for x in items collect x into acc sum x into acc))\n",
        );
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "loop-into-accumulator-kind-conflict");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("into_variable", json!("acc")),
                ("first_verb", json!("collect")),
                ("first_kind", json!("list")),
                ("verb", json!("sum")),
                ("kind", json!("number")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec![
                "into=acc".to_owned(),
                "first=collect:list".to_owned(),
                "conflicting=sum:number".to_owned(),
            ]
        );
        assert_eq!(
            finding.message(),
            "loop accumulates into `acc` as a list with `collect` and as a number with `sum`; \
             one variable cannot hold both"
        );
    }

    #[test]
    fn the_summary_counts_every_loop_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(loop for x in items collect x into a sum x into a)\n(loop for y in items collect y)\n",
        );
        assert_eq!(
            report.summary,
            vec![
                ("loop_form_count", json!(2)),
                ("modelled_loop_form_count", json!(2)),
            ]
        );
        assert_eq!(report.findings.len(), 1);
    }
}
