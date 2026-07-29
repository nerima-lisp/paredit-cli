use crate::extract_function::usecase::{
    ExtractFunctionInsert, ExtractFunctionPlan, ExtractFunctionRequest, plan_extract_function,
};
use anyhow::Result;
use clap::Args;
use paredit_core_cli::args::CompactSelectorArgs;
use paredit_core_cli::args::DialectArg;
use paredit_core_cli::args::MoveInsert;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use paredit_core_cli::shared::read_input_dialect_and_tree;
use paredit_core_cli::shared::require_output_file;
use paredit_core_cli::shared::resolve_compact_target;
use paredit_core_cli::shared::write_file_with_rollback;
use paredit_core_syntax::sexpr::Path;
use paredit_core_syntax::sexpr::SymbolName;
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ExtractFunctionArgs {
    /// Input file. Required when --write is used; reads stdin otherwise.
    #[arg(short, long)]
    file: Option<PathBuf>,
    /// Override extension-based dialect detection.
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[command(flatten)]
    selector: CompactSelectorArgs,
    /// New top-level function name.
    #[arg(long)]
    name: SymbolName,
    /// Explicit formal parameter name. Pass repeatedly in call order.
    #[arg(long = "param")]
    params: Vec<SymbolName>,
    /// Infer formal parameters from value-like atoms in the selected expression.
    #[arg(long)]
    infer_params: bool,
    /// Top-level insertion strategy for the generated helper.
    #[arg(long, value_enum, default_value_t = MoveInsert::Append)]
    insert: MoveInsert,
    /// Top-level anchor path. Required for --insert before/after.
    #[arg(long)]
    anchor_path: Option<Path>,
    /// Rewrite the input file in place. Without this flag, only prints a plan.
    #[arg(long)]
    write: bool,
    /// Output format for agent consumption.
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub fn extract_function(args: ExtractFunctionArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    if args.insert == MoveInsert::Append && args.anchor_path.is_some() {
        anyhow::bail!("--anchor-path is only valid with --insert before or --insert after");
    }
    if matches!(args.insert, MoveInsert::Before | MoveInsert::After) && args.anchor_path.is_none() {
        anyhow::bail!("--insert before/after requires --anchor-path");
    }

    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let target =
        resolve_compact_target(&tree, dialect, &args.selector, "refactor extract-function")?;
    let selection = tree.select_path(&target.path)?;
    let explicit_params = args
        .params
        .iter()
        .map(|param| param.as_str().to_owned())
        .collect();
    let plan = plan_extract_function(ExtractFunctionRequest {
        input: &input.text,
        selection,
        path: Some(target.path),
        dialect,
        name: args.name,
        explicit_params,
        infer_params: args.infer_params,
        insert: extract_function_insert(args.insert),
        anchor_path: args.anchor_path,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }

    print_extract_function_plan(&plan, written, args.output)
}

fn print_extract_function_plan(
    plan: &ExtractFunctionPlan,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            if let Some(path) = &plan.path {
                println!("path\t{}", safe_text!(path));
            }
            println!("span\t{}..{}", plan.span_start, plan.span_end);
            println!("name\t{}", safe_text!(plan.name));
            println!("params\t{}", safe_text!(plan.params.join(",")));
            println!(
                "inferred_params\t{}",
                safe_text!(plan.inferred_params.join(","))
            );
            println!("insert\t{}", plan.insert.label());
            if let Some(path) = &plan.anchor_path {
                println!("anchor_path\t{}", safe_text!(path));
            }
            if let Some(span) = plan.anchor_span {
                println!("anchor_span\t{}..{}", span.start().get(), span.end().get());
            }
            println!("call\t{}", safe_text!(plan.call));
            println!("definition\t{}", safe_text!(plan.definition));
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": plan.dialect.label(),
                "path": plan.path.as_ref().map(ToString::to_string),
                "span": {
                    "start": plan.span_start,
                    "end": plan.span_end,
                },
                "name": plan.name.as_str(),
                "params": &plan.params,
                "inferred_params": &plan.inferred_params,
                "insert": plan.insert.label(),
                "anchor_path": plan.anchor_path.as_ref().map(ToString::to_string),
                "anchor_span": plan.anchor_span.map(|span| json!({
                    "start": span.start().get(),
                    "end": span.end().get(),
                })),
                "call": &plan.call,
                "definition": &plan.definition,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}

/// Maps the CLI's insert position onto the use case's.
///
/// Lives here rather than on `MoveInsert` so that `cli::args` - which becomes
/// part of `core/cli` - does not name a feature use case. The dependency then
/// runs feature -> core, which is the direction the crate graph allows.
const fn extract_function_insert(insert: MoveInsert) -> ExtractFunctionInsert {
    match insert {
        MoveInsert::Append => ExtractFunctionInsert::Append,
        MoveInsert::Before => ExtractFunctionInsert::Before,
        MoveInsert::After => ExtractFunctionInsert::After,
    }
}
