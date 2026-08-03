#![doc = include_str!("../README.md")]

pub mod class_allocated_slot_with_initarg;
pub mod defgeneric_method_option_incongruent;
pub mod initialization_primary_without_call_next_method;
pub mod support;

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
/// boundary — this package is **not yet registered there**, deliberately; a
/// separate pass wires it. This array is the package's own copy, used by the
/// engine tests, the cost measurements and the corpus audit so that all three
/// run the rules through the *real* dispatcher rather than by calling `examine_*`
/// directly.
///
/// A domain test that calls `examine_*` on a node it picked itself stays green
/// no matter what the [`HeadFilter`] says, so nothing below the dispatcher can
/// catch a rule that is unreachable in production.
///
/// [`HeadFilter`]: paredit_core_lint_engine::model::HeadFilter
#[cfg(test)]
pub(crate) static ENTRIES: [RuleEntry; 3] = [
    RuleEntry::new(
        &defgeneric_method_option_incongruent::rule::META,
        &defgeneric_method_option_incongruent::rule::RULE,
    ),
    RuleEntry::new(
        &initialization_primary_without_call_next_method::rule::META,
        &initialization_primary_without_call_next_method::rule::RULE,
    ),
    RuleEntry::new(
        &class_allocated_slot_with_initarg::rule::META,
        &class_allocated_slot_with_initarg::rule::RULE,
    ),
];
