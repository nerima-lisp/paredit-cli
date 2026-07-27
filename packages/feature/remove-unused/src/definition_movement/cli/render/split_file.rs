use anyhow::Result;
use serde_json::json;

use crate::application::usecase::split_file::SplitFilePlan;

use super::super::super::OutputFormat;

pub(in crate::presentation::cli::definition_movement) fn print_split_file_plan(
    plan: &SplitFilePlan,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("from_file\t{}", safe_text!(plan.from_file.display()));
            println!("to_file\t{}", safe_text!(plan.to_file.display()));
            println!("from_dialect\t{}", plan.from_dialect.label());
            println!("to_dialect\t{}", plan.to_dialect.label());
            println!("definition_count\t{}", plan.items.len());
            for item in &plan.items {
                println!("path\t{}", safe_text!(item.path));
                println!(
                    "span\t{}..{}",
                    item.span.start().get(),
                    item.span.end().get()
                );
                println!("head\t{}", safe_text!(item.definition.head));
                if let Some(name) = &item.definition.name {
                    println!("name\t{}", safe_text!(name));
                }
                println!("category\t{}", item.definition.category.label());
            }
            println!("to_file_existed\t{}", plan.to_file_existed);
            println!("to_parent_existed\t{}", plan.to_parent_existed);
            println!("changed\t{}", plan.changed);
            println!("written\t{}", plan.written);
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "command": "split-file",
                "from_file": plan.from_file,
                "to_file": plan.to_file,
                "from_dialect": plan.from_dialect.label(),
                "to_dialect": plan.to_dialect.label(),
                "definition_count": plan.items.len(),
                "definitions": plan.items.iter().map(|item| {
                    json!({
                        "path": item.path.to_string(),
                        "span": {
                            "start": item.span.start().get(),
                            "end": item.span.end().get(),
                        },
                        "removal_span": {
                            "start": item.removal_span.start().get(),
                            "end": item.removal_span.end().get(),
                        },
                        "head": item.definition.head,
                        "name": item.definition.name,
                        "category": item.definition.category.label(),
                        "parameter_count": item.definition.parameter_count,
                        "body_form_count": item.definition.body_form_count,
                        "package": item.definition.package,
                        "text": item.definition_text,
                    })
                }).collect::<Vec<_>>(),
                "from_rewritten": plan.from_rewritten,
                "to_rewritten": plan.to_rewritten,
                "to_file_existed": plan.to_file_existed,
                "to_parent_existed": plan.to_parent_existed,
                "changed": plan.changed,
                "written": plan.written,
            }))?
        ),
    }
    Ok(())
}
