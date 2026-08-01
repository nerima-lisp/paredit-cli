#![doc = include_str!("../README.md")]

pub mod cerror_missing_continue_format;
pub mod define_condition_empty_superclass_list;
pub mod define_condition_missing_report_for_error_type;
pub mod handler_bind_handler_returns_bare_value;
pub mod ignore_errors_wraps_non_error_signal;
pub mod restart_case_clause_without_report;
pub mod signal_on_error_condition_returns_silently;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
