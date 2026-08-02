#![doc = include_str!("../README.md")]

pub mod asdf_perform_without_call_next_method;
pub mod asdf_self_referential_depends_on;
pub mod asdf_system_missing_version;
pub mod defpackage_without_in_package;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.
