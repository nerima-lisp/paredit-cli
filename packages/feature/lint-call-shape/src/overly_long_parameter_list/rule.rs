//! `overly-long-parameter-list`: a definition with more required parameters
//! than a threshold.
//!
//! The analysis lives in [`crate::overly_long_parameter_list::domain`], which
//! also backs the standalone `inspect overly-long-parameter-list` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleSetting,
    RuleTag, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::overly_long_parameter_list::domain::{
    DEFAULT_MAX_REQUIRED, examine_definition, message,
};

/// The knob: how many required parameters a definition may carry.
pub const MAX_REQUIRED: RuleSetting = RuleSetting::new(
    "max-required",
    DEFAULT_MAX_REQUIRED as i64,
    "how many required parameters a definition may declare before it is reported",
);

pub const META: RuleMeta = RuleMeta::new(
    "overly-long-parameter-list",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a definition declaring more required parameters than a threshold",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Style, RuleTag::Pedantic])
.with_settings(&[MAX_REQUIRED])
.with_explanation(
    RuleExplanation::new(
        "A long positional parameter list is unreadable at the call site: nothing there says \
         which argument is which, so every reader has to go and find the lambda list. `&key` \
         parameters name them in place; a structure groups the ones that always travel together.",
    )
    .with_example(
        "(defun render (buffer x y w h color filled dashed) …)",
        "(defun render (buffer rect &key color filled dashed) …)",
    )
    .with_caveat(
        "Only *required* parameters are counted. `&optional`, `&rest`, `&key`, `&aux` and \
         `&body` are the shape this rule suggests, so a definition with three required and nine \
         keyword parameters is never reported.",
    ),
);

/// Exactly the heads `examine_definition` reads.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("defgeneric"),
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
        let max_required = context.setting(META.name().as_str(), MAX_REQUIRED).max(0) as usize;
        let mut definition_count = 0;
        let mut items = Vec::new();
        examine_definition(
            context.tree(),
            view,
            max_required,
            &mut definition_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                message(
                    &item.form,
                    &item.name,
                    item.required_parameter_count,
                    item.threshold,
                ),
            );
        }
        Ok(())
    }
}
