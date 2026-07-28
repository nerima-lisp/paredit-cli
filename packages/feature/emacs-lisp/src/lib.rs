#![doc = include_str!("../README.md")]

pub mod autoload_cookie_without_form;
pub mod condition_case_without_handler;
pub mod defcustom_missing_group;
pub mod defcustom_missing_type;
pub mod file_report;
pub mod interactive_in_macro;
pub mod missing_lexical_binding;
pub mod obsolete_cl_alias;
pub mod quoted_lambda;
pub mod unreachable_lexical_binding;

mod shared;

#[cfg(test)]
mod tests;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).
