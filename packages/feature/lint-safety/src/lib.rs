#![doc = include_str!("../README.md")]

pub mod check_then_act;
pub mod declarative_style_score;
pub mod embedded_secret;
pub mod eval_of_non_constant;
pub mod execution_order_dependency;
pub mod format_tilde_slash_unvalidated_function_designator;
pub mod global_mutation_in_function;
pub mod handler_case_swallows_error;
pub mod insecure_temp_file_fixed_name_shared_directory;
pub mod path_traversal_via_concatenated_filename;
pub mod read_eval_star_rebound_to_t;
pub mod read_without_read_eval_guard;
pub mod sql_query_string_built_via_format;
pub mod stream_escapes_with_open_file;
pub mod subprocess_string_building;
pub mod support;
pub mod unclosed_stream;
pub mod unreachable_handler_clause;
pub mod world_writable_file_mode_in_open_call;

// A false-positive corpus for the six injection-shaped rules, linted through
// the real `HeadFilter::Heads` dispatch rather than a hand-rolled walk. Not part
// of the shipped surface, so it is `cfg(test)` rather than `pub`.
#[cfg(test)]
mod realistic_corpus;

// One module per rule: these rules have no standalone `inspect <rule>` command,
// so the domain/usecase/cli split the older lint packages use would be
// indirection with one consumer on the other end.
//
// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).
