#![doc = include_str!("../README.md")]

pub mod hook_lambda;
pub mod interactive_arity_mismatch;
pub mod keymap_binds_non_command;
pub mod require_obsolete_cl;
pub mod save_excursion_set_buffer;

mod shared;

#[cfg(test)]
mod tests;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2). This crate is deliberately left unregistered until
// a separate integration pass wires it.
