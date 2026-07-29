use paredit_core_cli::CliResult;

use paredit_feature_form_transform::sort_definitions::usecase::{
    SortDefinitionsRequest, plan_sort_definitions,
};

use super::args::SortDefinitionsArgs;
use super::render::sort_definitions::print_sort_definitions_plan;
use paredit_core_cli::shared::{read_input_and_dialect, write_file_with_rollback};

pub fn sort_definitions(args: SortDefinitionsArgs) -> CliResult<()> {
    let (input, dialect) = read_input_and_dialect(Some(args.file.clone()), args.dialect)?;
    let plan = plan_sort_definitions(SortDefinitionsRequest {
        file: args.file.clone(),
        input: &input.text,
        dialect,
        strategy: args.order.into(),
        write: args.write,
    })?;
    if plan.written {
        write_file_with_rollback(args.file.clone(), plan.rewritten.clone())?;
    }
    print_sort_definitions_plan(&plan, args.output)
}
