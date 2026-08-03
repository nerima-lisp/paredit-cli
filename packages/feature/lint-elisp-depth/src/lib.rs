#![doc = include_str!("../README.md")]

pub mod process_filter_assumes_whole_output;
pub mod repeating_timer_handle_discarded;

mod support;

#[cfg(test)]
mod tests;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2). This crate is deliberately left unregistered until
// a separate integration pass wires it.
