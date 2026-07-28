//! The vocabulary of the project context.

mod global_table;
mod qualified_symbol;

pub use global_table::{GlobalDefinition, GlobalKind, GlobalTable, GlobalTableBuilder};
pub use qualified_symbol::{PackageId, QualifiedSymbol};
