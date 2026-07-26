use super::*;
use crate::application::usecase::flatten_progn::{
    FlattenPrognPlan, FlattenPrognRequest, plan_flatten_progn,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

#[derive(Debug, Args)]
pub(super) struct FlattenPrognArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[arg(long)]
    path: Path,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn flatten_progn(args: FlattenPrognArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect, _) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let plan = plan_flatten_progn(FlattenPrognRequest {
        input: &input.text,
        dialect,
        path: args.path,
    })?;
    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }
    print_plan(&plan, written, args.output)
}

fn print_plan(plan: &FlattenPrognPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("nested_count\t{}", plan.nested_count);
            println!("result_form_count\t{}", plan.result_form_count);
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "dialect": plan.dialect.label(),
                "path": plan.path.to_string(),
                "form_span": {
                    "start": plan.form_span.start().get(),
                    "end": plan.form_span.end().get(),
                },
                "nested_count": plan.nested_count,
                "result_form_count": plan.result_form_count,
                "changed": plan.changed,
                "written": written,
                "replacement": plan.replacement,
                "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
