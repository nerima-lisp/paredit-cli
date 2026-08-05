#![doc = include_str!("../README.md")]

pub mod commented_repl_transcript;
pub mod leftover_break_call;
pub mod leftover_format_debug_marker;
pub mod leftover_inspect_call;
pub mod leftover_print_debug;
pub mod leftover_step_call;
pub mod leftover_time_benchmark_call;
pub mod leftover_trace_call;
#[cfg(test)]
mod report_only_tests;
pub mod support;

// One module per rule: the root's REGISTRY names each rule's META and RULE
// across this crate boundary (section 4.2), and each slice's cli owns its own
// subcommand. `cargo xtask new-lint-rule` inserts each new line below,
// keeping the list sorted.
