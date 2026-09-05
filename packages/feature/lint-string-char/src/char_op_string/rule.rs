//! `char-op-string`: a character function (char=/char-code/alpha-char-p/...) applied to a string literal (type error).
//!

use paredit_core_lint_engine::LintResult;

use crate::char_op_string::domain::{CharacterMismatch, examine_call};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::typing::Ty;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "char-op-string",
    RuleCategory::Malformed,
    Severity::Error,
    "a character function (char=/char-code/alpha-char-p/...) applied to a string literal (type error)",
    Fixability::ReportOnly,
);

/// Every function `examine_call` recognizes as requiring character arguments.
const HEADS: [NormalizedHead; 25] = [
    NormalizedHead::new("char="),
    NormalizedHead::new("char/="),
    NormalizedHead::new("char<"),
    NormalizedHead::new("char>"),
    NormalizedHead::new("char<="),
    NormalizedHead::new("char>="),
    NormalizedHead::new("char-equal"),
    NormalizedHead::new("char-not-equal"),
    NormalizedHead::new("char-lessp"),
    NormalizedHead::new("char-greaterp"),
    NormalizedHead::new("char-not-lessp"),
    NormalizedHead::new("char-not-greaterp"),
    NormalizedHead::new("char-code"),
    NormalizedHead::new("char-int"),
    NormalizedHead::new("char-upcase"),
    NormalizedHead::new("char-downcase"),
    NormalizedHead::new("char-name"),
    NormalizedHead::new("digit-char-p"),
    NormalizedHead::new("alpha-char-p"),
    NormalizedHead::new("alphanumericp"),
    NormalizedHead::new("upper-case-p"),
    NormalizedHead::new("lower-case-p"),
    NormalizedHead::new("both-case-p"),
    NormalizedHead::new("graphic-char-p"),
    NormalizedHead::new("standard-char-p"),
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
        // The type context turns "spelled as a string" into "cannot be a
        // character". The question is asked in the negative on purpose:
        // `is_definitely(Ty::String)` would miss `(char= (length xs) c)`,
        // while `is_definitely_not(Ty::Character)` catches every settled type
        // that shares no member with `character` and still says nothing about
        // an argument the layer could not settle.
        //
        // `Bottom` is excluded because it shares no member with anything, so a
        // binding whose declarations contradict would otherwise answer
        // "definitely not a character" — that is dead code, not a type error
        // at this call.
        let is_non_character = |argument: &ExpressionView| {
            NodeKey::of(argument).is_some_and(|key| {
                let ty = context.type_table().expression_type(key);
                ty.is_definitely_not(Ty::Character) && !ty.is_definitely(Ty::Bottom)
            })
        };

        let mut char_call_count = 0;
        let mut items = Vec::new();
        examine_call(view, &is_non_character, &mut char_call_count, &mut items);
        for item in items {
            let span = item.span;

            let message = match &item.mismatch {
                CharacterMismatch::StringLiteral(literal) => format!(
                    "{} is given string literal {literal}; it requires a character (type error)",
                    item.operator
                ),
                CharacterMismatch::InferredType => format!(
                    "{} is given an argument of an inferred non-character type; it requires a character (type error)",
                    item.operator
                ),
            };

            sink.report(span, message);
        }
        Ok(())
    }
}
