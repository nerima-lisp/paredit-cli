//! The vocabulary of the type context.

mod lattice;
mod table;
mod ty;

pub use lattice::{join, meet};
pub use table::{Type, TypeTable, TypeTableBuilder};
pub use ty::Ty;
