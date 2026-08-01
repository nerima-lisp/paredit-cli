//! Inferring types and narrowing them along control flow.

mod calls;
mod declarations;
mod emacs_lisp_declarations;
mod inference;
mod narrowing;

pub use inference::build_type_table;
