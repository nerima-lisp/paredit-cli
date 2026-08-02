#![doc = include_str!("../README.md")]

pub mod copy_before_destructive;
pub mod length_emptiness_test;
pub mod linear_search_in_loop;
pub mod loop_invariant_allocation;
pub mod quadratic_accumulation;
pub mod redundant_full_sequence_traversal_fusable_maps;
pub mod repeated_hash_table_lookup_same_key;
mod shared;
mod support;
pub mod unnecessary_copy;
pub mod unnecessary_sort_before_extremum_extraction;

#[cfg(test)]
mod realistic_corpus;

// One module per rule, rather than the `domain`/`usecase`/`cli` split the older
// lint packages use. That split exists so a rule's detection can be shared with
// its own standalone `inspect <rule>` command; none of these rules has one, so
// there is one consumer and splitting it across three files would be indirection
// with nothing on the other end.
//
// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).
