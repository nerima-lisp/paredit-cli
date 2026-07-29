use paredit_core_cli::CliResult;
use paredit_core_cli::args::OutputFormat;
use paredit_core_cli::safe_text;
use serde_json::json;

use super::types::RemoveDefinitionPlan;
use crate::remove_unused_definition::usecase::RemoveUnusedDefinitionsPlan;

pub fn print_remove_definition_plan(
    plan: &RemoveDefinitionPlan,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("file\t{}", safe_text!(plan.file.display()));
            println!("dialect\t{}", plan.dialect.label());
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
            println!("changed\t{}", plan.changed);
            println!("written\t{}", plan.written);
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "file": plan.file,
                "dialect": plan.dialect.label(),
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
                "rewritten": plan.rewritten,
                "changed": plan.changed,
                "written": plan.written,
            }))?
        ),
    }
    Ok(())
}

pub fn print_remove_unused_definitions_plan(
    plan: &RemoveUnusedDefinitionsPlan,
    written: bool,
    output: OutputFormat,
) -> CliResult<()> {
    match output {
        OutputFormat::Text => {
            println!("files\t{}", plan.files.len());
            println!("candidate_count\t{}", plan.candidate_count);
            println!("removal_count\t{}", plan.removal_count);
            println!("skipped_count\t{}", plan.skipped_count);
            println!("changed\t{}", plan.changed);
            println!("written\t{written}");
            for file in &plan.files {
                println!(
                    "{}\t{}\tremovals={}\tskipped={}\tchanged={}\tpackage={}",
                    safe_text!(file.path.display()),
                    file.dialect.label(),
                    file.removals.len(),
                    file.skipped.len(),
                    file.changed,
                    safe_text!(file.package.as_deref().unwrap_or(""))
                );
                for removal in &file.removals {
                    println!(
                        "\tremove\t{}\t{}\t{}\t{}..{}",
                        removal.definition.category.label(),
                        safe_text!(removal.definition.head),
                        safe_text!(removal.definition.name.as_deref().unwrap_or("")),
                        removal.definition.span.start().get(),
                        removal.definition.span.end().get()
                    );
                }
                for skipped in &file.skipped {
                    println!(
                        "\tskip\t{}\t{}\t{}\t{}",
                        skipped.definition.category.label(),
                        safe_text!(skipped.definition.head),
                        safe_text!(skipped.definition.name.as_deref().unwrap_or("")),
                        skipped.reason.label()
                    );
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "file_count": plan.files.len(),
                "candidate_count": plan.candidate_count,
                "removal_count": plan.removal_count,
                "skipped_count": plan.skipped_count,
                "changed": plan.changed,
                "written": written,
                "files": plan
                    .files
                    .iter()
                    .map(|file| json!({
                        "path": file.path.display().to_string(),
                        "dialect": file.dialect.label(),
                        "package": file.package.as_deref(),
                        "changed": file.changed,
                        "rewritten": file.rewritten.as_str(),
                        "removals": file
                            .removals
                            .iter()
                            .map(|removal| json!({
                                "path": removal.definition.path.as_str(),
                                "span": {
                                    "start": removal.definition.span.start().get(),
                                    "end": removal.definition.span.end().get(),
                                },
                                "removal_span": {
                                    "start": removal.removal_span.start().get(),
                                    "end": removal.removal_span.end().get(),
                                },
                                "head": removal.definition.head.as_str(),
                                "name": removal.definition.name.as_deref(),
                                "category": removal.definition.category.label(),
                                "parameter_count": removal.definition.parameter_count,
                                "body_form_count": removal.definition.body_form_count,
                                "package": removal.definition.package.as_deref(),
                                "text": removal.definition_text.as_str(),
                            }))
                            .collect::<Vec<_>>(),
                        "skipped": file
                            .skipped
                            .iter()
                            .map(|skipped| json!({
                                "path": skipped.definition.path.as_str(),
                                "span": {
                                    "start": skipped.definition.span.start().get(),
                                    "end": skipped.definition.span.end().get(),
                                },
                                "head": skipped.definition.head.as_str(),
                                "name": skipped.definition.name.as_deref(),
                                "category": skipped.definition.category.label(),
                                "package": skipped.definition.package.as_deref(),
                                "reason": skipped.reason.label(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        ),
    }

    Ok(())
}
