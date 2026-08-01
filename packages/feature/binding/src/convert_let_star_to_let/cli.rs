use crate::convert_let_star_to_let::usecase::{
    ConvertLetStarToLetPlan, ConvertLetStarToLetRequest, plan_convert_let_star_to_let,
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

#[derive(Debug, Args)]
pub struct ConvertLetStarToLetArgs {
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
    /// Instead of refusing when a binding's initializer depends on an
    /// earlier binding, convert the longest independent prefix of bindings
    /// into an outer `let` and keep the rest as a nested `let*`.
    #[arg(long)]
    allow_partial: bool,
}

pub fn convert_let_star_to_let(args: ConvertLetStarToLetArgs) -> CliResult<()> {
    if args.write && args.file.is_none() {
        return Err(paredit_core_cli::ArgumentError::WriteRequiresFile.into());
    }
    let (input, dialect, _) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let plan = plan_convert_let_star_to_let(ConvertLetStarToLetRequest {
        input: &input.text,
        dialect,
        path: args.path,
        allow_partial: args.allow_partial,
    })?;
    let written = args.write && plan.changed;
    if written {
        let file = require_output_file(input.file.as_ref())?;
        write_file_with_rollback(file.clone(), plan.rewritten.clone())?;
    }
    print_plan(&plan, written, args.output)
}

fn print_plan(
    plan: &ConvertLetStarToLetPlan,
    written: bool,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("binding_count\t{}", plan.binding_names.len());
            println!("changed\t{}", plan.changed);
            println!("partial\t{}", plan.partial);
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
                "partial": plan.partial,
                "written": written,
                "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
