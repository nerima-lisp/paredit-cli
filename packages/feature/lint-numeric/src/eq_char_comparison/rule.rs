//! `eq-char-comparison`: an eq compared against a character literal (eq on characters is unreliable; use eql/char=).
//!

use paredit_core_lint_engine::LintResult;

use crate::eq_char_comparison::domain::{CharacterEvidence, examine_comparison};
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::typing::Ty;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eq-char-comparison",
    RuleCategory::Suspicious,
    Severity::Error,
    "an eq compared against a character literal (eq on characters is unreliable; use eql/char=)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("eq")];

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
        // The type context turns "spelled as a character" into "is a
        // character". `Unknown` says nothing, which is what keeps this silent
        // on an argument the layer could not settle; `Bottom` is excluded
        // because it is a subtype of everything, so a binding whose
        // declarations contradict would otherwise answer "definitely a
        // character" — that is dead code, not an `eq` bug.
        let is_character = |argument: &ExpressionView| {
            NodeKey::of(argument).is_some_and(|key| {
                let ty = context.type_table().expression_type(key);
                ty.is_definitely(Ty::Character) && !ty.is_definitely(Ty::Bottom)
            })
        };

        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(view, &is_character, &mut comparison_form_count, &mut items);
        for item in items {
            // Asked only once a finding exists, so a file with no `eq` against
            // a character never reaches `root_view()` at all. `'((eq #\a) …)`
            // is a data row; a quasiquoted template that becomes code is not,
            // and stays reported.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let fix = {
                RuleFix::single(
                    item.head_span,
                    "eql".to_owned(),
                    "Compare with eql (eq is unreliable on characters)".to_owned(),
                )
            };

            let message = match &item.evidence {
                CharacterEvidence::Literal(literal) => {
                    format!("eq compares against character literal {literal}; use eql or char=")
                }
                CharacterEvidence::InferredType => {
                    "eq compares against an argument of inferred type character; use eql or char="
                        .to_owned()
                }
            };

            sink.report_fixed(span, message, fix);
        }
        Ok(())
    }
}
