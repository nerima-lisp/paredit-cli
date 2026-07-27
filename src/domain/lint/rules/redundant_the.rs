//! `redundant-the`: a (the t form) type declaration, which is vacuous and is just form (t matches every object).
//!
//! The analysis lives in [`crate::domain::redundant_the_report`], which also backs the
//! standalone `inspect redundant-the` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_the_report::{TheRedundancy, examine_the};
use crate::domain::semantics::NodeKey;
use crate::domain::semantics::typing::Ty;
use crate::domain::sexpr::ExpressionView;
use crate::domain::sexpr::reader::atom_symbol_text;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-the",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (the t form) type declaration, which is vacuous and is just form (t matches every object)",
    Fixability::Fixable,
);

/// `examine_the` only ever matches a `the` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("the")];

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
    ) -> Result<()> {
        let context_slice = |span| context.slice(span).to_owned();

        // Whether the form already provably has the type the `the` asserts,
        // which makes the assertion add nothing.
        //
        // An unmodelled or compound specifier (`fixnum`, `(integer 0 9)`)
        // yields `None` from `Ty::from_name` and the answer is no — this
        // layer declines to name those types, and a `the` it cannot read is
        // not one it may delete.
        //
        // `Bottom` is excluded because it is a subtype of everything, so a
        // form whose declarations contradict would otherwise satisfy every
        // assertion. That is dead code, not a redundant declaration.
        let already_known = |value_type: &ExpressionView, form: &ExpressionView| {
            let Some(asserted) = atom_symbol_text(value_type).and_then(Ty::from_name) else {
                return false;
            };
            NodeKey::of(form).is_some_and(|key| {
                let ty = context.type_table().expression_type(key);
                ty.is_definitely(asserted) && !ty.is_definitely(Ty::Bottom)
            })
        };

        let mut the_form_count = 0;
        let mut items = Vec::new();
        examine_the(
            view,
            context.path(),
            &already_known,
            &mut the_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let (message, hint) = match &item.redundancy {
                TheRedundancy::Vacuous => (
                    "(the t form) is a vacuous type declaration; it is just form".to_string(),
                    "Drop the vacuous (the t …) declaration".to_string(),
                ),
                TheRedundancy::AlreadySatisfied(name) => (
                    format!(
                        "(the {name} form) asserts a type the form already has; it is just form"
                    ),
                    format!("Drop the (the {name} …) declaration the form already satisfies"),
                ),
            };

            // (the TYPE form) is form: replace the whole declaration with the
            // inner form.
            let fix = RuleFix::single(item.span, context_slice(item.form_span), hint);

            sink.report_fixed(span, message, fix);
        }
        Ok(())
    }
}
