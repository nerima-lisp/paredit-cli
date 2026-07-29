use paredit_core_cli::CliResult;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::generate_defpackage::usecase::DefpackagePlan;

pub fn print_defpackage_plan(
    plan: &DefpackagePlan,
    written: bool,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("package\t{}", safe_text!(plan.package_name));
            println!("export_count\t{}", plan.exports.len());
            println!("use_count\t{}", plan.uses.len());
            println!("replaces_existing\t{}", plan.replaces.is_some());
            println!("written\t{written}");
            println!("generated\t{}", safe_text!(plan.generated.trim_end()));
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "report": "generate defpackage",
                "package": plan.package_name,
                "exports": plan.exports,
                "uses": plan.uses,
                "replaces_existing": plan.replaces.is_some(),
                "written": written,
                "generated": plan.generated,
            }))?
        ),
    }

    Ok(())
}
