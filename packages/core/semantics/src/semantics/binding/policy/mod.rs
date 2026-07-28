//! Dialect knowledge the binding builder consults but does not own.

mod assignment_heads;
mod head_index;
mod standard_control_forms;
mod standard_declarations;
mod standard_functions;
mod standard_scheme_procedures;

pub use assignment_heads::{PlacePositions, assignment_form, assignment_forms};
pub use standard_control_forms::is_standard_control_form;
pub use standard_declarations::is_standard_declaration_identifier;
pub use standard_functions::is_pure_standard_function;
pub use standard_scheme_procedures::is_pure_standard_scheme_procedure;
