//! The type context: what expressions are provably of what type.
//!
//! Common Lisp only (see [`policy::supports_type_inference`]). The layer
//! stacks on values: a literal's type comes straight from what the value layer
//! read, and everything else is added by declarations, a small hand-written
//! table of standard return types, and flow narrowing through type predicates.
//!
//! The lattice is coarse on purpose. Rules ask "is this definitely a number?",
//! and a coarse answer they can trust is worth more than a precise one they
//! cannot — so anything the sources disagree about, or that falls outside the
//! modelled types, stays `Unknown`.
pub mod model;
pub mod policy;
pub mod service;

pub use model::{Ty, Type, TypeTable};
