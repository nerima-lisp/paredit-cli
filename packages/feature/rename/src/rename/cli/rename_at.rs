use anyhow::{Context, Result};

use super::super::{read_input_and_dialect, write_file_with_rollback};
use super::args::RenameAtArgs;
use super::render::at::print_rename_at_plan;
use super::shared::ensure_rename_changed;
use crate::application::usecase::rename::{RenameAtRequest, plan_rename_at};
use crate::domain::sexpr::ByteOffset;

pub(in crate::presentation::cli) fn rename_at(args: RenameAtArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
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
        let file = args.file.as_ref().context("--write requires --file")?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }
    print_rename_at_plan(&plan, written, args.output)?;
    ensure_rename_changed(args.fail_on_no_change, plan.changed, "rename-at")
}
