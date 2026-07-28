//! The value context: what expressions provably evaluate to.
//!
//! Built on the binding table, because the question "what is `x` here?" is
//! meaningless without knowing which `x`. Resolution therefore goes through a
//! reference's position, not its name.
//!
//! Three services, in order of what they need: reading a literal atom needs
//! only the atom; folding a pure operation needs its operands' values; and
//! propagating a constant through a binding needs the binding table's verdict
//! that nothing can have changed it.
//!
//! The layer says `Known` only when it can prove the value. `zero-divisor`
//! reporting a false positive on working code is worse than missing a real
//! one, so every uncertainty — an unknown macro, a reader conditional, an
//! arithmetic overflow, an inexact division — collapses to `Unknown`.

pub mod model;
pub mod policy;
pub mod service;

pub use model::{LiteralValue, PropagatableValue, Value, ValueTable};
pub use service::{
    ProjectConstants, build_value_table, build_value_table_in_project, evaluate_constant,
};
