use anyhow::Result;
use serde_json::json;

use crate::application::usecase::sort_definitions::SortDefinitionsPlan;

use super::super::super::OutputFormat;

pub(in crate::presentation::cli::definition_movement) fn print_sort_definitions_plan(
    plan: &SortDefinitionsPlan,
    output: OutputFormat,
) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("file\t{}", safe_text!(plan.file.display()));
            println!("dialect\t{}", plan.dialect.label());
            println!("strategy\t{}", plan.strategy.label());
            println!("definition_count\t{}", plan.items.len());
            for item in &plan.items {
                println!("old_path\t{}", safe_text!(item.old_path));
                println!("new_path\t{}", safe_text!(item.new_path));
                println!(
                    "span\t{}..{}",
                    item.span.start().get(),
                    item.span.end().get()
                );
                println!("head\t{}", safe_text!(item.head));
                if let Some(name) = &item.name {
                    println!("name\t{}", safe_text!(name));
                }
                println!("category\t{}", item.category.label());
            }
            println!("changed\t{}", plan.changed);
            println!("written\t{}", plan.written);
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "command": "sort-definitions",
                "file": plan.file,
                "dialect": plan.dialect.label(),
                "strategy": plan.strategy.label(),
                "definition_count": plan.items.len(),
                "definitions": plan.items.iter().map(|item| {
                    json!({
                        "old_path": item.old_path.to_string(),
                        "new_path": item.new_path.to_string(),
                        "span": {
                            "start": item.span.start().get(),
                            "end": item.span.end().get(),
                        },
                        "head": item.head,
                        "name": item.name,
                        "category": item.category.label(),
                        "source_index": item.source_index,
                        "target_index": item.target_index,
                    })
                }).collect::<Vec<_>>(),
                "rewritten": plan.rewritten,
                "changed": plan.changed,
                "written": plan.written,
            }))?
        ),
    }
    Ok(())
}
