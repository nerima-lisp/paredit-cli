use anyhow::Result;

use crate::application::usecase::function_parameter::{
    ReorderFunctionParametersRequest, plan_reorder_function_parameters,
};
use crate::presentation::cli::{
    read_input_and_dialect, require_output_file, write_file_with_rollback,
};

use super::args::ReorderFunctionParametersArgs;
use super::render::reorder::print_reorder_function_parameters_plan;

pub(in crate::presentation::cli) fn reorder_function_parameters(
    args: ReorderFunctionParametersArgs,
) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_reorder_function_parameters(ReorderFunctionParametersRequest {
        input: &input.text,
        dialect,
        definition_path: args.definition_path,
        parameter_order: args.parameter_order,
        call_paths: args.call_paths,
        all_calls: args.all_calls,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_reorder_function_parameters_plan(&plan, written, args.output)
}
