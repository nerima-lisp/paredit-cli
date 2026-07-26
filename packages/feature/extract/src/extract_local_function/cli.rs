use crate::extract_local_function::usecase::{
    ExtractLocalFunctionPlan, ExtractLocalFunctionRequest, plan_extract_local_function,
};
use anyhow::Result;
use clap::Args;
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

#[derive(Debug, Args)]
pub struct ExtractLocalFunctionArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    /// Select by child index path, for example 0.3.1.
    #[arg(long)]
    path: Path,
    /// Select the enclosing list that receives the local binding.
    #[arg(long)]
    enclosing_path: Path,
    /// New local function name.
    #[arg(long)]
    name: SymbolName,
    /// Explicit formal parameter name. Pass repeatedly in call order.
    #[arg(long = "param")]
    params: Vec<SymbolName>,
    /// Infer formal parameters from value-like atoms in the selected expression.
    #[arg(long)]
    infer_params: bool,
    /// Use labels instead of flet for a recursive local function.
    #[arg(long)]
    recursive: bool,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub fn extract_local_function(args: ExtractLocalFunctionArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }

    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let selection = tree.select_path(&args.path)?;
    let enclosing = tree.select_path(&args.enclosing_path)?;
    let explicit_params = args
        .params
        .iter()
        .map(|param| param.as_str().to_owned())
        .collect();
    let plan = plan_extract_local_function(ExtractLocalFunctionRequest {
        input: &input.text,
        selection,
        path: Some(args.path),
        enclosing,
        enclosing_path: args.enclosing_path,
        dialect,
        name: args.name,
        explicit_params,
        infer_params: args.infer_params,
        recursive: args.recursive,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_extract_local_function_plan(&plan, written, args.output)
}

fn print_extract_local_function_plan(
    plan: &ExtractLocalFunctionPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    let binding = if plan.recursive { "labels" } else { "flet" };
    match output {
        OutputFormat::Text => {
            if let Some(path) = &plan.path {
                println!("path\t{}", safe_text!(path));
            }
            println!("enclosing_path\t{}", safe_text!(plan.enclosing_path));
            println!(
                "span\t{}..{}",
                plan.selected_span.start().get(),
                plan.selected_span.end().get()
            );
            println!(
                "enclosing_span\t{}..{}",
                plan.enclosing_span.start().get(),
                plan.enclosing_span.end().get()
            );
            println!("name\t{}", safe_text!(plan.name));
            println!("params\t{}", safe_text!(plan.params.join(",")));
            println!(
                "inferred_params\t{}",
                safe_text!(plan.inferred_params.join(","))
            );
            println!("binding\t{}", safe_text!(binding));
            println!("call\t{}", safe_text!(plan.call));
            println!("replacement\t{}", safe_text!(plan.replacement));
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "path": plan.path.as_ref().map(ToString::to_string),
                "enclosing_path": plan.enclosing_path.to_string(),
                "span": {
                    "start": plan.selected_span.start().get(),
                    "end": plan.selected_span.end().get(),
                },
                "enclosing_span": {
                    "start": plan.enclosing_span.start().get(),
                    "end": plan.enclosing_span.end().get(),
                },
                "name": plan.name.as_str(),
                "params": &plan.params,
                "inferred_params": &plan.inferred_params,
                "binding": binding,
                "call": &plan.call,
                "replacement": &plan.replacement,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
