//! `nested-boolean`: a same-operator and/or nested in an and/or, which flattens ((or a (or b c)) is (or a b c)).
//!

use paredit_core_lint_engine::LintResult;

use crate::nested_boolean::domain::examine_boolean;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-boolean",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a same-operator and/or nested in an and/or, which flattens ((or a (or b c)) is (or a b c))",
    Fixability::Fixable,
);

/// `examine_boolean` only ever matches an `and` or `or` head.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("and"), NormalizedHead::new("or")];

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
        let mut boolean_form_count = 0;
        let mut items = Vec::new();
        examine_boolean(view, &mut boolean_form_count, &mut items);
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
                // Splice the inner operands in place of the nested (op …) wrapper.

                RuleFix::single(
                    item.span,
                    context_slice(item.inner_span).trim().to_owned(),
                    "Flatten the nested same-operator and/or".to_owned(),
                )
            };
            let operator = item.operator;

            sink.report_fixed(
                span,
                format!("{operator} nested in a {operator} flattens; its operands splice in"),
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
            fixed("(defun f (a b c) (or a (or b c)))\n"),
            vec!["(defun f (a b c) (or a b c))\n".to_owned()]
        );
    }

    /// The control that matters most. A quasiquote template's contents really
    /// are emitted as code, so this rewrite is correct and must survive. A
    /// guard that read `is_data()` instead of the `hard` counter — the obvious
    /// wrong fix — would go quiet here and pass every other test in this file.
    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m (a) `(or ,a (or b c)))\n"),
            vec!["(defmacro m (a) `(or ,a b c))\n".to_owned()]
        );
    }

    // ----- the hard quote the guard exists for -----------------------------

    /// A form carrying its own `'`. The bytes this rule rewrites sit past that
    /// quote, so they are data even though nothing above the form quotes it.
    #[test]
    fn a_form_carrying_its_own_hard_quote_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* '(or a (or b c)))\n"), 0);
    }

    /// The measured shape: a quoted literal one level up, with the matched
    /// form carrying no prefix of its own.
    #[test]
    fn a_form_inside_a_quoted_ancestor_is_not_a_finding() {
        assert_eq!(count("(defparameter *p* '(f (or a (or b c))))\n"), 0);
    }
}
