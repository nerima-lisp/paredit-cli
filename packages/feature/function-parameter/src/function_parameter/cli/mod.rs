pub mod add;
pub mod args;
pub mod move_parameter;
pub mod remove;
pub mod render;
pub mod reorder;
pub mod swap;

// Hoisted for the composition root (section 4.2): the argument type and
// run function of each subcommand this slice owns.
pub use add::add_function_parameter;
pub use args::{
    AddFunctionParameterArgs, MoveFunctionParameterArgs, RemoveFunctionParameterArgs,
    ReorderFunctionParametersArgs, SwapFunctionParametersArgs,
};
pub use move_parameter::move_function_parameter;
pub use remove::remove_function_parameter;
pub use reorder::reorder_function_parameters;
pub use swap::swap_function_parameters;
