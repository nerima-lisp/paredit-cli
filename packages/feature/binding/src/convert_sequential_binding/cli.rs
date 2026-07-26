use super::*;
use crate::application::usecase::convert_sequential_binding::{
    ConvertSequentialBindingPlan, ConvertSequentialBindingRequest, plan_convert_do_star_to_do,
    plan_convert_prog_star_to_prog,
};
use crate::presentation::cli::shared::read_input_dialect_and_tree;

macro_rules! conversion_args {
    ($name:ident) => {
        #[derive(Debug, Args)]
        pub(super) struct $name {
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
    };
}

conversion_args!(ConvertDoStarToDoArgs);
conversion_args!(ConvertProgStarToProgArgs);

pub(super) fn convert_do_star_to_do(args: ConvertDoStarToDoArgs) -> Result<()> {
    run_conversion(
        args.file,
        args.dialect,
        args.path,
        args.write,
        args.output,
        plan_convert_do_star_to_do,
    )
}

pub(super) fn convert_prog_star_to_prog(args: ConvertProgStarToProgArgs) -> Result<()> {
    run_conversion(
        args.file,
        args.dialect,
        args.path,
        args.write,
        args.output,
        plan_convert_prog_star_to_prog,
    )
}

fn run_conversion(
    file: Option<PathBuf>,
    dialect_arg: Option<DialectArg>,
    path: Path,
    write: bool,
    output: OutputFormat,
    planner: for<'a> fn(
        ConvertSequentialBindingRequest<'a>,
    ) -> Result<ConvertSequentialBindingPlan>,
) -> Result<()> {
    if write && file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect, _) = read_input_dialect_and_tree(file, dialect_arg)?;
    let plan = planner(ConvertSequentialBindingRequest {
        input: &input.text,
        dialect,
        path,
    })?;
    let written = write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }
    print_plan(&plan, written, output)
}

fn print_plan(
    plan: &ConvertSequentialBindingPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("binding_count\t{}", plan.binding_names.len());
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
                "binding_names": plan.binding_names.iter().map(SymbolName::as_str).collect::<Vec<_>>(),
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
