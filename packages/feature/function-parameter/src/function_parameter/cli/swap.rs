use paredit_core_cli::CliResult;

use crate::function_parameter::usecase::{
    SwapFunctionParametersRequest, plan_swap_function_parameters,
};
use paredit_core_cli::shared::read_input_and_dialect;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::write_file_with_rollback;

use super::args::SwapFunctionParametersArgs;
use super::render::swap::print_swap_function_parameters_plan;

pub fn swap_function_parameters(args: SwapFunctionParametersArgs) -> CliResult<()> {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_swap_function_parameters(SwapFunctionParametersRequest {
        input: &input.text,
        dialect,
        definition_path: args.definition_path,
        left_name: args.left_name,
        right_name: args.right_name,
        call_paths: args.call_paths,
        all_calls: args.all_calls,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_swap_function_parameters_plan(&plan, written, args.output)
}
