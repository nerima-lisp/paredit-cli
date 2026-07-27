use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use super::super::types::MoveDefinitionPlan;

pub(in crate::presentation::cli::definition_movement) fn print_move_definition_plan(
    plan: &MoveDefinitionPlan,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("from_file\t{}", safe_text!(plan.from_file.display()));
            println!("to_file\t{}", safe_text!(plan.to_file.display()));
            println!("from_dialect\t{}", plan.from_dialect.label());
            println!("to_dialect\t{}", plan.to_dialect.label());
            println!("path\t{}", safe_text!(plan.path));
            println!(
                "span\t{}..{}",
                plan.span.start().get(),
                plan.span.end().get()
            );
            println!("head\t{}", safe_text!(plan.definition.head));
            if let Some(name) = &plan.definition.name {
                println!("name\t{}", safe_text!(name));
            }
            println!("category\t{}", plan.definition.category.label());
            println!("to_file_existed\t{}", plan.to_file_existed);
            println!("changed\t{}", plan.changed);
            println!("written\t{}", plan.written);
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "from_file": plan.from_file,
                "to_file": plan.to_file,
                "from_dialect": plan.from_dialect.label(),
                "to_dialect": plan.to_dialect.label(),
                "path": plan.path.to_string(),
                "span": {
                    "start": plan.span.start().get(),
                    "end": plan.span.end().get(),
                },
                "definition": {
                    "path": plan.definition.path,
                    "span": {
                        "start": plan.definition.span.start().get(),
                        "end": plan.definition.span.end().get(),
                    },
                    "head": plan.definition.head,
                    "name": plan.definition.name,
                    "category": plan.definition.category.label(),
                    "parameter_count": plan.definition.parameter_count,
                    "body_form_count": plan.definition.body_form_count,
                    "package": plan.definition.package,
                    "text": plan.definition_text,
                },
                "from_rewritten": plan.from_rewritten,
                "to_rewritten": plan.to_rewritten,
                "to_file_existed": plan.to_file_existed,
                "changed": plan.changed,
                "written": plan.written,
            }))?
        ),
    }
    Ok(())
}
