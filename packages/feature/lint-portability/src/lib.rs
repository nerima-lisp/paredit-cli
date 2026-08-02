#![doc = include_str!("../README.md")]

pub mod ascii_code_char;
pub mod char_code_limit_loop;
pub mod float_equality;
pub mod implementation_package_symbol;
pub mod namestring_round_trip;
pub mod sort_not_guaranteed_stable;
pub mod support;
pub mod unportable_pathname;

// One module per rule: these rules have no standalone `inspect <rule>` command,
// so the domain/usecase/cli split the older lint packages use would be
// indirection with one consumer on the other end.
//
// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).
