//! `nested-char-case`: nested char case ops where the outer dominates ((char-upcase (char-downcase c)) is (char-upcase c)).
//!

use paredit_core_lint_engine::LintResult;

use crate::nested_char_case::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-char-case",
    RuleCategory::Suspicious,
    Severity::Warning,
    "nested char case ops where the outer dominates ((char-upcase (char-downcase c)) is (char-upcase c))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("char-upcase"),
    NormalizedHead::new("char-downcase"),
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
        let mut char_case_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut char_case_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A rewrite of a form inside `'(…)` or `(quote …)` edits a
            // *data literal*, not code, so the finding is dropped rather
            // than fixed. Read on the `hard` counter alone: a `` `(…) ``
            // template's contents really are emitted as code, and going
            // quiet there would abandon the macro bodies this rule exists
            // to read. Asked once per finding, never per visited node.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // (OUTER (INNER c)) is (OUTER c), keeping the outer op.
                let text = format!(
                    "({} {})",
                    context_slice(item.outer_span),
                    context_slice(item.char_span)
                );

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    text,
                    "Collapse the nested char case op".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "the outer char case op dominates; the inner one is dead work".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
