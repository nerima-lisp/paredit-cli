use anyhow::{Context, Result};

use paredit_core_syntax::common_lisp::common_lisp_symbol_reference_eq;
use paredit_core_syntax::sexpr::SymbolName;

use super::types::{FunctionParameterTarget, ParameterLocation};

pub fn find_unique_parameter_location<'a>(
    target: &'a FunctionParameterTarget,
    parameter_name: &SymbolName,
    operation: &str,
) -> Result<&'a ParameterLocation> {
    let mut found = None;
    for parameter in &target.parameters {
        if common_lisp_symbol_reference_eq(&parameter.name, parameter_name.as_str())
            && found.replace(parameter).is_some()
        {
            anyhow::bail!("{operation} parameter '{parameter_name}' appears more than once");
        }
    }

    found.with_context(|| format!("{operation} parameter '{parameter_name}' was not found"))
}
