use super::{
    AddFunctionParameterRequest, FunctionParameterInsert, FunctionParameterSection,
    MissingArgumentPolicy, MoveFunctionParameterRequest, RemoveFunctionParameterRequest,
    ReorderFunctionParametersRequest, SwapFunctionParametersRequest, plan_add_function_parameter,
    plan_move_function_parameter, plan_remove_function_parameter, plan_reorder_function_parameters,
    plan_swap_function_parameters,
};
use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{Path, SymbolName, SyntaxTree};

fn path(value: &str) -> Path {
    value.parse().expect("path")
}

fn symbol(value: &str) -> SymbolName {
    SymbolName::new(value.to_owned()).expect("symbol")
}

mod add;
mod discovery;
mod move_parameter;
mod pbt;
mod remove;
mod swap_reorder;
