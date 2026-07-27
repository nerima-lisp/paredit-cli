use anyhow::Result;
use serde_json::json;

use super::super::super::OutputFormat;
use super::super::types::MoveFormPlan;

pub(in crate::presentation::cli::definition_movement) fn print_move_form_plan(
    plan: &MoveFormPlan,
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
            if let Some(head) = &plan.head {
                println!("head\t{}", safe_text!(head));
            }
            println!("insert\t{}", plan.insert.label());
            if let Some(anchor_path) = &plan.anchor_path {
                println!("anchor_path\t{}", safe_text!(anchor_path));
            }
            if let Some(anchor_span) = plan.anchor_span {
                println!(
                    "anchor_span\t{}..{}",
                    anchor_span.start().get(),
                    anchor_span.end().get()
                );
            }
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
                "head": plan.head,
                "text": plan.form_text,
                "insert": plan.insert.label(),
                "anchor_path": plan.anchor_path.as_ref().map(ToString::to_string),
                "anchor_span": plan.anchor_span.map(|span| {
                    json!({
                        "start": span.start().get(),
                        "end": span.end().get(),
                    })
                }),
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
