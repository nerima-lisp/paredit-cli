#![doc = include_str!("../README.md")]

pub mod check_then_act;
pub mod embedded_secret;
pub mod eval_of_non_constant;
pub mod execution_order_dependency;
pub mod global_mutation_in_function;
pub mod handler_case_swallows_error;
pub mod read_without_read_eval_guard;
pub mod stream_escapes_with_open_file;
pub mod subprocess_string_building;
pub mod unclosed_stream;
pub mod unreachable_handler_clause;

// One module per rule: these rules have no standalone `inspect <rule>` command,
// so the domain/usecase/cli split the older lint packages use would be
// indirection with one consumer on the other end.
//
// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).
