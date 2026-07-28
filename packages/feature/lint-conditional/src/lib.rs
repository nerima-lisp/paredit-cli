#![doc = include_str!("../README.md")]

pub mod case_nil_key;
pub mod cond_t_clause;
pub mod constant_if_test;
pub mod constant_when_test;
pub mod de_morgan;
pub mod dead_boolean_operand;
pub mod duplicate_boolean_operands;
pub mod duplicate_case_keys;
pub mod duplicate_cond_tests;
pub mod empty_body;
pub mod exhaustive_case_otherwise;
pub mod identical_if_branches;
pub mod if_arity;
pub mod if_not;
pub mod if_to_or;
pub mod if_to_unless;
pub mod malformed_case_clause;
pub mod malformed_cond_clause;
pub mod negated_comparison;
pub mod negated_if;
pub mod negated_when_unless;
pub mod nested_boolean;
pub mod nested_unless;
pub mod nested_when;
pub mod one_armed_if;
pub mod quoted_case_key;
pub mod redundant_boolean_identity;
pub mod redundant_if_nil;
pub mod single_clause_cond;
pub mod single_operand_boolean;
pub mod typecase_nil_key;
pub mod unreachable_case_clause;
pub mod unreachable_cond_clause;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
