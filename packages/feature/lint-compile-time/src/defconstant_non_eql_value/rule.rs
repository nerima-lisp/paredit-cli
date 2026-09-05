//! `defconstant-non-eql-value`: a constant whose initform allocates.
//!
//! # Cost
//!
//! `HeadFilter::Heads(&["defconstant"])`. `clean/forms/*` contains no
//! `defconstant`, so the rule is never dispatched there and contributes nothing
//! to the benchmark whose 10% threshold has failed this project five times.
//!
//! Within `check`, `classify` reads the initform node alone and rejects every
//! `defconstant` whose value is a number, a symbol, or any call this package
//! does not model — which is the great majority. Only a form already classified
//! `Fresh` reaches `is_top_level_form` and its tree access.
//!
//! Never `binding_table()`/`value_table()`/`type_table()`, never
//! `RuleContext::scratch_cache`.

use paredit_core_lint_engine::LintResult;

use crate::defconstant_non_eql_value::domain::{examine_defconstant, is_defconstant};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "defconstant-non-eql-value",
    // The form is well formed and means something other than it appears to:
    // it looks like a constant definition and is a redefinition collision.
    RuleCategory::Suspicious,
    // Measured: a fresh image that compiles and loads the file once already
    // signals DEFCONSTANT-UNEQL. Not a style preference.
    Severity::Error,
    "a defconstant whose initform allocates, so the compile-time and load-time values are not eql",
    // define-constant with which :test, or defparameter, is a judgement.
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defconstant")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        if !is_defconstant(view) {
            return Ok(());
        }
        let Some(item) = examine_defconstant(context.tree(), view) else {
            return Ok(());
        };
        sink.report(item.span, item.message());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule;
    use paredit_core_lint_engine::rule::RuleEntry;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule(&ENTRIES, source)
    }

    #[test]
    fn the_declared_head_reaches_the_rule() {
        assert_eq!(messages("(defconstant +x+ (list 1 2))\n").len(), 1);
    }

    #[test]
    fn every_defconstant_spelling_survives_dispatch_and_the_recheck() {
        for head in ["defconstant", "cl:defconstant", "DEFCONSTANT"] {
            assert_eq!(
                messages(&format!("({head} +x+ \"s\")\n")).len(),
                1,
                "`{head}` did not reach a finding"
            );
        }
    }

    /// `define-constant` normalizes to a different head, so the recommended
    /// idiom is never dispatched at all.
    #[test]
    fn the_alexandria_idiom_is_not_dispatched() {
        assert!(messages("(alexandria:define-constant +x+ \"s\" :test #'string=)\n").is_empty());
    }

    #[test]
    fn a_stable_valued_constant_produces_nothing_through_the_engine() {
        assert!(messages("(defconstant +x+ 100)\n").is_empty());
        assert!(messages("(defconstant +x+ :fast)\n").is_empty());
    }

    #[test]
    fn a_file_with_no_defconstant_produces_nothing() {
        assert!(messages("(defparameter *x* (list 1 2))\n").is_empty());
        assert!(messages("").is_empty());
    }

    #[test]
    fn each_offending_constant_is_reported_once() {
        assert_eq!(
            messages("(defconstant +a+ (list 1))\n(defconstant +b+ \"s\")\n(defconstant +c+ 3)\n")
                .len(),
            2
        );
    }
}
