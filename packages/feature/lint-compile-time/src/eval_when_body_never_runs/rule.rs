//! `eval-when-body-never-runs`: a nested `eval-when` whose situations the
//! standard ignores, so its body never runs.
//!
//! # Cost
//!
//! `HeadFilter::Heads(&["eval-when"])` — the same head as
//! [`crate::eval_when_execute_only`], and the two are complements rather than
//! duplicates: that rule fires only on a *top level* `eval-when` missing both
//! top-level situations, this one only on a *non*-top-level `eval-when` missing
//! `:execute`. No form can satisfy both, and a test in this module pins that.
//!
//! Sharing a head costs nothing: the head index maps one key to a list of
//! rules, so a second rule on `eval-when` adds one `check` call per `eval-when`
//! node and no extra walking. Files in `clean/forms/*` contain no `eval-when`
//! and so dispatch neither rule.
//!
//! Within `check`, the situation list is read from the dispatched node before
//! [`is_top_level_form`] touches the tree. That order matters more here than in
//! the sibling rule: the tree question is the expensive one *and* the one that
//! nearly always says "top level, no finding", so answering the cheap question
//! first is what keeps the common case free.
//!
//! Never `binding_table()`/`value_table()`/`type_table()`, never
//! `RuleContext::scratch_cache`.
//!
//! [`is_top_level_form`]: crate::support::is_top_level_form

use paredit_core_lint_engine::LintResult;

use crate::eval_when_body_never_runs::domain::examine_eval_when;
use crate::eval_when_execute_only::domain::is_eval_when;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eval-when-body-never-runs",
    // The body is unreachable in every phase. That is dead code, and unlike the
    // sibling rule's subject it is dead rather than merely misplaced.
    RuleCategory::DeadCode,
    // Certain, not probable: CLHS 3.2.3.1 makes a nested eval-when without
    // :execute equivalent to nil, and SBCL emits no diagnostic at all.
    Severity::Error,
    "a non-top-level eval-when naming only situations the standard ignores there, so its body \
     never runs",
    // Whether :execute was meant, or the form should be hoisted, or deleted, is
    // not recoverable from the source.
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

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule(&ENTRIES, source)
    }

    #[test]
    fn the_declared_head_reaches_the_rule() {
        assert_eq!(
            messages("(defun f () (eval-when (:compile-toplevel) (g)))\n").len(),
            1
        );
    }

    #[test]
    fn every_eval_when_spelling_survives_dispatch_and_the_recheck() {
        for head in ["eval-when", "cl:eval-when", "EVAL-WHEN"] {
            assert_eq!(
                messages(&format!("(defun f () ({head} (:compile-toplevel) (g)))\n")).len(),
                1,
                "`{head}` did not reach a finding"
            );
        }
    }

    #[test]
    fn a_top_level_eval_when_produces_nothing_through_the_engine() {
        assert!(messages("(eval-when (:compile-toplevel) (defmacro m () 1))\n").is_empty());
        assert!(messages("(progn (eval-when (:compile-toplevel) (f)))\n").is_empty());
    }

    #[test]
    fn a_file_with_no_eval_when_produces_nothing() {
        assert!(messages("(defun f () 1)\n").is_empty());
        assert!(messages("").is_empty());
    }

    /// The two `eval-when` rules in this package are complements. Running both
    /// catalogues over the same corpus of shapes must never produce two findings
    /// for one form — if it did, one of the two predicates would be wrong about
    /// what "top level" means.
    #[test]
    fn the_two_eval_when_rules_never_both_fire_on_one_form() {
        use crate::eval_when_execute_only;
        static BOTH: [RuleEntry; 2] = [
            RuleEntry::new(
                &eval_when_execute_only::rule::META,
                &eval_when_execute_only::rule::RULE,
            ),
            RuleEntry::new(&META, &RULE),
        ];
        for source in [
            "(eval-when (:execute) (defmacro m () 1))",
            "(eval-when (:compile-toplevel) (defmacro m () 1))",
            "(eval-when () (defmacro m () 1))",
            "(eval-when (:compile-toplevel :load-toplevel :execute) (defmacro m () 1))",
            "(defun f () (eval-when (:execute) (defmacro m () 1)))",
            "(defun f () (eval-when (:compile-toplevel) (defmacro m () 1)))",
            "(progn (eval-when (:execute) (defmacro m () 1)))",
            "(let () (eval-when (:load-toplevel) (defmacro m () 1)))",
            "(macrolet ((q () (eval-when (:execute) (defmacro m () 1)))) 1)",
        ] {
            let found = run_rule(&BOTH, source);
            assert!(
                found.len() <= 1,
                "both rules fired on `{source}`: {found:?}"
            );
        }
    }
}
