use crate::error::{FunctionParameterResult, ParameterSelectionError};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::sexpr::SymbolName;

use super::types::{FunctionParameterTarget, ParameterLocation};

pub fn find_unique_parameter_location<'a>(
    target: &'a FunctionParameterTarget,
    parameter_name: &SymbolName,
    operation: &'static str,
) -> FunctionParameterResult<&'a ParameterLocation> {
    let mut found = None;
    for parameter in &target.parameters {
        if common_lisp_symbol_reference_eq(&parameter.name, parameter_name.as_str())
            && found.replace(parameter).is_some()
        {
            return Err(ParameterSelectionError::Duplicate {
                operation,
                name: parameter_name.to_string(),
            }
            .into());
        }
    }

    found.ok_or_else(|| {
        ParameterSelectionError::NotFound {
            operation,
            name: parameter_name.to_string(),
        }
        .into()
    })
}
