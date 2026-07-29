use anyhow::Result;
use serde_json::json;

use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;

use crate::catalog::CatalogEntry;
use crate::run::usecase::{FileOutcome, MigrationTotals};

pub fn print_list(entries: &[CatalogEntry], output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!("recipes\t{}", entries.len());
            for entry in entries {
                println!(
                    "recipe\t{}\t{}\t{}\t{}\t{}",
                    safe_text!(entry.migration.name),
                    entry.migration.steps.len(),
                    entry.migration.dialect_labels().join(","),
                    safe_text!(entry.origin),
                    safe_text!(entry.migration.description)
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "recipeCount": entries.len(),
                "recipes": entries.iter().map(recipe_json).collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

pub fn print_explain(entry: &CatalogEntry, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Text => {
            println!(
                "recipe\t{}\t{}\t{}",
                safe_text!(entry.migration.name),
                entry.migration.dialect_labels().join(","),
                safe_text!(entry.origin)
            );
            println!("description\t{}", safe_text!(entry.migration.description));
            for (index, step) in entry.migration.steps.iter().enumerate() {
                println!(
                    "step\t{index}\t{}\t{}\t{}",
                    safe_text!(step.query),
                    safe_text!(step.rewrite),
                    step.note
                        .as_deref()
                        .map_or_else(|| "-".to_owned(), |note| safe_text!(note).to_string())
                );
            }
        }
        OutputFormat::Json => {
            let mut payload = recipe_json(entry);
            payload["schema_version"] = json!(1);
            payload["steps"] = json!(
                entry
                    .migration
                    .steps
                    .iter()
                    .enumerate()
                    .map(|(index, step)| json!({
                        "index": index,
                        "query": step.query,
                        "rewrite": step.rewrite,
                        "note": step.note,
                    }))
                    .collect::<Vec<_>>()
            );
            println!("{}", serde_json::to_string_pretty(&payload)?);
        }
    }
    Ok(())
}

pub fn print_run_report(
    entry: &CatalogEntry,
    files: &[FileOutcome],
    totals: &MigrationTotals,
    written: bool,
    output: OutputFormat,
) -> Result<()> {
    let touched: Vec<&FileOutcome> = files.iter().filter(|file| file.is_touched()).collect();

    match output {
        OutputFormat::Text => {
            println!(
                "recipe\t{}\tfiles_scanned\t{}\tout_of_scope\t{}\tfiles_touched\t{}\treplacements\t{}\tskipped\t{}",
                safe_text!(entry.migration.name),
                totals.files_scanned,
                totals.files_out_of_scope,
                totals.files_touched,
                totals.replacements,
                totals.skipped
            );
            println!("written\t{written}");
            for file in touched {
                println!(
                    "file\t{}\t{}\t{}",
                    safe_text!(file.path.display()),
                    file.replacements(),
                    file.skipped()
                );
                for (index, step) in file.steps.iter().enumerate() {
                    if step.replacements == 0 && step.skipped == 0 {
                        continue;
                    }
                    println!(
                        "\tstep\t{index}\t{}\t{}\t{}",
                        step.replacements,
                        step.skipped,
                        safe_text!(step.query)
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "recipe": entry.migration.name,
                "origin": entry.origin,
                "written": written,
                "summary": {
                    "filesScanned": totals.files_scanned,
                    "filesOutOfScope": totals.files_out_of_scope,
                    "filesTouched": totals.files_touched,
                    "replacements": totals.replacements,
                    "skipped": totals.skipped,
                },
                "files": touched
                    .iter()
                    .map(|file| json!({
                        "path": file.path.display().to_string(),
                        "dialect": file.dialect.label(),
                        "replacements": file.replacements(),
                        "skipped": file.skipped(),
                        "steps": file
                            .steps
                            .iter()
                            .enumerate()
                            .filter(|(_, step)| step.replacements > 0 || step.skipped > 0)
                            .map(|(index, step)| json!({
                                "index": index,
                                "query": step.query,
                                "rewrite": step.rewrite,
                                "replacements": step.replacements,
                                "skipped": step.skipped,
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }
    Ok(())
}

fn recipe_json(entry: &CatalogEntry) -> serde_json::Value {
    json!({
        "name": entry.migration.name,
        "description": entry.migration.description,
        "dialects": entry.migration.dialect_labels(),
        "stepCount": entry.migration.steps.len(),
        "origin": entry.origin,
    })
}
