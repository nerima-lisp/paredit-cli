use std::path::Path;

use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::generate_defsystem::usecase::DefsystemPlan;

pub fn print_defsystem_plan(
    plan: &DefsystemPlan,
    skipped_dialect: &[std::path::PathBuf],
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("system\t{}", safe_text!(plan.system_name));
            println!("component_count\t{}", plan.components.len());
            println!("depends_on_count\t{}", plan.depends_on.len());
            println!("skipped_dialect_count\t{}", skipped_dialect.len());
            println!("written\t{written}");
            println!("generated\t{}", safe_text!(plan.generated.trim_end()));
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "generate defsystem",
                "system": plan.system_name,
                "components": plan.components,
                "depends_on": plan.depends_on,
                "written": written,
                "generated": plan.generated,
                "skipped_dialect": skipped_dialect
                    .iter()
                    .map(|file| path_text(file))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}
