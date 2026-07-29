use crate::convert_sequential_binding::usecase::{
    ConvertSequentialBindingPlan, ConvertSequentialBindingRequest, plan_convert_do_star_to_do,
    plan_convert_prog_star_to_prog,
};
use clap::Args;
use paredit_core_cli::CliResult;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::write_file_with_rollback;
use paredit_core_syntax::sexpr::Path;
use paredit_core_syntax::sexpr::SymbolName;
use serde_json::json;
use std::path::PathBuf;

macro_rules! conversion_args {
    ($name:ident) => {
        #[derive(Debug, Args)]
        pub struct $name {
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

pub fn convert_do_star_to_do(args: ConvertDoStarToDoArgs) -> CliResult<()> {
    run_conversion(
        args.file,
        args.dialect,
        args.path,
        args.write,
        args.output,
        plan_convert_do_star_to_do,
    )
}

pub fn convert_prog_star_to_prog(args: ConvertProgStarToProgArgs) -> CliResult<()> {
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
    // The planner is a use case in this package, so it carries BindingError;
    // `?` widens it into this function's anyhow result alongside the CLI's own
    // I/O failures.
    planner: for<'a> fn(
        ConvertSequentialBindingRequest<'a>,
    ) -> crate::error::BindingResult<ConvertSequentialBindingPlan>,
) -> CliResult<()> {
    if write && file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
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
) -> CliResult<()> {
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
