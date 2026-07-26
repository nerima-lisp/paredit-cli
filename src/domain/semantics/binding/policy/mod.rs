//! Dialect knowledge the binding builder consults but does not own.

mod assignment_heads;
mod standard_control_forms;
mod standard_functions;

pub use assignment_heads::{PlacePositions, assignment_form, assignment_forms};
pub use standard_control_forms::is_standard_control_form;
pub use standard_functions::is_pure_standard_function;
