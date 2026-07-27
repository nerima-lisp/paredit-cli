use anyhow::Result;

use super::args::RenameMacroletArgs;
use super::shared::{CallableRenameCommand, CallableRenamePlanData, run_callable_rename};
use crate::application::usecase::rename as rename_usecase;

pub(in crate::presentation::cli) fn rename_macrolet(args: RenameMacroletArgs) -> Result<()> {
    run_callable_rename(
        CallableRenameCommand {
            files: &args.files,
            dialect: args.dialect,
            from: &args.from,
            to: &args.to,
            write: args.write,
            fail_on_no_change: args.fail_on_no_change,
            output: args.output,
            command: "rename-macrolet",
            missing_definition_error: "rename-macrolet requires at least one matching macrolet or compiler-macrolet definition",
        },
        |input, dialect| {
            let plan =
                rename_usecase::plan_rename_macrolet(rename_usecase::RenameMacroletRequest {
                    input,
                    dialect,
                    from: args.from.clone(),
                    to: args.to.clone(),
                })?;
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
