//! `eval-when-execute-only`: a top-level `eval-when` whose body `compile-file`
//! discards.
//!
//! # Cost
//!
//! `HeadFilter::Heads(&["eval-when"])`, so the rule is never dispatched on a
//! file without an `eval-when` — which is every file in `clean/forms/*`, the
//! benchmark whose 10% threshold has failed this project five times. A
//! `WholeTree` rule would be dispatched on all of them and pay a byte scan each;
//! this pays nothing, because the head index answers before `check` is reached.
//!
//! Within `check` the ordering is the one the module docs of
//! [`crate::support`] insist on: the situation list and the body are read from
//! the dispatched node alone, and only a form that has already failed both
//! node-local tests reaches [`is_top_level_form`], which materializes the
//! enclosing top-level form. A sibling package measured 450843 ns/call against
//! 28 ns/call purely from getting that order the wrong way round.
//!
//! Never `binding_table()`/`value_table()`/`type_table()` — this rule needs no
//! semantic pass and asks for none. Never `RuleContext::scratch_cache` either;
//! see [`crate::support`] for why that slot is not available to this package.
//!
//! [`is_top_level_form`]: crate::support::is_top_level_form

use paredit_core_lint_engine::LintResult;

use crate::eval_when_execute_only::domain::{examine_eval_when, is_eval_when};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eval-when-execute-only",
    // The file loads, compiles green, and means something other than it looks
    // like it means. That is exactly `Suspicious`.
    RuleCategory::Suspicious,
    // Not a judgement call: the compiled file provably does not contain the
    // definition, and SBCL reports the compilation as successful.
    Severity::Error,
    "a top-level eval-when naming :execute but neither top-level situation, wrapping a definition",
    // Which situations were meant cannot be recovered from the source.
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("eval-when")];

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
        // The head index is a pre-filter only, so the head is re-checked here.
        if !is_eval_when(view) {
            return Ok(());
        }
        let Some(item) = examine_eval_when(context.tree(), view) else {
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

    /// A one-rule catalogue, so the engine's dispatch — the thing that decides
    /// whether and how often `check` is called — is exercised for real. A wrong
    /// `Heads` list compiles, passes every domain test, and is simply never
    /// invoked by the CLI; only this catches that.
    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule(&ENTRIES, source)
    }

    #[test]
    fn the_declared_head_reaches_the_rule() {
        assert_eq!(
            messages("(eval-when (:execute) (defmacro m () 1))\n").len(),
            1
        );
    }

    /// The head index strips package qualifiers and folds case, and the rule's
    /// own re-check must agree with it or the spelling is dispatched and then
    /// silently dropped.
    #[test]
    fn every_eval_when_spelling_survives_dispatch_and_the_recheck() {
        for head in [
            "eval-when",
            "cl:eval-when",
            "EVAL-WHEN",
            "common-lisp:eval-when",
        ] {
            assert_eq!(
                messages(&format!("({head} (:execute) (defmacro m () 1))\n")).len(),
                1,
                "`{head}` did not reach a finding"
            );
        }
    }

    #[test]
    fn a_correct_eval_when_produces_nothing_through_the_engine() {
        assert!(
            messages("(eval-when (:compile-toplevel :load-toplevel :execute) (defmacro m () 1))\n")
                .is_empty()
        );
        assert!(messages("(eval-when (:load-toplevel :execute) (defmacro m () 1))\n").is_empty());
    }

    #[test]
    fn a_nested_eval_when_produces_nothing_through_the_engine() {
        assert!(messages("(defun f () (eval-when (:execute) (defmacro m () 1)))\n").is_empty());
    }

    #[test]
    fn a_file_with_no_eval_when_produces_nothing() {
        assert!(messages("(defun f () 1)\n(defmacro m () 2)\n").is_empty());
        assert!(messages("").is_empty());
    }

    #[test]
    fn each_offending_eval_when_is_reported_once() {
        let found = messages(
            "(eval-when (:execute) (defmacro a () 1))\n(eval-when (:execute) (defmacro b () 2))\n",
        );
        assert_eq!(found.len(), 2);
    }
}
