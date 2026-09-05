//! `redundant-prog1`: a prog1 wrapping a single form, which is just that form ((prog1 x) is x).
//!

use paredit_core_lint_engine::LintResult;

use crate::redundant_prog1::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-prog1",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a prog1 wrapping a single form, which is just that form ((prog1 x) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("prog1")];

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
        let mut prog1_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut prog1_form_count, &mut items);
        for item in items {
            let span = item.span;
            // Rewriting hard-quoted data edits a user's data literal rather than
            // code, and no round-trip property catches it. Read on the `hard`
            // counter alone: a `` `(…) `` template's contents really are emitted as
            // code. See `support::is_hard_quoted_at`.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // (prog1 x) is x: replace the whole form with its single inner form.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    context_slice(item.form_span),
                    "Drop the single-form prog1".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a prog1 wrapping a single form is just that form; (prog1 x) is x".to_owned(),
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
    fn still_fires_on_an_ordinary_unquoted_prog1() {
        assert_eq!(
            fixed("(defun f () (prog1 (g)))\n"),
            vec!["(defun f () (g))\n".to_owned()]
        );
    }

    /// The measured defect, at this rule's highest-rate shape: 17 of its 19
    /// corpus fixes dropped a prefix.
    #[test]
    fn a_quasiquoted_prog1_keeps_its_backquote() {
        assert_eq!(
            fixed("(defmacro m (x) `(prog1 ,x))\n"),
            vec!["(defmacro m (x) `,x)\n".to_owned()]
        );
    }

    #[test]
    fn still_fires_inside_a_quasiquote_template() {
        assert_eq!(
            fixed("(defmacro m (x) `(when c (prog1 ,x)))\n"),
            vec!["(defmacro m (x) `(when c ,x))\n".to_owned()]
        );
    }
}
