#![doc = include_str!("../README.md")]

pub mod dolist_result_form_references_loop_variable;
pub mod dotimes_bound_mutation_has_no_effect;
pub mod loop_clause_order_violation;
pub mod loop_for_across_statically_known_list;
pub mod loop_into_accumulator_kind_conflict;
pub mod loop_syntax;
pub mod loop_unreachable_finally_clause;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
