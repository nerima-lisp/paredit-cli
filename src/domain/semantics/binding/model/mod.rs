//! The vocabulary of the binding context: what a binding is, what scope it
//! lives in, and the table that holds them.

mod binding;
mod facts;
mod ids;
mod table;

pub use binding::{Binding, BindingDraft};
pub use facts::{BindingKind, ScopeOpacity, SpecialBinding};
pub use ids::{BindingId, ScopeId};
pub use table::{BindingTable, BindingTableBuilder, Scope};
