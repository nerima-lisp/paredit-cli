use crate::extract_constant::usecase::{
    ExtractConstantInsert, ExtractConstantPlan, ExtractConstantRequest, plan_extract_constant,
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
pub struct ExtractConstantArgs {
    #[arg(short, long)]
    file: Option<PathBuf>,
    #[arg(long)]
    dialect: Option<DialectArg>,
    #[command(flatten)]
    selector: CompactSelectorArgs,
    #[arg(long)]
    name: SymbolName,
    #[arg(long, value_enum, default_value_t = MoveInsert::Append)]
    insert: MoveInsert,
    #[arg(long)]
    anchor_path: Option<Path>,
    #[arg(long)]
    write: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,
}

pub fn extract_constant(args: ExtractConstantArgs) -> Result<()> {
    validate_args(&args)?;
    let (input, dialect, tree) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let target =
        resolve_compact_target(&tree, dialect, &args.selector, "refactor extract-constant")?;
    let selection = tree.select_path(&target.path)?;
    let path = target.path.clone();
    let plan = plan_extract_constant(ExtractConstantRequest {
        input: &input.text,
        tree: &tree,
        selection,
        path,
        dialect,
        name: args.name,
        insert: match args.insert {
            MoveInsert::Append => ExtractConstantInsert::Append,
            MoveInsert::Before => ExtractConstantInsert::Before,
            MoveInsert::After => ExtractConstantInsert::After,
        },
        anchor_path: args.anchor_path,
    })?;

    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }
    print_plan(&plan, written, args.output)
}

fn validate_args(args: &ExtractConstantArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    if args.insert == MoveInsert::Append && args.anchor_path.is_some() {
        anyhow::bail!("--anchor-path is only valid with --insert before or --insert after");
    }
    if matches!(args.insert, MoveInsert::Before | MoveInsert::After) && args.anchor_path.is_none() {
        anyhow::bail!("--insert before/after requires --anchor-path");
    }
    Ok(())
}

fn print_plan(plan: &ExtractConstantPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("span\t{}..{}", plan.span_start, plan.span_end);
            println!("name\t{}", safe_text!(plan.name));
            println!("insert\t{}", plan.insert.label());
            if let Some(path) = &plan.anchor_path {
                println!("anchor_path\t{}", safe_text!(path));
            }
            if let Some(span) = plan.anchor_span {
                println!("anchor_span\t{}..{}", span.start().get(), span.end().get());
            }
            println!("replacement\t{}", safe_text!(plan.replacement));
            println!("definition\t{}", safe_text!(plan.definition));
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "dialect": plan.dialect.label(),
                "path": plan.path.to_string(),
                "span": { "start": plan.span_start, "end": plan.span_end },
                "name": plan.name.as_str(),
                "insert": plan.insert.label(),
                "anchor_path": plan.anchor_path.as_ref().map(ToString::to_string),
                "anchor_span": plan.anchor_span.map(|span| json!({
                    "start": span.start().get(), "end": span.end().get(),
                })),
                "replacement": &plan.replacement,
                "definition": &plan.definition,
                "changed": plan.changed,
                "written": written,
                "rewritten": &plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
