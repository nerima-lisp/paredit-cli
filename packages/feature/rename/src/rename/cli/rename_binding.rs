use anyhow::{Context, Result};

use super::super::{read_input_and_dialect, write_file_with_rollback};
use super::args::RenameBindingArgs;
use super::render::binding::print_rename_binding_plan;
use super::shared::{ensure_rename_changed, rename_target};
use crate::application::usecase::rename as rename_usecase;

pub(in crate::presentation::cli) fn rename_binding(args: RenameBindingArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }

    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = rename_usecase::plan_rename_binding(rename_usecase::RenameBindingRequest {
        input: &input.text,
        dialect,
        target: rename_target(args.path, args.at)?,
        from: args.from,
        to: args.to,
    })?;
    let written = args.write && plan.changed;
    if written {
        let file = args.file.as_ref().context("--write requires --file")?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_rename_binding_plan(&plan, written, args.output)?;
    ensure_rename_changed(args.fail_on_no_change, plan.changed, "rename-binding")
}
