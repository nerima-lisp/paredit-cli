use anyhow::Result;
use clap::Args;
use serde_json::json;
use std::path::PathBuf;

use crate::rename_control::usecase::{
    RenameControlPlan, RenameControlRequest, plan_rename_block, plan_rename_tag,
};
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::write_file_with_rollback;
use paredit_core_syntax::sexpr::Path;
use paredit_core_syntax::sexpr::SymbolName;

#[derive(Debug, Args)]
pub struct RenameBlockArgs {
    #[command(flatten)]
    common: RenameControlArgs,
}
#[derive(Debug, Args)]
pub struct RenameTagArgs {
    #[command(flatten)]
    common: RenameControlArgs,
}

#[derive(Debug, Args)]
struct RenameControlArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[arg(long)]
    path: Path,
    #[arg(long)]
    from: SymbolName,
    #[arg(long)]
    to: SymbolName,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub fn rename_block(args: RenameBlockArgs) -> Result<()> {
    run(args.common, plan_rename_block)
}
pub fn rename_tag(args: RenameTagArgs) -> Result<()> {
    run(args.common, plan_rename_tag)
}

fn run(
    args: RenameControlArgs,
    // The planner carries this package's typed refusal; `?` widens it into
    // the anyhow result this presentation layer shares with the CLI's own
    // I/O failures.
    planner: fn(RenameControlRequest<'_>) -> crate::error::RenameResult<RenameControlPlan>,
) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect, _) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let plan = planner(RenameControlRequest {
        input: &input.text,
        dialect,
        path: args.path,
        from: args.from,
        to: args.to,
    })?;
    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }
    print_plan(&plan, written, args.output)
}

fn print_plan(plan: &RenameControlPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("reference_count\t{}", plan.reference_count);
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dialect": plan.dialect.label(), "path": plan.path.to_string(),
                "form_span": { "start": plan.form_span.start().get(), "end": plan.form_span.end().get() },
                "reference_count": plan.reference_count, "changed": plan.changed,
                "written": written, "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
