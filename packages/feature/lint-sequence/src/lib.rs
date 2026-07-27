#![doc = include_str!("../README.md")]

pub mod accessor_arity;
pub mod append_list_to_cons;
pub mod append_nil;
pub mod car_nthcdr;
pub mod car_reverse;
pub mod cons_to_list;
pub mod destructive_literal;
pub mod double_reverse;
pub mod eql_search_literal;
pub mod last_default_count;
pub mod list_star_nil;
pub mod list_star_to_cons;
pub mod nth_constant_index;
pub mod nthcdr_small_index;
pub mod nthcdr_zero;
pub mod redundant_count_nil;
pub mod redundant_end_nil;
pub mod redundant_eql_test;
pub mod redundant_from_end_nil;
pub mod redundant_identity_key;
pub mod redundant_start_zero;
pub mod single_operand_list_op;
pub mod subseq_zero;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
