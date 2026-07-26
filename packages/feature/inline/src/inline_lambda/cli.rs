use super::*;
use crate::application::usecase::inline_lambda::{
    InlineLambdaPlan, InlineLambdaRequest, plan_inline_lambda,
};

#[derive(Debug, Args)]
pub(super) struct InlineLambdaArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    /// Select the immediately invoked lambda call by child index path.
    #[arg(long)]
    path: Path,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub(super) fn inline_lambda(args: InlineLambdaArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect) = read_input_and_dialect(args.file.clone(), args.dialect)?;
    let plan = plan_inline_lambda(InlineLambdaRequest {
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

fn print_plan(plan: &InlineLambdaPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            for binding in &plan.bindings {
                println!(
                    "binding\t{}\targument={}",
                    safe_text!(binding.name),
                    safe_text!(binding.argument)
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
                "call_span": {
                    "start": plan.call_span.start().get(),
                    "end": plan.call_span.end().get(),
                },
                "lambda_span": {
                    "start": plan.lambda_span.start().get(),
                    "end": plan.lambda_span.end().get(),
                },
                "bindings": plan.bindings.iter().map(|binding| json!({
                    "name": binding.name.as_str(),
                    "argument": &binding.argument,
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
