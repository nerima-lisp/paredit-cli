use super::*;
use crate::application::usecase::inline_local_function::{
    InlineLocalFunctionPlan, InlineLocalFunctionRequest, plan_inline_local_function,
};

#[derive(Debug, Args)]
pub(super) struct InlineLocalFunctionArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    /// Select the flet form by child index path, for example 0.
    #[arg(long)]
    path: Path,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn inline_local_function(args: InlineLocalFunctionArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_inline_local_function(InlineLocalFunctionRequest {
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

fn print_plan(plan: &InlineLocalFunctionPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("function_name\t{}", safe_text!(plan.function_name));
            for parameter in &plan.parameters {
                println!(
                    "parameter\t{}\targument={}\treferences={}",
                    safe_text!(parameter.name),
                    safe_text!(parameter.argument),
                    parameter.reference_count
                );
            }
            println!("replacement\t{}", safe_text!(plan.replacement));
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
                "call_span": {
                    "start": plan.call_span.start().get(),
                    "end": plan.call_span.end().get(),
                },
                "function_name": plan.function_name.as_str(),
                "parameters": plan.parameters.iter().map(|parameter| json!({
                    "name": parameter.name.as_str(),
                    "argument": &parameter.argument,
                    "reference_count": parameter.reference_count,
                })).collect::<Vec<_>>(),
                "replacement": &plan.replacement,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
