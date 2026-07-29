use paredit_core_cli::CommandResult;

use super::args::RenameInFormArgs;
use super::render::scoped_form::print_rename_in_form_plan;
use super::shared::{ensure_rename_changed, rename_target};
use crate::rename::usecase as rename_usecase;
use paredit_core_cli::shared::{read_input_and_dialect, write_file_with_rollback};

pub fn rename_in_form(args: RenameInFormArgs) -> CommandResult {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = rename_usecase::plan_rename_in_form(rename_usecase::RenameInFormRequest {
        input: &input.text,
        dialect,
        target: rename_target(args.path, args.at)?,
        from: args.from,
        to: args.to,
    })?;
    let written = args.write && plan.changed;
    if written {
        let file = args
            .file
            .as_ref()
            .ok_or(paredit_core_cli::ArgumentError::WriteRequiresFile)?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_rename_in_form_plan(&plan, written, args.output)?;
    ensure_rename_changed(args.fail_on_no_change, plan.changed, "rename-in-form")
}
