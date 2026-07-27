//! Use-case planners for function parameter refactorings.

mod add;
mod calls;
mod definition;
mod list_edit;
mod move_parameter;
mod remove;
mod reorder;
mod swap;
#[cfg(test)]
mod tests;
mod types;

pub use add::plan_add_function_parameter;
pub use move_parameter::plan_move_function_parameter;
pub use remove::plan_remove_function_parameter;
pub use reorder::plan_reorder_function_parameters;
pub use swap::plan_swap_function_parameters;
pub use types::{
    AddFunctionParameterPlan, AddFunctionParameterRequest, FunctionParameterInsert,
    FunctionParameterSection, MissingArgumentPolicy, MoveFunctionParameterPlan,
    MoveFunctionParameterRequest, RemoveFunctionParameterPlan, RemoveFunctionParameterRequest,
    ReorderFunctionParametersPlan, ReorderFunctionParametersRequest, SwapFunctionParametersPlan,
    SwapFunctionParametersRequest,
};

/// Lists every declared parameter name in `parameter_form`, reusing the same
/// validated lambda-list parser the parameter add/remove/reorder refactors
/// rely on, so unused-parameter detection agrees with them on every edge
/// case (CL `&optional`/`&rest`/`&key`/`&aux` markers, `defmethod` type
/// specializer lists, dotted lambda-list tails) instead of drifting from a
/// second, less battle-tested implementation.
pub(crate) fn list_lambda_list_parameter_names(
    dialect: crate::domain::dialect::Dialect,
    parameter_form: &crate::domain::sexpr::ExpressionView,
) -> anyhow::Result<Vec<String>> {
    definition::parameter_locations(dialect, parameter_form, 0, true, "list-parameters").map(
        |locations| {
            locations
                .into_iter()
                .map(|location| location.name)
                .collect()
        },
    )
}
