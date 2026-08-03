#![doc = include_str!("../README.md")]

pub mod declaim_inside_body;
pub mod declare_not_at_head_of_body;
pub mod support;
pub mod the_form_with_impossible_type;
pub mod type_declaration_contradicts_initform;
pub mod type_declaration_on_rest_parameter;

#[cfg(test)]
mod corpus_audit;

#[cfg(test)]
mod cost_tests;

#[cfg(test)]
mod engine_pass_tests;

#[cfg(test)]
use paredit_core_lint_engine::rule::RuleEntry;

/// Every rule this package ships.
///
/// The root's `REGISTRY` names each rule's `META` and `RULE` across the crate
/// boundary; this array is the package's own copy, used by the engine tests, the
/// cost measurements and the corpus audit so that all three run the rules
/// through the *real* dispatcher rather than by calling `examine_*` directly.
///
/// A domain test that calls `examine_*` on a node it picked itself stays green
/// no matter what the [`HeadFilter`] says, so nothing below the dispatcher can
/// catch a rule that is unreachable in production.
///
/// [`HeadFilter`]: paredit_core_lint_engine::model::HeadFilter
#[cfg(test)]
pub(crate) static ENTRIES: [RuleEntry; 5] = [
    RuleEntry::new(
        &declare_not_at_head_of_body::rule::META,
        &declare_not_at_head_of_body::rule::RULE,
    ),
    RuleEntry::new(
        &declaim_inside_body::rule::META,
        &declaim_inside_body::rule::RULE,
    ),
    RuleEntry::new(
        &type_declaration_contradicts_initform::rule::META,
        &type_declaration_contradicts_initform::rule::RULE,
    ),
    RuleEntry::new(
        &the_form_with_impossible_type::rule::META,
        &the_form_with_impossible_type::rule::RULE,
    ),
    RuleEntry::new(
        &type_declaration_on_rest_parameter::rule::META,
        &type_declaration_on_rest_parameter::rule::RULE,
    ),
];
