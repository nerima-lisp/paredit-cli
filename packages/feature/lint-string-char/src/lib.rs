#![doc = include_str!("../README.md")]

pub mod char_case_fold;
pub mod char_op_string;
pub mod code_char_char_code;
pub mod format_missing_destination;
pub mod format_newline;
pub mod format_to_string;
pub mod nested_string_case;
pub mod string_case_fold;

// The composition root's REGISTRY names each rule's META and RULE across the
// crate boundary; the inspect subcommands are reached through each slice's cli.
