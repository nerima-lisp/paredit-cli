#![doc = include_str!("../README.md")]

pub mod eq_char_comparison;
pub mod eq_number_comparison;
pub mod eql_list_comparison;
pub mod eql_string_comparison;
pub mod equality_arity;
pub mod explicit_step_delta;
pub mod identity_arithmetic;
pub mod literal_place;
pub mod modify_macro_arity;
pub mod negated_step_delta;
pub mod nil_comparison;
pub mod one_step_arithmetic;
pub mod redundant_divisor;
pub mod self_comparison;
pub mod sign_comparison;
pub mod single_arg_comparison;
pub mod single_operand_arithmetic;
pub mod step_zero;
pub mod t_comparison;
pub mod verbose_negation;
pub mod zero_divisor;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
