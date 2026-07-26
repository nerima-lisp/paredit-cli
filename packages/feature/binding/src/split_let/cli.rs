use super::*;
use crate::application::usecase::split_let::{SplitLetPlan, SplitLetRequest, plan_split_let};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

#[derive(Debug, Args)]
pub(super) struct SplitLetArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[arg(long)]
    path: Path,
    #[arg(long)]
    binding_index: usize,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn split_let(args: SplitLetArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect, _) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let plan = plan_split_let(SplitLetRequest {
        input: &input.text,
        dialect,
        path: args.path,
        binding_index: args.binding_index,
    })?;
    let written = args.write && plan.changed;
    if written {
        write_file_with_rollback(
            require_output_file(input.file.as_ref())?.clone(),
            plan.rewritten.clone(),
        )?;
    }
    print_plan(&plan, written, args.output)
}

fn print_plan(plan: &SplitLetPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("binding_index\t{}", plan.binding_index);
            println!("outer_binding_count\t{}", plan.outer_binding_count);
            println!("inner_binding_count\t{}", plan.inner_binding_count);
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dialect": plan.dialect.label(), "path": plan.path.to_string(),
                "form_span": { "start": plan.form_span.start().get(), "end": plan.form_span.end().get() },
                "binding_index": plan.binding_index, "outer_binding_count": plan.outer_binding_count,
                "inner_binding_count": plan.inner_binding_count, "changed": plan.changed, "written": written, "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
