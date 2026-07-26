//! CLHS knowledge the inference services consult but do not own.

mod dialect_gate;
mod standard_returns;
mod type_predicates;

pub use dialect_gate::supports_type_inference;
pub use standard_returns::standard_return_type;
pub use type_predicates::{narrowed_by_predicate, type_predicate};
