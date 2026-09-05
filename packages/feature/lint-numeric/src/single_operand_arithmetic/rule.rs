//! `single-operand-arithmetic`: a single-operand +/* ((+ X) and (* X) are just X).
//!

use paredit_core_lint_engine::LintResult;

use crate::single_operand_arithmetic::domain::examine_arithmetic;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "single-operand-arithmetic",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a single-operand +/* ((+ X) and (* X) are just X)",
    Fixability::Fixable,
);

/// The two heads `examine_arithmetic` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("+"), NormalizedHead::new("*")];

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
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine_arithmetic(view, &mut arithmetic_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Replace the wrapper with its sole operand, copied verbatim.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    context_slice(item.inner_span),
                    format!("Unwrap the single-operand {}", item.operator),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} has a single operand; ({} X) is just X",
                    item.operator, item.operator
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
    fn still_fires_on_an_ordinary_unquoted_sum() {
        assert_eq!(
            fixed("(defun f (x) (+ x))\n"),
            vec!["(defun f (x) x)\n".to_owned()]
        );
    }

    #[test]
    fn a_quasiquoted_single_operand_sum_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(+ ,x))\n"),
            vec!["(defmacro m (x) `,x)\n".to_owned()]
        );
    }

    #[test]
    fn a_spliced_operand_is_not_one_operand() {
        assert_eq!(count("(defmacro m (ns) `(+ ,@ns))\n"), 0);
        assert_eq!(count("(defmacro m (ns) `(* ,@ns))\n"), 0);
    }

    /// Symmetric control for the splice guard.
    #[test]
    fn a_plain_unquoted_operand_is_still_one_operand() {
        assert_eq!(
            fixed("(defmacro m (n) `(* ,n))\n"),
            vec!["(defmacro m (n) `,n)\n".to_owned()]
        );
    }
}
