#![doc = include_str!("../README.md")]

pub mod binds_constant;
pub mod eval_when_situation;
pub mod explicit_nil_return;
pub mod handler_case_no_clauses;
pub mod malformed_iteration_spec;
pub mod nested_progn;
pub mod prog2_to_progn;
pub mod redundant_body_progn;
pub mod redundant_prog1;
pub mod redundant_progn;
pub mod unwind_protect_no_cleanup;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
