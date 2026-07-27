use super::*;
use crate::application::usecase::rename_control::{
    RenameControlPlan, RenameControlRequest, plan_rename_block, plan_rename_tag,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

#[derive(Debug, Args)]
pub(super) struct RenameBlockArgs {
    #[command(flatten)]
    common: RenameControlArgs,
}
#[derive(Debug, Args)]
pub(super) struct RenameTagArgs {
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

pub(super) fn rename_block(args: RenameBlockArgs) -> Result<()> {
    run(args.common, plan_rename_block)
}
pub(super) fn rename_tag(args: RenameTagArgs) -> Result<()> {
    run(args.common, plan_rename_tag)
}

fn run(
    args: RenameControlArgs,
    planner: fn(RenameControlRequest<'_>) -> Result<RenameControlPlan>,
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
