//! `negated-when-unless`: a when/unless whose test is a (not X)/(null X) negation (flip the macro instead).
//!
//! The analysis lives in [`crate::negated_when_unless::domain`], which also backs the
//! standalone `inspect negated-when-unless` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::negated_when_unless::domain::examine_conditional;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "negated-when-unless",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a when/unless whose test is a (not X)/(null X) negation (flip the macro instead)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("when"), NormalizedHead::new("unless")];

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
        let mut conditional_form_count = 0;
        let mut items = Vec::new();
        examine_conditional(view, &mut conditional_form_count, &mut items);
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
                // Two disjoint edits: flip the head macro and drop the negation,
                // leaving the body and all spacing byte-identical.
                let fix_first = Replacement::new(item.head_span, item.suggested_head.to_owned());
                let fix_rest = [Replacement::new(
                    item.test_span,
                    context_slice(item.inner_span),
                )];

                RuleFix::multi(
                    format!(
                        "Rewrite {} ({} …) as {}",
                        item.head, item.negator, item.suggested_head
                    ),
                    fix_first,
                    fix_rest,
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} test is ({} …); use {} on the un-negated test",
                    item.head, item.negator, item.suggested_head
                ),
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
            fixed("(defun f (c) (when (not c) (a)))\n"),
            vec!["(defun f (c) (unless c (a)))\n".to_owned()]
        );
    }

    /// The control that matters most. A quasiquote template's contents really
    /// are emitted as code, so this rewrite is correct and must survive. A
    /// guard that read `is_data()` instead of the `hard` counter — the obvious
    /// wrong fix — would go quiet here and pass every other test in this file.
    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m (c) `(when (not ,c) (a)))\n"),
            vec!["(defmacro m (c) `(unless ,c (a)))\n".to_owned()]
        );
    }

    // ----- the hard quote the guard exists for -----------------------------

    /// A form carrying its own `'`. The bytes this rule rewrites sit past that
    /// quote, so they are data even though nothing above the form quotes it.
    #[test]
    fn a_form_carrying_its_own_hard_quote_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* '(when (not c) (a)))\n"), 0);
    }

    /// The measured shape: a quoted literal one level up, with the matched
    /// form carrying no prefix of its own.
    #[test]
    fn a_form_inside_a_quoted_ancestor_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* '(f (when (not c) (a))))\n"), 0);
    }
}
