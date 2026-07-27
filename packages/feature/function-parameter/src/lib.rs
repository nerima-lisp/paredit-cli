#![doc = include_str!("../README.md")]

pub mod function_parameter;

// The contract with the composition root (section 4.2): each slice that
// owns a subcommand publishes its `clap` argument type and the function
// that runs it. command.rs and dispatch.rs need these two names and no more.
pub use function_parameter::cli::{AddFunctionParameterArgs, add_function_parameter};
pub use function_parameter::cli::{MoveFunctionParameterArgs, move_function_parameter};
pub use function_parameter::cli::{RemoveFunctionParameterArgs, remove_function_parameter};
pub use function_parameter::cli::{ReorderFunctionParametersArgs, reorder_function_parameters};
pub use function_parameter::cli::{SwapFunctionParametersArgs, swap_function_parameters};
