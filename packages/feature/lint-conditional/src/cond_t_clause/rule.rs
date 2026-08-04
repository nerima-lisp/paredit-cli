//! `cond-t-clause`: a cond with one t clause that has a body ((cond (t a b)) is (progn a b)).
//!
//! The analysis lives in [`crate::cond_t_clause::domain`], which also backs the
//! standalone `inspect cond-t-clause` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::cond_t_clause::domain::examine_cond;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "cond-t-clause",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cond with one t clause that has a body ((cond (t a b)) is (progn a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cond")];

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(view, &mut cond_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A hard-quoted form is a literal list, not code: rewriting its
            // contents edits the program's data rather than its behaviour.
            // `'(or x)` is a two-element list and `'x` is a symbol, so the
            // "fix" changes what the datum *is*.
            //
            // The verdict is the `hard` counter alone. A `` `(…) `` template's
            // contents really are emitted as code, so suppressing there would
            // go quiet on the macro bodies this rule exists to read.
            //
            // Asked only here, once a finding already exists — never per
            // visited node — so a file with no findings never pays for it.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // (cond (t body…)) -> (progn body…): splice the body verbatim.

                RuleFix::single(
                    item.span,
                    format!("(progn {})", context_slice(item.body_span).trim()),
                    "Rewrite the single t-clause cond as progn".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "single t-clause cond is just progn; (cond (t a b)) is (progn a b)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule_fixed;
    use paredit_core_lint_engine::rule::RuleEntry;

    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    /// The source each finding's fix produces, in report order. Asserting the
    /// rewritten text rather than the finding count is the point: a fix whose
    /// span is wrong still reports correctly, and only the spliced source
    /// shows it.
    fn fixed(source: &str) -> Vec<String> {
        run_rule_fixed(&ENTRIES, source)
            .into_iter()
            .map(|(_, source)| source)
            .collect()
    }

    fn count(source: &str) -> usize {
        run_rule_fixed(&ENTRIES, source).len()
    }

    // ----- positive controls: the guard did not widen ----------------------

    #[test]
    fn still_fires_on_ordinary_unquoted_code() {
        assert_eq!(
            fixed("(defun f () (cond (t (a) (b))))\n"),
            vec!["(defun f () (progn (a) (b)))\n".to_owned()]
        );
    }

    /// The control that matters most. A quasiquote template's contents really
    /// are emitted as code, so this rewrite is correct and must survive. A
    /// guard that read `is_data()` instead of the `hard` counter — the obvious
    /// wrong fix — would go quiet here and pass every other test in this file.
    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m (x) `(f (cond (t ,x (b)))))\n"),
            vec!["(defmacro m (x) `(f (progn ,x (b))))\n".to_owned()]
        );
    }

    // ----- the hard quote the guard exists for -----------------------------

    /// A form carrying its own `'`. The bytes this rule rewrites sit past that
    /// quote, so they are data even though nothing above the form quotes it.
    #[test]
    fn a_form_carrying_its_own_hard_quote_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* '(cond (t (a) (b))))\n"), 0);
    }

    /// The measured shape: a quoted literal one level up, with the matched
    /// form carrying no prefix of its own.
    #[test]
    fn a_form_inside_a_quoted_ancestor_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* '(f (cond (t (a) (b)))))\n"), 0);
    }
}
