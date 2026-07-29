use paredit_core_cli::CliResult;

use crate::function_parameter::usecase::{
    MissingArgumentPolicy, RemoveFunctionParameterRequest, plan_remove_function_parameter,
};
use paredit_core_cli::shared::read_input_and_dialect;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::write_file_with_rollback;

use super::args::RemoveFunctionParameterArgs;
use super::render::remove::print_remove_function_parameter_plan;

pub fn remove_function_parameter(args: RemoveFunctionParameterArgs) -> CliResult<()> {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_remove_function_parameter(RemoveFunctionParameterRequest {
        input: &input.text,
        dialect,
        definition_path: args.definition_path,
        name: args.name,
        call_paths: args.call_paths,
        all_calls: args.all_calls,
        missing_argument_policy: if args.allow_missing_argument {
            MissingArgumentPolicy::Ignore
        } else {
            MissingArgumentPolicy::Reject
        },
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_remove_function_parameter_plan(&plan, written, args.output)
}
