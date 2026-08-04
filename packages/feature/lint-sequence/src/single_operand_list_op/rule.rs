//! `single-operand-list-op`: a single-argument append/nconc/list*, which returns its argument unchanged ((append x) is x).
//!
//! The analysis lives in [`crate::single_operand_list_op::domain`], which also backs the
//! standalone `inspect single-operand-list-op` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::single_operand_list_op::domain::examine_form;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "single-operand-list-op",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a single-argument append/nconc/list*, which returns its argument unchanged ((append x) is x)",
    Fixability::Fixable,
);

/// The three heads `examine_form` accepts.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("append"),
    NormalizedHead::new("nconc"),
    NormalizedHead::new("list*"),
];

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
        let mut list_op_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, &mut list_op_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (append x) is x: replace the whole form with the argument source.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    context_slice(item.arg_span),
                    format!("Drop the no-op single-argument {}", item.head),
                )
            };
            let head = item.head;

            sink.report_fixed(
                span,
                format!("{head} of one argument returns it unchanged; ({head} x) is x"),
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

    /// The source each finding's fix produces, in report order.
    fn fixed(source: &str) -> Vec<String> {
        run_rule_fixed(&ENTRIES, source)
            .into_iter()
            .map(|(_, source)| source)
            .collect()
    }

    #[allow(dead_code)]
    fn count(source: &str) -> usize {
        run_rule_fixed(&ENTRIES, source).len()
    }

    #[test]
    fn still_fires_on_an_ordinary_unquoted_append() {
        assert_eq!(
            fixed("(defun f (x) (append x))\n"),
            vec!["(defun f (x) x)\n".to_owned()]
        );
    }

    #[test]
    fn a_quasiquoted_single_operand_append_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(append ,x))\n"),
            vec!["(defmacro m (x) `,x)\n".to_owned()]
        );
    }

    /// `` `(append ,@parts) `` is not a one-operand append: `parts` may hold
    /// none or several, and `` `,@parts `` is not a well-formed backquote
    /// expression at all.
    #[test]
    fn a_spliced_operand_is_not_one_operand() {
        assert_eq!(count("(defmacro m (ps) `(append ,@ps))\n"), 0);
    }

    /// Symmetric control: a plain unquote really is one operand.
    #[test]
    fn a_plain_unquoted_operand_is_still_one_operand() {
        assert_eq!(
            fixed("(defmacro m (p) `(nconc ,p))\n"),
            vec!["(defmacro m (p) `,p)\n".to_owned()]
        );
    }
}
