//! Reading literals, folding pure operations, and propagating constants.

mod folding;
mod literal_reader;
mod propagation;

pub use folding::evaluate_constant;
pub use literal_reader::literal_value;
pub use propagation::build_value_table;
