use anyhow::Result;

use super::args::RenameLocalFunctionArgs;
use super::shared::{CallableRenameCommand, CallableRenamePlanData, run_callable_rename};
use crate::application::usecase::rename as rename_usecase;

pub(in crate::presentation::cli) fn rename_local_function(
    args: RenameLocalFunctionArgs,
) -> Result<()> {
    run_callable_rename(
        CallableRenameCommand {
            files: &args.files,
            dialect: args.dialect,
            from: &args.from,
            to: &args.to,
            write: args.write,
            fail_on_no_change: args.fail_on_no_change,
            output: args.output,
            command: "rename-local-function",
            missing_definition_error: "rename-local-function requires at least one matching local function definition",
        },
        |input, dialect| {
            let plan = rename_usecase::plan_rename_local_function(
                rename_usecase::RenameLocalFunctionRequest {
                    input,
                    dialect,
                    from: args.from.clone(),
                    to: args.to.clone(),
                },
            )?;
            Ok(CallableRenamePlanData {
                dialect: plan.dialect,
                definitions: plan.definitions,
                calls: plan.calls,
                rewritten: plan.rewritten,
                changed: plan.changed,
            })
        },
    )
}
