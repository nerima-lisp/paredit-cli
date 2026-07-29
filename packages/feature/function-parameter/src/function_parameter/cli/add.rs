use paredit_core_cli::CliResult;

use crate::function_parameter::usecase::{
    AddFunctionParameterRequest, FunctionParameterInsert, plan_add_function_parameter,
};
use paredit_core_cli::shared::read_input_and_dialect;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::write_file_with_rollback;

use super::args::AddFunctionParameterArgs;
use super::render::add::print_add_function_parameter_plan;
use paredit_core_cli::args::ParameterInsert;

pub fn add_function_parameter(args: AddFunctionParameterArgs) -> CliResult<()> {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_add_function_parameter(AddFunctionParameterRequest {
        input: &input.text,
        dialect,
        definition_path: args.definition_path,
        name: args.name,
        argument: args.argument,
        call_paths: args.call_paths,
        all_calls: args.all_calls,
        insert: function_parameter_insert(args.insert),
        section: args.section.into_function_parameter_section(),
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_add_function_parameter_plan(&plan, written, args.output)
}

/// Maps the CLI's insert position onto the use case's.
///
/// Lives here rather than on `ParameterInsert` so that `cli::args` - which
/// becomes part of `core/cli` - does not name a feature use case.
const fn function_parameter_insert(insert: ParameterInsert) -> FunctionParameterInsert {
    match insert {
        ParameterInsert::Start => FunctionParameterInsert::Start,
        ParameterInsert::End => FunctionParameterInsert::End,
    }
}
