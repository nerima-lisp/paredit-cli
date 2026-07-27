use super::*;
use crate::application::usecase::remove_unused_control::{
    RemoveUnusedControlPlan, RemoveUnusedControlRequest, plan_remove_unused_block,
    plan_remove_unused_tag,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

#[derive(Debug, Args)]
pub(super) struct RemoveUnusedBlockArgs {
    #[command(flatten)]
    common: RemoveUnusedControlArgs,
}
#[derive(Debug, Args)]
pub(super) struct RemoveUnusedTagArgs {
    #[command(flatten)]
    common: RemoveUnusedControlArgs,
}
#[derive(Debug, Args)]
struct RemoveUnusedControlArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[arg(long)]
    path: Path,
    #[arg(long)]
    name: String,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}
pub(super) fn remove_unused_block(args: RemoveUnusedBlockArgs) -> Result<()> {
    run(args.common, plan_remove_unused_block)
}
pub(super) fn remove_unused_tag(args: RemoveUnusedTagArgs) -> Result<()> {
    run(args.common, plan_remove_unused_tag)
}
fn run(
    args: RemoveUnusedControlArgs,
    planner: fn(RemoveUnusedControlRequest<'_>) -> Result<RemoveUnusedControlPlan>,
) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect, _) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let plan = planner(RemoveUnusedControlRequest {
        input: &input.text,
        dialect,
        path: args.path,
        name: args.name,
    })?;
    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }
    match args.output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("reference_count\t{}", plan.reference_count);
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({ "dialect": plan.dialect.label(), "path": plan.path.to_string(), "form_span": { "start": plan.form_span.start().get(), "end": plan.form_span.end().get() }, "reference_count": plan.reference_count, "changed": plan.changed, "written": written, "rewritten": plan.rewritten })
            )?
        ),
    }
    Ok(())
}
