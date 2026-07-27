use crate::convert_flet_to_labels::usecase::{
    ConvertFletToLabelsPlan, ConvertFletToLabelsRequest, plan_convert_flet_to_labels,
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
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ConvertFletToLabelsArgs {
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

pub fn convert_flet_to_labels(args: ConvertFletToLabelsArgs) -> Result<()> {
    if args.write && args.file.is_none() {
        anyhow::bail!("--write requires --file");
    }
    let (input, dialect, _) = read_input_dialect_and_tree(args.file.clone(), args.dialect)?;
    let plan = plan_convert_flet_to_labels(ConvertFletToLabelsRequest {
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

fn print_plan(plan: &ConvertFletToLabelsPlan, written: bool, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("dialect\t{}", plan.dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!("binding_count\t{}", plan.binding_count);
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
                "binding_count": plan.binding_count,
                "changed": plan.changed,
                "written": written,
                "rewritten": plan.rewritten,
            }))?
        ),
    }
    Ok(())
}
