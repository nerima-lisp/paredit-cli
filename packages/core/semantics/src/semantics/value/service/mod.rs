//! Reading literals, folding pure operations, and propagating constants.

mod folding;
mod literal_reader;
mod propagation;

pub use folding::{constant_key, evaluate_constant};
pub use literal_reader::literal_value;
pub use propagation::{ProjectConstants, build_value_table, build_value_table_in_project};
