#![doc = include_str!("../README.md")]

pub mod directory_without_wild_component;
pub mod output_stream_without_if_exists;
pub mod pathname_built_by_concatenation;
pub mod pathname_component_compared_case_sensitively;
pub mod support;
pub mod with_open_file_result_captures_stream;

// One module per rule: these rules have no standalone `inspect <rule>` command,
// so the domain/usecase/cli split the older lint packages use would be
// indirection with one consumer on the other end.
//
// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).
