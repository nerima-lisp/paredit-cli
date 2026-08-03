#![doc = include_str!("../README.md")]

pub mod begin_single_form;
pub mod let_star_independent_bindings;
pub mod memq_assq_literal_key;
pub mod named_let_never_recurs;
pub mod support;

// One module per rule: the root's REGISTRY names each rule's META and RULE
// across this crate boundary (section 4.2), and each slice's cli owns its own
// subcommand. `cargo xtask new-lint-rule` inserts each new line above,
// keeping the list sorted.
