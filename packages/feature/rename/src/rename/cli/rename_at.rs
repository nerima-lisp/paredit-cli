use paredit_core_cli::CommandResult;

use super::args::RenameAtArgs;
use super::render::at::print_rename_at_plan;
use super::shared::ensure_rename_changed;
use crate::rename::usecase::{RenameAtRequest, plan_rename_at};
use paredit_core_cli::shared::{read_input_and_dialect, write_file_with_rollback};
use paredit_core_syntax::sexpr::ByteOffset;

pub fn rename_at(args: RenameAtArgs) -> CommandResult {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }
    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_rename_at(RenameAtRequest {
        input: &input.text,
        dialect,
        at: ByteOffset::new(args.at),
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
    print_rename_at_plan(&plan, written, args.output)?;
    ensure_rename_changed(args.fail_on_no_change, plan.changed, "rename-at")
}
