use anyhow::Result;

use crate::application::usecase::function_parameter::{
    MoveFunctionParameterRequest, plan_move_function_parameter,
};
use crate::presentation::cli::{
    read_input_and_dialect, require_output_file, write_file_with_rollback,
};

use super::args::MoveFunctionParameterArgs;
use super::render::move_parameter::print_move_function_parameter_plan;

pub(in crate::presentation::cli) fn move_function_parameter(
    args: MoveFunctionParameterArgs,
) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_move_function_parameter(MoveFunctionParameterRequest {
        input: &input.text,
        dialect,
        definition_path: args.definition_path,
        name: args.name,
        to_index: args.to_index,
        call_paths: args.call_paths,
        all_calls: args.all_calls,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_move_function_parameter_plan(&plan, written, args.output)
}
