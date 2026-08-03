//! `defstruct-boa-aux-uninitialized-slot`: a BOA constructor binding a slot as
//! a bare `&aux` variable, leaving it uninitialized.
//!
//! The analysis lives in
//! [`crate::defstruct_boa_aux_uninitialized_slot::domain`], which documents the
//! neighbouring premise it replaces — that *omitting* a slot skips its
//! `:initform` — and why that one is false. This module only registers the rule
//! and phrases its findings.

use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::defstruct_boa_aux_uninitialized_slot::domain::examine_defstruct_boa_aux_uninitialized_slot;

pub const META: RuleMeta = RuleMeta::new(
    "defstruct-boa-aux-uninitialized-slot",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a BOA constructor binding a slot as a bare &aux variable, leaving it uninitialized",
    // No fix. The two repairs — give the &aux variable a value form, or drop
    // it so the slot's :initform runs — mean different things, and choosing
    // needs the intent that put the slot in the &aux section.
    Fixability::ReportOnly,
);

/// `examine_defstruct_boa_aux_uninitialized_slot` only matches a `defstruct`.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defstruct")];

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
        let mut defstruct_form_count = 0;
        let mut items = Vec::new();
        examine_defstruct_boa_aux_uninitialized_slot(
            context.tree(),
            view,
            &mut defstruct_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
