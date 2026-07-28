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
pub fn list_lambda_list_parameter_names(
    dialect: paredit_core_syntax::dialect::Dialect,
    parameter_form: &paredit_core_syntax::sexpr::ExpressionView,
) -> crate::error::FunctionParameterResult<Vec<String>> {
    list_lambda_list_parameter_names_from(dialect, parameter_form, 0)
}

/// As [`list_lambda_list_parameter_names`], but skipping a leading prefix of
/// the lambda list that is not made of parameters.
///
/// Scheme's `(define (f x y) ...)` keeps the procedure name in the same node
/// as its parameters, so callers pass
/// `DefinitionShape::lambda_list_first_parameter_index` here. Reading from 0
/// reports `f` as a parameter of itself.
pub fn list_lambda_list_parameter_names_from(
    dialect: paredit_core_syntax::dialect::Dialect,
    parameter_form: &paredit_core_syntax::sexpr::ExpressionView,
    first_parameter_index: usize,
) -> crate::error::FunctionParameterResult<Vec<String>> {
    definition::parameter_locations(
        dialect,
        parameter_form,
        first_parameter_index,
        true,
        "list-parameters",
    )
    .map(|locations| {
        locations
            .into_iter()
            .map(|location| location.name)
            .collect()
    })
}
